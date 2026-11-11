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
pub fn ready_path(models_dir: &Path) -> Option<PathBuf> {
    let path = models_dir.join(FILENAME);
    let ok = matches!(std::fs::metadata(&path), Ok(m) if m.len() == SIZE) && crate::segment_models::sha256_file(&path).map(|h| h == SHA256).unwrap_or(false);
    ok.then_some(path)
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
}
