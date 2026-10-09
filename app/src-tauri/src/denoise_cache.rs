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

/// Removes everything of one photo: its entries (any model, any tiling), half-written `.part` files and
/// the region preview file (`crop-<hash>.png`).
pub fn remove_photo(dir: &Path, content_hash: &str) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let (entry_prefix, crop) = (format!("{content_hash}."), format!("crop-{content_hash}.png"));
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if name.starts_with(&entry_prefix) || name == crop {
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
        for name in ["h1.m1.t512o32.png", "h1.m2.t512o32.png", "h1.m1.t512o32.png.part", "h10.m1.t512o32.png", "crop-h1.png", "crop-h10.png"] {
            std::fs::write(dir.join(name), b"x").unwrap();
        }
        remove_photo(&dir, "h1");
        let mut left: Vec<String> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().to_string()).collect();
        left.sort();
        assert_eq!(left, vec!["crop-h10.png", "h10.m1.t512o32.png"]);
    }
}
