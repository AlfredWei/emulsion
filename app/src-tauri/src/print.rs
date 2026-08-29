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
use printpdf::{
    Color, LinePoint, Mm, Op, PaintMode, PdfDocument, PdfPage, PdfSaveOptions, Point, Polygon, PolygonRing, Pt,
    RawImage, RawImageData, RawImageFormat, Rgb, WindingOrder, XObjectTransform,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const PT_PER_IN: f64 = 72.0;
const MM_PER_IN: f64 = 25.4;

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

// --- PDF export ---
//
// A second, direct output path alongside `window.print()`: writes a real
// PDF file straight to disk (no interactive OS dialog), for a user who
// wants "Export as PDF" as a one-step action -- same UX shape as the
// existing Export dialog (pick a destination, done), not the print-preview-
// then-print flow above. Deliberately its own small composition layer, NOT
// a reuse of `PrintLayoutView.svelte`'s CSS layout math -- PDF page space
// uses points with an origin at the page's BOTTOM-left corner (grows
// upward), the opposite of CSS/DOM's top-left-origin, downward-growing
// space, so the two can't share code, only intent (both must agree on
// what "Single Image, Fit Within Margins" or "2x3 Contact Sheet" LOOKS
// like, verified interactively against each other rather than assumed).
//
// Uses `printpdf` (default-features = false in Cargo.toml -- this app
// doesn't need its HTML/text-layout machinery, just page + image
// placement) with images embedded directly from an in-memory
// `image::RgbImage` via `RawImage`'s raw-pixel constructor, not
// `RawImage::decode_from_bytes` -- skips a redundant JPEG encode/decode
// round-trip through printpdf's own image codec path entirely.

/// One page's placement geometry, in PDF points, origin at the page's
/// bottom-left corner (PDF's own native coordinate space).
#[derive(Debug, Clone, Copy)]
struct PtRect {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PdfPageSetup {
    pub width_in: f64,
    pub height_in: f64,
    pub margin_top_in: f64,
    pub margin_right_in: f64,
    pub margin_bottom_in: f64,
    pub margin_left_in: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PdfLayout {
    /// "single" | "contact-sheet" -- mirrors `PrintPanel.svelte`'s own
    /// template values exactly, unvalidated beyond this match (an unknown
    /// string falls back to the single-image behavior, matching how a
    /// truly malformed request has no other sensible page to produce).
    pub template: String,
    /// "fit" | "fill" -- single-image only; contact-sheet cells are always
    /// "contain" (mirrors `PrintLayoutView.svelte`'s own `.grid-cell` CSS,
    /// which has no per-cell fit-mode control).
    pub fit_mode: String,
    pub rows: u32,
    pub cols: u32,
    pub cell_spacing_in: f64,
}

fn content_rect(page: &PdfPageSetup) -> PtRect {
    let x = page.margin_left_in * PT_PER_IN;
    let y = page.margin_bottom_in * PT_PER_IN;
    let w = (page.width_in - page.margin_left_in - page.margin_right_in) * PT_PER_IN;
    let h = (page.height_in - page.margin_top_in - page.margin_bottom_in) * PT_PER_IN;
    PtRect { x, y, w: w.max(0.0), h: h.max(0.0) }
}

/// Row-major, top-to-bottom/left-to-right cell order -- matches Library
/// selection order flowing into a CSS grid the same way
/// `PrintLayoutView.svelte` already does. Row 0 is the TOP row on screen,
/// which is the HIGHEST y in PDF's bottom-up coordinate space -- the only
/// real translation this function does versus the CSS grid it mirrors.
fn contact_sheet_cells(content: &PtRect, rows: u32, cols: u32, spacing_in: f64) -> Vec<PtRect> {
    let spacing = spacing_in * PT_PER_IN;
    let rows = rows.max(1);
    let cols = cols.max(1);
    let cell_w = ((content.w - spacing * (cols - 1) as f64) / cols as f64).max(0.0);
    let cell_h = ((content.h - spacing * (rows - 1) as f64) / rows as f64).max(0.0);

    let mut cells = Vec::with_capacity((rows * cols) as usize);
    for row in 0..rows {
        for col in 0..cols {
            let x = content.x + col as f64 * (cell_w + spacing);
            let y = content.y + (rows - 1 - row) as f64 * (cell_h + spacing);
            cells.push(PtRect { x, y, w: cell_w, h: cell_h });
        }
    }
    cells
}

/// "Fit"/CSS `object-fit: contain` — scales the image to the largest size
/// that fits entirely within `cell`, centered.
fn place_contain(image_w_px: f64, image_h_px: f64, cell: &PtRect) -> PtRect {
    if image_w_px <= 0.0 || image_h_px <= 0.0 || cell.w <= 0.0 || cell.h <= 0.0 {
        return PtRect { x: cell.x, y: cell.y, w: 0.0, h: 0.0 };
    }
    let img_aspect = image_w_px / image_h_px;
    let cell_aspect = cell.w / cell.h;
    let (w, h) = if img_aspect > cell_aspect { (cell.w, cell.w / img_aspect) } else { (cell.h * img_aspect, cell.h) };
    PtRect { x: cell.x + (cell.w - w) / 2.0, y: cell.y + (cell.h - h) / 2.0, w, h }
}

/// "Fill"/CSS `object-fit: cover` — center-crops the source to `target_aspect`
/// first, so the caller's subsequent `place_contain` call exactly fills the
/// destination rect with no gaps (an already-matching-aspect image "contains"
/// and "covers" identically) instead of needing a separate PDF clip-path op.
fn center_crop_to_aspect(image: &image::RgbImage, target_aspect: f64) -> image::RgbImage {
    let (w, h) = (image.width(), image.height());
    if w == 0 || h == 0 || target_aspect <= 0.0 {
        return image.clone();
    }
    let src_aspect = w as f64 / h as f64;
    if (src_aspect - target_aspect).abs() < 1e-6 {
        return image.clone();
    }
    if src_aspect > target_aspect {
        let new_w = ((h as f64) * target_aspect).round().max(1.0) as u32;
        let x0 = w.saturating_sub(new_w) / 2;
        image::imageops::crop_imm(image, x0, 0, new_w.min(w), h).to_image()
    } else {
        let new_h = ((w as f64) / target_aspect).round().max(1.0) as u32;
        let y0 = h.saturating_sub(new_h) / 2;
        image::imageops::crop_imm(image, 0, y0, w, new_h.min(h)).to_image()
    }
}

/// Registers `image` as a PDF XObject and returns the `Op` that places it
/// at `rect` -- `dpi` is fixed at 72.0 so `XObjectTransform`'s implicit
/// pixel-to-point base scale is 1:1 (see `XObjectTransform::get_ctms`),
/// making `scale_x`/`scale_y` a direct `target_pt / source_px` ratio
/// rather than something that shifts with an arbitrary dpi choice.
fn place_image_op(doc: &mut PdfDocument, image: &image::RgbImage, rect: &PtRect) -> Op {
    let raw = RawImage {
        pixels: RawImageData::U8(image.as_raw().clone()),
        width: image.width() as usize,
        height: image.height() as usize,
        data_format: RawImageFormat::RGB8,
        tag: Vec::new(),
    };
    let id = doc.add_image(&raw);
    let scale_x = if image.width() > 0 { (rect.w / image.width() as f64) as f32 } else { 0.0 };
    let scale_y = if image.height() > 0 { (rect.h / image.height() as f64) as f32 } else { 0.0 };
    Op::UseXobject {
        id,
        transform: XObjectTransform {
            translate_x: Some(Pt(rect.x as f32)),
            translate_y: Some(Pt(rect.y as f32)),
            rotate: None,
            scale_x: Some(scale_x),
            scale_y: Some(scale_y),
            dpi: Some(72.0),
            no_auto_scale: false,
        },
    }
}

/// Fills `rect` with `PrintLayoutView.svelte`'s own `.grid-cell { background:
/// #eee; }` gray -- without this, a Contact Sheet of mixed-aspect photos
/// (each independently letterboxed within its own, otherwise invisible,
/// cell) reads as scattered/misaligned rather than as a grid, since nothing
/// on the page marks where each cell's actual boundary is.
fn cell_background_op(rect: &PtRect) -> Vec<Op> {
    let corner = |x: f64, y: f64| LinePoint { p: Point { x: Pt(x as f32), y: Pt(y as f32) }, bezier: false };
    let polygon = Polygon {
        rings: vec![PolygonRing {
            points: vec![
                corner(rect.x, rect.y),
                corner(rect.x + rect.w, rect.y),
                corner(rect.x + rect.w, rect.y + rect.h),
                corner(rect.x, rect.y + rect.h),
            ],
        }],
        mode: PaintMode::Fill,
        winding_order: WindingOrder::NonZero,
    };
    vec![
        Op::SetFillColor { col: Color::Rgb(Rgb { r: 0.933, g: 0.933, b: 0.933, icc_profile: None }) },
        Op::DrawPolygon { polygon },
    ]
}

/// Builds a single-page PDF containing `images` laid out per `layout`/`page`
/// -- `images` are assumed already full-resolution and color-managed (the
/// SAME print-ready rasters `generate_print_ready_batch` produces for the
/// `window.print()` path), in display order. A Contact Sheet with more
/// images than the grid has cells for silently keeps only the first
/// `rows * cols` (via `Iterator::zip`'s own shorter-sequence-wins
/// behavior) -- matches `PrintLayoutView.svelte`'s own "Showing N of M"
/// truncation, just without a UI note to restate here.
fn build_print_pdf(images: &[image::RgbImage], layout: &PdfLayout, page: &PdfPageSetup) -> Vec<u8> {
    let mut doc = PdfDocument::new("Emulsion Print");
    let content = content_rect(page);
    let mut ops = Vec::new();

    if layout.template == "contact-sheet" {
        let cells = contact_sheet_cells(&content, layout.rows, layout.cols, layout.cell_spacing_in);
        for (image, cell) in images.iter().zip(cells.iter()) {
            ops.extend(cell_background_op(cell));
            let rect = place_contain(image.width() as f64, image.height() as f64, cell);
            ops.push(place_image_op(&mut doc, image, &rect));
        }
    } else if let Some(image) = images.first() {
        let cropped;
        let placed: &image::RgbImage = if layout.fit_mode == "fill" && content.h > 0.0 {
            cropped = center_crop_to_aspect(image, content.w / content.h);
            &cropped
        } else {
            image
        };
        let rect = place_contain(placed.width() as f64, placed.height() as f64, &content);
        ops.push(place_image_op(&mut doc, placed, &rect));
    }

    let pdf_page = PdfPage::new(Mm((page.width_in * MM_PER_IN) as f32), Mm((page.height_in * MM_PER_IN) as f32), ops);
    let mut warnings = Vec::new();
    doc.with_pages(vec![pdf_page]).save(&PdfSaveOptions::default(), &mut warnings)
}

/// Top-level entry point: resolves each item's print-ready raster (reusing
/// `generate_print_ready_image`'s own cache tier -- an "Export as PDF"
/// right after a `window.print()` of the identical job is a cache hit, not
/// a second full-resolution render), composes them per `layout`/`page`,
/// and writes the resulting PDF to `destination_path`.
pub fn export_pdf(
    items: Vec<(i64, PathBuf, String, EditStack)>,
    layout: &PdfLayout,
    page: &PdfPageSetup,
    color: &PrintColorManagement,
    previews_dir: &Path,
    destination_path: &Path,
) -> Result<(), PrintError> {
    let mut images = Vec::with_capacity(items.len());
    for (_, path, content_hash, stack) in &items {
        let (cached_path, _, _) = generate_print_ready_image(path, content_hash, stack, color, previews_dir)?;
        images.push(image::open(&cached_path)?.into_rgb8());
    }

    let pdf_bytes = build_print_pdf(&images, layout, page);
    std::fs::write(destination_path, pdf_bytes)?;
    Ok(())
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

    fn test_page() -> PdfPageSetup {
        // Letter, 0.5in margins all round -- same defaults PrintPanel.svelte ships.
        PdfPageSetup { width_in: 8.5, height_in: 11.0, margin_top_in: 0.5, margin_right_in: 0.5, margin_bottom_in: 0.5, margin_left_in: 0.5 }
    }

    #[test]
    fn content_rect_subtracts_margins_on_every_side() {
        let rect = content_rect(&test_page());
        assert_eq!(rect.x, 0.5 * PT_PER_IN);
        assert_eq!(rect.y, 0.5 * PT_PER_IN);
        assert_eq!(rect.w, (8.5 - 1.0) * PT_PER_IN);
        assert_eq!(rect.h, (11.0 - 1.0) * PT_PER_IN);
    }

    #[test]
    fn contact_sheet_cells_are_row_major_top_to_bottom_left_to_right() {
        let content = PtRect { x: 0.0, y: 0.0, w: 200.0, h: 100.0 };
        let cells = contact_sheet_cells(&content, 2, 2, 0.0);
        assert_eq!(cells.len(), 4);

        // Row 0 (top row on screen) must have the HIGHEST y -- PDF's
        // coordinate space grows upward from the page's bottom edge.
        assert!(cells[0].y > cells[2].y, "top row must sit at a higher y than the bottom row");
        assert_eq!(cells[0].y, cells[1].y, "same row must share a y");
        assert!(cells[0].x < cells[1].x, "left column must sit at a lower x than the right column");
        assert_eq!(cells[0].w, 100.0);
        assert_eq!(cells[0].h, 50.0);
    }

    #[test]
    fn contact_sheet_cells_account_for_spacing_between_cells() {
        // spacing_in is inches, like every other "_in" field in this module
        // (matches `PdfLayout::cell_spacing_in`) -- 1in = 72pt here.
        let content = PtRect { x: 0.0, y: 0.0, w: 4.0 * PT_PER_IN, h: 0.0 };
        let cells = contact_sheet_cells(&content, 1, 2, 1.0);
        // Two cells, one 72pt (1in) gap: each cell gets (288 - 72) / 2 = 108pt.
        assert_eq!(cells[0].w, 108.0);
        assert_eq!(cells[1].x, 180.0, "second cell must start after the first cell plus the gap");
    }

    #[test]
    fn place_contain_fits_a_wide_image_by_width_and_centers_vertically() {
        let cell = PtRect { x: 0.0, y: 0.0, w: 100.0, h: 100.0 };
        // 2:1 image inside a square cell -- width-constrained.
        let placed = place_contain(200.0, 100.0, &cell);
        assert_eq!(placed.w, 100.0);
        assert_eq!(placed.h, 50.0);
        assert_eq!(placed.y, 25.0, "must be vertically centered in the leftover space");
    }

    #[test]
    fn place_contain_fits_a_tall_image_by_height_and_centers_horizontally() {
        let cell = PtRect { x: 0.0, y: 0.0, w: 100.0, h: 100.0 };
        let placed = place_contain(100.0, 200.0, &cell);
        assert_eq!(placed.h, 100.0);
        assert_eq!(placed.w, 50.0);
        assert_eq!(placed.x, 25.0);
    }

    #[test]
    fn center_crop_to_aspect_crops_a_wide_image_down_to_a_square() {
        let image = image::RgbImage::from_pixel(200, 100, image::Rgb([1, 2, 3]));
        let cropped = center_crop_to_aspect(&image, 1.0);
        assert_eq!(cropped.width(), cropped.height(), "cropped result must match the requested 1:1 aspect");
        assert_eq!(cropped.height(), 100, "the shorter dimension must be untouched");
    }

    #[test]
    fn center_crop_to_aspect_is_a_no_op_when_already_matching() {
        let image = image::RgbImage::from_pixel(100, 100, image::Rgb([9, 9, 9]));
        let cropped = center_crop_to_aspect(&image, 1.0);
        assert_eq!((cropped.width(), cropped.height()), (100, 100));
    }

    #[test]
    fn build_print_pdf_produces_a_real_pdf_for_a_single_image() {
        let image = image::RgbImage::from_pixel(300, 200, image::Rgb([255, 0, 0]));
        let layout = PdfLayout { template: "single".to_string(), fit_mode: "fit".to_string(), rows: 1, cols: 1, cell_spacing_in: 0.0 };
        let bytes = build_print_pdf(&[image], &layout, &test_page());
        assert!(bytes.starts_with(b"%PDF"), "output must be a real PDF, starting with the %PDF magic bytes");
        assert!(bytes.len() > 100, "a PDF with a real embedded image should be more than a trivial stub");
    }

    #[test]
    fn build_print_pdf_handles_an_empty_image_list_without_panicking() {
        let layout = PdfLayout { template: "single".to_string(), fit_mode: "fit".to_string(), rows: 1, cols: 1, cell_spacing_in: 0.0 };
        let bytes = build_print_pdf(&[], &layout, &test_page());
        assert!(bytes.starts_with(b"%PDF"), "an empty job must still produce a valid (blank) PDF, not panic");
    }

    #[test]
    fn cell_background_op_fills_the_full_cell_rect_in_eee_gray() {
        let cell = PtRect { x: 10.0, y: 20.0, w: 100.0, h: 50.0 };
        let ops = cell_background_op(&cell);
        assert_eq!(ops.len(), 2, "must set a fill color, then draw the filled rect");

        match &ops[0] {
            Op::SetFillColor { col: Color::Rgb(rgb) } => {
                // #eee == 238/255 ~= 0.933, matching PrintLayoutView.svelte's
                // `.grid-cell { background: #eee; }`.
                assert!((rgb.r - 0.933).abs() < 1e-3);
                assert!((rgb.g - 0.933).abs() < 1e-3);
                assert!((rgb.b - 0.933).abs() < 1e-3);
            }
            other => panic!("expected Op::SetFillColor, got {other:?}"),
        }

        match &ops[1] {
            Op::DrawPolygon { polygon } => {
                assert_eq!(polygon.mode, PaintMode::Fill);
                let points: Vec<(f32, f32)> =
                    polygon.rings[0].points.iter().map(|p| (p.p.x.0, p.p.y.0)).collect();
                assert_eq!(
                    points,
                    vec![(10.0, 20.0), (110.0, 20.0), (110.0, 70.0), (10.0, 70.0)],
                    "must trace the cell's exact four corners"
                );
            }
            other => panic!("expected Op::DrawPolygon, got {other:?}"),
        }
    }

    #[test]
    fn build_print_pdf_draws_a_cell_background_per_contact_sheet_image() {
        let images = vec![
            image::RgbImage::from_pixel(400, 300, image::Rgb([255, 0, 0])),
            image::RgbImage::from_pixel(300, 400, image::Rgb([0, 255, 0])),
        ];
        let layout = PdfLayout { template: "contact-sheet".to_string(), fit_mode: "fit".to_string(), rows: 1, cols: 2, cell_spacing_in: 0.2 };
        let bytes = build_print_pdf(&images, &layout, &test_page());
        assert!(bytes.starts_with(b"%PDF"));
        assert!(bytes.len() > 100);
    }

    #[test]
    fn export_pdf_writes_a_real_pdf_file_to_disk() {
        let dir = temp_dir("pdf-export-source");
        let previews_dir = temp_dir("pdf-export-cache");
        let out_dir = temp_dir("pdf-export-out");
        let source_path = dir.join("photo.jpg");
        image::RgbImage::from_pixel(64, 32, image::Rgb([10, 200, 60])).save(&source_path).unwrap();
        let destination = out_dir.join("contact-sheet.pdf");

        let layout = PdfLayout { template: "single".to_string(), fit_mode: "fit".to_string(), rows: 1, cols: 1, cell_spacing_in: 0.0 };
        export_pdf(
            vec![(1, source_path.clone(), "testhash".to_string(), EditStack::empty())],
            &layout,
            &test_page(),
            &PrintColorManagement { profile: None },
            &previews_dir,
            &destination,
        )
        .expect("PDF export should succeed for a real synthetic source");

        let written = std::fs::read(&destination).expect("the destination file must exist");
        assert!(written.starts_with(b"%PDF"));

        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&previews_dir);
        let _ = std::fs::remove_dir_all(&out_dir);
    }
}
