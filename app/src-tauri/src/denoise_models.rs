//! The AI denoise model file (M6 slice 3b, RFC-0027 §3.5, §8): NAFNet-SIDD-width32 as ONNX.
//!
//! Same discipline as `segment_models.rs` (pinned size and SHA-256, verified before use, copied into the cache
//! by an atomic rename, never committed) for the **import** path only. There is no download yet: the file is
//! a conversion made by `docs/rfc/RFC-0027-appendix/export_nafnet.py` from MIT-licensed upstream weights, and
//! it has no published, pinned home to fetch it from; where to publish it is an open decision (RFC-0027 §8),
//! so slice 3c's dialog offers "I have the file" until then.

use crate::segment_models::sha256_file;
use serde::Serialize;
use std::path::{Path, PathBuf};

/// Part of every cache key: a different model must never reuse another model's denoised result.
pub const MODEL_ID: &str = "nafnet_sidd_w32";
pub const FILENAME: &str = "nafnet_sidd_w32.onnx";
pub const SIZE: u64 = 117_316_644;
pub const SHA256: &str = "5ff072283fe8c86b7c990aa7331cdb26183e20b08961769c70057b82260b133d";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelState {
    Ready,
    Missing,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelStatus {
    pub state: ModelState,
    /// Size of the file the user would have to provide (0 when ready).
    pub missing_bytes: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum DenoiseModelError {
    #[error("could not create model cache directory {0}: {1}")]
    CreateDir(PathBuf, std::io::Error),
    #[error("{0} is not the denoise model file ({FILENAME})")]
    UnexpectedFile(String),
    #[error("could not read {0}: {1}")]
    Read(PathBuf, std::io::Error),
    #[error("{0} failed checksum verification -- expected sha256:{SHA256}, got sha256:{1}")]
    ChecksumMismatch(String, String),
    #[error("could not write {0}: {1}")]
    Write(PathBuf, std::io::Error),
}

fn is_valid(path: &Path) -> bool {
    match std::fs::metadata(path) {
        Ok(m) if m.len() == SIZE => sha256_file(path).map(|h| h == SHA256).unwrap_or(false),
        _ => false,
    }
}

/// The verified model file, or `None` (the caller then offers the import).
pub fn ready_path(dir: &Path) -> Option<PathBuf> {
    let p = dir.join(FILENAME);
    is_valid(&p).then_some(p)
}

pub fn status(dir: &Path) -> ModelStatus {
    match ready_path(dir) {
        Some(_) => ModelStatus { state: ModelState::Ready, missing_bytes: 0 },
        None => ModelStatus { state: ModelState::Missing, missing_bytes: SIZE },
    }
}

/// The offline path: `file` must be named `nafnet_sidd_w32.onnx` and match the pinned checksum, and is only
/// then copied into the cache; a wrong or damaged file is rejected without touching it.
pub fn import_file(dir: &Path, file: &Path) -> Result<ModelStatus, DenoiseModelError> {
    let name = file.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    if name != FILENAME {
        return Err(DenoiseModelError::UnexpectedFile(name));
    }
    let actual = sha256_file(file).map_err(|e| DenoiseModelError::Read(file.to_path_buf(), e))?;
    if actual != SHA256 {
        return Err(DenoiseModelError::ChecksumMismatch(name, actual));
    }
    std::fs::create_dir_all(dir).map_err(|e| DenoiseModelError::CreateDir(dir.to_path_buf(), e))?;
    let dest = dir.join(FILENAME);
    let tmp = dir.join(format!("{FILENAME}.part"));
    std::fs::copy(file, &tmp).map_err(|e| DenoiseModelError::Write(tmp.clone(), e))?;
    std::fs::rename(&tmp, &dest).map_err(|e| DenoiseModelError::Write(dest.clone(), e))?;
    Ok(status(dir))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("emulsion-denoise-models-test-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn an_empty_cache_is_missing_and_a_right_named_wrong_file_does_not_count() {
        let dir = test_dir("empty");
        assert_eq!(status(&dir).state, ModelState::Missing);
        assert_eq!(status(&dir).missing_bytes, SIZE);
        std::fs::write(dir.join(FILENAME), b"not the model").unwrap();
        assert_eq!(status(&dir).state, ModelState::Missing);
        assert!(ready_path(&dir).is_none());
    }

    #[test]
    fn import_rejects_a_stray_name_and_a_bad_checksum_without_touching_the_cache() {
        let cache = test_dir("import-cache");
        let src = test_dir("import-src");
        let stray = src.join("other.onnx");
        std::fs::write(&stray, b"x").unwrap();
        assert!(matches!(import_file(&cache, &stray), Err(DenoiseModelError::UnexpectedFile(_))));
        let fake = src.join(FILENAME);
        std::fs::write(&fake, b"fake model bytes").unwrap();
        assert!(matches!(import_file(&cache, &fake), Err(DenoiseModelError::ChecksumMismatch(..))));
        assert!(!cache.join(FILENAME).exists() && !cache.join(format!("{FILENAME}.part")).exists());
    }

    /// With the real file: `DENOISE_MODEL=/path/nafnet_sidd_w32.onnx cargo test --lib -- --ignored denoise_models_import_real`.
    /// Proves the pinned size and checksum equal the exported file's.
    #[test]
    #[ignore]
    fn denoise_models_import_real_file() {
        let src = PathBuf::from(std::env::var("DENOISE_MODEL").expect("DENOISE_MODEL"));
        let cache = test_dir("real");
        let s = import_file(&cache, &src).expect("import");
        assert_eq!(s.state, ModelState::Ready);
        assert_eq!(ready_path(&cache).unwrap(), cache.join(FILENAME));
    }
}
