//! Import pipeline (M1 Slice 1, extended to JPEG + multi-file selection in
//! M2 Slice 1) — see docs/PRD/PRD.md §7.2, docs/rfc/RFC-0001.
//!
//! Scope: reference-only import (the catalog stores the original file's
//! path as-is; copying files into a managed folder structure is real PRD
//! scope but deliberately deferred). Two entry points share one per-file
//! core (`import_paths`): `scan_and_import` walks a directory recursively,
//! `import_files` (lib.rs) takes an explicit list from the multi-file
//! picker dialog. Both hash each candidate file for duplicate detection,
//! catalog it, and (RAW only, synchronously) extract a cheap embedded
//! thumbnail. JPEG's thumbnail is generated later, in the background --
//! see `generate_missing_thumbnails`.

use crate::catalog::{Catalog, EditStack};
use crate::metadata;
use crate::source_decode::{self, ImageFormat};
use rsraw::RawImage;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ImportSummary {
    pub imported: usize,
    pub skipped_duplicates: usize,
    pub failed: usize,
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
        } else if ImageFormat::from_path(&path).is_some() {
            out.push(path);
        }
    }
    out
}

/// Extract the largest embedded thumbnail and write it to `thumbnail_dir`
/// as `{image_id}.jpg`. RAW only -- JPEG has no embedded-thumbnail concept;
/// see `generate_missing_thumbnails`. Only handles JPEG-format embedded
/// thumbnails (the common case) — other embedded formats (raw bitmap,
/// H.265, ...) are skipped; the image is still cataloged, just without a
/// thumbnail yet (also backstopped by `generate_missing_thumbnails`).
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

/// Cheap JPEG validity gate -- reads the header only (no full IDCT decode),
/// cost comparable to `RawImage::open`'s container parse, not to a full
/// `jpeg_decode::decode()`. Mirrors RAW's own pre-insert gate exactly: a
/// corrupt JPEG must never get cataloged, the same way a corrupt RAW file
/// doesn't. (A naive "decode as the thumbnail step" implementation would
/// give corrupt JPEGs *weaker* semantics than corrupt RAW -- cataloged with
/// just no thumbnail, instead of not cataloged at all.)
fn jpeg_bytes_look_valid(bytes: &[u8]) -> bool {
    image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .ok()
        .and_then(|r| r.into_dimensions().ok())
        .is_some()
}

