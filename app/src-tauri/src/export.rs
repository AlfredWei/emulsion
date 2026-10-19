//! Export pipeline (M1 Slice 5) — see docs/PRD/PRD.md §7.5.
//!
//! Applies the current edit stack (via `develop_engine::apply_edit_stack`
//! -- see that module for the formula and its parity obligations) to a
//! full-resolution RAW decode and writes a real JPEG to disk. CPU-side
//! render, not native wgpu or the in-webview WebGPU path Develop uses --
//! per ADR-0004, the export path has no <100ms latency requirement, so the
//! constraints that shape the interactive pipeline don't apply here.
//!
//! Scope cut for this pass (see the plan): JPEG output only, single image
//! only (no multi-select UI in Library yet), long-edge-or-original resize,
//! one quality setting, no color management (none exists in this pipeline
//! yet -- see raw_decode.rs), no live progress stream. Source format is
//! not restricted -- source_decode.rs dispatches RAW or JPEG sources alike
//! (M2 Slice 1).

use crate::catalog::{EditStack, ExportPlugin};
use crate::denoise_cache;
use crate::develop_engine::{ai_denoise_amount, apply_crop, apply_edit_stack, apply_lens_correction, apply_perspective, blend_denoised};
use crate::export_plugin;
use crate::metadata_writer::{self, ExportMetadata, MetadataWriteOptions};
use crate::source_decode::{self, DecodeError};
use image::codecs::jpeg::JpegEncoder;
use image::RgbImage;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
pub struct ExportOptions {
    pub destination_dir: String,
    /// `None` means export at the source's native resolution.
    pub long_edge: Option<u32>,
    pub quality: u8,
    /// The export-plugin (RFC-0006) to run after each successfully
    /// exported file, if any -- an id, not the resolved row: this module
    /// stays catalog-free (see this file's own header comment), so
    /// `lib.rs`'s `export_images` command resolves this id to a real
    /// `ExportPlugin` under its existing catalog lock, the same place it
    /// already resolves each item's edit stack, and hands the resolved
    /// row to `export_batch` directly instead.
    pub plugin_id: Option<i64>,
    /// Which metadata groups to embed in the exported JPEG (EXIF, IPTC,
    /// and GPS within EXIF). Absent/all-false keeps the bare-JPEG output.
    #[serde(default)]
    pub metadata: MetadataWriteOptions,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExportResult {
    pub source_path: String,
    pub output_path: Option<String>,
    pub error: Option<String>,
    /// Set only when a configured export plugin's own `spawn()` fails
    /// (bad command, not found, not executable) -- kept separate from
    /// `error` above so a plugin misconfiguration never makes an
    /// otherwise-successful export look like a failed one (RFC-0006 §3.3).
    pub plugin_error: Option<String>,
    /// Set when the file exported fine but embedding the requested
    /// EXIF/IPTC failed, so the JPEG on disk has no metadata. Separate from
    /// `error` for the same reason as `plugin_error`: the export itself
    /// succeeded.
    pub metadata_warning: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error(transparent)]
    Decode(#[from] DecodeError),
    #[error("decoded RGB buffer size didn't match its own reported dimensions")]
    BufferMismatch,
    #[error(transparent)]
    Image(#[from] image::ImageError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// First `{stem}.{ext}`, `{stem}-1.{ext}`, `{stem}-2.{ext}`, ... that
/// doesn't already exist in `dir`. Inherently check-then-write (a
/// concurrent writer could still race it) -- the frontend disabling the
/// Export button while a request is in flight is what actually prevents
/// this app from double-firing the same export, so that's an accepted,
/// not a fixed, race.
pub(crate) fn unique_output_path(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    let candidate = dir.join(format!("{stem}.{ext}"));
    if !candidate.exists() {
        return candidate;
    }
    let mut n = 1u32;
    loop {
        let candidate = dir.join(format!("{stem}-{n}.{ext}"));
        if !candidate.exists() {
            return candidate;
        }
        n += 1;
    }
}

/// Writes `bytes` to the first free `{stem}.{ext}`, `{stem}-1.{ext}`, ... in `dir`, claiming the name atomically
/// (`create_new`), so two exports running at once (RFC-0030 slice F) that want the same name (`IMG_0001.CR2` and
/// `IMG_0001.JPG` both make `IMG_0001.jpg`) get different files instead of one overwriting the other. A failed write
/// removes the half-made file.
pub(crate) fn write_unique(dir: &Path, stem: &str, ext: &str, bytes: &[u8]) -> std::io::Result<PathBuf> {
    use std::io::Write;
    let mut n = 0u32;
    loop {
        let name = if n == 0 { format!("{stem}.{ext}") } else { format!("{stem}-{n}.{ext}") };
        let candidate = dir.join(name);
        match std::fs::OpenOptions::new().write(true).create_new(true).open(&candidate) {
            Ok(mut file) => {
                return match file.write_all(bytes).and_then(|_| file.flush()) {
                    Ok(()) => Ok(candidate),
                    Err(e) => {
                        drop(file);
                        let _ = std::fs::remove_file(&candidate);
                        Err(e)
                    }
                };
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => n += 1,
            Err(e) => return Err(e),
        }
    }
}

/// How many exports may render at once, and how much picture they may hold between them (RFC-0030 slice F). A render
/// keeps the decoded photo, the edited copy and the encoded bytes: measured on this machine, 8 threads over a mix of
/// 7 to 24 MP photos peaked at 2.8 GB (1 thread: 1.4 GB), about 35 MB per megapixel in flight; the budget assumes up
/// to 60 MB. The system's free memory is not probed (no such code exists in the app yet), so the budget is a
/// conservative constant: a single photo always runs, even above it.
pub(crate) struct ExportBudget {
    state: std::sync::Mutex<(usize, f64)>,
    freed: std::sync::Condvar,
    max_running: usize,
    max_megapixels: f64,
}

pub(crate) struct ExportSlot<'a> {
    budget: &'a ExportBudget,
    megapixels: f64,
}

impl ExportBudget {
    pub(crate) fn new(max_running: usize, max_megapixels: f64) -> Self {
        Self { state: std::sync::Mutex::new((0, 0.0)), freed: std::sync::Condvar::new(), max_running: max_running.max(1), max_megapixels }
    }

    /// Waits until `megapixels` more fit (or nothing else is running), then holds that share until the slot drops.
    pub(crate) fn acquire(&self, megapixels: f64) -> ExportSlot<'_> {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        while state.0 > 0 && (state.0 >= self.max_running || state.1 + megapixels > self.max_megapixels) {
            state = self.freed.wait(state).unwrap_or_else(|e| e.into_inner());
        }
        state.0 += 1;
        state.1 += megapixels;
        ExportSlot { budget: self, megapixels }
    }
}

impl Drop for ExportSlot<'_> {
    fn drop(&mut self) {
        let mut state = self.budget.state.lock().unwrap_or_else(|e| e.into_inner());
        state.0 -= 1;
        state.1 = (state.1 - self.megapixels).max(0.0);
        self.budget.freed.notify_all();
    }
}

