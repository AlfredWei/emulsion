//! The object-removal model file (RFC-0032, ADR-0010): LaMa, fp32, as ONNX, from the Hugging Face repository
//! `Carve/LaMa-ONNX` at revision `c3c0c9e468934d62e79c329e35d82dd09ff8c444` (Apache-2.0 code and weights; trained on
//! Places2, whose terms were not read: the owner accepted that on 2026-11-02, ADR-0010 §2). One file, 208 MB, shipped in
//! the installer (`bundled_models::INPAINT`) and installed into the `inpaint_models` folder of the cache root; there is
//! no download dialog, so a build without it has no Remove and says so.

use crate::segment_models::sha256_file;
use std::path::{Path, PathBuf};

pub const FILENAME: &str = "lama_fp32.onnx";
pub const SIZE: u64 = 208_044_816;
pub const SHA256: &str = "1faef5301d78db7dda502fe59966957ec4b79dd64e16f03ed96913c7a4eb68d6";
/// What a patch made with this model is called in the edit stack and the catalog.
pub const ENGINE: &str = "lama-512-places2";

/// The model's path if it is installed in `dir` and is the pinned file; `None` otherwise (a damaged copy counts as absent).
pub fn ready_path(dir: &Path) -> Option<PathBuf> {
    let path = dir.join(FILENAME);
    let sized = matches!(std::fs::metadata(&path), Ok(m) if m.len() == SIZE);
    (sized && sha256_file(&path).map(|h| h == SHA256).unwrap_or(false)).then_some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_absent_or_wrong_file_is_not_ready() {
        let dir = std::env::temp_dir().join("emulsion-inpaint-models-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert!(ready_path(&dir).is_none());
        std::fs::write(dir.join(FILENAME), b"not the inpainter").unwrap();
        assert!(ready_path(&dir).is_none());
    }
}
