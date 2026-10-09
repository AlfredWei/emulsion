//! Model files shipped inside the installer (user decision, 2026-10-09): the denoise model and the two Select
//! Subject files, so both features work straight after installation with no download.
//!
//! A release build (`npm run build:release`) puts them in the app's resources under `models/`. On first use they
//! are **copied into the same model directories the downloads use** (verified by size and SHA-256 on the way), so
//! everything downstream (`ready_path`, `ready_paths`, the helper) is unchanged and a later downloaded or
//! updated model simply replaces the file there. The cost is a second copy on disk (about 272 MB); the benefit is
//! no second code path for "where is the model". A debug or CI build has no `models/` resource and does nothing.

use crate::segment_models::sha256_file;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

/// A model file the installer may carry, as the app verifies it.
pub struct Shipped {
    pub name: &'static str,
    pub size: u64,
    pub sha256: &'static str,
}

pub const DENOISE: [Shipped; 1] = [Shipped { name: crate::denoise_models::FILENAME, size: crate::denoise_models::SIZE, sha256: crate::denoise_models::SHA256 }];

pub const SEGMENT: [Shipped; 2] = [
    Shipped { name: crate::segment_models::ENCODER.filename, size: crate::segment_models::ENCODER.size, sha256: crate::segment_models::ENCODER.sha256 },
    Shipped { name: crate::segment_models::DECODER.filename, size: crate::segment_models::DECODER.size, sha256: crate::segment_models::DECODER.sha256 },
];

/// One seeding at a time: the startup pass and a status check must not both write a `.part` file, and a status
/// check that arrives mid-copy waits for it rather than reporting "missing" and opening the download dialog.
static SEED_LOCK: Mutex<()> = Mutex::new(());

/// The `models/` folder of the app's resources, if this build carries one.
pub fn bundled_dir(app: &AppHandle) -> Option<PathBuf> {
    app.path().resource_dir().ok().map(|d| d.join("models")).filter(|d| d.is_dir())
}

fn valid(path: &Path, f: &Shipped) -> bool {
    matches!(std::fs::metadata(path), Ok(m) if m.len() == f.size) && sha256_file(path).map(|h| h == f.sha256).unwrap_or(false)
}

/// Copies each of `wanted` that `bundled` has and `dest_dir` lacks (or has damaged), verified before it is kept.
/// Returns how many were copied. A bundled file that fails verification is an error and leaves nothing behind.
pub fn seed(dest_dir: &Path, bundled: &Path, wanted: &[Shipped]) -> Result<usize, String> {
    let _guard = SEED_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let mut copied = 0;
    for f in wanted {
        let dest = dest_dir.join(f.name);
        let src = bundled.join(f.name);
        if !src.is_file() || valid(&dest, f) {
            continue;
        }
        std::fs::create_dir_all(dest_dir).map_err(|e| format!("could not create {}: {e}", dest_dir.display()))?;
        let part = dest_dir.join(format!("{}.part", f.name));
        std::fs::copy(&src, &part).map_err(|e| format!("could not copy {}: {e}", src.display()))?;
        if !valid(&part, f) {
            let _ = std::fs::remove_file(&part);
            return Err(format!("the bundled {} failed verification", f.name));
        }
        std::fs::rename(&part, &dest).map_err(|e| {
            let _ = std::fs::remove_file(&part);
            format!("could not install {}: {e}", f.name)
        })?;
        copied += 1;
    }
    Ok(copied)
}

/// Seeds one family from this build's bundle, if it has one. Failures are logged, not raised: the download path
/// still works, and a status check must never fail because a bundled copy could not be installed.
pub fn seed_from_app(app: &AppHandle, dest_dir: &Path, wanted: &[Shipped]) {
    let Some(bundled) = bundled_dir(app) else { return };
    if let Err(e) = seed(dest_dir, &bundled, wanted) {
        eprintln!("bundled model seeding: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("emulsion-bundled-models-{name}"));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    // SHA-256("abc"), the standard test vector.
    const ABC: Shipped = Shipped { name: "abc.onnx", size: 3, sha256: "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad" };

    #[test]
    fn a_missing_file_is_copied_verified_and_a_valid_one_is_left_alone() {
        let (bundled, dest) = (dir("a-bundled"), dir("a-dest").join("models"));
        std::fs::write(bundled.join("abc.onnx"), b"abc").unwrap();
        assert_eq!(seed(&dest, &bundled, &[ABC]).unwrap(), 1);
        assert_eq!(std::fs::read(dest.join("abc.onnx")).unwrap(), b"abc");
        assert!(!dest.join("abc.onnx.part").exists());
        assert_eq!(seed(&dest, &bundled, &[ABC]).unwrap(), 0, "already installed and valid");
    }

    #[test]
    fn a_damaged_installed_copy_is_replaced_from_the_bundle() {
        let (bundled, dest) = (dir("b-bundled"), dir("b-dest"));
        std::fs::write(bundled.join("abc.onnx"), b"abc").unwrap();
        std::fs::write(dest.join("abc.onnx"), b"xyz").unwrap();
        assert_eq!(seed(&dest, &bundled, &[ABC]).unwrap(), 1);
        assert_eq!(std::fs::read(dest.join("abc.onnx")).unwrap(), b"abc");
    }

    #[test]
    fn a_bundled_file_that_fails_verification_is_refused_and_leaves_nothing() {
        let (bundled, dest) = (dir("c-bundled"), dir("c-dest"));
        std::fs::write(bundled.join("abc.onnx"), b"abd").unwrap();
        assert!(seed(&dest, &bundled, &[ABC]).unwrap_err().contains("failed verification"));
        assert!(!dest.join("abc.onnx").exists() && !dest.join("abc.onnx.part").exists());
    }

    #[test]
    fn a_build_without_the_file_does_nothing() {
        let (bundled, dest) = (dir("d-bundled"), dir("d-dest"));
        assert_eq!(seed(&dest, &bundled, &[ABC]).unwrap(), 0);
        assert!(std::fs::read_dir(&dest).unwrap().next().is_none());
    }

    #[test]
    fn the_bundle_manifest_matches_the_constants_the_app_verifies_models_with() {
        let manifest: serde_json::Value = serde_json::from_str(include_str!("../../../models/bundled-models.json")).unwrap();
        let listed: Vec<(String, u64, String, String)> = manifest["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| (f["name"].as_str().unwrap().into(), f["size"].as_u64().unwrap(), f["sha256"].as_str().unwrap().into(), f["url"].as_str().unwrap().into()))
            .collect();
        let known: Vec<&Shipped> = DENOISE.iter().chain(SEGMENT.iter()).collect();
        assert_eq!(listed.len(), known.len());
        for (name, size, sha, url) in &listed {
            let k = known.iter().find(|k| k.name == name).unwrap_or_else(|| panic!("{name} is not a model the app verifies"));
            assert_eq!((k.size, k.sha256), (*size, sha.as_str()), "{name}");
            assert!(url.starts_with("https://") && url.ends_with(name.as_str()), "{url}");
        }
    }
}
