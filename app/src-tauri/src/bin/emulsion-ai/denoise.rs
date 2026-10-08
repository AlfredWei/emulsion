//! AI denoise inference, run inside the `emulsion-ai` helper process (M6 slice 3b, RFC-0027): a NAFNet-class
//! image-to-image ONNX model on the ONNX Runtime CPU provider (CoreML is pathological for the denoisers
//! measured in RFC-0023), applied to a full-size image in overlapping 512 px tiles (`ai/tiling.rs`).
//!
//! One lazy session, keyed by model path and dropped on `release()` (the process exiting when idle is what
//! actually returns the memory, as for SAM 2). The model contract is the one `export_nafnet.py` produces and
//! SCUNet's export shares: input `image` `1x3xHxW` f32 in 0..1 (any H, W the model accepts; the tiler always
//! sends `TILE x TILE`), first output the denoised tensor of the same shape.

use ort::session::{builder::GraphOptimizationLevel, Session};
use ort::value::TensorRef;
use std::path::Path;
use std::sync::Mutex;
use std::time::Instant;

use crate::protocol::{DenoiseResult, Region};
use crate::tiling::{run_tiled, tile_count, OVERLAP, TILE};

#[derive(Debug, thiserror::Error)]
pub enum DenoiseError {
    #[error("inference failed: {0}")]
    Ort(String),
    #[error("unexpected model output: {0}")]
    BadOutput(String),
    #[error("image error: {0}")]
    Image(String),
    #[error("denoise cancelled")]
    Cancelled,
    #[error("denoise state is poisoned")]
    Poisoned,
}

impl<T> From<ort::Error<T>> for DenoiseError {
    fn from(e: ort::Error<T>) -> Self {
        DenoiseError::Ort(e.to_string())
    }
}

/// The part of `img` inside `region` (the whole image when `None`); a region that is empty or leaves the
/// image is an error, never silently clamped (the caller chose it from what it showed the user).
pub fn crop_to_region(img: image::RgbImage, region: Option<Region>) -> Result<image::RgbImage, DenoiseError> {
    let Some(r) = region else { return Ok(img) };
    if r.w == 0 || r.h == 0 || r.x.checked_add(r.w).is_none_or(|e| e > img.width()) || r.y.checked_add(r.h).is_none_or(|e| e > img.height()) {
        return Err(DenoiseError::Image(format!("region {}x{} at ({}, {}) does not fit the {}x{} image", r.w, r.h, r.x, r.y, img.width(), img.height())));
    }
    Ok(image::imageops::crop_imm(&img, r.x, r.y, r.w, r.h).to_image())
}

/// Writes `img` as a PNG at `path` through a `.part` file and a rename, so a reader (or a crash) never sees a
/// half-written result. Fast compression: this is a cache entry written once per job, not an archive.
pub fn write_png_atomic(img: &image::RgbImage, path: &Path) -> Result<(), DenoiseError> {
    use image::codecs::png::{CompressionType, FilterType, PngEncoder};
    use image::ImageEncoder;
    let tmp = path.with_extension("png.part");
    let file = std::fs::File::create(&tmp).map_err(|e| DenoiseError::Image(format!("{}: {e}", tmp.display())))?;
    let mut writer = std::io::BufWriter::new(file);
    PngEncoder::new_with_quality(&mut writer, CompressionType::Fast, FilterType::Sub)
        .write_image(img.as_raw(), img.width(), img.height(), image::ExtendedColorType::Rgb8)
        .map_err(|e| DenoiseError::Image(e.to_string()))?;
    std::io::Write::flush(&mut writer).map_err(|e| DenoiseError::Image(e.to_string()))?;
    drop(writer);
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        DenoiseError::Image(format!("{}: {e}", path.display()))
    })
}

struct Loaded {
    model: std::path::PathBuf,
    session: Session,
}

#[derive(Default)]
pub struct Denoiser {
    loaded: Mutex<Option<Loaded>>,
}

impl Denoiser {
    /// Denoises `image_path` (or `region` of it) into the PNG at `out_path`. `progress(done, total)` is called
    /// after every tile; returning `false` stops the run with [`DenoiseError::Cancelled`] and nothing is written.
    /// Blocking and CPU-heavy (minutes for a full frame): the helper serves nothing else meanwhile.
    pub fn run(
        &self,
        model: &Path,
        image_path: &Path,
        out_path: &Path,
        region: Option<Region>,
        progress: impl FnMut(usize, usize) -> bool,
    ) -> Result<DenoiseResult, DenoiseError> {
        let started = Instant::now();
        let img = image::open(image_path).map_err(|e| DenoiseError::Image(format!("could not read {}: {e}", image_path.display())))?.to_rgb8();
        let img = crop_to_region(img, region)?;
        let (w, h) = (img.width() as usize, img.height() as usize);

        let mut guard = self.loaded.lock().map_err(|_| DenoiseError::Poisoned)?;
        if guard.as_ref().is_none_or(|l| l.model != model) {
            // Memory-pattern planning off, as for SAM 2 (RFC-0026 §7); its effect on this model is measured in the real-model test.
            let session = Session::builder()?.with_optimization_level(GraphOptimizationLevel::Level3)?.with_memory_pattern(false)?.commit_from_file(model)?;
            *guard = Some(Loaded { model: model.to_path_buf(), session });
        }
        let loaded = guard.as_mut().expect("just filled");

        let plane = TILE * TILE;
        let out = run_tiled(
            img.as_raw(),
            w,
            h,
            TILE,
            OVERLAP,
            |tile| -> Result<Vec<f32>, DenoiseError> {
                let outputs = loaded.session.run(ort::inputs!["image" => TensorRef::from_array_view(([1usize, 3, TILE, TILE], tile))?])?;
                let (shape, data) = outputs[0].try_extract_tensor::<f32>()?;
                if data.len() != 3 * plane {
                    return Err(DenoiseError::BadOutput(format!("output has shape {shape:?} for a 1x3x{TILE}x{TILE} tile")));
                }
                Ok(data.to_vec())
            },
            progress,
        )?;
        drop(guard);
        let Some(bytes) = out else { return Err(DenoiseError::Cancelled) };
        let result = image::RgbImage::from_raw(w as u32, h as u32, bytes).ok_or_else(|| DenoiseError::BadOutput("result size mismatch".into()))?;
        write_png_atomic(&result, out_path)?;
        Ok(DenoiseResult { width: w as u32, height: h as u32, tiles: tile_count(w, h, TILE, OVERLAP) as u32, ms: started.elapsed().as_millis() as u64 })
    }

