//! AI super-resolution, run inside the `emulsion-ai` helper process (M6 slice 4b, RFC-0029): a 4x image-to-image
//! ONNX model (Real-ESRGAN-compact class, `image` 1x3xHxW f32 in 0..1 -> first output 1x3x4Hx4W) on the ONNX
//! Runtime CPU provider, applied in overlapping 256 px tiles by the band-streamed pipeline of
//! `ai/upscale_pipeline.rs` and written **row by row** into a JPEG, so the result's size does not decide the
//! memory the job needs.
//!
//! The encoder pulls rows (`jpeg_encoder::ImageBuffer`), the pipeline produces them: asking for row `y` runs
//! as many tiles as it takes. A cancel, a model error or a full disk is remembered, the writer is told to
//! refuse further bytes (so the encoder stops at once instead of coding the rest of the picture), and the
//! half-written `.part` file is removed; the result file appears only by a rename when it is complete.

use jpeg_encoder::{Encoder, ImageBuffer, JpegColorType};
use ort::session::{builder::GraphOptimizationLevel, Session};
use ort::value::TensorRef;
use std::cell::{Cell, RefCell};
use std::io::Write;
use std::path::Path;
use std::rc::Rc;
use std::sync::Mutex;
use std::time::Instant;

use crate::denoise::{crop_to_region, DenoiseError};
use crate::protocol::{Region, UpscaleResult};
use crate::upscale_pipeline::{Pipeline, MODEL_SCALE, OVERLAP, TILE};

/// JPEG's hard limit on either side.
pub const MAX_SIDE: u32 = 65_535;

#[derive(Debug, thiserror::Error)]
pub enum UpscaleError {
    #[error("inference failed: {0}")]
    Ort(String),
    #[error("unexpected model output: {0}")]
    BadOutput(String),
    #[error("image error: {0}")]
    Image(String),
    #[error("{0}")]
    BadRequest(String),
    #[error("upscale cancelled")]
    Cancelled,
    #[error("upscale state is poisoned")]
    Poisoned,
}

impl<T> From<ort::Error<T>> for UpscaleError {
    fn from(e: ort::Error<T>) -> Self {
        UpscaleError::Ort(e.to_string())
    }
}

/// What a job asks for (the wire request, minus the paths).
#[derive(Debug, Clone, Copy)]
pub struct Job {
    pub out_w: u32,
    pub out_h: u32,
    /// 1..=100: how much of the model's result is blended over a plain enlargement.
    pub strength: u8,
    /// JPEG quality, 1..=100 (4:4:4 from 90 up).
    pub quality: u8,
}

impl Job {
    fn validate(&self) -> Result<(), UpscaleError> {
        let bad = |m: String| Err(UpscaleError::BadRequest(m));
        if !(1..=100).contains(&self.strength) {
            return bad(format!("strength must be 1..=100, got {}", self.strength));
        }
        if !(1..=100).contains(&self.quality) {
            return bad(format!("jpeg quality must be 1..=100, got {}", self.quality));
        }
        if self.out_w == 0 || self.out_h == 0 || self.out_w > MAX_SIDE || self.out_h > MAX_SIDE {
            return bad(format!("{}x{} is outside what a JPEG can hold (1..={MAX_SIDE} on a side)", self.out_w, self.out_h));
        }
        Ok(())
    }
}

/// Writes through to `inner` until `abort` is set, then refuses every byte: the way to make the encoder give
/// up in the middle of a picture whose rows can no longer be produced.
struct AbortableWriter<W: Write> {
    inner: W,
    abort: Rc<Cell<bool>>,
}

impl<W: Write> Write for AbortableWriter<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if self.abort.get() {
            return Err(std::io::Error::other("job aborted"));
        }
        self.inner.write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

struct FeedState<'a, M, P> {
    pipeline: Pipeline<'a>,
    model: M,
    progress: P,
    row: Vec<u8>,
    /// Rows fetched from the pipeline so far.
    fetched: usize,
}

