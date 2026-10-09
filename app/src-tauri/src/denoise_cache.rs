//! The cache of denoised full-size images (M6 slice 3b, RFC-0027 §3.1): one lossless PNG per
//! `(content hash, model, tile parameters)`, written by the helper job and read when an edit stack with
//! `ai_denoise` is rendered. The cache never decides anything: a missing, unreadable or wrong-sized entry is
//! simply "not denoised yet", and the render continues without the effect.
//!
//! The directory lives under the same cache root as thumbnails and previews (`<root>/denoise_cache`), so it
//! follows Settings > Storage. Eviction (RFC-0027 §7.3) is slice 3d's.

use crate::ai::tiling::{OVERLAP, TILE};
use crate::denoise_models::MODEL_ID;
use std::path::{Path, PathBuf};

/// The file a finished job for this photo, model and tiling is stored at. The tile parameters are in the name
/// because they change the result (a different overlap blends differently), so an old entry is never reused.
pub fn entry_path(dir: &Path, content_hash: &str, model_id: &str) -> PathBuf {
    dir.join(format!("{content_hash}.{model_id}.t{TILE}o{OVERLAP}.png"))
}

/// This photo's entry for the current model, if a complete file of exactly `size` pixels is there. A size
/// mismatch (the source was decoded differently, or replaced) is treated as no entry at all.
pub fn lookup(dir: &Path, content_hash: &str, size: (u32, u32)) -> Option<PathBuf> {
    let path = entry_path(dir, content_hash, MODEL_ID);
    (image::image_dimensions(&path).ok()? == size).then_some(path)
}

/// Removes everything of one photo: its entries (any model, any tiling), half-written `.part` files, the
/// region preview files (`crop-<hash>-<job>.before|after.png`) and the Develop preview blends
/// (`blend-<hash>-<tier>-<amount>.png`). The separator after the hash keeps one photo's prefix from matching
/// another's.
pub fn remove_photo(dir: &Path, content_hash: &str) {
    remove_with_prefixes(dir, &[format!("{content_hash}."), format!("crop-{content_hash}-"), format!("blend-{content_hash}-")]);
}

/// Removes only the files denoise derives into the previews directory (crop previews and preview blends),
/// never the photo's own `<hash>.png` previews that live beside them.
pub fn remove_derived(previews_dir: &Path, content_hash: &str) {
    remove_with_prefixes(previews_dir, &[format!("crop-{content_hash}-"), format!("blend-{content_hash}-")]);
}

/// Removes the region preview files of one photo (a new crop preview replaces the last one).
pub fn remove_crops(dir: &Path, content_hash: &str) {
    remove_with_prefixes(dir, &[format!("crop-{content_hash}-")]);
}

/// How much the kept denoised copies may take in total before the oldest are removed (RFC-0027 §7.3). About
/// 40-60 MB per 24 MP photo, so roughly 40 photos; a user-visible setting is not built yet.
pub const CAP_BYTES: u64 = 2_000_000_000;

/// True for the whole-photo entries (`<hash>.<model>.t512o32.png`), not the job hand-off, crop or blend files.
fn is_entry(name: &str) -> bool {
    name.ends_with(".png") && !name.starts_with("input-") && !name.starts_with("crop-") && !name.starts_with("blend-")
}

/// Removes the oldest whole-photo entries until the total is within `cap`, never `keep` (the entry just made).
/// "Oldest" is the time the entry was written: reading one must not change it, since a preview blend is rebuilt
/// when it is older than its entry. Returns the removed paths.
pub fn evict_over_cap(dir: &Path, cap: u64, keep: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut files: Vec<(std::time::SystemTime, u64, PathBuf)> = entries
        .flatten()
        .filter(|e| is_entry(&e.file_name().to_string_lossy()))
        .filter_map(|e| {
            let m = e.metadata().ok()?;
            Some((m.modified().ok()?, m.len(), e.path()))
        })
        .collect();
    let mut total: u64 = files.iter().map(|f| f.1).sum();
    files.sort_by_key(|f| f.0);
    let mut removed = Vec::new();
    for (_, len, path) in files {
        if total <= cap {
            break;
        }
        if path == keep {
            continue;
        }
        if std::fs::remove_file(&path).is_ok() {
            total -= len;
            removed.push(path);
        }
    }
    removed
}