    /// Drops the session (the model's weights); the process exiting when idle returns the rest.
    pub fn release(&self) {
        if let Ok(mut g) = self.loaded.lock() {
            *g = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("emulsion-denoise-test-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_region_crops_exactly_and_one_that_leaves_the_image_is_an_error() {
        let img = image::RgbImage::from_fn(10, 8, |x, y| image::Rgb([x as u8, y as u8, 0]));
        let all = crop_to_region(img.clone(), None).unwrap();
        assert_eq!((all.width(), all.height()), (10, 8));
        let part = crop_to_region(img.clone(), Some(Region { x: 3, y: 2, w: 4, h: 5 })).unwrap();
        assert_eq!((part.width(), part.height()), (4, 5));
        assert_eq!(part.get_pixel(0, 0).0, [3, 2, 0]);
        assert_eq!(part.get_pixel(3, 4).0, [6, 6, 0]);
        for bad in [Region { x: 8, y: 0, w: 3, h: 1 }, Region { x: 0, y: 7, w: 1, h: 2 }, Region { x: 0, y: 0, w: 0, h: 1 }, Region { x: u32::MAX, y: 0, w: 2, h: 1 }] {
            assert!(matches!(crop_to_region(img.clone(), Some(bad)), Err(DenoiseError::Image(_))), "{bad:?}");
        }
    }

    #[test]
    fn the_png_is_written_whole_or_not_at_all_and_round_trips() {
        let dir = test_dir("png");
        let img = image::RgbImage::from_fn(37, 21, |x, y| image::Rgb([(x * 5) as u8, (y * 9) as u8, (x + y) as u8]));
        let path = dir.join("out.png");
        write_png_atomic(&img, &path).unwrap();
        assert!(!dir.join("out.png.part").exists());
        assert_eq!(image::open(&path).unwrap().to_rgb8(), img);
        // A directory that does not exist is an error and leaves no stray file.
        assert!(write_png_atomic(&img, &dir.join("missing").join("x.png")).is_err());
    }

    #[test]
    fn an_unreadable_image_or_model_is_a_clean_error() {
        let d = Denoiser::default();
        let dir = test_dir("errors");
        let r = d.run(&dir.join("no-model.onnx"), &dir.join("no-image.png"), &dir.join("o.png"), None, |_, _| true);
        assert!(matches!(r, Err(DenoiseError::Image(_))), "{r:?}");
        image::RgbImage::new(4, 4).save(dir.join("in.png")).unwrap();
        let r = d.run(&dir.join("no-model.onnx"), &dir.join("in.png"), &dir.join("o.png"), None, |_, _| true);
        assert!(matches!(r, Err(DenoiseError::Ort(_))), "{r:?}");
        assert!(!dir.join("o.png").exists());
    }

    /// Real weights: `DENOISE_MODEL=/path/nafnet_sidd_w32.onnx DENOISE_IMAGE=/path/photo.jpg cargo test --bins -- --ignored
    /// real_model_denoises`. Writes `denoise-real-out.png` next to the test dir and prints time per tile; the
    /// optional `DENOISE_REGION=x,y,w,h` limits the run (the crop preview path).
    #[test]
    #[ignore]
    fn real_model_denoises_a_photo_and_stops_on_request() {
        let model = std::env::var("DENOISE_MODEL").expect("DENOISE_MODEL");
        let image = std::env::var("DENOISE_IMAGE").expect("DENOISE_IMAGE");
        let region = std::env::var("DENOISE_REGION").ok().map(|s| {
            let v: Vec<u32> = s.split(',').map(|n| n.trim().parse().unwrap()).collect();
            Region { x: v[0], y: v[1], w: v[2], h: v[3] }
        });
        let d = Denoiser::default();
        let out = test_dir("real").join("denoise-real-out.png");
        let mut last = (0, 0);
        let r = d.run(Path::new(&model), Path::new(&image), &out, region, |done, total| {
            last = (done, total);
            true
        });
        let r = r.expect("denoise");
        eprintln!("denoised {}x{} in {} tiles, {} ms ({} ms per tile) -> {}", r.width, r.height, r.tiles, r.ms, r.ms / r.tiles.max(1) as u64, out.display());
        assert_eq!(last.0, last.1);
        let back = image::open(&out).unwrap().to_rgb8();
        assert_eq!((back.width(), back.height()), (r.width, r.height));

        // Stopping after the first tile writes nothing.
        let out2 = out.with_file_name("denoise-real-cancelled.png");
        let stopped = d.run(Path::new(&model), Path::new(&image), &out2, region, |_, _| false);
        assert!(matches!(stopped, Err(DenoiseError::Cancelled)), "{stopped:?}");
        assert!(!out2.exists());
    }
}
