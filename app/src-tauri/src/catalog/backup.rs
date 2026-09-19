use super::*;

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
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
