use super::*;

/// A non-destructive edit stack: an ordered list of typed operations,
/// versioned so future op types (masks in M2, AI masks in M5, ...) can be
/// added without breaking catalogs written by older code (ADR-0006).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EditStack {
    pub schema_version: u32,
    pub ops: Vec<serde_json::Value>,
}

impl EditStack {
    pub fn empty() -> Self {
        Self {
            schema_version: 1,
            ops: Vec::new(),
        }
    }
}

/// What `Catalog::apply_ops_to_versions` did: the versions whose stack changed (their thumbnails are
/// stale) and the ones that no longer exist.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ApplyOpsOutcome {
    pub changed: Vec<i64>,
    pub missing: Vec<i64>,
}

fn op_name(op: &serde_json::Value) -> Option<&str> {
    op.get("op").and_then(|v| v.as_str())
}

/// `target` without the `removed_ops`, with each of `ops` replacing the same-named op wholesale -- in
/// place when the target has it (so an unchanged op leaves the stack byte-identical), appended when not.
/// Name-only, like the frontend's `applyPresetOps`.
fn merge_ops(target: &EditStack, ops: &[serde_json::Value], removed_ops: &[String]) -> EditStack {
    let mut merged: Vec<serde_json::Value> = target
        .ops
        .iter()
        .filter(|existing| !removed_ops.iter().any(|r| Some(r.as_str()) == op_name(existing)))
        .cloned()
        .collect();
    for incoming in ops {
        let slot = op_name(incoming).and_then(|name| merged.iter().position(|e| op_name(e) == Some(name)));
        match slot {
            Some(i) => merged[i] = incoming.clone(),
            None => merged.push(incoming.clone()),
        }
    }
    EditStack { schema_version: target.schema_version, ops: merged }
}

/// Per-version cap on stored `edit_history` rows (M3) -- oldest-by-id
/// pruned first once exceeded. See `record_edit_stack`.
const MAX_HISTORY_ENTRIES: i64 = 100;

/// The label of the row `write_edit_stack` records for the stack a version had BEFORE its first history
/// row, so the first edit can be undone (Lightroom's "Import" row; this project's name for it).
pub const ORIGINAL_LABEL: &str = "Original";

/// One entry in a version's linear undo/redo history (M3). Deliberately
/// does NOT include `edit_stack_json` -- the History panel only ever needs
/// to LIST entries (id/label/timestamp); the full stack is fetched only
/// when actually restoring a specific one (`restore_history_entry`), kept
/// as a separate, smaller round trip.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: i64,
    pub label: String,
    pub created_at: String,
}

/// A named, user-created save point (M3) -- same shape as `HistoryEntry`
/// for the same reason (list without the payload; `restore_snapshot`
/// fetches the full stack).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SnapshotEntry {
    pub id: i64,
    pub name: String,
    pub created_at: String,
}

