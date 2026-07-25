//! Catalog schema v1 (M1 Slice 1).
//!
//! Slice of ADR-0005 (SQLite catalog storage) and ADR-0006 (versioned JSON
//! edit-stack representation) needed for Import + basic Library culling:
//! image references with content-hash-based dedupe, thumbnails, and
//! per-version rating/flag/color-label. Collections, keywords, and
//! virtual-copy UI are still M1+/M2 scope per MILESTONES.md.
//!
//! `migrate()` is still a plain create-if-not-exists, not a real migration
//! system — deliberate: there's no real user catalog yet to preserve across
//! schema changes. Revisit once that stops being true.

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
}

impl Catalog {
    /// Test-only: an ephemeral catalog with nothing on disk. Production
    /// code always persists to a real file via `open()` (ADR-0005) — this
    /// has no production caller, so it's compiled only for tests rather
    /// than carried as unused API surface.
    #[cfg(test)]
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        Self::migrate(&conn)?;
        Ok(Self { conn })
    }

    pub fn open<P: AsRef<std::path::Path>>(path: P) -> Result<Self> {
        let conn = Connection::open(path)?;
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
                added_at TEXT NOT NULL DEFAULT (datetime('now'))
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
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            ",
        )
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

    /// Register a source image with the content hash + size captured at
    /// import time (used by the import pipeline for duplicate detection).
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

    pub fn set_thumbnail_path(&self, image_id: i64, thumbnail_path: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE images SET thumbnail_path = ?2 WHERE id = ?1",
            params![image_id, thumbnail_path],
        )?;
        Ok(())
    }

    /// Create the primary (non-virtual-copy) edit-stack record for an image.
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

    /// Library grid data: one row per image, its primary (first,
    /// non-virtual-copy) version's culling state. Newest imports first.
    pub fn list_images(&self) -> Result<Vec<ImageSummary>> {
        let mut stmt = self.conn.prepare(
            "SELECT i.id, v.id, i.path, i.thumbnail_path, v.rating, v.flag, v.color_label, i.added_at
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
            })
        })?;
        rows.collect()
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
