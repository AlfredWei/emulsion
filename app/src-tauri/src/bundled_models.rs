//! Model files shipped inside the installer (user decision, 2026-10-09): the denoise model and the two Select
//! Subject files, so both features work straight after installation with no download. The AI enlargement model
//! (4.8 MB, committed in `models/`) joined them on 2026-10-17 (user decision), and the person detector (79 MB, RFC-0037)
//! on 2026-10-31.
//!
//! A release build (`npm run build:release`) puts them in the app's resources under `models/`. They are **read in
//! place** from there (verified by size and SHA-256 on every lookup) and never copied, so a store build in a sandbox
//! container holds one copy of each file. The cache-root model folders keep only what a person downloaded or imported.
//! A copy an earlier build left in a cache folder stays where it is and is used only when the bundle has no valid
//! file of that name. A debug or CI build has no `models/` resource, and a debug build also reads the checkout's `models/`.

use crate::segment_models::sha256_file;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
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

/// The person detector (RFC-0037), installed beside the Select Subject files: the detector's boxes are cut by that model.
pub const DETECT: [Shipped; 1] = [Shipped { name: crate::detect_models::FILENAME, size: crate::detect_models::SIZE, sha256: crate::detect_models::SHA256 }];

/// The object-removal model (RFC-0032, ADR-0010), installed into its own folder.
pub const INPAINT: [Shipped; 1] = [Shipped { name: crate::inpaint_models::FILENAME, size: crate::inpaint_models::SIZE, sha256: crate::inpaint_models::SHA256 }];

/// The face-mesh model for blink evidence (RFC-0031 §8.15), installed into its own folder.
pub const BLINK: [Shipped; 1] = [Shipped { name: crate::blink_models::FILENAME, size: crate::blink_models::SIZE, sha256: crate::blink_models::SHA256 }];

pub const UPSCALE: [Shipped; 1] = [Shipped { name: crate::upscale_models::FILENAME, size: crate::upscale_models::SIZE, sha256: crate::upscale_models::SHA256 }];

/// The folders that may carry this build's models, best first: the `models/` folder of the app's resources, and in a
/// development build also the source checkout's `models/`. A development build used to take the first one only, so a
/// stale `target/debug/models` left by an earlier bundled build hid the checkout's files (the large removal model is
/// copied into the checkout's `models/` by hand, never committed). Release builds never look in the checkout.
pub fn bundled_dirs(app: &AppHandle) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = app.path().resource_dir().ok().map(|d| d.join("models")).into_iter().filter(|d| d.is_dir()).collect();
    #[cfg(debug_assertions)]
    {
        let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../models");
        if source.is_dir() {
            dirs.push(source);
        }
    }
    dirs
}

/// Where this build's copy of a file the repository root ships as a bundled resource is (`tauri.conf.json`
/// `bundle.resources`): next to the app in its resources, and in a development build also the checkout's copy.
/// `None` when neither exists.
fn resource_path(app: &AppHandle, name: &str) -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = app.path().resource_dir().ok().map(|d| d.join(name)).into_iter().collect();
    #[cfg(debug_assertions)]
    candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").join(name));
    first_existing(&candidates)
}

/// This build's `THIRD_PARTY_NOTICES.md` (the AI models' licences).
pub fn notices_path(app: &AppHandle) -> Option<PathBuf> {
    resource_path(app, NOTICES_FILE)
}

/// This build's `THIRD_PARTY_LIBRARIES.md` (the code libraries' licences; the texts it names are in `licenses/` beside it).
pub fn libraries_path(app: &AppHandle) -> Option<PathBuf> {
    resource_path(app, LIBRARIES_FILE)
}

const NOTICES_FILE: &str = "THIRD_PARTY_NOTICES.md";
const LIBRARIES_FILE: &str = "THIRD_PARTY_LIBRARIES.md";

fn first_existing(candidates: &[PathBuf]) -> Option<PathBuf> {
    candidates.iter().find(|p| p.is_file()).cloned()
}

fn valid(path: &Path, f: &Shipped) -> bool {
    matches!(std::fs::metadata(path), Ok(m) if m.len() == f.size) && sha256_file(path).map(|h| h == f.sha256).unwrap_or(false)
}

