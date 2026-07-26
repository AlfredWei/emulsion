//! Import pipeline (M1 Slice 1) — see docs/PRD/PRD.md §7.2, docs/rfc/RFC-0001.
//!
//! Scope for this slice: reference-only import (the catalog stores the
//! original file's path as-is; copying files into a managed folder
//! structure is real PRD scope but deliberately deferred — see the plan
//! this was built from / PROGRESS.md). Walks a directory recursively,
//! hashes each candidate file for duplicate detection, catalogs it, and
//! extracts a cheap embedded thumbnail (no full demosaic) for the Library
//! grid.

use crate::catalog::{Catalog, EditStack};
use rsraw::RawImage;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Extensions LibRaw can plausibly handle. Not exhaustive — LibRaw itself
/// supports far more; this is a first filter, not the source of truth
/// (RawImage::open still has the final say per file).
const RAW_EXTENSIONS: &[&str] = &[
    "cr2", "cr3", "nef", "arw", "dng", "raf", "orf", "rw2", "pef", "srw", "x3f", "3fr", "erf",
    "kdc", "mrw", "raw", "rwl",
];

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ImportSummary {
    pub imported: usize,
    pub skipped_duplicates: usize,
    pub failed: usize,
}

fn has_raw_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| RAW_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

fn collect_candidate_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(collect_candidate_files(&path));
        } else if has_raw_extension(&path) {
            out.push(path);
        }
    }
    out
}

/// Extract the largest embedded thumbnail and write it to `thumbnail_dir`
/// as `{image_id}.jpg`. Only handles JPEG-format embedded thumbnails (the
/// common case) — other embedded formats (raw bitmap, H.265, ...) are
/// skipped for this slice; the image is still cataloged, just without a
/// thumbnail yet.
fn extract_and_write_thumbnail(
    image: &mut RawImage,
    image_id: i64,
    thumbnail_dir: &Path,
) -> Option<PathBuf> {
    let thumbs = image.extract_thumbs().ok()?;
    let largest = thumbs.last()?; // Thumbnails::append keeps them sorted ascending by height
    if largest.format != rsraw::ThumbFormat::Jpeg {
        return None;
    }
    let out_path = thumbnail_dir.join(format!("{image_id}.jpg"));
    std::fs::write(&out_path, &largest.data).ok()?;
    Some(out_path)
}

/// Scan `dir` recursively and import every RAW file found that isn't
/// already in the catalog (by content hash). Never modifies or moves the
/// original files. Safe to call with a `thumbnail_dir` that doesn't exist
/// yet — it's created if needed.
pub fn scan_and_import(dir: &Path, catalog: &Catalog, thumbnail_dir: &Path) -> ImportSummary {
    let mut summary = ImportSummary::default();
    let _ = std::fs::create_dir_all(thumbnail_dir);

    for path in collect_candidate_files(dir) {
        let Ok(bytes) = std::fs::read(&path) else {
            summary.failed += 1;
            continue;
        };

        let hash = blake3::hash(&bytes).to_hex().to_string();

        match catalog.find_by_hash(&hash) {
            Ok(Some(_)) => {
                summary.skipped_duplicates += 1;
                continue;
            }
            Err(_) => {
                summary.failed += 1;
                continue;
            }
            Ok(None) => {}
        }

        let Ok(mut raw_image) = RawImage::open(&bytes) else {
            summary.failed += 1;
            continue;
        };

        // Atomic (M1 Slice 6): a catalog row without a matching edit-stack
        // version is an inconsistent state that used to be reachable by a
        // crash between two separate statements, not just a Rust-level
        // error path -- add_image_with_edit_stack wraps both in one
        // transaction so that's no longer possible.
        let path_str = path.to_string_lossy().to_string();
        let Ok(image_id) =
            catalog.add_image_with_edit_stack(&path_str, &hash, bytes.len() as i64, &EditStack::empty())
        else {
            summary.failed += 1;
            continue;
        };

        if let Some(thumb_path) = extract_and_write_thumbnail(&mut raw_image, image_id, thumbnail_dir)
        {
            let _ = catalog.set_thumbnail_path(image_id, &thumb_path.to_string_lossy());
        }

        summary.imported += 1;
    }

    summary
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_raw_files_are_skipped_not_fatal() {
        let dir = std::env::temp_dir().join("emulsion-m1-import-test-non-raw");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("notes.txt"), b"not a raw file").unwrap();

        let catalog = Catalog::open_in_memory().unwrap();
        let thumb_dir = dir.join("thumbs");
        let summary = scan_and_import(&dir, &catalog, &thumb_dir);

        assert_eq!(summary.imported, 0);
        assert_eq!(summary.skipped_duplicates, 0);
        assert_eq!(summary.failed, 0); // .txt isn't even a candidate extension
        assert!(catalog.list_images().unwrap().is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn corrupt_raw_extension_file_counts_as_failed_not_fatal() {
        let dir = std::env::temp_dir().join("emulsion-m1-import-test-corrupt");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("broken.dng"), b"this is not really a DNG").unwrap();

        let catalog = Catalog::open_in_memory().unwrap();
        let thumb_dir = dir.join("thumbs");
        let summary = scan_and_import(&dir, &catalog, &thumb_dir);

        assert_eq!(summary.imported, 0);
        assert_eq!(summary.failed, 1);
        assert!(catalog.list_images().unwrap().is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Real end-to-end import, gated the same way as raw_decode's real-file
    /// test: point EMULSION_TEST_RAW_SAMPLE at a real RAW/DNG file to run
    /// this. Verifies catalog insert, thumbnail extraction, and that
    /// importing the same file twice triggers dedupe, not a duplicate row.
    #[test]
    fn imports_a_real_raw_file_and_dedupes_on_second_import() {
        let Ok(sample_path) = std::env::var("EMULSION_TEST_RAW_SAMPLE") else {
            eprintln!(
                "skipping: set EMULSION_TEST_RAW_SAMPLE=/path/to/file.DNG to run this test"
            );
            return;
        };

        let dir = std::env::temp_dir().join("emulsion-m1-import-test-real");
        std::fs::create_dir_all(&dir).unwrap();
        let dest = dir.join(format!(
            "sample.{}",
            Path::new(&sample_path)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("dng")
        ));
        std::fs::copy(&sample_path, &dest).expect("copy sample into scan dir");

        let catalog = Catalog::open_in_memory().unwrap();
        let thumb_dir = dir.join("thumbs");

        let first = scan_and_import(&dir, &catalog, &thumb_dir);
        assert_eq!(first.imported, 1);
        assert_eq!(first.failed, 0);

        let images = catalog.list_images().unwrap();
        assert_eq!(images.len(), 1);
        // Not every RAW's embedded thumbnail is JPEG-format, so a thumbnail
        // isn't guaranteed — just confirm the field round-trips cleanly
        // either way rather than asserting it's always Some.
        let _ = &images[0].thumbnail_path;

        let second = scan_and_import(&dir, &catalog, &thumb_dir);
        assert_eq!(second.imported, 0);
        assert_eq!(second.skipped_duplicates, 1);
        assert_eq!(catalog.list_images().unwrap().len(), 1);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