impl<M, P> FeedState<'_, M, P>
where
    M: FnMut(&[f32]) -> Result<Vec<f32>, UpscaleError>,
    P: FnMut(usize, usize) -> bool,
{
    fn fetch(&mut self) -> Result<(), UpscaleError> {
        loop {
            if let Some(row) = self.pipeline.take_row() {
                self.row = row;
                self.fetched += 1;
                return Ok(());
            }
            match self.pipeline.run_tile(&mut self.model)? {
                Some((done, total)) => {
                    if !(self.progress)(done, total) {
                        return Err(UpscaleError::Cancelled);
                    }
                }
                None => return Err(UpscaleError::BadOutput("the tiles ran out before the last row".into())),
            }
        }
    }
}

/// The encoder's view of the picture: row `y` on request, rows asked for in order (the last one repeatedly at
/// the bottom edge, which the encoder pads with).
struct Feed<'a, M, P> {
    width: u16,
    height: u16,
    state: RefCell<FeedState<'a, M, P>>,
    abort: Rc<Cell<bool>>,
    /// The first failure (a cancel is one); shared because the encoder consumes the feed.
    error: Rc<RefCell<Option<UpscaleError>>>,
}

/// BT.601 full-range RGB -> YCbCr, the JFIF way.
fn ycbcr(r: u8, g: u8, b: u8) -> (u8, u8, u8) {
    let (r, g, b) = (r as f32, g as f32, b as f32);
    let y = 0.299 * r + 0.587 * g + 0.114 * b;
    let cb = 128.0 - 0.168_736 * r - 0.331_264 * g + 0.5 * b;
    let cr = 128.0 + 0.5 * r - 0.418_688 * g - 0.081_312 * b;
    let q = |v: f32| (v + 0.5).clamp(0.0, 255.0) as u8;
    (q(y), q(cb), q(cr))
}

impl<M, P> ImageBuffer for Feed<'_, M, P>
where
    M: FnMut(&[f32]) -> Result<Vec<f32>, UpscaleError>,
    P: FnMut(usize, usize) -> bool,
{
    fn get_jpeg_color_type(&self) -> JpegColorType {
        JpegColorType::Ycbcr
    }
    fn width(&self) -> u16 {
        self.width
    }
    fn height(&self) -> u16 {
        self.height
    }
    fn fill_buffers(&self, y: u16, buffers: &mut [Vec<u8>; 4]) {
        let mut st = self.state.borrow_mut();
        while st.fetched <= y as usize && self.error.borrow().is_none() {
            if let Err(e) = st.fetch() {
                *self.error.borrow_mut() = Some(e);
                self.abort.set(true);
            }
        }
        if self.error.borrow().is_some() {
            // Anything will do: the writer refuses the bytes that follow.
            for _ in 0..self.width {
                buffers[0].push(0);
                buffers[1].push(128);
                buffers[2].push(128);
            }
            return;
        }
        for px in st.row.chunks_exact(3) {
            let (yy, cb, cr) = ycbcr(px[0], px[1], px[2]);
            buffers[0].push(yy);
            buffers[1].push(cb);
            buffers[2].push(cr);
        }
    }
}

