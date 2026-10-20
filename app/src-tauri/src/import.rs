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

/// How the per-file import reaches the catalog. The plain form borrows one `Catalog` for the whole call (the tests, and
/// any caller that already holds the lock); the shared form locks it for each single step (RFC-0030 slice C), so the
/// rest of the app can use the catalog while a long import runs. `None` means the catalog could not be reached (a
/// poisoned lock), which the import counts as a failure of that file.
trait CatalogAccess {
    fn with<R>(&self, f: impl FnOnce(&Catalog) -> R) -> Option<R>;
}

struct Direct<'a>(&'a Catalog);

impl CatalogAccess for Direct<'_> {
    fn with<R>(&self, f: impl FnOnce(&Catalog) -> R) -> Option<R> {
        Some(f(self.0))
    }
}

struct Shared<'a>(&'a Mutex<Catalog>);

impl CatalogAccess for Shared<'_> {
    fn with<R>(&self, f: impl FnOnce(&Catalog) -> R) -> Option<R> {
        self.0.lock().ok().map(|c| f(&c))
    }
}

/// What importing one file came to.
enum FileOutcome {
    /// Cataloged. `needs_thumbnail` is true when no thumbnail was written on the spot (every JPEG, and a RAW whose
    /// embedded picture was not a JPEG), so something still has to make one.
    Imported { image_id: i64, needs_thumbnail: bool },
    Duplicate,
    Failed,
}

/// One id for every image landed by one import call: wall-clock millis, since there is no dedicated batch table to
/// hand one out from. Backs the Library sidebar's "Last Import" source (`import_batch == max(import_batch)`).
fn new_import_batch() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// The per-file core shared by every import entry point: read, hash, skip a known hash, catalog (one transaction),
/// and for a RAW file write its embedded thumbnail. A JPEG's thumbnail is a full decode, categorically heavier than
/// RAW's cheap embedded-picture extraction, so it is made by whoever called this (the background pass, or the pool of
/// `import_paths_streaming`), never here.
fn import_one<A: CatalogAccess>(path: &Path, access: &A, thumbnail_dir: &Path, import_batch: i64) -> FileOutcome {
    let Ok(bytes) = std::fs::read(path) else { return FileOutcome::Failed };
    let hash = blake3::hash(&bytes).to_hex().to_string();

    match access.with(|c| c.find_by_hash(&hash)) {
        Some(Ok(Some(_))) => return FileOutcome::Duplicate,
        Some(Ok(None)) => {}
        Some(Err(_)) | None => return FileOutcome::Failed,
    }

    let Some(format) = ImageFormat::from_path(path) else { return FileOutcome::Failed };
    let path_str = path.to_string_lossy().to_string();

    match format {
        ImageFormat::Raw => {
            // See raw_decode::LIBRAW_LOCK's own comment: LibRaw isn't safely reentrant across threads, and this open
            // (plus extract_and_write_thumbnail's extract_thumbs() below) can run concurrently with a Develop
            // preview decode, an HDR merge, or an on-demand priority thumbnail on another spawn_blocking task.
            // Metadata extraction reads only already-open header fields (no further LibRaw calls), so the lock
            // doesn't need to span it.
            let raw_lock = crate::raw_decode::LIBRAW_LOCK.lock().unwrap();
            let Ok(mut raw_image) = RawImage::open(&bytes) else { return FileOutcome::Failed };
            drop(raw_lock);

            // M2 Slice 2: free to call here -- RawImage::open() already populated the header fields this reads, no
            // unpack()/process() needed (confirmed against rsraw's own source).
            let file_metadata = metadata::extract_from_raw(&raw_image);

            // Atomic (M1 Slice 6): both rows (+ metadata, M2 Slice 2) in one transaction, so a crash partway through
            // can't leave a permanently-orphaned or inconsistently-tagged row.
            let added = access.with(|c| {
                let id = c.add_image_with_edit_stack(&path_str, &hash, bytes.len() as i64, &EditStack::empty(), &file_metadata).ok()?;
                let _ = c.set_import_batch(id, import_batch);
                Some(id)
            });
            let Some(Some(image_id)) = added else { return FileOutcome::Failed };

            let raw_lock = crate::raw_decode::LIBRAW_LOCK.lock().unwrap();
            let thumb_result = extract_and_write_thumbnail(&mut raw_image, image_id, thumbnail_dir);
            drop(raw_lock);
            let recorded = thumb_result.is_some_and(|thumb_path| {
                access.with(|c| c.set_thumbnail_path(image_id, &thumb_path.to_string_lossy()).is_ok()).unwrap_or(false)
            });
            FileOutcome::Imported { image_id, needs_thumbnail: !recorded }
        }
        ImageFormat::Jpeg => {
            if !jpeg_bytes_look_valid(&bytes) {
                return FileOutcome::Failed;
            }
            // M2 Slice 2: a fresh EXIF read, deliberately not shared with jpeg_decode.rs's orientation read.
            let file_metadata = metadata::extract_from_jpeg(&bytes);
            let added = access.with(|c| {
                let id = c.add_image_with_edit_stack(&path_str, &hash, bytes.len() as i64, &EditStack::empty(), &file_metadata).ok()?;
                let _ = c.set_import_batch(id, import_batch);
                Some(id)
            });
            match added {
                Some(Some(image_id)) => {
                    // Tier 0: the picture the camera stored inside the file, shown with the row at once. The real
                    // thumbnail (`needs_thumbnail`) replaces it.
                    if let Some(placeholder) = write_placeholder_thumbnail(&bytes, image_id, thumbnail_dir) {
                        let _ = access.with(|c| c.set_thumbnail_path(image_id, &placeholder.to_string_lossy()));
                    }
                    FileOutcome::Imported { image_id, needs_thumbnail: true }
                }
                _ => FileOutcome::Failed,
            }
        }
    }
}

/// The files an import call will look at: a directory is walked recursively, anything else is taken as it is.
fn candidates_of(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut candidate_paths = Vec::new();
    for p in paths {
        if p.is_dir() {
            candidate_paths.extend(collect_candidate_files(p));
        } else {
            candidate_paths.push(p.clone());
        }
    }
    candidate_paths
}

fn count(summary: &mut ImportSummary, outcome: &FileOutcome) {
    match outcome {
        FileOutcome::Imported { .. } => summary.imported += 1,
        FileOutcome::Duplicate => summary.skipped_duplicates += 1,
        FileOutcome::Failed => summary.failed += 1,
    }
}

/// Same as `import_paths`, but calls `on_progress(files_done, total_files)`
/// after every candidate file is processed (imported, skipped, or failed) --
/// lib.rs's Tauri commands use this to emit the `import-progress` event a
/// frontend progress bar listens for. Makes no thumbnails for JPEGs (see
/// `import_one`): the caller runs `generate_missing_thumbnails*` afterwards.
/// The app itself uses `import_paths_streaming` below; this is the form for
/// callers that hold one `&Catalog` (the tests).
#[allow(dead_code)]
pub fn import_paths_with_progress<F: FnMut(usize, usize)>(
    paths: &[PathBuf],
    catalog: &Catalog,
    thumbnail_dir: &Path,
    mut on_progress: F,
) -> ImportSummary {
    let import_batch = new_import_batch();
    let candidate_paths = candidates_of(paths);
    let mut summary = ImportSummary { import_batch, ..Default::default() };
    let _ = std::fs::create_dir_all(thumbnail_dir);
    let total = candidate_paths.len();
    let access = Direct(catalog);

    for (i, path) in candidate_paths.iter().enumerate() {
        let outcome = import_one(path, &access, thumbnail_dir, import_batch);
        count(&mut summary, &outcome);
        on_progress(i + 1, total);
    }
    summary
}

