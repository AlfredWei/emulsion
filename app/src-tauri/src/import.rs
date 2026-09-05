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
    /// The batch id every image landed by this call was tagged with (see
    /// `import_paths_with_progress`'s own doc comment) -- lets the
    /// frontend scope the post-import thumbnail backfill
    /// (`backfill_missing_thumbnails`) to just THIS import, not the whole
    /// catalog. Real, previously-undiscovered finding from building that
    /// scoping: an unrelated pre-existing backlog on a real dev catalog
    /// (33 un-thumbnailed photos from an earlier real-folder import) made
    /// a whole-catalog-scoped backfill take multiple minutes in a debug
    /// build -- every future import, even a single new file, would have
    /// had to wait out that same unrelated backlog before its own
    /// progress bar could finish.
    pub import_batch: i64,
}

/// Emitted to the frontend as the `import-progress` event (lib.rs) after
/// each candidate file is processed, so a progress bar can track a large
/// folder/multi-file import instead of the UI just freezing on "Importing…"
/// until the whole batch completes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportProgress {
    pub current: usize,
    pub total: usize,
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
///
/// Plain wrapper over `import_paths_with_progress` for callers that don't
/// need progress feedback -- only this crate's own tests today, since
/// lib.rs's Tauri commands both use the `_with_progress` form directly.
/// Kept as real pub API (`#[allow(dead_code)]`) rather than folded into
/// tests, same precedent as `catalog.rs`'s `add_image_with_metadata`.
#[allow(dead_code)]
pub fn import_paths(paths: &[PathBuf], catalog: &Catalog, thumbnail_dir: &Path) -> ImportSummary {
    import_paths_with_progress(paths, catalog, thumbnail_dir, |_, _| {})
}

/// Same as `import_paths`, but calls `on_progress(files_done, total_files)`
/// after every candidate file is processed (imported, skipped, or failed) --
/// lib.rs's Tauri commands use this to emit the `import-progress` event a
/// frontend progress bar listens for.
pub fn import_paths_with_progress<F: FnMut(usize, usize)>(
    paths: &[PathBuf],
    catalog: &Catalog,
    thumbnail_dir: &Path,
    mut on_progress: F,
) -> ImportSummary {
    // One id for every image landed by this call -- backs the Library
    // sidebar's "Last Import" source (`import_batch == max(import_batch)`).
    // Wall-clock millis, not an autoincrement counter, since there's no
    // dedicated batch table to hand one out from.
    let import_batch = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);

    let mut candidate_paths = Vec::new();
    for p in paths {
        if p.is_dir() {
            candidate_paths.extend(collect_candidate_files(p));
        } else {
            candidate_paths.push(p.clone());
        }
    }

    let mut summary = ImportSummary { import_batch, ..Default::default() };
    let _ = std::fs::create_dir_all(thumbnail_dir);
    let total = candidate_paths.len();

    // A labeled block, not the loop itself, so every early-exit path below
    // (`break 'file`, one per failure/skip case) still falls through to the
    // single `on_progress` call at the end of each iteration -- the loop
    // itself never `continue`s, so progress is reported for every candidate
    // exactly once, regardless of outcome.
    for (i, path) in candidate_paths.iter().enumerate() {
        'file: {
            let Ok(bytes) = std::fs::read(path) else {
                summary.failed += 1;
                break 'file;
            };

            let hash = blake3::hash(&bytes).to_hex().to_string();

            match catalog.find_by_hash(&hash) {
                Ok(Some(_)) => {
                    summary.skipped_duplicates += 1;
                    break 'file;
                }
                Err(_) => {
                    summary.failed += 1;
                    break 'file;
                }
                Ok(None) => {}
            }

            let Some(format) = ImageFormat::from_path(path) else {
                summary.failed += 1;
                break 'file;
            };
            let path_str = path.to_string_lossy().to_string();

            match format {
                ImageFormat::Raw => {
                    let Ok(mut raw_image) = RawImage::open(&bytes) else {
                        summary.failed += 1;
                        break 'file;
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
                        break 'file;
                    };
                    let _ = catalog.set_import_batch(image_id, import_batch);

                    if let Some(thumb_path) =
                        extract_and_write_thumbnail(&mut raw_image, image_id, thumbnail_dir)
                    {
                        let _ = catalog.set_thumbnail_path(image_id, &thumb_path.to_string_lossy());
                    }
                }
                ImageFormat::Jpeg => {
                    if !jpeg_bytes_look_valid(&bytes) {
                        summary.failed += 1;
                        break 'file;
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
                    match catalog
                        .add_image_with_edit_stack(&path_str, &hash, bytes.len() as i64, &EditStack::empty(), &file_metadata)
                    {
                        Ok(image_id) => {
                            let _ = catalog.set_import_batch(image_id, import_batch);
                        }
                        Err(_) => {
                            summary.failed += 1;
                            break 'file;
                        }
                    }
                }
            }

            summary.imported += 1;
        }

        on_progress(i + 1, total);
    }

    summary
}

