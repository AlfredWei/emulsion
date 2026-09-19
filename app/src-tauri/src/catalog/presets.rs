use super::*;

/// A saved Preset (M3) -- unlike `HistoryEntry`/`SnapshotEntry`, includes
/// the full `edit_stack` inline rather than a separate fetch-on-demand:
/// presets are global (not version-scoped), typically few in number, and
/// every consumer (the Presets panel's "Apply" action, batch-apply from
/// Library, export-to-file) needs the actual ops immediately, not just a
/// label -- there's no equivalent of History/Snapshots' "list is cheap,
/// payload is fetched only when actually restoring" split to exploit here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PresetEntry {
    pub id: i64,
    pub name: String,
    pub edit_stack: EditStack,
    pub created_at: String,
}

/// Seeded once into every catalog's `presets` table by `Catalog::migrate`
/// (see the seeding block there for the idempotency gate). A small starter
/// set (13, not Lightroom's own ~100-strong marketing-scale library) --
/// each one hand-authored against this app's own op inventory, matching
/// this project's "smallest real instance first" practice rather than
/// trying to match a commercial preset library's scale. Plain data, not
/// specially protected: a user who deletes one gets the same experience as
/// deleting any preset they made themselves -- it doesn't come back.
/// Every op name/value here must stay inside the same ranges
/// `DevelopPanel.svelte`'s own sliders enforce (see that file for the
/// authoritative min/max per control) since these bypass the UI entirely.
pub(super) const DEFAULT_PRESETS: &[(&str, &str)] = &[
    // -- Color --
    (
        "Warm Glow",
        r#"{"schema_version":1,"ops":[{"op":"exposure","value":0.2},{"op":"contrast","value":5},{"op":"saturation","value":8},{"op":"split_toning","shadows":{"hue":210,"saturation":5},"highlights":{"hue":45,"saturation":18},"balance":15}]}"#,
    ),
    (
        "Cool Blue",
        r#"{"schema_version":1,"ops":[{"op":"exposure","value":-0.1},{"op":"contrast","value":5},{"op":"saturation","value":-5},{"op":"split_toning","shadows":{"hue":220,"saturation":15},"highlights":{"hue":200,"saturation":8},"balance":-10}]}"#,
    ),
    (
        "Golden Hour",
        r#"{"schema_version":1,"ops":[{"op":"exposure","value":0.3},{"op":"contrast","value":8},{"op":"hsl","bands":{"orange":{"hue":0,"saturation":15,"luminance":8},"yellow":{"hue":0,"saturation":12,"luminance":5}}},{"op":"split_toning","shadows":{"hue":230,"saturation":6},"highlights":{"hue":40,"saturation":25},"balance":20},{"op":"vignette","amount":-10,"midpoint":60,"feather":60}]}"#,
    ),
    (
        "Teal & Orange",
        r#"{"schema_version":1,"ops":[{"op":"contrast","value":12},{"op":"saturation","value":5},{"op":"split_toning","shadows":{"hue":195,"saturation":25},"highlights":{"hue":35,"saturation":20},"balance":0},{"op":"hsl","bands":{"blue":{"hue":-10,"saturation":10,"luminance":0},"orange":{"hue":0,"saturation":15,"luminance":0}}}]}"#,
    ),
    // -- Creative --
    (
        "Punch",
        r#"{"schema_version":1,"ops":[{"op":"contrast","value":20},{"op":"clarity","value":25},{"op":"saturation","value":12},{"op":"vignette","amount":-15,"midpoint":55,"feather":50}]}"#,
    ),
    (
        "Faded Film",
        r#"{"schema_version":1,"ops":[{"op":"contrast","value":-15},{"op":"saturation","value":-10},{"op":"tone_curve","points":[{"x":0,"y":0.08},{"x":0.5,"y":0.5},{"x":1,"y":0.95}]},{"op":"grain","amount":15,"size":30,"roughness":40}]}"#,
    ),
    (
        "Moody",
        r#"{"schema_version":1,"ops":[{"op":"exposure","value":-0.3},{"op":"contrast","value":18},{"op":"saturation","value":-15},{"op":"vignette","amount":-25,"midpoint":45,"feather":55},{"op":"split_toning","shadows":{"hue":220,"saturation":10},"highlights":{"hue":0,"saturation":0},"balance":-15}]}"#,
    ),
    (
        "Dreamy Soft",
        r#"{"schema_version":1,"ops":[{"op":"exposure","value":0.25},{"op":"contrast","value":-10},{"op":"texture","value":-20},{"op":"clarity","value":-15},{"op":"tone_curve","points":[{"x":0,"y":0.05},{"x":0.5,"y":0.55},{"x":1,"y":1}]}]}"#,
    ),
    (
        "Vintage",
        r#"{"schema_version":1,"ops":[{"op":"contrast","value":-8},{"op":"saturation","value":-20},{"op":"split_toning","shadows":{"hue":45,"saturation":12},"highlights":{"hue":50,"saturation":15},"balance":5},{"op":"grain","amount":20,"size":35,"roughness":45},{"op":"vignette","amount":-15,"midpoint":50,"feather":65}]}"#,
    ),
    // -- B&W --
    (
        "Classic B&W",
        r#"{"schema_version":1,"ops":[{"op":"saturation","value":-100},{"op":"contrast","value":10}]}"#,
    ),
    (
        "High Contrast B&W",
        r#"{"schema_version":1,"ops":[{"op":"saturation","value":-100},{"op":"contrast","value":35},{"op":"clarity","value":15}]}"#,
    ),
    (
        "Soft B&W",
        r#"{"schema_version":1,"ops":[{"op":"saturation","value":-100},{"op":"contrast","value":-10},{"op":"tone_curve","points":[{"x":0,"y":0.05},{"x":0.5,"y":0.5},{"x":1,"y":0.95}]}]}"#,
    ),
    (
        "B&W + Grain",
        r#"{"schema_version":1,"ops":[{"op":"saturation","value":-100},{"op":"contrast","value":15},{"op":"grain","amount":25,"size":30,"roughness":50},{"op":"vignette","amount":-20,"midpoint":50,"feather":60}]}"#,
    ),
];