/// The import the app runs (RFC-0030 slice C): the same per-file core, but
///
/// - the catalog is locked for each single step, not for the whole import, so listing photos, rating or opening one
///   keeps working while a long import runs and the grid can show the rows as they land;
/// - a pool of threads (`thumbnail_worker_count`) makes each picture's thumbnail while the next files are still being
///   read, instead of a separate pass afterwards. Every JPEG's thumbnail is ready within moments of its row.
///
/// `on_ready(image_id, path)` fires for each thumbnail written by the pool, from the calling thread, once the catalog
/// holds the path. The call returns when every file is catalogued and every queued thumbnail is made.
pub fn import_paths_streaming<F: FnMut(usize, usize), R: FnMut(i64, &Path)>(
    paths: &[PathBuf],
    catalog: &Arc<Mutex<Catalog>>,
    thumbnail_dir: &Path,
    workers: Option<usize>,
    mut on_progress: F,
    mut on_ready: R,
) -> ImportSummary {
    let import_batch = new_import_batch();
    let candidate_paths = candidates_of(paths);
    let mut summary = ImportSummary { import_batch, ..Default::default() };
    let _ = std::fs::create_dir_all(thumbnail_dir);
    let total = candidate_paths.len();
    if total == 0 {
        return summary;
    }
    let workers = workers.unwrap_or_else(|| thumbnail_worker_count(std::thread::available_parallelism().map_or(1, |n| n.get()), total));
    let access = Shared(catalog);
    // The user is waiting for this import: the background large-thumbnail pass yields until it is over.
    let _foreground = ForegroundWork::begin();

    let (job_tx, job_rx) = std::sync::mpsc::channel::<(i64, PathBuf)>();
    let job_rx = Mutex::new(job_rx);
    let (ready_tx, ready_rx) = std::sync::mpsc::channel::<(i64, PathBuf)>();

    std::thread::scope(|scope| {
        for _ in 0..workers {
            let ready_tx = ready_tx.clone();
            let job_rx = &job_rx;
            scope.spawn(move || loop {
                // The lock is held only while waiting for the next job; the picture itself is made outside it.
                let job = { job_rx.lock().ok().and_then(|rx| rx.recv().ok()) };
                let Some((image_id, path)) = job else { break };
                let made = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| generate_thumbnail_file(image_id, &path, thumbnail_dir, ThumbTier::Small)))
                    .unwrap_or_else(|_| {
                        eprintln!("thumbnail generation panicked for {}", path.display());
                        None
                    });
                let Some(out) = made else { continue };
                let recorded = catalog.lock().ok().is_some_and(|c| record_thumbnail(&c, image_id, &out, ThumbTier::Small));
                if recorded {
                    remove_lower_tier_files(thumbnail_dir, image_id, ThumbTier::Small);
                    if ready_tx.send((image_id, out)).is_err() {
                        break;
                    }
                }
            });
        }
        drop(ready_tx);

        for (i, path) in candidate_paths.iter().enumerate() {
            let outcome = import_one(path, &access, thumbnail_dir, import_batch);
            count(&mut summary, &outcome);
            if let FileOutcome::Imported { image_id, needs_thumbnail: true } = outcome {
                let _ = job_tx.send((image_id, path.clone()));
            }
            // Pictures finished while this file was being read go out now, not at the end.
            while let Ok((id, out)) = ready_rx.try_recv() {
                on_ready(id, &out);
            }
            on_progress(i + 1, total);
        }
        drop(job_tx);
        for (id, out) in ready_rx.iter() {
            on_ready(id, &out);
        }
    });

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
#[allow(dead_code)]
pub fn scan_and_import_with_progress<F: FnMut(usize, usize)>(
    dir: &Path,
    catalog: &Catalog,
    thumbnail_dir: &Path,
    on_progress: F,
) -> ImportSummary {
    import_paths_with_progress(&collect_candidate_files(dir), catalog, thumbnail_dir, on_progress)
}

/// `import_paths_streaming` over everything found under `dir`.
pub fn scan_and_import_streaming<F: FnMut(usize, usize), R: FnMut(i64, &Path)>(
    dir: &Path,
    catalog: &Arc<Mutex<Catalog>>,
    thumbnail_dir: &Path,
    on_progress: F,
    on_ready: R,
) -> ImportSummary {
    import_paths_streaming(&collect_candidate_files(dir), catalog, thumbnail_dir, None, on_progress, on_ready)
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
/// The tiers a thumbnail goes through (RFC-0030 slices D and E): the camera's own small EXIF picture (tier 0, written
/// as `{id}-t.jpg` at cataloguing), a 512 px grid thumbnail (`{id}-s.jpg`, made first by the worker pool), and the
/// 1024 px thumbnail (`{id}.jpg`, the name every existing catalog already uses, made afterwards in the background).
/// The file's name says which tier it is, so the catalog needs no extra column: a path ending `-t.jpg` or `-s.jpg` is
/// provisional and the next tier is still to come. Every consumer reads the one `thumbnail_path`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThumbTier {
    Small,
    Large,
}

impl ThumbTier {
    /// Where it sits in the order none (0) < tier 0 (1) < small (2) < large (3).
    fn rank(self) -> u8 {
        match self {
            ThumbTier::Small => 2,
            ThumbTier::Large => 3,
        }
    }

    fn max_dimension(self) -> u32 {
        match self {
            ThumbTier::Small => 512,
            ThumbTier::Large => 1024,
        }
    }

    fn file_name(self, image_id: i64) -> String {
        match self {
            ThumbTier::Small => format!("{image_id}{SMALL_SUFFIX}"),
            ThumbTier::Large => format!("{image_id}.jpg"),
        }
    }
}

const PLACEHOLDER_SUFFIX: &str = "-t.jpg";
const SMALL_SUFFIX: &str = "-s.jpg";

/// 0 = none, 1 = tier 0 (the camera's picture), 2 = small, 3 = large or anything else (an edited picture, a RAW's
/// embedded JPEG, every thumbnail made before tiers existed): final.
pub(crate) fn thumbnail_rank(path: Option<&str>) -> u8 {
    match path {
        None => 0,
        Some(p) if p.ends_with(PLACEHOLDER_SUFFIX) => 1,
        Some(p) if p.ends_with(SMALL_SUFFIX) => 2,
        Some(_) => 3,
    }
}

/// True when a better thumbnail is still to come (tier 0 or small).
pub(crate) fn is_provisional_thumbnail(path: &str) -> bool {
    thumbnail_rank(Some(path)) < 3
}

/// Whether making `target` would improve on what the catalog holds.
fn wants(current: Option<&str>, target: ThumbTier) -> bool {
    thumbnail_rank(current) < target.rank()
}

/// Records `path` as the image's thumbnail only if it improves on what is there: never replaces an equal or better
/// tier (an edited picture, a large one that got in first). Returns whether it recorded.
fn record_thumbnail(catalog: &Catalog, image_id: i64, path: &Path, target: ThumbTier) -> bool {
    let current = catalog.get_thumbnail_path(image_id).ok().flatten();
    wants(current.as_deref(), target) && catalog.set_thumbnail_path(image_id, &path.to_string_lossy()).is_ok()
}

/// Deletes the files of the tiers `target` replaced (best effort: a leftover is a few KB).
fn remove_lower_tier_files(thumbnail_dir: &Path, image_id: i64, target: ThumbTier) {
    let _ = std::fs::remove_file(thumbnail_dir.join(format!("{image_id}{PLACEHOLDER_SUFFIX}")));
    if target == ThumbTier::Large {
        let _ = std::fs::remove_file(thumbnail_dir.join(format!("{image_id}{SMALL_SUFFIX}")));
    }
}

/// Work the user is waiting for (an import, a small-thumbnail pass) is counted here, and the background large pass
/// waits while it is non-zero, so the picture-making the user is watching never competes with the upgrade.
static FOREGROUND_WORK: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

struct ForegroundWork;

impl ForegroundWork {
    fn begin() -> Self {
        FOREGROUND_WORK.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        ForegroundWork
    }
}

impl Drop for ForegroundWork {
    fn drop(&mut self) {
        FOREGROUND_WORK.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
    }
}

pub(crate) fn wait_for_foreground_work() {
    while FOREGROUND_WORK.load(std::sync::atomic::Ordering::SeqCst) > 0 {
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

/// Exact k x k box average, so the expensive resize filter only has to take the picture the last factor of two
/// (RFC-0030 §4.1: this and orienting later cut a thumbnail's cost by a quarter). Edge pixels beyond a multiple of
/// `k` are dropped (less than `k` of them).
fn box_down(src: &image::RgbImage, k: u32) -> image::RgbImage {
    let (w, h) = (src.width() / k, src.height() / k);
    let sw = src.width() as usize;
    let raw = src.as_raw();
    let area = k * k;
    let mut out = vec![0u8; (w * h * 3) as usize];
    for y in 0..h as usize {
        for x in 0..w as usize {
            let mut acc = [0u32; 3];
            for dy in 0..k as usize {
                let row = ((y * k as usize + dy) * sw + x * k as usize) * 3;
                for dx in 0..k as usize {
                    let p = row + dx * 3;
                    acc[0] += raw[p] as u32;
                    acc[1] += raw[p + 1] as u32;
                    acc[2] += raw[p + 2] as u32;
                }
            }
            let o = (y * w as usize + x) * 3;
            out[o] = (acc[0] / area) as u8;
            out[o + 1] = (acc[1] / area) as u8;
            out[o + 2] = (acc[2] / area) as u8;
        }
    }
    image::RgbImage::from_raw(w, h, out).expect("the buffer is w * h * 3 long")
}

/// Shrinks `source` so its long edge is at most `max_dim`: a box average by the whole factor that leaves at least
/// twice `max_dim`, then a triangle filter for the rest. The same function makes unedited and edited thumbnails, so
/// the two never differ in look inside one grid.
pub(crate) fn downscale_to(source: image::RgbImage, max_dim: u32) -> image::RgbImage {
    let long = source.width().max(source.height());
    if long <= max_dim {
        return source;
    }
    let k = (long / (max_dim * 2)).max(1);
    let pre = if k > 1 { box_down(&source, k) } else { source };
    let (w, h) = (pre.width(), pre.height());
    if w.max(h) <= max_dim {
        return pre;
    }
    let scale = max_dim as f64 / w.max(h) as f64;
    let target_w = ((w as f64) * scale).round().max(1.0) as u32;
    let target_h = ((h as f64) * scale).round().max(1.0) as u32;
    image::imageops::resize(&pre, target_w, target_h, image::imageops::FilterType::Triangle)
}

/// Writes a thumbnail picture as a JPEG with the dedicated encoder (faster than the `image` crate's, quality 82).
fn write_thumbnail_jpeg(img: &image::RgbImage, out_path: &Path) -> std::io::Result<()> {
    let mut bytes = Vec::new();
    jpeg_encoder::Encoder::new(&mut bytes, 82)
        .encode(img.as_raw(), img.width() as u16, img.height() as u16, jpeg_encoder::ColorType::Rgb)
        .map_err(std::io::Error::other)?;
    std::fs::write(out_path, bytes)
}

/// Writes the JPEG's own EXIF thumbnail as the tier-0 picture, when it has a usable one (`metadata::exif_thumbnail`).
fn write_placeholder_thumbnail(bytes: &[u8], image_id: i64, thumbnail_dir: &Path) -> Option<PathBuf> {
    let thumb = metadata::exif_thumbnail(bytes)?;
    let out = thumbnail_dir.join(format!("{image_id}{PLACEHOLDER_SUFFIX}"));
    std::fs::write(&out, thumb).ok()?;
    Some(out)
}

/// One finished thumbnail, as the `thumbnail-ready` event carries it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadyThumbnail {
    pub image_id: i64,
    pub thumbnail_path: String,
}

/// Groups finished thumbnails so a fast pass sends the webview a message every `interval`, not one per file. The
/// first one goes out at once (that is the "first picture" the user waits for); `take` hands out what is left when
/// the pass ends. Pure bookkeeping, no clock but the one passed in, so it is tested without sleeping.
pub struct ReadyBatcher {
    pending: Vec<ReadyThumbnail>,
    last_sent: Option<std::time::Instant>,
    interval: std::time::Duration,
}

impl ReadyBatcher {
    pub fn new(interval: std::time::Duration) -> Self {
        Self { pending: Vec::new(), last_sent: None, interval }
    }

    /// Adds one; returns the group to send now when it is time (the first call, or `interval` after the last send).
    pub fn push(&mut self, item: ReadyThumbnail, now: std::time::Instant) -> Option<Vec<ReadyThumbnail>> {
        self.pending.push(item);
        let due = self.last_sent.is_none_or(|t| now.duration_since(t) >= self.interval);
        if !due {
            return None;
        }
        self.last_sent = Some(now);
        Some(std::mem::take(&mut self.pending))
    }

    /// Whatever has not been sent yet (empty when nothing is waiting).
    pub fn take(&mut self) -> Vec<ReadyThumbnail> {
        std::mem::take(&mut self.pending)
    }
}

/// Makes the final (large) thumbnail for every image that has none or only a provisional one, with no progress.
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
    on_progress: F,
) {
    generate_missing_thumbnails_streaming(catalog, thumbnail_dir, import_batch, ThumbTier::Large, on_progress, |_, _| {});
}

