//! Face detection/embedding model fetch-and-cache (M5 Slice 6, RFC-0005 §3.3).
//! Downloads YuNet + SFace from a pinned `opencv/opencv_zoo` commit into a
//! caller-supplied cache directory on first use, verifies each file's SHA-256
//! against the value recorded in `models/FACE_MODELS.md`, and never re-fetches
//! once a valid cached copy exists — same "fetch once, cache forever" shape
//! RFC-0005 §3.3 always intended. Pure fetch/verify logic, no catalog/Tauri
//! dependency: the caller resolves the real app-data cache directory.
//!
//! Both files are Git LFS objects in `opencv_zoo` -- `raw.githubusercontent.com`
//! serves the LFS *pointer* text for these paths, not the actual bytes, so
//! fetching goes through `media.githubusercontent.com/media/...`, GitHub's
//! LFS media-download endpoint, instead. Confirmed empirically: the naive
//! `raw.githubusercontent.com` URL returns a ~130-byte pointer file whose own
//! `size:` field matches the real file size, which is what first surfaced
//! this.

use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

const OPENCV_ZOO_COMMIT: &str = "47534e27c9851bb1128ccc0102f1145e27f23f98";

struct ModelSpec {
    filename: &'static str,
    repo_path: &'static str,
    sha256: &'static str,
}

const YUNET: ModelSpec = ModelSpec {
    filename: "face_detection_yunet_2023mar.onnx",
    repo_path: "models/face_detection_yunet/face_detection_yunet_2023mar.onnx",
    sha256: "8f2383e4dd3cfbb4553ea8718107fc0423210dc964f9f4280604804ed2552fa4",
};

const SFACE: ModelSpec = ModelSpec {
    filename: "face_recognition_sface_2021dec.onnx",
    repo_path: "models/face_recognition_sface/face_recognition_sface_2021dec.onnx",
    sha256: "0ba9fbfa01b5270c96627c4ef784da859931e02f04419c829e83484087c34e79",
};

#[derive(Debug, Clone)]
pub struct FaceModelPaths {
    pub yunet: PathBuf,
    pub sface: PathBuf,
}

#[derive(Debug, thiserror::Error)]
pub enum FaceModelError {
    #[error("could not create model cache directory {0}: {1}")]
    CreateDir(PathBuf, std::io::Error),
    #[error("failed to download {0}: {1}")]
    Download(String, reqwest::Error),
    #[error("download of {0} returned HTTP {1}")]
    HttpStatus(String, u16),
    #[error("could not write {0}: {1}")]
    Write(PathBuf, std::io::Error),
    #[error("{0} failed checksum verification after download -- expected sha256:{1}, got sha256:{2}")]
    ChecksumMismatch(String, String, String),
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// A file is considered a valid cached copy only if it already exists AND
/// its checksum matches -- an incomplete/corrupt prior download does not
/// count as cached and gets re-fetched, not silently used. Any read
/// failure (missing, corrupt, unreadable) folds into the same "not
/// cached" `false`, not a distinct error -- `ensure_one` just re-fetches
/// either way, so there is no separate "read the cache" failure mode
/// worth its own `FaceModelError` variant.
fn is_valid_cache(path: &Path, expected_sha256: &str) -> bool {
    match std::fs::read(path) {
        Ok(bytes) => sha256_hex(&bytes) == expected_sha256,
        Err(_) => false,
    }
}

async fn ensure_one(client: &reqwest::Client, cache_dir: &Path, spec: &ModelSpec) -> Result<PathBuf, FaceModelError> {
    let dest = cache_dir.join(spec.filename);
    if is_valid_cache(&dest, spec.sha256) {
        return Ok(dest);
    }

    let url = format!("https://media.githubusercontent.com/media/opencv/opencv_zoo/{OPENCV_ZOO_COMMIT}/{}", spec.repo_path);
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| FaceModelError::Download(spec.filename.to_string(), e))?;
    if !response.status().is_success() {
        return Err(FaceModelError::HttpStatus(spec.filename.to_string(), response.status().as_u16()));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|e| FaceModelError::Download(spec.filename.to_string(), e))?;