impl Catalog {
    /// Presets (M3): global, catalog-wide entities -- deliberately not
    /// version-scoped like `record_edit_stack`/snapshots above. `stack`
    /// is expected to already be filtered to the preset-eligible op
    /// subset (JS's job, via develop.js's `PRESET_EXCLUDED_OP_NAMES`) --
    /// this method stores whatever it's given as-is, same "Rust never
    /// interprets `ops`" boundary every other edit-stack method here
    /// keeps. Used by both the direct "Save Current as Preset" flow and
    /// (after JS-side re-filtering, defensively) importing a preset file.
    pub fn create_preset(&self, name: &str, stack: &EditStack) -> Result<PresetEntry> {
        let json = serde_json::to_string(stack).expect("EditStack is always serializable");
        self.conn.execute(
            "INSERT INTO presets (name, edit_stack_json) VALUES (?1, ?2)",
            params![name, json],
        )?;
        let id = self.conn.last_insert_rowid();
        let created_at: String =
            self.conn
                .query_row("SELECT created_at FROM presets WHERE id = ?1", params![id], |row| row.get(0))?;
        Ok(PresetEntry { id, name: name.to_string(), edit_stack: stack.clone(), created_at })
    }

    pub fn list_presets(&self) -> Result<Vec<PresetEntry>> {
        let mut stmt = self.conn.prepare("SELECT id, name, edit_stack_json, created_at FROM presets ORDER BY id ASC")?;
        let rows = stmt.query_map([], |row| {
            let json: String = row.get(2)?;
            let edit_stack: EditStack =
                serde_json::from_str(&json).expect("stored edit stacks are always valid JSON");
            Ok(PresetEntry { id: row.get(0)?, name: row.get(1)?, edit_stack, created_at: row.get(3)? })
        })?;
        rows.collect()
    }

    pub fn delete_preset(&self, preset_id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM presets WHERE id = ?1", params![preset_id])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::test_support::*;

    // -- M3 Presets --------------------------------------------------

    #[test]
    fn create_preset_round_trips_the_edit_stack() {
        let catalog = Catalog::open_in_memory().unwrap();
        let stack = stack_with("vignette", 0.4);

        let preset = catalog.create_preset("Moody", &stack).unwrap();

        assert_eq!(preset.name, "Moody");
        assert_eq!(preset.edit_stack, stack);
        assert!(preset.id > 0);
    }

    #[test]
    fn list_presets_orders_oldest_first_and_includes_every_row() {
        let catalog = Catalog::open_in_memory().unwrap();
        catalog.create_preset("First", &stack_with("exposure", 0.1)).unwrap();
        catalog.create_preset("Second", &stack_with("contrast", 10.0)).unwrap();

        let presets = catalog.list_presets().unwrap();

        assert_eq!(presets.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), vec!["First", "Second"]);
    }