/// The app-wide budget every export draws on: cores minus two renders at once (at least 1, at most 8), 100 MP in flight.
fn export_budget() -> &'static ExportBudget {
    static BUDGET: std::sync::OnceLock<ExportBudget> = std::sync::OnceLock::new();
    BUDGET.get_or_init(|| ExportBudget::new(std::thread::available_parallelism().map_or(1, |n| n.get()).saturating_sub(2).clamp(1, 8), 100.0))
}

/// The full-resolution decode-through-crop pipeline shared by `export_one`
/// below and `print.rs`'s print-ready raster generation -- both need the
/// SAME final-quality, native-resolution rendered image (per ADR-0004: no
/// <100ms latency requirement on this path, so it runs CPU-side, not via
/// the interactive WGSL shader). Kept here, not duplicated in `print.rs`,
/// since this exact op ordering (lens correction -> perspective -> edit
/// stack -> crop) is correctness-sensitive -- see the doc comments on each
/// step below, unchanged from before this was extracted.
///
/// `denoise_dir` is the AI-denoise cache (RFC-0027): where a finished job's result for this photo may be. `None`
/// (Print, until slice 3d teaches its raster cache about it) renders without the `ai_denoise` op.
pub(crate) fn render_full_resolution(source_path: &Path, stack: &EditStack, denoise_dir: Option<&Path>) -> Result<RgbImage, ExportError> {
    let decoded = source_decode::decode_preview(source_path)?;
    let mut image = RgbImage::from_raw(decoded.width, decoded.height, decoded.rgb)
        .ok_or(ExportError::BufferMismatch)?;

    // RFC-0013: a hidden panel's ops are stripped here, once, before any of
    // the four apply_* calls below -- also covers Print (print.rs reuses
    // this same function) -- see effective_stack_for_render's own doc
    // comment.
    let stack = &crate::develop_engine::effective_stack_for_render(stack);

    // AI denoise (M6, RFC-0027 §3.2): mixed into the decoded source before everything else, so lens
    // resampling, grading and a later exposure lift all work on the cleaned pixels. Reads a cached
    // result; never runs the model.
    apply_ai_denoise(&mut image, source_path, stack, denoise_dir);
    // Lens Corrections (M3): runs FIRST, before grading -- the user is
    // grading/cropping the corrected image, not the raw lens-distorted
    // one, matching real Lightroom. See develop_engine.rs's own header
    // comment on this op for why it's a separate resample step, not part
    // of apply_edit_stack's per-pixel loop.
    apply_lens_correction(&mut image, stack);
    // Perspective Correction (M4): same slot as lens correction, right
    // after it -- see develop_engine.rs's own header comment on this op.
    apply_perspective(&mut image, stack);
    apply_edit_stack(&mut image, stack);
    // Crop & Straighten (M3): a pure geometric post-process, deliberately
    // separate from apply_edit_stack -- see develop_engine.rs's own doc
    // comment on `apply_crop`. Crop-then-resize, not resize-then-crop, so
    // the long-edge cap below describes the DELIVERED (post-crop) image.
    apply_crop(&mut image, stack);

    Ok(image)
}

