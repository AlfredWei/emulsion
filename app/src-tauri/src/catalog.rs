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
            // M2 Slice 4: same "explicit, don't rely on CASCADE" discipline
            // as the image_versions delete above.
            tx.execute(
                "DELETE FROM image_keywords WHERE image_id = ?1",
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

    /// Overwrite a version's edit stack (Slice 3: called whenever a Develop
    /// slider changes). Complements `add_edit_stack`, which only INSERTs
    /// the initial stack at import time.
    pub fn update_edit_stack(&self, version_id: i64, stack: &EditStack) -> Result<()> {
        let json = serde_json::to_string(stack).expect("EditStack is always serializable");
        self.conn.execute(
            "UPDATE image_versions SET edit_stack_json = ?2, updated_at = datetime('now') WHERE id = ?1",
            params![version_id, json],
        )?;
        Ok(())
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
    fn update_edit_stack_overwrites_the_stored_stack() {
        let catalog = Catalog::open_in_memory().expect("in-memory catalog opens");
        let image_id = catalog.add_image("/a.CR3").unwrap();
        let version_id = catalog.add_edit_stack(image_id, &EditStack::empty()).unwrap();

        let updated = EditStack {
            schema_version: 1,
            ops: vec![json!({"op": "exposure", "value": 0.4}), json!({"op": "contrast", "value": 12.0})],
        };
        catalog.update_edit_stack(version_id, &updated).unwrap();

        assert_eq!(catalog.get_edit_stack(version_id).unwrap(), updated);
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
}
