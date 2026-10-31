//! The person-detector model file (RFC-0037): D-FINE medium trained on COCO (`ustc-community/dfine-medium-coco`, Apache-2.0),
//! exported to ONNX at a fixed 640 x 640 input and constant-folded with the ONNX Runtime's basic graph optimisation
//! (`docs/rfc/RFC-0037-appendix/export_onnx.py`; the helper's ONNX Runtime has no CPU `Cos`, which the positional embedding
//! would otherwise need, and the folded file matches the PyTorch model to 1e-5 on real photographs). One file, 79 MB, shipped in the installer like the Select Subject files and installed beside them
//! (`bundled_models::DETECT`); there is no download dialog, so a build without it simply has no detector and the person
//! region keeps its face-only behaviour.

use crate::segment_models::sha256_file;
use std::path::{Path, PathBuf};

pub const FILENAME: &str = "dfine_medium_coco.onnx";
pub const SIZE: u64 = 78_809_647;
pub const SHA256: &str = "f0ccdada55b67e57f6d6c7aba48045bb8cc417fd0b2e7bf18e89a6a74cd2e6ab";

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
        let dir = std::env::temp_dir().join("emulsion-detect-models-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert!(ready_path(&dir).is_none());
        std::fs::write(dir.join(FILENAME), b"not the detector").unwrap();
        assert!(ready_path(&dir).is_none());
    }
}
