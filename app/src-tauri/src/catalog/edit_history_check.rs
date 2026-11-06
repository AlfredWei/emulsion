//! Bounded exhaustive check of the Develop undo/redo history (formal-verification skill:
//! `.claude/skills/verify-edit-history/`).
//!
//! PROMISE UNDER CHECK. The History panel is a LINEAR undo/redo list. Jumping to a past entry and
//! then making a new edit must discard everything after the entry you jumped to, and the list
//! afterwards must be exactly "the entries up to where you were, plus the new edit".
//!
//! HOW. The real `Catalog` (in-memory SQLite) is driven through EVERY sequence of
//! `record_edit_stack` / `restore_history_entry` calls up to `MAX_DEPTH` operations over a small
//! alphabet, and compared after every call with a 15-line reference model that keeps an explicit
//! cursor. The real code has no stored cursor (see `restore_history_entry`'s doc): it re-derives the
//! cursor in `write_edit_stack` by finding the NEWEST history row whose JSON equals the live stack.
//!
//! Test-only module; compiled out of the app. Both checks are `#[ignore]`d so `make test` stays fast
//! and green; run them with `app/../.claude/skills/verify-edit-history/run.sh`.

use super::test_support::stack_with;
use super::*;

/// One call into the catalog. `Record(i)` writes stack `i` of the alphabet with a label;
/// `Restore(k)` jumps to the k-th history row (0 = oldest).
#[derive(Clone, Copy, Debug)]
enum Op {
    Record(usize),
    Restore(usize),
}

/// Reference semantics: a Vec of stacks plus an explicit cursor.
#[derive(Clone, Debug)]
struct Model {
    live: usize,
    hist: Vec<usize>,
    cursor: usize, // meaningful only when !hist.is_empty()
}

impl Model {
    fn new() -> Self {
        Self { live: 0, hist: vec![], cursor: 0 }
    }
    fn apply(&mut self, op: Op) {
        match op {
            Op::Record(s) => {
                if s != self.live {
                    if self.hist.is_empty() {
                        self.hist.push(self.live); // the "Original" row
                        self.cursor = 0;
                    }
                    self.hist.truncate(self.cursor + 1); // drop the abandoned redo branch
                    self.hist.push(s);
                    self.cursor = self.hist.len() - 1;
                }
                self.live = s;
            }
            Op::Restore(k) => {
                self.live = self.hist[k];
                self.cursor = k;
            }
        }
    }
}

/// Alphabet of stack contents. Content 0 is the version's starting stack (`EditStack::empty()`).
fn stack_of(content: usize) -> EditStack {
    if content == 0 {
        EditStack::empty()
    } else {
        stack_with("exposure", content as f64)
    }
}

/// Replays `ops` on a fresh catalog and the model; `Some(description)` on the first divergence.
fn first_divergence(ops: &[Op]) -> Option<String> {
    let catalog = Catalog::open_in_memory().unwrap();
    let image = catalog.add_image("/a.CR3").unwrap();
    let version = catalog.add_edit_stack(image, &EditStack::empty()).unwrap();
    let mut model = Model::new();
    for (step, &op) in ops.iter().enumerate() {
        let history = match op {
            Op::Record(s) => catalog.record_edit_stack(version, &stack_of(s), Some("edit")).unwrap(),
            Op::Restore(k) => {
                let rows = catalog.get_history(version).unwrap();
                catalog.restore_history_entry(version, rows[k].id).unwrap();
                catalog.get_history(version).unwrap()
            }
        };
        model.apply(op);
        let live = catalog.get_edit_stack(version).unwrap();
        let rows: Vec<EditStack> = history.iter().map(|h| catalog.peek_history_entry(version, h.id).unwrap()).collect();
        let want_rows: Vec<EditStack> = model.hist.iter().map(|&c| stack_of(c)).collect();
        if live != stack_of(model.live) || rows != want_rows {
            return Some(format!(
                "after step {} ({:?}): history holds contents {:?} (live {:?}); a linear history would hold {:?} (live {})",
                step + 1,
                op,
                contents(&rows),
                contents(std::slice::from_ref(&live)),
                model.hist,
                model.live
            ));
        }
    }
    None
}

