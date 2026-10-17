//! The app side of one AI enlargement (M6 slice 4b part 2, RFC-0029 §3.3): decide the output size, render the
//! photo's graded, cropped picture at native resolution, hand it to the `emulsion-ai` helper as a lossless
//! intermediate, and get the JPEG back. Free of Tauri and of the catalog (the commands of slices 4c / 4d wrap
//! it, add the catalog row and the model lookup), and of the helper itself: the round trip is a closure, so
//! the job is tested with a stand-in that checks what would have been sent.
//!
//! The picture the model sees is `export::render_full_resolution` (the same pixels Export and Print use,
//! cached denoise blend included), so "enlarge what I see" means exactly that.

// Wired up by the commands of slices 4c / 4d.
#![allow(dead_code)]

use crate::ai::protocol::{Request, UpscaleResult};
use crate::catalog::EditStack;
use crate::denoise_commands::write_png_fast;
use crate::export::render_full_resolution;
use std::path::{Path, PathBuf};

/// JPEG's limit on either side (RFC-0029 §2).
pub const MAX_SIDE: u32 = 65_535;
/// Above this many output megapixels the dialog warns: several GB, and most editors cannot open the file (§3.2).
pub const VERY_LARGE_MP: f64 = 100.0;

/// What the user asked for in the dialog: 2x, 4x, or a long edge in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Scale(u32),
    LongEdge(u32),
}

/// Output size for a `w x h` picture. 2x and 4x are exact multiples; a long edge keeps the aspect ratio and
/// rounds the short side to the nearest pixel. Never smaller than the source (this is an enlargement), never
/// past what a JPEG can hold.
pub fn output_size(w: u32, h: u32, target: Target) -> Result<(u32, u32), String> {
    if w == 0 || h == 0 {
        return Err("the photo has no pixels".into());
    }
    let (ow, oh) = match target {
        Target::Scale(n @ (2 | 4)) => (w as u64 * n as u64, h as u64 * n as u64),
        Target::Scale(n) => return Err(format!("{n}x is not an offered scale (2x or 4x)")),
        Target::LongEdge(px) => {
            let long = w.max(h);
            if px <= long {
                return Err(format!("a long edge of {px} px is not larger than the photo's {long} px"));
            }
            let scale = px as f64 / long as f64;
            let other = |side: u32| ((side as f64 * scale).round() as u64).max(1);
            if w >= h { (px as u64, other(h)) } else { (other(w), px as u64) }
        }
    };
    if ow > MAX_SIDE as u64 || oh > MAX_SIDE as u64 {
        return Err(format!("{ow}x{oh} is more than a JPEG can hold ({MAX_SIDE} px on a side)"));
    }
    Ok((ow as u32, oh as u32))
}

pub fn megapixels(size: (u32, u32)) -> f64 {
    size.0 as f64 * size.1 as f64 / 1_000_000.0
}

pub fn is_very_large(size: (u32, u32)) -> bool {
    megapixels(size) > VERY_LARGE_MP
}

/// Model runs for a `w x h` picture: what the progress total will be and what a time estimate multiplies.
pub fn tile_count(w: u32, h: u32) -> u32 {
    crate::ai::tiling::tile_count(w as usize, h as usize, crate::ai::upscale_pipeline::TILE, crate::ai::upscale_pipeline::OVERLAP) as u32
}

/// The file stem of the result, next to the original: `<stem>-Enlarged-4x`, `-2x` or `-6000px` for a long edge
/// (RFC-0029 §3.1). The caller makes it unique with `export::unique_output_path`.
pub fn result_stem(source_stem: &str, target: Target) -> String {
    match target {
        Target::Scale(n) => format!("{source_stem}-Enlarged-{n}x"),
        Target::LongEdge(px) => format!("{source_stem}-Enlarged-{px}px"),
    }
}

/// Content hash and size of the written result, streamed: a 150 MP JPEG is far too big to read into memory
/// just to hash it.
pub fn fingerprint(path: &Path) -> std::io::Result<(String, u64)> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = blake3::Hasher::new();
    let size = std::io::copy(&mut file, &mut hasher)?;
    Ok((hasher.finalize().to_hex().to_string(), size))
}