/// How many threads make thumbnails (RFC-0030 slice B): the cores minus two (the interface and the file system keep
/// theirs), at most 8 (measured: ten cores are no faster than eight, the extra two are the slow ones), at least one,
/// and never more than there are images. Each worker holds one decoded photo (about 0.2 GB at 24 MP), so the cap is
/// also the memory bound.
/// Threads of the quiet large-thumbnail pass: two, so the upgrade never takes the machine from the user.
pub(crate) const BACKGROUND_WORKERS: usize = 2;

pub(crate) fn thumbnail_worker_count(cores: usize, images: usize) -> usize {
    cores.saturating_sub(2).clamp(1, 8).min(images).max(1)
}

/// Same pass, and additionally calls `on_ready(image_id, thumbnail_path)` the moment each thumbnail is written and
/// recorded in the catalog (RFC-0030 slice A), so the grid can show it then instead of when the whole pass ends.
/// Not called for an image whose generation failed, nor for one `ensure_thumbnail` filled in first.
///
/// The pictures are made by a pool of threads (slice B, `thumbnail_worker_count`); the catalog writes and both
/// callbacks stay on the calling thread, one at a time, so they need not be `Send` and the catalog is touched in
/// short single-row steps. `on_progress` still fires once per candidate with a count that only goes up, but the
/// order in which pictures finish is no longer the order of the list.
pub fn generate_missing_thumbnails_streaming<F: FnMut(usize, usize), R: FnMut(i64, &Path)>(
    catalog: &Arc<Mutex<Catalog>>,
    thumbnail_dir: &Path,
    import_batch: Option<i64>,
    tier: ThumbTier,
    on_progress: F,
    on_ready: R,
) {
    generate_missing_thumbnails_with_workers(catalog, thumbnail_dir, import_batch, tier, None, on_progress, on_ready);
}

/// The pass with an explicit worker count (`None` = `thumbnail_worker_count` for this machine); tests and the speed
/// probe pin it.
pub(crate) fn generate_missing_thumbnails_with_workers<F: FnMut(usize, usize), R: FnMut(i64, &Path)>(
    catalog: &Arc<Mutex<Catalog>>,
    thumbnail_dir: &Path,
    import_batch: Option<i64>,
    tier: ThumbTier,
    workers: Option<usize>,
    mut on_progress: F,
    mut on_ready: R,
) {
    use std::sync::atomic::{AtomicUsize, Ordering};

    // The user waits for a small-thumbnail pass; the large one is the quiet upgrade behind it (see FOREGROUND_WORK).
    let _foreground = (tier == ThumbTier::Small).then(ForegroundWork::begin);
    let images: Vec<_> = {
        let Ok(catalog) = catalog.lock() else { return };
        catalog
            .list_images()
            .unwrap_or_default()
            .into_iter()
            .filter(|image| wants(image.thumbnail_path.as_deref(), tier))
            .filter(|image| import_batch.is_none_or(|batch| image.import_batch == Some(batch)))
            .collect()
    };
    let _ = std::fs::create_dir_all(thumbnail_dir);
    let total = images.len();
    if total == 0 {
        return;
    }
    let workers = workers
        .unwrap_or_else(|| match tier {
            ThumbTier::Small => thumbnail_worker_count(std::thread::available_parallelism().map_or(1, |n| n.get()), total),
            ThumbTier::Large => BACKGROUND_WORKERS,
        })
        .clamp(1, total);

    let next = AtomicUsize::new(0);
    // (image id, the file written, or None when there is nothing to record): every candidate sends exactly one.
    let (tx, rx) = std::sync::mpsc::channel::<(i64, Option<PathBuf>)>();
    std::thread::scope(|scope| {
        for _ in 0..workers {
            let tx = tx.clone();
            let (images, next) = (&images, &next);
            scope.spawn(move || {
                loop {
                    if tier == ThumbTier::Large {
                        wait_for_foreground_work();
                    }
                    let i = next.fetch_add(1, Ordering::SeqCst);
                    let Some(image) = images.get(i) else { break };
                    // Re-check under the lock (cheap indexed lookup, not another full list_images() scan) rather
                    // than trusting the snapshot above: `ensure_thumbnail`'s on-demand "jump the queue" path can
                    // fill in exactly this image's thumbnail after the candidate list was captured, and
                    // re-decoding a large file just to overwrite a correct result is real wasted work.
                    let current = catalog.lock().ok().and_then(|c| c.get_thumbnail_path(image.image_id).ok()).flatten();
                    let out = if !wants(current.as_deref(), tier) {
                        None
                    } else {
                        // A decoder that panics on a hostile file costs that one picture, not the whole pass.
                        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            generate_thumbnail_file(image.image_id, Path::new(&image.path), thumbnail_dir, tier)
                        }))
                        .unwrap_or_else(|_| {
                            eprintln!("thumbnail generation panicked for {}", image.path);
                            None
                        })
                    };
                    if tx.send((image.image_id, out)).is_err() {
                        break;
                    }
                }
            });
        }
        drop(tx);

        for (done, (image_id, out)) in rx.iter().enumerate() {
            if let Some(out_path) = out {
                let recorded = {
                    let Ok(catalog) = catalog.lock() else { return };
                    record_thumbnail(&catalog, image_id, &out_path, tier)
                };
                // After the lock is released, so the callback may use the catalog itself.
                if recorded {
                    remove_lower_tier_files(thumbnail_dir, image_id, tier);
                    on_ready(image_id, &out_path);
                }
            }
            on_progress(done + 1, total);
        }
    });
}