/// Maps stacks back to their alphabet index, for readable counterexamples.
fn contents(stacks: &[EditStack]) -> Vec<usize> {
    stacks
        .iter()
        .map(|s| s.ops.first().and_then(|o| o["value"].as_f64()).map(|v| v as usize).unwrap_or(0))
        .collect()
}

/// Depth-first enumeration of every operation sequence of exactly `depth` calls. `record_alphabet`
/// lists, per step index, which stack contents `Record` may write.
fn enumerate(
    depth: usize,
    record_alphabet: &dyn Fn(usize) -> Vec<usize>,
    prefix: &mut Vec<Op>,
    model: &Model,
    visited: &mut u64,
) -> Option<(Vec<Op>, String)> {
    if prefix.len() == depth {
        *visited += 1;
        return first_divergence(prefix).map(|why| (prefix.clone(), why));
    }
    let step = prefix.len();
    let mut choices: Vec<Op> = record_alphabet(step).into_iter().map(Op::Record).collect();
    choices.extend((0..model.hist.len()).map(Op::Restore));
    for op in choices {
        let mut next = model.clone();
        next.apply(op);
        prefix.push(op);
        let hit = enumerate(depth, record_alphabet, prefix, &next, visited);
        prefix.pop();
        if hit.is_some() {
            return hit;
        }
    }
    None
}

/// Iterative deepening, so the first counterexample found is a shortest one.
fn check_up_to(max_depth: usize, record_alphabet: &dyn Fn(usize) -> Vec<usize>) -> (u64, Option<(Vec<Op>, String)>) {
    let mut visited = 0;
    for depth in 1..=max_depth {
        if let Some(hit) = enumerate(depth, record_alphabet, &mut Vec::new(), &Model::new(), &mut visited) {
            return (visited, Some(hit));
        }
    }
    (visited, None)
}

const MAX_DEPTH: usize = 8;
/// The 3-content alphabet branches far more than the fresh-content one (~8 choices per step), so its bound is lower.
const REVISIT_DEPTH: usize = 7;

/// Every edit writes content never seen before in this history. Expected to PASS: the cursor lookup
/// by JSON is unambiguous when no two rows hold equal content.
#[test]
#[ignore = "run via .claude/skills/verify-edit-history/run.sh"]
fn history_is_linear_when_every_edit_is_new_content() {
    // Step i writes content i+1, which no earlier step and not the starting content (0) used.
    let (visited, hit) = check_up_to(MAX_DEPTH, &|step| vec![step + 1]);
    eprintln!("fresh-content alphabet: {visited} sequences of up to {MAX_DEPTH} calls checked");
    if let Some((ops, why)) = hit {
        panic!("history is not linear.\n  sequence: {ops:?}\n  {why}");
    }
}

/// Edits may return to content an earlier row already holds (slider moved and put back; undo then
/// edit back). Failed while the cursor was re-derived from content (see the skill's Findings section);
/// passes now that it is stored.
fn revisiting_content_is_linear(max_depth: usize) {
    let (visited, hit) = check_up_to(max_depth, &|_| vec![0, 1, 2]);
    eprintln!("revisiting alphabet: {visited} sequences of up to {max_depth} calls checked");
    if let Some((ops, why)) = hit {
        panic!("history is not linear.\n  sequence: {ops:?}\n  {why}");
    }
}

/// Depth 5 runs in `make test` (the old counterexample needs 4 calls); `REVISIT_DEPTH` is the exhaustive run below.
#[test]
fn history_is_linear_when_edits_may_revisit_earlier_content_shallow() {
    revisiting_content_is_linear(5);
}

#[test]
#[ignore = "run via .claude/skills/verify-edit-history/run.sh"]
fn history_is_linear_when_edits_may_revisit_earlier_content() {
    revisiting_content_is_linear(REVISIT_DEPTH);
}
