//! The face-mesh model file for blink evidence (RFC-0031 §8.14, §8.15): Google MediaPipe Face Mesh V2 as ONNX, 4.9 MB.
//!
//! Committed in `models/` and shipped in the installer like the upscale model (`upscale_models.rs`), installed on first use
//! by `bundled_models` into `blink_models/`, verified by size and SHA-256 on the way. There is **no download path**: the file
//! travels with the app; a damaged installed copy is replaced from the bundle. Provenance and the licence reading are in
//! `models/BLINK_MODEL.md`.

use std::path::{Path, PathBuf};

pub const FILENAME: &str = "face_landmarks.onnx";
pub const SIZE: u64 = 4_920_979;
pub const SHA256: &str = "d16e5a55e6a480284d468ee32469692464049518c311662ab3956681de31e3e9";
/// The folder inside the app's cache root that holds it.
pub const DIR: &str = "blink_models";

/// The installed file's path when it is there and matches its pin; None otherwise (never a path to a damaged file).
pub const FAMILY: crate::editions::ModelFamily = crate::editions::ModelFamily::Blink;

pub fn ready_path(models_dir: &Path) -> Option<PathBuf> {
    ready_path_for(crate::editions::current(), models_dir)
}

/// `None` too when the edition leaves the eye check out, even if a valid copy is cached.
pub fn ready_path_for(edition: &crate::editions::Edition, models_dir: &Path) -> Option<PathBuf> {
    if !edition.includes(FAMILY) {
        return None;
    }
    crate::bundled_models::resolve(models_dir, &crate::bundled_models::BLINK[0])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_or_wrong_file_is_not_ready() {
        let dir = std::env::temp_dir().join("emulsion-blink-models-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert!(ready_path(&dir).is_none());
        std::fs::write(dir.join(FILENAME), b"not the model").unwrap();
        assert!(ready_path(&dir).is_none());
    }

    #[test]
    fn the_committed_file_is_the_one_the_app_pins() {
        let file = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../models").join(FILENAME);
        assert!(file.is_file(), "models/{FILENAME} is missing");
        assert_eq!(std::fs::metadata(&file).unwrap().len(), SIZE);
        assert_eq!(crate::segment_models::sha256_file(&file).unwrap(), SHA256);
    }

    #[test]
    fn an_excluded_eye_check_model_is_hidden_even_when_the_pinned_file_is_installed() {
        use crate::editions::{tests::without, Edition};
        let dir = std::env::temp_dir().join("emulsion-blink-edition-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::copy(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../models").join(FILENAME), dir.join(FILENAME)).unwrap();
        assert!(ready_path_for(&Edition::all("default"), &dir).is_some());
        assert!(ready_path_for(&without(FAMILY), &dir).is_none(), "excluded beats cached");
    }

}
