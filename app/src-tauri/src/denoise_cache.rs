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
