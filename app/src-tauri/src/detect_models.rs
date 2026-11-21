//! The person-detector model file (RFC-0037): D-FINE medium trained on COCO (`ustc-community/dfine-medium-coco`, Apache-2.0),
//! exported to ONNX at a fixed 640 x 640 input and constant-folded with the ONNX Runtime's basic graph optimisation
//! (`docs/rfc/RFC-0037-appendix/export_onnx.py`; the helper's ONNX Runtime has no CPU `Cos`, which the positional embedding
//! would otherwise need, and the folded file matches the PyTorch model to 1e-5 on real photographs). One file, 79 MB, shipped in the installer like the Select Subject files and installed beside them
//! (`bundled_models::DETECT`); there is no download dialog, so a build without it simply has no detector and the person
//! region keeps its face-only behaviour.

use std::path::{Path, PathBuf};

pub const FILENAME: &str = "dfine_medium_coco.onnx";
pub const SIZE: u64 = 78_809_647;
pub const SHA256: &str = "f0ccdada55b67e57f6d6c7aba48045bb8cc417fd0b2e7bf18e89a6a74cd2e6ab";

/// The model's path if it is installed in `dir` and is the pinned file; `None` otherwise (a damaged copy counts as absent).
pub const FAMILY: crate::editions::ModelFamily = crate::editions::ModelFamily::Detect;

pub fn ready_path(dir: &Path) -> Option<PathBuf> {
    ready_path_for(crate::editions::current(), dir)
}

/// `None` too when the edition leaves person detection out, even if a valid copy is cached.
pub fn ready_path_for(edition: &crate::editions::Edition, dir: &Path) -> Option<PathBuf> {
    if !edition.includes(FAMILY) {
        return None;
    }
    crate::bundled_models::resolve(dir, &crate::bundled_models::DETECT[0])
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

    #[test]
    fn an_excluded_detector_is_never_ready() {
        use crate::editions::{tests::without, Edition};
        // Nothing is installed here, so the default edition cannot be ready either: the point is that the gate adds no
        // path to a file the edition leaves out. (The pinned file is 79 MB and is not committed.)
        let dir = std::env::temp_dir().join("emulsion-detect-edition-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert!(ready_path_for(&Edition::all("default"), &dir).is_none());
        assert!(ready_path_for(&without(FAMILY), &dir).is_none());
    }

}