impl Catalog {
    /// Create an edit-stack record for an image. Lower-level building
    /// block, same status as `add_image_with_metadata` above -- superseded
    /// as the import pipeline's caller by `add_image_with_edit_stack`,
    /// kept as real pub API for tests and a future M2 virtual-copy path.
    #[allow(dead_code)]
    pub fn add_edit_stack(&self, image_id: i64, stack: &EditStack) -> Result<i64> {
        let json = serde_json::to_string(stack).expect("EditStack is always serializable");
        self.conn.execute(
            "INSERT INTO image_versions (image_id, edit_stack_json) VALUES (?1, ?2)",
            params![image_id, json],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn get_edit_stack(&self, version_id: i64) -> Result<EditStack> {
        let json: String = self.conn.query_row(
            "SELECT edit_stack_json FROM image_versions WHERE id = ?1",
            params![version_id],
            |row| row.get(0),
        )?;
        Ok(serde_json::from_str(&json).expect("stored edit stacks are always valid JSON"))
    }

    /// The real write path every Develop edit goes through as of M3
    /// History/Undo/Snapshots. One transaction does, in order:
    ///
    /// 1. Reads the version's CURRENT `edit_stack_json` (before the
    ///    write).
    /// 2. If `label` is given AND the new stack differs from the current
    ///    one: finds the most recent `edit_history` row (by id DESC)
    ///    whose OWN `edit_stack_json` matches the CURRENT (pre-write)
    ///    stack -- that row is "where the undo/redo cursor currently
    ///    sits". This project deliberately does NOT persist a separate
    ///    position column (see the module-level design note this
    ///    function's own PR description covers): the cursor is always
    ///    re-derivable from content alone, since every write that ever
    ///    changes the live stack either (a) came from this same function,
    ///    which always records a matching history row, or (b) came from
    ///    `restore_history_entry`/`restore_snapshot`, which set the live
    ///    stack to content that ALREADY has a matching row. If no match
    ///    is found (the very first edit ever for this version), the
    ///    cursor is treated as "before all history" -- nothing to delete,
    ///    and the pre-write stack is recorded first as an `ORIGINAL_LABEL`
    ///    row (the thing the first edit's undo goes back to).
    ///    Every history row NEWER than the cursor is then deleted --
    ///    discards the abandoned redo branch, the same "a new edit after
    ///    undo cuts off redo" behavior every text editor and real
    ///    Lightroom has.
    /// 3. Inserts the new history row, then prunes back down to
    ///    `MAX_HISTORY_ENTRIES` (oldest-by-id first) if the cap was
    ///    exceeded -- `id`, not `created_at` (only second-resolution
    ///    here), is what orders/prunes, so a burst of edits landing in
    ///    the same second can't tie.
    /// 4. Writes the new stack to `image_versions` unconditionally (same
    ///    idempotent-rewrite behavior `update_edit_stack` always had --
    ///    `flushEditStack` on the JS side fires from many call sites,
    ///    most with nothing new pending, and this must stay a harmless
    ///    no-op-content re-write in that case, not an error).
    ///
    /// `label` is `None` in exactly the case where nothing was actually
    /// pending when a flush fired (switching images, exporting, closing
    /// the window) -- in practice this coincides with "new stack equals
    /// current stack" (step 2's own diff check), so no history row is
    /// skipped that should have existed. A content change arriving with
    /// `label: None` from some future, not-yet-existing caller would
    /// still persist correctly, just without a history entry -- this
    /// function never silently drops an edit for lack of a label.
    ///
    /// Returns the version's full, now-current history list (newest
    /// last), so the caller can refresh its own History panel in the
    /// SAME round trip.
    pub fn record_edit_stack(
        &self,
        version_id: i64,
        stack: &EditStack,
        label: Option<&str>,
    ) -> Result<Vec<HistoryEntry>> {
        let tx = self.conn.unchecked_transaction()?;
        Self::write_edit_stack(&tx, version_id, stack, label)?;
        tx.commit()?;
        self.get_history(version_id)
    }

    /// The body of `record_edit_stack`, inside a transaction the caller owns, so a batch write
    /// (`apply_ops_to_versions`) can run many of these in one transaction.
    fn write_edit_stack(
        tx: &rusqlite::Transaction<'_>,
        version_id: i64,
        stack: &EditStack,
        label: Option<&str>,
    ) -> Result<()> {
        let new_json = serde_json::to_string(stack).expect("EditStack is always serializable");

        if let Some(label) = label {
            let current_json: String = tx.query_row(
                "SELECT edit_stack_json FROM image_versions WHERE id = ?1",
                params![version_id],
                |row| row.get(0),
            )?;
            if new_json != current_json {
                let cursor_id: Option<i64> = tx
                    .query_row(
                        "SELECT id FROM edit_history WHERE version_id = ?1 AND edit_stack_json = ?2 ORDER BY id DESC LIMIT 1",
                        params![version_id, current_json],
                        |row| row.get(0),
                    )
                    .optional()?;
                tx.execute(
                    "DELETE FROM edit_history WHERE version_id = ?1 AND id > ?2",
                    params![version_id, cursor_id.unwrap_or(0)],
                )?;
                if cursor_id.is_none() {
                    // The live stack matches no history row (the very first edit ever, or a stack written
                    // outside this path): record it first as the row to undo back to, so even the first
                    // edit has a "before" (`ORIGINAL_LABEL`).
                    tx.execute(
                        "INSERT INTO edit_history (version_id, edit_stack_json, label) VALUES (?1, ?2, ?3)",
                        params![version_id, current_json, ORIGINAL_LABEL],
                    )?;
                }
                tx.execute(
                    "INSERT INTO edit_history (version_id, edit_stack_json, label) VALUES (?1, ?2, ?3)",
                    params![version_id, new_json, label],
                )?;
                tx.execute(
                    "DELETE FROM edit_history WHERE version_id = ?1 AND id NOT IN (
                        SELECT id FROM edit_history WHERE version_id = ?1 ORDER BY id DESC LIMIT ?2
                    )",
                    params![version_id, MAX_HISTORY_ENTRIES],
                )?;
            }
        }

        tx.execute(
            "UPDATE image_versions SET edit_stack_json = ?2, updated_at = datetime('now') WHERE id = ?1",
            params![version_id, new_json],
        )?;
        Ok(())
    }

    /// Auto Sync (RFC-0028 §3.5): writes the same op values into several versions in ONE transaction. For
    /// each target the stored stack is read, every op named in `removed_ops` is dropped, each op in `ops`
    /// replaces the target's op of the same name wholesale (appended when absent), and the result goes
    /// through the same write as `record_edit_stack` (one history row labelled `label`, and none when the
    /// stack did not change). Everything else in a target's stack is left alone. Targets that no longer
    /// exist are skipped and returned in `missing`; any other failure rolls the whole batch back.
    pub fn apply_ops_to_versions(
        &self,
        version_ids: &[i64],
        ops: &[serde_json::Value],
        removed_ops: &[String],
        label: &str,
    ) -> Result<ApplyOpsOutcome> {
        let tx = self.conn.unchecked_transaction()?;
        let mut outcome = ApplyOpsOutcome::default();
        for &version_id in version_ids {
            let current_json: Option<String> = tx
                .query_row(
                    "SELECT edit_stack_json FROM image_versions WHERE id = ?1",
                    params![version_id],
                    |row| row.get(0),
                )
                .optional()?;
            let Some(current_json) = current_json else {
                outcome.missing.push(version_id);
                continue;
            };
            let current: EditStack =
                serde_json::from_str(&current_json).expect("stored edit stacks are always valid JSON");
            let merged = merge_ops(&current, ops, removed_ops);
            if merged == current {
                continue;
            }
            Self::write_edit_stack(&tx, version_id, &merged, Some(label))?;
            outcome.changed.push(version_id);
        }
        tx.commit()?;
        Ok(outcome)
    }

    /// List a version's history entries (oldest first -- the order a
    /// History panel would render top-to-bottom), without the (larger)
    /// `edit_stack_json` payload -- that's fetched separately, only when
    /// actually restoring a specific entry.
    pub fn get_history(&self, version_id: i64) -> Result<Vec<HistoryEntry>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, label, created_at FROM edit_history WHERE version_id = ?1 ORDER BY id ASC")?;
        let rows = stmt.query_map(params![version_id], |row| {
            Ok(HistoryEntry {
                id: row.get(0)?,
                label: row.get(1)?,
                created_at: row.get(2)?,
            })
        })?;
        rows.collect()
    }