/// What the `ai_denoise` stage did, for the export pre-flight of RFC-0027 §3.6 (slice 3d).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DenoiseStage {
    /// The stack has no `ai_denoise` (or amount 0), or no cache is available to this caller.
    Off,
    Applied,
    /// The op is on but there is no usable cached result: the render goes ahead without it.
    Missing,
}

/// Mixes the cached AI-denoised copy of `source_path` into `image` (the decoded source) by the op's amount.
/// The cache is keyed by the content hash of the source file, the same hash the preview cache uses.
pub(crate) fn apply_ai_denoise(image: &mut RgbImage, source_path: &Path, stack: &EditStack, denoise_dir: Option<&Path>) -> DenoiseStage {
    let amount = ai_denoise_amount(stack);
    let Some(dir) = denoise_dir.filter(|_| amount > 0.0) else { return DenoiseStage::Off };
    let Ok(bytes) = std::fs::read(source_path) else { return DenoiseStage::Missing };
    let hash = blake3::hash(&bytes).to_hex().to_string();
    let Some(entry) = denoise_cache::lookup(dir, &hash, image.dimensions()) else { return DenoiseStage::Missing };
    let Ok(denoised) = image::open(entry) else { return DenoiseStage::Missing };
    blend_denoised(image, &denoised.into_rgb8(), amount);
    DenoiseStage::Applied
}