/// Enlarges the `w x h` RGB8 picture `rgb` to `job.out_w x job.out_h` and writes it as a JPEG at `out_path`
/// (through `<out_path>.part` and a rename). `model` maps a `TILE x TILE` CHW tensor to the `4 TILE` square
/// one; `progress(done, total)` is called per tile and returns `false` to stop (nothing is left behind).
pub fn encode_enlargement<M, P>(rgb: &[u8], (w, h): (usize, usize), job: Job, out_path: &Path, (tile, overlap): (usize, usize), model: M, progress: P) -> Result<usize, UpscaleError>
where
    M: FnMut(&[f32]) -> Result<Vec<f32>, UpscaleError>,
    P: FnMut(usize, usize) -> bool,
{
    job.validate()?;
    let pipeline = Pipeline::new(rgb, (w, h), (job.out_w as usize, job.out_h as usize), job.strength, (tile, overlap));
    let tiles = pipeline.tile_count();

    let mut part = out_path.as_os_str().to_owned();
    part.push(".part");
    let part = std::path::PathBuf::from(part);
    let file = std::fs::File::create(&part).map_err(|e| UpscaleError::Image(format!("{}: {e}", part.display())))?;
    let abort = Rc::new(Cell::new(false));
    let mut writer = AbortableWriter { inner: std::io::BufWriter::with_capacity(1 << 20, file), abort: abort.clone() };

    let error = Rc::new(RefCell::new(None));
    let feed = Feed {
        width: job.out_w as u16,
        height: job.out_h as u16,
        state: RefCell::new(FeedState { pipeline, model, progress, row: Vec::new(), fetched: 0 }),
        abort,
        error: error.clone(),
    };
    let encoded = Encoder::new(&mut writer, job.quality).encode_image(feed);
    let flushed = writer.flush();
    drop(writer);
    let failed = |e: UpscaleError| {
        let _ = std::fs::remove_file(&part);
        Err(e)
    };
    if let Some(e) = error.borrow_mut().take() {
        return failed(e);
    }
    if let Err(e) = encoded {
        return failed(UpscaleError::Image(format!("jpeg encode: {e}")));
    }
    if let Err(e) = flushed {
        return failed(UpscaleError::Image(format!("{}: {e}", part.display())));
    }
    if let Err(e) = std::fs::rename(&part, out_path) {
        return failed(UpscaleError::Image(format!("{}: {e}", out_path.display())));
    }
    Ok(tiles)
}

struct Loaded {
    model: std::path::PathBuf,
    session: Session,
}

#[derive(Default)]
pub struct Upscaler {
    loaded: Mutex<Option<Loaded>>,
}