/// Decodes `path` fresh (unedited) and writes the `tier`'s JPEG thumbnail (512 px `{image_id}-s.jpg`, or 1024 px
/// `{image_id}.jpg`) to `thumbnail_dir`. Pure filesystem/decode work, no catalog access -- callers persist the
/// returned path themselves. Shared by the passes, the import's pool and the on-demand single-image path
/// (`ensure_thumbnail`), so all of them make the same kind of thumbnail whichever generated it.
fn generate_thumbnail_file(image_id: i64, path: &Path, thumbnail_dir: &Path, tier: ThumbTier) -> Option<PathBuf> {
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

    let resized = downscale_to(source, tier.max_dimension());
    let out_path = thumbnail_dir.join(tier.file_name(image_id));
    if let Err(e) = write_thumbnail_jpeg(&resized, &out_path) {
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
    let current = catalog.lock().ok()?.get_thumbnail_path(source.image_id).ok().flatten();
    if let Some(existing) = current.as_ref().filter(|p| !is_provisional_thumbnail(p)) {
        return Some(existing.clone());
    }

    let _ = std::fs::create_dir_all(thumbnail_dir);
    let out_path = generate_thumbnail_file(source.image_id, Path::new(&source.path), thumbnail_dir, ThumbTier::Large)?;
    let out_path_str = out_path.to_string_lossy().into_owned();
    // A final thumbnail that landed meanwhile (the background pass, an edit) wins; the answer is what the catalog holds.
    let held = {
        let catalog = catalog.lock().ok()?;
        if record_thumbnail(&catalog, source.image_id, &out_path, ThumbTier::Large) {
            out_path_str
        } else {
            catalog.get_thumbnail_path(source.image_id).ok().flatten().unwrap_or(out_path_str)
        }
    };
    remove_lower_tier_files(thumbnail_dir, source.image_id, ThumbTier::Large);
    Some(held)
}

/// Thumbnail refresh after a Develop edit: unlike `generate_missing_thumbnails`
/// above (a fresh, unedited RAW/JPEG decode), this takes the edited picture
/// from `preview_cache::ensure_graded_preview_for_hash` -- the same cached
/// ~2048 px render the Library's Loupe view uses, built here if it is not
/// there yet -- and downscales it with the same cap/filter as an unedited
/// thumbnail so there's no visible size/quality mismatch in the grid.
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
    denoise_dir: Option<&Path>,
) -> Option<PathBuf> {
    // The edited picture is built ONCE, by the same cached call the Library's Loupe view makes
    // (`ensure_graded_preview_for_hash`: the draft preview, the AI-denoised copy when the stack asks for one,
    // lens / perspective / edit stack / crop), and the thumbnail is that picture shrunk. So leaving Develop pays
    // for the edit pipeline once and the Loupe's first open is a cache hit (it was a ~2 s cold render per edit
    // before: the thumbnail ran the pipeline and threw the result away).
    let graded = crate::preview_cache::ensure_graded_preview_for_hash(source_path, content_hash, stack, previews_dir, denoise_dir).ok()?;
    let decoded = image::open(&graded.path).ok()?.into_rgb8();
    // RFC-0013 / RFC-0027: the file name is keyed by what the picture was built from -- the effective stack
    // (a hidden panel's ops stripped) and the kept denoised copy, if one went in.
    let effective = crate::develop_engine::effective_stack_for_render(stack);
    let denoise_tag = crate::denoise_preview::graded_tag(crate::develop_engine::ai_denoise_amount(&effective), denoise_dir, content_hash);
    let stack = &effective;

    let resized = downscale_to(decoded, ThumbTier::Large.max_dimension());

    let stack_json = serde_json::to_string(stack).ok()?;
    let stack_hash = blake3::hash(stack_json.as_bytes()).to_hex().to_string();
    let out_path = thumbnail_dir.join(format!("{image_id}-{}{denoise_tag}.jpg", &stack_hash[..8]));
    let _ = std::fs::create_dir_all(thumbnail_dir);
    write_thumbnail_jpeg(&resized, &out_path).ok()?;
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

    /// RFC-0030 slice A: the streaming pass reports each thumbnail the moment it is recorded, with the path the
    /// catalog now holds, once per image and in order.
    #[test]
    fn the_streaming_pass_reports_each_thumbnail_as_it_is_written() {
        let (dir, thumb_dir) = scan_and_thumb_dirs("streaming");
        image::RgbImage::from_pixel(200, 100, image::Rgb([120, 90, 60])).save(dir.join("a.jpg")).unwrap();
        image::RgbImage::from_pixel(200, 100, image::Rgb([10, 20, 30])).save(dir.join("b.jpg")).unwrap();
        let catalog = Catalog::open_in_memory().unwrap();
        assert_eq!(scan_and_import(&dir, &catalog, &thumb_dir).imported, 2);
        let catalog = Arc::new(Mutex::new(catalog));

        let ready = std::cell::RefCell::new(Vec::new());
        generate_missing_thumbnails_streaming(&catalog, &thumb_dir, None, ThumbTier::Large, |_, _| {}, |id, path| {
            // The catalog already holds the path when the event fires.
            let held = catalog.lock().unwrap().get_thumbnail_path(id).unwrap();
            ready.borrow_mut().push((id, path.to_path_buf(), held));
        });

        let ready = ready.into_inner();
        assert_eq!(ready.len(), 2);
        for (id, path, held) in &ready {
            assert!(path.exists(), "the file is on disk when it is announced");
            assert_eq!(held.as_deref(), Some(path.to_string_lossy().as_ref()), "image {id}");
        }
        let _ = std::fs::remove_dir_all(dir.parent().unwrap());
    }

    #[test]
    fn the_batcher_sends_the_first_at_once_then_groups_by_interval_and_hands_out_the_rest() {
        use std::time::{Duration, Instant};
        let item = |id: i64| ReadyThumbnail { image_id: id, thumbnail_path: format!("/t/{id}.jpg") };
        let t0 = Instant::now();
        let mut b = ReadyBatcher::new(Duration::from_millis(50));
        assert_eq!(b.push(item(1), t0), Some(vec![item(1)]), "the first picture goes out immediately");
        assert_eq!(b.push(item(2), t0 + Duration::from_millis(10)), None);
        assert_eq!(b.push(item(3), t0 + Duration::from_millis(49)), None);
        assert_eq!(b.push(item(4), t0 + Duration::from_millis(50)), Some(vec![item(2), item(3), item(4)]));
        assert_eq!(b.push(item(5), t0 + Duration::from_millis(60)), None);
        assert_eq!(b.take(), vec![item(5)]);
        assert!(b.take().is_empty());
    }

    #[test]
    fn the_worker_count_leaves_cores_free_and_is_capped_by_the_work() {
        assert_eq!(thumbnail_worker_count(10, 1000), 8, "capped at eight");
        assert_eq!(thumbnail_worker_count(8, 1000), 6, "two cores stay free");
        assert_eq!(thumbnail_worker_count(2, 1000), 1, "never zero");
        assert_eq!(thumbnail_worker_count(1, 1000), 1);
        assert_eq!(thumbnail_worker_count(10, 3), 3, "no more threads than pictures");
        assert_eq!(thumbnail_worker_count(10, 0), 1);
    }

    /// RFC-0030 slice B: with several workers every picture still gets made, recorded and announced exactly once, and
    /// the progress count only goes up to the total.
    #[test]
    fn a_pool_makes_every_thumbnail_once_and_counts_progress_up_to_the_total() {
        let (dir, thumb_dir) = scan_and_thumb_dirs("pool");
        for n in 0..12u8 {
            image::RgbImage::from_pixel(120 + n as u32, 80, image::Rgb([n * 20, 90, 60])).save(dir.join(format!("p{n}.jpg"))).unwrap();
        }
        let catalog = Catalog::open_in_memory().unwrap();
        assert_eq!(scan_and_import(&dir, &catalog, &thumb_dir).imported, 12);
        let catalog = Arc::new(Mutex::new(catalog));

        let progress = std::cell::RefCell::new(Vec::new());
        let ready = std::cell::RefCell::new(Vec::new());
        generate_missing_thumbnails_with_workers(
            &catalog,
            &thumb_dir,
            None,
            ThumbTier::Large,
            Some(4),
            |current, total| progress.borrow_mut().push((current, total)),
            |id, path| {
                assert!(path.exists());
                ready.borrow_mut().push(id);
            },
        );

        assert_eq!(progress.into_inner(), (1..=12).map(|c| (c, 12)).collect::<Vec<_>>());
        let mut ready = ready.into_inner();
        ready.sort_unstable();
        ready.dedup();
        assert_eq!(ready.len(), 12, "each image announced once");
        let images = catalog.lock().unwrap().list_images().unwrap();
        assert!(images.iter().all(|i| i.thumbnail_path.is_some()));
        let _ = std::fs::remove_dir_all(dir.parent().unwrap());
    }

    /// A file the pool cannot decode costs its own picture only: the others are made and every candidate is counted.
    #[test]
    fn a_picture_that_cannot_be_made_does_not_stop_the_pool() {
        let (dir, thumb_dir) = scan_and_thumb_dirs("pool-bad");
        for n in 0..5u8 {
            image::RgbImage::from_pixel(100, 60, image::Rgb([n * 40, 10, 10])).save(dir.join(format!("ok{n}.jpg"))).unwrap();
        }
        let catalog = Catalog::open_in_memory().unwrap();
        assert_eq!(scan_and_import(&dir, &catalog, &thumb_dir).imported, 5);
        // One file vanishes between cataloguing and the pass.
        std::fs::remove_file(dir.join("ok2.jpg")).unwrap();
        let catalog = Arc::new(Mutex::new(catalog));

        let last = std::cell::Cell::new((0, 0));
        generate_missing_thumbnails_with_workers(&catalog, &thumb_dir, None, ThumbTier::Large, Some(3), |c, t| last.set((c, t)), |_, _| {});

        assert_eq!(last.get(), (5, 5));
        let done = catalog.lock().unwrap().list_images().unwrap().iter().filter(|i| i.thumbnail_path.is_some()).count();
        assert_eq!(done, 4);
        let _ = std::fs::remove_dir_all(dir.parent().unwrap());
    }

    /// RFC-0030 slice C: the streaming import catalogs every file, makes every JPEG's thumbnail before it returns, and
    /// announces each one once; a duplicate and a corrupt file are counted, not made into pictures.
    #[test]
    fn the_streaming_import_catalogs_and_thumbnails_every_jpeg_before_it_returns() {
        let (dir, thumb_dir) = scan_and_thumb_dirs("streaming-import");
        for n in 0..9u8 {
            image::RgbImage::from_pixel(160 + n as u32, 90, image::Rgb([n * 25, 80, 40])).save(dir.join(format!("s{n}.jpg"))).unwrap();
        }
        std::fs::copy(dir.join("s0.jpg"), dir.join("s0-copy.jpg")).unwrap(); // same bytes: a duplicate
        std::fs::write(dir.join("broken.jpg"), b"not a jpeg").unwrap();
        let catalog = Arc::new(Mutex::new(Catalog::open_in_memory().unwrap()));

        let progress = std::cell::RefCell::new(Vec::new());
        let ready = std::cell::RefCell::new(Vec::new());
        let summary = import_paths_streaming(
            std::slice::from_ref(&dir),
            &catalog,
            &thumb_dir,
            Some(3),
            |c, t| progress.borrow_mut().push((c, t)),
            |id, path| {
                assert!(path.exists());
                ready.borrow_mut().push(id);
            },
        );

        assert_eq!((summary.imported, summary.skipped_duplicates, summary.failed), (9, 1, 1));
        let progress = progress.into_inner();
        assert_eq!(progress.len(), 11);
        assert!(progress.iter().enumerate().all(|(i, &(c, t))| (c, t) == (i + 1, 11)));
        let images = catalog.lock().unwrap().list_images().unwrap();
        assert_eq!(images.len(), 9);
        assert!(images.iter().all(|i| i.thumbnail_path.is_some() && i.import_batch == Some(summary.import_batch)));
        let mut ready = ready.into_inner();
        ready.sort_unstable();
        ready.dedup();
        assert_eq!(ready.len(), 9, "each thumbnail announced once");
        let _ = std::fs::remove_dir_all(dir.parent().unwrap());
    }

    /// The point of the per-step lock: while the import runs, the rest of the app can read the catalog and sees the
    /// rows that have landed so far. (With the old whole-import lock this would deadlock or only ever see the end.)
    #[test]
    fn the_catalog_is_free_between_files_of_a_streaming_import() {
        let (dir, thumb_dir) = scan_and_thumb_dirs("streaming-free");
        for n in 0..5u8 {
            image::RgbImage::from_pixel(100, 60, image::Rgb([n * 40, 5, 5])).save(dir.join(format!("f{n}.jpg"))).unwrap();
        }
        let catalog = Arc::new(Mutex::new(Catalog::open_in_memory().unwrap()));
        let seen = std::cell::RefCell::new(Vec::new());
        import_paths_streaming(std::slice::from_ref(&dir), &catalog, &thumb_dir, Some(1), |_, _| {
            seen.borrow_mut().push(catalog.lock().unwrap().list_images().unwrap().len());
        }, |_, _| {});
        assert_eq!(seen.into_inner(), vec![1, 2, 3, 4, 5]);
        let _ = std::fs::remove_dir_all(dir.parent().unwrap());
    }

    /// A RAW file keeps its on-the-spot embedded thumbnail (no pool job, no announcement) in the streaming import too.
    #[test]
    fn a_raw_file_in_a_streaming_import_gets_its_embedded_thumbnail_at_once() {
        let raw = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test_image/hdr-bracket-cc-by/setup1_complex_gt1.cr2");
        if !raw.exists() {
            eprintln!("skipping: {} not present", raw.display());
            return;
        }
        let (dir, thumb_dir) = scan_and_thumb_dirs("streaming-raw");
        std::fs::copy(&raw, dir.join("one.cr2")).unwrap();
        let catalog = Arc::new(Mutex::new(Catalog::open_in_memory().unwrap()));
        let announced = std::cell::Cell::new(0);
        let summary = import_paths_streaming(std::slice::from_ref(&dir), &catalog, &thumb_dir, Some(2), |_, _| {}, |_, _| announced.set(announced.get() + 1));
        assert_eq!(summary.imported, 1);
        assert_eq!(announced.get(), 0, "written during cataloguing, so it arrives with the row");
        assert!(catalog.lock().unwrap().list_images().unwrap()[0].thumbnail_path.is_some());
        let _ = std::fs::remove_dir_all(dir.parent().unwrap());
    }

    /// RFC-0030 slice D: a JPEG that carries a camera thumbnail gets it as the row's picture at once (tier 0, named
    /// `{id}-t.jpg`); a JPEG that does not stays blank, as before.
    #[test]
    fn a_jpegs_own_exif_thumbnail_is_its_picture_the_moment_it_is_catalogued() {
        let (dir, thumb_dir) = scan_and_thumb_dirs("tier0");
        std::fs::write(dir.join("with.jpg"), crate::metadata::test_support::jpeg_with_exif_thumbnail((640, 480), (160, 120), None)).unwrap();
        image::RgbImage::from_pixel(64, 48, image::Rgb([9, 9, 9])).save(dir.join("without.jpg")).unwrap();
        let catalog = Catalog::open_in_memory().unwrap();
        assert_eq!(scan_and_import(&dir, &catalog, &thumb_dir).imported, 2);

        let images = catalog.list_images().unwrap();
        let with = images.iter().find(|i| i.path.ends_with("with.jpg")).unwrap();
        let without = images.iter().find(|i| i.path.ends_with("without.jpg")).unwrap();
        let tier0 = with.thumbnail_path.as_deref().expect("tier 0 recorded");
        assert!(thumbnail_rank(Some(tier0)) == 1 && Path::new(tier0).exists());
        assert!(without.thumbnail_path.is_none());
        let _ = std::fs::remove_dir_all(dir.parent().unwrap());
    }

    /// The real thumbnail replaces tier 0: a pass treats a placeholder as "still missing", records the real file,
    /// announces it, and deletes the tier-0 file.
    #[test]
    fn the_real_thumbnail_replaces_the_tier_0_picture() {
        let (dir, thumb_dir) = scan_and_thumb_dirs("tier0-replace");
        std::fs::write(dir.join("with.jpg"), crate::metadata::test_support::jpeg_with_exif_thumbnail((640, 480), (160, 120), None)).unwrap();
        let catalog = Catalog::open_in_memory().unwrap();
        assert_eq!(scan_and_import(&dir, &catalog, &thumb_dir).imported, 1);
        let before = catalog.list_images().unwrap()[0].thumbnail_path.clone().unwrap();
        let catalog = Arc::new(Mutex::new(catalog));

        let ready = std::cell::RefCell::new(Vec::new());
        generate_missing_thumbnails_streaming(&catalog, &thumb_dir, None, ThumbTier::Large, |_, _| {}, |id, path| ready.borrow_mut().push((id, path.to_path_buf())));

        let after = catalog.lock().unwrap().list_images().unwrap()[0].thumbnail_path.clone().unwrap();
        assert!(thumbnail_rank(Some(&after)) == 3 && Path::new(&after).exists());
        assert!(!Path::new(&before).exists(), "the tier-0 file is removed");
        assert_eq!(ready.into_inner().len(), 1);
        let _ = std::fs::remove_dir_all(dir.parent().unwrap());
    }

    /// The streaming import replaces tier 0 itself (the pool makes the small, 512 px one): every row ends with a
    /// small thumbnail and no tier-0 file is left behind.
    #[test]
    fn a_streaming_import_ends_with_small_thumbnails_even_where_tier_0_was_written() {
        let (dir, thumb_dir) = scan_and_thumb_dirs("tier0-streaming");
        for n in 0..4u32 {
            let mut file = crate::metadata::test_support::jpeg_with_exif_thumbnail((640 + n, 480), (160, 120), None);
            file.extend(format!("unique-{n}").bytes());
            std::fs::write(dir.join(format!("c{n}.jpg")), file).unwrap();
        }
        let catalog = Arc::new(Mutex::new(Catalog::open_in_memory().unwrap()));
        let summary = import_paths_streaming(std::slice::from_ref(&dir), &catalog, &thumb_dir, Some(2), |_, _| {}, |_, _| {});
        assert_eq!(summary.imported, 4);
        let images = catalog.lock().unwrap().list_images().unwrap();
        assert!(images.iter().all(|i| thumbnail_rank(i.thumbnail_path.as_deref()) == 2));
        assert!(std::fs::read_dir(&thumb_dir).unwrap().flatten().all(|e| !e.file_name().to_string_lossy().ends_with("-t.jpg")), "no tier-0 file is left behind");
        let _ = std::fs::remove_dir_all(dir.parent().unwrap());
    }

    /// RFC-0030 slice E: a big photo gets a 512 px thumbnail from the import and a 1024 px one from the background
    /// pass, which replaces it, announces it and deletes the small file.
    #[test]
    fn the_large_pass_upgrades_small_thumbnails_and_removes_them() {
        let (dir, thumb_dir) = scan_and_thumb_dirs("tiers");
        image::RgbImage::from_fn(2400, 1600, |x, y| image::Rgb([(x / 10) as u8, (y / 8) as u8, 90])).save(dir.join("big.jpg")).unwrap();
        let catalog = Arc::new(Mutex::new(Catalog::open_in_memory().unwrap()));
        import_paths_streaming(std::slice::from_ref(&dir), &catalog, &thumb_dir, Some(1), |_, _| {}, |_, _| {});

        let small = catalog.lock().unwrap().list_images().unwrap()[0].thumbnail_path.clone().unwrap();
        assert!(small.ends_with("-s.jpg"));
        assert_eq!(image::image_dimensions(&small).unwrap(), (512, 341));

        let ready = std::cell::RefCell::new(Vec::new());
        generate_missing_thumbnails_streaming(&catalog, &thumb_dir, None, ThumbTier::Large, |_, _| {}, |id, path| ready.borrow_mut().push((id, path.to_path_buf())));

        let large = catalog.lock().unwrap().list_images().unwrap()[0].thumbnail_path.clone().unwrap();
        assert_eq!(thumbnail_rank(Some(&large)), 3);
        assert_eq!(image::image_dimensions(&large).unwrap(), (1024, 683));
        assert!(!Path::new(&small).exists(), "the small file is removed");
        assert_eq!(ready.into_inner().len(), 1);

        // Nothing left to do: a second large pass announces nothing.
        let again = std::cell::Cell::new(0);
        generate_missing_thumbnails_streaming(&catalog, &thumb_dir, None, ThumbTier::Large, |_, _| {}, |_, _| again.set(again.get() + 1));
        assert_eq!(again.get(), 0);
        let _ = std::fs::remove_dir_all(dir.parent().unwrap());
    }

    /// The upgrade is quiet: while foreground work (an import, a small pass) is counted, the large pass waits.
    #[test]
    fn the_large_pass_waits_while_foreground_work_runs() {
        let (dir, thumb_dir) = scan_and_thumb_dirs("tiers-yield");
        image::RgbImage::from_pixel(1200, 800, image::Rgb([40, 80, 120])).save(dir.join("a.jpg")).unwrap();
        let catalog = Arc::new(Mutex::new(Catalog::open_in_memory().unwrap()));
        import_paths_streaming(std::slice::from_ref(&dir), &catalog, &thumb_dir, Some(1), |_, _| {}, |_, _| {});

        let guard = ForegroundWork::begin();
        let worker = {
            let (catalog, thumb_dir) = (catalog.clone(), thumb_dir.clone());
            std::thread::spawn(move || generate_missing_thumbnails_streaming(&catalog, &thumb_dir, None, ThumbTier::Large, |_, _| {}, |_, _| {}))
        };
        std::thread::sleep(std::time::Duration::from_millis(300));
        let held = catalog.lock().unwrap().list_images().unwrap()[0].thumbnail_path.clone().unwrap();
        assert!(held.ends_with("-s.jpg"), "still the small one while foreground work runs");
        drop(guard);
        worker.join().unwrap();
        let after = catalog.lock().unwrap().list_images().unwrap()[0].thumbnail_path.clone().unwrap();
        assert_eq!(thumbnail_rank(Some(&after)), 3);
        let _ = std::fs::remove_dir_all(dir.parent().unwrap());
    }

    #[test]
    fn downscaling_keeps_the_shape_never_enlarges_and_keeps_flat_colour() {
        let big = image::RgbImage::from_pixel(6000, 4000, image::Rgb([200, 100, 50]));
        let small = downscale_to(big, 512);
        assert_eq!((small.width(), small.height()), (512, 341));
        assert_eq!(small.get_pixel(256, 170).0, [200, 100, 50], "a flat colour survives the box average and the filter");

        let tiny = image::RgbImage::from_pixel(300, 200, image::Rgb([1, 2, 3]));
        assert_eq!(downscale_to(tiny, 512).dimensions(), (300, 200), "never enlarged");

        let tall = image::RgbImage::from_pixel(1500, 3000, image::Rgb([9, 9, 9]));
        assert_eq!(downscale_to(tall, 1024).dimensions(), (512, 1024));
    }

    #[test]
    fn the_tier_of_a_thumbnail_is_read_from_its_name() {
        assert_eq!(thumbnail_rank(None), 0);
        assert_eq!(thumbnail_rank(Some("/t/5-t.jpg")), 1);
        assert_eq!(thumbnail_rank(Some("/t/5-s.jpg")), 2);
        assert_eq!(thumbnail_rank(Some("/t/5.jpg")), 3);
        assert_eq!(thumbnail_rank(Some("/t/5-ab12cd34.jpg")), 3, "an edited picture is final");
        assert!(is_provisional_thumbnail("/t/5-s.jpg") && !is_provisional_thumbnail("/t/5.jpg"));
    }

    /// Recording only ever improves: nothing, tier 0 or small may be replaced by a better tier; an equal or better
    /// one is never replaced (an edited picture, a large one that got in first).
    #[test]
    fn recording_a_thumbnail_only_ever_improves_the_tier() {
        let (dir, thumb_dir) = scan_and_thumb_dirs("record-guard");
        image::RgbImage::from_pixel(64, 48, image::Rgb([9, 9, 9])).save(dir.join("a.jpg")).unwrap();
        let catalog = Catalog::open_in_memory().unwrap();
        scan_and_import(&dir, &catalog, &thumb_dir);
        let id = catalog.list_images().unwrap()[0].image_id;

        assert!(record_thumbnail(&catalog, id, Path::new("/t/1-s.jpg"), ThumbTier::Small), "nothing there: recorded");
        assert!(!record_thumbnail(&catalog, id, Path::new("/t/1-s2.jpg"), ThumbTier::Small), "small over small: kept");
        assert!(record_thumbnail(&catalog, id, Path::new("/t/1.jpg"), ThumbTier::Large), "small there: replaced by large");
        assert!(!record_thumbnail(&catalog, id, Path::new("/t/1-s.jpg"), ThumbTier::Small), "large there: kept");
        assert!(!record_thumbnail(&catalog, id, Path::new("/t/other.jpg"), ThumbTier::Large), "large over large: kept");
        assert_eq!(catalog.get_thumbnail_path(id).unwrap().as_deref(), Some("/t/1.jpg"));
        let _ = std::fs::remove_dir_all(dir.parent().unwrap());
    }

    /// Opening a photo that still shows tier 0 makes its real thumbnail at once, like one with none.
    #[test]
    fn ensure_thumbnail_upgrades_a_tier_0_picture() {
        let (dir, thumb_dir) = scan_and_thumb_dirs("tier0-ensure");
        std::fs::write(dir.join("with.jpg"), crate::metadata::test_support::jpeg_with_exif_thumbnail((640, 480), (160, 120), None)).unwrap();
        let catalog = Catalog::open_in_memory().unwrap();
        scan_and_import(&dir, &catalog, &thumb_dir);
        let version_id = catalog.list_images().unwrap()[0].version_id;
        let catalog = Arc::new(Mutex::new(catalog));

        let path = ensure_thumbnail(&catalog, version_id, &thumb_dir).expect("a thumbnail");
        assert!(thumbnail_rank(Some(&path)) == 3 && Path::new(&path).exists());
        assert_eq!(ensure_thumbnail(&catalog, version_id, &thumb_dir).as_deref(), Some(path.as_str()), "now a no-op");
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

    /// RFC-0027 slice 3d: the Library thumbnail is built from the kept AI-denoised copy when the stack asks for
    /// it, under a different name than one made before the copy existed.
    #[test]
    fn regenerate_edited_thumbnail_mixes_in_the_kept_denoised_copy() {
        let (dir, thumb_dir) = scan_and_thumb_dirs("regen-thumbnail-denoise");
        let previews_dir = dir.parent().unwrap().join("previews");
        let denoise_dir = dir.parent().unwrap().join("denoise");
        std::fs::create_dir_all(&denoise_dir).unwrap();
        let source_path = dir.join("photo.jpg");
        image::RgbImage::from_pixel(200, 100, image::Rgb([200, 200, 200])).save(&source_path).unwrap();
        let content_hash = blake3::hash(&std::fs::read(&source_path).unwrap()).to_hex().to_string();
        let stack = EditStack { schema_version: 1, ops: vec![serde_json::json!({"op": "ai_denoise", "amount": 100})] };

        let before = regenerate_edited_thumbnail(&source_path, &content_hash, 7, &stack, &previews_dir, &thumb_dir, Some(&denoise_dir)).unwrap();
        assert!(image::open(&before).unwrap().into_rgb8().get_pixel(2, 2).0[0] > 150, "no copy yet: plain");

        image::RgbImage::from_pixel(200, 100, image::Rgb([20, 20, 20]))
            .save(crate::denoise_cache::entry_path(&denoise_dir, &content_hash, crate::denoise_models::MODEL_ID))
            .unwrap();
        let after = regenerate_edited_thumbnail(&source_path, &content_hash, 7, &stack, &previews_dir, &thumb_dir, Some(&denoise_dir)).unwrap();
        assert_ne!(before, after, "a thumbnail made before the copy existed must not be reused");
        assert!(image::open(&after).unwrap().into_rgb8().get_pixel(2, 2).0[0] < 40, "amount 100 is the kept copy");

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

        let out_path = regenerate_edited_thumbnail(&source_path, &content_hash, 42, &stack, &previews_dir, &thumb_dir, None)
            .expect("regeneration should succeed for a real JPEG");

        assert!(out_path.exists());
        // The Loupe view's graded preview for this exact stack was built on the way, so its first open is a
        // cache hit (a ~2 s cold render per edit otherwise).
        let graded = crate::preview_cache::ensure_graded_preview_for_hash(&source_path, &content_hash, &stack, &previews_dir, None).unwrap();
        let before = std::fs::metadata(&graded.path).unwrap().modified().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        let again = crate::preview_cache::ensure_graded_preview_for_hash(&source_path, &content_hash, &stack, &previews_dir, None).unwrap();
        assert_eq!(std::fs::metadata(&again.path).unwrap().modified().unwrap(), before, "not rebuilt");
        let graded_files = std::fs::read_dir(&previews_dir).unwrap().flatten().filter(|e| e.file_name().to_string_lossy().ends_with("_graded.png")).count();
        assert_eq!(graded_files, 1, "regenerating the thumbnail left exactly the Loupe's graded preview behind");
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
            regenerate_edited_thumbnail(&source_path, &content_hash, 42, &stack, &previews_dir, &thumb_dir, None).unwrap();
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

/// Measurements behind docs/rfc/RFC-0030-import-speed.md. Not a unit test: ignored unless asked for, prints a table.
///
///   IMPORT_PROBE_DIR=/path/with/jpegs-and-raws cargo test --release --lib speed_probe -- --ignored --nocapture
///
/// Files are read once to warm the page cache first (the numbers are CPU and memory, not a cold disk), each stage is
/// the best of three runs, and the parallel part runs the whole directory several times over through N workers.
#[cfg(test)]
mod speed_probe {
    use super::*;
    use std::time::{Duration, Instant};

    const TARGET: u32 = 512;

    fn best<T>(mut f: impl FnMut() -> T) -> (Duration, T) {
        let mut best: Option<(Duration, T)> = None;
        for _ in 0..3 {
            let t = Instant::now();
            let v = f();
            let d = t.elapsed();
            if best.as_ref().is_none_or(|(b, _)| d < *b) {
                best = Some((d, v));
            }
        }
        best.unwrap()
    }

    fn ms(d: Duration) -> f64 {
        d.as_secs_f64() * 1000.0
    }

    fn files(dir: &Path, exts: &[&str]) -> Vec<PathBuf> {
        let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|e| e.to_str()).is_some_and(|e| exts.contains(&e.to_lowercase().as_str())))
            .collect();
        v.sort();
        v
    }

    fn orientation(bytes: &[u8]) -> u32 {
        let mut c = std::io::Cursor::new(bytes);
        exif::Reader::new()
            .read_from_container(&mut c)
            .ok()
            .and_then(|e| e.get_field(exif::Tag::Orientation, exif::In::PRIMARY).and_then(|f| f.value.get_uint(0)))
            .filter(|v| (1..=8).contains(v))
            .unwrap_or(1)
    }

    fn orient(img: image::RgbImage, o: u32) -> image::RgbImage {
        use image::imageops::*;
        match o {
            2 => flip_horizontal(&img),
            3 => rotate180(&img),
            4 => flip_vertical(&img),
            5 => flip_horizontal(&rotate90(&img)),
            6 => rotate90(&img),
            7 => flip_horizontal(&rotate270(&img)),
            8 => rotate270(&img),
            _ => img,
        }
    }

    /// Exact k x k box average, row by row (cheap, no float weights): the prefilter of the fast path.
    fn box_down(src: &image::RgbImage, k: u32) -> image::RgbImage {
        let (w, h) = (src.width() / k, src.height() / k);
        let sw = src.width() as usize;
        let raw = src.as_raw();
        let mut out = vec![0u8; (w * h * 3) as usize];
        let kk = k * k;
        for y in 0..h as usize {
            for x in 0..w as usize {
                let mut acc = [0u32; 3];
                for dy in 0..k as usize {
                    let row = ((y * k as usize + dy) * sw + x * k as usize) * 3;
                    for dx in 0..k as usize {
                        let p = row + dx * 3;
                        acc[0] += raw[p] as u32;
                        acc[1] += raw[p + 1] as u32;
                        acc[2] += raw[p + 2] as u32;
                    }
                }
                let o = (y * w as usize + x) * 3;
                out[o] = (acc[0] / kk) as u8;
                out[o + 1] = (acc[1] / kk) as u8;
                out[o + 2] = (acc[2] / kk) as u8;
            }
        }
        image::RgbImage::from_raw(w, h, out).unwrap()
    }

    fn encode(img: &image::RgbImage, q: u8) -> Vec<u8> {
        let mut out = Vec::new();
        jpeg_encoder::Encoder::new(&mut out, q).encode(img.as_raw(), img.width() as u16, img.height() as u16, jpeg_encoder::ColorType::Rgb).unwrap();
        out
    }

    /// What the app does today for a JPEG's grid thumbnail (see `generate_thumbnail_file` + `jpeg_decode::decode`).
    fn current(bytes: &[u8]) -> Vec<u8> {
        let o = orientation(bytes);
        let full = image::load_from_memory_with_format(bytes, image::ImageFormat::Jpeg).unwrap().to_rgb8();
        let full = orient(full, o);
        let (w, h) = (full.width(), full.height());
        let small = if w.max(h) > 1024 {
            let s = 1024.0 / w.max(h) as f64;
            image::imageops::resize(&full, (w as f64 * s).round() as u32, (h as f64 * s).round() as u32, image::imageops::FilterType::Triangle)
        } else {
            full
        };
        let mut out = Vec::new();
        small.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Jpeg).unwrap();
        out
    }

    /// The candidate: full decode (the only decoder in the tree), box prefilter, triangle for the remainder, orient
    /// the SMALL picture, encode with jpeg-encoder at `TARGET`.
    fn candidate(bytes: &[u8]) -> Vec<u8> {
        let o = orientation(bytes);
        let full = image::load_from_memory_with_format(bytes, image::ImageFormat::Jpeg).unwrap().to_rgb8();
        let k = (full.width().max(full.height()) / (TARGET * 2)).max(1);
        let pre = if k > 1 { box_down(&full, k) } else { full };
        let (w, h) = (pre.width(), pre.height());
        let small = if w.max(h) > TARGET {
            let s = TARGET as f64 / w.max(h) as f64;
            image::imageops::resize(&pre, (w as f64 * s).round().max(1.0) as u32, (h as f64 * s).round().max(1.0) as u32, image::imageops::FilterType::Triangle)
        } else {
            pre
        };
        encode(&orient(small, o), 82)
    }

    #[test]
    #[ignore = "measurement, needs IMPORT_PROBE_DIR"]
    fn jpeg_stages() {
        let dir = PathBuf::from(std::env::var("IMPORT_PROBE_DIR").expect("IMPORT_PROBE_DIR"));
        println!("\nJPEG per-file stages, ms (best of 3, warm cache), {} cores", std::thread::available_parallelism().unwrap());
        println!("{:<34} {:>5} {:>5} | {:>6} {:>6} {:>6} | {:>7} {:>7} {:>7} {:>7} | {:>8} {:>8}", "file", "MB", "MP", "read", "hash", "exif", "decode", "orient", "resize", "encode", "current", "cand.");
        let (mut tot_cur, mut tot_cand) = (0.0, 0.0);
        for p in files(&dir, &["jpg", "jpeg"]) {
            let (t_read, bytes) = best(|| std::fs::read(&p).unwrap());
            let (t_hash, _) = best(|| blake3::hash(&bytes));
            let (t_exif, o) = best(|| {
                let _ = metadata::extract_from_jpeg(&bytes);
                orientation(&bytes)
            });
            let (t_dec, full) = best(|| image::load_from_memory_with_format(&bytes, image::ImageFormat::Jpeg).unwrap().to_rgb8());
            let (t_or, full) = best(|| orient(full.clone(), o));
            let mp = full.width() as f64 * full.height() as f64 / 1e6;
            let (t_rs, small) = best(|| {
                let s = 1024.0 / full.width().max(full.height()) as f64;
                image::imageops::resize(&full, (full.width() as f64 * s).round() as u32, (full.height() as f64 * s).round() as u32, image::imageops::FilterType::Triangle)
            });
            let (t_enc, _) = best(|| {
                let mut out = Vec::new();
                small.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Jpeg).unwrap();
                out
            });
            let (t_cur, cur) = best(|| current(&bytes));
            let (t_cand, cand) = best(|| candidate(&bytes));
            tot_cur += ms(t_cur);
            tot_cand += ms(t_cand);
            println!(
                "{:<34} {:>5.1} {:>5.1} | {:>6.1} {:>6.1} {:>6.1} | {:>7.1} {:>7.1} {:>7.1} {:>7.1} | {:>8.1} {:>8.1}   thumb {} KB -> {} KB",
                p.file_name().unwrap().to_string_lossy().chars().take(34).collect::<String>(),
                bytes.len() as f64 / 1e6, mp, ms(t_read), ms(t_hash), ms(t_exif), ms(t_dec), ms(t_or), ms(t_rs), ms(t_enc), ms(t_cur), ms(t_cand),
                cur.len() / 1000, cand.len() / 1000
            );
        }
        println!("total per-set thumbnail time: current {tot_cur:.0} ms, candidate {tot_cand:.0} ms");
    }

    #[test]
    #[ignore = "measurement, needs IMPORT_PROBE_DIR"]
    fn raw_stages() {
        let dir = PathBuf::from(std::env::var("IMPORT_PROBE_DIR").expect("IMPORT_PROBE_DIR"));
        println!("\nRAW per-file stages, ms (best of 3, warm cache)");
        for p in files(&dir, &["cr2", "cr3", "dng", "nef", "arw"]) {
            let (t_read, bytes) = best(|| std::fs::read(&p).unwrap());
            let (t_hash, _) = best(|| blake3::hash(&bytes));
            let (t_open, _) = best(|| RawImage::open(&bytes).map(|_| ()).is_ok());
            let mut raw = RawImage::open(&bytes).unwrap();
            let (t_thumbs, sizes) = best(|| raw.extract_thumbs().map(|t| t.iter().map(|x| (x.width, x.height, x.data.len())).collect::<Vec<_>>()).unwrap_or_default());
            let largest = raw.extract_thumbs().unwrap();
            let largest = largest.last().unwrap();
            let (t_dec, img) = best(|| image::load_from_memory_with_format(&largest.data, image::ImageFormat::Jpeg).unwrap().to_rgb8());
            let (t_small, _) = best(|| {
                let k = (img.width().max(img.height()) / (TARGET * 2)).max(1);
                let pre = if k > 1 { box_down(&img, k) } else { img.clone() };
                let s = TARGET as f64 / pre.width().max(pre.height()) as f64;
                encode(&image::imageops::resize(&pre, (pre.width() as f64 * s).round() as u32, (pre.height() as f64 * s).round() as u32, image::imageops::FilterType::Triangle), 82)
            });
            println!(
                "{:<30} {:>5.1} MB | read {:>5.1} hash {:>5.1} open {:>6.1} extract_thumbs {:>6.1} | embedded {:?} | decode-embedded {:>6.1} -> 512px+encode {:>6.1}",
                p.file_name().unwrap().to_string_lossy(), bytes.len() as f64 / 1e6, ms(t_read), ms(t_hash), ms(t_open), ms(t_thumbs), sizes, ms(t_dec), ms(t_small)
            );
        }
    }

    /// Whole-set throughput through N workers pulling from one queue (the shape the fast path would have).
    #[test]
    #[ignore = "measurement, needs IMPORT_PROBE_DIR"]
    fn parallel_scaling() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let dir = PathBuf::from(std::env::var("IMPORT_PROBE_DIR").expect("IMPORT_PROBE_DIR"));
        let paths: Vec<PathBuf> = std::iter::repeat_n(files(&dir, &["jpg", "jpeg"]), 4).flatten().collect();
        let warm: u64 = paths.iter().map(|p| std::fs::metadata(p).unwrap().len()).sum();
        println!("\nparallel scaling over {} JPEG files ({:.0} MB), read + hash + exif + thumbnail, ms to finish all / to the FIRST thumbnail", paths.len(), warm as f64 / 1e6);
        for (label, f) in [("current", current as fn(&[u8]) -> Vec<u8>), ("candidate", candidate as fn(&[u8]) -> Vec<u8>)] {
            for workers in [1usize, 2, 4, 8, 10] {
                let next = AtomicUsize::new(0);
                let first = std::sync::Mutex::new(None::<Duration>);
                let start = Instant::now();
                std::thread::scope(|s| {
                    for _ in 0..workers {
                        s.spawn(|| loop {
                            let i = next.fetch_add(1, Ordering::SeqCst);
                            let Some(p) = paths.get(i) else { break };
                            let bytes = std::fs::read(p).unwrap();
                            let _ = blake3::hash(&bytes);
                            let _ = metadata::extract_from_jpeg(&bytes);
                            let _ = f(&bytes);
                            first.lock().unwrap().get_or_insert(start.elapsed());
                        });
                    }
                });
                let total = start.elapsed();
                println!("  {label:<9} workers {workers:>2}: all {:>7.0} ms ({:>5.1} files/s), first thumbnail {:>6.0} ms", ms(total), paths.len() as f64 / total.as_secs_f64(), ms(first.lock().unwrap().unwrap()));
            }
        }
    }
    /// The real pass end to end (cataloguing, then the thumbnail pass), 1 worker against the pool, with the time to the
    /// first finished picture. Point `IMPORT_PROBE_DIR` at a folder of JPEGs with unique content.
    #[test]
    #[ignore = "measurement, needs IMPORT_PROBE_DIR"]
    fn real_pass() {
        let dir = PathBuf::from(std::env::var("IMPORT_PROBE_DIR").expect("IMPORT_PROBE_DIR"));
        let cores = std::thread::available_parallelism().unwrap().get();
        println!("\nreal pass: cataloguing, then the thumbnail pass ({} cores)", cores);
        for workers in [1, 2, 4, thumbnail_worker_count(cores, usize::MAX)] {
            let thumbs = std::env::temp_dir().join(format!("emulsion-probe-thumbs-{workers}"));
            let _ = std::fs::remove_dir_all(&thumbs);
            let catalog = Catalog::open_in_memory().unwrap();
            let t = Instant::now();
            let summary = scan_and_import(&dir, &catalog, &thumbs);
            let catalog_ms = ms(t.elapsed());
            let catalog = Arc::new(Mutex::new(catalog));
            let first = std::cell::Cell::new(None::<Duration>);
            let t = Instant::now();
            generate_missing_thumbnails_with_workers(&catalog, &thumbs, None, ThumbTier::Large, Some(workers), |_, _| {}, |_, _| {
                first.set(first.get().or(Some(t.elapsed())));
            });
            println!(
                "  workers {workers:>2}: imported {} in {catalog_ms:>6.0} ms, thumbnails: first {:>5.0} ms, all {:>6.0} ms",
                summary.imported,
                ms(first.get().unwrap_or_default()),
                ms(t.elapsed())
            );
            let _ = std::fs::remove_dir_all(&thumbs);
        }
        println!("overlapped (slice C): thumbnails are made while the files are still being read");
        for workers in [1, 4, thumbnail_worker_count(cores, usize::MAX)] {
            let thumbs = std::env::temp_dir().join(format!("emulsion-probe-thumbs-c{workers}"));
            let _ = std::fs::remove_dir_all(&thumbs);
            let catalog = Arc::new(Mutex::new(Catalog::open_in_memory().unwrap()));
            let first = std::cell::Cell::new(None::<Duration>);
            let t = Instant::now();
            let summary = import_paths_streaming(std::slice::from_ref(&dir), &catalog, &thumbs, Some(workers), |_, _| {}, |_, _| {
                first.set(first.get().or(Some(t.elapsed())));
            });
            println!(
                "  workers {workers:>2}: imported {} and every thumbnail made in {:>6.0} ms, first picture {:>5.0} ms",
                summary.imported,
                ms(t.elapsed()),
                ms(first.get().unwrap_or_default())
            );
            let _ = std::fs::remove_dir_all(&thumbs);
        }
    }

    /// How many real JPEGs carry a usable EXIF thumbnail (tier 0, RFC-0030 slice D), and what extraction costs.
    #[test]
    #[ignore = "measurement, needs IMPORT_PROBE_DIR"]
    fn exif_thumbnails() {
        let dir = PathBuf::from(std::env::var("IMPORT_PROBE_DIR").expect("IMPORT_PROBE_DIR"));
        println!("\nEXIF thumbnail (tier 0) per JPEG");
        for p in files(&dir, &["jpg", "jpeg"]) {
            let bytes = std::fs::read(&p).unwrap();
            let (t, thumb) = best(|| metadata::exif_thumbnail(&bytes));
            let o = orientation(&bytes);
            match thumb {
                Some(b) => {
                    let (w, h) = image::ImageReader::new(std::io::Cursor::new(&b)).with_guessed_format().unwrap().into_dimensions().unwrap();
                    println!("  {:<40} orientation {o}: {w}x{h}, {} bytes, {:.2} ms", p.file_name().unwrap().to_string_lossy(), b.len(), ms(t));
                }
                None => println!("  {:<40} orientation {o}: none ({:.2} ms)", p.file_name().unwrap().to_string_lossy(), ms(t)),
            }
        }
    }

    /// Writes the old thumbnail (image crate resize, 1024) and the new ones (box prefilter, 1024 and 512) of every
    /// JPEG to `IMPORT_PROBE_OUT`, and prints how far the new 1024 is from the old (PSNR), for looking at by eye.
    #[test]
    #[ignore = "measurement, needs IMPORT_PROBE_DIR and IMPORT_PROBE_OUT"]
    fn compare_thumbnails() {
        let dir = PathBuf::from(std::env::var("IMPORT_PROBE_DIR").expect("IMPORT_PROBE_DIR"));
        let out = PathBuf::from(std::env::var("IMPORT_PROBE_OUT").expect("IMPORT_PROBE_OUT"));
        std::fs::create_dir_all(&out).unwrap();
        println!("\nold 1024 vs new 1024 (PSNR, higher = closer; > 40 dB is visually identical)");
        for p in files(&dir, &["jpg", "jpeg"]) {
            let stem = p.file_stem().unwrap().to_string_lossy().into_owned();
            let bytes = std::fs::read(&p).unwrap();
            let old = current(&bytes);
            std::fs::write(out.join(format!("{stem}-old1024.jpg")), &old).unwrap();
            let full = crate::jpeg_decode::decode(&p).unwrap();
            let full = image::RgbImage::from_raw(full.width, full.height, full.rgb).unwrap();
            let new_large = downscale_to(full.clone(), 1024);
            let new_small = downscale_to(full, 512);
            write_thumbnail_jpeg(&new_large, &out.join(format!("{stem}-new1024.jpg"))).unwrap();
            write_thumbnail_jpeg(&new_small, &out.join(format!("{stem}-new512.jpg"))).unwrap();
            let old_img = image::load_from_memory(&old).unwrap().to_rgb8();
            let new_img = image::open(out.join(format!("{stem}-new1024.jpg"))).unwrap().to_rgb8();
            if old_img.dimensions() != new_img.dimensions() {
                println!("  {stem:<40} different size {:?} vs {:?}", old_img.dimensions(), new_img.dimensions());
                continue;
            }
            let mse = old_img.as_raw().iter().zip(new_img.as_raw()).map(|(a, b)| (*a as f64 - *b as f64).powi(2)).sum::<f64>() / old_img.as_raw().len() as f64;
            println!("  {stem:<40} {:.1} dB   ({} KB -> {} KB; 512: {} KB)", 10.0 * (255.0f64 * 255.0 / mse.max(1e-9)).log10(), old.len() / 1000, std::fs::metadata(out.join(format!("{stem}-new1024.jpg"))).unwrap().len() / 1000, std::fs::metadata(out.join(format!("{stem}-new512.jpg"))).unwrap().len() / 1000);
        }
    }
}