static BUNDLE: OnceLock<Vec<PathBuf>> = OnceLock::new();

#[cfg(test)]
thread_local! {
    // A test names its own bundle here because the process-wide one is set once and shared by every test thread.
    static TEST_BUNDLE: std::cell::RefCell<Option<Vec<PathBuf>>> = const { std::cell::RefCell::new(None) };
}

/// Records where this build's bundled models are. Called once at startup, before any model lookup.
pub fn init(app: &AppHandle) {
    let _ = BUNDLE.set(bundled_dirs(app));
}

fn bundle() -> Vec<PathBuf> {
    #[cfg(test)]
    if let Some(dirs) = TEST_BUNDLE.with(|b| b.borrow().clone()) {
        return dirs;
    }
    BUNDLE.get().cloned().unwrap_or_default()
}

/// The verified file for `f`. A valid copy in the app bundle wins and is never copied out. Otherwise a valid copy in
/// `cache_dir` (downloaded, imported, or left by an earlier build) is used. Callers check the edition first.
pub fn resolve(cache_dir: &Path, f: &Shipped) -> Option<PathBuf> {
    resolve_in(&bundle(), cache_dir, f)
}

fn resolve_in(bundles: &[PathBuf], cache_dir: &Path, f: &Shipped) -> Option<PathBuf> {
    bundles.iter().map(|d| d.join(f.name)).chain(std::iter::once(cache_dir.join(f.name))).find(|p| valid(p, f))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editions::{Edition, ModelFamily};

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("emulsion-bundled-models-{name}"));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    // SHA-256("abc"), the standard test vector.
    const ABC: Shipped = Shipped { name: "abc.onnx", size: 3, sha256: "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad" };

    /// A model row in the notices must also say what the model was trained on and under what terms (the last column),
    /// so a model cannot be added with only its weights licence recorded. The text is a research record, not a
    /// legal conclusion, but an empty cell is always wrong.
    #[test]
    fn every_model_row_in_the_third_party_notices_states_its_training_data() {
        let notices = include_str!("../../../THIRD_PARTY_NOTICES.md");
        let rows: Vec<&str> = notices.lines().filter(|l| l.starts_with("| ") && l.contains(".onnx")).collect();
        assert!(rows.len() >= 8, "expected at least the eight model rows, found {}", rows.len());
        for row in rows {
            let cells: Vec<&str> = row.trim().trim_matches('|').split('|').map(str::trim).collect();
            assert_eq!(cells.len(), 5, "a model row needs five columns (model, use, source, licence, training data): {row}");
            assert!(cells[4].len() > 10, "empty training-data cell in: {row}");
        }
    }

    /// The licence texts that carry conditions beyond "keep the notice" (LibRaw, the `lensfun` crate and its database,
    /// MPL-2.0, CDLA-Permissive-2.0), and the notices every other crate asks for, are copied by `scripts/gen_third_party_libraries.py` into `licenses/`, and
    /// `tauri.conf.json` ships that folder. A file that goes missing, or a folder that stops being bundled, fails here.
    #[test]
    fn the_licence_texts_are_in_the_repo_and_bundled_with_the_installer() {
        let conf = include_str!("../tauri.conf.json");
        assert!(conf.contains("\"../../licenses/\": \"licenses/\""), "tauri.conf.json must bundle the licenses/ folder");
        let libs = include_str!("../../../THIRD_PARTY_LIBRARIES.md");
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../licenses");
        let files = [
            "LibRaw-COPYRIGHT", "LibRaw-LICENSE.LGPL", "LibRaw-LICENSE.CDDL", "lensfun-LICENSE-LGPL-3.0", "lensfun-LICENSE-GPL-3.0",
            "lensfun-NOTICE", "CC-BY-SA-3.0.txt", "MPL-2.0.txt", "CDLA-Permissive-2.0.txt", "README.txt",
            "vcpkg-jasper-copyright.txt", "vcpkg-lcms-copyright.txt", "vcpkg-libjpeg-turbo-copyright.txt", "vcpkg-zlib-copyright.txt",
            "rust-crates.txt", "LibRaw-third-party-BSD.txt", "IJG-notice.txt",
        ];
        for f in files {
            let len = std::fs::metadata(dir.join(f)).unwrap_or_else(|e| panic!("licenses/{f}: {e}")).len();
            assert!(len > 200, "licenses/{f} is empty or truncated");
            // README.txt is the only one that names the others; every text must be named in the library list's table or the README.
            if f != "README.txt" {
                let readme = std::fs::read_to_string(dir.join("README.txt")).unwrap();
                assert!(readme.contains(f) || libs.contains(f), "licenses/{f} is not named in README.txt or THIRD_PARTY_LIBRARIES.md");
            }
        }
        for name in ["LibRaw", "`lensfun` crate", "Lensfun lens database", "jasper", "lcms", "libjpeg-turbo", "zlib"] {
            assert!(libs.contains(name), "THIRD_PARTY_LIBRARIES.md does not name {name}");
        }
        assert!(libs.contains("licenses/"), "THIRD_PARTY_LIBRARIES.md must say where the texts are");
        assert!(libs.contains("Emulsion uses LibRaw 0.21.3 under the CDDL-1.0"), "THIRD_PARTY_LIBRARIES.md must state the LibRaw election");
    }

    #[test]
    fn notices_lookup_takes_the_first_candidate_that_is_a_file() {
        let d = dir("notices");
        let present = d.join(NOTICES_FILE);
        std::fs::write(&present, "x").unwrap();
        // A directory of the same name, a missing path, then the real file.
        let candidates = [d.join("nope"), d.clone(), present.clone()];
        assert_eq!(first_existing(&candidates), Some(present));
        assert_eq!(first_existing(&[d.join("nope")]), None);
    }

    /// Every model file the installer ships must be named in THIRD_PARTY_NOTICES.md (which the installer also ships),
    /// so a newly bundled model cannot go out without its licence being stated for recipients.
    #[test]
    fn every_bundled_model_has_an_entry_in_the_third_party_notices() {
        let notices = include_str!("../../../THIRD_PARTY_NOTICES.md");
        let manifest: serde_json::Value = serde_json::from_str(include_str!("../../../models/bundled-models.json")).unwrap();
        let files = manifest["files"].as_array().unwrap();
        assert!(!files.is_empty());
        for f in files {
            let name = f["name"].as_str().unwrap();
            // SAM 2's two files are one entry, named by their shared stem.
            let stem = name.strip_suffix(".encoder.onnx").or_else(|| name.strip_suffix(".decoder.onnx")).map_or_else(|| name.to_string(), |s| format!("{s}.*.onnx"));
            assert!(notices.contains(&stem), "THIRD_PARTY_NOTICES.md has no entry for {name} (looked for `{stem}`)");
        }
    }

    fn with_bundle<T>(dirs: Vec<PathBuf>, f: impl FnOnce() -> T) -> T {
        TEST_BUNDLE.with(|b| *b.borrow_mut() = Some(dirs));
        let out = f();
        TEST_BUNDLE.with(|b| *b.borrow_mut() = None);
        out
    }

    fn committed_models() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../models")
    }

    #[test]
    fn a_valid_bundled_file_is_returned_from_the_bundle_and_nothing_is_copied() {
        let (bundled, cache) = (dir("a-bundled"), dir("a-cache"));
        std::fs::write(bundled.join("abc.onnx"), b"abc").unwrap();
        assert_eq!(resolve_in(&[bundled.clone()], &cache, &ABC), Some(bundled.join("abc.onnx")));
        assert!(std::fs::read_dir(&cache).unwrap().next().is_none());
    }

    #[test]
    fn the_bundle_is_tried_before_the_cache_and_the_cache_only_when_the_bundle_has_no_valid_file() {
        let (bundled, cache) = (dir("b-bundled"), dir("b-cache"));
        std::fs::write(cache.join("abc.onnx"), b"abc").unwrap();
        assert_eq!(resolve_in(&[bundled.clone()], &cache, &ABC), Some(cache.join("abc.onnx")), "bundle empty");
        std::fs::write(bundled.join("abc.onnx"), b"abd").unwrap();
        assert_eq!(resolve_in(&[bundled.clone()], &cache, &ABC), Some(cache.join("abc.onnx")), "bundled copy damaged");
        std::fs::write(bundled.join("abc.onnx"), b"abc").unwrap();
        assert_eq!(resolve_in(&[bundled.clone()], &cache, &ABC), Some(bundled.join("abc.onnx")), "bundle first");
    }

    #[test]
    fn a_damaged_or_missing_file_everywhere_resolves_to_nothing() {
        let (bundled, cache) = (dir("c-bundled"), dir("c-cache"));
        assert_eq!(resolve_in(&[bundled.clone()], &cache, &ABC), None);
        std::fs::write(bundled.join("abc.onnx"), b"abd").unwrap();
        std::fs::write(cache.join("abc.onnx"), b"xyz").unwrap();
        assert_eq!(resolve_in(&[bundled], &cache, &ABC), None);
    }

    #[test]
    fn a_family_the_edition_leaves_out_refuses_even_when_the_bundle_holds_its_file() {
        let (all, none_blink) = (Edition::all("default"), {
            let mut e = Edition::all("test-no-blink");
            e.include.remove(&ModelFamily::Blink);
            e
        });
        let mut none_upscale = Edition::all("test-no-upscale");
        none_upscale.include.remove(&ModelFamily::Upscale);
        let cache = dir("d-cache");
        with_bundle(vec![committed_models()], || {
            let blink = crate::blink_models::ready_path_for(&all, &cache).expect("the committed blink model resolves from the bundle");
            assert!(blink.starts_with(committed_models()));
            assert!(crate::blink_models::ready_path_for(&none_blink, &cache).is_none());
            assert!(crate::upscale_models::ready_path_for(&all, &cache).is_some());
            assert!(crate::upscale_models::ready_path_for(&none_upscale, &cache).is_none());
            assert_eq!(crate::upscale_models::status_for(&none_upscale, &cache).state, crate::upscale_models::ModelState::NotInEdition);
        });
        assert!(std::fs::read_dir(&cache).unwrap().next().is_none(), "reading in place copies nothing");
    }

    #[test]
    fn the_bundle_manifest_matches_the_constants_the_app_verifies_models_with() {
        let manifest: serde_json::Value = serde_json::from_str(include_str!("../../../models/bundled-models.json")).unwrap();
        let listed: Vec<(String, u64, String, String, Option<String>)> = manifest["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| (f["name"].as_str().unwrap().into(), f["size"].as_u64().unwrap(), f["sha256"].as_str().unwrap().into(), f["url"].as_str().unwrap_or("").to_string(), f["path"].as_str().map(String::from)))
            .collect();
        let known: Vec<(&Shipped, ModelFamily)> = DENOISE
            .iter()
            .map(|k| (k, crate::denoise_models::FAMILY))
            .chain(SEGMENT.iter().map(|k| (k, crate::segment_models::FAMILY)))
            .chain(DETECT.iter().map(|k| (k, crate::detect_models::FAMILY)))
            .chain(UPSCALE.iter().map(|k| (k, crate::upscale_models::FAMILY)))
            .chain(INPAINT.iter().map(|k| (k, crate::inpaint_models::FAMILY)))
            .chain(BLINK.iter().map(|k| (k, crate::blink_models::FAMILY)))
            .collect();
        assert_eq!(listed.len(), known.len());
        for (name, size, sha, url, path) in &listed {
            let (k, family_of_k) = known.iter().find(|(k, _)| k.name == name).unwrap_or_else(|| panic!("{name} is not a model the app verifies"));
            assert_eq!((k.size, k.sha256), (*size, sha.as_str()), "{name}");
            let family = manifest["files"].as_array().unwrap().iter().find(|f| f["name"] == name.as_str()).unwrap()["family"].clone();
            assert_eq!(serde_json::to_value(family_of_k).unwrap(), family, "{name}: the manifest's `family` and the module's FAMILY differ");
            match path {
                // A small model committed in models/: the file must be there and be the one the app verifies.
                Some(rel) => {
                    let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../models").join(rel);
                    assert!(valid(&file, k), "{rel} is missing from models/ or does not match its pin");
                }
                None => assert!(url.starts_with("https://") && url.ends_with(name.as_str()), "{url}"),
            }
        }
    }
}
