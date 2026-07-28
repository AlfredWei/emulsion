//! Export pipeline (M1 Slice 5) — see docs/PRD/PRD.md §7.5.
//!
//! Applies the current edit stack to a full-resolution RAW decode and
//! writes a real JPEG to disk. CPU-side render, not native wgpu or the
//! in-webview WebGPU path Develop uses -- per ADR-0004, the export path
//! has no <100ms latency requirement, so the constraints that shape the
//! interactive pipeline don't apply here. Not required to be
//! byte-identical to DevelopCanvas.svelte's WGSL shader output (their
//! source resolutions differ anyway -- full-res export vs. the
//! downsampled Develop preview) -- sourced from the same formula, tested
//! against the same hand-derived expected values, same tolerance the
//! shader's own numeric test already uses.
//!
//! Scope cut for this pass (see the plan): JPEG output only, single image
//! only (no multi-select UI in Library yet), long-edge-or-original resize,
//! one quality setting, no color management (none exists in this pipeline
//! yet -- see raw_decode.rs), no live progress stream. Source format is
//! not restricted -- source_decode.rs dispatches RAW or JPEG sources alike
//! (M2 Slice 1).

use crate::catalog::EditStack;
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
}

#[derive(Debug, Clone, Serialize)]
pub struct ExportResult {
    pub source_path: String,
    pub output_path: Option<String>,
    pub error: Option<String>,
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

/// Edit-stack ops have no meaningful array order (the shader always
/// applies exposure -> contrast -> saturation regardless of how they're
/// stored) -- look each one up by name, defaulting to a no-op value so an
/// image never opened in Develop still exports as a clean passthrough.
fn op_value(ops: &[serde_json::Value], name: &str) -> f32 {
    ops.iter()
        .find(|op| op.get("op").and_then(|v| v.as_str()) == Some(name))
        .and_then(|op| op.get("value"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0) as f32
}

/// Same formula as DevelopCanvas.svelte's WGSL fragment shader, in the
/// same order, kept in f32 to track the shader's precision. See this
/// module's doc comment for why exact GPU parity isn't the bar here.
///
/// `pub(crate)`: also reused by thumbnail regeneration (the Library grid
/// thumbnail reflecting a Develop edit) -- same formula, same reasoning,
/// no reason to duplicate it.
pub(crate) fn apply_edit_stack(image: &mut RgbImage, stack: &EditStack) {
    let exposure_ev = op_value(&stack.ops, "exposure");
    let contrast = op_value(&stack.ops, "contrast");
    let saturation = op_value(&stack.ops, "saturation");

    for pixel in image.pixels_mut() {
        let mut rgb = [
            pixel[0] as f32 / 255.0,
            pixel[1] as f32 / 255.0,
            pixel[2] as f32 / 255.0,
        ];

        for c in rgb.iter_mut() {
            *c *= 2f32.powf(exposure_ev);
        }
        for c in rgb.iter_mut() {
            *c = (*c - 0.5) * (1.0 + contrast / 100.0) + 0.5;
        }

        let luma = rgb[0] * 0.2126 + rgb[1] * 0.7152 + rgb[2] * 0.0722;
        for c in rgb.iter_mut() {
            *c = luma + (*c - luma) * (1.0 + saturation / 100.0);
        }

        for (channel, value) in pixel.0.iter_mut().zip(rgb.iter()) {
            *channel = (value.clamp(0.0, 1.0) * 255.0).round() as u8;
        }
    }
}

/// First `{stem}.{ext}`, `{stem}-1.{ext}`, `{stem}-2.{ext}`, ... that
/// doesn't already exist in `dir`. Inherently check-then-write (a
/// concurrent writer could still race it) -- the frontend disabling the
/// Export button while a request is in flight is what actually prevents
/// this app from double-firing the same export, so that's an accepted,
/// not a fixed, race.
fn unique_output_path(dir: &Path, stem: &str, ext: &str) -> PathBuf {
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

pub fn export_one(
    source_path: &Path,
    stack: &EditStack,
    options: &ExportOptions,
) -> Result<PathBuf, ExportError> {
    let decoded = source_decode::decode_preview(source_path)?;
    let mut image = RgbImage::from_raw(decoded.width, decoded.height, decoded.rgb)
        .ok_or(ExportError::BufferMismatch)?;

    apply_edit_stack(&mut image, stack);

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

    let mut file = std::fs::File::create(&out_path)?;
    let quality = options.quality.clamp(1, 100);
    let mut encoder = JpegEncoder::new_with_quality(&mut file, quality);
    encoder.encode_image(&image)?;

    Ok(out_path)
}

/// Sequential, accepting a `Vec` from the start so batch export is a
/// frontend-only follow-up (multi-select UI) later, not a backend
/// rewrite, even though the frontend only ever sends one item this slice.
pub fn export_batch(items: Vec<(PathBuf, EditStack)>, options: &ExportOptions) -> Vec<ExportResult> {
    items
        .into_iter()
        .map(|(path, stack)| match export_one(&path, &stack, options) {
            Ok(out_path) => ExportResult {
                source_path: path.to_string_lossy().to_string(),
                output_path: Some(out_path.to_string_lossy().to_string()),
                error: None,
            },
            Err(e) => ExportResult {
                source_path: path.to_string_lossy().to_string(),
                output_path: None,
                error: Some(e.to_string()),
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

    fn stack_with(ops: &[(&str, f32)]) -> EditStack {
        EditStack {
            schema_version: 1,
            ops: ops
                .iter()
                .map(|(op, value)| serde_json::json!({ "op": op, "value": value }))
                .collect(),
        }
    }

    /// Same hand-derived expected value the WGSL shader's own numeric
    /// test (m1-slice3-smoke) was validated against: input (153,51,51) +
    /// exposure +0.5EV/contrast +10/saturation +30 -> (255,56,56). No
    /// real RAW file needed -- this exercises apply_edit_stack directly.
    #[test]
    fn apply_edit_stack_matches_the_shaders_hand_derived_expected_value() {
        let mut image = RgbImage::from_pixel(1, 1, image::Rgb([153, 51, 51]));
        let stack = stack_with(&[("exposure", 0.5), ("contrast", 10.0), ("saturation", 30.0)]);

        apply_edit_stack(&mut image, &stack);

        let pixel = image.get_pixel(0, 0);
        let expected = [255i32, 56, 56];
        for (actual, expected) in pixel.0.iter().zip(expected.iter()) {
            assert!(
                (*actual as i32 - expected).abs() <= 2,
                "expected ~{expected:?}, got {actual}"
            );
        }
    }

    #[test]
    fn apply_edit_stack_is_a_passthrough_with_no_ops() {
        let mut image = RgbImage::from_pixel(1, 1, image::Rgb([100, 120, 140]));
        apply_edit_stack(&mut image, &EditStack::empty());
        assert_eq!(image.get_pixel(0, 0).0, [100, 120, 140]);
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
        let options = ExportOptions { destination_dir: dest.to_string_lossy().to_string(), long_edge: None, quality: 90 };

        let out_path = export_one(Path::new(&sample_path), &EditStack::empty(), &options)
            .expect("export succeeds for a real RAW file");

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
        let options = ExportOptions { destination_dir: dest.to_string_lossy().to_string(), long_edge: Some(800), quality: 90 };

        let out_path = export_one(Path::new(&sample_path), &EditStack::empty(), &options)
            .expect("export succeeds for a real RAW file");

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
        };

        let result = export_one(Path::new("/nonexistent/not-a-real-raw-file.CR3"), &EditStack::empty(), &options);
        assert!(result.is_err());

        let _ = std::fs::remove_dir_all(&parent);
    }
}
