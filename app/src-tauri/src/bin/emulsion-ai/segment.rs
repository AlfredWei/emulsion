//! Click-to-select inference, run inside the `emulsion-ai` helper process (M6 slice 2a, RFC-0026): SAM 2 Hiera-Tiny on the
//! ONNX Runtime CPU provider (CoreML is pathological for this encoder, RFC-0023
//! §4). Two pieces of state, both lazy and both droppable:
//!
//! - the two `ort` sessions (≈ 155 MB of weights), created on first use and
//!   dropped on `release()` (the helper process itself exits when idle, which is
//!   what actually returns the memory to the OS);
//! - **one** image embedding (≈ 16 MB), keyed by content hash, for the image
//!   currently being edited. Never persisted (RFC-0026 §3.1).
//!
//! Everything that is not the model call is a pure function (pre-processing,
//! prompt packing, logit quantisation and PNG coding) so it is unit-tested
//! without weights; the model path has an `#[ignore]`d test that needs the
//! real files (`SEGMENT_MODELS_DIR`).
//!
//! Coordinates: prompts and results are **normalized to the full, uncropped
//! image** (the frame every mask in this app uses). The model sees the image
//! **squashed** to 1024x1024 (as SAM 2's own pipeline and the RFC-0023
//! reference do), so a normalized point maps by plain scaling and the 256x256
//! logit field covers the whole frame.

use ort::session::{builder::GraphOptimizationLevel, Session};
use ort::value::TensorRef;
use std::path::Path;
use std::sync::Mutex;
use std::time::Instant;

use crate::protocol::{Candidate, ModelPaths, Prompt};

/// Side of the square the model sees.
pub const MODEL_SIZE: usize = 1024;
/// Side of the decoder's mask output (logits), covering the whole frame.
pub const MASK_SIZE: usize = 256;
/// Logits are stored as 8-bit over this symmetric range (RFC-0026 §3.2).
pub const LOGIT_RANGE: f32 = 8.0;

const MEAN: [f32; 3] = [0.485, 0.456, 0.406];
const STD: [f32; 3] = [0.229, 0.224, 0.225];

// Encoder output shapes (from the RFC-0023 reference run): 1x256x64x64, 1x32x256x256, 1x64x128x128.
const EMBED_SHAPE: [usize; 4] = [1, 256, 64, 64];
const HR0_SHAPE: [usize; 4] = [1, 32, 256, 256];
const HR1_SHAPE: [usize; 4] = [1, 64, 128, 128];

#[derive(Debug, thiserror::Error)]
pub enum SegmentError {
    #[error("no image is prepared for {0}; prepare it first")]
    NotPrepared(String),
    #[error("inference failed: {0}")]
    Ort(String),
    #[error("unexpected model output: {0}")]
    BadOutput(String),
    #[error("image error: {0}")]
    Image(String),
    #[error("segmentation state is poisoned")]
    Poisoned,
}

impl<T> From<ort::Error<T>> for SegmentError {
    fn from(e: ort::Error<T>) -> Self {
        SegmentError::Ort(e.to_string())
    }
}

// ---- pure helpers ---------------------------------------------------------

/// Squashes `img` to 1024x1024 and returns the normalized CHW f32 tensor data
/// (ImageNet mean/std, as the reference script does).
pub fn preprocess(img: &image::RgbImage) -> Vec<f32> {
    let resized = image::imageops::resize(img, MODEL_SIZE as u32, MODEL_SIZE as u32, image::imageops::FilterType::Triangle);
    let plane = MODEL_SIZE * MODEL_SIZE;
    let mut out = vec![0f32; 3 * plane];
    for (i, px) in resized.pixels().enumerate() {
        for c in 0..3 {
            out[c * plane + i] = (px.0[c] as f32 / 255.0 - MEAN[c]) / STD[c];
        }
    }
    out
}

/// Packs prompts for the decoder: coordinates in model pixels plus labels
/// (1 positive, 0 negative). SAM's convention for "no box prompt" appends one
/// padding point at (0, 0) with label -1, which the reference also does.
pub fn pack_prompts(prompts: &[Prompt]) -> (Vec<f32>, Vec<f32>) {
    let mut coords = Vec::with_capacity((prompts.len() + 1) * 2);
    let mut labels = Vec::with_capacity(prompts.len() + 1);
    for p in prompts {
        coords.push(p.x.clamp(0.0, 1.0) * MODEL_SIZE as f32);
        coords.push(p.y.clamp(0.0, 1.0) * MODEL_SIZE as f32);
        labels.push(if p.positive { 1.0 } else { 0.0 });
    }
    coords.extend([0.0, 0.0]);
    labels.push(-1.0);
    (coords, labels)
}