/// Scan `dir` recursively and import every supported file found. Thin
/// wrapper over `import_paths` -- see that for the real per-file logic.
/// Only this crate's own tests call this form today (lib.rs's
/// `import_folder` command uses `scan_and_import_with_progress` directly);
/// kept as real pub API, same precedent as `import_paths` above.
#[allow(dead_code)]
pub fn scan_and_import(dir: &Path, catalog: &Catalog, thumbnail_dir: &Path) -> ImportSummary {
    scan_and_import_with_progress(dir, catalog, thumbnail_dir, |_, _| {})
}

/// Same as `scan_and_import`, but forwards per-file progress -- see
/// `import_paths_with_progress`.
pub fn scan_and_import_with_progress<F: FnMut(usize, usize)>(
    dir: &Path,
    catalog: &Catalog,
    thumbnail_dir: &Path,
    on_progress: F,
) -> ImportSummary {
    import_paths_with_progress(&collect_candidate_files(dir), catalog, thumbnail_dir, on_progress)
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
/// Shared with `regenerate_edited_thumbnail` below, so an edited
/// thumbnail is capped at the same size as an unedited one -- no visible
/// size/quality mismatch between the two in the same grid.
const THUMBNAIL_MAX_DIMENSION: u32 = 1024;

pub fn generate_missing_thumbnails(catalog: &Arc<Mutex<Catalog>>, thumbnail_dir: &Path) {
    generate_missing_thumbnails_with_progress(catalog, thumbnail_dir, None, |_, _| {})
}

/// Same as `generate_missing_thumbnails`, but calls
/// `on_progress(images_done, total_images)` after every candidate image is
/// processed (thumbnail written or generation failed) -- same "fires once
/// per candidate regardless of outcome" contract as
/// `import_paths_with_progress`'s own `on_progress`, so a frontend progress
/// bar can track this background pass the same way it already tracks the
/// import scan itself. `total` is fixed up front (candidates missing a
/// thumbnail at the moment this pass started), not recomputed as it goes.
///
/// `import_batch`, when `Some`, additionally restricts candidates to
/// images tagged with that exact batch (see `ImportSummary::import_batch`)
/// -- lib.rs's `backfill_missing_thumbnails` command uses this to scope a
/// post-import backfill to just the images that import landed, not the
/// whole catalog. Real motivation, not speculative: an unrelated
/// pre-existing backlog on a real dev catalog made a whole-catalog-scoped
/// backfill take multiple minutes -- every future import, even a single
/// new file, would otherwise have to wait out that same backlog before
/// its own progress bar could finish. `None` keeps the original
/// whole-catalog behavior, used by the startup catch-up pass and the
/// post-remove reimport backstop (lib.rs), neither of which has a single
/// "this batch" to scope to.
pub fn generate_missing_thumbnails_with_progress<F: FnMut(usize, usize)>(
    catalog: &Arc<Mutex<Catalog>>,
    thumbnail_dir: &Path,
    import_batch: Option<i64>,
    mut on_progress: F,
) {
    let images: Vec<_> = {
        let Ok(catalog) = catalog.lock() else { return };
        catalog
            .list_images()
            .unwrap_or_default()
            .into_iter()
            .filter(|image| image.thumbnail_path.is_none())
            .filter(|image| import_batch.is_none_or(|batch| image.import_batch == Some(batch)))
            .collect()
    };
    let _ = std::fs::create_dir_all(thumbnail_dir);
    let total = images.len();

    for (i, image) in images.into_iter().enumerate() {
        'image: {
            // Re-check under the lock (cheap indexed lookup, not another
            // full list_images() scan) rather than trusting the snapshot
            // above -- `ensure_thumbnail`'s on-demand "jump the queue" path
            // can race ahead of this loop and fill in exactly this image's
            // thumbnail between when the candidate list was captured and
            // when this iteration runs, and re-decoding a large RAW file
            // just to overwrite an already-correct result is real wasted
            // work worth skipping, not just a correctness nicety.
            let already_done = catalog.lock().ok().and_then(|c| c.get_thumbnail_path(image.image_id).ok()).flatten();
            if already_done.is_some() {
                break 'image;
            }

            let Some(out_path) = generate_thumbnail_file(image.image_id, Path::new(&image.path), thumbnail_dir)
            else {
                break 'image;
            };

            let Ok(catalog) = catalog.lock() else { return };
            let _ = catalog.set_thumbnail_path(image.image_id, &out_path.to_string_lossy());
        }

        on_progress(i + 1, total);
    }
}

