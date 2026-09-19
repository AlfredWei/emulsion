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

mod backup;
mod collections;
mod culling;
mod edit_stack;
mod export_plugins;
mod faces;
mod geo;
mod images;
mod keywords;
mod merge_sources;
mod presets;
mod schema;
#[cfg(test)]
mod test_support;

pub use edit_stack::{EditStack, HistoryEntry, SnapshotEntry};
pub use presets::PresetEntry;
pub use export_plugins::ExportPlugin;
pub use images::ImageSummary;
pub use faces::{FaceRow, PersonRow};
pub use keywords::{KeywordRef, KeywordNode, ImageKeywordAssignment};
pub use collections::CollectionSummary;
pub use backup::{BackupOutcome, BackupSettings};

pub struct Catalog {
    conn: Connection,
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
        // `seed_defaults: false` -- every test in this module that touches
        // presets wants a clean, empty starting table (a real, deliberate
        // choice, not an oversight -- see `default_presets_are_seeded_...`
        // below for the one test that opts back in via a real file-backed
        // `open()` instead).
        Self::migrate(&conn, false)?;
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
        Self::migrate(&conn, true)?;
        Ok(Self { conn })
    }
}