/// Everything one job needs besides the helper.
#[derive(Debug, Clone)]
pub struct Spec<'a> {
    pub source_path: &'a Path,
    pub stack: &'a EditStack,
    /// The AI-denoise cache, so the render includes a finished denoise (RFC-0027); `None` renders without it.
    pub denoise_dir: Option<&'a Path>,
    /// Where the lossless intermediate goes while the job runs (removed afterwards, success or not).
    pub scratch_dir: &'a Path,
    pub model: &'a Path,
    pub out_path: &'a Path,
    pub target: Target,
    /// 1..=100: how much of the model over a plain enlargement.
    pub strength: u8,
    pub jpeg_quality: u8,
}

/// Renders the picture, asks the helper (through `send`) to enlarge it, and cleans up the intermediate.
/// `send` performs the round trip for the request it is given, reporting progress itself; a `cancelled`
/// answer should come back as `Err("cancelled")`-style text from the caller, this only passes errors on.
pub fn run_enlargement(spec: &Spec, id: u64, send: impl FnOnce(Request) -> Result<UpscaleResult, String>) -> Result<UpscaleResult, String> {
    let image = render_full_resolution(spec.source_path, spec.stack, spec.denoise_dir).map_err(|e| e.to_string())?;
    let (out_width, out_height) = output_size(image.width(), image.height(), spec.target)?;
    std::fs::create_dir_all(spec.scratch_dir).map_err(|e| format!("could not create {}: {e}", spec.scratch_dir.display()))?;
    let input: PathBuf = spec.scratch_dir.join(format!("enlarge-input-{id}.png"));
    let result = write_png_fast(&image, &input).and_then(|()| {
        drop(image); // the helper reads it from disk; do not hold a second copy while it runs
        send(Request::UpscaleRun {
            model: spec.model.to_path_buf(),
            image_path: input.clone(),
            out_path: spec.out_path.to_path_buf(),
            out_width,
            out_height,
            strength: spec.strength,
            jpeg_quality: spec.jpeg_quality,
            region: None,
        })
    });
    let _ = std::fs::remove_file(&input);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_and_four_times_are_exact_and_other_scales_are_refused() {
        assert_eq!(output_size(6000, 4000, Target::Scale(2)), Ok((12_000, 8_000)));
        assert_eq!(output_size(6000, 4000, Target::Scale(4)), Ok((24_000, 16_000)));
        assert!(output_size(6000, 4000, Target::Scale(3)).is_err());
        assert!(output_size(0, 10, Target::Scale(2)).is_err());
    }

    #[test]
    fn a_long_edge_keeps_the_aspect_ratio_for_landscape_and_portrait() {
        assert_eq!(output_size(3000, 2000, Target::LongEdge(6000)), Ok((6000, 4000)));
        assert_eq!(output_size(2000, 3000, Target::LongEdge(6000)), Ok((4000, 6000)));
        assert_eq!(output_size(1000, 667, Target::LongEdge(4000)), Ok((4000, 2668)));
        assert_eq!(output_size(1000, 1000, Target::LongEdge(2500)), Ok((2500, 2500)));
        // A very thin strip never rounds its short side to nothing.
        assert_eq!(output_size(5000, 1, Target::LongEdge(5001)).map(|s| s.1), Ok(1));
    }

    #[test]
    fn a_long_edge_that_is_not_an_enlargement_or_does_not_fit_a_jpeg_is_refused() {
        assert!(output_size(3000, 2000, Target::LongEdge(3000)).is_err());
        assert!(output_size(3000, 2000, Target::LongEdge(1000)).is_err());
        assert!(output_size(3000, 2000, Target::LongEdge(65_536)).is_err());
        assert_eq!(output_size(3000, 2000, Target::LongEdge(65_535)).map(|s| s.0), Ok(65_535));
        // 4x of a 20,000 px edge is past the JPEG limit.
        assert!(output_size(20_000, 100, Target::Scale(4)).is_err());
    }

    #[test]
    fn the_result_is_named_after_the_original_and_the_target() {
        assert_eq!(result_stem("IMG_0042", Target::Scale(4)), "IMG_0042-Enlarged-4x");
        assert_eq!(result_stem("IMG_0042", Target::Scale(2)), "IMG_0042-Enlarged-2x");
        assert_eq!(result_stem("IMG_0042", Target::LongEdge(6000)), "IMG_0042-Enlarged-6000px");
    }

    #[test]
    fn the_fingerprint_is_the_same_hash_import_uses_and_the_size() {
        let d = std::env::temp_dir().join(format!("emulsion-fingerprint-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let p = d.join("f.bin");
        let bytes: Vec<u8> = (0..300_000u32).map(|i| (i * 31 % 251) as u8).collect();
        std::fs::write(&p, &bytes).unwrap();
        assert_eq!(fingerprint(&p).unwrap(), (blake3::hash(&bytes).to_hex().to_string(), 300_000));
        assert!(fingerprint(&d.join("missing")).is_err());
    }

    #[test]
    fn very_large_starts_above_a_hundred_megapixels_and_tiles_follow_the_helpers_grid() {
        assert!(!is_very_large((10_000, 10_000)));
        assert!(is_very_large((10_001, 10_000)));
        assert_eq!(megapixels((4000, 3000)), 12.0);
        // 256 px windows every 224 px: 6000 -> 27, 4000 -> 18.
        assert_eq!(tile_count(6000, 4000), 27 * 18);
        assert_eq!(tile_count(100, 100), 1);
    }

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("emulsion-upscale-job-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn jpeg_source(d: &Path, w: u32, h: u32) -> PathBuf {
        let path = d.join("src.jpg");
        image::RgbImage::from_fn(w, h, |x, y| image::Rgb([(x * 255 / w) as u8, (y * 255 / h) as u8, 90])).save(&path).unwrap();
        path
    }

    fn spec<'a>(source: &'a Path, stack: &'a EditStack, scratch: &'a Path, out: &'a Path, target: Target) -> Spec<'a> {
        Spec { source_path: source, stack, denoise_dir: None, scratch_dir: scratch, model: Path::new("m.onnx"), out_path: out, target, strength: 45, jpeg_quality: 95 }
    }

    #[test]
    fn the_helper_is_asked_for_the_rendered_picture_at_the_planned_size_and_the_intermediate_is_removed() {
        let d = dir("ok");
        let source = jpeg_source(&d, 120, 80);
        let stack = EditStack::empty();
        let scratch = d.join("scratch");
        let out = d.join("big.jpg");
        let mut seen = None;
        let r = run_enlargement(&spec(&source, &stack, &scratch, &out, Target::Scale(4)), 7, |req| {
            let Request::UpscaleRun { image_path, out_path, out_width, out_height, strength, jpeg_quality, region, model } = req else { panic!("wrong request") };
            // The intermediate is a lossless picture of the full render, present while the helper works.
            let img = image::open(&image_path).unwrap().to_rgb8();
            seen = Some((image_path.clone(), img.dimensions()));
            assert_eq!((out_path.as_path(), out_width, out_height, strength, jpeg_quality, region, model.as_path()), (out.as_path(), 480, 320, 45, 95, None, Path::new("m.onnx")));
            Ok(UpscaleResult { width: out_width, height: out_height, tiles: 1, ms: 5 })
        })
        .unwrap();
        assert_eq!((r.width, r.height), (480, 320));
        let (input, dims) = seen.unwrap();
        assert_eq!(dims, (120, 80));
        assert!(!input.exists(), "the intermediate must not outlive the job");
    }

    #[test]
    fn the_render_includes_the_crop_so_the_enlargement_is_of_what_the_user_sees() {
        let d = dir("crop");
        let source = jpeg_source(&d, 200, 100);
        let mut stack = EditStack::empty();
        stack.ops.push(serde_json::json!({"op": "crop", "x": 0.0, "y": 0.0, "width": 0.5, "height": 1.0, "angle": 0.0}));
        let scratch = d.join("scratch");
        let out = d.join("c.jpg");
        let r = run_enlargement(&spec(&source, &stack, &scratch, &out, Target::Scale(2)), 1, |req| {
            let Request::UpscaleRun { image_path, out_width, out_height, .. } = req else { panic!() };
            let (w, h) = image::image_dimensions(&image_path).unwrap();
            assert_eq!((w, h), (100, 100), "the helper must be handed the cropped picture");
            assert_eq!((out_width, out_height), (200, 200));
            Ok(UpscaleResult { width: out_width, height: out_height, tiles: 1, ms: 1 })
        });
        assert!(r.is_ok(), "{r:?}");
    }

    #[test]
    fn a_failed_job_passes_the_error_on_and_still_removes_the_intermediate() {
        let d = dir("fail");
        let source = jpeg_source(&d, 64, 64);
        let stack = EditStack::empty();
        let scratch = d.join("scratch");
        let r = run_enlargement(&spec(&source, &stack, &scratch, &d.join("x.jpg"), Target::Scale(2)), 3, |_| Err("upscale cancelled".to_string()));
        assert_eq!(r.unwrap_err(), "upscale cancelled");
        assert!(std::fs::read_dir(&scratch).unwrap().next().is_none(), "no stray intermediate");
    }

    #[test]
    fn a_size_that_cannot_be_planned_never_writes_or_sends_anything() {
        let d = dir("size");
        let source = jpeg_source(&d, 64, 64);
        let stack = EditStack::empty();
        let scratch = d.join("scratch");
        let r = run_enlargement(&spec(&source, &stack, &scratch, &d.join("x.jpg"), Target::LongEdge(10)), 4, |_| panic!("must not send"));
        assert!(r.is_err());
        assert!(!scratch.exists() || std::fs::read_dir(&scratch).unwrap().next().is_none());
    }

    #[test]
    fn a_missing_source_is_an_error_not_a_panic() {
        let d = dir("missing");
        let stack = EditStack::empty();
        let r = run_enlargement(&spec(&d.join("none.jpg"), &stack, &d.join("s"), &d.join("x.jpg"), Target::Scale(2)), 5, |_| panic!("must not send"));
        assert!(r.is_err());
    }

    /// RFC-0029 §4 / slice 4c: what the rest of the app does with a very large JPEG (the enlargement is one).
    /// One step per run, so `/usr/bin/time -l` gives that step's own peak memory:
    /// `LARGE_JPEG=/path/150mp.jpg LARGE_STEP=decode cargo test --release --lib -- --ignored --nocapture large_jpeg_probe`
    /// with `LARGE_STEP` one of `decode`, `thumbnail`, `develop_preview`, `full_preview`, `render`, `export`.
    #[test]
    #[ignore]
    fn large_jpeg_probe() {
        let path = PathBuf::from(std::env::var("LARGE_JPEG").expect("LARGE_JPEG"));
        let step = std::env::var("LARGE_STEP").expect("LARGE_STEP");
        let out = dir("large-probe");
        let started = std::time::Instant::now();
        let note = match step.as_str() {
            "decode" => {
                let d = crate::source_decode::decode_preview(&path).unwrap();
                format!("{}x{}", d.width, d.height)
            }
            "thumbnail" => {
                // What `import::generate_thumbnail_file` does: full decode, then a Triangle downscale to the grid size.
                let d = crate::source_decode::decode_preview(&path).unwrap();
                let src = image::RgbImage::from_raw(d.width, d.height, d.rgb).unwrap();
                let scale = 400.0 / src.width().max(src.height()) as f64;
                let t = image::imageops::resize(&src, (src.width() as f64 * scale) as u32, (src.height() as f64 * scale) as u32, image::imageops::FilterType::Triangle);
                format!("{}x{}", t.width(), t.height())
            }
            "develop_preview" => {
                let i = crate::preview_cache::ensure_develop_preview_for_hash(&path, "probe-hash", &out).unwrap();
                format!("{}x{}", i.width, i.height)
            }
            "full_preview" => {
                let i = crate::preview_cache::ensure_develop_full_preview_for_hash(&path, "probe-hash", &out).unwrap();
                format!("{}x{}", i.width, i.height)
            }
            "render" | "export" => {
                let stack = EditStack {
                    schema_version: 1,
                    ops: vec![serde_json::json!({"op": "exposure", "value": 0.3}), serde_json::json!({"op": "contrast", "value": 10}), serde_json::json!({"op": "sharpen", "amount": 40, "radius": 1.0})],
                };
                if step == "render" {
                    let img = render_full_resolution(&path, &stack, None).unwrap();
                    format!("{}x{}", img.width(), img.height())
                } else {
                    let opts: crate::export::ExportOptions = serde_json::from_value(serde_json::json!({"destination_dir": out.to_string_lossy(), "long_edge": null, "quality": 90, "plugin_id": null})).unwrap();
                    let (p, _) = crate::export::export_one(&path, &stack, &opts, None, None).unwrap();
                    format!("{} bytes", std::fs::metadata(p).unwrap().len())
                }
            }
            other => panic!("unknown LARGE_STEP {other}"),
        };
        eprintln!("LARGE_PROBE {step}: {} ms ({note})", started.elapsed().as_millis());
    }
}
