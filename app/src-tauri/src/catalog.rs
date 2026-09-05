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
    /// Which `import_paths` call brought this image in (M4 Library slice) --
    /// shared by every image imported in the same folder/files operation,
    /// so the frontend's "Last Import" source can filter by
    /// `import_batch == max(import_batch)` without a separate batch table.
    /// `None` for rows inserted before this column existed.
    pub import_batch: Option<i64>,
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
    pub exposure_bias: Option<f32>,
    pub metering_mode: Option<String>,
    pub flash: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub altitude: Option<f32>,
    pub file_size: Option<i64>,
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
    pub id: i64,
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

/// Internal to the Rust command layer only (HDR merge, M5) -- never
/// returned to the frontend. See `get_image_exposure_info`.
#[derive(Debug, Clone)]
pub struct ImageExposureInfo {
    pub path: String,
    pub iso: Option<u32>,
    pub aperture: Option<f32>,
    pub shutter_speed: Option<f32>,
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
const DEFAULT_PRESETS: &[(&str, &str)] = &[
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

    fn migrate(conn: &Connection, seed_defaults: bool) -> Result<()> {
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
                iso, aperture, shutter_speed, focal_length,
                exposure_bias, metering_mode, flash,
                width, height, latitude, longitude, altitude,
                captured_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)",
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
                metadata.exposure_bias,
                metadata.metering_mode,
                metadata.flash,
                metadata.width,
                metadata.height,
                metadata.latitude,
                metadata.longitude,
                metadata.altitude,
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

    /// One image's path + exposure fields, resolved by id -- the catalog-
    /// side half of building an `hdr_merge::BracketInput` (RFC-0003 §3.6);
    /// `hdr_merge.rs` itself has no catalog dependency (see that module's
    /// own header comment), so the Tauri command layer (`lib.rs`) is what
    /// bridges the two, calling this once per selected image id.
    pub fn get_image_exposure_info(&self, image_id: i64) -> Result<Option<ImageExposureInfo>> {
        self.conn
            .query_row(
                "SELECT path, iso, aperture, shutter_speed FROM images WHERE id = ?1",
                params![image_id],
                |row| {
                    Ok(ImageExposureInfo {
                        path: row.get(0)?,
                        iso: row.get(1)?,
                        aperture: row.get(2)?,
                        shutter_speed: row.get(3)?,
                    })
                },
            )
            .optional()
    }

    /// Records one `hdr_merge_sources` provenance row per bracket member
    /// (RFC-0003 §3.6) -- `sources` is `(source_image_id, ordinal,
    /// ev_offset, dx, dy)`, in the caller's own original bracket order.
    /// All-or-nothing: a partial provenance record for a merge result
    /// would be actively misleading (looks complete, silently isn't), so
    /// this is one transaction rather than best-effort per row.
    pub fn add_hdr_merge_sources(&self, result_image_id: i64, sources: &[(i64, i32, f32, i32, i32)]) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        for &(source_image_id, ordinal, ev_offset, dx, dy) in sources {
            tx.execute(
                "INSERT INTO hdr_merge_sources (result_image_id, source_image_id, ordinal, ev_offset, dx, dy)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![result_image_id, source_image_id, ordinal, ev_offset, dx, dy],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Reads back the provenance rows `add_hdr_merge_sources` wrote for
    /// one merge result, ordered by `ordinal` -- the natural read
    /// counterpart to that write-only method. Kept as real pub API ready
    /// for a future "Show HDR sources" UI (same "no UI trigger yet"
    /// precedent as `add_image_with_metadata`'s own doc comment) --
    /// exercised today by this file's own test plus `hdr_merge.rs`'s
    /// real-bracket end-to-end test, which (being a different module)
    /// has no access to this one's private `conn` field.
    #[allow(dead_code)]
    pub fn get_hdr_merge_sources(&self, result_image_id: i64) -> Result<Vec<(i64, i32, f32, i32, i32)>> {
        let mut stmt = self.conn.prepare(
            "SELECT source_image_id, ordinal, ev_offset, dx, dy FROM hdr_merge_sources
             WHERE result_image_id = ?1 ORDER BY ordinal",
        )?;
        let rows = stmt
            .query_map(params![result_image_id], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?))
            })?
            .collect();
        rows
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
                            id: image_id,
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
            // M5 (HDR merge): same explicit discipline -- both directions,
            // since a removed image might be a merge's *result* (drop its
            // whole provenance row set) or one of its *sources* (drop just
            // that one row; the result and its other sources are unaffected).
            tx.execute(
                "DELETE FROM hdr_merge_sources WHERE result_image_id = ?1 OR source_image_id = ?1",
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

    /// The user's chosen override for where thumbnails + the Develop
    /// preview cache live (Settings > Storage) -- `None` means "use the
    /// OS default app-data directory", the same `settings` KV table
    /// pattern as `get_backup_settings`. Deliberately just this one key,
    /// not a `StorageSettings` struct like `BackupSettings` -- there's
    /// only ever this single value to round-trip.
    pub fn get_cache_dir(&self) -> Result<Option<String>> {
        self.conn
            .query_row(
                "SELECT value FROM settings WHERE key = 'cache_dir'",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()
    }

    pub fn set_cache_dir(&self, dir: Option<&str>) -> Result<()> {
        match dir {
            Some(v) => self.conn.execute(
                "INSERT INTO settings (key, value) VALUES ('cache_dir', ?1)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![v],
            ),
            None => self
                .conn
                .execute("DELETE FROM settings WHERE key = 'cache_dir'", []),
        }?;
        Ok(())
    }

    /// Used by `storage::move_cache_dir` after physically moving thumbnail
    /// files to a new directory: every `images.thumbnail_path` starting
    /// with `old_prefix` is rewritten to start with `new_prefix` instead,
    /// so the catalog's stored (absolute) paths keep pointing at real
    /// files. Returns the number of rows changed. A plain string
    /// prefix-replace, not a full reparse -- `thumbnail_path` is always
    /// `<thumbnail_dir>/<image_id>.jpg` (see `import::generate_thumbnail_file`),
    /// so the directory component is always exactly this prefix.
    pub fn rewrite_thumbnail_path_prefix(&self, old_prefix: &str, new_prefix: &str) -> Result<usize> {
        self.conn.execute(
            "UPDATE images SET thumbnail_path = ?2 || substr(thumbnail_path, ?3)
             WHERE thumbnail_path LIKE ?1 || '%'",
            params![old_prefix, new_prefix, old_prefix.len() as i64 + 1],
        )
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

    /// Used by the on-demand "jump the queue" thumbnail path
    /// (`import::ensure_thumbnail`) to check, under the lock, whether the
    /// background backfill pass has already generated this image's
    /// thumbnail by the time the request is served -- an indexed
    /// single-row lookup by primary key, not `list_images()`'s full-table
    /// join, since this is a per-click hot path.
    pub fn get_thumbnail_path(&self, image_id: i64) -> Result<Option<String>> {
        self.conn
            .query_row("SELECT thumbnail_path FROM images WHERE id = ?1", params![image_id], |row| row.get(0))
    }

    /// Tags a just-inserted image with the batch id `import_paths` computed
    /// once for the whole import call -- a separate step from
    /// `add_image_with_edit_stack`'s own insert transaction (rather than a
    /// new parameter threaded through it and its ~20 existing test call
    /// sites) since this is bookkeeping for the "Last Import" library source,
    /// not data the atomic image+version+metadata insert needs to guard.
    pub fn set_import_batch(&self, image_id: i64, import_batch: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE images SET import_batch = ?2 WHERE id = ?1",
            params![image_id, import_batch],
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

    /// Set or update the GPS coordinates and altitude for an image.
    pub fn set_geo_location(
        &self,
        image_id: i64,
        latitude: Option<f64>,
        longitude: Option<f64>,
        altitude: Option<f32>,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE images SET latitude = ?2, longitude = ?3, altitude = ?4 WHERE id = ?1",
            params![image_id, latitude, longitude, altitude],
        )?;
        Ok(())
    }

    /// Library grid data: one row per image, its primary (first,
    /// non-virtual-copy) version's culling state. Newest imports first.
    pub fn list_images(&self) -> Result<Vec<ImageSummary>> {
        let mut stmt = self.conn.prepare(
            "SELECT i.id, v.id, i.path, i.thumbnail_path, v.rating, v.flag, v.color_label, i.added_at, i.content_hash,
                    i.camera_make, i.camera_model, i.lens_model, i.iso, i.aperture, i.shutter_speed, i.focal_length,
                    i.exposure_bias, i.metering_mode, i.flash, i.width, i.height, i.latitude, i.longitude, i.altitude,
                    i.file_size, i.captured_at,
                    v.caption, i.copyright, i.contact, i.import_batch
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
                exposure_bias: row.get(16)?,
                metering_mode: row.get(17)?,
                flash: row.get(18)?,
                width: row.get(19)?,
                height: row.get(20)?,
                latitude: row.get(21)?,
                longitude: row.get(22)?,
                altitude: row.get(23)?,
                file_size: row.get(24)?,
                captured_at: row.get(25)?,
                caption: row.get(26)?,
                copyright: row.get(27)?,
                contact: row.get(28)?,
                import_batch: row.get(29)?,
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
    fn get_image_exposure_info_resolves_path_and_exif_fields() {
        let catalog = Catalog::open_in_memory().expect("in-memory catalog opens");
        let metadata = crate::metadata::ImageMetadata {
            iso: Some(400),
            aperture: Some(5.6),
            shutter_speed: Some(1.0 / 250.0),
            ..Default::default()
        };
        let image_id = catalog
            .add_image_with_edit_stack("/bracket-a.CR3", "hash-a", 100, &EditStack::empty(), &metadata)
            .unwrap();

        let info = catalog.get_image_exposure_info(image_id).unwrap().expect("row exists");
        assert_eq!(info.path, "/bracket-a.CR3");
        assert_eq!(info.iso, Some(400));
        assert_eq!(info.aperture, Some(5.6));
        assert_eq!(info.shutter_speed, Some(1.0 / 250.0));

        assert!(catalog.get_image_exposure_info(image_id + 999).unwrap().is_none());
    }

    #[test]
    fn add_hdr_merge_sources_records_one_row_per_bracket_member() {
        let catalog = Catalog::open_in_memory().expect("in-memory catalog opens");
        let a = catalog.add_image("/a.CR3").unwrap();
        let b = catalog.add_image("/b.CR3").unwrap();
        let result = catalog
            .add_image_with_edit_stack("/merged.jpg", "hash-merged", 100, &EditStack::empty(), &crate::metadata::ImageMetadata::default())
            .unwrap();

        catalog
            .add_hdr_merge_sources(result, &[(a, 0, 0.0, 0, 0), (b, 1, -1.0, -3, 2)])
            .unwrap();

        let rows = catalog.get_hdr_merge_sources(result).unwrap();
        assert_eq!(rows, vec![(a, 0, 0.0, 0, 0), (b, 1, -1.0, -3, 2)]);
    }

    /// Removing the merge *result* drops its whole provenance row set;
    /// removing one *source* only drops that one row, leaving the result
    /// and its other sources untouched -- see `remove_images`'s own doc
    /// comment on why this is two explicit directions, not a plain
    /// `ON DELETE CASCADE` off of `source_image_id`.
    #[test]
    fn removing_a_merge_result_or_a_source_cleans_up_hdr_merge_sources_correctly() {
        let catalog = Catalog::open_in_memory().expect("in-memory catalog opens");
        let a = catalog.add_image("/a.CR3").unwrap();
        let b = catalog.add_image("/b.CR3").unwrap();
        let result = catalog
            .add_image_with_edit_stack("/merged.jpg", "hash-merged", 100, &EditStack::empty(), &crate::metadata::ImageMetadata::default())
            .unwrap();
        catalog
            .add_hdr_merge_sources(result, &[(a, 0, 0.0, 0, 0), (b, 1, -1.0, -3, 2)])
            .unwrap();

        catalog.remove_images(&[a]).unwrap();
        let remaining_after_source_removed: i64 = catalog
            .conn
            .query_row("SELECT count(*) FROM hdr_merge_sources WHERE result_image_id = ?1", params![result], |r| r.get(0))
            .unwrap();
        assert_eq!(remaining_after_source_removed, 1, "removing one source must only drop that source's own row");

        catalog.remove_images(&[result]).unwrap();
        let remaining_after_result_removed: i64 = catalog
            .conn
            .query_row("SELECT count(*) FROM hdr_merge_sources WHERE result_image_id = ?1", params![result], |r| r.get(0))
            .unwrap();
        assert_eq!(remaining_after_result_removed, 0, "removing the result must drop its whole provenance row set");
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
            ..Default::default()
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

    #[test]
    fn set_geo_location_round_trips() {
        let catalog = Catalog::open_in_memory().expect("in-memory catalog opens");
        let image_id = catalog
            .add_image_with_edit_stack("/a.CR3", "hash-gps", 4096, &EditStack::empty(), &crate::metadata::ImageMetadata::default())
            .unwrap();

        catalog.set_geo_location(image_id, Some(25.033964), Some(121.564468), Some(50.5)).unwrap();

        let images = catalog.list_images().unwrap();
        assert_eq!(images[0].latitude, Some(25.033964));
        assert_eq!(images[0].longitude, Some(121.564468));
        assert_eq!(images[0].altitude, Some(50.5));
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
        Catalog::migrate(&catalog.conn, false).unwrap();
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
    fn cache_dir_defaults_to_none_and_round_trips_through_set_and_clear() {
        let catalog = Catalog::open_in_memory().expect("in-memory catalog opens");
        assert_eq!(catalog.get_cache_dir().unwrap(), None);

        catalog.set_cache_dir(Some("/Volumes/BigDrive/emulsion-cache")).unwrap();
        assert_eq!(
            catalog.get_cache_dir().unwrap(),
            Some("/Volumes/BigDrive/emulsion-cache".to_string())
        );

        // Setting it again (not just clearing) must overwrite, not insert
        // a duplicate row under the same PRIMARY KEY.
        catalog.set_cache_dir(Some("/other/path")).unwrap();
        assert_eq!(catalog.get_cache_dir().unwrap(), Some("/other/path".to_string()));

        catalog.set_cache_dir(None).unwrap();
        assert_eq!(catalog.get_cache_dir().unwrap(), None);
    }

    #[test]
    fn rewrite_thumbnail_path_prefix_updates_only_matching_rows() {
        let catalog = Catalog::open_in_memory().expect("in-memory catalog opens");
        let moved = catalog.add_image("/photo-a.jpg").unwrap();
        let untouched = catalog.add_image("/photo-b.jpg").unwrap();
        catalog.set_thumbnail_path(moved, "/old/thumbnails/1.jpg").unwrap();
        catalog.set_thumbnail_path(untouched, "/other/thumbnails/2.jpg").unwrap();

        let changed = catalog
            .rewrite_thumbnail_path_prefix("/old/thumbnails", "/new/location/thumbnails")
            .unwrap();
        assert_eq!(changed, 1);

        assert_eq!(
            catalog.get_thumbnail_path(moved).unwrap(),
            Some("/new/location/thumbnails/1.jpg".to_string())
        );
        // A different, non-matching prefix must be left completely alone.
        assert_eq!(
            catalog.get_thumbnail_path(untouched).unwrap(),
            Some("/other/thumbnails/2.jpg".to_string())
        );
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
    fn peek_history_entry_returns_the_stack_but_never_writes_it_back() {
        let catalog = Catalog::open_in_memory().unwrap();
        let image_id = catalog.add_image("/a.CR3").unwrap();
        let version_id = catalog.add_edit_stack(image_id, &EditStack::empty()).unwrap();

        let history = catalog
            .record_edit_stack(version_id, &stack_with("exposure", 0.5), Some("Exposure"))
            .unwrap();
        let entry_id = history[0].id;
        catalog.record_edit_stack(version_id, &stack_with("exposure", 0.9), Some("Exposure 2")).unwrap();

        let peeked = catalog.peek_history_entry(version_id, entry_id).unwrap();

        assert_eq!(peeked, stack_with("exposure", 0.5));
        // Unlike restore_history_entry, the live stack (and history) must
        // be completely untouched by a peek.
        assert_eq!(catalog.get_edit_stack(version_id).unwrap(), stack_with("exposure", 0.9));
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
        // add a history row.
        assert_eq!(catalog.get_edit_stack(version_id).unwrap(), stack_with("exposure", 0.9));
        assert_eq!(catalog.get_history(version_id).unwrap().len(), 2);
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

    /// M5 Slice 3 (GPU performance validation): MILESTONES.md's M5 exit
    /// criterion ("...on a 50k-image catalog...") had never been measured
    /// against the SQLite persistence path -- the interactive Develop
    /// render loop itself is architecturally decoupled from catalog size
    /// (ADR-0004: it never touches the catalog), so the real risk to that
    /// criterion is here, not in the shader. `#[ignore]`d (not part of the
    /// default fast suite) since seeding 50k rows takes real wall time;
    /// run explicitly with `cargo test --release -- --ignored --nocapture
    /// catalog_scales_to_50k_images`.
    ///
    /// Seeds a real 50k-image, 50k-version in-memory catalog (one initial
    /// edit-stack version per image, matching real import's own shape --
    /// see `add_image_with_edit_stack`), then times the three operations
    /// the Develop/Library UI actually calls against a live catalog:
    /// `list_images` (Library grid's own full-catalog query),
    /// `record_edit_stack` (every debounced slider-settle flush, always
    /// against ONE version by primary key regardless of catalog size), and
    /// `get_edit_stack` (Develop's own open-image read).
    ///
    /// This DID find a real bug on its first run, before
    /// `idx_image_versions_image_id` existed: `list_images`'s per-row
    /// correlated subquery (`SELECT id FROM image_versions WHERE image_id
    /// = i.id ...`) had no index to satisfy that filter, so SQLite fell
    /// back to a full table scan of `image_versions` for every one of the
    /// 50,000 outer rows -- confirmed via `EXPLAIN QUERY PLAN` (run
    /// separately against a minimal reproduction of this schema, not
    /// inline in this test) showing `SCAN image_versions` inside the
    /// subquery before the index, `SEARCH ... USING INDEX
    /// idx_image_versions_image_id` after. Directly measured at 37.4s
    /// (release build) for one `list_images()` call over this test's real
    /// 50,000-row catalog -- ~75x this test's own 500ms budget for that
    /// operation, and a real regression against PRD §9's own catalog-
    /// open-time target for a 50k-image catalog. Adding the index
    /// (present in `migrate()` as of this slice) dropped it to ~75ms --
    /// re-confirmed by temporarily reverting the index and re-running,
    /// which reproduced the 37.4s result again.
    /// `record_edit_stack`/`get_edit_stack` were already fast before the
    /// fix (both filter `image_versions` by its own indexed `id` primary
    /// key, never by `image_id`) -- included here as a permanent
    /// regression guard for all three, not because the other two were
    /// ever actually at risk.
    #[test]
    #[ignore]
    fn catalog_scales_to_50k_images() {
        use std::time::Instant;

        const N: i64 = 50_000;
        // Generous budgets, not tight ones -- this test's job is to catch
        // a real O(n) or worse regression (like the missing-index bug it
        // already found once), not to enforce the PRD's interactive
        // ≤100ms figure to the millisecond against an in-memory SQLite
        // connection with no real disk I/O.
        const LIST_IMAGES_BUDGET_MS: u128 = 500;
        const SINGLE_ROW_OP_BUDGET_MS: u128 = 50;

        let catalog = Catalog::open_in_memory().unwrap();
        for i in 0..N {
            catalog
                .add_image_with_edit_stack(
                    &format!("/synthetic/img_{i:06}.CR3"),
                    &format!("hash_{i:06}"),
                    20_000_000,
                    &EditStack::empty(),
                    &ImageMetadata::default(),
                )
                .unwrap();
        }

        let started = Instant::now();
        let images = catalog.list_images().unwrap();
        let list_images_ms = started.elapsed().as_millis();
        assert_eq!(images.len(), N as usize);
        eprintln!("list_images() over {N} images: {list_images_ms}ms");
        assert!(
            list_images_ms < LIST_IMAGES_BUDGET_MS,
            "list_images() took {list_images_ms}ms over {N} images, budget is {LIST_IMAGES_BUDGET_MS}ms -- likely a missing index on image_versions(image_id) or images(...) that a query now needs"
        );

        // The last-inserted row -- the worst case for any query that (if
        // it were ever mis-written to) scanned from the front, and the
        // one a real user would actually be editing right after a large
        // import.
        let target_version_id = images.last().unwrap().version_id;

        let started = Instant::now();
        catalog
            .record_edit_stack(target_version_id, &stack_with("exposure", 1.5), Some("Exposure"))
            .unwrap();
        let record_ms = started.elapsed().as_millis();
        eprintln!("record_edit_stack() against a 50k-row catalog: {record_ms}ms");
        assert!(
            record_ms < SINGLE_ROW_OP_BUDGET_MS,
            "record_edit_stack() took {record_ms}ms over {N} images, budget is {SINGLE_ROW_OP_BUDGET_MS}ms"
        );

        let started = Instant::now();
        let stack = catalog.get_edit_stack(target_version_id).unwrap();
        let get_ms = started.elapsed().as_millis();
        eprintln!("get_edit_stack() against a 50k-row catalog: {get_ms}ms");
        assert_eq!(stack, stack_with("exposure", 1.5));
        assert!(
            get_ms < SINGLE_ROW_OP_BUDGET_MS,
            "get_edit_stack() took {get_ms}ms over {N} images, budget is {SINGLE_ROW_OP_BUDGET_MS}ms"
        );
    }
}