/// `metadata` is the catalog-resolved EXIF/IPTC source, `None` when the
/// caller has none (or metadata writing wasn't requested). The `Option<String>`
/// in the return is a metadata-embedding warning, not an export failure.
pub fn export_one(
    source_path: &Path,
    stack: &EditStack,
    options: &ExportOptions,
    metadata: Option<&ExportMetadata>,
    denoise_dir: Option<&Path>,
) -> Result<(PathBuf, Option<String>), ExportError> {
    let image = render_full_resolution(source_path, stack, denoise_dir)?;

    let image = match options.long_edge {
        Some(long_edge) if image.width().max(image.height()) > long_edge => {
            let (w, h) = (image.width(), image.height());
            let scale = long_edge as f64 / w.max(h) as f64;
            let target_w = ((w as f64) * scale).round().max(1.0) as u32;
            let target_h = ((h as f64) * scale).round().max(1.0) as u32;
            image::imageops::resize(&image, target_w, target_h, image::imageops::FilterType::Lanczos3)
        }
        _ => image,
    };

    let destination_dir = Path::new(&options.destination_dir);
    // The folder picker only offers existing directories, but a
    // permissions error or a disconnected network share should still
    // surface as a clean ExportResult.error, not a panic.
    std::fs::create_dir_all(destination_dir)?;

    let stem = source_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("export");

    let quality = options.quality.clamp(1, 100);
    let mut bytes = Vec::new();
    JpegEncoder::new_with_quality(&mut bytes, quality).encode_image(&image)?;

    let mut warning = None;
    if let (true, Some(metadata)) = (options.metadata.any(), metadata) {
        match metadata_writer::apply(bytes.clone(), metadata, &options.metadata) {
            Ok(with_metadata) => bytes = with_metadata,
            Err(e) => warning = Some(e.to_string()),
        }
    }
    let out_path = write_unique(destination_dir, stem, "jpg", &bytes)?;

    Ok((out_path, warning))
}

/// One photo to export, resolved by the caller (this module stays catalog-free): where it is, its edit stack, the
/// metadata to embed (if asked for), and its size in megapixels (what a render of it holds in memory).
pub struct ExportJob {
    pub path: PathBuf,
    pub stack: EditStack,
    pub metadata: Option<ExportMetadata>,
    pub megapixels: f64,
}

/// What a photo of unknown size is assumed to be, for the memory budget.
pub const DEFAULT_MEGAPIXELS: f64 = 24.0;

/// Exports the jobs in order. The app's frontend sends one photo per call and runs several calls at once (RFC-0030
/// slice F), so each photo waits here for a share of the app-wide budget (`export_budget`: cores and memory) before
/// it renders; a lone batch call behaves as before.
///
/// `plugin`, if set, is invoked once per successfully exported file (not
/// once per batch) via `export_plugin::invoke` -- fire-and-forget, so a
/// slow or hanging plugin can't stall or fail the rest of the batch
/// (RFC-0006 §3.3). A file that failed to export is never handed to the
/// plugin at all.
pub fn export_batch(
    items: Vec<ExportJob>,
    options: &ExportOptions,
    plugin: Option<&ExportPlugin>,
    denoise_dir: Option<&Path>,
) -> Vec<ExportResult> {
    export_batch_with_budget(items, options, plugin, denoise_dir, export_budget())
}

