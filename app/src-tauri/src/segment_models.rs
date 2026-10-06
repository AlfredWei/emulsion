//! SAM 2 Tiny model fetch-and-cache (M6 slice 2a, RFC-0026 §3.5).
//!
//! Same shape as `face_models.rs` (fetch once, verify SHA-256, never commit,
//! never re-fetch a valid copy), with two differences the size forces: the
//! 134 MB encoder is **streamed to a `.part` file while hashing** instead of
//! held in memory, and there is a **manual path** (`import_files`) for
//! machines that must not or cannot download: the user picks the two files,
//! they are verified against the same hashes and copied into the cache.
//!
//! Source: the ONNX re-host `vietanhdev/segment-anything-2-onnx-models`
//! (model card: Apache-2.0, weights from Meta's `facebookresearch/sam2`),
//! pinned to one commit; the hashes are the LFS object ids at that commit and
//! equal the ones recorded in `docs/rfc/RFC-0023-appendix/SHA256SUMS`.

use serde::Serialize;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const HF_COMMIT: &str = "071f58077599431edd0e5d2ac52ecca4c78f1cab";
const HF_REPO: &str = "vietanhdev/segment-anything-2-onnx-models";

pub struct ModelSpec {
    pub filename: &'static str,
    pub sha256: &'static str,
    pub size: u64,
}

pub const ENCODER: ModelSpec = ModelSpec {
    filename: "sam2_hiera_tiny.encoder.onnx",
    sha256: "4cc015ee18520e93f8c7ddfeaca7436039daaaaf19721b4b96a8810a805e82f7",
    size: 134_261_315,
};

pub const DECODER: ModelSpec = ModelSpec {
    filename: "sam2_hiera_tiny.decoder.onnx",
    sha256: "f5a4bd656c143899fb7f52d64ed81e6f6aeb37d477a0b6da50146ac7cf2187bf",
    size: 20_640_886,
};

const SPECS: [&ModelSpec; 2] = [&ENCODER, &DECODER];

#[derive(Debug, Clone)]
pub struct SegmentModelPaths {
    pub encoder: PathBuf,
    pub decoder: PathBuf,
}

/// What the UI needs to know before arming the tool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelState {
    /// Both files present and their checksums match.
    Ready,
    /// At least one file is missing or fails its checksum.
    Missing,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelStatus {
    pub state: ModelState,
    /// Bytes still to fetch (0 when ready) -- for the consent dialog's "155 MB".
    pub download_bytes: u64,
    /// The host a download would contact, shown in the consent dialog.
    pub host: &'static str,
}

#[derive(Debug, thiserror::Error)]
pub enum SegmentModelError {
    #[error("could not create model cache directory {0}: {1}")]
    CreateDir(PathBuf, std::io::Error),
    #[error("failed to download {0}: {1}")]
    Download(String, reqwest::Error),
    #[error("download of {0} returned HTTP {1}")]
    HttpStatus(String, u16),
    #[error("could not write {0}: {1}")]
    Write(PathBuf, std::io::Error),
    #[error("{0} failed checksum verification -- expected sha256:{1}, got sha256:{2}")]
    ChecksumMismatch(String, String, String),
    #[error("{0} is not one of the expected model files ({1}, {2})")]
    UnexpectedFile(String, &'static str, &'static str),
    #[error("could not read {0}: {1}")]
    Read(PathBuf, std::io::Error),
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Streams a file through SHA-256 (the encoder is 134 MB; never read it whole).
pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex(&hasher.finalize()))
}

/// A cached file counts only if it exists, has the pinned size and matches
/// the checksum; any read failure folds into "not cached" (same rule as
/// `face_models::is_valid_cache`).
fn is_valid_cache(path: &Path, spec: &ModelSpec) -> bool {
    match std::fs::metadata(path) {
        Ok(meta) if meta.len() == spec.size => sha256_file(path).map(|h| h == spec.sha256).unwrap_or(false),
        _ => false,
    }
}

