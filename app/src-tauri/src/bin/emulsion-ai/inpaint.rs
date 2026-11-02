//! Object removal, run inside the `emulsion-ai` helper process (RFC-0032, ADR-0010): LaMa (fp32 ONNX, 512 x 512 window)
//! on the ONNX Runtime CPU provider (CoreML was far slower in the bake-off, RFC-0032 §9.2). The window, the network's
//! inputs and the patch are the pure functions of `ai/inpaint_pipeline.rs`; this is the model call and the file I/O
//! around them. The model path has an `#[ignore]`d test that needs the real file (`INPAINT_MODEL`).

use crate::inpaint_pipeline::{compose_patch, is_reduced, mask_bbox, plan_window, to_net_input, FEATHER_PX, NET};
use crate::protocol::InpaintResult;
use ort::session::{builder::GraphOptimizationLevel, Session};
use ort::value::TensorRef;
use std::path::Path;
use std::sync::Mutex;
use std::time::Instant;

#[derive(Debug, thiserror::Error)]
pub enum InpaintError {
    #[error("inference failed: {0}")]
    Ort(String),
    #[error("unexpected model output: {0}")]
    BadOutput(String),
    #[error("image error: {0}")]
    Image(String),
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error("removal cancelled")]
    Cancelled,
    #[error("inpainter state is poisoned")]
    Poisoned,
}

impl<T> From<ort::Error<T>> for InpaintError {
    fn from(e: ort::Error<T>) -> Self {
        InpaintError::Ort(e.to_string())
    }
}

/// Writes `img` as a PNG through a `.part` file and a rename, so a reader never sees half a patch.
fn write_rgba_png_atomic(img: &image::RgbaImage, path: &Path) -> Result<(), InpaintError> {
    use image::codecs::png::{CompressionType, FilterType, PngEncoder};
    use image::ImageEncoder;
    let tmp = path.with_extension("png.part");
    let file = std::fs::File::create(&tmp).map_err(|e| InpaintError::Image(format!("{}: {e}", tmp.display())))?;
    let mut writer = std::io::BufWriter::new(file);
    PngEncoder::new_with_quality(&mut writer, CompressionType::Default, FilterType::Adaptive)
        .write_image(img.as_raw(), img.width(), img.height(), image::ExtendedColorType::Rgba8)
        .map_err(|e| InpaintError::Image(e.to_string()))?;
    std::io::Write::flush(&mut writer).map_err(|e| InpaintError::Image(e.to_string()))?;
    drop(writer);
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        InpaintError::Image(format!("{}: {e}", path.display()))
    })
}

struct Loaded {
    model: std::path::PathBuf,
    session: Session,
}

/// The lazy session, created on first use and dropped on `release()`.
#[derive(Default)]
pub struct Inpainter {
    loaded: Mutex<Option<Loaded>>,
}

impl Inpainter {
    /// Fills the mask of `mask_path` in `image_path` and writes the patch to `out_path`. Blocking and CPU-heavy (a few
    /// seconds); `progress(done, total)` is called between its four steps and returning `false` stops the run.
    pub fn run(&self, model: &Path, image_path: &Path, mask_path: &Path, out_path: &Path, progress: &mut dyn FnMut(usize, usize) -> bool) -> Result<InpaintResult, InpaintError> {
        const STEPS: usize = 4;
        let started = Instant::now();
        let img = image::open(image_path).map_err(|e| InpaintError::Image(format!("could not read {}: {e}", image_path.display())))?.to_rgb8();
        let mask = image::open(mask_path).map_err(|e| InpaintError::Image(format!("could not read {}: {e}", mask_path.display())))?.to_luma8();
        if mask.dimensions() != img.dimensions() {
            return Err(InpaintError::BadRequest(format!("the mask is {}x{} but the image is {}x{}", mask.width(), mask.height(), img.width(), img.height())));
        }
        let bbox = mask_bbox(&mask).ok_or_else(|| InpaintError::BadRequest("the mask is empty".into()))?;
        let window = plan_window(bbox, img.width(), img.height());
        let crop = image::imageops::crop_imm(&img, window.x, window.y, window.w, window.h).to_image();
        let crop_mask = image::imageops::crop_imm(&mask, window.x, window.y, window.w, window.h).to_image();
        if !progress(1, STEPS) {
            return Err(InpaintError::Cancelled);
        }

        let (input, mask_input) = to_net_input(&crop, &crop_mask);
        let answer = {
            let mut guard = self.loaded.lock().map_err(|_| InpaintError::Poisoned)?;
            if guard.as_ref().is_none_or(|l| l.model != model) {
                // Same session settings as the other models (RFC-0026 §7): memory-pattern planning off.
                let session = Session::builder()?.with_optimization_level(GraphOptimizationLevel::Level3)?.with_memory_pattern(false)?.commit_from_file(model)?;
                *guard = Some(Loaded { model: model.to_path_buf(), session });
            }
            let loaded = guard.as_mut().expect("just filled");
            let side = NET as usize;
            let outputs = loaded.session.run(ort::inputs![
                "image" => TensorRef::from_array_view(([1usize, 3, side, side], &input[..]))?,
                "mask" => TensorRef::from_array_view(([1usize, 1, side, side], &mask_input[..]))?,
            ])?;
            let (shape, data) = outputs[0].try_extract_tensor::<f32>()?;
            if data.len() != 3 * side * side {
                return Err(InpaintError::BadOutput(format!("the output has shape {shape:?}, expected 1x3x{side}x{side}")));
            }
            data.to_vec()
        };
        if !progress(2, STEPS) {
            return Err(InpaintError::Cancelled);
        }

        let (patch, wx, wy) = compose_patch(&crop, &crop_mask, &answer, FEATHER_PX);
        if !progress(3, STEPS) {
            return Err(InpaintError::Cancelled);
        }
        write_rgba_png_atomic(&patch, out_path)?;
        progress(4, STEPS);
        Ok(InpaintResult {
            x: window.x + wx,
            y: window.y + wy,
            width: patch.width(),
            height: patch.height(),
            window,
            reduced: is_reduced(window),
            ms: started.elapsed().as_millis() as u64,
        })
    }