    /// Moves a version's live edit stack to match a PAST history entry's
    /// own content -- used for both undo/redo (the immediately-adjacent
    /// entry, decided by the caller) and click-to-jump-to-any-point in
    /// the History panel; they're the same operation. Deliberately does
    /// NOT insert a new history row for the restore itself (moving the
    /// cursor isn't a new edit) -- see `record_edit_stack`'s own doc
    /// comment for why a later genuine edit still truncates/re-anchors
    /// correctly from here regardless.
    pub fn restore_history_entry(&self, version_id: i64, history_id: i64) -> Result<EditStack> {
        let json: String = self.conn.query_row(
            "SELECT edit_stack_json FROM edit_history WHERE id = ?1 AND version_id = ?2",
            params![history_id, version_id],
            |row| row.get(0),
        )?;
        self.conn.execute(
            "UPDATE image_versions SET edit_stack_json = ?2, updated_at = datetime('now') WHERE id = ?1",
            params![version_id, json],
        )?;
        Ok(serde_json::from_str(&json).expect("stored edit stacks are always valid JSON"))
    }

    /// Read-only counterpart to `restore_history_entry`, for the History
    /// panel's hover-preview (M4.5): a hover must be able to show a past
    /// entry's resulting look WITHOUT writing it back to `image_versions`
    /// -- only an actual click (still going through `restore_history_entry`)
    /// should commit.
    pub fn peek_history_entry(&self, version_id: i64, history_id: i64) -> Result<EditStack> {
        let json: String = self.conn.query_row(
            "SELECT edit_stack_json FROM edit_history WHERE id = ?1 AND version_id = ?2",
            params![history_id, version_id],
            |row| row.get(0),
        )?;
        Ok(serde_json::from_str(&json).expect("stored edit stacks are always valid JSON"))
    }

