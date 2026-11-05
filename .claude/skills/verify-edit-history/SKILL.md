---
name: verify-edit-history
description: Bounded exhaustive check that Emulsion's Develop undo/redo history stays a linear list (Rust, app/src-tauri/src/catalog/edit_stack.rs). Use after any change to record_edit_stack, write_edit_stack, restore_history_entry, apply_ops_to_versions, restore_snapshot, or the edit_history table.
---

# Verify the Develop edit history

## The property, in plain words

**Promise:** the History panel is a linear undo/redo list. If you jump back to an entry and then make a new edit, everything after the entry you jumped to is discarded, and the list becomes "entries up to there, plus the new edit".

**Harm if broken:** the list keeps entries you thought you had thrown away, so Undo from the new edit lands on one of them instead of the entry you came from. Edits are never lost; the history is wrong.

## Tool, and why

Exhaustive enumeration of operation sequences against the **real** `Catalog` (in-memory SQLite), compared after every call with a 15-line reference model that keeps an explicit cursor. No TLA+/Kani/etc.: the logic is SQL inside one Rust function, no verifier runs on rusqlite-backed code, and the repo's own toolchain (`cargo`) is all that is needed. Nothing to install.

The code under check keeps **no stored cursor**. `write_edit_stack` re-derives it by finding the *newest* history row whose JSON equals the live stack. That is correct only when no two rows hold equal content, which is exactly what the two checks separate.

## Run it

```bash
.claude/skills/verify-edit-history/run.sh
```

About 20 s (first build adds ~15 s). Exit 0 means both checks pass. The checks live in `app/src-tauri/src/catalog/edit_history_check.rs` (`#[cfg(test)]`, `#[ignore]`d so `make test` is untouched).

| Check | Alphabet | Bound | Result at sync commit |
|---|---|---|---|
| `history_is_linear_when_every_edit_is_new_content` | each `record_edit_stack` writes content never seen before; `restore_history_entry(k)` for any existing row `k` | every sequence of up to 8 calls (14,721 sequences) | **passes** |
| `history_is_linear_when_edits_may_revisit_earlier_content` | `record_edit_stack` may write any of 3 contents, including ones an earlier row already holds | every sequence of up to 8 calls; stops at first (shortest) counterexample | **FAILS: open finding** |

## Finding (current code, not fixed; product code was not changed)

Shortest counterexample, in the code's words (content `0` = the photo's starting stack, `1` = e.g. Exposure +1):

1. Edit to content 1 (history: `Original(0)`, `1`).
2. Edit back to content 0, for example drag Exposure back to 0 (history: `Original(0)`, `1`, `0`).
3. Click the first History row, `Original`. Live stack is now content 0.
4. Make a new edit to content 1.

Expected history: `Original(0)`, `1`. Actual history: `Original(0)`, `1`, `0`, `1`. `write_edit_stack` looked up "where am I" by content, matched the *newest* row holding content 0 (step 2's row, not the `Original` row the user clicked), and so truncated nothing. The abandoned `1`, `0` entries stay in the list.

Why it is a finding and not a model quirk: `restore_history_entry`'s doc says a later edit "still truncates/re-anchors correctly from here regardless", and `a_new_edit_after_undo_truncates_the_abandoned_redo_branch` (edit_stack.rs) states the intent. That test only uses distinct contents, which is why it passes.

Not verified: that the Svelte UI can reach step 3 then 4 with exactly this content (the check drives the Rust API the Tauri commands call). A UI proof belongs in `verify-emulsion` (Develop > History).

## Able to fail (shown on broken variants of `write_edit_stack`, then reverted)

- Delete the `DELETE FROM edit_history ... id > cursor` statement: **both** checks fail (`[Record(1), Restore(0), Record(3)]` leaves `[0,1,3]`).
- Change the cursor lookup `ORDER BY id DESC LIMIT 1` to `ASC`: the revisiting check fails the other way (`[Record(1), Record(0), Record(1)]` leaves `[0,1]`: the cursor matched the oldest row, so the just-made edit was truncated away); the fresh-content check still passes.

## Scope: what a pass covers and does not

Covered (model vs. code): `Catalog::record_edit_stack`, `Catalog::write_edit_stack` (cursor derivation, redo-branch truncation, `Original` row insertion), `Catalog::restore_history_entry`, `Catalog::get_history`, `Catalog::peek_history_entry`. `apply_ops_to_versions` and `restore_snapshot` call `write_edit_stack` and inherit its behaviour but are **not** driven by the checks.

Bounds and assumptions: sequences of at most 8 calls; at most 3 distinct stack contents in the revisiting check; one version; labels all `Some`. Not covered: the 100-row cap (`MAX_HISTORY_ENTRIES`, tested by `history_is_pruned_to_the_cap_oldest_by_id_first`), `record_edit_stack` with `label: None`, multiple versions, concurrent callers (the app serializes every call through one `Mutex<Catalog>`), crash mid-transaction, and the frontend (`app/src/lib/state/develop.svelte.js` debounce and `flushEditStack`).

## Telling a real bug from a modeling error

On a failure the output prints the shortest sequence and, after the failing call, the contents the history holds versus what a linear history would hold. Then read the model in `edit_history_check.rs` (`Model::apply`):

- If the model's expectation is what the History panel promises (see "The property"), the code is wrong: report it with the sequence, and do not weaken the alphabet or the model.
- If a deliberate product change altered the promise (for example redo rows are now kept), change `Model::apply` in the same commit and say so in the commit message.

## Keeping it in step

Coverage record (files and functions above) was last confirmed against the code at:

**Sync commit: `63fed6d`** (`app/src-tauri/src/catalog/edit_stack.rs` unchanged since; the check files themselves were added on top of it, uncommitted at generation time).

Before a change, run `git diff 63fed6d -- app/src-tauri/src/catalog/edit_stack.rs app/src-tauri/src/lib.rs`; if `record_edit_stack`, `write_edit_stack`, `restore_history_entry` or the `edit_history` schema in `schema.rs` changed, run the check and re-read the model. Move the sync commit forward only after you confirm that `Model::apply` and the scope list still describe the code at the new commit.

If this check's results are ever replaced by a TLA+ model, read them with the pstack `/statechart` skill.
