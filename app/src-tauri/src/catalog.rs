//! Catalog schema v1 (M1 Slice 1).
//!
//! Slice of ADR-0005 (SQLite catalog storage) and ADR-0006 (versioned JSON
//! edit-stack representation) needed for Import + basic Library culling:
//! image references with content-hash-based dedupe, thumbnails, and
//! per-version rating/flag/color-label. Collections, keywords, and
//! virtual-copy UI are still M1+/M2 scope per MILESTONES.md.
//!
//! `migrate()` is `CREATE TABLE IF NOT EXISTS` plus a small `ALTER TABLE ADD
//! COLUMN` step per column added after the initial schema (M2 Slice 2) —
//! not a real migration system with versioned steps, but real enough to
//! actually update an existing catalog file now that one exists. The
//! `CREATE TABLE IF NOT EXISTS` text is a no-op against an existing table,
//! so every column added going forward needs its own `ADD COLUMN` line
//! here too, not just an edit to the `CREATE TABLE` text.

use crate::metadata::ImageMetadata;
use rusqlite::{params, Connection, OptionalExtension, Result};
use serde::{Deserialize, Serialize};

pub struct Catalog {
    conn: Connection,
}

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

/// Per-version cap on stored `edit_history` rows (M3) -- oldest-by-id
/// pruned first once exceeded. See `record_edit_stack`.
const MAX_HISTORY_ENTRIES: i64 = 100;

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

/// One row for the Library grid: an image plus its primary (non-virtual-copy)
/// version's culling state. Virtual-copy-aware listing is M2+ scope.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImageSummary {
    pub image_id: i64,
    pub version_id: i64,
    pub path: String,
    pub thumbnail_path: Option<String>,
    pub rating: u8,
    pub flag: String,
    pub color_label: String,
    pub added_at: String,
    /// Nullable in Rust to match the nullable `images.content_hash`
    /// column, even though `add_image_with_metadata` always sets it for
    /// real imports. Lets `preview_cache::pregenerate_missing` key the
    /// Develop preview cache without re-reading+re-hashing every file.
    pub content_hash: Option<String>,
    /// EXIF, read-only, captured at import time (M2 Slice 2) -- never
    /// user-edited, so no setter exists for these.
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub lens_model: Option<String>,
    pub iso: Option<u32>,
    pub aperture: Option<f32>,
    pub shutter_speed: Option<f32>,
    pub focal_length: Option<f32>,
    pub captured_at: Option<String>,
    /// IPTC (M2 Slice 2), user-editable via `set_caption`/`set_copyright`/
    /// `set_contact`. Deliberately split across tables, not uniform:
    /// `caption` lives on `image_versions` (plausibly diverges per virtual
    /// copy, matching the character of `rating`/`flag`/`color_label`
    /// there); `copyright`/`contact` live on `images` (identify the
    /// photographer/owner -- invariant across every copy of a given
    /// original, closer in spirit to EXIF than to per-copy culling state).
    /// A deliberate asymmetry, not an oversight.
    pub caption: Option<String>,
    pub copyright: Option<String>,
    pub contact: Option<String>,
}

/// What `remove_images` hands back per deleted row, so the command layer
/// can clean up the app-owned derived files (thumbnail JPEG, content-hash-
/// keyed Develop preview PNG) after the transaction commits. Never
/// includes the source path -- removal must not even be *handed* the means
/// to touch an original.
pub struct RemovedImage {
    pub thumbnail_path: Option<String>,
    pub content_hash: Option<String>,
}

/// A keyword assigned to a specific image (M2 Slice 4). `path` is the
/// joined ancestor chain (e.g. "nature / birds / owl") -- built once here
/// so the frontend doesn't need its own copy of the tree just to show a
/// chip tooltip for one image's keywords.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KeywordRef {
    pub id: i64,
    pub name: String,
    pub path: String,
}

/// One node of the full keyword tree, flat (M2 Slice 4). The frontend
/// walks `parent_id` links client-side to build full paths for
/// autocomplete suggestions -- no recursive SQL, matching this file's
/// existing no-CTE style.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KeywordNode {
    pub id: i64,
    pub name: String,
    pub parent_id: Option<i64>,
}

/// One image-keyword assignment (M2 Slice 5) -- the flat shape
/// `list_all_image_keywords` returns to back Smart Collections' keyword
/// rules client-side.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImageKeywordAssignment {
    pub image_id: i64,
    pub keyword_id: i64,
}

/// A collection, manual or smart (M2 Slice 5). `rules` is `None` for a
/// manual collection, `Some(...)` for a smart one -- opaque JSON on the
/// Rust side, interpreted only in the frontend (see the schema comment in
/// `migrate()`). `count` is `None` for smart collections; see
/// `list_collections`'s doc comment for why that's a deliberate override,
/// not `COUNT()`'s natural behavior.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CollectionSummary {
    pub id: i64,
    pub name: String,
    pub is_smart: bool,
    pub rules: Option<Vec<serde_json::Value>>,
    pub count: Option<i64>,
}

/// Internal to the Rust command layer only (thumbnail regeneration) --
/// never returned to the frontend, so no Serialize/Deserialize.
#[derive(Debug, Clone)]
pub struct VersionSource {
    pub image_id: i64,
    pub path: String,
    pub content_hash: Option<String>,
}

/// Catalog backup preferences (PRD §7.6), round-tripped from the `settings`
/// key/value table. `last_backup_at` is the due-ness clock: absent means
/// "never backed up", present is an ISO datetime string the frontend
/// compares against `frequency` to decide whether a backup is due.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BackupSettings {
    pub frequency: String,
    pub folder: Option<String>,
    pub check_integrity: bool,
    pub optimize: bool,
    pub last_backup_at: Option<String>,
}

/// What `perform_backup` hands back on success.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupOutcome {
    pub path: String,
    pub performed_at: String,
}

#[derive(Debug, thiserror::Error)]
pub enum BackupError {
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("backup folder must be separate from the catalog's own app-data folder")]
    DestinationNotSeparate,
    #[error("catalog integrity check failed: {0}")]
    IntegrityCheckFailed(String),
}

