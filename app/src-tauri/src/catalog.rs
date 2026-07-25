//! Catalog schema v0 (M0 spike).
//!
//! Minimal slice of ADR-0005 (SQLite catalog storage) and ADR-0006
//! (versioned JSON edit-stack representation): enough to store one image
//! reference and one non-destructive edit record, and read it back.
//! Collections, keywords, virtual-copy branching, etc. are M1+ scope.

use rusqlite::{params, Connection, Result};
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

impl Catalog {
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
                added_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS image_versions (
                id INTEGER PRIMARY KEY,
                image_id INTEGER NOT NULL REFERENCES images(id) ON DELETE CASCADE,
                is_virtual_copy INTEGER NOT NULL DEFAULT 0,
                edit_stack_json TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            ",
        )
    }

    /// Register a source image reference on disk. Never copies or touches
    /// the original file — the catalog only ever stores a path (PRD §7.1).
    pub fn add_image(&self, path: &str) -> Result<i64> {
        self.conn
            .execute("INSERT INTO images (path) VALUES (?1)", params![path])?;
        Ok(self.conn.last_insert_rowid())
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
}