/// Logits -> 8-bit: `-8..8` mapped linearly onto `0..255`, clamped.
pub fn quantise_logits(logits: &[f32]) -> Vec<u8> {
    logits
        .iter()
        .map(|&l| (((l.clamp(-LOGIT_RANGE, LOGIT_RANGE) + LOGIT_RANGE) / (2.0 * LOGIT_RANGE)) * 255.0).round() as u8)
        .collect()
}

pub fn dequantise_logits(bytes: &[u8]) -> Vec<f32> {
    bytes.iter().map(|&b| b as f32 / 255.0 * (2.0 * LOGIT_RANGE) - LOGIT_RANGE).collect()
}

/// 256x256 quantised logits -> PNG bytes.
pub fn encode_logits_png(quantised: &[u8]) -> Result<Vec<u8>, SegmentError> {
    use image::ImageEncoder;
    let mut out = Vec::new();
    image::codecs::png::PngEncoder::new(&mut out)
        .write_image(quantised, MASK_SIZE as u32, MASK_SIZE as u32, image::ExtendedColorType::L8)
        .map_err(|e| SegmentError::Image(e.to_string()))?;
    Ok(out)
}

/// PNG bytes -> 256x256 logits (inverse of `encode_logits_png` + `quantise_logits`).
pub fn decode_logits_png(png: &[u8]) -> Result<Vec<f32>, SegmentError> {
    let img = image::load_from_memory_with_format(png, image::ImageFormat::Png).map_err(|e| SegmentError::Image(e.to_string()))?;
    let gray = img.to_luma8();
    if gray.width() as usize != MASK_SIZE || gray.height() as usize != MASK_SIZE {
        return Err(SegmentError::BadOutput(format!("mask is {}x{}, expected {MASK_SIZE}x{MASK_SIZE}", gray.width(), gray.height())));
    }
    Ok(dequantise_logits(gray.as_raw()))
}

fn b64(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn unb64(s: &str) -> Result<Vec<u8>, SegmentError> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.decode(s).map_err(|e| SegmentError::Image(e.to_string()))
}

// ---- model state ----------------------------------------------------------

struct Embedding {
    content_hash: String,
    image_embed: Vec<f32>,
    high_res_0: Vec<f32>,
    high_res_1: Vec<f32>,
}

struct Loaded {
    encoder: Session,
    decoder: Session,
    last_used: Instant,
}

/// Lazy sessions + the single cached embedding. Held in Tauri-managed state.
#[derive(Default)]
pub struct Segmenter {
    loaded: Mutex<Option<Loaded>>,
    embedding: Mutex<Option<Embedding>>,
}

fn build_session(path: &Path) -> Result<Session, SegmentError> {
    // Memory-pattern planning off: measured 1.44 GB -> 1.15 GB peak RSS with no speed change
    // (RFC-0026 §7); disabling the CPU arena or lowering the optimisation level made memory worse.
    Ok(Session::builder()?
        .with_optimization_level(GraphOptimizationLevel::Level3)?
        .with_memory_pattern(false)?
        .commit_from_file(path)?)
}

impl Segmenter {
    /// Whether an embedding for this image is already cached.
    pub fn has_embedding(&self, content_hash: &str) -> bool {
        self.embedding.lock().map(|e| e.as_ref().is_some_and(|e| e.content_hash == content_hash)).unwrap_or(false)
    }