    pub fn release(&self) {
        if let Ok(mut g) = self.loaded.lock() {
            *g = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mask_of_another_size_and_an_empty_mask_are_refused_before_any_model_is_loaded() {
        let dir = std::env::temp_dir().join(format!("emulsion-inpaint-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        image::RgbImage::new(64, 48).save(dir.join("i.png")).unwrap();
        image::GrayImage::new(32, 48).save(dir.join("wrong.png")).unwrap();
        image::GrayImage::new(64, 48).save(dir.join("empty.png")).unwrap();
        let none = |_: usize, _: usize| true;
        let run = |mask: &str| Inpainter::default().run(Path::new("missing.onnx"), &dir.join("i.png"), &dir.join(mask), &dir.join("o.png"), &mut { none });
        assert!(matches!(run("wrong.png"), Err(InpaintError::BadRequest(m)) if m.contains("32x48")));
        assert!(matches!(run("empty.png"), Err(InpaintError::BadRequest(m)) if m.contains("empty")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Needs the real file: `INPAINT_MODEL=<lama_fp32.onnx> INPAINT_IMAGE=<photo> cargo test --release --bin emulsion-ai -- --ignored inpaint_model --nocapture`.
    /// Fills a disc in the middle of the photograph and checks that only the disc (and its fade) changed.
    #[test]
    #[ignore]
    fn inpaint_model_fills_a_hole_and_leaves_the_rest_of_the_photograph_alone() {
        let model = std::path::PathBuf::from(std::env::var("INPAINT_MODEL").expect("INPAINT_MODEL"));
        let photo = image::open(std::env::var("INPAINT_IMAGE").expect("INPAINT_IMAGE")).unwrap().to_rgb8();
        let (w, h) = photo.dimensions();
        let dir = std::env::temp_dir().join(format!("emulsion-inpaint-real-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        photo.save(dir.join("i.png")).unwrap();
        let r = (w.min(h) / 10) as i32;
        let (cx, cy) = ((w / 2) as i32, (h / 2) as i32);
        let mask = image::GrayImage::from_fn(w, h, |x, y| image::Luma([if (x as i32 - cx).pow(2) + (y as i32 - cy).pow(2) <= r * r { 255 } else { 0 }]));
        mask.save(dir.join("m.png")).unwrap();
        let mut steps = Vec::new();
        let result = Inpainter::default()
            .run(&model, &dir.join("i.png"), &dir.join("m.png"), &dir.join("o.png"), &mut |d, t| {
                steps.push((d, t));
                true
            })
            .unwrap();
        eprintln!("{result:?}");
        assert_eq!(steps.last(), Some(&(4, 4)));
        let patch = image::open(dir.join("o.png")).unwrap().to_rgba8();
        assert_eq!((patch.width(), patch.height()), (result.width, result.height));
        let mut out = photo.clone();
        crate::inpaint_pipeline::apply_patch(&mut out, &patch, result.x, result.y);
        let (mut changed_outside, mut changed_inside) = (0u32, 0u32);
        for (x, y, p) in out.enumerate_pixels() {
            let inside = (x as i32 - cx).pow(2) + (y as i32 - cy).pow(2) <= r * r;
            let differs = p.0 != photo.get_pixel(x, y).0;
            if inside && differs {
                changed_inside += 1;
            }
            let beyond_fade = (x as i32 - cx).pow(2) + (y as i32 - cy).pow(2) > (r + 4 * FEATHER_PX as i32).pow(2);
            if beyond_fade && differs {
                changed_outside += 1;
            }
        }
        assert_eq!(changed_outside, 0, "pixels beyond the fade must be the photograph's own");
        assert!(changed_inside > 0, "the hole must have been filled");
        out.save(dir.join("result.png")).unwrap();
        eprintln!("result at {}", dir.join("result.png").display());
    }
}
