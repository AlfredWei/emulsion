//! The AI denoise model file (M6 slice 3b, RFC-0027 §3.5, §8): NAFNet-SIDD-width32 as ONNX.
//!
//! Same discipline as `segment_models.rs`: pinned size and SHA-256, verified before use, streamed to a `.part`
//! file while hashing and renamed into place, never committed, with a manual **import** path for machines
//! that must not or cannot download.
//!
//! The file is our own conversion (`docs/rfc/RFC-0027-appendix/export_nafnet.py`) of MIT-licensed upstream
//! weights, so it is **hosted by this project** as a GitHub release asset rather than by a third party;
//! provenance and licence are in `models/DENOISE_MODEL.md`. A release asset can be replaced, so the pinned
//! hash, not the URL, is what makes the file trustworthy.

use crate::segment_models::sha256_file;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

const RELEASE_TAG: &str = "denoise-model-nafnet-sidd-w32-v1";
const REPO: &str = "AlfredWei/emulsion";
/// Shown in the consent dialog: the host a download contacts (GitHub then serves the file from its own CDN).
pub const HOST: &str = "github.com";

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
    /// Bytes still to fetch (0 when ready) -- for the consent dialog's "112 MB".
    pub download_bytes: u64,
    pub host: &'static str,
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
    #[error("failed to download {FILENAME}: {0}")]
    Download(reqwest::Error),
    #[error("download of {FILENAME} returned HTTP {0}")]
    HttpStatus(u16),
    #[error("download cancelled")]
    Cancelled,
}