pub(crate) fn export_batch_with_budget(
    items: Vec<ExportJob>,
    options: &ExportOptions,
    plugin: Option<&ExportPlugin>,
    denoise_dir: Option<&Path>,
    budget: &ExportBudget,
) -> Vec<ExportResult> {
    items
        .into_iter()
        .map(|job| {
            let ExportJob { path, stack, metadata, megapixels } = job;
            let exported = {
                let _share = budget.acquire(megapixels);
                export_one(&path, &stack, options, metadata.as_ref(), denoise_dir)
            };
            match exported {
                Ok((out_path, metadata_warning)) => {
                    let plugin_error = plugin.and_then(|p| export_plugin::invoke(p, &out_path).err()).map(|e| e.to_string());
                    ExportResult {
                        source_path: path.to_string_lossy().to_string(),
                        output_path: Some(out_path.to_string_lossy().to_string()),
                        error: None,
                        plugin_error,
                        metadata_warning,
                    }
                }
                Err(e) => ExportResult {
                    source_path: path.to_string_lossy().to_string(),
                    output_path: None,
                    error: Some(e.to_string()),
                    plugin_error: None,
                    metadata_warning: None,
                },
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("emulsion-export-test-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn exports_claim_their_names_atomically_even_when_they_all_want_the_same_one() {
        let dir = temp_dir("write-unique");
        let names: Vec<PathBuf> = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..8u8).map(|n| {
                let dir = &dir;
                scope.spawn(move || write_unique(dir, "IMG_0001", "jpg", &[n; 4]).unwrap())
            }).collect();
            handles.into_iter().map(|h| h.join().unwrap()).collect()
        });
        let mut distinct = names.clone();
        distinct.sort();
        distinct.dedup();
        assert_eq!(distinct.len(), 8, "eight writers, eight different files");
        // Each file holds exactly one writer's bytes.
        let mut contents: Vec<u8> = names.iter().map(|p| { let b = std::fs::read(p).unwrap(); assert_eq!(b.len(), 4); assert!(b.iter().all(|x| *x == b[0])); b[0] }).collect();
        contents.sort_unstable();
        assert_eq!(contents, (0..8).collect::<Vec<u8>>());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_budget_admits_up_to_its_cores_and_memory_and_always_one_photo() {
        use std::sync::mpsc;
        let budget = std::sync::Arc::new(ExportBudget::new(2, 50.0));
        let a = budget.acquire(20.0);
        let b = budget.acquire(20.0);

        // A third does not fit (two running already): it waits until one is released.
        let (tx, rx) = mpsc::channel();
        let waiter = {
            let budget = budget.clone();
            std::thread::spawn(move || {
                let _c = budget.acquire(5.0);
                tx.send(()).unwrap();
            })
        };
        assert!(rx.recv_timeout(std::time::Duration::from_millis(200)).is_err(), "still waiting");
        drop(a);
        rx.recv_timeout(std::time::Duration::from_secs(5)).expect("admitted once a slot freed");
        waiter.join().unwrap();
        drop(b);

        // Memory counts too: 30 + 30 > 50, so the second waits for the first even with a free core.
        let big = budget.acquire(30.0);
        let (tx, rx) = mpsc::channel();
        let waiter = {
            let budget = budget.clone();
            std::thread::spawn(move || {
                let _x = budget.acquire(30.0);
                tx.send(()).unwrap();
            })
        };
        assert!(rx.recv_timeout(std::time::Duration::from_millis(200)).is_err());
        drop(big);
        rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
        waiter.join().unwrap();

        // A photo bigger than the whole budget still runs, alone.
        let huge = budget.acquire(500.0);
        drop(huge);
    }

    /// RFC-0030 slice F: many export calls at once, all from photos with the same file name, each get a file of
    /// their own and all succeed (this is the shape of the frontend's pool: one photo per call, several calls live).
    #[test]
    fn concurrent_single_photo_exports_do_not_overwrite_each_other() {
        let root = temp_dir("concurrent");
        let dest = root.join("out");
        let budget = ExportBudget::new(3, 100.0);
        let options = ExportOptions { destination_dir: dest.to_string_lossy().to_string(), long_edge: Some(64), quality: 80, plugin_id: None, metadata: MetadataWriteOptions::default() };
        let sources: Vec<PathBuf> = (0..6u8).map(|n| {
            let d = root.join(format!("src{n}"));
            std::fs::create_dir_all(&d).unwrap();
            let p = d.join("IMG_0001.jpg");
            image::RgbImage::from_pixel(200, 120, image::Rgb([n * 40, 90, 30])).save(&p).unwrap();
            p
        }).collect();

        let results: Vec<ExportResult> = std::thread::scope(|scope| {
            let handles: Vec<_> = sources.iter().map(|p| {
                let (options, budget) = (&options, &budget);
                scope.spawn(move || export_batch_with_budget(vec![ExportJob { path: p.clone(), stack: EditStack::empty(), metadata: None, megapixels: 0.02 }], options, None, None, budget).remove(0))
            }).collect();
            handles.into_iter().map(|h| h.join().unwrap()).collect()
        });

        assert!(results.iter().all(|r| r.error.is_none()), "{:?}", results.iter().map(|r| &r.error).collect::<Vec<_>>());
        let mut outputs: Vec<&String> = results.iter().map(|r| r.output_path.as_ref().unwrap()).collect();
        outputs.sort();
        outputs.dedup();
        assert_eq!(outputs.len(), 6, "six different output files");
        assert!(outputs.iter().all(|p| image::open(p).unwrap().width() <= 64));
        let _ = std::fs::remove_dir_all(&root);
    }


    #[test]
    fn unique_output_path_avoids_collisions() {
        let dir = temp_dir("collision");
        std::fs::write(dir.join("photo.jpg"), b"existing").unwrap();

        let first = unique_output_path(&dir, "photo", "jpg");
        assert_eq!(first, dir.join("photo-1.jpg"));

        std::fs::write(&first, b"also existing").unwrap();
        let second = unique_output_path(&dir, "photo", "jpg");
        assert_eq!(second, dir.join("photo-2.jpg"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Real-file-gated, same pattern as raw_decode.rs/preview_cache.rs:
    /// point EMULSION_TEST_RAW_SAMPLE at a real RAW/DNG file to run these.
    #[test]
    fn exports_a_real_jpeg_at_native_resolution() {
        let Ok(sample_path) = std::env::var("EMULSION_TEST_RAW_SAMPLE") else {
            eprintln!("skipping: set EMULSION_TEST_RAW_SAMPLE=/path/to/file.DNG to run this test");
            return;
        };
        let dest = temp_dir("native-res");
        let options = ExportOptions { destination_dir: dest.to_string_lossy().to_string(), long_edge: None, quality: 90, plugin_id: None, metadata: MetadataWriteOptions::default() };

        let out_path = export_one(Path::new(&sample_path), &EditStack::empty(), &options, None, None)
            .expect("export succeeds for a real RAW file")
            .0;

        assert!(out_path.exists());
        let decoded = image::open(&out_path).expect("output is a valid, decodable JPEG");
        assert!(decoded.width() > 0 && decoded.height() > 0);

        let _ = std::fs::remove_dir_all(&dest);
    }

    #[test]
    fn exports_respect_the_long_edge_cap() {
        let Ok(sample_path) = std::env::var("EMULSION_TEST_RAW_SAMPLE") else {
            eprintln!("skipping: set EMULSION_TEST_RAW_SAMPLE=/path/to/file.DNG to run this test");
            return;
        };
        let dest = temp_dir("long-edge");
        let options = ExportOptions { destination_dir: dest.to_string_lossy().to_string(), long_edge: Some(800), quality: 90, plugin_id: None, metadata: MetadataWriteOptions::default() };

        let out_path = export_one(Path::new(&sample_path), &EditStack::empty(), &options, None, None)
            .expect("export succeeds for a real RAW file")
            .0;

        let decoded = image::open(&out_path).expect("output is a valid JPEG");
        assert!(decoded.width().max(decoded.height()) <= 800);

        let _ = std::fs::remove_dir_all(&dest);
    }

    #[test]
    fn a_destination_that_cannot_be_created_returns_a_clean_error_not_a_panic() {
        // A regular file can't be create_dir_all'd into -- forces the
        // error path without needing real filesystem permission tricks.
        let parent = temp_dir("bad-destination");
        let blocking_file = parent.join("not-a-directory");
        std::fs::write(&blocking_file, b"x").unwrap();
        let bad_destination = blocking_file.join("nested");

        let options = ExportOptions {
            destination_dir: bad_destination.to_string_lossy().to_string(),
            long_edge: None,
            quality: 90,
            plugin_id: None,
            metadata: MetadataWriteOptions::default(),
        };

        let result = export_one(Path::new("/nonexistent/not-a-real-raw-file.CR3"), &EditStack::empty(), &options, None, None);
        assert!(result.is_err());

        let _ = std::fs::remove_dir_all(&parent);
    }

    #[test]
    fn export_embeds_only_the_metadata_groups_the_user_chose() {
        let dir = temp_dir("metadata-choice");
        let source = dir.join("src.jpg");
        image::RgbImage::from_pixel(64, 48, image::Rgb([100, 110, 120])).save(&source).unwrap();
        let meta = ExportMetadata {
            camera_make: Some("Emulsion Co".into()),
            latitude: Some(25.0339),
            longitude: Some(121.5645),
            caption: Some("hello".into()),
            ..Default::default()
        };
        let export = |name: &str, opts: MetadataWriteOptions| {
            let options = ExportOptions {
                destination_dir: dir.join(name).to_string_lossy().to_string(),
                long_edge: None,
                quality: 90,
                plugin_id: None,
                metadata: opts,
            };
            let (path, warning) = export_one(&source, &EditStack::empty(), &options, Some(&meta), None).unwrap();
            assert_eq!(warning, None);
            std::fs::read(path).unwrap()
        };

        let bare = export("bare", MetadataWriteOptions::default());
        assert_eq!(crate::metadata::extract_from_jpeg(&bare).camera_make, None);
        assert!(!bare.windows(13).any(|w| w == b"Photoshop 3.0"));

        let exif_no_gps = export("exif", MetadataWriteOptions { exif: true, iptc: false, gps: false });
        let back = crate::metadata::extract_from_jpeg(&exif_no_gps);
        assert_eq!(back.camera_make.as_deref(), Some("Emulsion Co"));
        assert_eq!(back.latitude, None);

        let everything = export("all", MetadataWriteOptions { exif: true, iptc: true, gps: true });
        let back = crate::metadata::extract_from_jpeg(&everything);
        assert!((back.latitude.unwrap() - 25.0339).abs() < 1e-5);
        assert!(everything.windows(13).any(|w| w == b"Photoshop 3.0"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---- AI denoise stage (RFC-0027 §3.2) -------------------------------------------------------

    fn denoise_fixture(name: &str) -> (PathBuf, PathBuf, String) {
        let dir = temp_dir(name);
        let source = dir.join("photo.jpg");
        image::RgbImage::from_pixel(6, 4, image::Rgb([200, 100, 50])).save(&source).unwrap();
        let hash = blake3::hash(&std::fs::read(&source).unwrap()).to_hex().to_string();
        (dir.join("denoise"), source, hash)
    }

    fn denoise_stack(amount: u32) -> EditStack {
        EditStack { schema_version: 1, ops: vec![serde_json::json!({"op": "ai_denoise", "amount": amount})] }
    }

    #[test]
    fn the_stage_is_off_without_the_op_an_amount_or_a_cache() {
        let (dir, source, _) = denoise_fixture("denoise-off");
        let mut img = image::RgbImage::from_pixel(6, 4, image::Rgb([1, 2, 3]));
        assert_eq!(apply_ai_denoise(&mut img, &source, &EditStack::empty(), Some(&dir)), DenoiseStage::Off);
        assert_eq!(apply_ai_denoise(&mut img, &source, &denoise_stack(0), Some(&dir)), DenoiseStage::Off);
        assert_eq!(apply_ai_denoise(&mut img, &source, &denoise_stack(80), None), DenoiseStage::Off);
        assert_eq!(img, image::RgbImage::from_pixel(6, 4, image::Rgb([1, 2, 3])));
    }

    #[test]
    fn with_the_op_on_but_no_usable_cache_entry_the_render_goes_ahead_without_it() {
        let (dir, source, hash) = denoise_fixture("denoise-missing");
        let mut img = image::RgbImage::from_pixel(6, 4, image::Rgb([1, 2, 3]));
        assert_eq!(apply_ai_denoise(&mut img, &source, &denoise_stack(80), Some(&dir)), DenoiseStage::Missing);
        // A wrong-sized entry is not an entry.
        std::fs::create_dir_all(&dir).unwrap();
        image::RgbImage::new(5, 5).save(denoise_cache::entry_path(&dir, &hash, crate::denoise_models::MODEL_ID)).unwrap();
        assert_eq!(apply_ai_denoise(&mut img, &source, &denoise_stack(80), Some(&dir)), DenoiseStage::Missing);
        assert_eq!(apply_ai_denoise(&mut img, Path::new("/nonexistent/photo.jpg"), &denoise_stack(80), Some(&dir)), DenoiseStage::Missing);
        assert_eq!(img, image::RgbImage::from_pixel(6, 4, image::Rgb([1, 2, 3])));
    }

    #[test]
    fn a_cached_result_is_mixed_in_by_the_amount_and_a_hidden_panel_switches_it_off() {
        let (dir, source, hash) = denoise_fixture("denoise-on");
        std::fs::create_dir_all(&dir).unwrap();
        image::RgbImage::from_pixel(6, 4, image::Rgb([100, 200, 250])).save(denoise_cache::entry_path(&dir, &hash, crate::denoise_models::MODEL_ID)).unwrap();
        let base = || image::RgbImage::from_pixel(6, 4, image::Rgb([200, 100, 50]));
        let mut half = base();
        assert_eq!(apply_ai_denoise(&mut half, &source, &denoise_stack(50), Some(&dir)), DenoiseStage::Applied);
        assert_eq!(half.get_pixel(3, 2).0, [150, 150, 150]);
        let mut full = base();
        apply_ai_denoise(&mut full, &source, &denoise_stack(100), Some(&dir));
        assert_eq!(full.get_pixel(0, 0).0, [100, 200, 250]);

        // Through the real render entry: the Noise Reduction panel hidden strips the op (RFC-0013), so it is Off.
        let mut hidden = denoise_stack(100);
        hidden.ops.push(serde_json::json!({"op": "panel_hidden", "panel": "noise_reduction"}));
        let shown = render_full_resolution(&source, &denoise_stack(100), Some(&dir)).unwrap();
        let none = render_full_resolution(&source, &hidden, Some(&dir)).unwrap();
        let plain = render_full_resolution(&source, &EditStack::empty(), Some(&dir)).unwrap();
        assert_eq!(none, plain, "a hidden panel's op must not render");
        assert!(shown.get_pixel(0, 0).0[2] > plain.get_pixel(0, 0).0[2] + 100, "the cached result must show: {:?} vs {:?}", shown.get_pixel(0, 0), plain.get_pixel(0, 0));
    }

    /// Measures batch export through N threads (RFC-0030 slice F): every JPEG/RAW in `EXPORT_PROBE_DIR` is exported
    /// at native resolution with a basic exposure edit, once per thread count in `EXPORT_PROBE_THREADS` (default
    /// "1,2,4,8"). Run under `/usr/bin/time -l` with one thread count to read the peak memory.
    ///   EXPORT_PROBE_DIR=<dir> EXPORT_PROBE_THREADS=4 cargo test --release --lib export_throughput -- --ignored --nocapture
    #[test]
    #[ignore = "measurement, needs EXPORT_PROBE_DIR"]
    fn export_throughput() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let dir = PathBuf::from(std::env::var("EXPORT_PROBE_DIR").expect("EXPORT_PROBE_DIR"));
        let counts: Vec<usize> = std::env::var("EXPORT_PROBE_THREADS").unwrap_or_else(|_| "1,2,4,8".into()).split(',').filter_map(|n| n.trim().parse().ok()).collect();
        let mut files: Vec<PathBuf> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.path()).filter(|p| source_decode::ImageFormat::from_path(p).is_some()).collect();
        files.sort();
        let stack = EditStack { schema_version: 1, ops: vec![serde_json::json!({"op": "exposure", "amount": 30})] };
        println!("\nexport of {} files at native resolution (decode, edit, encode, write)", files.len());
        for threads in counts {
            let dest = temp_dir(&format!("throughput-{threads}"));
            let options = ExportOptions { destination_dir: dest.to_string_lossy().to_string(), long_edge: None, quality: 90, plugin_id: None, metadata: MetadataWriteOptions::default() };
            let next = AtomicUsize::new(0);
            let first = std::sync::Mutex::new(None::<std::time::Duration>);
            let start = std::time::Instant::now();
            std::thread::scope(|scope| {
                for _ in 0..threads {
                    scope.spawn(|| loop {
                        let i = next.fetch_add(1, Ordering::SeqCst);
                        let Some(path) = files.get(i) else { break };
                        export_one(path, &stack, &options, None, None).expect("export");
                        first.lock().unwrap().get_or_insert(start.elapsed());
                    });
                }
            });
            let total = start.elapsed();
            println!("  threads {threads:>2}: {:>7.0} ms total ({:.2} files/s), first file after {:>5.0} ms", total.as_secs_f64() * 1000.0, files.len() as f64 / total.as_secs_f64(), first.lock().unwrap().unwrap().as_secs_f64() * 1000.0);
            let _ = std::fs::remove_dir_all(&dest);
        }
    }
}