    /// Runs the encoder for `img` unless its embedding is cached. Returns the
    /// milliseconds the encoder took (0 on a cache hit) for status/telemetry.
    /// Blocking and CPU-heavy: call from `spawn_blocking`.
    pub fn prepare(&self, paths: &ModelPaths, content_hash: &str, img: &image::RgbImage) -> Result<u128, SegmentError> {
        if self.has_embedding(content_hash) {
            self.touch();
            return Ok(0);
        }
        let started = Instant::now();
        let input = preprocess(img);
        let mut guard = self.loaded.lock().map_err(|_| SegmentError::Poisoned)?;
        let loaded = Self::ensure_loaded(&mut guard, paths)?;

        let outputs = loaded.encoder.run(ort::inputs!["image" => TensorRef::from_array_view(([1usize, 3, MODEL_SIZE, MODEL_SIZE], &input[..]))?])?;
        // Output order, as in the reference run: high_res_feats_0, high_res_feats_1, image_embed.
        let take = |i: usize, shape: [usize; 4]| -> Result<Vec<f32>, SegmentError> {
            let (s, data) = outputs[i].try_extract_tensor::<f32>()?;
            let want: Vec<i64> = shape.iter().map(|&d| d as i64).collect();
            if s.iter().copied().collect::<Vec<i64>>() != want {
                return Err(SegmentError::BadOutput(format!("encoder output {i} has shape {s:?}, expected {want:?}")));
            }
            Ok(data.to_vec())
        };
        let high_res_0 = take(0, HR0_SHAPE)?;
        let high_res_1 = take(1, HR1_SHAPE)?;
        let image_embed = take(2, EMBED_SHAPE)?;
        drop(outputs);
        loaded.last_used = Instant::now();
        drop(guard);

        *self.embedding.lock().map_err(|_| SegmentError::Poisoned)? =
            Some(Embedding { content_hash: content_hash.to_string(), image_embed, high_res_0, high_res_1 });
        Ok(started.elapsed().as_millis())
    }

    /// Runs only the decoder for the prepared image: all candidates for these
    /// prompts, best predicted IoU first. `refine` is the previous accepted
    /// candidate's `logits_png` for a refinement click.
    pub fn decode(
        &self,
        paths: &ModelPaths,
        content_hash: &str,
        prompts: &[Prompt],
        refine: Option<&str>,
    ) -> Result<Vec<Candidate>, SegmentError> {
        let emb_guard = self.embedding.lock().map_err(|_| SegmentError::Poisoned)?;
        let emb = emb_guard.as_ref().filter(|e| e.content_hash == content_hash).ok_or_else(|| SegmentError::NotPrepared(content_hash.to_string()))?;

        let (coords, labels) = pack_prompts(prompts);
        let n = labels.len();
        let (mask_input, has_mask) = match refine {
            Some(png) => (decode_logits_png(&unb64(png)?)?, 1.0f32),
            None => (vec![0f32; MASK_SIZE * MASK_SIZE], 0.0f32),
        };
        let has_mask_arr = [has_mask];

        let mut guard = self.loaded.lock().map_err(|_| SegmentError::Poisoned)?;
        let loaded = Self::ensure_loaded(&mut guard, paths)?;
        let outputs = loaded.decoder.run(ort::inputs![
            "image_embed" => TensorRef::from_array_view((EMBED_SHAPE, &emb.image_embed[..]))?,
            "high_res_feats_0" => TensorRef::from_array_view((HR0_SHAPE, &emb.high_res_0[..]))?,
            "high_res_feats_1" => TensorRef::from_array_view((HR1_SHAPE, &emb.high_res_1[..]))?,
            "point_coords" => TensorRef::from_array_view(([1usize, n, 2], &coords[..]))?,
            "point_labels" => TensorRef::from_array_view(([1usize, n], &labels[..]))?,
            "mask_input" => TensorRef::from_array_view(([1usize, 1, MASK_SIZE, MASK_SIZE], &mask_input[..]))?,
            "has_mask_input" => TensorRef::from_array_view(([1usize], &has_mask_arr[..]))?,
        ])?;
        let (mask_shape, masks) = outputs[0].try_extract_tensor::<f32>()?;
        let (_, ious) = outputs[1].try_extract_tensor::<f32>()?;
        let count = ious.len();
        if mask_shape.len() != 4 || mask_shape[2] as usize != MASK_SIZE || mask_shape[3] as usize != MASK_SIZE || masks.len() != count * MASK_SIZE * MASK_SIZE {
            return Err(SegmentError::BadOutput(format!("decoder masks have shape {mask_shape:?} for {count} scores")));
        }
        let mut out = Vec::with_capacity(count);
        for k in 0..count {
            let logits = &masks[k * MASK_SIZE * MASK_SIZE..(k + 1) * MASK_SIZE * MASK_SIZE];
            out.push(Candidate { index: k, iou: ious[k], logits_png: b64(&encode_logits_png(&quantise_logits(logits))?) });
        }
        loaded.last_used = Instant::now();
        out.sort_by(|a, b| b.iou.partial_cmp(&a.iou).unwrap_or(std::cmp::Ordering::Equal));
        Ok(out)
    }