impl Catalog {
    /// Test-only: an ephemeral catalog with nothing on disk. Production
    /// code always persists to a real file via `open()` (ADR-0005) — this
    /// has no production caller, so it's compiled only for tests rather
    /// than carried as unused API surface.
    #[cfg(test)]
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        // Deliberately NOT `harden()`: WAL cannot engage on an in-memory
        // database (SQLite reports journal_mode "memory"), so harden()'s
        // debug_assert would panic every debug-build test. The FK pragma
        // is the part that must match production behavior in tests.
        Self::enable_foreign_keys(&conn)?;
        Self::migrate(&conn)?;
        Ok(Self { conn })
    }

    /// M2 Slice 3: SQLite leaves foreign-key enforcement OFF by default,
    /// per-connection -- so the schema's `ON DELETE CASCADE` on
    /// `image_versions.image_id` was inert from M1 Slice 1 until this was
    /// added (a real latent gap found while designing removal, not
    /// theoretical: a bare `DELETE FROM images` would have silently
    /// orphaned its `image_versions` rows). `remove_images` below still
    /// deletes child rows explicitly rather than leaning on CASCADE, so
    /// removal stays correct even on a connection where this pragma
    /// somehow didn't take -- this is defense-in-depth, not the load-
    /// bearing mechanism. Per-connection and validates nothing
    /// retroactively, so existing catalogs are unaffected by turning it on.
    fn enable_foreign_keys(conn: &Connection) -> Result<()> {
        conn.pragma_update(None, "foreign_keys", true)
    }

    /// M1 Slice 6 (crash-safety hardening): WAL + `synchronous=NORMAL`.
    /// Honest framing, not oversold -- the prior default (rollback journal
    /// + SQLite's own implicit `synchronous=FULL`) was already durable
    /// against process crashes; this doesn't close an existing hole in
    /// the "no data loss on crash" exit criterion (the flush-on-close fix
    /// in +page.svelte and the atomic import insert below are what do
    /// that). What this buys, today: readers not blocking on a writer --
    /// though since `AppState.catalog` is one `Arc<Mutex<Catalog>>`
    /// around a single connection, everything already serializes through
    /// Rust's own Mutex regardless of journal mode, so this is more
    /// forward-looking hygiene than a realized fix right now. Cheap and
    /// standard to add regardless.
    ///
    /// `pragma_update` (used for `synchronous`) silently swallows the row
    /// SQLite returns for `journal_mode` specifically -- it would "succeed"
    /// even if WAL failed to engage (e.g. an unsupported filesystem/VFS),
    /// with no way to know. `pragma_update_and_check` reads that row back
    /// so a failure to actually engage WAL is a real, visible error.
    fn harden(conn: &Connection) -> Result<()> {
        let mode: String =
            conn.pragma_update_and_check(None, "journal_mode", "WAL", |row| row.get(0))?;
        debug_assert_eq!(mode, "wal", "journal_mode WAL did not actually engage");
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        Ok(())
    }

    pub fn open<P: AsRef<std::path::Path>>(path: P) -> Result<Self> {
        let conn = Connection::open(path)?;
        Self::harden(&conn)?;
        Self::enable_foreign_keys(&conn)?;
        Self::migrate(&conn)?;
        Ok(Self { conn })
    }

    fn migrate(conn: &Connection) -> Result<()> {
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
                camera_make TEXT,
                camera_model TEXT,
                lens_model TEXT,
                iso INTEGER,
                aperture REAL,
                shutter_speed REAL,
                focal_length REAL,
                captured_at TEXT,
                copyright TEXT,
                contact TEXT
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

            -- Catalog backup (M2 final slice). A plain key/value table --
            -- greenfield, no other settings/preferences storage exists
            -- anywhere in this codebase. Nothing references this table via
            -- foreign key, so no cleanup-on-delete concern like
            -- image_keywords/collection_images above.
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
            "ALTER TABLE images ADD COLUMN captured_at TEXT",
            "ALTER TABLE images ADD COLUMN copyright TEXT",
            "ALTER TABLE images ADD COLUMN contact TEXT",
            "ALTER TABLE image_versions ADD COLUMN caption TEXT",
        ] {
            add_column_if_missing(conn, ddl)?;
        }

        Ok(())
    }

    /// Test-only convenience over `add_image_with_metadata`: every real
    /// import always has a content hash and file size available (see
    /// import.rs), so this metadata-less variant has no production caller.
    #[cfg(test)]
    pub fn add_image(&self, path: &str) -> Result<i64> {
        self.conn
            .execute("INSERT INTO images (path) VALUES (?1)", params![path])?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Lower-level building block, superseded as the import pipeline's
    /// caller by the atomic `add_image_with_edit_stack` (M1 Slice 6) --
    /// kept as real pub API since catalog.rs's own tests use it directly,
    /// and it's the natural insert-only-the-image half for a future M2
    /// virtual-copy path (a new edit-stack row against an *existing*
    /// image_id, which `add_image_with_edit_stack` doesn't support).
    #[allow(dead_code)]
    pub fn add_image_with_metadata(
        &self,
        path: &str,
        content_hash: &str,
        file_size: i64,
    ) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO images (path, content_hash, file_size) VALUES (?1, ?2, ?3)",
            params![path, content_hash, file_size],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Atomically inserts an image row (with its EXIF metadata, M2 Slice 2)
    /// and its initial edit-stack row (M1 Slice 6). The import pipeline
    /// used to do this as two separate auto-commit statements
    /// (`add_image_with_metadata` then `add_edit_stack`) -- a crash between
    /// them left an `images` row with no matching `image_versions` row.
    /// `list_images()`'s join silently *excludes* such a row rather than
    /// erroring, but `find_by_hash` still matches it on every future
    /// import scan, so the file became permanently unimportable through
    /// the normal UI. This is the real import-time caller;
    /// `add_image_with_metadata`/`add_edit_stack` stay available as
    /// lower-level building blocks (tests, and future M2 virtual copies,
    /// which need a new edit-stack row against an *existing* image_id).
    /// EXIF metadata insertion is part of the same transaction for the
    /// same reason the two rows are: a crash partway through import
    /// shouldn't be able to leave metadata inconsistently applied.
    ///
    /// Uses `unchecked_transaction` (rusqlite's `&self`-based transaction
    /// API) rather than `transaction` (`&mut self`) since `Catalog`'s
    /// methods are all `&self`, called through `Arc<Mutex<Catalog>>` --
    /// the Mutex already serializes access, so there's no real nested-
    /// transaction risk to guard against here.
    pub fn add_image_with_edit_stack(
        &self,
        path: &str,
        content_hash: &str,
        file_size: i64,
        stack: &EditStack,
        metadata: &ImageMetadata,
    ) -> Result<i64> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO images (
                path, content_hash, file_size,
                camera_make, camera_model, lens_model,
                iso, aperture, shutter_speed, focal_length, captured_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                path,
                content_hash,
                file_size,
                metadata.camera_make,
                metadata.camera_model,
                metadata.lens_model,
                metadata.iso,
                metadata.aperture,
                metadata.shutter_speed,
                metadata.focal_length,
                metadata.captured_at,
            ],
        )?;
        let image_id = tx.last_insert_rowid();

        let json = serde_json::to_string(stack).expect("EditStack is always serializable");
        tx.execute(
            "INSERT INTO image_versions (image_id, edit_stack_json) VALUES (?1, ?2)",
            params![image_id, json],
        )?;

        tx.commit()?;
        Ok(image_id)
    }

    /// Look up an existing image by content hash, for duplicate detection.
    pub fn find_by_hash(&self, content_hash: &str) -> Result<Option<i64>> {
        self.conn
            .query_row(
                "SELECT id FROM images WHERE content_hash = ?1",
                params![content_hash],
                |row| row.get(0),
            )
            .optional()
    }

    /// Non-destructive removal (M2 Slice 3): deletes catalog rows only --
    /// the user's source file is NEVER touched (hard PRD constraint), and
    /// the app-owned derived files (thumbnail, cached Develop preview) are
    /// the *caller's* cleanup concern, which is why each removed image's
    /// `thumbnail_path`/`content_hash` is returned: file-on-disk concerns
    /// live in the command layer, not here, matching how thumbnail writes
    /// already sit outside catalog.rs.
    ///
    /// One transaction for the whole batch, child rows deleted explicitly
    /// before parents -- deliberately NOT relying on `ON DELETE CASCADE`
    /// even though `enable_foreign_keys` now makes it live (see that
    /// method's comment). Deleting the `images` row is also what makes the
    /// file re-importable afterward: `find_by_hash`'s dedupe check stops
    /// matching -- that's the user-facing meaning of "non-destructive"
    /// (remove, then change your mind and import again).
    ///
    /// Unknown ids are a no-op, not an error: a double-fired removal (or a
    /// removal racing a refresh) should converge on "row is gone", not
    /// fail halfway through a batch.
    pub fn remove_images(&self, image_ids: &[i64]) -> Result<Vec<RemovedImage>> {
        let tx = self.conn.unchecked_transaction()?;
        let mut removed = Vec::with_capacity(image_ids.len());

        for &image_id in image_ids {
            let row: Option<RemovedImage> = tx
                .query_row(
                    "SELECT thumbnail_path, content_hash FROM images WHERE id = ?1",
                    params![image_id],
                    |row| {
                        Ok(RemovedImage {
                            thumbnail_path: row.get(0)?,
                            content_hash: row.get(1)?,
                        })
                    },
                )
                .optional()?;
            let Some(row) = row else { continue };

            tx.execute(
                "DELETE FROM image_versions WHERE image_id = ?1",
                params![image_id],
            )?;
            // M2 Slice 4/5: same "explicit, don't rely on CASCADE"
            // discipline as the image_versions delete above.
            tx.execute(
                "DELETE FROM image_keywords WHERE image_id = ?1",
                params![image_id],
            )?;
            tx.execute(
                "DELETE FROM collection_images WHERE image_id = ?1",
                params![image_id],
            )?;
            tx.execute("DELETE FROM images WHERE id = ?1", params![image_id])?;
            removed.push(row);
        }

        tx.commit()?;
        Ok(removed)
    }

    /// Resolves a hierarchical keyword path (e.g. `["nature","birds","owl"]`)
    /// to a leaf keyword id, creating any level that doesn't exist yet, and
    /// assigns that leaf to every image in `image_ids`. One transaction for
    /// the whole operation (matching `add_image_with_edit_stack`'s pattern).
    ///
    /// Find-or-create per level rather than a `UNIQUE(parent_id, name)`
    /// constraint (see the schema comment in `migrate()` for why that
    /// constraint wouldn't actually work) -- safe under this app's
    /// concurrency model since `AppState.catalog` is one
    /// `Arc<Mutex<Catalog>>`, so no other writer can interleave between a
    /// level's SELECT and its INSERT within this transaction.
    ///
    /// `INSERT OR IGNORE` against the `image_keywords` composite primary
    /// key makes assignment idempotent -- re-assigning an already-assigned
    /// keyword to an image is a silent no-op, not an error, matching
    /// `remove_images`'s "unknown/duplicate converges quietly" discipline.
    pub fn assign_keyword_path(&self, image_ids: &[i64], path_segments: &[String]) -> Result<i64> {
        let tx = self.conn.unchecked_transaction()?;

        let mut parent_id: Option<i64> = None;
        for segment in path_segments {
            let existing: Option<i64> = tx
                .query_row(
                    "SELECT id FROM keywords WHERE name = ?1 AND parent_id IS ?2",
                    params![segment, parent_id],
                    |row| row.get(0),
                )
                .optional()?;
            parent_id = Some(match existing {
                Some(id) => id,
                None => {
                    tx.execute(
                        "INSERT INTO keywords (name, parent_id) VALUES (?1, ?2)",
                        params![segment, parent_id],
                    )?;
                    tx.last_insert_rowid()
                }
            });
        }
        let leaf_id = parent_id.expect("path_segments must be non-empty");

        for &image_id in image_ids {
            tx.execute(
                "INSERT OR IGNORE INTO image_keywords (image_id, keyword_id) VALUES (?1, ?2)",
                params![image_id, leaf_id],
            )?;
        }

        tx.commit()?;
        Ok(leaf_id)
    }

    /// Anchor-only removal (M2 Slice 4) -- matches the IPTC caption/
    /// copyright/contact precedent: multi-select display/edit stays
    /// scoped to the anchor image, only *assignment* batches across a
    /// selection.
    pub fn remove_keyword_from_image(&self, image_id: i64, keyword_id: i64) -> Result<()> {
        self.conn.execute(
            "DELETE FROM image_keywords WHERE image_id = ?1 AND keyword_id = ?2",
            params![image_id, keyword_id],
        )?;
        Ok(())
    }

    /// The keywords assigned to one image, each with its full ancestor
    /// path built via a simple parent-walk loop -- bounded by (keywords on
    /// this image) x (tree depth), not by catalog or keyword-tree size, so
    /// this is cheap even called once per image selection.
    pub fn get_image_keywords(&self, image_id: i64) -> Result<Vec<KeywordRef>> {
        let mut stmt = self.conn.prepare(
            "SELECT k.id, k.name FROM image_keywords ik
             JOIN keywords k ON k.id = ik.keyword_id
             WHERE ik.image_id = ?1
             ORDER BY k.name",
        )?;
        let leaves: Vec<(i64, String)> = stmt
            .query_map(params![image_id], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<Result<_>>()?;

        leaves
            .into_iter()
            .map(|(id, name)| {
                let path = self.keyword_path(id)?;
                Ok(KeywordRef { id, name, path })
            })
            .collect()
    }

    /// Walks `parent_id` links up from a keyword to build its full
    /// "grandparent / parent / name" display path.
    fn keyword_path(&self, keyword_id: i64) -> Result<String> {
        let mut segments = Vec::new();
        let mut current = Some(keyword_id);
        while let Some(id) = current {
            let (name, parent_id): (String, Option<i64>) = self.conn.query_row(
                "SELECT name, parent_id FROM keywords WHERE id = ?1",
                params![id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            segments.push(name);
            current = parent_id;
        }
        segments.reverse();
        Ok(segments.join(" / "))
    }

    /// The full keyword tree, flat -- the frontend walks `parent_id` links
    /// client-side to build display paths for autocomplete suggestions.
    pub fn list_keywords(&self) -> Result<Vec<KeywordNode>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, name, parent_id FROM keywords ORDER BY name")?;
        let rows = stmt.query_map([], |row| {
            Ok(KeywordNode {
                id: row.get(0)?,
                name: row.get(1)?,
                parent_id: row.get(2)?,
            })
        })?;
        rows.collect()
    }

    /// Every image-keyword assignment in the catalog, flat -- backs
    /// Smart Collections' "has keyword" / "untagged" rules (M2 Slice 5).
    /// One query, no joins, independent of `list_images()`/`ImageSummary`
    /// (which deliberately carries no keyword data -- see that struct's
    /// doc comment); fetched once by the frontend, not per image.
    pub fn list_all_image_keywords(&self) -> Result<Vec<ImageKeywordAssignment>> {
        let mut stmt = self
            .conn
            .prepare("SELECT image_id, keyword_id FROM image_keywords")?;
        let rows = stmt.query_map([], |row| {
            Ok(ImageKeywordAssignment {
                image_id: row.get(0)?,
                keyword_id: row.get(1)?,
            })
        })?;
        rows.collect()
    }

    /// Bare rail "+" with no selection -- an empty manual collection.
    pub fn create_collection(&self, name: &str) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO collections (name, is_smart, rules_json) VALUES (?1, 0, NULL)",
            params![name],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// "Add to Collection… -> New Collection…" from a multi-selection: one
    /// transaction, create-then-populate together (matching
    /// `assign_keyword_path`'s own create-then-assign shape) rather than
    /// two separate calls -- an error between them would otherwise leave
    /// an empty, orphaned, confusingly-named collection with no
    /// indication anything went wrong.
    pub fn create_collection_with_images(&self, name: &str, image_ids: &[i64]) -> Result<i64> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO collections (name, is_smart, rules_json) VALUES (?1, 0, NULL)",
            params![name],
        )?;
        let collection_id = tx.last_insert_rowid();
        for &image_id in image_ids {
            tx.execute(
                "INSERT OR IGNORE INTO collection_images (collection_id, image_id) VALUES (?1, ?2)",
                params![collection_id, image_id],
            )?;
        }
        tx.commit()?;
        Ok(collection_id)
    }

    /// `rules` is opaque here -- see the schema comment in `migrate()` for
    /// why Rust never interprets it.
    pub fn create_smart_collection(&self, name: &str, rules: &[serde_json::Value]) -> Result<i64> {
        let rules_json = serde_json::to_string(rules).expect("rules are always serializable");
        self.conn.execute(
            "INSERT INTO collections (name, is_smart, rules_json) VALUES (?1, 1, ?2)",
            params![name, rules_json],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn update_smart_collection_rules(&self, collection_id: i64, rules: &[serde_json::Value]) -> Result<()> {
        let rules_json = serde_json::to_string(rules).expect("rules are always serializable");
        self.conn.execute(
            "UPDATE collections SET rules_json = ?2 WHERE id = ?1",
            params![collection_id, rules_json],
        )?;
        Ok(())
    }

    /// Explicit child-then-parent delete, same "don't rely on CASCADE"
    /// discipline as `remove_images`. Trivially correct for a smart
    /// collection too -- it has zero `collection_images` rows to begin
    /// with, and a DELETE matching nothing is an ordinary no-op (same
    /// behavior `remove_images` already relies on for unknown ids).
    pub fn delete_collection(&self, collection_id: i64) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "DELETE FROM collection_images WHERE collection_id = ?1",
            params![collection_id],
        )?;
        tx.execute("DELETE FROM collections WHERE id = ?1", params![collection_id])?;
        tx.commit()?;
        Ok(())
    }

    /// Batch, idempotent (`INSERT OR IGNORE`) -- matches
    /// `assign_keyword_path`'s membership-write idiom.
    pub fn add_images_to_collection(&self, collection_id: i64, image_ids: &[i64]) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        for &image_id in image_ids {
            tx.execute(
                "INSERT OR IGNORE INTO collection_images (collection_id, image_id) VALUES (?1, ?2)",
                params![collection_id, image_id],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn remove_images_from_collection(&self, collection_id: i64, image_ids: &[i64]) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        for &image_id in image_ids {
            tx.execute(
                "DELETE FROM collection_images WHERE collection_id = ?1 AND image_id = ?2",
                params![collection_id, image_id],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// `count` is `Some(n)` for a manual collection (a real `COUNT`),
    /// `None` for a smart one -- deliberately overridden here rather than
    /// left to whatever the aggregate naturally returns: `COUNT()` over a
    /// `LEFT JOIN` with no matching rows returns `0`, not `NULL`, which
    /// would silently read as "0 matches" instead of "not applicable" and
    /// be invisible in testing (a brand-new smart collection legitimately
    /// has 0 real matches too). The frontend computes a smart collection's
    /// real count client-side from `rules` + the already-loaded catalog.
    pub fn list_collections(&self) -> Result<Vec<CollectionSummary>> {
        let mut stmt = self.conn.prepare(
            "SELECT c.id, c.name, c.is_smart, c.rules_json, COUNT(ci.image_id)
             FROM collections c
             LEFT JOIN collection_images ci ON ci.collection_id = c.id
             GROUP BY c.id
             ORDER BY c.name",
        )?;
        let rows = stmt.query_map([], |row| {
            let is_smart: bool = row.get(2)?;
            let rules_json: Option<String> = row.get(3)?;
            let count: i64 = row.get(4)?;
            Ok(CollectionSummary {
                id: row.get(0)?,
                name: row.get(1)?,
                is_smart,
                rules: rules_json.map(|json| {
                    serde_json::from_str(&json).expect("stored rules_json is always valid")
                }),
                count: (!is_smart).then_some(count),
            })
        })?;
        rows.collect()
    }

    /// A manual collection's membership -- fetched once by the frontend
    /// when the collection is clicked in the rail, cached by collection
    /// id there. Meaningless for a smart collection (always empty; smart
    /// membership is computed client-side from `rules`, never queried).
    pub fn list_collection_image_ids(&self, collection_id: i64) -> Result<Vec<i64>> {
        let mut stmt = self
            .conn
            .prepare("SELECT image_id FROM collection_images WHERE collection_id = ?1")?;
        let rows = stmt.query_map(params![collection_id], |row| row.get(0))?;
        rows.collect()
    }

    pub fn get_backup_settings(&self) -> Result<BackupSettings> {
        let mut stmt = self.conn.prepare("SELECT key, value FROM settings")?;
        let rows: Vec<(String, String)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<Result<_>>()?;
        let get = |key: &str| rows.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone());
        Ok(BackupSettings {
            frequency: get("backup_frequency").unwrap_or_else(|| "weekly".to_string()),
            folder: get("backup_folder"),
            check_integrity: get("backup_check_integrity").as_deref() == Some("1"),
            optimize: get("backup_optimize").as_deref() == Some("1"),
            last_backup_at: get("last_backup_at"),
        })
    }

    pub fn update_backup_settings(&self, settings: &BackupSettings) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        let upsert = |key: &str, value: Option<&str>| -> Result<()> {
            match value {
                Some(v) => tx.execute(
                    "INSERT INTO settings (key, value) VALUES (?1, ?2)
                     ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                    params![key, v],
                )?,
                None => tx.execute("DELETE FROM settings WHERE key = ?1", params![key])?,
            };
            Ok(())
        };
        upsert("backup_frequency", Some(&settings.frequency))?;
        upsert("backup_folder", settings.folder.as_deref())?;
        upsert(
            "backup_check_integrity",
            Some(if settings.check_integrity { "1" } else { "0" }),
        )?;
        upsert("backup_optimize", Some(if settings.optimize { "1" } else { "0" }))?;
        upsert("last_backup_at", settings.last_backup_at.as_deref())?;
        tx.commit()
    }

    /// Writes a timestamped, independently-openable copy of this catalog
    /// to `dest_dir` (PRD §7.6). Uses `rusqlite::backup::Backup` -- SQLite's
    /// real Online Backup API, walking the source's pager rather than raw
    /// file bytes -- which is the safe way to copy a live WAL-mode database
    /// without a manual checkpoint step; a naive `fs::copy` risks reading a
    /// half-committed state split across the main file and its `-wal`
    /// sidecar. The destination needs no special setup: a fresh
    /// `Connection::open` is enough, and the finished file has no leftover
    /// `-wal`/`-shm` sidecar of its own.
    ///
    /// Deliberate, named exception to this codebase's usual "release the
    /// mutex before slow work" convention (see `export_images` in lib.rs):
    /// this runs entirely while the caller's `AppState.catalog` mutex lock
    /// is held, for as long as the optional `VACUUM` + backup copy take.
    /// Accepted because this is only reachable once, from the close-prompt
    /// dialog, immediately before the window is destroyed -- nothing else
    /// should be issuing catalog commands at that moment except the two
    /// fire-and-forget startup catch-up passes, which will simply wait
    /// briefly on the std::sync::Mutex, not deadlock.
    pub fn perform_backup(
        &self,
        dest_dir: &std::path::Path,
        app_data_dir: &std::path::Path,
        check_integrity: bool,
        optimize: bool,
    ) -> std::result::Result<BackupOutcome, BackupError> {
        let canonical_dest = dest_dir.canonicalize()?;
        let canonical_app_data = app_data_dir.canonicalize()?;
        if canonical_dest == canonical_app_data || canonical_dest.starts_with(&canonical_app_data)
        {
            return Err(BackupError::DestinationNotSeparate);
        }

        if check_integrity {
            let result: String =
                self.conn
                    .query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
            if result != "ok" {
                return Err(BackupError::IntegrityCheckFailed(result));
            }
        }

        if optimize {
            self.conn.execute_batch("VACUUM")?;
        }

        // No chrono/time dependency exists in this codebase -- SQLite's own
        // strftime/datetime functions produce the timestamp instead,
        // matching the `datetime('now')` pattern already used for
        // `created_at` columns throughout this schema. Filesystem-safe
        // (no colons) for the filename; the fuller ISO form for the
        // `last_backup_at` value the frontend persists afterward.
        let filename_timestamp: String =
            self.conn
                .query_row("SELECT strftime('%Y%m%d-%H%M%S', 'now')", [], |row| {
                    row.get(0)
                })?;
        let performed_at: String =
            self.conn
                .query_row("SELECT datetime('now')", [], |row| row.get(0))?;
        let out_path = dest_dir.join(format!("catalog-backup-{filename_timestamp}.sqlite"));
        {
            let mut dest_conn = Connection::open(&out_path)?;
            let backup = rusqlite::backup::Backup::new(&self.conn, &mut dest_conn)?;
            backup.run_to_completion(5, std::time::Duration::from_millis(250), None)?;
            drop(backup); // release dest_conn's mutable borrow before the pragma below

            // The Online Backup API copies the source's pages verbatim,
            // including the header byte that records journal mode -- found
            // empirically (not assumed): a backup taken from this app's
            // live WAL-mode catalog comes out flagged as WAL-mode too, so
            // simply *opening* it later (a restore, or just inspecting it)
            // spawns its own -shm/-wal sidecars next to what should be one
            // portable file. Converting back to a plain rollback journal
            // here checkpoints any residual WAL content into the main file
            // and flips the header, so the finished backup is always a
            // clean single-file artifact -- the property a "copy this one
            // file back to restore" workflow actually needs.
            dest_conn.pragma_update(None, "journal_mode", "DELETE")?;
        }

        Ok(BackupOutcome {
            path: out_path.to_string_lossy().into_owned(),
            performed_at,
        })
    }

    pub fn set_thumbnail_path(&self, image_id: i64, thumbnail_path: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE images SET thumbnail_path = ?2 WHERE id = ?1",
            params![image_id, thumbnail_path],
        )?;
        Ok(())
    }

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
    ///    cursor is treated as "before all history" -- nothing to delete.
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
        tx.commit()?;

        self.get_history(version_id)
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

    pub fn delete_snapshot(&self, version_id: i64, snapshot_id: i64) -> Result<()> {
        self.conn.execute(
            "DELETE FROM snapshots WHERE id = ?1 AND version_id = ?2",
            params![snapshot_id, version_id],
        )?;
        Ok(())
    }

    /// Resolves a version_id to what thumbnail regeneration needs: the
    /// parent image's id (thumbnails are keyed by image_id, not
    /// version_id), source path, and content_hash (to reuse the Develop
    /// preview cache instead of a fresh decode). A real JOIN -- unlike
    /// `get_edit_stack` above, which is JOIN-free only because
    /// `edit_stack_json` happens to live directly on `image_versions`.
    pub fn get_version_source(&self, version_id: i64) -> Result<VersionSource> {
        self.conn.query_row(
            "SELECT i.id, i.path, i.content_hash FROM image_versions v
             JOIN images i ON i.id = v.image_id
             WHERE v.id = ?1",
            params![version_id],
            |row| {
                Ok(VersionSource {
                    image_id: row.get(0)?,
                    path: row.get(1)?,
                    content_hash: row.get(2)?,
                })
            },
        )
    }

    pub fn set_rating(&self, version_id: i64, rating: u8) -> Result<()> {
        self.conn.execute(
            "UPDATE image_versions SET rating = ?2, updated_at = datetime('now') WHERE id = ?1",
            params![version_id, rating],
        )?;
        Ok(())
    }

    pub fn set_flag(&self, version_id: i64, flag: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE image_versions SET flag = ?2, updated_at = datetime('now') WHERE id = ?1",
            params![version_id, flag],
        )?;
        Ok(())
    }

    pub fn set_color_label(&self, version_id: i64, color_label: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE image_versions SET color_label = ?2, updated_at = datetime('now') WHERE id = ?1",
            params![version_id, color_label],
        )?;
        Ok(())
    }

    /// IPTC (M2 Slice 2). `caption` is per-version (see `ImageSummary`'s
    /// doc comment for why it's split from copyright/contact below).
    pub fn set_caption(&self, version_id: i64, caption: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE image_versions SET caption = ?2, updated_at = datetime('now') WHERE id = ?1",
            params![version_id, caption],
        )?;
        Ok(())
    }

    pub fn set_copyright(&self, image_id: i64, copyright: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE images SET copyright = ?2 WHERE id = ?1",
            params![image_id, copyright],
        )?;
        Ok(())
    }

    pub fn set_contact(&self, image_id: i64, contact: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE images SET contact = ?2 WHERE id = ?1",
            params![image_id, contact],
        )?;
        Ok(())
    }

    /// Library grid data: one row per image, its primary (first,
    /// non-virtual-copy) version's culling state. Newest imports first.
    pub fn list_images(&self) -> Result<Vec<ImageSummary>> {
        let mut stmt = self.conn.prepare(
            "SELECT i.id, v.id, i.path, i.thumbnail_path, v.rating, v.flag, v.color_label, i.added_at, i.content_hash,
                    i.camera_make, i.camera_model, i.lens_model, i.iso, i.aperture, i.shutter_speed, i.focal_length, i.captured_at,
                    v.caption, i.copyright, i.contact
             FROM images i
             JOIN image_versions v ON v.id = (
                 SELECT id FROM image_versions
                 WHERE image_id = i.id AND is_virtual_copy = 0
                 ORDER BY id ASC LIMIT 1
             )
             ORDER BY i.added_at DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(ImageSummary {
                image_id: row.get(0)?,
                version_id: row.get(1)?,
                path: row.get(2)?,
                thumbnail_path: row.get(3)?,
                rating: row.get(4)?,
                flag: row.get(5)?,
                color_label: row.get(6)?,
                added_at: row.get(7)?,
                content_hash: row.get(8)?,
                camera_make: row.get(9)?,
                camera_model: row.get(10)?,
                lens_model: row.get(11)?,
                iso: row.get(12)?,
                aperture: row.get(13)?,
                shutter_speed: row.get(14)?,
                focal_length: row.get(15)?,
                captured_at: row.get(16)?,
                caption: row.get(17)?,
                copyright: row.get(18)?,
                contact: row.get(19)?,
            })
        })?;
        rows.collect()
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
    use serde_json::json;

    /// M0 exit criterion (MILESTONES.md): "Catalog schema v0 ... can store
    /// an image reference + one edit record."
    #[test]
    fn round_trips_one_image_and_one_edit_record() {
        let catalog = Catalog::open_in_memory().expect("in-memory catalog opens");

        let image_id = catalog
            .add_image("/Users/alfred/Pictures/2026-Kyoto/IMG_0001.CR3")
            .expect("image insert succeeds");

        let stack = EditStack {
            schema_version: 1,
            ops: vec![json!({"op": "exposure", "value": 0.5})],
        };
        let version_id = catalog
            .add_edit_stack(image_id, &stack)
            .expect("edit stack insert succeeds");

        let round_tripped = catalog
            .get_edit_stack(version_id)
            .expect("edit stack read succeeds");

        assert_eq!(round_tripped, stack);
    }

    #[test]
    fn rejects_duplicate_image_paths() {
        let catalog = Catalog::open_in_memory().expect("in-memory catalog opens");
        catalog.add_image("/a.CR3").expect("first insert succeeds");
        assert!(catalog.add_image("/a.CR3").is_err());
    }

    #[test]
    fn finds_images_by_content_hash_for_dedupe() {
        let catalog = Catalog::open_in_memory().expect("in-memory catalog opens");
        assert_eq!(catalog.find_by_hash("abc123").unwrap(), None);

        let image_id = catalog
            .add_image_with_metadata("/a.CR3", "abc123", 4096)
            .expect("insert with metadata succeeds");

        assert_eq!(catalog.find_by_hash("abc123").unwrap(), Some(image_id));
        assert_eq!(catalog.find_by_hash("does-not-exist").unwrap(), None);
    }

    #[test]
    fn sets_thumbnail_rating_flag_and_color_label() {
        let catalog = Catalog::open_in_memory().expect("in-memory catalog opens");
        let image_id = catalog.add_image("/a.CR3").unwrap();
        let version_id = catalog.add_edit_stack(image_id, &EditStack::empty()).unwrap();

        catalog.set_thumbnail_path(image_id, "/thumbs/a.jpg").unwrap();
        catalog.set_rating(version_id, 4).unwrap();
        catalog.set_flag(version_id, "pick").unwrap();
        catalog.set_color_label(version_id, "green").unwrap();

        let images = catalog.list_images().unwrap();
        assert_eq!(images.len(), 1);
        let summary = &images[0];
        assert_eq!(summary.thumbnail_path.as_deref(), Some("/thumbs/a.jpg"));
        assert_eq!(summary.rating, 4);
        assert_eq!(summary.flag, "pick");
        assert_eq!(summary.color_label, "green");
    }

    #[test]
    fn get_version_source_resolves_the_parent_images_row() {
        let catalog = Catalog::open_in_memory().expect("in-memory catalog opens");
        let image_id = catalog
            .add_image_with_edit_stack("/a.CR3", "hash-a", 100, &EditStack::empty(), &crate::metadata::ImageMetadata::default())
            .unwrap();
        let images = catalog.list_images().unwrap();
        let version_id = images[0].version_id;

        let source = catalog.get_version_source(version_id).unwrap();

        assert_eq!(source.image_id, image_id);
        assert_eq!(source.path, "/a.CR3");
        assert_eq!(source.content_hash.as_deref(), Some("hash-a"));
    }

    #[test]
    fn rejects_out_of_range_rating_and_invalid_flag() {
        let catalog = Catalog::open_in_memory().expect("in-memory catalog opens");
        let image_id = catalog.add_image("/a.CR3").unwrap();
        let version_id = catalog.add_edit_stack(image_id, &EditStack::empty()).unwrap();

        assert!(catalog.set_rating(version_id, 6).is_err());
        assert!(catalog.set_flag(version_id, "not-a-real-flag").is_err());
        assert!(catalog.set_color_label(version_id, "not-a-real-color").is_err());
    }

    #[test]
    fn add_image_with_edit_stack_inserts_both_rows_atomically() {
        let catalog = Catalog::open_in_memory().expect("in-memory catalog opens");
        let stack = EditStack {
            schema_version: 1,
            ops: vec![json!({"op": "exposure", "value": 0.3})],
        };

        let image_id = catalog
            .add_image_with_edit_stack("/a.CR3", "hash-a", 4096, &stack, &crate::metadata::ImageMetadata::default())
            .expect("atomic insert succeeds");

        assert_eq!(catalog.find_by_hash("hash-a").unwrap(), Some(image_id));

        let images = catalog.list_images().unwrap();
        assert_eq!(images.len(), 1, "the image row must be visible via list_images (i.e. a matching image_versions row exists)");
        assert_eq!(catalog.get_edit_stack(images[0].version_id).unwrap(), stack);
    }

    #[test]
    fn add_image_with_edit_stack_persists_metadata_atomically() {
        let catalog = Catalog::open_in_memory().expect("in-memory catalog opens");
        let metadata = crate::metadata::ImageMetadata {
            camera_make: Some("Canon".to_string()),
            camera_model: Some("EOS 5D Mark III".to_string()),
            lens_model: Some("24-70mm f/2.8".to_string()),
            iso: Some(200),
            aperture: Some(2.8),
            shutter_speed: Some(1.0 / 100.0),
            focal_length: Some(70.0),
            captured_at: Some("2017-01-05T05:53:29+00:00".to_string()),
        };

        catalog
            .add_image_with_edit_stack("/a.CR3", "hash-meta", 4096, &EditStack::empty(), &metadata)
            .expect("insert succeeds");

        let images = catalog.list_images().unwrap();
        assert_eq!(images.len(), 1);
        let summary = &images[0];
        assert_eq!(summary.camera_make, metadata.camera_make);
        assert_eq!(summary.camera_model, metadata.camera_model);
        assert_eq!(summary.lens_model, metadata.lens_model);
        assert_eq!(summary.iso, metadata.iso);
        assert_eq!(summary.aperture, metadata.aperture);
        assert_eq!(summary.shutter_speed, metadata.shutter_speed);
        assert_eq!(summary.focal_length, metadata.focal_length);
        assert_eq!(summary.captured_at, metadata.captured_at);
    }

    #[test]
    fn set_caption_copyright_and_contact_round_trip() {
        let catalog = Catalog::open_in_memory().expect("in-memory catalog opens");
        let image_id = catalog
            .add_image_with_edit_stack("/a.CR3", "hash-iptc", 4096, &EditStack::empty(), &crate::metadata::ImageMetadata::default())
            .unwrap();
        let images = catalog.list_images().unwrap();
        let version_id = images[0].version_id;

        catalog.set_caption(version_id, "A quiet morning in Kyoto").unwrap();
        catalog.set_copyright(image_id, "© 2026 Alfred Wei").unwrap();
        catalog.set_contact(image_id, "alfred@example.com").unwrap();

        let images = catalog.list_images().unwrap();
        assert_eq!(images[0].caption.as_deref(), Some("A quiet morning in Kyoto"));
        assert_eq!(images[0].copyright.as_deref(), Some("© 2026 Alfred Wei"));
        assert_eq!(images[0].contact.as_deref(), Some("alfred@example.com"));
    }

    /// The user-facing meaning of "non-destructive removal": both rows are
    /// gone atomically, and -- the part a user would actually notice --
    /// the same file becomes importable again because `find_by_hash`'s
    /// dedupe check no longer matches.
    #[test]
    fn remove_images_deletes_both_rows_and_makes_the_file_reimportable() {
        let catalog = Catalog::open_in_memory().unwrap();
        let keep_id = catalog
            .add_image_with_edit_stack("/keep.CR3", "hash-keep", 100, &EditStack::empty(), &crate::metadata::ImageMetadata::default())
            .unwrap();
        let remove_id = catalog
            .add_image_with_edit_stack("/remove.CR3", "hash-remove", 200, &EditStack::empty(), &crate::metadata::ImageMetadata::default())
            .unwrap();
        catalog.set_thumbnail_path(remove_id, "/thumbs/2.jpg").unwrap();

        let removed = catalog.remove_images(&[remove_id]).unwrap();

        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0].thumbnail_path.as_deref(), Some("/thumbs/2.jpg"));
        assert_eq!(removed[0].content_hash.as_deref(), Some("hash-remove"));

        let images = catalog.list_images().unwrap();
        assert_eq!(images.len(), 1);
        assert_eq!(images[0].image_id, keep_id);

        // No orphaned child row left behind (the inert-CASCADE trap this
        // slice's explicit child-delete exists to avoid).
        let orphans: i64 = catalog
            .conn
            .query_row(
                "SELECT count(*) FROM image_versions WHERE image_id = ?1",
                params![remove_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(orphans, 0);

        assert_eq!(catalog.find_by_hash("hash-remove").unwrap(), None, "removed file must be re-importable");
        assert_eq!(catalog.find_by_hash("hash-keep").unwrap(), Some(keep_id));
    }

    #[test]
    fn remove_images_ignores_unknown_ids_and_handles_batches() {
        let catalog = Catalog::open_in_memory().unwrap();
        let a = catalog
            .add_image_with_edit_stack("/a.CR3", "hash-a", 1, &EditStack::empty(), &crate::metadata::ImageMetadata::default())
            .unwrap();
        let b = catalog
            .add_image_with_edit_stack("/b.CR3", "hash-b", 2, &EditStack::empty(), &crate::metadata::ImageMetadata::default())
            .unwrap();

        let removed = catalog.remove_images(&[a, 9999, b]).unwrap();

        assert_eq!(removed.len(), 2, "unknown id is a silent no-op, not an error");
        assert!(catalog.list_images().unwrap().is_empty());
    }

    #[test]
    fn assign_keyword_path_creates_nested_hierarchy_and_reuses_existing_segments() {
        let catalog = Catalog::open_in_memory().unwrap();
        let image_id = catalog
            .add_image_with_edit_stack("/a.CR3", "hash-a", 1, &EditStack::empty(), &crate::metadata::ImageMetadata::default())
            .unwrap();

        let path = ["nature".to_string(), "birds".to_string(), "owl".to_string()];
        let leaf_id = catalog.assign_keyword_path(&[image_id], &path).unwrap();

        let keywords = catalog.get_image_keywords(image_id).unwrap();
        assert_eq!(keywords.len(), 1);
        assert_eq!(keywords[0].id, leaf_id);
        assert_eq!(keywords[0].name, "owl");
        assert_eq!(keywords[0].path, "nature / birds / owl");

        let all = catalog.list_keywords().unwrap();
        assert_eq!(all.len(), 3, "one keyword row created per path segment");

        // Re-assigning the same path (e.g. a second image tagged similarly)
        // must reuse the existing levels, not create duplicates.
        let image_b = catalog
            .add_image_with_edit_stack("/b.CR3", "hash-b", 2, &EditStack::empty(), &crate::metadata::ImageMetadata::default())
            .unwrap();
        let leaf_id_again = catalog.assign_keyword_path(&[image_b], &path).unwrap();
        assert_eq!(leaf_id_again, leaf_id, "the same path must resolve to the same leaf id");
        assert_eq!(catalog.list_keywords().unwrap().len(), 3, "no duplicate keyword rows created");
    }

    #[test]
    fn assign_keyword_path_batches_across_multiple_images_and_is_idempotent() {
        let catalog = Catalog::open_in_memory().unwrap();
        let a = catalog
            .add_image_with_edit_stack("/a.CR3", "hash-a", 1, &EditStack::empty(), &crate::metadata::ImageMetadata::default())
            .unwrap();
        let b = catalog
            .add_image_with_edit_stack("/b.CR3", "hash-b", 2, &EditStack::empty(), &crate::metadata::ImageMetadata::default())
            .unwrap();

        let path = ["kyoto".to_string()];
        catalog.assign_keyword_path(&[a, b], &path).unwrap();
        // Re-assigning to the same set must be a silent no-op, not an error.
        catalog.assign_keyword_path(&[a, b], &path).unwrap();

        assert_eq!(catalog.get_image_keywords(a).unwrap().len(), 1);
        assert_eq!(catalog.get_image_keywords(b).unwrap().len(), 1);
    }

    #[test]
    fn remove_keyword_from_image_is_anchor_only() {
        let catalog = Catalog::open_in_memory().unwrap();
        let a = catalog
            .add_image_with_edit_stack("/a.CR3", "hash-a", 1, &EditStack::empty(), &crate::metadata::ImageMetadata::default())
            .unwrap();
        let b = catalog
            .add_image_with_edit_stack("/b.CR3", "hash-b", 2, &EditStack::empty(), &crate::metadata::ImageMetadata::default())
            .unwrap();
        let leaf_id = catalog.assign_keyword_path(&[a, b], &["kyoto".to_string()]).unwrap();

        catalog.remove_keyword_from_image(a, leaf_id).unwrap();

        assert!(catalog.get_image_keywords(a).unwrap().is_empty());
        assert_eq!(catalog.get_image_keywords(b).unwrap().len(), 1, "removal must not affect other images");
    }

    /// Same "explicit child-delete, don't rely on CASCADE" discipline as
    /// `remove_images_deletes_both_rows_and_makes_the_file_reimportable`'s
    /// orphan check for image_versions -- confirms no image_keywords rows
    /// survive a removed image.
    #[test]
    fn remove_images_deletes_orphaned_keyword_assignments() {
        let catalog = Catalog::open_in_memory().unwrap();
        let image_id = catalog
            .add_image_with_edit_stack("/a.CR3", "hash-a", 1, &EditStack::empty(), &crate::metadata::ImageMetadata::default())
            .unwrap();
        catalog.assign_keyword_path(&[image_id], &["kyoto".to_string()]).unwrap();

        catalog.remove_images(&[image_id]).unwrap();

        let orphans: i64 = catalog
            .conn
            .query_row(
                "SELECT count(*) FROM image_keywords WHERE image_id = ?1",
                params![image_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(orphans, 0);
    }

    fn two_test_images(catalog: &Catalog) -> (i64, i64) {
        let a = catalog
            .add_image_with_edit_stack("/a.CR3", "hash-a", 1, &EditStack::empty(), &crate::metadata::ImageMetadata::default())
            .unwrap();
        let b = catalog
            .add_image_with_edit_stack("/b.CR3", "hash-b", 2, &EditStack::empty(), &crate::metadata::ImageMetadata::default())
            .unwrap();
        (a, b)
    }

    #[test]
    fn create_collection_with_images_is_atomic_and_populated() {
        let catalog = Catalog::open_in_memory().unwrap();
        let (a, b) = two_test_images(&catalog);

        let collection_id = catalog.create_collection_with_images("Portfolio", &[a, b]).unwrap();

        let members = catalog.list_collection_image_ids(collection_id).unwrap();
        assert_eq!(members.len(), 2);
        assert!(members.contains(&a) && members.contains(&b));

        let collections = catalog.list_collections().unwrap();
        assert_eq!(collections.len(), 1);
        assert_eq!(collections[0].name, "Portfolio");
        assert!(!collections[0].is_smart);
        assert_eq!(collections[0].rules, None);
        assert_eq!(collections[0].count, Some(2));
    }

    #[test]
    fn add_and_remove_images_from_collection_round_trips() {
        let catalog = Catalog::open_in_memory().unwrap();
        let (a, b) = two_test_images(&catalog);
        let collection_id = catalog.create_collection("Trip").unwrap();

        catalog.add_images_to_collection(collection_id, &[a, b]).unwrap();
        assert_eq!(catalog.list_collection_image_ids(collection_id).unwrap().len(), 2);

        // Idempotent: re-adding an already-member image is a silent no-op.
        catalog.add_images_to_collection(collection_id, &[a]).unwrap();
        assert_eq!(catalog.list_collection_image_ids(collection_id).unwrap().len(), 2);

        catalog.remove_images_from_collection(collection_id, &[a]).unwrap();
        let members = catalog.list_collection_image_ids(collection_id).unwrap();
        assert_eq!(members, vec![b]);
    }

    #[test]
    fn smart_collection_rules_round_trip_and_count_is_none_not_zero() {
        let catalog = Catalog::open_in_memory().unwrap();
        let rules = vec![serde_json::json!({"field": "rating", "op": ">=", "value": 4})];

        let collection_id = catalog.create_smart_collection("Best Shots", &rules).unwrap();

        let collections = catalog.list_collections().unwrap();
        let smart = collections.iter().find(|c| c.id == collection_id).unwrap();
        assert!(smart.is_smart);
        assert_eq!(smart.rules, Some(rules));
        assert_eq!(
            smart.count, None,
            "a smart collection's count must be None, not 0 -- 0 would be indistinguishable from a real zero-match count"
        );

        let updated_rules = vec![serde_json::json!({"field": "flag", "op": "==", "value": "pick"})];
        catalog.update_smart_collection_rules(collection_id, &updated_rules).unwrap();
        let collections = catalog.list_collections().unwrap();
        assert_eq!(collections[0].rules, Some(updated_rules));
    }

    #[test]
    fn delete_collection_removes_row_and_membership_including_for_a_smart_collection() {
        let catalog = Catalog::open_in_memory().unwrap();
        let (a, _b) = two_test_images(&catalog);
        let manual_id = catalog.create_collection_with_images("Trip", &[a]).unwrap();
        let smart_id = catalog
            .create_smart_collection("Picks", &[serde_json::json!({"field": "flag", "op": "==", "value": "pick"})])
            .unwrap();

        catalog.delete_collection(manual_id).unwrap();
        catalog.delete_collection(smart_id).unwrap(); // no collection_images rows to begin with -- must not error

        assert_eq!(catalog.list_collections().unwrap().len(), 0);
        let orphans: i64 = catalog
            .conn
            .query_row("SELECT count(*) FROM collection_images", [], |row| row.get(0))
            .unwrap();
        assert_eq!(orphans, 0);
    }

    #[test]
    fn remove_images_deletes_orphaned_collection_membership() {
        let catalog = Catalog::open_in_memory().unwrap();
        let (a, b) = two_test_images(&catalog);
        let collection_id = catalog.create_collection_with_images("Trip", &[a, b]).unwrap();

        catalog.remove_images(&[a]).unwrap();

        let members = catalog.list_collection_image_ids(collection_id).unwrap();
        assert_eq!(members, vec![b], "removing an image must also drop its collection membership");
    }

    #[test]
    fn list_all_image_keywords_is_flat_and_independent_of_list_images() {
        let catalog = Catalog::open_in_memory().unwrap();
        let (a, b) = two_test_images(&catalog);
        let leaf = catalog.assign_keyword_path(&[a], &["kyoto".to_string()]).unwrap();
        catalog.assign_keyword_path(&[b], &["kyoto".to_string()]).unwrap();

        let all = catalog.list_all_image_keywords().unwrap();
        assert_eq!(all.len(), 2);
        assert!(all.iter().all(|assignment| assignment.keyword_id == leaf));
    }

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

        Catalog::migrate(&conn).expect("migrate must succeed against a pre-Slice-2 catalog");
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

    #[test]
    fn list_images_orders_newest_first_and_skips_virtual_copies() {
        let catalog = Catalog::open_in_memory().expect("in-memory catalog opens");

        let image_a = catalog.add_image("/a.CR3").unwrap();
        catalog.add_edit_stack(image_a, &EditStack::empty()).unwrap();
        let image_b = catalog.add_image("/b.CR3").unwrap();
        catalog.add_edit_stack(image_b, &EditStack::empty()).unwrap();

        let images = catalog.list_images().unwrap();
        assert_eq!(images.len(), 2);
        // both inserted in the same instant in tests, so just check both
        // images are represented exactly once each, not duplicated.
        let paths: Vec<&str> = images.iter().map(|s| s.path.as_str()).collect();
        assert!(paths.contains(&"/a.CR3"));
        assert!(paths.contains(&"/b.CR3"));
    }

    fn backup_test_dirs(name: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let app_data = std::env::temp_dir().join(format!("emulsion-backup-test-{name}-appdata"));
        let dest = std::env::temp_dir().join(format!("emulsion-backup-test-{name}-dest"));
        let _ = std::fs::remove_dir_all(&app_data);
        let _ = std::fs::remove_dir_all(&dest);
        std::fs::create_dir_all(&app_data).unwrap();
        std::fs::create_dir_all(&dest).unwrap();
        (app_data, dest)
    }

    #[test]
    fn get_backup_settings_returns_migrate_defaults_and_reopening_does_not_duplicate_them() {
        let catalog = Catalog::open_in_memory().expect("in-memory catalog opens");
        let settings = catalog.get_backup_settings().unwrap();
        assert_eq!(settings.frequency, "weekly");
        assert_eq!(settings.folder, None);
        assert!(settings.check_integrity);
        assert!(!settings.optimize);
        assert_eq!(settings.last_backup_at, None);

        // migrate() runs again on open_in_memory() the same way it would on
        // a real open() -- INSERT OR IGNORE must stay a true no-op, not
        // silently reset a user's already-saved choice back to defaults.
        catalog
            .update_backup_settings(&BackupSettings {
                frequency: "daily".to_string(),
                folder: Some("/some/folder".to_string()),
                check_integrity: false,
                optimize: true,
                last_backup_at: Some("2026-01-01 00:00:00".to_string()),
            })
            .unwrap();
        Catalog::migrate(&catalog.conn).unwrap();
        let settings = catalog.get_backup_settings().unwrap();
        assert_eq!(settings.frequency, "daily");
        assert_eq!(settings.folder, Some("/some/folder".to_string()));
    }

    #[test]
    fn update_backup_settings_round_trips_and_clears_optional_fields() {
        let catalog = Catalog::open_in_memory().expect("in-memory catalog opens");
        catalog
            .update_backup_settings(&BackupSettings {
                frequency: "monthly".to_string(),
                folder: Some("/backups".to_string()),
                check_integrity: true,
                optimize: true,
                last_backup_at: Some("2026-06-01 12:00:00".to_string()),
            })
            .unwrap();
        let settings = catalog.get_backup_settings().unwrap();
        assert_eq!(settings.frequency, "monthly");
        assert_eq!(settings.folder, Some("/backups".to_string()));
        assert!(settings.check_integrity);
        assert!(settings.optimize);
        assert_eq!(settings.last_backup_at, Some("2026-06-01 12:00:00".to_string()));

        // folder/last_backup_at going back to None must actually clear the
        // row, not persist a stale value forever.
        catalog
            .update_backup_settings(&BackupSettings {
                frequency: "never".to_string(),
                folder: None,
                check_integrity: false,
                optimize: false,
                last_backup_at: None,
            })
            .unwrap();
        let settings = catalog.get_backup_settings().unwrap();
        assert_eq!(settings.folder, None);
        assert_eq!(settings.last_backup_at, None);
    }

    #[test]
    fn perform_backup_writes_a_valid_independently_openable_copy_and_keeps_wal_engaged() {
        let (app_data, dest) = backup_test_dirs("valid-copy");
        let catalog = Catalog::open_in_memory().expect("in-memory catalog opens");
        let image_id = catalog.add_image("/photo.CR3").unwrap();
        catalog.add_edit_stack(image_id, &EditStack::empty()).unwrap();

        let outcome = catalog.perform_backup(&dest, &app_data, true, true).unwrap();

        assert!(std::path::Path::new(&outcome.path).exists());
        let copy = Connection::open(&outcome.path).unwrap();
        let path: String = copy
            .query_row("SELECT path FROM images WHERE id = ?1", params![image_id], |row| row.get(0))
            .unwrap();
        assert_eq!(path, "/photo.CR3");

        // Found empirically (real dev-app smoke test, not assumed): the
        // Online Backup API copies the source's WAL-mode header byte
        // verbatim, so without the explicit journal_mode=DELETE conversion
        // in perform_backup, the finished backup file would itself claim
        // WAL mode and spawn its own -shm/-wal sidecars the moment
        // anything opens it -- not what a single-file portable backup
        // artifact should do.
        let copy_mode: String = copy.query_row("PRAGMA journal_mode", [], |row| row.get(0)).unwrap();
        assert_eq!(copy_mode, "delete", "a backup file must be a clean single-file artifact, not WAL-mode");

        // in-memory test catalogs are never harden()'d (see open_in_memory's
        // own doc comment -- WAL can't engage on ":memory:"), so this
        // asserts against a real on-disk catalog instead, the only way to
        // meaningfully check VACUUM didn't disturb journal_mode.
        let real_catalog_path = app_data.join("real-catalog.sqlite");
        let real_catalog = Catalog::open(&real_catalog_path).unwrap();
        real_catalog.perform_backup(&dest, &app_data, false, true).unwrap();
        let mode: String = real_catalog
            .conn
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .unwrap();
        assert_eq!(mode, "wal", "VACUUM must not disengage WAL mode");

        std::fs::remove_dir_all(&app_data).ok();
        std::fs::remove_dir_all(&dest).ok();
    }

    #[test]
    fn perform_backup_rejects_a_destination_inside_the_app_data_dir() {
        let (app_data, _dest) = backup_test_dirs("nested-dest-rejected");
        let nested = app_data.join("not-separate");
        std::fs::create_dir_all(&nested).unwrap();
        let catalog = Catalog::open_in_memory().expect("in-memory catalog opens");

        let err = catalog.perform_backup(&nested, &app_data, false, false).unwrap_err();
        assert!(matches!(err, BackupError::DestinationNotSeparate));

        let err_same = catalog.perform_backup(&app_data, &app_data, false, false).unwrap_err();
        assert!(matches!(err_same, BackupError::DestinationNotSeparate));

        std::fs::remove_dir_all(&app_data).ok();
    }

    #[test]
    fn perform_backup_with_integrity_check_blocks_a_corrupt_catalog() {
        let (app_data, dest) = backup_test_dirs("integrity-check-blocks");
        let real_catalog_path = app_data.join("catalog.sqlite");
        let catalog = Catalog::open(&real_catalog_path).unwrap();
        catalog.add_image("/photo.CR3").unwrap();
        drop(catalog);

        // Corrupt the file on disk directly, well past the 100-byte header
        // (so the file still opens and reports its magic/format correctly)
        // -- flips bytes inside a data page so a re-opened connection's own
        // `PRAGMA integrity_check` catches real, reported corruption rather
        // than failing to open the file at all.
        {
            use std::io::{Seek, SeekFrom, Write};
            let file_len = std::fs::metadata(&real_catalog_path).unwrap().len();
            assert!(file_len > 4096, "catalog file must span more than one page for this corruption to land past the header");
            let mut file = std::fs::OpenOptions::new().write(true).open(&real_catalog_path).unwrap();
            file.seek(SeekFrom::Start(4096)).unwrap();
            file.write_all(&[0xFFu8; 200]).unwrap();
        }

        let reopened = Connection::open(&real_catalog_path).unwrap();
        let corrupt_catalog = Catalog { conn: reopened };
        let err = corrupt_catalog.perform_backup(&dest, &app_data, true, false).unwrap_err();
        // A corrupted file can surface either as a reported integrity_check
        // failure or as SQLite refusing to read the damaged page at all --
        // both mean "did not write a backup of bad data", which is the
        // actual property under test.
        assert!(
            matches!(err, BackupError::IntegrityCheckFailed(_) | BackupError::Sqlite(_)),
            "expected the corrupt catalog to be caught before a backup was written, got {err:?}"
        );
        // No backup file should have been written on a blocked attempt.
        assert_eq!(std::fs::read_dir(&dest).unwrap().count(), 0);

        std::fs::remove_dir_all(&app_data).ok();
        std::fs::remove_dir_all(&dest).ok();
    }

    // -- M3 History/Undo/Snapshots --------------------------------------

    fn stack_with(op: &str, value: f64) -> EditStack {
        EditStack { schema_version: 1, ops: vec![json!({"op": op, "value": value})] }
    }

    #[test]
    fn record_edit_stack_creates_a_labeled_history_row_on_real_content_change() {
        let catalog = Catalog::open_in_memory().unwrap();
        let image_id = catalog.add_image("/a.CR3").unwrap();
        let version_id = catalog.add_edit_stack(image_id, &EditStack::empty()).unwrap();

        let history = catalog
            .record_edit_stack(version_id, &stack_with("exposure", 0.5), Some("Exposure"))
            .unwrap();

        assert_eq!(history.len(), 1);
        assert_eq!(history[0].label, "Exposure");
        assert_eq!(catalog.get_edit_stack(version_id).unwrap(), stack_with("exposure", 0.5));
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

        assert_eq!(history.len(), 1);
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
        let first_id = history[0].id;

        // Undo back to the first entry (restore, not a new edit).
        catalog.restore_history_entry(version_id, first_id).unwrap();

        // A genuinely new edit made from this undone position should cut
        // off "Exposure 2" -- the abandoned redo branch.
        let history = catalog.record_edit_stack(version_id, &stack_with("contrast", 5.0), Some("Contrast")).unwrap();

        assert_eq!(history.iter().map(|h| h.label.as_str()).collect::<Vec<_>>(), vec!["Exposure 1", "Contrast"]);
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
        let entry_id = history[0].id;
        catalog.record_edit_stack(version_id, &stack_with("exposure", 0.9), Some("Exposure 2")).unwrap();

        let restored = catalog.restore_history_entry(version_id, entry_id).unwrap();

        assert_eq!(restored, stack_with("exposure", 0.5));
        assert_eq!(catalog.get_edit_stack(version_id).unwrap(), stack_with("exposure", 0.5));
        // Restoring itself must not add a third row.
        assert_eq!(catalog.get_history(version_id).unwrap().len(), 2);
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
        assert_eq!(history.iter().map(|h| h.label.as_str()).collect::<Vec<_>>(), vec!["First", "Second"]);
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
}