/// Removes job hand-off files (`input-<id>.png`) and half-written `.part` files older than `max_age`: what a
/// crash or a killed app leaves behind. Only one job runs at a time and each deletes its own input when it ends.
pub fn sweep_stale(dir: &Path, max_age: std::time::Duration) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let now = std::time::SystemTime::now();
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        let stale_kind = name.starts_with("input-") || name.ends_with(".part");
        let old = e.metadata().and_then(|m| m.modified()).ok().and_then(|t| now.duration_since(t).ok()).is_some_and(|age| age >= max_age);
        if stale_kind && old {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

fn remove_with_prefixes(dir: &Path, prefixes: &[String]) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if prefixes.iter().any(|p| name.starts_with(p.as_str())) {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("emulsion-denoise-cache-test-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_key_names_the_photo_the_model_and_the_tiling() {
        let p = entry_path(Path::new("c"), "abc123", "nafnet_sidd_w32");
        assert_eq!(p, Path::new("c").join("abc123.nafnet_sidd_w32.t512o32.png"));
        assert_ne!(entry_path(Path::new("c"), "abc123", "other"), p);
    }

    #[test]
    fn lookup_needs_a_readable_file_of_the_expected_size() {
        let dir = test_dir("lookup");
        assert!(lookup(&dir, "h", (4, 3)).is_none(), "nothing there yet");
        image::RgbImage::new(4, 3).save(entry_path(&dir, "h", MODEL_ID)).unwrap();
        assert!(lookup(&dir, "h", (4, 3)).is_some());
        assert!(lookup(&dir, "h", (3, 4)).is_none(), "wrong size is no entry");
        std::fs::write(entry_path(&dir, "broken", MODEL_ID), b"not a png").unwrap();
        assert!(lookup(&dir, "broken", (4, 3)).is_none(), "a damaged file is no entry");
        assert!(lookup(&dir.join("missing-dir"), "h", (4, 3)).is_none());
    }

    #[test]
    fn eviction_removes_the_oldest_entries_over_the_cap_and_never_the_new_one_or_other_files() {
        let dir = test_dir("evict");
        let write = |name: &str, bytes: usize| std::fs::write(dir.join(name), vec![0u8; bytes]).unwrap();
        write("old.m.t512o32.png", 400);
        std::thread::sleep(std::time::Duration::from_millis(20));
        write("mid.m.t512o32.png", 400);
        std::thread::sleep(std::time::Duration::from_millis(20));
        write("new.m.t512o32.png", 400);
        write("input-3.png", 5000);
        write("crop-x-1.after.png", 5000);
        write("blend-x-draft-50.png", 5000);
        let keep = dir.join("new.m.t512o32.png");
        assert!(evict_over_cap(&dir, 5000, &keep).is_empty(), "within the cap: nothing goes");
        let removed = evict_over_cap(&dir, 900, &keep);
        assert_eq!(removed, vec![dir.join("old.m.t512o32.png")]);
        assert!(dir.join("mid.m.t512o32.png").exists() && keep.exists());
        let removed = evict_over_cap(&dir, 100, &keep);
        assert_eq!(removed, vec![dir.join("mid.m.t512o32.png")], "the entry just made is kept even when it alone exceeds the cap");
        assert!(keep.exists() && dir.join("input-3.png").exists() && dir.join("crop-x-1.after.png").exists() && dir.join("blend-x-draft-50.png").exists());
    }

    #[test]
    fn the_sweep_removes_only_old_hand_off_and_part_files() {
        let dir = test_dir("sweep");
        for name in ["input-7.png", "h.m.t512o32.png.part", "h.m.t512o32.png", "crop-h-1.after.png"] {
            std::fs::write(dir.join(name), b"x").unwrap();
        }
        sweep_stale(&dir, std::time::Duration::from_secs(3600));
        assert!(dir.join("input-7.png").exists(), "too young to be stale");
        sweep_stale(&dir, std::time::Duration::ZERO);
        assert!(!dir.join("input-7.png").exists() && !dir.join("h.m.t512o32.png.part").exists());
        assert!(dir.join("h.m.t512o32.png").exists() && dir.join("crop-h-1.after.png").exists(), "finished entries and crops are not swept");
    }

    #[test]
    fn removing_a_photo_removes_all_its_entries_and_only_its_own() {
        let dir = test_dir("remove");
        for name in [
            "h1.m1.t512o32.png",
            "h1.m2.t512o32.png",
            "h1.m1.t512o32.png.part",
            "h10.m1.t512o32.png",
            "crop-h1-7.before.png",
            "crop-h1-7.after.png",
            "crop-h10-7.after.png",
            "blend-h1-draft-50.png",
            "blend-h10-draft-50.png",
        ] {
            std::fs::write(dir.join(name), b"x").unwrap();
        }
        remove_photo(&dir, "h1");
        let mut left: Vec<String> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().to_string()).collect();
        left.sort();
        assert_eq!(left, vec!["blend-h10-draft-50.png", "crop-h10-7.after.png", "h10.m1.t512o32.png"]);
        std::fs::write(dir.join("crop-h10-8.before.png"), b"x").unwrap();
        remove_derived(&dir, "h10");
        assert!(!dir.join("crop-h10-7.after.png").exists() && !dir.join("crop-h10-8.before.png").exists());
        assert!(dir.join("h10.m1.t512o32.png").exists(), "a derived-file sweep leaves the whole-photo entry");
        std::fs::write(dir.join("h10.png"), b"x").unwrap();
        remove_derived(&dir, "h10");
        assert!(dir.join("h10.png").exists(), "and never the photo's own preview");
    }
}