fn url() -> String {
    format!("https://github.com/{REPO}/releases/download/{RELEASE_TAG}/{FILENAME}")
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn is_valid(path: &Path) -> bool {
    is_valid_with(path, SHA256, SIZE)
}

fn is_valid_with(path: &Path, sha256: &str, size: u64) -> bool {
    match std::fs::metadata(path) {
        Ok(m) if m.len() == size => sha256_file(path).map(|h| h == sha256).unwrap_or(false),
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
        Some(_) => ModelStatus { state: ModelState::Ready, download_bytes: 0, host: HOST },
        None => ModelStatus { state: ModelState::Missing, download_bytes: SIZE, host: HOST },
    }
}

/// Downloads the model unless a valid copy is cached (then no request is made), streaming to
/// `nafnet_sidd_w32.onnx.part` while hashing, verifying, then renaming into place.
/// `on_progress(downloaded, total)` is called per chunk (the caller throttles what it emits).
///
/// **Resumable**: a `.part` left by a dropped connection or a cancel is continued with a `Range` request (its
/// bytes are re-hashed first); a server that ignores the range restarts the file. Only a checksum failure
/// removes the partial file (so a damaged partial costs one retry). `cancel` is polled before the request and after each chunk.
pub async fn ensure_model(dir: &Path, cancel: &AtomicBool, on_progress: impl FnMut(u64, u64)) -> Result<PathBuf, DenoiseModelError> {
    fetch(&url(), dir, SHA256, SIZE, cancel, on_progress).await
}

async fn fetch(
    url: &str,
    dir: &Path,
    sha256: &str,
    size: u64,
    cancel: &AtomicBool,
    mut on_progress: impl FnMut(u64, u64),
) -> Result<PathBuf, DenoiseModelError> {
    let dest = dir.join(FILENAME);
    if is_valid_with(&dest, sha256, size) {
        return Ok(dest);
    }
    std::fs::create_dir_all(dir).map_err(|e| DenoiseModelError::CreateDir(dir.to_path_buf(), e))?;
    if cancel.load(Ordering::Relaxed) {
        return Err(DenoiseModelError::Cancelled);
    }
    let tmp = dir.join(format!("{FILENAME}.part"));
    // A partial file at or past the full size is useless (a complete one would have been renamed): start over.
    let have = std::fs::metadata(&tmp).map(|m| m.len()).unwrap_or(0).min(size);
    let have = if have == size { 0 } else { have };
    let mut request = reqwest::Client::new().get(url);
    if have > 0 {
        request = request.header(reqwest::header::RANGE, format!("bytes={have}-"));
    }
    let mut response = request.send().await.map_err(DenoiseModelError::Download)?;
    let status = response.status().as_u16();
    // 206 continues the file; 200 is the whole file again (range ignored); anything else is a failure.
    let resumed = have > 0 && status == 206;
    if !(status == 200 || resumed) {
        if have > 0 && status == 416 {
            let _ = std::fs::remove_file(&tmp);
        }
        return Err(DenoiseModelError::HttpStatus(status));
    }
    let mut hasher = Sha256::new();
    let mut done = 0u64;
    let mut file = if resumed {
        let mut prefix = std::fs::File::open(&tmp).map_err(|e| DenoiseModelError::Read(tmp.clone(), e))?;
        let mut buf = vec![0u8; 1 << 20];
        let mut left = have;
        while left > 0 {
            let want = left.min(buf.len() as u64) as usize;
            let n = std::io::Read::read(&mut prefix, &mut buf[..want]).map_err(|e| DenoiseModelError::Read(tmp.clone(), e))?;
            if n == 0 {
                return Err(DenoiseModelError::Read(tmp.clone(), std::io::ErrorKind::UnexpectedEof.into()));
            }
            hasher.update(&buf[..n]);
            left -= n as u64;
        }
        done = have;
        // Append after the hashed bytes. A damaged partial is caught by the final checksum, which removes it.
        std::fs::OpenOptions::new().append(true).open(&tmp).map_err(|e| DenoiseModelError::Write(tmp.clone(), e))?
    } else {
        std::fs::File::create(&tmp).map_err(|e| DenoiseModelError::Write(tmp.clone(), e))?
    };
    while let Some(chunk) = response.chunk().await.map_err(DenoiseModelError::Download)? {
        hasher.update(&chunk);
        file.write_all(&chunk).map_err(|e| DenoiseModelError::Write(tmp.clone(), e))?;
        done += chunk.len() as u64;
        on_progress(done, size);
        if cancel.load(Ordering::Relaxed) {
            return Err(DenoiseModelError::Cancelled); // the partial file stays, to be resumed
        }
    }
    drop(file);
    let actual = hex(&hasher.finalize());
    if actual != sha256 {
        let _ = std::fs::remove_file(&tmp);
        return Err(DenoiseModelError::ChecksumMismatch(FILENAME.to_string(), actual));
    }
    std::fs::rename(&tmp, &dest).map_err(|e| DenoiseModelError::Write(dest.clone(), e))?;
    Ok(dest)
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
        assert_eq!(status(&dir).download_bytes, SIZE);
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

    #[tokio::test]
    async fn a_download_cancelled_before_it_starts_makes_no_request_and_leaves_nothing_behind() {
        let dir = test_dir("cancelled");
        let cancel = AtomicBool::new(true);
        let err = ensure_model(&dir, &cancel, |_, _| panic!("no progress without a request")).await.unwrap_err();
        assert!(matches!(err, DenoiseModelError::Cancelled), "{err}");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
    }

    /// A one-file HTTP/1.1 server on localhost: serves `body`, honouring `Range: bytes=N-` unless `ignore_range`,
    /// and records the Range header of each request.
    fn serve(body: Vec<u8>, ignore_range: bool, requests: u32) -> (String, std::sync::Arc<std::sync::Mutex<Vec<Option<String>>>>) {
        use std::io::Read;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/file", listener.local_addr().unwrap());
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let log = seen.clone();
        std::thread::spawn(move || {
            for _ in 0..requests {
                let (mut conn, _) = listener.accept().unwrap();
                let mut head = Vec::new();
                let mut b = [0u8; 1];
                while !head.ends_with(b"\r\n\r\n") {
                    if conn.read(&mut b).unwrap() == 0 {
                        break;
                    }
                    head.push(b[0]);
                }
                let text = String::from_utf8_lossy(&head).to_string();
                let range = text.lines().find_map(|l| l.to_ascii_lowercase().strip_prefix("range: bytes=").map(|r| r.trim_end_matches('-').to_string()));
                log.lock().unwrap().push(range.clone());
                let (status, slice) = match range.and_then(|r| r.parse::<usize>().ok()).filter(|_| !ignore_range) {
                    Some(from) => ("206 Partial Content", &body[from..]),
                    None => ("200 OK", &body[..]),
                };
                let _ = write!(conn, "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", slice.len());
                let _ = conn.write_all(slice);
            }
        });
        (url, seen)
    }

    fn sample() -> (Vec<u8>, String) {
        let body: Vec<u8> = (0..300_000u32).map(|i| (i * 31 % 251) as u8).collect();
        let sha = hex(&Sha256::digest(&body));
        (body, sha)
    }

    #[tokio::test]
    async fn a_download_is_verified_and_a_partial_file_is_resumed_with_a_range_request() {
        let (body, sha) = sample();
        let size = body.len() as u64;
        // Fresh.
        let (url, seen) = serve(body.clone(), false, 1);
        let dir = test_dir("fetch-fresh");
        let mut last = 0;
        let path = fetch(&url, &dir, &sha, size, &AtomicBool::new(false), |d, t| {
            assert_eq!(t, size);
            last = d;
        })
        .await
        .unwrap();
        assert_eq!((last, std::fs::read(&path).unwrap()), (size, body.clone()));
        assert_eq!(seen.lock().unwrap().as_slice(), &[None]);
        assert!(!dir.join(format!("{FILENAME}.part")).exists());

        // Resume: the first 100 000 bytes are already there, as after a dropped connection.
        let (url, seen) = serve(body.clone(), false, 1);
        let dir = test_dir("fetch-resume");
        std::fs::write(dir.join(format!("{FILENAME}.part")), &body[..100_000]).unwrap();
        let mut first = None;
        let path = fetch(&url, &dir, &sha, size, &AtomicBool::new(false), |d, _| {
            first.get_or_insert(d);
        })
        .await
        .unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), body, "resumed file must equal the original");
        assert_eq!(seen.lock().unwrap().as_slice(), &[Some("100000".to_string())]);
        assert!(first.unwrap() > 100_000, "progress continues from the resumed offset");

        // A damaged partial is resumed, then fails the checksum and is removed (the retry starts clean).
        let (url, _) = serve(body.clone(), false, 1);
        let dir = test_dir("fetch-damaged-partial");
        let mut bad = body[..100_000].to_vec();
        bad[10] ^= 0xff;
        std::fs::write(dir.join(format!("{FILENAME}.part")), &bad).unwrap();
        let err = fetch(&url, &dir, &sha, size, &AtomicBool::new(false), |_, _| {}).await.unwrap_err();
        assert!(matches!(err, DenoiseModelError::ChecksumMismatch(..)), "{err}");
        assert!(!dir.join(format!("{FILENAME}.part")).exists());
    }

    #[tokio::test]
    async fn a_server_that_ignores_the_range_restarts_the_file_and_a_bad_body_is_rejected() {
        let (body, sha) = sample();
        let size = body.len() as u64;
        let (url, _) = serve(body.clone(), true, 1);
        let dir = test_dir("fetch-ignore-range");
        std::fs::write(dir.join(format!("{FILENAME}.part")), &body[..50_000]).unwrap();
        let path = fetch(&url, &dir, &sha, size, &AtomicBool::new(false), |_, _| {}).await.unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), body);

        // The wrong bytes under the right size: checksum failure, nothing kept.
        let mut wrong = body.clone();
        wrong[1000] ^= 1;
        let (url, _) = serve(wrong, false, 1);
        let dir = test_dir("fetch-wrong");
        let err = fetch(&url, &dir, &sha, size, &AtomicBool::new(false), |_, _| {}).await.unwrap_err();
        assert!(matches!(err, DenoiseModelError::ChecksumMismatch(..)), "{err}");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0, "a failed checksum leaves neither file");
    }

    #[test]
    fn the_download_url_names_the_pinned_release_asset_and_this_project() {
        let u = url();
        assert_eq!(u, "https://github.com/AlfredWei/emulsion/releases/download/denoise-model-nafnet-sidd-w32-v1/nafnet_sidd_w32.onnx");
        assert!(u.contains(&format!("//{HOST}/")));
    }

    /// Needs the real network (117 MB): `cargo test --lib -- --ignored denoise_models_live_fetch`. Proves the
    /// published release asset equals the pinned size and checksum.
    #[tokio::test]
    #[ignore]
    async fn denoise_models_live_fetch() {
        let dir = test_dir("live");
        let mut last = 0;
        let path = ensure_model(&dir, &AtomicBool::new(false), |d, _| last = d).await.expect("fetch");
        assert_eq!(last, SIZE);
        assert_eq!(status(&dir).state, ModelState::Ready);
        assert_eq!(path, dir.join(FILENAME));
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
