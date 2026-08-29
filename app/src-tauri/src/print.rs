//! Print module (M4, final scope item) — produces a full-resolution,
//! optionally printer-color-managed raster ready to hand to the OS print
//! dialog (`window.print()`, triggered from the frontend; see
//! `PrintLayoutView.svelte`). This module owns exactly one new concern:
//! generating that raster. Everything else (on-screen layout preview,
//! page setup, the actual print trigger) reuses existing infrastructure --
//! `get_graded_develop_preview`/`get_soft_proof_preview` for the live
//! WYSIWYG layout, and `soft_proof.rs`'s lcms2 ICC transform unchanged for
//! "printer color management."
//!
//! Deliberately its own cache tier, not a reuse of
//! `preview_cache::ensure_graded_preview_for_hash` -- that tier is capped
//! at `DEVELOP_PREVIEW_MAX_DIMENSION` (2048px), fine for a screen preview,
//! wrong for the actual print payload, which needs the source's native
//! resolution (same as `export.rs`'s own final-quality render).

use crate::catalog::EditStack;
use crate::export::{self, ExportError};
use crate::soft_proof::{self, SoftProofError, SoftProofSettings};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// `profile: None` = "Printer Manages Colors" (Lightroom's own default) --
/// no ICC transform, ships the full-resolution graded sRGB buffer as-is,
/// same "no color management" baseline `export.rs`'s JPEG encode already
/// accepts. `Some(...)` = "Managed by Printer Profile" -- reuses
/// `SoftProofSettings` unchanged (a printer/paper `.icc` is just another
/// `TARGET_CUSTOM` profile); `gamut_warning` on the supplied settings is
/// ignored (see `render_print_ready_image`'s doc comment) -- gamut warning
/// is a screen-only soft-proof aid, not meaningful for the actual print
/// buffer.
#[derive(Debug, Clone, Deserialize)]
pub struct PrintColorManagement {
    pub profile: Option<SoftProofSettings>,
}