impl Upscaler {
    /// Enlarges `image_path` (or `region` of it) to `job`'s size into the JPEG at `out_path`.
    /// Blocking and CPU-heavy (about a minute per 6 MP of input on an M1 Pro, RFC-0029 §9).
    pub fn run(&self, model: &Path, image_path: &Path, out_path: &Path, region: Option<Region>, job: Job, progress: impl FnMut(usize, usize) -> bool) -> Result<UpscaleResult, UpscaleError> {
        let started = Instant::now();
        job.validate()?;
        let img = image::open(image_path).map_err(|e| UpscaleError::Image(format!("could not read {}: {e}", image_path.display())))?.to_rgb8();
        let img = crop_to_region(img, region).map_err(|e| match e {
            DenoiseError::Image(m) => UpscaleError::Image(m),
            other => UpscaleError::Image(other.to_string()),
        })?;
        let (w, h) = (img.width() as usize, img.height() as usize);

        let mut guard = self.loaded.lock().map_err(|_| UpscaleError::Poisoned)?;
        if guard.as_ref().is_none_or(|l| l.model != model) {
            let session = Session::builder()?.with_optimization_level(GraphOptimizationLevel::Level3)?.with_memory_pattern(false)?.commit_from_file(model)?;
            *guard = Some(Loaded { model: model.to_path_buf(), session });
        }
        let loaded = guard.as_mut().expect("just filled");
        let out_plane = (TILE * MODEL_SCALE) * (TILE * MODEL_SCALE);

        let tiles = encode_enlargement(
            img.as_raw(),
            (w, h),
            job,
            out_path,
            (TILE, OVERLAP),
            |tile| {
                let outputs = loaded.session.run(ort::inputs!["image" => TensorRef::from_array_view(([1usize, 3, TILE, TILE], tile))?])?;
                let (shape, data) = outputs[0].try_extract_tensor::<f32>()?;
                if data.len() != 3 * out_plane {
                    return Err(UpscaleError::BadOutput(format!("output has shape {shape:?} for a 1x3x{TILE}x{TILE} tile (want 1x3x{0}x{0})", TILE * MODEL_SCALE)));
                }
                Ok(data.to_vec())
            },
            progress,
        )?;
        drop(guard);
        Ok(UpscaleResult { width: job.out_w, height: job.out_h, tiles: tiles as u32, ms: started.elapsed().as_millis() as u64 })
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
        let dir = std::env::temp_dir().join(format!("emulsion-upscale-test-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A stand-in for the model: each input pixel becomes a 4x4 block of itself.
    fn nearest(tile: usize) -> impl FnMut(&[f32]) -> Result<Vec<f32>, UpscaleError> {
        move |t| {
            let (o, plane, oplane) = (tile * 4, tile * tile, tile * 4 * tile * 4);
            let mut out = vec![0f32; 3 * oplane];
            for c in 0..3 {
                for y in 0..o {
                    for x in 0..o {
                        out[c * oplane + y * o + x] = t[c * plane + (y / 4) * tile + x / 4];
                    }
                }
            }
            Ok(out)
        }
    }

    fn smooth(w: usize, h: usize) -> Vec<u8> {
        (0..h).flat_map(|y| (0..w).flat_map(move |x| [(x * 255 / w) as u8, (y * 255 / h) as u8, ((x + y) * 255 / (w + h)) as u8])).collect()
    }

    fn job(out_w: u32, out_h: u32) -> Job {
        Job { out_w, out_h, strength: 100, quality: 100 }
    }

    fn mean_abs_diff(a: &[u8], b: &[u8]) -> f64 {
        a.iter().zip(b).map(|(x, y)| (*x as f64 - *y as f64).abs()).sum::<f64>() / a.len() as f64
    }

    #[test]
    fn the_streamed_jpeg_decodes_to_the_enlarged_picture_at_the_requested_size() {
        let dir = test_dir("roundtrip");
        let (w, h) = (45, 38);
        let img = smooth(w, h);
        let out = dir.join("big.jpg");
        let tiles = encode_enlargement(&img, (w, h), job(180, 152), &out, (16, 4), nearest(16), |_, _| true).unwrap();
        assert!(tiles > 1);
        assert!(!dir.join("big.jpg.part").exists());
        let back = image::open(&out).unwrap().to_rgb8();
        assert_eq!((back.width(), back.height()), (180, 152));
        let want: Vec<u8> = (0..152).flat_map(|y| (0..180).flat_map(move |x| (0..3).map(move |c| (x, y, c)))).map(|(x, y, c)| img[((y / 4) * w + x / 4) * 3 + c]).collect();
        // A smooth picture at quality 100 and 4:4:4: the JPEG's own error only.
        let d = mean_abs_diff(back.as_raw(), &want);
        assert!(d < 1.5, "mean abs difference {d}");
    }

    #[test]
    fn a_target_that_is_not_four_times_the_source_still_comes_out_at_exactly_that_size() {
        let dir = test_dir("odd");
        let (w, h) = (40, 30);
        let out = dir.join("odd.jpg");
        encode_enlargement(&smooth(w, h), (w, h), Job { strength: 60, ..job(77, 53) }, &out, (16, 4), nearest(16), |_, _| true).unwrap();
        let back = image::open(&out).unwrap().to_rgb8();
        assert_eq!((back.width(), back.height()), (77, 53));
    }

    #[test]
    fn a_cancel_leaves_no_file_and_stops_before_the_last_tile() {
        let dir = test_dir("cancel");
        let (w, h) = (100, 90);
        let out = dir.join("c.jpg");
        let mut last = 0;
        let r = encode_enlargement(&smooth(w, h), (w, h), job(400, 360), &out, (16, 4), nearest(16), |done, _| {
            last = done;
            done < 3
        });
        assert!(matches!(r, Err(UpscaleError::Cancelled)), "{r:?}");
        assert_eq!(last, 3, "no tile may run after the stop");
        assert!(!out.exists() && !dir.join("c.jpg.part").exists());
    }

    #[test]
    fn a_model_error_leaves_no_file_and_comes_back_as_that_error() {
        let dir = test_dir("model-error");
        let out = dir.join("e.jpg");
        let r = encode_enlargement(&smooth(30, 30), (30, 30), job(120, 120), &out, (16, 4), |_| Err(UpscaleError::Ort("boom".into())), |_, _| true);
        assert!(matches!(r, Err(UpscaleError::Ort(m)) if m == "boom"));
        assert!(!out.exists() && !dir.join("e.jpg.part").exists());
    }

    #[test]
    fn a_directory_that_does_not_exist_is_an_image_error() {
        let dir = test_dir("nodir");
        let r = encode_enlargement(&smooth(8, 8), (8, 8), job(32, 32), &dir.join("missing").join("x.jpg"), (16, 4), nearest(16), |_, _| true);
        assert!(matches!(r, Err(UpscaleError::Image(_))), "{r:?}");
    }

    #[test]
    fn requests_a_jpeg_cannot_hold_are_refused_before_any_work() {
        let dir = test_dir("limits");
        let out = dir.join("x.jpg");
        let img = smooth(8, 8);
        for bad in [job(0, 10), job(10, 0), job(65_536, 10), job(10, 70_000), Job { strength: 0, ..job(32, 32) }, Job { strength: 101, ..job(32, 32) }, Job { quality: 0, ..job(32, 32) }] {
            let r = encode_enlargement(&img, (8, 8), bad, &out, (16, 4), nearest(16), |_, _| true);
            assert!(matches!(r, Err(UpscaleError::BadRequest(_))), "{bad:?}: {r:?}");
        }
        assert!(!out.exists() && !dir.join("x.jpg.part").exists());
    }

    #[test]
    fn an_unreadable_image_or_model_is_a_clean_error() {
        let u = Upscaler::default();
        let dir = test_dir("errors");
        let r = u.run(&dir.join("no-model.onnx"), &dir.join("no-image.png"), &dir.join("o.jpg"), None, job(32, 32), |_, _| true);
        assert!(matches!(r, Err(UpscaleError::Image(_))), "{r:?}");
        image::RgbImage::new(4, 4).save(dir.join("in.png")).unwrap();
        let r = u.run(&dir.join("no-model.onnx"), &dir.join("in.png"), &dir.join("o.jpg"), None, job(16, 16), |_, _| true);
        assert!(matches!(r, Err(UpscaleError::Ort(_))), "{r:?}");
        assert!(!dir.join("o.jpg").exists());
    }

    /// Real weights: `UPSCALE_MODEL=/path/realesr-general-x4v3.onnx UPSCALE_IMAGE=/path/photo.jpg cargo test --release --bins --
    /// --ignored --nocapture real_model_enlarges`. Optional `UPSCALE_REGION=x,y,w,h`, `UPSCALE_STRENGTH` (default 50).
    #[test]
    #[ignore]
    fn real_model_enlarges_a_photo_and_stops_on_request() {
        let model = std::env::var("UPSCALE_MODEL").expect("UPSCALE_MODEL");
        let image = std::env::var("UPSCALE_IMAGE").expect("UPSCALE_IMAGE");
        let strength: u8 = std::env::var("UPSCALE_STRENGTH").ok().and_then(|s| s.parse().ok()).unwrap_or(50);
        let region = std::env::var("UPSCALE_REGION").ok().map(|s| {
            let v: Vec<u32> = s.split(',').map(|n| n.trim().parse().unwrap()).collect();
            Region { x: v[0], y: v[1], w: v[2], h: v[3] }
        });
        let (rw, rh) = region.map(|r| (r.w, r.h)).unwrap_or_else(|| image::image_dimensions(&image).unwrap());
        let u = Upscaler::default();
        let out = test_dir("real").join("upscale-real-out.jpg");
        let j = Job { out_w: rw * 2, out_h: rh * 2, strength, quality: 95 };
        let r = u.run(Path::new(&model), Path::new(&image), &out, region, j, |_, _| true).expect("upscale");
        eprintln!("enlarged {rw}x{rh} to {}x{} in {} tiles, {} ms ({} ms per tile) -> {}", r.width, r.height, r.tiles, r.ms, r.ms / r.tiles.max(1) as u64, out.display());
        let back = image::open(&out).unwrap().to_rgb8();
        assert_eq!((back.width(), back.height()), (r.width, r.height));

        let out2 = out.with_file_name("upscale-real-cancelled.jpg");
        let stopped = u.run(Path::new(&model), Path::new(&image), &out2, region, j, |_, _| false);
        assert!(matches!(stopped, Err(UpscaleError::Cancelled)), "{stopped:?}");
        assert!(!out2.exists());
    }
}