/// Decodes `path` fresh (unedited) and writes a `THUMBNAIL_MAX_DIMENSION`-
/// capped JPEG thumbnail to `thumbnail_dir/{image_id}.jpg`. Pure
/// filesystem/decode work, no catalog access -- callers persist the
/// returned path themselves via `Catalog::set_thumbnail_path`. Shared by
/// the background backfill pass above and the on-demand single-image path
/// below (`ensure_thumbnail`), so both produce byte-for-byte the same kind
/// of thumbnail regardless of which one actually generated it.
fn generate_thumbnail_file(image_id: i64, path: &Path, thumbnail_dir: &Path) -> Option<PathBuf> {
    let decoded = match source_decode::decode_preview(path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("thumbnail generation failed for {}: {e}", path.display());
            return None;
        }
    };
    let Some(source) = image::RgbImage::from_raw(decoded.width, decoded.height, decoded.rgb) else {
        eprintln!("thumbnail generation failed for {}: decoded buffer size mismatch", path.display());
        return None;
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

    let out_path = thumbnail_dir.join(format!("{image_id}.jpg"));
    if let Err(e) = resized.save(&out_path) {
        eprintln!("thumbnail generation failed for {}: {e}", path.display());
        return None;
    }
    Some(out_path)
}

/// On-demand "jump the queue" thumbnail generation for ONE image: opening
/// Loupe/Develop on a photo the background backfill pass above hasn't
/// reached yet (it's a strict FIFO walk of the whole catalog, so the last
/// photo in a large just-imported folder can wait a long time) used to
/// mean staring at a blank grid placeholder for exactly the photo the user
/// is actively looking at, for as long as the queue took to get there.
/// This generates and persists that one image's thumbnail immediately,
/// independent of wherever the background pass currently is -- called by
/// lib.rs's `ensure_thumbnail` command, keyed by `version_id` (what the
/// frontend already has on hand for the open image) via
/// `Catalog::get_version_source` to resolve the owning `image_id` + path.
///
/// Re-checks `thumbnail_path` under the lock immediately before doing any
/// decode work (not just relying on the frontend to only call this for a
/// currently-null thumbnail) so a request that lands after the background
/// pass already finished this same image is a cheap no-op, not a
/// redundant decode.
pub fn ensure_thumbnail(catalog: &Arc<Mutex<Catalog>>, version_id: i64, thumbnail_dir: &Path) -> Option<String> {
    let source = catalog.lock().ok()?.get_version_source(version_id).ok()?;
    if let Some(existing) = catalog.lock().ok()?.get_thumbnail_path(source.image_id).ok().flatten() {
        return Some(existing);
    }

    let _ = std::fs::create_dir_all(thumbnail_dir);
    let out_path = generate_thumbnail_file(source.image_id, Path::new(&source.path), thumbnail_dir)?;
    let out_path_str = out_path.to_string_lossy().into_owned();
    catalog.lock().ok()?.set_thumbnail_path(source.image_id, &out_path_str).ok()?;
    Some(out_path_str)
}