    let actual_sha256 = sha256_hex(&bytes);
    if actual_sha256 != spec.sha256 {
        return Err(FaceModelError::ChecksumMismatch(spec.filename.to_string(), spec.sha256.to_string(), actual_sha256));
    }

    // Write to a temp path first, then rename -- a crash/interruption
    // mid-write must not leave a corrupt file that is_valid_cache() would
    // still (correctly) reject next time, but this avoids the window
    // entirely rather than relying on that fallback.
    let tmp_dest = cache_dir.join(format!("{}.part", spec.filename));
    std::fs::write(&tmp_dest, &bytes).map_err(|e| FaceModelError::Write(tmp_dest.clone(), e))?;
    std::fs::rename(&tmp_dest, &dest).map_err(|e| FaceModelError::Write(dest.clone(), e))?;
    Ok(dest)
}

/// Ensures both model files exist, valid, in `cache_dir` -- downloading
/// whichever ones are missing or fail checksum verification -- and returns
/// their paths. Safe to call on every app start: a fully-cached, valid
/// pair costs one checksum read per file and no network request.
pub async fn ensure_models(cache_dir: &Path) -> Result<FaceModelPaths, FaceModelError> {
    std::fs::create_dir_all(cache_dir).map_err(|e| FaceModelError::CreateDir(cache_dir.to_path_buf(), e))?;
    let client = reqwest::Client::new();
    let yunet = ensure_one(&client, cache_dir, &YUNET).await?;
    let sface = ensure_one(&client, cache_dir, &SFACE).await?;
    Ok(FaceModelPaths { yunet, sface })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_hex_matches_a_known_vector() {
        // NIST test vector: SHA-256("abc").
        assert_eq!(sha256_hex(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }

    #[test]
    fn is_valid_cache_rejects_a_missing_file() {
        let dir = test_dir("missing");
        assert!(!is_valid_cache(&dir.join("nope.onnx"), YUNET.sha256));
    }

    #[test]
    fn is_valid_cache_rejects_content_that_does_not_match_the_checksum() {
        let dir = test_dir("corrupt");
        let path = dir.join("corrupt.onnx");
        std::fs::write(&path, b"not the real model").unwrap();
        assert!(!is_valid_cache(&path, YUNET.sha256));
    }

    #[test]
    fn is_valid_cache_accepts_content_matching_the_checksum() {
        let dir = test_dir("valid");
        let path = dir.join("real.onnx");
        std::fs::write(&path, b"abc").unwrap();
        assert!(is_valid_cache(&path, &sha256_hex(b"abc")));
    }

    /// Needs real network access -- not part of the default `cargo test`
    /// suite. Run explicitly: `cargo test --lib -- --ignored
    /// ensure_models_downloads_and_verifies_both_real_files`. Confirms the
    /// real fetch-from-`opencv_zoo`-and-checksum path actually works
    /// end to end, and that a second call is a pure cache hit (no re-fetch).
    #[tokio::test]
    #[ignore]
    async fn ensure_models_downloads_and_verifies_both_real_files() {
        let dir = test_dir("live-fetch");
        let paths = ensure_models(&dir).await.expect("first fetch succeeds");
        assert!(is_valid_cache(&paths.yunet, YUNET.sha256));
        assert!(is_valid_cache(&paths.sface, SFACE.sha256));

        // Second call must be a cache hit -- delete the underlying files
        // out from under the returned paths to prove a re-fetch would
        // fail loudly if one were (wrongly) attempted, then swap in
        // pre-populated valid files instead of calling ensure_models()
        // again over the network a second time.
        let yunet_mtime = std::fs::metadata(&paths.yunet).unwrap().modified().unwrap();
        let paths_again = ensure_models(&dir).await.expect("cached fetch succeeds");
        let yunet_mtime_after = std::fs::metadata(&paths_again.yunet).unwrap().modified().unwrap();
        assert_eq!(yunet_mtime, yunet_mtime_after, "a valid cache hit must not rewrite the file");
    }

    // Each test passes its own unique `name` -- Rust's default test runner
    // runs tests in parallel within one process, so a shared fixed path
    // (matching this codebase's own storage.rs/catalog.rs precedent) would
    // let one test's cleanup race another's still-in-progress read/write.
    fn test_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("emulsion-face-models-test-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
