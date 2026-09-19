use super::presets::DEFAULT_PRESETS;
use super::*;

impl Catalog {
    pub(super) fn migrate(conn: &Connection, seed_defaults: bool) -> Result<()> {
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS images (
                id INTEGER PRIMARY KEY,
                path TEXT NOT NULL UNIQUE,
                content_hash TEXT,
                file_size INTEGER,
                thumbnail_path TEXT,
                stack_id INTEGER,
                added_at TEXT NOT NULL DEFAULT (datetime('now')),
                import_batch INTEGER,
                camera_make TEXT,
                camera_model TEXT,
                lens_model TEXT,
                iso INTEGER,
                aperture REAL,
                shutter_speed REAL,
                focal_length REAL,
                exposure_bias REAL,
                metering_mode TEXT,
                flash TEXT,
                width INTEGER,
                height INTEGER,
                latitude REAL,
                longitude REAL,
                altitude REAL,
                captured_at TEXT,
                copyright TEXT,
                contact TEXT,
                -- Face detection (M5 Slice 6 follow-up): whether detection
                -- has EVER been attempted for this image, regardless of
                -- whether it found any faces -- distinct from zero rows in
                -- faces, which is ambiguous between never scanned and
                -- scanned-but-genuinely-no-faces. Without this, selecting
                -- a photo or running Find People would re-decode and
                -- re-run the detector on every already-scanned image on
                -- every call, and a real face-free photo could never be
                -- told apart from one nobody had gotten to yet.
                faces_scanned INTEGER NOT NULL DEFAULT 0
            );

            CREATE INDEX IF NOT EXISTS idx_images_content_hash
                ON images(content_hash);

            CREATE TABLE IF NOT EXISTS image_versions (
                id INTEGER PRIMARY KEY,
                image_id INTEGER NOT NULL REFERENCES images(id) ON DELETE CASCADE,
                is_virtual_copy INTEGER NOT NULL DEFAULT 0,
                edit_stack_json TEXT NOT NULL,
                rating INTEGER NOT NULL DEFAULT 0 CHECK (rating BETWEEN 0 AND 5),
                flag TEXT NOT NULL DEFAULT 'none' CHECK (flag IN ('none','pick','reject')),
                color_label TEXT NOT NULL DEFAULT 'none'
                    CHECK (color_label IN ('none','red','yellow','green','blue','purple')),
                caption TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            -- M5 Slice 3 (GPU performance validation): list_images()'s own
            -- per-row correlated subquery filters this table by
            -- `image_id` (not its indexed `id` primary key) to find each
            -- image's oldest non-virtual-copy version. With no index on
            -- that column, SQLite has no way to satisfy the filter except
            -- a full table scan of image_versions for every single row of
            -- images -- confirmed directly: list_images() took 37.4s
            -- (release build) over a real 50,000-image catalog before
            -- this index existed (catalog.rs's own
            -- catalog_scales_to_50k_images test), dropping to ~75ms after
            -- adding it. A real
            -- regression against PRD §9's own catalog-open-time target for
            -- a 50k-image catalog, not a hypothetical one.
            CREATE INDEX IF NOT EXISTS idx_image_versions_image_id
                ON image_versions(image_id);

            -- M2 Slice 4 (keywording): no UNIQUE(parent_id, name) here --
            -- SQLite treats every NULL as distinct in a UNIQUE check, so
            -- that constraint would silently fail to stop two different
            -- top-level (parent_id NULL) keywords sharing a name.
            -- Uniqueness is enforced at the application layer instead (see
            -- assign_keyword_path's find-or-create), safe because
            -- AppState.catalog is one Arc<Mutex<Catalog>> -- no concurrent
            -- writer can interleave between a SELECT and its INSERT.
            CREATE TABLE IF NOT EXISTS keywords (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                parent_id INTEGER REFERENCES keywords(id) ON DELETE CASCADE,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS image_keywords (
                image_id INTEGER NOT NULL REFERENCES images(id) ON DELETE CASCADE,
                keyword_id INTEGER NOT NULL REFERENCES keywords(id) ON DELETE CASCADE,
                PRIMARY KEY (image_id, keyword_id)
            );

            -- M2 Slice 5 (collections). Unlike keywords' NULL-parent_id
            -- situation, a plain CHECK here is correctly enforced by
            -- SQLite (not a NULL-distinctness trap) -- matches this
            -- schema's existing use of CHECK for the same kind of
            -- invariant (rating/flag/color_label above).
            --
            -- rules_json is opaque JSON (a Vec<serde_json::Value> on the
            -- Rust side) round-tripped without interpretation -- rule
            -- evaluation happens entirely in the frontend, since it only
            -- ever needs rating/flag/color_label/keyword data that's
            -- already loaded there, no pixel access. If a future feature
            -- ever needs Rust-side action scoped to the images in a smart
            -- collection (e.g. batch-export a smart collection), the fix
            -- is computing the matching id list in JS and passing it
            -- through the existing id-list commands (export_images,
            -- remove_images, add_images_to_collection, ...) -- not a
            -- second Rust rule-interpreter.
            --
            -- collection_images is used ONLY for manual collections;
            -- smart collections never get rows here -- their membership
            -- is always computed, never stored.
            CREATE TABLE IF NOT EXISTS collections (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                is_smart INTEGER NOT NULL DEFAULT 0,
                rules_json TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                CHECK (
                    (is_smart = 0 AND rules_json IS NULL) OR
                    (is_smart = 1 AND rules_json IS NOT NULL)
                )
            );

            CREATE TABLE IF NOT EXISTS collection_images (
                collection_id INTEGER NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
                image_id INTEGER NOT NULL REFERENCES images(id) ON DELETE CASCADE,
                PRIMARY KEY (collection_id, image_id)
            );

            CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );

            -- M3 (History/Undo/Snapshots). Both store FULL edit-stack
            -- snapshots, not diffs -- these are small JSON blobs (a photo's
            -- whole edit stack is a handful of KB at most), so diffing
            -- would be real complexity for no real benefit. `id` (not
            -- `created_at`, which is only second-resolution here) is what
            -- orders/prunes edit_history -- multiple entries can land in
            -- the same second under a burst of edits, but `id` (an
            -- AUTOINCREMENT-free INTEGER PRIMARY KEY, still strictly
            -- monotonic per SQLite's own rowid rules) never ties.
            CREATE TABLE IF NOT EXISTS edit_history (
                id INTEGER PRIMARY KEY,
                version_id INTEGER NOT NULL REFERENCES image_versions(id) ON DELETE CASCADE,
                edit_stack_json TEXT NOT NULL,
                label TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE INDEX IF NOT EXISTS idx_edit_history_version
                ON edit_history(version_id, id);

            -- Named, user-created save points -- deliberately no
            -- UNIQUE(version_id, name): real Lightroom allows duplicate
            -- snapshot names, and enforcing uniqueness here would add a
            -- whole name-taken error-handling path with no user-facing
            -- requirement asking for it.
            CREATE TABLE IF NOT EXISTS snapshots (
                id INTEGER PRIMARY KEY,
                version_id INTEGER NOT NULL REFERENCES image_versions(id) ON DELETE CASCADE,
                edit_stack_json TEXT NOT NULL,
                name TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE INDEX IF NOT EXISTS idx_snapshots_version
                ON snapshots(version_id, id);

            -- Presets (M3): a global, catalog-wide entity, unlike
            -- edit_history/snapshots above -- deliberately no FK to any
            -- image/version (same shape as `collections`), since a
            -- preset outlives and is independent of any single photo.
            -- `edit_stack_json` holds an EditStack-shaped JSON blob, but
            -- only the preset-ELIGIBLE subset of ops (global tonal/color
            -- adjustments) -- crop and every mask kind are excluded at
            -- save time in JS (develop.js), since both carry per-image
            -- geometry/sampled-pixel data that wouldn't transfer
            -- meaningfully to a different photo. No UNIQUE(name): same
            -- reasoning as snapshots' own duplicate-name allowance above.
            CREATE TABLE IF NOT EXISTS presets (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                edit_stack_json TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            -- HDR merge (M5, RFC-0003 §3.6): pure provenance -- which
            -- originals fed a merge, in what order, with what computed
            -- alignment/EV -- never consulted by any render path. New
            -- modeling, not a repurposing of `images.stack_id` (confirmed
            -- unused/unimplemented anywhere) or `image_versions.is_virtual_copy`
            -- (confirmed to mean 'multiple edit stacks over one file', the
            -- opposite relationship from 'one file derived from many').
            -- `source_image_id` deliberately has no FK/CASCADE of its own:
            -- a source image being removed later shouldn't silently delete
            -- the *other* provenance rows for the same merge result.
            CREATE TABLE IF NOT EXISTS hdr_merge_sources (
                result_image_id INTEGER NOT NULL REFERENCES images(id) ON DELETE CASCADE,
                source_image_id INTEGER NOT NULL,
                ordinal INTEGER NOT NULL,
                ev_offset REAL NOT NULL,
                dx INTEGER NOT NULL,
                dy INTEGER NOT NULL,
                PRIMARY KEY (result_image_id, source_image_id)
            );

            -- Panorama merge (M5, RFC-0004 §3.6): same pure-provenance
            -- framing as hdr_merge_sources just above -- which originals
            -- fed a stitch, in what order, with what final reference-
            -- space homography -- never consulted by any render path.
            -- `homography_json` is a JSON array of the 9 row-major matrix
            -- values: unlike HDR's dx/dy/ev_offset, nothing else in the
            -- catalog ever queries into individual matrix cells, so
            -- there's no reason to model them as separate columns.
            CREATE TABLE IF NOT EXISTS panorama_merge_sources (
                result_image_id INTEGER NOT NULL REFERENCES images(id) ON DELETE CASCADE,
                source_image_id INTEGER NOT NULL,
                ordinal INTEGER NOT NULL,
                homography_json TEXT NOT NULL,
                PRIMARY KEY (result_image_id, source_image_id)
            );

            -- Face detection / People view (M5 Slice 6, RFC-0005 §3.4).
            -- Schema only at this point -- no detection/embedding model has
            -- been chosen yet (RFC-0005 §5 open questions), so nothing
            -- populates these tables in production yet. Landing the schema
            -- ahead of the model choice mirrors how hdr_merge_sources/
            -- panorama_merge_sources' shape was settled by their RFCs
            -- independently of the pixel-processing code that fills them.
            --
            -- `person_id` starts NULL (detected but not yet clustered) --
            -- clustering (face_cluster.rs) is what populates it later.
            -- `embedding` lives on `images`, not `image_versions`: a face's
            -- identity doesn't change across virtual copies/edits of the
            -- same source image, matching how content_hash/EXIF already
            -- live on `images`. Deleting an image cascades to its `faces`
            -- rows; deleting a `people` row (a future merge/cleanup action)
            -- clears `faces.person_id` back to NULL rather than deleting
            -- the underlying detections -- same both-directions shape this
            -- schema already uses for hdr_merge_sources/panorama_merge_sources.
            CREATE TABLE IF NOT EXISTS people (
                id INTEGER PRIMARY KEY,
                name TEXT,
                cover_face_id INTEGER,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS faces (
                id INTEGER PRIMARY KEY,
                image_id INTEGER NOT NULL REFERENCES images(id) ON DELETE CASCADE,
                person_id INTEGER REFERENCES people(id) ON DELETE SET NULL,
                bbox_x REAL NOT NULL,
                bbox_y REAL NOT NULL,
                bbox_w REAL NOT NULL,
                bbox_h REAL NOT NULL,
                embedding BLOB NOT NULL,
                -- 0 = a real detection, eligible for clustering (whether
                -- or not it's been assigned a person_id yet); 1 = the user
                -- confirmed this crop is not a face -- permanently
                -- excluded from clustering, but the row is kept, not
                -- deleted (see this table's own top comment).
                excluded INTEGER NOT NULL DEFAULT 0,
                detected_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE INDEX IF NOT EXISTS idx_faces_image_id ON faces(image_id);
            CREATE INDEX IF NOT EXISTS idx_faces_person_id ON faces(person_id);

            -- Export plugins (M5, RFC-0006 §3.1): the v0 extensibility
            -- surface -- a user-configured external command run after
            -- export completes, resolved via export_plugin.rs's
            -- placeholder substitution against args_template_json. Same
            -- shape as `presets` above (a global, catalog-wide named
            -- entity with one opaque payload column), not the single-value
            -- `settings` KV pattern -- this is a list of independently
            -- named entries, not one round-tripped value.
            CREATE TABLE IF NOT EXISTS export_plugins (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                command TEXT NOT NULL,
                args_template_json TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            ",
        )?;

        // Defaults inserted once, idempotently -- INSERT OR IGNORE is a
        // no-op on every open() after the first. `backup_frequency`
        // defaults to "weekly", not "never": at the time this table was
        // added (M2), the close-prompt dialog was the *only* place backup
        // settings could ever be changed (no settings menu existed yet), so
        // defaulting to "never" would have left no reachable way to ever
        // opt in. M3's Settings dialog now gives a second editing surface,
        // but the same default still matches PRD/MILESTONES' own framing of
        // this feature as modeled on real Lightroom, which also prompts
        // from the first session with a default weekly cadence.
        conn.execute_batch(
            "
            INSERT OR IGNORE INTO settings (key, value) VALUES
                ('backup_frequency', 'weekly'),
                ('backup_check_integrity', '1'),
                ('backup_optimize', '0');
            ",
        )?;

        // Default presets (M3), seeded once. `presets` has no UNIQUE(name)
        // to key an `INSERT OR IGNORE` off (by design -- see the table's
        // own schema comment above), so the gate is a dedicated settings
        // flag instead, same "runs every open(), real no-op after the
        // first" shape as the backup defaults just above. A user who
        // deletes some or all of these afterward doesn't get them back --
        // the flag stays set, matching how real Lightroom's own default
        // presets don't reappear once removed. Gated on `seed_defaults` so
        // `open_in_memory()`'s test fixtures stay a clean, empty table --
        // see that function's own doc comment.
        if seed_defaults {
            let default_presets_seeded: bool = conn
                .query_row(
                    "SELECT 1 FROM settings WHERE key = 'default_presets_seeded'",
                    [],
                    |_| Ok(()),
                )
                .optional()?
                .is_some();
            if !default_presets_seeded {
                for (name, edit_stack_json) in DEFAULT_PRESETS {
                    conn.execute(
                        "INSERT INTO presets (name, edit_stack_json) VALUES (?1, ?2)",
                        params![name, edit_stack_json],
                    )?;
                }
                conn.execute(
                    "INSERT INTO settings (key, value) VALUES ('default_presets_seeded', '1')",
                    [],
                )?;
            }
        }

        // M2 Slice 2: the columns above were added to the CREATE TABLE text
        // after real catalogs already existed with the pre-Slice-2 schema --
        // `CREATE TABLE IF NOT EXISTS` is a no-op against a table that's
        // already there, so a real ALTER TABLE step is needed to actually
        // land these columns on an existing catalog file, not just on a
        // brand-new one. Found empirically: this project's own dev catalog
        // (in continuous use since M1) was still missing all of these
        // columns until this fix, and every metadata-bearing import against
        // it failed with a real SQL error as a result -- this is the first
        // schema change since content_hash/file_size, which happened to
        // land before any real catalog existed to migrate, so this gap was
        // latent until now. Each ADD COLUMN is tried independently and a
        // "duplicate column name" error (SQLite's way of saying "already
        // applied") is swallowed; any other error still propagates.
        for ddl in [
            "ALTER TABLE images ADD COLUMN camera_make TEXT",
            "ALTER TABLE images ADD COLUMN camera_model TEXT",
            "ALTER TABLE images ADD COLUMN lens_model TEXT",
            "ALTER TABLE images ADD COLUMN iso INTEGER",
            "ALTER TABLE images ADD COLUMN aperture REAL",
            "ALTER TABLE images ADD COLUMN shutter_speed REAL",
            "ALTER TABLE images ADD COLUMN focal_length REAL",
            "ALTER TABLE images ADD COLUMN exposure_bias REAL",
            "ALTER TABLE images ADD COLUMN metering_mode TEXT",
            "ALTER TABLE images ADD COLUMN flash TEXT",
            "ALTER TABLE images ADD COLUMN width INTEGER",
            "ALTER TABLE images ADD COLUMN height INTEGER",
            "ALTER TABLE images ADD COLUMN latitude REAL",
            "ALTER TABLE images ADD COLUMN longitude REAL",
            "ALTER TABLE images ADD COLUMN altitude REAL",
            "ALTER TABLE images ADD COLUMN captured_at TEXT",
            "ALTER TABLE images ADD COLUMN copyright TEXT",
            "ALTER TABLE images ADD COLUMN contact TEXT",
            "ALTER TABLE image_versions ADD COLUMN caption TEXT",
            "ALTER TABLE images ADD COLUMN import_batch INTEGER",
            // M5 Slice 6 (face detection wiring): a real gap found while
            // implementing `reassign_face`'s "not a face" action
            // (RFC-0005 §3.6) -- `person_id IS NULL` alone can't
            // distinguish "detected, not yet clustered" (should still be
            // considered by clustering) from "confirmed not a face"
            // (must never be, but the detection row is kept rather than
            // deleted, same as the schema's own comment already promised).
            "ALTER TABLE faces ADD COLUMN excluded INTEGER NOT NULL DEFAULT 0",
            // Face detection UX fix (2026-09-16): see this column's own
            // CREATE TABLE comment above -- existing catalogs (every one
            // imported before this column existed) need it backfilled to
            // 0 (SQLite's own NOT NULL DEFAULT already does this for the
            // ADD COLUMN itself), which correctly reads as "not yet
            // scanned" and is what makes per-photo/per-folder on-demand
            // detection below actually run for them.
            "ALTER TABLE images ADD COLUMN faces_scanned INTEGER NOT NULL DEFAULT 0",
        ] {
            add_column_if_missing(conn, ddl)?;
        }

        Ok(())
    }
}

/// Runs a single `ALTER TABLE ... ADD COLUMN` and treats "duplicate column
/// name" (SQLite's error when the column is already there) as success --
/// the poor-man's migration primitive `migrate()` uses for every column
/// added after the initial `CREATE TABLE` text, so re-running it against an
/// already-migrated catalog is a harmless no-op. Any other SQLite error
/// (a real schema problem) still propagates.
fn add_column_if_missing(conn: &Connection, ddl: &str) -> Result<()> {
    match conn.execute(ddl, []) {
        Ok(_) => Ok(()),
        Err(rusqlite::Error::SqliteFailure(_, Some(msg))) if msg.contains("duplicate column name") => {
            Ok(())
        }
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression test for a real bug found while dogfooding this slice:
    /// this project's own dev catalog predates M2 Slice 2's new columns,
    /// and `CREATE TABLE IF NOT EXISTS` is a no-op against a table that
    /// already exists -- so opening `Catalog` against an existing
    /// pre-Slice-2 catalog file used to leave it permanently missing
    /// `camera_make`/`caption`/etc., and every metadata-bearing import
    /// against it failed with a real SQL error. Simulates that catalog
    /// shape by hand (the pre-Slice-2 `CREATE TABLE` text) with one
    /// pre-existing row, then confirms `migrate()` brings it up to date
    /// without losing the row, and that a metadata-bearing insert
    /// succeeds afterward.
    #[test]
    fn migrate_adds_new_columns_to_a_pre_existing_catalog_without_losing_data() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "
            CREATE TABLE images (
                id INTEGER PRIMARY KEY,
                path TEXT NOT NULL UNIQUE,
                content_hash TEXT,
                file_size INTEGER,
                thumbnail_path TEXT,
                stack_id INTEGER,
                added_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE TABLE image_versions (
                id INTEGER PRIMARY KEY,
                image_id INTEGER NOT NULL REFERENCES images(id) ON DELETE CASCADE,
                is_virtual_copy INTEGER NOT NULL DEFAULT 0,
                edit_stack_json TEXT NOT NULL,
                rating INTEGER NOT NULL DEFAULT 0,
                flag TEXT NOT NULL DEFAULT 'none',
                color_label TEXT NOT NULL DEFAULT 'none',
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            INSERT INTO images (path, content_hash, file_size) VALUES ('/pre-existing.CR3', 'pre-hash', 1024);
            INSERT INTO image_versions (image_id, edit_stack_json) VALUES (1, '{\"schema_version\":1,\"ops\":[]}');
            ",
        )
        .unwrap();

        Catalog::migrate(&conn, false).expect("migrate must succeed against a pre-Slice-2 catalog");
        let catalog = Catalog { conn };

        let images = catalog.list_images().unwrap();
        assert_eq!(images.len(), 1, "the pre-existing row must survive migration");
        assert_eq!(images[0].path, "/pre-existing.CR3");
        assert_eq!(images[0].camera_make, None, "new columns default to NULL on an existing row");

        catalog
            .add_image_with_edit_stack(
                "/new.CR3",
                "new-hash",
                2048,
                &EditStack::empty(),
                &crate::metadata::ImageMetadata {
                    camera_make: Some("Fujifilm".to_string()),
                    ..Default::default()
                },
            )
            .expect("a metadata-bearing insert must succeed after migration, not error on missing columns");
    }
}