/// Cheap enough for every arm of the tool: two size checks, then two streamed
/// hashes only when sizes match (about 0.3 s for 155 MB on an SSD).
pub fn status(cache_dir: &Path) -> ModelStatus {
    let missing: u64 = SPECS
        .iter()
        .filter(|spec| !is_valid_cache(&cache_dir.join(spec.filename), spec))
        .map(|spec| spec.size)
        .sum();
    ModelStatus {
        state: if missing == 0 { ModelState::Ready } else { ModelState::Missing },
        download_bytes: missing,
        host: "huggingface.co",
    }
}

/// Paths of a fully valid pair, or `None` (the caller then offers download / manual import).
pub fn ready_paths(cache_dir: &Path) -> Option<SegmentModelPaths> {
    let encoder = cache_dir.join(ENCODER.filename);
    let decoder = cache_dir.join(DECODER.filename);
    (is_valid_cache(&encoder, &ENCODER) && is_valid_cache(&decoder, &DECODER)).then_some(SegmentModelPaths { encoder, decoder })
}

fn url_for(spec: &ModelSpec) -> String {
    format!("https://huggingface.co/{HF_REPO}/resolve/{HF_COMMIT}/{}", spec.filename)
}

/// Downloads whichever files are missing or invalid, streaming to
/// `<name>.part` while hashing, verifying, then renaming into place.
/// `on_progress(downloaded_so_far, total_to_download)` is called per chunk
/// (the caller throttles what it emits). Safe to call when everything is
/// already cached: no request is made.
pub async fn ensure_models(
    cache_dir: &Path,
    mut on_progress: impl FnMut(u64, u64),
) -> Result<SegmentModelPaths, SegmentModelError> {
    std::fs::create_dir_all(cache_dir).map_err(|e| SegmentModelError::CreateDir(cache_dir.to_path_buf(), e))?;
    let total: u64 = SPECS.iter().filter(|s| !is_valid_cache(&cache_dir.join(s.filename), s)).map(|s| s.size).sum();
    let mut done: u64 = 0;
    let client = reqwest::Client::new();

    for spec in SPECS {
        let dest = cache_dir.join(spec.filename);
        if is_valid_cache(&dest, spec) {
            continue;
        }
        let name = spec.filename.to_string();
        let mut response = client
            .get(url_for(spec))
            .send()
            .await
            .map_err(|e| SegmentModelError::Download(name.clone(), e))?;
        if !response.status().is_success() {
            return Err(SegmentModelError::HttpStatus(name, response.status().as_u16()));
        }
        let tmp = cache_dir.join(format!("{}.part", spec.filename));
        let mut file = std::fs::File::create(&tmp).map_err(|e| SegmentModelError::Write(tmp.clone(), e))?;
        let mut hasher = Sha256::new();
        while let Some(chunk) = response.chunk().await.map_err(|e| SegmentModelError::Download(name.clone(), e))? {
            hasher.update(&chunk);
            file.write_all(&chunk).map_err(|e| SegmentModelError::Write(tmp.clone(), e))?;
            done += chunk.len() as u64;
            on_progress(done, total);
        }
        drop(file);
        let actual = hex(&hasher.finalize());
        if actual != spec.sha256 {
            let _ = std::fs::remove_file(&tmp);
            return Err(SegmentModelError::ChecksumMismatch(name, spec.sha256.to_string(), actual));
        }
        std::fs::rename(&tmp, &dest).map_err(|e| SegmentModelError::Write(dest.clone(), e))?;
    }
    ready_paths(cache_dir).ok_or_else(|| SegmentModelError::ChecksumMismatch("model files".into(), "valid pair".into(), "missing after download".into()))
}