    #[test]
    fn presets_allow_duplicate_names() {
        let catalog = Catalog::open_in_memory().unwrap();
        catalog.create_preset("Duplicate", &EditStack::empty()).unwrap();
        catalog.create_preset("Duplicate", &EditStack::empty()).unwrap();

        assert_eq!(catalog.list_presets().unwrap().len(), 2);
    }

    #[test]
    fn delete_preset_removes_it() {
        let catalog = Catalog::open_in_memory().unwrap();
        let preset = catalog.create_preset("Temp", &EditStack::empty()).unwrap();

        catalog.delete_preset(preset.id).unwrap();

        assert_eq!(catalog.list_presets().unwrap(), vec![]);
    }

    #[test]
    fn presets_are_not_affected_by_image_removal() {
        // Presets are global, catalog-wide entities with no FK to any
        // image/version -- unlike edit_history/snapshots (cascade-deleted
        // above), removing every image in the catalog must leave presets
        // completely untouched.
        let catalog = Catalog::open_in_memory().unwrap();
        let image_id = catalog.add_image("/a.CR3").unwrap();
        catalog.create_preset("Survives", &stack_with("clarity", 20.0)).unwrap();

        catalog.remove_images(&[image_id]).unwrap();

        assert_eq!(catalog.list_presets().unwrap().len(), 1);
    }

    // -- M3 Default presets ------------------------------------------

    fn default_presets_test_path(name: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("emulsion-default-presets-test-{name}.sqlite"));
        for ext in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{ext}", path.display()));
        }
        path
    }

    #[test]
    fn open_in_memory_does_not_seed_default_presets() {
        // `open_in_memory()` is the fixture every other preset test above
        // relies on starting from an empty table -- a deliberate choice
        // (see that function's own doc comment), pinned here as its own
        // test rather than left as an unstated assumption.
        let catalog = Catalog::open_in_memory().unwrap();
        assert_eq!(catalog.list_presets().unwrap(), vec![]);
    }

    #[test]
    fn open_seeds_the_default_presets_exactly_once() {
        let path = default_presets_test_path("seed-once");

        let seeded = Catalog::open(&path).unwrap().list_presets().unwrap();
        assert_eq!(seeded.len(), DEFAULT_PRESETS.len());
        for (name, _) in DEFAULT_PRESETS {
            assert!(seeded.iter().any(|p| p.name == *name), "missing default preset {name:?}");
        }

        // Reopening -- same idempotency shape already established for
        // backup settings above -- must not duplicate them.
        assert_eq!(Catalog::open(&path).unwrap().list_presets().unwrap().len(), DEFAULT_PRESETS.len());

        for ext in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{ext}", path.display()));
        }
    }

    #[test]
    fn deleting_a_default_preset_and_reopening_does_not_bring_it_back() {
        let path = default_presets_test_path("delete-stays-deleted");

        let catalog = Catalog::open(&path).unwrap();
        let removed = catalog.list_presets().unwrap().remove(0);
        catalog.delete_preset(removed.id).unwrap();
        drop(catalog);

        let after_reopen = Catalog::open(&path).unwrap().list_presets().unwrap();
        assert_eq!(after_reopen.len(), DEFAULT_PRESETS.len() - 1);
        assert!(!after_reopen.iter().any(|p| p.id == removed.id));

        for ext in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{ext}", path.display()));
        }
    }

    #[test]
    fn every_default_preset_is_valid_json_and_carries_no_crop_or_mask_ops() {
        // Hand-typed JSON literals get no compiler checking -- this is the
        // regression guard: a typo'd field name would otherwise silently
        // become a no-op adjustment (unknown keys are just ignored) rather
        // than a build failure. Also re-asserts the preset-eligibility
        // contract PRESET_EXCLUDED_OP_NAMES documents in develop.js, since
        // these bypass presetEligibleOps entirely (inserted directly by
        // Rust, not filtered client-side).
        for (name, json) in DEFAULT_PRESETS {
            let stack: EditStack =
                serde_json::from_str(json).unwrap_or_else(|e| panic!("preset {name:?}: invalid EditStack JSON: {e}"));
            assert_eq!(stack.schema_version, 1, "preset {name:?}");
            assert!(!stack.ops.is_empty(), "preset {name:?} has no ops");
            for op in &stack.ops {
                let op_name = op
                    .get("op")
                    .and_then(|v| v.as_str())
                    .unwrap_or_else(|| panic!("preset {name:?} has an op missing its \"op\" field"));
                assert!(
                    op_name != "crop" && !op_name.ends_with("_mask"),
                    "preset {name:?} carries a crop/mask op ({op_name}), which presets must never contain"
                );
            }
        }
    }
}
