//! The object-removal model file (RFC-0032, ADR-0010): LaMa, fp32, as ONNX, from the Hugging Face repository
//! `Carve/LaMa-ONNX` at revision `c3c0c9e468934d62e79c329e35d82dd09ff8c444` (Apache-2.0 code and weights; trained on
//! Places2, whose terms were not read: the owner accepted that on 2026-11-02, ADR-0010 §2). One file, 208 MB, shipped in
//! the installer (`bundled_models::INPAINT`) and installed into the `inpaint_models` folder of the cache root; there is
//! no download dialog, so a build without it has no Remove and says so.

use crate::segment_models::sha256_file;
use serde::Serialize;
use std::path::{Path, PathBuf};

pub const FILENAME: &str = "lama_fp32.onnx";
pub const SIZE: u64 = 208_044_816;
pub const SHA256: &str = "1faef5301d78db7dda502fe59966957ec4b79dd64e16f03ed96913c7a4eb68d6";
/// What a patch made with this model is called in the edit stack and the catalog.
pub const ENGINE: &str = "lama-512-places2";

/// The model's path if it is installed in `dir` and is the pinned file; `None` otherwise (a damaged copy counts as absent).
pub const FAMILY: crate::editions::ModelFamily = crate::editions::ModelFamily::Inpaint;

/// What Remove needs to know before arming: the model is there, it is not (no download exists), or the build's edition
/// leaves it out (RFC-0038 slice 4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelState {
    Ready,
    Missing,
    NotInEdition,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelStatus {
    pub state: ModelState,
}

pub fn status(dir: &Path) -> ModelStatus {
    status_for(crate::editions::current(), dir)
}

pub fn status_for(edition: &crate::editions::Edition, dir: &Path) -> ModelStatus {
    let state = if !edition.includes(FAMILY) {
        ModelState::NotInEdition
    } else if ready_path_for(edition, dir).is_some() {
        ModelState::Ready
    } else {
        ModelState::Missing
    };
    ModelStatus { state }
}

pub fn ready_path(dir: &Path) -> Option<PathBuf> {
    ready_path_for(crate::editions::current(), dir)
}

/// `None` too when the edition leaves Remove out, even if a valid copy is cached.
pub fn ready_path_for(edition: &crate::editions::Edition, dir: &Path) -> Option<PathBuf> {
    if !edition.includes(FAMILY) {
        return None;
    }
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

    #[test]
    fn remove_reports_ready_missing_or_not_in_edition() {
        use crate::editions::{tests::without, Edition};
        let dir = std::env::temp_dir().join("emulsion-inpaint-edition-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(status_for(&Edition::all("default"), &dir).state, ModelState::Missing);
        // A damaged copy is never "ready", and an excluded family is never "missing": there is no download to offer.
        std::fs::write(dir.join(FILENAME), b"not the model").unwrap();
        assert_eq!(status_for(&Edition::all("default"), &dir).state, ModelState::Missing);
        assert_eq!(status_for(&without(FAMILY), &dir).state, ModelState::NotInEdition);
        assert_eq!(serde_json::to_string(&status_for(&without(FAMILY), &dir)).unwrap(), r#"{"state":"not_in_edition"}"#);
        assert!(ready_path_for(&without(FAMILY), &dir).is_none());
    }

}