/// The offline path: the user chooses the encoder and/or decoder file. Each
/// file is matched **by name** to its spec, verified against the pinned
/// checksum and only then copied into the cache; a wrong or damaged file is
/// rejected without touching the cache.
pub fn import_files(cache_dir: &Path, files: &[PathBuf]) -> Result<ModelStatus, SegmentModelError> {
    std::fs::create_dir_all(cache_dir).map_err(|e| SegmentModelError::CreateDir(cache_dir.to_path_buf(), e))?;
    for file in files {
        let name = file.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let spec = SPECS
            .iter()
            .find(|s| s.filename == name)
            .ok_or_else(|| SegmentModelError::UnexpectedFile(name.clone(), ENCODER.filename, DECODER.filename))?;
        let actual = sha256_file(file).map_err(|e| SegmentModelError::Read(file.clone(), e))?;
        if actual != spec.sha256 {
            return Err(SegmentModelError::ChecksumMismatch(name, spec.sha256.to_string(), actual));
        }
        let dest = cache_dir.join(spec.filename);
        let tmp = cache_dir.join(format!("{}.part", spec.filename));
        std::fs::copy(file, &tmp).map_err(|e| SegmentModelError::Write(tmp.clone(), e))?;
        std::fs::rename(&tmp, &dest).map_err(|e| SegmentModelError::Write(dest.clone(), e))?;
    }
    Ok(status(cache_dir))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("emulsion-segment-models-test-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn sha256_file_matches_a_known_vector() {
        let dir = test_dir("vector");
        let path = dir.join("abc");
        std::fs::write(&path, b"abc").unwrap();
        assert_eq!(sha256_file(&path).unwrap(), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }

    #[test]
    fn an_empty_cache_is_missing_and_reports_the_full_download_size() {
        let s = status(&test_dir("empty"));
        assert_eq!(s.state, ModelState::Missing);
        assert_eq!(s.download_bytes, ENCODER.size + DECODER.size);
        assert!(ready_paths(&test_dir("empty2")).is_none());
    }

    #[test]
    fn a_right_named_file_with_the_wrong_content_does_not_count_as_cached() {
        let dir = test_dir("wrong-content");
        std::fs::write(dir.join(DECODER.filename), b"not the decoder").unwrap();
        assert_eq!(status(&dir).download_bytes, ENCODER.size + DECODER.size);
    }

    #[test]
    fn import_rejects_an_unknown_file_name_and_a_checksum_mismatch_without_touching_the_cache() {
        let cache = test_dir("import-cache");
        let src = test_dir("import-src");
        let stray = src.join("something_else.onnx");
        std::fs::write(&stray, b"x").unwrap();
        assert!(matches!(import_files(&cache, &[stray]), Err(SegmentModelError::UnexpectedFile(..))));

        let fake = src.join(DECODER.filename);
        std::fs::write(&fake, b"fake decoder bytes").unwrap();
        assert!(matches!(import_files(&cache, &[fake]), Err(SegmentModelError::ChecksumMismatch(..))));
        assert!(!cache.join(DECODER.filename).exists(), "a rejected file must not reach the cache");
        assert!(!cache.join(format!("{}.part", DECODER.filename)).exists());
    }

    #[test]
    fn the_download_url_is_pinned_to_one_commit() {
        let url = url_for(&ENCODER);
        assert!(url.contains(HF_COMMIT) && url.ends_with("sam2_hiera_tiny.encoder.onnx"), "{url}");
        assert!(!url.contains("/main/"));
    }

    /// Needs the real network (155 MB): `cargo test --lib -- --ignored
    /// segment_models_live_fetch`. Proves the pinned URLs and hashes agree.
    #[tokio::test]
    #[ignore]
    async fn segment_models_live_fetch() {
        let dir = test_dir("live");
        let mut last = 0;
        let paths = ensure_models(&dir, |d, _| last = d).await.expect("fetch");
        assert!(paths.encoder.exists() && paths.decoder.exists());
        assert_eq!(last, ENCODER.size + DECODER.size);
        assert_eq!(status(&dir).state, ModelState::Ready);
    }
}
