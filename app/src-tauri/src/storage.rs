//! User-configurable cache location (Settings > Storage): thumbnails and
//! the Develop preview cache can together reach many GB on a library of
//! any real size, and the OS default app-data directory (often the small
//! system drive) may not be where a user wants that space spent.
//!
//! Deliberately scoped to *cache* only -- thumbnails (regenerable from the
//! source file at any time, see `import::generate_thumbnail_file`) and the
//! Develop preview cache (content-hash-keyed, see `preview_cache.rs`, never
//! referenced by absolute path from the catalog). HDR merge results and
//! the catalog database itself stay in the fixed app-data location: those
//! are real user content/state, not cache, and relocating them is a much
//! higher-stakes problem than this feature is solving.

use crate::catalog::Catalog;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StorageInfo {
    /// The user's override, exactly as stored -- `None` means "use the
    /// default app-data directory".
    pub cache_dir: Option<String>,
    /// The directory actually in effect right now (the override if set,
    /// otherwise the resolved default) -- what the UI should display as
    /// "thumbnails and previews are currently stored in...".
    pub effective_dir: String,
    pub thumbnails_bytes: u64,
    pub previews_bytes: u64,
}

/// The directory thumbnails/previews should be read from and written to
/// right now: the user's override if one is set, else `default_dir`
/// (the OS app-data directory). Every command that touches thumbnails or
/// the preview cache calls this fresh rather than caching the result, so
/// a change takes effect on the very next operation.
pub fn resolve_cache_root(catalog: &Catalog, default_dir: &Path) -> rusqlite::Result<PathBuf> {
    Ok(match catalog.get_cache_dir()? {
        Some(dir) => PathBuf::from(dir),
        None => default_dir.to_path_buf(),
    })
}

fn dir_size(dir: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    entries
        .filter_map(|e| e.ok())
        .filter_map(|e| e.metadata().ok())
        .filter(|m| m.is_file())
        .map(|m| m.len())
        .sum()
}

pub fn get_storage_info(catalog: &Catalog, default_dir: &Path) -> Result<StorageInfo, String> {
    let cache_dir = catalog.get_cache_dir().map_err(|e| e.to_string())?;
    let root = resolve_cache_root(catalog, default_dir).map_err(|e| e.to_string())?;
    Ok(StorageInfo {
        cache_dir,
        effective_dir: root.to_string_lossy().into_owned(),
        thumbnails_bytes: dir_size(&root.join("thumbnails")),
        previews_bytes: dir_size(&root.join("previews")),
    })
}