    /// Saves the version's CURRENT edit stack as a new named snapshot.
    /// No uniqueness constraint on `name` -- real Lightroom allows
    /// duplicate snapshot names too.
    pub fn add_snapshot(&self, version_id: i64, name: &str) -> Result<SnapshotEntry> {
        let json: String = self.conn.query_row(
            "SELECT edit_stack_json FROM image_versions WHERE id = ?1",
            params![version_id],
            |row| row.get(0),
        )?;
        self.conn.execute(
            "INSERT INTO snapshots (version_id, edit_stack_json, name) VALUES (?1, ?2, ?3)",
            params![version_id, json, name],
        )?;
        let id = self.conn.last_insert_rowid();
        let created_at: String = self.conn.query_row(
            "SELECT created_at FROM snapshots WHERE id = ?1",
            params![id],
            |row| row.get(0),
        )?;
        Ok(SnapshotEntry {
            id,
            name: name.to_string(),
            created_at,
        })
    }

    /// List a version's snapshots (oldest first), same "no payload in
    /// the list" shape `get_history` uses.
    pub fn get_snapshots(&self, version_id: i64) -> Result<Vec<SnapshotEntry>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, name, created_at FROM snapshots WHERE version_id = ?1 ORDER BY id ASC")?;
        let rows = stmt.query_map(params![version_id], |row| {
            Ok(SnapshotEntry {
                id: row.get(0)?,
                name: row.get(1)?,
                created_at: row.get(2)?,
            })
        })?;
        rows.collect()
    }

    /// Restoring a snapshot goes through `record_edit_stack` (NOT
    /// `restore_history_entry`'s "don't record" path) -- deliberately, so
    /// restoring a snapshot becomes its own undoable step in the linear
    /// history, not an operation that sits silently outside the undo
    /// system.
    pub fn restore_snapshot(&self, version_id: i64, snapshot_id: i64) -> Result<(EditStack, Vec<HistoryEntry>)> {
        let (json, name): (String, String) = self.conn.query_row(
            "SELECT edit_stack_json, name FROM snapshots WHERE id = ?1 AND version_id = ?2",
            params![snapshot_id, version_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let stack: EditStack = serde_json::from_str(&json).expect("stored edit stacks are always valid JSON");
        let label = format!("Restore Snapshot: {name}");
        let history = self.record_edit_stack(version_id, &stack, Some(&label))?;
        Ok((stack, history))
    }

    /// Read-only counterpart to `restore_snapshot`, same hover-preview
    /// purpose as `peek_history_entry` above -- never touches
    /// `image_versions` or `edit_history`.
    pub fn peek_snapshot(&self, version_id: i64, snapshot_id: i64) -> Result<EditStack> {
        let json: String = self.conn.query_row(
            "SELECT edit_stack_json FROM snapshots WHERE id = ?1 AND version_id = ?2",
            params![snapshot_id, version_id],
            |row| row.get(0),
        )?;
        Ok(serde_json::from_str(&json).expect("stored edit stacks are always valid JSON"))
    }

    pub fn delete_snapshot(&self, version_id: i64, snapshot_id: i64) -> Result<()> {
        self.conn.execute(
            "DELETE FROM snapshots WHERE id = ?1 AND version_id = ?2",
            params![snapshot_id, version_id],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::test_support::*;
    use serde_json::json;

    #[test]
    fn record_edit_stack_creates_a_labeled_history_row_on_real_content_change() {
        let catalog = Catalog::open_in_memory().unwrap();
        let image_id = catalog.add_image("/a.CR3").unwrap();
        let version_id = catalog.add_edit_stack(image_id, &EditStack::empty()).unwrap();

        let history = catalog
            .record_edit_stack(version_id, &stack_with("exposure", 0.5), Some("Exposure"))
            .unwrap();

        // The first edit records the stack it started from as "Original", so it has a "before" to undo to.
        assert_eq!(history.iter().map(|h| h.label.as_str()).collect::<Vec<_>>(), vec![ORIGINAL_LABEL, "Exposure"]);
        assert_eq!(catalog.peek_history_entry(version_id, history[0].id).unwrap(), EditStack::empty());
        assert_eq!(catalog.get_edit_stack(version_id).unwrap(), stack_with("exposure", 0.5));
    }

    #[test]
    fn the_original_row_keeps_the_stack_a_version_started_with_and_is_written_once() {
        let catalog = Catalog::open_in_memory().unwrap();
        let started = stack_with("contrast", 12.0);
        let version_id = catalog.add_edit_stack(catalog.add_image("/a.CR3").unwrap(), &started).unwrap();

        catalog.record_edit_stack(version_id, &stack_with("exposure", 0.5), Some("Exposure")).unwrap();
        let history = catalog.record_edit_stack(version_id, &stack_with("exposure", 0.9), Some("Exposure 2")).unwrap();

        assert_eq!(history.iter().map(|h| h.label.as_str()).collect::<Vec<_>>(), vec![ORIGINAL_LABEL, "Exposure", "Exposure 2"]);
        assert_eq!(catalog.peek_history_entry(version_id, history[0].id).unwrap(), started, "not an empty stack: what the version had");
        // Undoing the first edit lands on it, and the next edit from there does not add a second "Original".
        catalog.restore_history_entry(version_id, history[0].id).unwrap();
        let after = catalog.record_edit_stack(version_id, &stack_with("exposure", 0.2), Some("Exposure 3")).unwrap();
        assert_eq!(after.iter().map(|h| h.label.as_str()).collect::<Vec<_>>(), vec![ORIGINAL_LABEL, "Exposure 3"]);
    }

    #[test]
    fn record_edit_stack_skips_a_history_row_when_the_stack_did_not_actually_change() {
        let catalog = Catalog::open_in_memory().unwrap();
        let image_id = catalog.add_image("/a.CR3").unwrap();
        let version_id = catalog.add_edit_stack(image_id, &EditStack::empty()).unwrap();

        let stack = stack_with("exposure", 0.5);
        catalog.record_edit_stack(version_id, &stack, Some("Exposure")).unwrap();
        // Same content again -- simulates flushEditStack firing from a
        // call site (switching images, exporting) with nothing new
        // pending. Must be a harmless no-op-content rewrite, not a
        // spammed second history row.
        let history = catalog.record_edit_stack(version_id, &stack, Some("Exposure")).unwrap();

        assert_eq!(history.len(), 2, "Original and Exposure, and no third row");
    }

    #[test]
    fn record_edit_stack_with_no_label_never_creates_a_history_row() {
        let catalog = Catalog::open_in_memory().unwrap();
        let image_id = catalog.add_image("/a.CR3").unwrap();
        let version_id = catalog.add_edit_stack(image_id, &EditStack::empty()).unwrap();

        let history = catalog.record_edit_stack(version_id, &stack_with("exposure", 0.5), None).unwrap();

        assert_eq!(history.len(), 0);
        // The base row is still written even with no label.
        assert_eq!(catalog.get_edit_stack(version_id).unwrap(), stack_with("exposure", 0.5));
    }

    #[test]
    fn a_new_edit_after_undo_truncates_the_abandoned_redo_branch() {
        let catalog = Catalog::open_in_memory().unwrap();
        let image_id = catalog.add_image("/a.CR3").unwrap();
        let version_id = catalog.add_edit_stack(image_id, &EditStack::empty()).unwrap();

        catalog.record_edit_stack(version_id, &stack_with("exposure", 0.1), Some("Exposure 1")).unwrap();
        let history = catalog.record_edit_stack(version_id, &stack_with("exposure", 0.2), Some("Exposure 2")).unwrap();
        let first_id = history[1].id; // [0] is "Original"

        // Undo back to the first entry (restore, not a new edit).
        catalog.restore_history_entry(version_id, first_id).unwrap();

        // A genuinely new edit made from this undone position should cut
        // off "Exposure 2" -- the abandoned redo branch.
        let history = catalog.record_edit_stack(version_id, &stack_with("contrast", 5.0), Some("Contrast")).unwrap();

        assert_eq!(history.iter().map(|h| h.label.as_str()).collect::<Vec<_>>(), vec![ORIGINAL_LABEL, "Exposure 1", "Contrast"]);
    }

    #[test]
    fn history_is_pruned_to_the_cap_oldest_by_id_first() {
        let catalog = Catalog::open_in_memory().unwrap();
        let image_id = catalog.add_image("/a.CR3").unwrap();
        let version_id = catalog.add_edit_stack(image_id, &EditStack::empty()).unwrap();

        let total = MAX_HISTORY_ENTRIES + 10;
        let mut history = Vec::new();
        for i in 0..total {
            history = catalog
                .record_edit_stack(version_id, &stack_with("exposure", i as f64), Some(&format!("Edit {i}")))
                .unwrap();
        }

        assert_eq!(history.len() as i64, MAX_HISTORY_ENTRIES);
        // Oldest entries (Edit 0..10) were pruned; the newest survive.
        assert_eq!(history.first().unwrap().label, "Edit 10");
        assert_eq!(history.last().unwrap().label, format!("Edit {}", total - 1));
    }

    #[test]
    fn restore_history_entry_moves_the_live_stack_without_creating_a_new_history_row() {
        let catalog = Catalog::open_in_memory().unwrap();
        let image_id = catalog.add_image("/a.CR3").unwrap();
        let version_id = catalog.add_edit_stack(image_id, &EditStack::empty()).unwrap();

        let history = catalog
            .record_edit_stack(version_id, &stack_with("exposure", 0.5), Some("Exposure"))
            .unwrap();
        let entry_id = history[1].id; // [0] is "Original"
        catalog.record_edit_stack(version_id, &stack_with("exposure", 0.9), Some("Exposure 2")).unwrap();

        let restored = catalog.restore_history_entry(version_id, entry_id).unwrap();

        assert_eq!(restored, stack_with("exposure", 0.5));
        assert_eq!(catalog.get_edit_stack(version_id).unwrap(), stack_with("exposure", 0.5));
        // Restoring itself must not add a row: Original, Exposure, Exposure 2.
        assert_eq!(catalog.get_history(version_id).unwrap().len(), 3);
    }

    #[test]
    fn peek_history_entry_returns_the_stack_but_never_writes_it_back() {
        let catalog = Catalog::open_in_memory().unwrap();
        let image_id = catalog.add_image("/a.CR3").unwrap();
        let version_id = catalog.add_edit_stack(image_id, &EditStack::empty()).unwrap();

        let history = catalog
            .record_edit_stack(version_id, &stack_with("exposure", 0.5), Some("Exposure"))
            .unwrap();
        let entry_id = history[1].id; // [0] is "Original"
        catalog.record_edit_stack(version_id, &stack_with("exposure", 0.9), Some("Exposure 2")).unwrap();

        let peeked = catalog.peek_history_entry(version_id, entry_id).unwrap();

        assert_eq!(peeked, stack_with("exposure", 0.5));
        // Unlike restore_history_entry, the live stack (and history) must
        // be completely untouched by a peek.
        assert_eq!(catalog.get_edit_stack(version_id).unwrap(), stack_with("exposure", 0.9));
        assert_eq!(catalog.get_history(version_id).unwrap().len(), 3);
    }

    #[test]
    fn snapshots_round_trip_and_can_be_deleted() {
        let catalog = Catalog::open_in_memory().unwrap();
        let image_id = catalog.add_image("/a.CR3").unwrap();
        let version_id = catalog.add_edit_stack(image_id, &EditStack::empty()).unwrap();
        catalog.record_edit_stack(version_id, &stack_with("exposure", 0.5), Some("Exposure")).unwrap();

        let snap = catalog.add_snapshot(version_id, "Before crop").unwrap();
        assert_eq!(snap.name, "Before crop");

        let snapshots = catalog.get_snapshots(version_id).unwrap();
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].id, snap.id);

        catalog.delete_snapshot(version_id, snap.id).unwrap();
        assert_eq!(catalog.get_snapshots(version_id).unwrap().len(), 0);
    }

    #[test]
    fn snapshot_names_do_not_need_to_be_unique() {
        let catalog = Catalog::open_in_memory().unwrap();
        let image_id = catalog.add_image("/a.CR3").unwrap();
        let version_id = catalog.add_edit_stack(image_id, &EditStack::empty()).unwrap();

        catalog.add_snapshot(version_id, "Draft").unwrap();
        catalog.add_snapshot(version_id, "Draft").unwrap();

        assert_eq!(catalog.get_snapshots(version_id).unwrap().len(), 2);
    }

    #[test]
    fn restore_snapshot_is_itself_an_undoable_step() {
        let catalog = Catalog::open_in_memory().unwrap();
        let image_id = catalog.add_image("/a.CR3").unwrap();
        let version_id = catalog.add_edit_stack(image_id, &EditStack::empty()).unwrap();

        catalog.record_edit_stack(version_id, &stack_with("exposure", 0.5), Some("Exposure")).unwrap();
        let snap = catalog.add_snapshot(version_id, "Checkpoint").unwrap();
        catalog.record_edit_stack(version_id, &stack_with("exposure", 0.9), Some("Exposure 2")).unwrap();

        let (restored, history) = catalog.restore_snapshot(version_id, snap.id).unwrap();

        assert_eq!(restored, stack_with("exposure", 0.5));
        assert_eq!(catalog.get_edit_stack(version_id).unwrap(), stack_with("exposure", 0.5));
        // Unlike restore_history_entry, restoring a snapshot DOES add a
        // new history row of its own -- it's an undoable step, not a
        // silent side channel outside the undo system.
        assert_eq!(history.last().unwrap().label, "Restore Snapshot: Checkpoint");
        assert_eq!(catalog.get_history(version_id).unwrap().len(), 4);
    }

    #[test]
    fn peek_snapshot_returns_the_stack_but_never_writes_it_back() {
        let catalog = Catalog::open_in_memory().unwrap();
        let image_id = catalog.add_image("/a.CR3").unwrap();
        let version_id = catalog.add_edit_stack(image_id, &EditStack::empty()).unwrap();

        catalog.record_edit_stack(version_id, &stack_with("exposure", 0.5), Some("Exposure")).unwrap();
        let snap = catalog.add_snapshot(version_id, "Checkpoint").unwrap();
        catalog.record_edit_stack(version_id, &stack_with("exposure", 0.9), Some("Exposure 2")).unwrap();

        let peeked = catalog.peek_snapshot(version_id, snap.id).unwrap();

        assert_eq!(peeked, stack_with("exposure", 0.5));
        // Unlike restore_snapshot, a peek must not touch the live stack or
        // add a history row (Original, Exposure, Exposure 2).
        assert_eq!(catalog.get_edit_stack(version_id).unwrap(), stack_with("exposure", 0.9));
        assert_eq!(catalog.get_history(version_id).unwrap().len(), 3);
    }

    #[test]
    fn get_history_orders_oldest_first() {
        let catalog = Catalog::open_in_memory().unwrap();
        let image_id = catalog.add_image("/a.CR3").unwrap();
        let version_id = catalog.add_edit_stack(image_id, &EditStack::empty()).unwrap();

        catalog.record_edit_stack(version_id, &stack_with("exposure", 0.1), Some("First")).unwrap();
        catalog.record_edit_stack(version_id, &stack_with("exposure", 0.2), Some("Second")).unwrap();

        let history = catalog.get_history(version_id).unwrap();
        assert_eq!(history.iter().map(|h| h.label.as_str()).collect::<Vec<_>>(), vec![ORIGINAL_LABEL, "First", "Second"]);
    }

    #[test]
    fn deleting_a_version_cascades_to_its_history_and_snapshots() {
        let catalog = Catalog::open_in_memory().unwrap();
        let image_id = catalog.add_image("/a.CR3").unwrap();
        let version_id = catalog.add_edit_stack(image_id, &EditStack::empty()).unwrap();
        catalog.record_edit_stack(version_id, &stack_with("exposure", 0.5), Some("Exposure")).unwrap();
        catalog.add_snapshot(version_id, "Checkpoint").unwrap();

        catalog.remove_images(&[image_id]).unwrap();

        // Foreign-key ON DELETE CASCADE is enforced (enable_foreign_keys
        // is set on every connection) -- both child tables must be empty,
        // not just orphaned, after the parent version row is gone.
        let history_count: i64 = catalog
            .conn
            .query_row("SELECT COUNT(*) FROM edit_history WHERE version_id = ?1", params![version_id], |r| r.get(0))
            .unwrap();
        let snapshot_count: i64 = catalog
            .conn
            .query_row("SELECT COUNT(*) FROM snapshots WHERE version_id = ?1", params![version_id], |r| r.get(0))
            .unwrap();
        assert_eq!(history_count, 0);
        assert_eq!(snapshot_count, 0);
    }

    fn version_with(catalog: &Catalog, path: &str, ops: Vec<serde_json::Value>) -> i64 {
        let image_id = catalog.add_image(path).unwrap();
        catalog.add_edit_stack(image_id, &EditStack { schema_version: 1, ops }).unwrap()
    }

    #[test]
    fn apply_ops_replaces_only_the_named_ops_and_keeps_the_targets_own() {
        let catalog = Catalog::open_in_memory().unwrap();
        let b = version_with(&catalog, "/b.CR3", vec![json!({"op": "exposure", "value": 0.1}), json!({"op": "contrast", "value": 7.0})]);
        let c = version_with(&catalog, "/c.CR3", vec![]);

        let outcome = catalog
            .apply_ops_to_versions(&[b, c], &[json!({"op": "exposure", "value": 0.8})], &[], "Auto Sync: Exposure")
            .unwrap();

        assert_eq!(outcome.changed, vec![b, c]);
        assert!(outcome.missing.is_empty());
        assert_eq!(
            catalog.get_edit_stack(b).unwrap().ops,
            vec![json!({"op": "exposure", "value": 0.8}), json!({"op": "contrast", "value": 7.0})],
            "replaced in place, the target's contrast untouched"
        );
        assert_eq!(catalog.get_edit_stack(c).unwrap().ops, vec![json!({"op": "exposure", "value": 0.8})]);
        assert_eq!(catalog.get_history(b).unwrap().iter().map(|h| h.label.as_str()).collect::<Vec<_>>(), vec![ORIGINAL_LABEL, "Auto Sync: Exposure"]);
    }

    #[test]
    fn apply_ops_removes_named_ops_and_a_whole_list_op_is_replaced_as_one() {
        let catalog = Catalog::open_in_memory().unwrap();
        let b = version_with(
            &catalog,
            "/b.CR3",
            vec![json!({"op": "exposure", "value": 0.1}), json!({"op": "tone_curve", "points": [[0, 0], [1, 1]]})],
        );

        catalog
            .apply_ops_to_versions(
                &[b],
                &[json!({"op": "tone_curve", "points": [[0, 0], [0.5, 0.6], [1, 1]]})],
                &["exposure".to_string()],
                "Auto Sync: Tone Curve",
            )
            .unwrap();

        assert_eq!(
            catalog.get_edit_stack(b).unwrap().ops,
            vec![json!({"op": "tone_curve", "points": [[0, 0], [0.5, 0.6], [1, 1]]})]
        );
    }

    #[test]
    fn apply_ops_that_change_nothing_write_no_history_and_report_no_change() {
        let catalog = Catalog::open_in_memory().unwrap();
        let b = version_with(&catalog, "/b.CR3", vec![json!({"op": "exposure", "value": 0.8}), json!({"op": "contrast", "value": 7.0})]);

        let outcome = catalog
            .apply_ops_to_versions(&[b], &[json!({"op": "exposure", "value": 0.8})], &["vignette".to_string()], "Auto Sync: Exposure")
            .unwrap();

        assert!(outcome.changed.is_empty());
        assert!(catalog.get_history(b).unwrap().is_empty());
    }

    #[test]
    fn apply_ops_skips_a_missing_version_and_still_writes_the_others() {
        let catalog = Catalog::open_in_memory().unwrap();
        let b = version_with(&catalog, "/b.CR3", vec![]);

        let outcome = catalog
            .apply_ops_to_versions(&[9999, b], &[json!({"op": "exposure", "value": 0.8})], &[], "Auto Sync: Exposure")
            .unwrap();

        assert_eq!(outcome.missing, vec![9999]);
        assert_eq!(outcome.changed, vec![b]);
    }

    #[test]
    fn apply_ops_is_one_transaction_a_failure_rolls_every_target_back() {
        let catalog = Catalog::open_in_memory().unwrap();
        let b = version_with(&catalog, "/b.CR3", vec![]);
        let c = version_with(&catalog, "/c.CR3", vec![]);
        // Make the second target's history insert fail.
        catalog
            .conn
            .execute_batch(&format!(
                "CREATE TRIGGER fail_c BEFORE INSERT ON edit_history WHEN NEW.version_id = {c} BEGIN SELECT RAISE(ABORT, 'boom'); END;"
            ))
            .unwrap();

        let result = catalog.apply_ops_to_versions(&[b, c], &[json!({"op": "exposure", "value": 0.8})], &[], "Auto Sync: Exposure");

        assert!(result.is_err());
        assert!(catalog.get_edit_stack(b).unwrap().ops.is_empty(), "b was rolled back");
        assert!(catalog.get_history(b).unwrap().is_empty());
    }

    #[test]
    fn merge_ops_ignores_an_incoming_op_with_no_name_by_appending_it() {
        let target = EditStack { schema_version: 1, ops: vec![json!({"op": "exposure", "value": 0.1})] };
        let merged = merge_ops(&target, &[json!({"value": 1})], &[]);
        assert_eq!(merged.ops.len(), 2);
    }
}