#[derive(Debug, thiserror::Error)]
pub enum PrintError {
    #[error(transparent)]
    Export(#[from] ExportError),
    #[error(transparent)]
    SoftProof(#[from] SoftProofError),
    #[error(transparent)]
    Image(#[from] image::ImageError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, Serialize)]
pub struct PrintReadyResult {
    pub version_id: i64,
    pub path: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub error: Option<String>,
}

/// A short, stable string identifying the color-management configuration,
/// for this module's own cache-key folding -- mirrors
/// `soft_proof::settings_cache_key`'s own precedent (content-addressed, so
/// a settings change naturally produces a new filename with no explicit
/// invalidation step to get wrong).
fn color_management_cache_key(color: &PrintColorManagement) -> Result<String, PrintError> {
    match &color.profile {
        None => Ok("none".to_string()),
        Some(settings) => Ok(soft_proof::settings_cache_key(settings)?),
    }
}

/// Renders the full-resolution, print-ready image: `export.rs`'s own
/// decode -> lens-correction -> perspective -> edit-stack -> crop pipeline
/// (via the shared `render_full_resolution`, unchanged from Export's own
/// behavior), then -- if a printer profile was chosen -- the same lcms2
/// proofing transform `soft_proof.rs` already uses for on-screen soft
/// proofing, always with `gamut_warning` forced off (not meaningful for a
/// buffer that's about to be printed, not displayed with an alarm color).
fn render_print_ready_image(source_path: &Path, stack: &EditStack, color: &PrintColorManagement) -> Result<image::RgbImage, PrintError> {
    let mut image = export::render_full_resolution(source_path, stack)?;
    if let Some(settings) = &color.profile {
        let settings = SoftProofSettings { gamut_warning: false, ..settings.clone() };
        soft_proof::apply_soft_proof(&mut image, &settings)?;
    }
    Ok(image)
}

/// Content-addressed cache, same discipline as every `preview_cache.rs`
/// tier: `{content_hash}_{stack_hash[..8]}_print_{color_key}.jpg`. Output
/// is a fixed quality-95 JPEG -- Print doesn't need a second
/// user-facing quality control, Export already owns that.
pub fn generate_print_ready_image(
    source_path: &Path,
    content_hash: &str,
    stack: &EditStack,
    color: &PrintColorManagement,
    previews_dir: &Path,
) -> Result<(PathBuf, u32, u32), PrintError> {
    let stack_json = serde_json::to_string(stack).unwrap_or_default();
    let stack_hash = blake3::hash(stack_json.as_bytes()).to_hex().to_string();
    let color_key = color_management_cache_key(color)?;
    let out_path = previews_dir.join(format!("{content_hash}_{}_print_{color_key}.jpg", &stack_hash[..8]));

    if out_path.exists() {
        let (width, height) = image::image_dimensions(&out_path)?;
        return Ok((out_path, width, height));
    }

    let image = render_print_ready_image(source_path, stack, color)?;

    std::fs::create_dir_all(previews_dir)?;
    let mut file = std::fs::File::create(&out_path)?;
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut file, 95);
    encoder.encode_image(&image)?;

    Ok((out_path, image.width(), image.height()))
}

/// Sequential, per-item error isolation -- mirrors `export::export_batch`'s
/// exact convention, so one unreadable/offline source doesn't abort a
/// print job covering several photos (e.g. a Contact Sheet).
pub fn generate_print_ready_batch(
    items: Vec<(i64, PathBuf, String, EditStack)>,
    color: &PrintColorManagement,
    previews_dir: &Path,
) -> Vec<PrintReadyResult> {
    items
        .into_iter()
        .map(|(version_id, path, content_hash, stack)| {
            match generate_print_ready_image(&path, &content_hash, &stack, color, previews_dir) {
                Ok((out_path, width, height)) => PrintReadyResult {
                    version_id,
                    path: Some(out_path.to_string_lossy().to_string()),
                    width: Some(width),
                    height: Some(height),
                    error: None,
                },
                Err(e) => PrintReadyResult { version_id, path: None, width: None, height: None, error: Some(e.to_string()) },
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("emulsion-print-test-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn narrow_test_profile_bytes() -> Vec<u8> {
        // Same deliberately-narrow synthetic profile shape as
        // soft_proof.rs's own tests -- saved to a real .icc file on disk
        // since PrintColorManagement's TARGET_CUSTOM path reads from a
        // path, not an in-memory Profile.
        use lcms2::{CIExyY, CIExyYTRIPLE, ToneCurve};
        let white = CIExyY { x: 0.3127, y: 0.3290, Y: 1.0 };
        let primaries = CIExyYTRIPLE {
            Red: CIExyY { x: 0.40, y: 0.30, Y: 1.0 },
            Green: CIExyY { x: 0.30, y: 0.40, Y: 1.0 },
            Blue: CIExyY { x: 0.28, y: 0.25, Y: 1.0 },
        };
        let curve = ToneCurve::new(2.2);
        let profile = lcms2::Profile::new_rgb(&white, &primaries, &[&curve, &curve, &curve]).unwrap();
        profile.icc().unwrap()
    }

    #[test]
    fn color_management_cache_key_differs_for_none_vs_a_profile() {
        let none_key = color_management_cache_key(&PrintColorManagement { profile: None }).unwrap();
        assert_eq!(none_key, "none");

        let settings = SoftProofSettings {
            target: soft_proof::TARGET_ADOBE_RGB.to_string(),
            custom_profile_path: None,
            intent: "relative".to_string(),
            gamut_warning: false,
        };
        let managed_key = color_management_cache_key(&PrintColorManagement { profile: Some(settings) }).unwrap();
        assert_ne!(none_key, managed_key);
    }

    /// Real-file-gated, same pattern as export.rs/preview_cache.rs: point
    /// EMULSION_TEST_RAW_SAMPLE at a real RAW/DNG file to run these.
    #[test]
    fn print_ready_image_exceeds_the_develop_preview_cap() {
        let Ok(sample_path) = std::env::var("EMULSION_TEST_RAW_SAMPLE") else {
            eprintln!("skipping: set EMULSION_TEST_RAW_SAMPLE=/path/to/file.DNG to run this test");
            return;
        };
        let previews_dir = temp_dir("native-res");

        let (_, width, height) = generate_print_ready_image(
            Path::new(&sample_path),
            "testhash",
            &EditStack::empty(),
            &PrintColorManagement { profile: None },
            &previews_dir,
        )
        .expect("generation succeeds for a real RAW file");

        assert!(
            width.max(height) > crate::preview_cache::DEVELOP_PREVIEW_MAX_DIMENSION,
            "print-ready output ({width}x{height}) must exceed the Develop preview's 2048px cap"
        );

        let _ = std::fs::remove_dir_all(&previews_dir);
    }

    #[test]
    fn printer_manages_colors_is_a_true_pass_through() {
        let dir = temp_dir("pass-through-source");
        let previews_dir = temp_dir("pass-through-cache");
        let source_path = dir.join("photo.jpg");
        image::RgbImage::from_pixel(64, 32, image::Rgb([200, 60, 40])).save(&source_path).unwrap();

        let (out_path, _, _) = generate_print_ready_image(
            &source_path,
            "testhash",
            &EditStack::empty(),
            &PrintColorManagement { profile: None },
            &previews_dir,
        )
        .expect("generation succeeds for a synthetic JPEG source");

        let output = image::open(&out_path).unwrap().into_rgb8();
        let pixel = output.get_pixel(0, 0);
        // JPEG re-encoding introduces small lossy drift -- this pins "no
        // ICC transform ran" (would produce a much larger, deliberate
        // shift), not byte-exact equality.
        let diff: i16 = (pixel[0] as i16 - 200).abs() + (pixel[1] as i16 - 60).abs() + (pixel[2] as i16 - 40).abs();
        assert!(diff <= 10, "expected a near-identity pass-through, got {pixel:?}");

        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&previews_dir);
    }

    #[test]
    fn a_managed_custom_profile_demonstrably_changes_pixels() {
        let dir = temp_dir("managed-source");
        let previews_dir = temp_dir("managed-cache");
        let source_path = dir.join("photo.jpg");
        // Fully saturated red -- well outside the narrow test profile's
        // tiny gamut, same setup as soft_proof.rs's own gamut tests.
        image::RgbImage::from_pixel(64, 32, image::Rgb([255, 0, 0])).save(&source_path).unwrap();

        let profile_path = dir.join("narrow.icc");
        std::fs::write(&profile_path, narrow_test_profile_bytes()).unwrap();

        let color = PrintColorManagement {
            profile: Some(SoftProofSettings {
                target: soft_proof::TARGET_CUSTOM.to_string(),
                custom_profile_path: Some(profile_path.to_string_lossy().to_string()),
                intent: "relative".to_string(),
                gamut_warning: true, // must be ignored/forced off for print
            }),
        };

        let (out_path, _, _) =
            generate_print_ready_image(&source_path, "testhash", &EditStack::empty(), &color, &previews_dir)
                .expect("generation succeeds with a managed custom profile");

        let output = image::open(&out_path).unwrap().into_rgb8();
        let pixel = output.get_pixel(0, 0);
        assert_ne!(pixel.0, [255, 0, 0], "an out-of-gamut color must actually be remapped, not passed through");
        assert_ne!(
            pixel.0,
            [0x80, 0x80, 0x80],
            "gamut_warning must be forced off for print -- the alarm color must never appear in a print buffer"
        );

        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&previews_dir);
    }

    #[test]
    fn cache_key_changes_when_color_management_changes_and_is_stable_on_repeat_calls() {
        let dir = temp_dir("cache-key-source");
        let previews_dir = temp_dir("cache-key-cache");
        let source_path = dir.join("photo.jpg");
        image::RgbImage::from_pixel(64, 32, image::Rgb([120, 90, 60])).save(&source_path).unwrap();

        let stack = EditStack::empty();
        let none = PrintColorManagement { profile: None };
        let managed = PrintColorManagement {
            profile: Some(SoftProofSettings {
                target: soft_proof::TARGET_ADOBE_RGB.to_string(),
                custom_profile_path: None,
                intent: "relative".to_string(),
                gamut_warning: false,
            }),
        };

        let (none_path, _, _) =
            generate_print_ready_image(&source_path, "testhash", &stack, &none, &previews_dir).unwrap();
        let (managed_path, _, _) =
            generate_print_ready_image(&source_path, "testhash", &stack, &managed, &previews_dir).unwrap();
        assert_ne!(none_path, managed_path, "different color management must produce a distinct cache entry");

        let mtime_before = std::fs::metadata(&none_path).unwrap().modified().unwrap();
        let (none_path_again, _, _) =
            generate_print_ready_image(&source_path, "testhash", &stack, &none, &previews_dir).unwrap();
        let mtime_after = std::fs::metadata(&none_path_again).unwrap().modified().unwrap();
        assert_eq!(none_path, none_path_again, "repeat call with identical settings must hit the cache");
        assert_eq!(mtime_before, mtime_after, "cache hit must not rewrite the file");

        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&previews_dir);
    }
}