    fn ensure_loaded<'a>(slot: &'a mut Option<Loaded>, paths: &ModelPaths) -> Result<&'a mut Loaded, SegmentError> {
        if slot.is_none() {
            *slot = Some(Loaded { encoder: build_session(&paths.encoder)?, decoder: build_session(&paths.decoder)?, last_used: Instant::now() });
        }
        Ok(slot.as_mut().expect("just filled"))
    }

    fn touch(&self) {
        if let Ok(mut g) = self.loaded.lock() {
            if let Some(l) = g.as_mut() {
                l.last_used = Instant::now();
            }
        }
    }

    /// Drops everything (sessions and embedding), e.g. when the tool is released.
    pub fn release(&self) {
        if let Ok(mut g) = self.loaded.lock() {
            *g = None;
        }
        if let Ok(mut e) = self.embedding.lock() {
            *e = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preprocess_normalises_with_imagenet_mean_and_std_in_chw_order() {
        // A solid colour survives any resize filter exactly.
        let img = image::RgbImage::from_pixel(7, 5, image::Rgb([255, 0, 128]));
        let t = preprocess(&img);
        let plane = MODEL_SIZE * MODEL_SIZE;
        assert_eq!(t.len(), 3 * plane);
        let want = [(1.0 - MEAN[0]) / STD[0], (0.0 - MEAN[1]) / STD[1], (128.0 / 255.0 - MEAN[2]) / STD[2]];
        for (c, w) in want.iter().enumerate() {
            assert!((t[c * plane] - w).abs() < 1e-4 && (t[c * plane + plane - 1] - w).abs() < 1e-4, "channel {c}");
        }
    }

    #[test]
    fn prompts_scale_to_model_pixels_and_get_the_sam_padding_point() {
        let (coords, labels) = pack_prompts(&[
            Prompt { x: 0.5, y: 0.25, positive: true },
            Prompt { x: 1.5, y: -1.0, positive: false }, // out of range clamps to the frame
        ]);
        assert_eq!(coords, vec![512.0, 256.0, 1024.0, 0.0, 0.0, 0.0]);
        assert_eq!(labels, vec![1.0, 0.0, -1.0]);
    }

    #[test]
    fn logit_quantisation_hits_the_ends_and_the_middle_and_round_trips_within_half_a_step() {
        assert_eq!(quantise_logits(&[-20.0, -8.0, 0.0, 8.0, 20.0]), vec![0, 0, 128, 255, 255]);
        let step = 2.0 * LOGIT_RANGE / 255.0;
        for l in [-7.9f32, -3.3, -0.1, 0.0, 0.07, 2.5, 7.9] {
            let back = dequantise_logits(&quantise_logits(&[l]))[0];
            assert!((back - l).abs() <= step / 2.0 + 1e-5, "{l} -> {back}");
        }
        // The sign (inside/outside) is never lost to quantisation away from zero.
        assert!(dequantise_logits(&quantise_logits(&[0.2]))[0] > 0.0 && dequantise_logits(&quantise_logits(&[-0.2]))[0] < 0.0);
    }

    #[test]
    fn a_logit_field_survives_png_and_base64() {
        let logits: Vec<f32> = (0..MASK_SIZE * MASK_SIZE).map(|i| ((i % 256) as f32 - 128.0) / 16.0).collect();
        let q = quantise_logits(&logits);
        let png = encode_logits_png(&q).unwrap();
        let again = decode_logits_png(&unb64(&b64(&png)).unwrap()).unwrap();
        assert_eq!(again, dequantise_logits(&q));
    }

    #[test]
    fn a_png_of_the_wrong_size_is_rejected() {
        use image::ImageEncoder;
        let mut out = Vec::new();
        image::codecs::png::PngEncoder::new(&mut out).write_image(&[0u8; 16], 4, 4, image::ExtendedColorType::L8).unwrap();
        assert!(matches!(decode_logits_png(&out), Err(SegmentError::BadOutput(_))));
    }

    #[test]
    fn decode_without_prepare_says_so() {
        let seg = Segmenter::default();
        assert!(!seg.has_embedding("abc"));
        let paths = ModelPaths { encoder: "e".into(), decoder: "d".into() };
        assert!(matches!(seg.decode(&paths, "abc", &[], None), Err(SegmentError::NotPrepared(_))));
    }

    fn rss_mb() -> u64 {
        let out = std::process::Command::new("ps").args(["-o", "rss=", "-p", &std::process::id().to_string()]).output().unwrap();
        String::from_utf8_lossy(&out.stdout).trim().parse::<u64>().unwrap_or(0) / 1024
    }

    /// Needs the real weights and a reference dump from Python onnxruntime
    /// (`SEGMENT_MODELS_DIR` = folder with the two .onnx files, `SEGMENT_REF_DIR`
    /// = output of `docs/rfc/RFC-0026-appendix/ref_sam2.py`):
    /// `SEGMENT_MODELS_DIR=... SEGMENT_REF_DIR=... cargo test --lib --release -- --ignored --nocapture segment_matches`.
    /// The six RFC-0023 §4.2 clicks through the Rust path must reproduce the
    /// Python runtime's candidates (same ONNX Runtime, so near-identical).
    #[test]
    #[ignore]
    fn segment_matches_the_python_reference_on_the_six_rfc_0023_clicks() {
        let models = std::path::PathBuf::from(std::env::var("SEGMENT_MODELS_DIR").expect("SEGMENT_MODELS_DIR"));
        let refs = std::path::PathBuf::from(std::env::var("SEGMENT_REF_DIR").expect("SEGMENT_REF_DIR"));
        let paths = ModelPaths { encoder: models.join("sam2_hiera_tiny.encoder.onnx"), decoder: models.join("sam2_hiera_tiny.decoder.onnx") };
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test_image");
        let cases = [
            ("Field-corn-Liechtenstein-landscape.jpg", 0.5, 0.75),
            ("Field-corn-Liechtenstein-landscape.jpg", 0.3, 0.2),
            ("Szechenyi-Chain-Bridge-Budapest-night.jpg", 0.37, 0.72),
            ("Cavenagh-Bridge-Singapore-panorama-frame-a.jpg", 0.28, 0.3),
            ("Smiling-woman-pink-shirt-portrait.jpg", 0.42, 0.82),
            ("Smiling-woman-pink-shirt-portrait.jpg", 0.5, 0.35),
        ];
        let read_f32 = |p: std::path::PathBuf| -> Vec<f32> { std::fs::read(p).unwrap().chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect() };
        let seg = Segmenter::default();
        let mut worst = 1.0f32;
        for (i, (file, x, y)) in cases.iter().enumerate() {
            let img = image::open(root.join(file)).unwrap().to_rgb8();
            let t = Instant::now();
            let enc_ms = seg.prepare(&paths, file, &img).unwrap();
            let t_dec = Instant::now();
            let cands = seg.decode(&paths, file, &[Prompt { x: *x, y: *y, positive: true }], None).unwrap();
            let dec_ms = t_dec.elapsed().as_millis();
            let ref_logits = read_f32(refs.join(format!("case{i}.logits.f32")));
            let ref_iou = read_f32(refs.join(format!("case{i}.iou.f32")));
            assert!(cands.windows(2).all(|w| w[0].iou >= w[1].iou), "candidates must be best-IoU first");
            for c in &cands {
                let mine = decode_logits_png(&unb64(&c.logits_png).unwrap()).unwrap();
                let theirs = &ref_logits[c.index * MASK_SIZE * MASK_SIZE..(c.index + 1) * MASK_SIZE * MASK_SIZE];
                let (mut inter, mut uni) = (0usize, 0usize);
                for (a, b) in mine.iter().zip(theirs) {
                    let (a, b) = (*a > 0.0, *b > 0.0);
                    inter += (a && b) as usize;
                    uni += (a || b) as usize;
                }
                let iou = if uni == 0 { 1.0 } else { inter as f32 / uni as f32 };
                // Degenerate candidates (predicted IoU < 0.3, a few texels) flip on 1-2 texels from the
                // 8-bit quantisation and the resize filter, so only candidates the model itself rates are held to 0.95 (the squash uses the image crate's triangle filter, PIL's bilinear in the reference, which moves uncertain candidates a few percent).
                if c.iou >= 0.3 {
                    worst = worst.min(iou);
                } else {
                    assert!(iou >= 0.85, "case {i} degenerate candidate {}: mask IoU {iou}", c.index);
                }
                assert!((c.iou - ref_iou[c.index]).abs() < 0.05, "case {i} candidate {}: predicted IoU {} vs reference {}", c.index, c.iou, ref_iou[c.index]);
                println!("case {i} {file} cand {} pred_iou {:.3} mask-IoU vs python {:.4}", c.index, c.iou, iou);
            }
            println!("case {i}: encode {enc_ms} ms (incl. session load on first), decode {dec_ms} ms, total {} ms", t.elapsed().as_millis());
            assert!(worst >= 0.95, "case {i}: worst mask IoU {worst}");
        }
        println!("rss after the last image (sessions + embedding loaded): {} MB", rss_mb());
        seg.release();
        println!("rss after release(): {} MB", rss_mb());
        println!("worst mask IoU over all candidates: {worst:.4}");
    }
}