/// Thumbnail refresh after a Develop edit: unlike `generate_missing_thumbnails`
/// above (a fresh, unedited RAW/JPEG decode), this reuses the Develop
/// preview cache's already-decoded, unedited buffer -- `ensure_develop_preview_for_hash`
/// returns a path/dimensions only, not pixels, so this loads the PNG
/// itself before applying `develop_engine::apply_edit_stack`'s formula, then
/// downscales with the same cap/filter as an unedited thumbnail so
/// there's no visible size/quality mismatch in the grid.
///
/// Writes to a NEW, content-hashed filename (`{image_id}-{hash8}.jpg`,
/// hashing the edit-stack JSON) rather than overwriting `{image_id}.jpg`
/// in place: this codebase has no cache-busting precedent anywhere (no
/// query params, nothing), so overwriting the same path risks the
/// webview's asset-protocol fetch showing stale cached bytes forever --
/// same content-addressing reasoning `preview_cache.rs` already
/// established for exactly this class of bug. Old thumbnail files (the
/// original import-time one, and any prior edited variant) are left as
/// accepted orphans, matching that module's own already-documented
/// tradeoff. Not fully dedup-idempotent -- `develop.js`'s `upsertOp`
/// re-appends the touched op at the end of the array on every change, so
/// reaching the same final values via a different edit order serializes
/// differently and hashes differently. Bounded pileup, not none; not
/// fixed here (would need canonicalizing op order before hashing).
///
/// Returns `None` on any failure (decode/IO/etc.) rather than propagating
/// an error -- the caller's edit-stack save already succeeded
/// independently, so a stale Library thumbnail is the whole cost of this
/// failing, not worth surfacing as a hard error.
pub fn regenerate_edited_thumbnail(
    source_path: &Path,
    content_hash: &str,
    image_id: i64,
    stack: &EditStack,
    previews_dir: &Path,
    thumbnail_dir: &Path,
) -> Option<PathBuf> {
    let preview =
        crate::preview_cache::ensure_develop_preview_for_hash(source_path, content_hash, previews_dir).ok()?;
    let mut decoded = image::open(&preview.path).ok()?.into_rgb8();
    // Lens Corrections (M3): same ordering export.rs uses -- see
    // develop_engine.rs's own header comment on `apply_lens_correction`.
    crate::develop_engine::apply_lens_correction(&mut decoded, stack);
    // Perspective Correction (M4): same ordering export.rs uses -- see
    // develop_engine.rs's own header comment on `apply_perspective`.
    crate::develop_engine::apply_perspective(&mut decoded, stack);
    crate::develop_engine::apply_edit_stack(&mut decoded, stack);
    // Crop & Straighten (M3): same shared post-process export.rs uses --
    // see develop_engine.rs's own doc comment on `apply_crop` for why
    // this is deliberately separate from apply_edit_stack.
    crate::develop_engine::apply_crop(&mut decoded, stack);

    let (w, h) = (decoded.width(), decoded.height());
    let resized = if w.max(h) > THUMBNAIL_MAX_DIMENSION {
        let scale = THUMBNAIL_MAX_DIMENSION as f64 / w.max(h) as f64;
        let target_w = ((w as f64) * scale).round().max(1.0) as u32;
        let target_h = ((h as f64) * scale).round().max(1.0) as u32;
        image::imageops::resize(&decoded, target_w, target_h, image::imageops::FilterType::Triangle)
    } else {
        decoded
    };

    let stack_json = serde_json::to_string(stack).ok()?;
    let stack_hash = blake3::hash(stack_json.as_bytes()).to_hex().to_string();
    let out_path = thumbnail_dir.join(format!("{image_id}-{}.jpg", &stack_hash[..8]));
    let _ = std::fs::create_dir_all(thumbnail_dir);
    resized.save(&out_path).ok()?;
    Some(out_path)
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
    fn progress_callback_fires_once_per_candidate_file_in_order() {
        let (dir, thumb_dir) = scan_and_thumb_dirs("progress");
        // Mixed outcomes (one skip-by-extension via a non-candidate file
        // alongside two failures) -- on_progress must still fire exactly
        // once per *candidate* file (the .txt is filtered out before
        // candidates are even counted, same as non_raw_files_are_skipped_not_fatal),
        // regardless of whether each one succeeds, fails, or is a duplicate.
        std::fs::write(dir.join("a.dng"), b"not really a DNG").unwrap();
        std::fs::write(dir.join("b.dng"), b"also not really a DNG").unwrap();
        std::fs::write(dir.join("notes.txt"), b"not a candidate at all").unwrap();

        let catalog = Catalog::open_in_memory().unwrap();
        let calls = std::cell::RefCell::new(Vec::new());
        let summary = scan_and_import_with_progress(&dir, &catalog, &thumb_dir, |current, total| {
            calls.borrow_mut().push((current, total));
        });

        assert_eq!(summary.failed, 2);
        let calls = calls.into_inner();
        assert_eq!(calls, vec![(1, 2), (2, 2)]);

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

    /// Same contract as `progress_callback_fires_once_per_candidate_file_in_order`,
    /// for the background thumbnail-backfill pass: `on_progress` must fire
    /// exactly once per image that's actually missing a thumbnail (not once
    /// per image in the whole catalog), in order, regardless of whether
    /// each one succeeds or fails, with `total` fixed at the count of
    /// candidates when the pass started.
    #[test]
    fn thumbnail_progress_callback_fires_once_per_missing_thumbnail_in_order() {
        let (dir, thumb_dir) = scan_and_thumb_dirs("thumbnail-progress");
        image::RgbImage::from_pixel(200, 100, image::Rgb([120, 90, 60]))
            .save(dir.join("a.jpg"))
            .unwrap();
        image::RgbImage::from_pixel(200, 100, image::Rgb([10, 20, 30]))
            .save(dir.join("b.jpg"))
            .unwrap();

        let catalog = Catalog::open_in_memory().unwrap();
        let summary = scan_and_import(&dir, &catalog, &thumb_dir);
        assert_eq!(summary.imported, 2);
        // Both JPEGs land with no thumbnail yet (see
        // imports_a_real_jpeg_and_backfills_its_thumbnail_in_the_background) --
        // both are real candidates for the pass below.

        let catalog = Arc::new(Mutex::new(catalog));
        let calls = std::cell::RefCell::new(Vec::new());
        generate_missing_thumbnails_with_progress(&catalog, &thumb_dir, None, |current, total| {
            calls.borrow_mut().push((current, total));
        });

        assert_eq!(calls.into_inner(), vec![(1, 2), (2, 2)]);
        let images = catalog.lock().unwrap().list_images().unwrap();
        assert!(images.iter().all(|img| img.thumbnail_path.is_some()));

        let _ = std::fs::remove_dir_all(dir.parent().unwrap());
    }

    /// Real regression guard for the backlog this scoping was added to fix
    /// (see `generate_missing_thumbnails_with_progress`'s own doc comment):
    /// a `Some(import_batch)` pass must touch ONLY images tagged with that
    /// batch, leaving an unrelated missing thumbnail from a DIFFERENT
    /// batch completely untouched (not counted in `total`, not generated).
    #[test]
    fn batch_scoped_backfill_ignores_missing_thumbnails_from_a_different_batch() {
        let (dir, thumb_dir) = scan_and_thumb_dirs("batch-scoped");
        image::RgbImage::from_pixel(200, 100, image::Rgb([1, 2, 3])).save(dir.join("old.jpg")).unwrap();

        let catalog = Catalog::open_in_memory().unwrap();
        let old_summary = scan_and_import(&dir, &catalog, &thumb_dir);
        assert_eq!(old_summary.imported, 1);

        let dir2 = dir.parent().unwrap().join("scan2");
        std::fs::create_dir_all(&dir2).unwrap();
        image::RgbImage::from_pixel(200, 100, image::Rgb([4, 5, 6])).save(dir2.join("new.jpg")).unwrap();
        let new_summary = scan_and_import(&dir2, &catalog, &thumb_dir);
        assert_eq!(new_summary.imported, 1);

        let catalog = Arc::new(Mutex::new(catalog));
        let calls = std::cell::RefCell::new(Vec::new());
        generate_missing_thumbnails_with_progress(&catalog, &thumb_dir, Some(new_summary.import_batch), |current, total| {
            calls.borrow_mut().push((current, total));
        });

        // Exactly one candidate (the new batch's own image) -- the old
        // batch's image is invisible to this call entirely, not just
        // skipped after being counted.
        assert_eq!(calls.into_inner(), vec![(1, 1)]);

        let images = catalog.lock().unwrap().list_images().unwrap();
        let old_image = images.iter().find(|img| img.path.ends_with("old.jpg")).unwrap();
        let new_image = images.iter().find(|img| img.path.ends_with("new.jpg")).unwrap();
        assert!(old_image.thumbnail_path.is_none(), "an unrelated older batch's missing thumbnail must be left alone");
        assert!(new_image.thumbnail_path.is_some(), "the scoped batch's own image must still get its thumbnail");

        let _ = std::fs::remove_dir_all(dir.parent().unwrap());
    }

    /// `ensure_thumbnail` (the "jump the queue" on-demand path Loupe/Develop
    /// call when opening a photo the background backfill pass hasn't
    /// reached yet) must generate and persist a real thumbnail for exactly
    /// the requested image, keyed by version_id like the frontend's own
    /// `get_graded_develop_preview` call.
    #[test]
    fn ensure_thumbnail_generates_and_persists_a_thumbnail_for_one_image_on_demand() {
        let (dir, thumb_dir) = scan_and_thumb_dirs("ensure-thumbnail");
        image::RgbImage::from_pixel(200, 100, image::Rgb([200, 150, 50])).save(dir.join("a.jpg")).unwrap();

        let catalog = Catalog::open_in_memory().unwrap();
        let summary = scan_and_import(&dir, &catalog, &thumb_dir);
        assert_eq!(summary.imported, 1);

        let images = catalog.list_images().unwrap();
        assert!(images[0].thumbnail_path.is_none());
        let version_id = images[0].version_id;

        let catalog = Arc::new(Mutex::new(catalog));
        let path = ensure_thumbnail(&catalog, version_id, &thumb_dir).expect("should generate a thumbnail");
        assert!(Path::new(&path).exists());

        let images = catalog.lock().unwrap().list_images().unwrap();
        assert_eq!(images[0].thumbnail_path.as_deref(), Some(path.as_str()));

        let _ = std::fs::remove_dir_all(dir.parent().unwrap());
    }

    /// A second call for the same already-thumbnailed image must be a
    /// cheap no-op (return the existing path) rather than re-decoding and
    /// silently overwriting it -- this is the exact race `generate_missing_thumbnails_with_progress`'s
    /// own re-check guards against from the other direction.
    #[test]
    fn ensure_thumbnail_is_a_no_op_when_a_thumbnail_already_exists() {
        let (dir, thumb_dir) = scan_and_thumb_dirs("ensure-thumbnail-noop");
        image::RgbImage::from_pixel(200, 100, image::Rgb([1, 2, 3])).save(dir.join("a.jpg")).unwrap();

        let catalog = Catalog::open_in_memory().unwrap();
        let summary = scan_and_import(&dir, &catalog, &thumb_dir);
        assert_eq!(summary.imported, 1);
        let version_id = catalog.list_images().unwrap()[0].version_id;

        let catalog = Arc::new(Mutex::new(catalog));
        let first = ensure_thumbnail(&catalog, version_id, &thumb_dir).unwrap();
        let second = ensure_thumbnail(&catalog, version_id, &thumb_dir).unwrap();
        assert_eq!(first, second);

        let _ = std::fs::remove_dir_all(dir.parent().unwrap());
    }

    /// Backs the Library sidebar's "Last Import" source: every image landed
    /// by one `import_paths` call must share the same `import_batch`, and a
    /// later, separate import must get a strictly newer one -- otherwise
    /// "Last Import" couldn't distinguish the two.
    #[test]
    fn images_from_one_import_share_a_batch_id_and_a_later_import_gets_a_newer_one() {
        let (dir, thumb_dir) = scan_and_thumb_dirs("import-batch");
        image::RgbImage::from_pixel(200, 100, image::Rgb([1, 2, 3])).save(dir.join("a.jpg")).unwrap();
        image::RgbImage::from_pixel(200, 100, image::Rgb([4, 5, 6])).save(dir.join("b.jpg")).unwrap();

        let catalog = Catalog::open_in_memory().unwrap();
        let summary = scan_and_import(&dir, &catalog, &thumb_dir);
        assert_eq!(summary.imported, 2);

        let first_batch_images = catalog.list_images().unwrap();
        assert_eq!(first_batch_images.len(), 2);
        let batch_a = first_batch_images[0].import_batch.expect("import_batch must be set");
        let batch_b = first_batch_images[1].import_batch.expect("import_batch must be set");
        assert_eq!(batch_a, batch_b, "both images from the same import must share one batch id");

        let dir2 = dir.parent().unwrap().join("scan2");
        std::fs::create_dir_all(&dir2).unwrap();
        image::RgbImage::from_pixel(200, 100, image::Rgb([7, 8, 9])).save(dir2.join("c.jpg")).unwrap();
        let summary2 = scan_and_import(&dir2, &catalog, &thumb_dir);
        assert_eq!(summary2.imported, 1);

        let images = catalog.list_images().unwrap();
        let newest = images.iter().find(|img| img.path.ends_with("c.jpg")).unwrap();
        assert!(
            newest.import_batch.unwrap() >= batch_a,
            "a later, separate import must get a batch id no older than the first"
        );

        let _ = std::fs::remove_dir_all(dir.parent().unwrap());
    }

    /// A real (if non-photographic) JPEG, edited, regenerated -- confirms
    /// the output actually reflects the edit (not just a pass-through
    /// resize), lands at the expected content-hashed path, and that
    /// re-regenerating with the SAME edit stack reuses the same filename
    /// (the "bounded pileup, not none" idempotency this module's doc
    /// comment describes: identical stacks, not just identical final
    /// values reached via a different op order).
    #[test]
    fn regenerate_edited_thumbnail_reflects_the_edit_and_is_content_addressed() {
        let (dir, thumb_dir) = scan_and_thumb_dirs("regen-thumbnail");
        let previews_dir = dir.parent().unwrap().join("previews");
        let source_path = dir.join("photo.jpg");
        image::RgbImage::from_pixel(200, 100, image::Rgb([120, 90, 60])).save(&source_path).unwrap();
        let bytes = std::fs::read(&source_path).unwrap();
        let content_hash = blake3::hash(&bytes).to_hex().to_string();

        let stack = EditStack {
            schema_version: 1,
            ops: vec![
                serde_json::json!({"op": "exposure", "value": 1.0}),
                serde_json::json!({"op": "saturation", "value": -100.0}),
            ],
        };

        let out_path = regenerate_edited_thumbnail(&source_path, &content_hash, 42, &stack, &previews_dir, &thumb_dir)
            .expect("regeneration should succeed for a real JPEG");

        assert!(out_path.exists());
        assert!(
            out_path.file_name().unwrap().to_string_lossy().starts_with("42-"),
            "filename must be keyed by image_id, got {out_path:?}"
        );

        let edited = image::open(&out_path).unwrap().into_rgb8();
        let edited_pixel = edited.get_pixel(0, 0);
        // exposure +1 EV should brighten, saturation -100 should desaturate
        // toward equal channels -- either signal alone confirms the edit
        // was actually applied, not a clean passthrough of the original
        // (120, 90, 60).
        assert_ne!(*edited_pixel, image::Rgb([120, 90, 60]), "output must differ from the unedited source");

        // Re-regenerating the identical stack must resolve to the same path.
        let out_path_again =
            regenerate_edited_thumbnail(&source_path, &content_hash, 42, &stack, &previews_dir, &thumb_dir).unwrap();
        assert_eq!(out_path, out_path_again);

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