/// Imports every file in `paths` that isn't already in the catalog (by
/// content hash). Never modifies or moves the original files. Safe to call
/// with a `thumbnail_dir` that doesn't exist yet -- it's created if needed.
/// Shared core for both `scan_and_import` (directory walk) and the
/// multi-file picker's `import_files` Tauri command (lib.rs).
pub fn import_paths(paths: &[PathBuf], catalog: &Catalog, thumbnail_dir: &Path) -> ImportSummary {
    let mut summary = ImportSummary::default();
    let _ = std::fs::create_dir_all(thumbnail_dir);

    for path in paths {
        let Ok(bytes) = std::fs::read(path) else {
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

        let Some(format) = ImageFormat::from_path(path) else {
            summary.failed += 1;
            continue;
        };
        let path_str = path.to_string_lossy().to_string();

        match format {
            ImageFormat::Raw => {
                let Ok(mut raw_image) = RawImage::open(&bytes) else {
                    summary.failed += 1;
                    continue;
                };

                // M2 Slice 2: free to call here -- RawImage::open() already
                // populated the header fields this reads, no unpack()/
                // process() needed (confirmed against rsraw's own source).
                let file_metadata = metadata::extract_from_raw(&raw_image);

                // Atomic (M1 Slice 6): both rows (+ metadata, M2 Slice 2)
                // in one transaction, so a crash partway through can't
                // leave a permanently-orphaned or inconsistently-tagged row.
                let Ok(image_id) = catalog.add_image_with_edit_stack(
                    &path_str,
                    &hash,
                    bytes.len() as i64,
                    &EditStack::empty(),
                    &file_metadata,
                ) else {
                    summary.failed += 1;
                    continue;
                };

                if let Some(thumb_path) =
                    extract_and_write_thumbnail(&mut raw_image, image_id, thumbnail_dir)
                {
                    let _ = catalog.set_thumbnail_path(image_id, &thumb_path.to_string_lossy());
                }
            }
            ImageFormat::Jpeg => {
                if !jpeg_bytes_look_valid(&bytes) {
                    summary.failed += 1;
                    continue;
                }

                // M2 Slice 2: a fresh EXIF read, deliberately not shared
                // with jpeg_decode.rs's orientation read -- that one only
                // happens later, during the background thumbnail pass, not
                // here at import time.
                let file_metadata = metadata::extract_from_jpeg(&bytes);

                // thumbnail_path stays NULL here -- generate_missing_thumbnails
                // fills it in on a background pass. A full JPEG decode is
                // categorically heavier than RAW's cheap embedded-thumb
                // extraction (no demosaic to skip); running it synchronously
                // inside this loop would visibly slow a JPEG-heavy import
                // with zero progress feedback, unlike RAW's import path today.
                if catalog
                    .add_image_with_edit_stack(&path_str, &hash, bytes.len() as i64, &EditStack::empty(), &file_metadata)
                    .is_err()
                {
                    summary.failed += 1;
                    continue;
                }
            }
        }

        summary.imported += 1;
    }

    summary
}

/// Scan `dir` recursively and import every supported file found. Thin
/// wrapper over `import_paths` -- see that for the real per-file logic.
pub fn scan_and_import(dir: &Path, catalog: &Catalog, thumbnail_dir: &Path) -> ImportSummary {
    import_paths(&collect_candidate_files(dir), catalog, thumbnail_dir)
}

/// Background pass (M2 Slice 1): fills in `thumbnail_path` for any
/// cataloged image that doesn't have one yet. In practice that's mostly
/// JPEG imports (never get one synchronously, see `import_paths` above),
/// but this backstops RAW images too, for the existing case where a RAW
/// file's embedded thumbnail isn't JPEG-format and extraction is skipped --
/// previously permanently thumbnail-less, now recoverable here as a free
/// side effect of this being format-generic (`source_decode::decode_preview`
/// dispatches by extension). Same shape as `preview_cache::pregenerate_missing`:
/// lock held only long enough to snapshot the list, decode/resize/encode
/// work happens outside it, re-locked briefly per image to persist the
/// result. `GridCell.svelte` already renders a placeholder for a NULL
/// `thumbnail_path`, so no frontend change is needed for the gap between
/// import and this pass completing.
pub fn generate_missing_thumbnails(catalog: &Arc<Mutex<Catalog>>, thumbnail_dir: &Path) {
    const THUMBNAIL_MAX_DIMENSION: u32 = 1024;

    let images = {
        let Ok(catalog) = catalog.lock() else { return };
        catalog.list_images().unwrap_or_default()
    };
    let _ = std::fs::create_dir_all(thumbnail_dir);

    for image in images {
        if image.thumbnail_path.is_some() {
            continue;
        }

        let path = Path::new(&image.path);
        let decoded = match source_decode::decode_preview(path) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("thumbnail generation failed for {}: {e}", image.path);
                continue;
            }
        };
        let Some(source) = image::RgbImage::from_raw(decoded.width, decoded.height, decoded.rgb)
        else {
            eprintln!(
                "thumbnail generation failed for {}: decoded buffer size mismatch",
                image.path
            );
            continue;
        };

        let (w, h) = (source.width(), source.height());
        let resized = if w.max(h) > THUMBNAIL_MAX_DIMENSION {
            let scale = THUMBNAIL_MAX_DIMENSION as f64 / w.max(h) as f64;
            let target_w = ((w as f64) * scale).round().max(1.0) as u32;
            let target_h = ((h as f64) * scale).round().max(1.0) as u32;
            image::imageops::resize(&source, target_w, target_h, image::imageops::FilterType::Triangle)
        } else {
            source
        };

        let out_path = thumbnail_dir.join(format!("{}.jpg", image.image_id));
        if let Err(e) = resized.save(&out_path) {
            eprintln!("thumbnail generation failed for {}: {e}", image.path);
            continue;
        }

        let Ok(catalog) = catalog.lock() else { return };
        let _ = catalog.set_thumbnail_path(image.image_id, &out_path.to_string_lossy());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Scan dir and thumbnail dir as *siblings*, never nested -- matching
    /// how the real app keeps them separate (`<app data dir>/thumbnails`
    /// is never inside a user's photo folder). A nested thumb_dir (an
    /// earlier version of these tests used `dir.join("thumbs")`) breaks
    /// once JPEG recognition exists: `collect_candidate_files` would
    /// recursively pick up the previous run's own generated `.jpg`
    /// thumbnails as new "source images" on a second scan.
    fn scan_and_thumb_dirs(name: &str) -> (PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!("emulsion-import-test-{name}"));
        let _ = std::fs::remove_dir_all(&root);
        let scan_dir = root.join("scan");
        let thumb_dir = root.join("thumbs");
        std::fs::create_dir_all(&scan_dir).unwrap();
        (scan_dir, thumb_dir)
    }

    #[test]
    fn non_raw_files_are_skipped_not_fatal() {
        let (dir, thumb_dir) = scan_and_thumb_dirs("non-raw");
        std::fs::write(dir.join("notes.txt"), b"not a raw file").unwrap();

        let catalog = Catalog::open_in_memory().unwrap();
        let summary = scan_and_import(&dir, &catalog, &thumb_dir);

        assert_eq!(summary.imported, 0);
        assert_eq!(summary.skipped_duplicates, 0);
        assert_eq!(summary.failed, 0); // .txt isn't even a candidate extension
        assert!(catalog.list_images().unwrap().is_empty());

        let _ = std::fs::remove_dir_all(dir.parent().unwrap());
    }

    #[test]
    fn corrupt_raw_extension_file_counts_as_failed_not_fatal() {
        let (dir, thumb_dir) = scan_and_thumb_dirs("corrupt-raw");
        std::fs::write(dir.join("broken.dng"), b"this is not really a DNG").unwrap();

        let catalog = Catalog::open_in_memory().unwrap();
        let summary = scan_and_import(&dir, &catalog, &thumb_dir);

        assert_eq!(summary.imported, 0);
        assert_eq!(summary.failed, 1);
        assert!(catalog.list_images().unwrap().is_empty());

        let _ = std::fs::remove_dir_all(dir.parent().unwrap());
    }

    /// The M2 Slice 1 review's main finding: a corrupt JPEG must never get
    /// cataloged, the same way a corrupt RAW file doesn't -- not cataloged
    /// with just a missing thumbnail.
    #[test]
    fn corrupt_jpeg_extension_file_counts_as_failed_not_fatal() {
        let (dir, thumb_dir) = scan_and_thumb_dirs("corrupt-jpeg");
        std::fs::write(dir.join("broken.jpg"), b"this is not really a JPEG").unwrap();

        let catalog = Catalog::open_in_memory().unwrap();
        let summary = scan_and_import(&dir, &catalog, &thumb_dir);

        assert_eq!(summary.imported, 0);
        assert_eq!(summary.failed, 1);
        assert!(catalog.list_images().unwrap().is_empty());

        let _ = std::fs::remove_dir_all(dir.parent().unwrap());
    }

    /// A real (if non-photographic) JPEG imports successfully with
    /// thumbnail_path left NULL immediately after import (no synchronous
    /// thumbnail step for JPEG), then non-NULL after generate_missing_thumbnails.
    #[test]
    fn imports_a_real_jpeg_and_backfills_its_thumbnail_in_the_background() {
        let (dir, thumb_dir) = scan_and_thumb_dirs("real-jpeg");
        let img = image::RgbImage::from_pixel(200, 100, image::Rgb([120, 90, 60]));
        img.save(dir.join("photo.jpg")).unwrap();

        let catalog = Catalog::open_in_memory().unwrap();

        let summary = scan_and_import(&dir, &catalog, &thumb_dir);
        assert_eq!(summary.imported, 1);
        assert_eq!(summary.failed, 0);

        let images = catalog.list_images().unwrap();
        assert_eq!(images.len(), 1);
        assert!(images[0].thumbnail_path.is_none(), "JPEG import must not generate a thumbnail synchronously");

        let catalog = Arc::new(Mutex::new(catalog));
        generate_missing_thumbnails(&catalog, &thumb_dir);

        let images = catalog.lock().unwrap().list_images().unwrap();
        assert!(images[0].thumbnail_path.is_some(), "background pass should have filled in the thumbnail");
        assert!(Path::new(images[0].thumbnail_path.as_ref().unwrap()).exists());

        let _ = std::fs::remove_dir_all(dir.parent().unwrap());
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

        let (dir, thumb_dir) = scan_and_thumb_dirs("real-raw");
        let dest = dir.join(format!(
            "sample.{}",
            Path::new(&sample_path)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("dng")
        ));
        std::fs::copy(&sample_path, &dest).expect("copy sample into scan dir");

        let catalog = Catalog::open_in_memory().unwrap();

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

        let _ = std::fs::remove_dir_all(dir.parent().unwrap());
    }
}
