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
use crate::develop_engine::{apply_crop, apply_edit_stack, apply_lens_correction, apply_perspective};
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

/// The full-resolution decode-through-crop pipeline shared by `export_one`
/// below and `print.rs`'s print-ready raster generation -- both need the
/// SAME final-quality, native-resolution rendered image (per ADR-0004: no
/// <100ms latency requirement on this path, so it runs CPU-side, not via
/// the interactive WGSL shader). Kept here, not duplicated in `print.rs`,
/// since this exact op ordering (lens correction -> perspective -> edit
/// stack -> crop) is correctness-sensitive -- see the doc comments on each
/// step below, unchanged from before this was extracted.
pub(crate) fn render_full_resolution(source_path: &Path, stack: &EditStack) -> Result<RgbImage, ExportError> {
    let decoded = source_decode::decode_preview(source_path)?;
    let mut image = RgbImage::from_raw(decoded.width, decoded.height, decoded.rgb)
        .ok_or(ExportError::BufferMismatch)?;

    // RFC-0013: a hidden panel's ops are stripped here, once, before any of
    // the four apply_* calls below -- also covers Print (print.rs reuses
    // this same function) -- see effective_stack_for_render's own doc
    // comment.
    let stack = &crate::develop_engine::effective_stack_for_render(stack);

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

/// `metadata` is the catalog-resolved EXIF/IPTC source, `None` when the
/// caller has none (or metadata writing wasn't requested). The `Option<String>`
/// in the return is a metadata-embedding warning, not an export failure.
pub fn export_one(
    source_path: &Path,
    stack: &EditStack,
    options: &ExportOptions,
    metadata: Option<&ExportMetadata>,
) -> Result<(PathBuf, Option<String>), ExportError> {
    let image = render_full_resolution(source_path, stack)?;

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
    let out_path = unique_output_path(destination_dir, stem, "jpg");

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
    std::fs::write(&out_path, &bytes)?;

    Ok((out_path, warning))
}

/// Sequential, accepting a `Vec` from the start so batch export is a
/// frontend-only follow-up (multi-select UI) later, not a backend
/// rewrite, even though the frontend only ever sends one item this slice.
///
/// `plugin`, if set, is invoked once per successfully exported file (not
/// once per batch) via `export_plugin::invoke` -- fire-and-forget, so a
/// slow or hanging plugin can't stall or fail the rest of the batch
/// (RFC-0006 §3.3). A file that failed to export is never handed to the
/// plugin at all.
pub fn export_batch(
    items: Vec<(PathBuf, EditStack, Option<ExportMetadata>)>,
    options: &ExportOptions,
    plugin: Option<&ExportPlugin>,
) -> Vec<ExportResult> {
    items
        .into_iter()
        .map(|(path, stack, metadata)| match export_one(&path, &stack, options, metadata.as_ref()) {
            Ok((out_path, metadata_warning)) => {
                let plugin_error = plugin
                    .and_then(|p| export_plugin::invoke(p, &out_path).err())
                    .map(|e| e.to_string());
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

        let out_path = export_one(Path::new(&sample_path), &EditStack::empty(), &options, None)
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

        let out_path = export_one(Path::new(&sample_path), &EditStack::empty(), &options, None)
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

        let result = export_one(Path::new("/nonexistent/not-a-real-raw-file.CR3"), &EditStack::empty(), &options, None);
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
            let (path, warning) = export_one(&source, &EditStack::empty(), &options, Some(&meta)).unwrap();
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
}