/// Moves every file under the current cache root's `thumbnails/` and
/// `previews/` into `new_dir` (creating it if needed), rewrites every
/// `images.thumbnail_path` row to point at the new location, and updates
/// the stored setting -- so nothing is silently orphaned and no thumbnail
/// goes missing from the Library grid mid-move.
///
/// `new_dir: None` resets to the default app-data location.
///
/// Holds the catalog lock for the whole operation: every thumbnail-
/// writing path (`import`, `ensure_thumbnail`, the startup backfill) also
/// locks the catalog before writing, so this can't race a concurrent
/// write landing in the directory being moved out from under it. The one
/// gap this doesn't close -- a caller that already resolved the *old*
/// root via `resolve_cache_root` moments before this call started, and
/// writes into it *after* this call releases the lock -- is a narrow,
/// accepted race for a rare admin action, the same tradeoff `remove_images`
/// already documents for its own thumbnail-cleanup race.
pub fn move_cache_dir(
    catalog: &Arc<Mutex<Catalog>>,
    default_dir: &Path,
    new_dir: Option<&str>,
) -> Result<StorageInfo, String> {
    let catalog = catalog.lock().map_err(|e| e.to_string())?;
    let old_root = resolve_cache_root(&catalog, default_dir).map_err(|e| e.to_string())?;
    let new_root = match new_dir {
        Some(d) => PathBuf::from(d),
        None => default_dir.to_path_buf(),
    };

    if old_root != new_root {
        for subdir in ["thumbnails", "previews"] {
            let old_dir = old_root.join(subdir);
            let new_sub = new_root.join(subdir);
            std::fs::create_dir_all(&new_sub).map_err(|e| e.to_string())?;

            let Ok(entries) = std::fs::read_dir(&old_dir) else {
                continue; // nothing to move -- old_dir never existed
            };
            for entry in entries.filter_map(|e| e.ok()) {
                let from = entry.path();
                if !from.is_file() {
                    continue;
                }
                let to = new_sub.join(entry.file_name());
                // Best-effort per file: `rename` is atomic and cheap but
                // fails across filesystems/drives (the exact case this
                // feature exists for), so fall back to copy+remove. A
                // single file's failure (e.g. transiently locked) doesn't
                // abort the whole move -- it's left behind in the old
                // location and just needs regenerating, same as any other
                // cache miss.
                if std::fs::rename(&from, &to).is_err() {
                    if std::fs::copy(&from, &to).is_ok() {
                        let _ = std::fs::remove_file(&from);
                    }
                }
            }
            let _ = std::fs::remove_dir(&old_dir); // no-op unless now empty
        }

        let old_thumb_prefix = old_root.join("thumbnails").to_string_lossy().into_owned();
        let new_thumb_prefix = new_root.join("thumbnails").to_string_lossy().into_owned();
        catalog
            .rewrite_thumbnail_path_prefix(&old_thumb_prefix, &new_thumb_prefix)
            .map_err(|e| e.to_string())?;

        catalog.set_cache_dir(new_dir).map_err(|e| e.to_string())?;
    }

    get_storage_info(&catalog, default_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_dirs(name: &str) -> (PathBuf, PathBuf) {
        let default_dir = std::env::temp_dir().join(format!("emulsion-storage-test-{name}-default"));
        let custom_dir = std::env::temp_dir().join(format!("emulsion-storage-test-{name}-custom"));
        let _ = std::fs::remove_dir_all(&default_dir);
        let _ = std::fs::remove_dir_all(&custom_dir);
        std::fs::create_dir_all(&default_dir).unwrap();
        (default_dir, custom_dir)
    }

    fn write_file(dir: &Path, name: &str, contents: &[u8]) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join(name), contents).unwrap();
    }

    #[test]
    fn resolve_cache_root_falls_back_to_default_until_a_dir_is_set() {
        let catalog = Catalog::open_in_memory().unwrap();
        let (default_dir, custom_dir) = test_dirs("resolve-fallback");

        assert_eq!(resolve_cache_root(&catalog, &default_dir).unwrap(), default_dir);

        catalog.set_cache_dir(Some(custom_dir.to_str().unwrap())).unwrap();
        assert_eq!(resolve_cache_root(&catalog, &default_dir).unwrap(), custom_dir);
    }

    #[test]
    fn get_storage_info_reports_real_directory_sizes() {
        let catalog = Catalog::open_in_memory().unwrap();
        let (default_dir, _) = test_dirs("sizes");
        write_file(&default_dir.join("thumbnails"), "1.jpg", b"12345");
        write_file(&default_dir.join("thumbnails"), "2.jpg", b"1234567");
        write_file(&default_dir.join("previews"), "abc.png", b"123");

        let info = get_storage_info(&catalog, &default_dir).unwrap();
        assert_eq!(info.cache_dir, None);
        assert_eq!(info.effective_dir, default_dir.to_string_lossy());
        assert_eq!(info.thumbnails_bytes, 12);
        assert_eq!(info.previews_bytes, 3);
    }

    #[test]
    fn get_storage_info_is_zero_for_a_directory_that_does_not_exist_yet() {
        let catalog = Catalog::open_in_memory().unwrap();
        let (default_dir, _) = test_dirs("empty");
        // default_dir itself exists (test_dirs creates it), but its
        // thumbnails/previews subdirectories never got created -- a brand
        // new install, before any thumbnail has ever been generated.
        let info = get_storage_info(&catalog, &default_dir).unwrap();
        assert_eq!(info.thumbnails_bytes, 0);
        assert_eq!(info.previews_bytes, 0);
    }

    #[test]
    fn move_cache_dir_moves_files_rewrites_catalog_paths_and_persists_the_setting() {
        let catalog = Arc::new(Mutex::new(Catalog::open_in_memory().unwrap()));
        let (default_dir, custom_dir) = test_dirs("move-basic");
        write_file(&default_dir.join("thumbnails"), "1.jpg", b"thumb-data");
        write_file(&default_dir.join("previews"), "hash.png", b"preview-data");

        let image_id = {
            let c = catalog.lock().unwrap();
            let id = c.add_image("/photo.jpg").unwrap();
            c.set_thumbnail_path(id, default_dir.join("thumbnails").join("1.jpg").to_str().unwrap())
                .unwrap();
            id
        };

        let info = move_cache_dir(&catalog, &default_dir, Some(custom_dir.to_str().unwrap())).unwrap();

        assert_eq!(info.cache_dir, Some(custom_dir.to_string_lossy().into_owned()));
        assert_eq!(info.effective_dir, custom_dir.to_string_lossy());
        assert_eq!(info.thumbnails_bytes, "thumb-data".len() as u64);
        assert_eq!(info.previews_bytes, "preview-data".len() as u64);

        // Files physically relocated, old directory cleaned up.
        assert!(custom_dir.join("thumbnails").join("1.jpg").exists());
        assert!(custom_dir.join("previews").join("hash.png").exists());
        assert!(!default_dir.join("thumbnails").join("1.jpg").exists());

        // The catalog's own stored path follows the move -- a caller
        // reading it back gets a real, existing file, not a dangling
        // reference to the old location.
        let new_thumb_path = catalog.lock().unwrap().get_thumbnail_path(image_id).unwrap().unwrap();
        assert_eq!(
            new_thumb_path,
            custom_dir.join("thumbnails").join("1.jpg").to_string_lossy()
        );

        // Setting persisted -- a fresh resolve (as any later command
        // would do) sees the new location without needing this same
        // move_cache_dir call in scope.
        let c = catalog.lock().unwrap();
        assert_eq!(resolve_cache_root(&c, &default_dir).unwrap(), custom_dir);
    }

    #[test]
    fn move_cache_dir_back_to_default_clears_the_override() {
        let catalog = Arc::new(Mutex::new(Catalog::open_in_memory().unwrap()));
        let (default_dir, custom_dir) = test_dirs("move-back");
        write_file(&custom_dir.join("thumbnails"), "1.jpg", b"x");
        catalog.lock().unwrap().set_cache_dir(Some(custom_dir.to_str().unwrap())).unwrap();

        let info = move_cache_dir(&catalog, &default_dir, None).unwrap();

        assert_eq!(info.cache_dir, None);
        assert_eq!(info.effective_dir, default_dir.to_string_lossy());
        assert!(default_dir.join("thumbnails").join("1.jpg").exists());
    }

    #[test]
    fn move_cache_dir_is_a_no_op_when_the_target_equals_the_current_location() {
        let catalog = Arc::new(Mutex::new(Catalog::open_in_memory().unwrap()));
        let (default_dir, _) = test_dirs("move-noop");
        write_file(&default_dir.join("thumbnails"), "1.jpg", b"unchanged");

        let info = move_cache_dir(&catalog, &default_dir, None).unwrap();

        assert_eq!(info.cache_dir, None);
        assert!(default_dir.join("thumbnails").join("1.jpg").exists());
    }
}
