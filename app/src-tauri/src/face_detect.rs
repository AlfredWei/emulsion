//! Face detection + embedding (M5 Slice 6, RFC-0005 §3.1-§3.2). Pure image
//! processing over `tract`-loaded ONNX models, no catalog/Tauri dependency
//! -- matches `panorama_merge.rs`/`hdr_merge.rs`'s own "this module only
//! knows about pixels" split.
//!
//! Preprocessing/decode logic is ported directly from OpenCV's own
//! `FaceDetectorYN` (detection) and `FaceRecognizerSF` (embedding) C++
//! implementations (`opencv/opencv` 5.x,
//! `modules/objdetect/src/face_detect.cpp` and `face_recognize.cpp`) rather
//! than reverse-engineered from the ONNX graphs alone -- channel order
//! (BGR for YuNet, RGB for SFace), the exact anchor-decode formula, the
//! score/NMS thresholds, and the 112x112 alignment template are all taken
//! verbatim from that reference, not guessed. `tract`'s op coverage against
//! both real model files was verified empirically before writing this
//! (RFC-0005 §3.2's own named risk) -- see PROGRESS.md.

use image::RgbImage;
use std::path::Path;
use tract_onnx::prelude::*;

#[derive(Debug, thiserror::Error)]
pub enum FaceDetectError {
    #[error("could not load model {0}: {1}")]
    LoadModel(String, String),
    #[error("inference over {0} failed: {1}")]
    Inference(&'static str, String),
}

type RunnablePlan = std::sync::Arc<TypedRunnableModel>;

/// YuNet's own output ordering (`cls_8, cls_16, cls_32, obj_8, ..., bbox_8,
/// ..., kps_8, ...`) -- confirmed both from OpenCV's `output_names` list
/// and empirically from `model.outputs` on the real file: 12 outputs in
/// exactly this grouping.
const YUNET_INPUT_SIZE: usize = 640;
const YUNET_STRIDES: [usize; 3] = [8, 16, 32];
const YUNET_SCORE_THRESHOLD: f32 = 0.6;
const YUNET_NMS_IOU_THRESHOLD: f32 = 0.3;

const SFACE_INPUT_SIZE: u32 = 112;

/// One detected face, in coordinates normalized to `[0, 1]` of the input
/// image -- matches the `faces` table's own `bbox_x/y/w/h` convention
/// (catalog.rs `migrate()`).
#[derive(Debug, Clone, PartialEq)]
pub struct DetectedFace {
    pub bbox_x: f32,
    pub bbox_y: f32,
    pub bbox_w: f32,
    pub bbox_h: f32,
    /// Right eye, left eye, nose tip, right mouth corner, left mouth
    /// corner -- YuNet's own training-time landmark order, preserved
    /// as-is rather than relabeled, since `FaceEmbedder::embed` expects
    /// landmarks in this exact order for alignment.
    pub landmarks: [(f32, f32); 5],
    pub score: f32,
}

pub struct FaceDetector {
    plan: RunnablePlan,
}

pub struct FaceEmbedder {
    plan: RunnablePlan,
}

fn load_plan(path: &Path) -> Result<RunnablePlan, FaceDetectError> {
    tract_onnx::onnx()
        .model_for_path(path)
        .and_then(|m| m.into_optimized())
        .and_then(|m| m.into_runnable())
        .map_err(|e| FaceDetectError::LoadModel(path.display().to_string(), e.to_string()))
}

impl FaceDetector {
    pub fn load(path: &Path) -> Result<Self, FaceDetectError> {
        Ok(Self { plan: load_plan(path)? })
    }

    /// Detects faces in `image`, returning normalized bounding boxes +
    /// landmarks, highest score first, already non-max-suppressed.
    pub fn detect(&self, image: &RgbImage) -> Result<Vec<DetectedFace>, FaceDetectError> {
        let resized = image::imageops::resize(
            image,
            YUNET_INPUT_SIZE as u32,
            YUNET_INPUT_SIZE as u32,
            image::imageops::FilterType::Triangle,
        );

        // YuNet expects BGR, CHW, raw 0-255 f32 (OpenCV's own
        // `blobFromImage` call uses `swapRB=false` against a native-BGR
        // `Mat`, and no mean/scale normalization) -- our `image` buffers
        // are RGB, so channels 0 and 2 are swapped here rather than at
        // decode time.
        let mut tensor = Tensor::zero::<f32>(&[1, 3, YUNET_INPUT_SIZE, YUNET_INPUT_SIZE])
            .map_err(|e| FaceDetectError::Inference("yunet", e.to_string()))?;
        {
            let mut view = tensor.to_plain_array_view_mut::<f32>().map_err(|e| FaceDetectError::Inference("yunet", e.to_string()))?;
            for y in 0..YUNET_INPUT_SIZE {
                for x in 0..YUNET_INPUT_SIZE {
                    let p = resized.get_pixel(x as u32, y as u32);
                    view[[0, 0, y, x]] = p[2] as f32; // B
                    view[[0, 1, y, x]] = p[1] as f32; // G
                    view[[0, 2, y, x]] = p[0] as f32; // R
                }
            }
        }

        let outputs = self.plan.run(tvec!(tensor.into())).map_err(|e| FaceDetectError::Inference("yunet", e.to_string()))?;
        decode_yunet_outputs(&outputs)
    }
}

impl FaceEmbedder {
    pub fn load(path: &Path) -> Result<Self, FaceDetectError> {
        Ok(Self { plan: load_plan(path)? })
    }

    /// Aligns `image` against `landmarks` (in `image`'s own pixel
    /// coordinates, NOT normalized) onto SFace's canonical 112x112
    /// template, then returns the raw embedding vector. Callers hold a
    /// `DetectedFace`'s normalized landmarks -- multiply by
    /// `(image.width(), image.height())` first.
    pub fn embed(&self, image: &RgbImage, landmarks: &[(f32, f32); 5]) -> Result<Vec<f32>, FaceDetectError> {
        let aligned = align_face(image, landmarks);

        // SFace expects RGB (OpenCV's own `blobFromImage` call here uses
        // `swapRB=true` against a native-BGR `Mat`, i.e. it wants RGB) --
        // our buffer already is RGB, so no channel swap this time, unlike
        // YuNet above.
        let mut tensor = Tensor::zero::<f32>(&[1, 3, SFACE_INPUT_SIZE as usize, SFACE_INPUT_SIZE as usize])
            .map_err(|e| FaceDetectError::Inference("sface", e.to_string()))?;
        {
            let mut view = tensor.to_plain_array_view_mut::<f32>().map_err(|e| FaceDetectError::Inference("sface", e.to_string()))?;
            for y in 0..SFACE_INPUT_SIZE {
                for x in 0..SFACE_INPUT_SIZE {
                    let p = aligned.get_pixel(x, y);
                    view[[0, 0, y as usize, x as usize]] = p[0] as f32;
                    view[[0, 1, y as usize, x as usize]] = p[1] as f32;
                    view[[0, 2, y as usize, x as usize]] = p[2] as f32;
                }
            }
        }

        let outputs = self.plan.run(tvec!(tensor.into())).map_err(|e| FaceDetectError::Inference("sface", e.to_string()))?;
        let embedding = outputs[0].to_plain_array_view::<f32>().map_err(|e| FaceDetectError::Inference("sface", e.to_string()))?;
        Ok(embedding.iter().copied().collect())
    }
}

fn decode_yunet_outputs(outputs: &[TValue]) -> Result<Vec<DetectedFace>, FaceDetectError> {
    let mut candidates: Vec<DetectedFace> = Vec::new();

    for (i, &stride) in YUNET_STRIDES.iter().enumerate() {
        let cols = YUNET_INPUT_SIZE / stride;
        let rows = YUNET_INPUT_SIZE / stride;

        let cls = outputs[i].to_plain_array_view::<f32>().map_err(|e| FaceDetectError::Inference("yunet", e.to_string()))?;
        let obj = outputs[i + 3].to_plain_array_view::<f32>().map_err(|e| FaceDetectError::Inference("yunet", e.to_string()))?;
        let bbox = outputs[i + 6].to_plain_array_view::<f32>().map_err(|e| FaceDetectError::Inference("yunet", e.to_string()))?;
        let kps = outputs[i + 9].to_plain_array_view::<f32>().map_err(|e| FaceDetectError::Inference("yunet", e.to_string()))?;

        for r in 0..rows {
            for c in 0..cols {
                let idx = r * cols + c;
                let cls_score = cls[[0, idx, 0]].clamp(0.0, 1.0);
                let obj_score = obj[[0, idx, 0]].clamp(0.0, 1.0);
                let score = (cls_score * obj_score).sqrt();
                if score < YUNET_SCORE_THRESHOLD {
                    continue;
                }

                let cx = (c as f32 + bbox[[0, idx, 0]]) * stride as f32;
                let cy = (r as f32 + bbox[[0, idx, 1]]) * stride as f32;
                let w = bbox[[0, idx, 2]].exp() * stride as f32;
                let h = bbox[[0, idx, 3]].exp() * stride as f32;
                let x1 = cx - w / 2.0;
                let y1 = cy - h / 2.0;

                let mut landmarks = [(0.0f32, 0.0f32); 5];
                for (n, lm) in landmarks.iter_mut().enumerate() {
                    let lx = (kps[[0, idx, 2 * n]] + c as f32) * stride as f32;
                    let ly = (kps[[0, idx, 2 * n + 1]] + r as f32) * stride as f32;
                    *lm = (lx / YUNET_INPUT_SIZE as f32, ly / YUNET_INPUT_SIZE as f32);
                }

                candidates.push(DetectedFace {
                    bbox_x: x1 / YUNET_INPUT_SIZE as f32,
                    bbox_y: y1 / YUNET_INPUT_SIZE as f32,
                    bbox_w: w / YUNET_INPUT_SIZE as f32,
                    bbox_h: h / YUNET_INPUT_SIZE as f32,
                    landmarks,
                    score,
                });
            }
        }
    }

    Ok(non_max_suppress(candidates, YUNET_NMS_IOU_THRESHOLD))
}

fn iou(a: &DetectedFace, b: &DetectedFace) -> f32 {
    let ax2 = a.bbox_x + a.bbox_w;
    let ay2 = a.bbox_y + a.bbox_h;
    let bx2 = b.bbox_x + b.bbox_w;
    let by2 = b.bbox_y + b.bbox_h;

    let ix1 = a.bbox_x.max(b.bbox_x);
    let iy1 = a.bbox_y.max(b.bbox_y);
    let ix2 = ax2.min(bx2);
    let iy2 = ay2.min(by2);

    let iw = (ix2 - ix1).max(0.0);
    let ih = (iy2 - iy1).max(0.0);
    let intersection = iw * ih;
    let union = a.bbox_w * a.bbox_h + b.bbox_w * b.bbox_h - intersection;
    if union <= 0.0 {
        0.0
    } else {
        intersection / union
    }
}

/// Standard greedy NMS: highest score first, dropping any later candidate
/// that overlaps an already-kept one above `iou_threshold`.
fn non_max_suppress(mut candidates: Vec<DetectedFace>, iou_threshold: f32) -> Vec<DetectedFace> {
    candidates.sort_by(|a, b| b.score.partial_cmp(&a.score).expect("scores are never NaN"));
    let mut kept: Vec<DetectedFace> = Vec::new();
    for candidate in candidates {
        if !kept.iter().any(|k| iou(k, &candidate) > iou_threshold) {
            kept.push(candidate);
        }
    }
    kept
}

/// SFace's own canonical 112x112 alignment template (right eye, left eye,
/// nose tip, right mouth corner, left mouth corner) -- taken verbatim from
/// `FaceRecognizerSF::getSimilarityTransformMatrix`'s `dst` array.
const SFACE_TEMPLATE: [(f32, f32); 5] =
    [(38.2946, 51.6963), (73.5318, 51.5014), (56.0252, 71.7366), (41.5493, 92.3655), (70.7299, 92.2041)];

/// Least-squares similarity transform (uniform scale + rotation +
/// translation, no reflection) mapping `src` onto `dst`, both 5 point
/// pairs. This is the standard 2D specialization of Umeyama (1991) via a
/// single complex-number scale/rotation factor `z = sum(dst_d *
/// conj(src_d)) / sum(|src_d|^2)` -- algebraically equivalent to OpenCV's
/// own general N-dimensional SVD-based implementation
/// (`face_recognize.cpp`) for the non-degenerate case real face landmarks
/// always produce (5 points that are never collinear), but without
/// needing a general SVD: a single complex multiplier can only rotate and
/// scale, never reflect, so the "correct for a reflection" branch OpenCV's
/// general solver needs is structurally impossible to hit here.
/// Returns `[a, b, tx, c, d, ty]` mapping `(x, y) -> (a*x + b*y + tx, c*x
/// + d*y + ty)`.
fn similarity_transform(src: &[(f32, f32); 5], dst: &[(f32, f32); 5]) -> [f32; 6] {
    let src_mean = (src.iter().map(|p| p.0).sum::<f32>() / 5.0, src.iter().map(|p| p.1).sum::<f32>() / 5.0);
    let dst_mean = (dst.iter().map(|p| p.0).sum::<f32>() / 5.0, dst.iter().map(|p| p.1).sum::<f32>() / 5.0);

    let mut num_re = 0.0f32;
    let mut num_im = 0.0f32;
    let mut denom = 0.0f32;
    for i in 0..5 {
        let sx = src[i].0 - src_mean.0;
        let sy = src[i].1 - src_mean.1;
        let dx = dst[i].0 - dst_mean.0;
        let dy = dst[i].1 - dst_mean.1;
        // dst_d * conj(src_d) = (dx + i dy)(sx - i sy)
        num_re += dx * sx + dy * sy;
        num_im += dy * sx - dx * sy;
        denom += sx * sx + sy * sy;
    }

    let scale = num_re.hypot(num_im) / denom;
    let theta = num_im.atan2(num_re);
    let (sin_t, cos_t) = theta.sin_cos();

    let a = scale * cos_t;
    let b = -scale * sin_t;
    let c = scale * sin_t;
    let d = scale * cos_t;
    let tx = dst_mean.0 - (a * src_mean.0 + b * src_mean.1);
    let ty = dst_mean.1 - (c * src_mean.0 + d * src_mean.1);
    [a, b, tx, c, d, ty]
}

/// Inverts a `[a,b,tx,c,d,ty]` affine map (`(x,y) -> (ax+by+tx, cx+dy+ty)`).
fn invert_affine(m: [f32; 6]) -> [f32; 6] {
    let [a, b, tx, c, d, ty] = m;
    let det = a * d - b * c;
    let ia = d / det;
    let ib = -b / det;
    let ic = -c / det;
    let id = a / det;
    let itx = -(ia * tx + ib * ty);
    let ity = -(ic * tx + id * ty);
    [ia, ib, itx, ic, id, ity]
}

fn sample_bilinear(image: &RgbImage, x: f32, y: f32) -> [u8; 3] {
    let (w, h) = (image.width() as f32, image.height() as f32);
    let x = x.clamp(0.0, w - 1.001);
    let y = y.clamp(0.0, h - 1.001);
    let x0 = x.floor() as u32;
    let y0 = y.floor() as u32;
    let x1 = (x0 + 1).min(image.width() - 1);
    let y1 = (y0 + 1).min(image.height() - 1);
    let fx = x - x0 as f32;
    let fy = y - y0 as f32;

    let p00 = image.get_pixel(x0, y0);
    let p10 = image.get_pixel(x1, y0);
    let p01 = image.get_pixel(x0, y1);
    let p11 = image.get_pixel(x1, y1);

    let mut out = [0u8; 3];
    for ch in 0..3 {
        let top = p00[ch] as f32 * (1.0 - fx) + p10[ch] as f32 * fx;
        let bottom = p01[ch] as f32 * (1.0 - fx) + p11[ch] as f32 * fx;
        out[ch] = (top * (1.0 - fy) + bottom * fy).round() as u8;
    }
    out
}

/// Warps `image` so `landmarks` (in `image`'s own pixel coordinates) land
/// on `SFACE_TEMPLATE`, producing a 112x112 aligned crop -- inverse-mapped
/// pull-sampling with bilinear interpolation, the same shape
/// `panorama_merge.rs`'s own per-pixel warp step already uses in this
/// codebase.
fn align_face(image: &RgbImage, landmarks: &[(f32, f32); 5]) -> RgbImage {
    let forward = similarity_transform(landmarks, &SFACE_TEMPLATE);
    let [ia, ib, itx, ic, id, ity] = invert_affine(forward);

    let mut out = RgbImage::new(SFACE_INPUT_SIZE, SFACE_INPUT_SIZE);
    for dy in 0..SFACE_INPUT_SIZE {
        for dx in 0..SFACE_INPUT_SIZE {
            let (dxf, dyf) = (dx as f32, dy as f32);
            let sx = ia * dxf + ib * dyf + itx;
            let sy = ic * dxf + id * dyf + ity;
            let px = sample_bilinear(image, sx, sy);
            out.put_pixel(dx, dy, image::Rgb(px));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn similarity_transform_is_identity_when_src_equals_dst() {
        let pts = SFACE_TEMPLATE;
        let m = similarity_transform(&pts, &pts);
        let [a, b, tx, c, d, ty] = m;
        assert!((a - 1.0).abs() < 1e-4, "a={a}");
        assert!(b.abs() < 1e-4, "b={b}");
        assert!(c.abs() < 1e-4, "c={c}");
        assert!((d - 1.0).abs() < 1e-4, "d={d}");
        assert!(tx.abs() < 1e-3, "tx={tx}");
        assert!(ty.abs() < 1e-3, "ty={ty}");
    }

    #[test]
    fn similarity_transform_recovers_a_known_rotation_and_scale() {
        // dst = src rotated 30° about the origin and scaled by 2, then
        // translated by (10, -5) -- the transform must recover exactly
        // that scale/rotation/translation.
        let src: [(f32, f32); 5] = [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0), (0.5, 0.5)];
        let theta = std::f32::consts::PI / 6.0;
        let scale = 2.0f32;
        let (sin_t, cos_t) = theta.sin_cos();
        let dst: [(f32, f32); 5] = src.map(|(x, y)| {
            let rx = x * cos_t - y * sin_t;
            let ry = x * sin_t + y * cos_t;
            (scale * rx + 10.0, scale * ry - 5.0)
        });

        let [a, b, tx, c, d, ty] = similarity_transform(&src, &dst);
        assert!((a - scale * cos_t).abs() < 1e-3, "a={a}");
        assert!((b - (-scale * sin_t)).abs() < 1e-3, "b={b}");
        assert!((c - scale * sin_t).abs() < 1e-3, "c={c}");
        assert!((d - scale * cos_t).abs() < 1e-3, "d={d}");
        assert!((tx - 10.0).abs() < 1e-2, "tx={tx}");
        assert!((ty - (-5.0)).abs() < 1e-2, "ty={ty}");
    }

    #[test]
    fn invert_affine_composed_with_forward_is_identity() {
        let m = similarity_transform(
            &[(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0), (0.5, 0.5)],
            &SFACE_TEMPLATE,
        );
        let inv = invert_affine(m);
        // Apply forward then inverse to a handful of points; must land
        // back on the original point.
        for &(x, y) in &[(0.0, 0.0), (1.0, 0.0), (0.3, 0.7), (-2.0, 5.0)] {
            let fx = m[0] * x + m[1] * y + m[2];
            let fy = m[3] * x + m[4] * y + m[5];
            let bx = inv[0] * fx + inv[1] * fy + inv[2];
            let by = inv[3] * fx + inv[4] * fy + inv[5];
            assert!((bx - x).abs() < 1e-3, "x roundtrip: {bx} vs {x}");
            assert!((by - y).abs() < 1e-3, "y roundtrip: {by} vs {y}");
        }
    }

    #[test]
    fn iou_of_identical_boxes_is_one() {
        let a = DetectedFace { bbox_x: 0.1, bbox_y: 0.1, bbox_w: 0.2, bbox_h: 0.2, landmarks: [(0.0, 0.0); 5], score: 0.9 };
        assert!((iou(&a, &a) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn iou_of_disjoint_boxes_is_zero() {
        let a = DetectedFace { bbox_x: 0.0, bbox_y: 0.0, bbox_w: 0.1, bbox_h: 0.1, landmarks: [(0.0, 0.0); 5], score: 0.9 };
        let b = DetectedFace { bbox_x: 0.5, bbox_y: 0.5, bbox_w: 0.1, bbox_h: 0.1, landmarks: [(0.0, 0.0); 5], score: 0.8 };
        assert_eq!(iou(&a, &b), 0.0);
    }

    #[test]
    fn non_max_suppress_drops_a_heavily_overlapping_lower_score_box() {
        let keep = DetectedFace { bbox_x: 0.1, bbox_y: 0.1, bbox_w: 0.2, bbox_h: 0.2, landmarks: [(0.0, 0.0); 5], score: 0.95 };
        let dupe = DetectedFace { bbox_x: 0.11, bbox_y: 0.11, bbox_w: 0.2, bbox_h: 0.2, landmarks: [(0.0, 0.0); 5], score: 0.7 };
        let distinct = DetectedFace { bbox_x: 0.7, bbox_y: 0.7, bbox_w: 0.1, bbox_h: 0.1, landmarks: [(0.0, 0.0); 5], score: 0.6 };
        let result = non_max_suppress(vec![dupe, keep.clone(), distinct.clone()], 0.3);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], keep);
        assert_eq!(result[1], distinct);
    }

    #[test]
    fn sample_bilinear_at_a_pixel_center_matches_that_pixel() {
        let mut img = RgbImage::new(4, 4);
        img.put_pixel(2, 1, image::Rgb([200, 100, 50]));
        let px = sample_bilinear(&img, 2.0, 1.0);
        assert_eq!(px, [200, 100, 50]);
    }

    #[test]
    fn sample_bilinear_averages_two_horizontally_adjacent_pixels() {
        let mut img = RgbImage::new(4, 4);
        img.put_pixel(0, 0, image::Rgb([0, 0, 0]));
        img.put_pixel(1, 0, image::Rgb([200, 200, 200]));
        let px = sample_bilinear(&img, 0.5, 0.0);
        assert_eq!(px, [100, 100, 100]);
    }

    // --- Real-model tests, gated behind an env var pointing at a local
    // models directory, same pattern as EMULSION_TEST_HDR_BRACKET_DIR
    // (RFC-0005 §4). Populate it via face_models::ensure_models() once,
    // then point EMULSION_TEST_FACE_MODELS_DIR at that directory:
    //
    //   cargo test --lib -- --ignored face_detect
    //
    // Not `#[ignore]`d on the model-loading step alone since these also
    // need a real face photo -- test_image/Smiling-woman-pink-shirt-portrait.jpg,
    // already committed for other tests, same "already-established real
    // face photo fixture" precedent as develop_engine.rs's
    // Red-eye-flash.jpeg test.
    fn models_dir() -> Option<std::path::PathBuf> {
        std::env::var("EMULSION_TEST_FACE_MODELS_DIR").ok().map(std::path::PathBuf::from)
    }

    #[test]
    fn detects_a_real_face_in_a_real_photo() {
        let Some(dir) = models_dir() else {
            eprintln!("skipping: EMULSION_TEST_FACE_MODELS_DIR not set");
            return;
        };
        let detector = FaceDetector::load(&dir.join("yunet.onnx")).unwrap();
        let img = image::open("../../test_image/Smiling-woman-pink-shirt-portrait.jpg").unwrap().to_rgb8();
        let faces = detector.detect(&img).unwrap();
        assert!(!faces.is_empty(), "expected at least one face in a real portrait photo");
        let best = &faces[0];
        assert!(best.score > YUNET_SCORE_THRESHOLD);
        // A portrait's face should be a real, roughly-square-ish region,
        // not a sliver or the whole frame -- loose sanity bounds, not a
        // tight golden-value assertion (real model output, not hand-computed).
        assert!(best.bbox_w > 0.05 && best.bbox_w < 0.95, "bbox_w={}", best.bbox_w);
        assert!(best.bbox_h > 0.05 && best.bbox_h < 0.95, "bbox_h={}", best.bbox_h);
    }

    #[test]
    fn embeds_a_detected_face_into_a_128_dim_vector() {
        let Some(dir) = models_dir() else {
            eprintln!("skipping: EMULSION_TEST_FACE_MODELS_DIR not set");
            return;
        };
        let detector = FaceDetector::load(&dir.join("yunet.onnx")).unwrap();
        let embedder = FaceEmbedder::load(&dir.join("sface.onnx")).unwrap();
        let img = image::open("../../test_image/Smiling-woman-pink-shirt-portrait.jpg").unwrap().to_rgb8();
        let faces = detector.detect(&img).unwrap();
        assert!(!faces.is_empty());
        let landmarks_px = faces[0].landmarks.map(|(x, y)| (x * img.width() as f32, y * img.height() as f32));
        let embedding = embedder.embed(&img, &landmarks_px).unwrap();
        assert_eq!(embedding.len(), 128);
        let norm: f32 = embedding.iter().map(|v| v * v).sum::<f32>().sqrt();
        assert!(norm > 0.0, "embedding must not be all-zero");
    }

    #[test]
    fn embedding_of_the_same_face_is_much_closer_to_itself_than_to_a_different_photo() {
        let Some(dir) = models_dir() else {
            eprintln!("skipping: EMULSION_TEST_FACE_MODELS_DIR not set");
            return;
        };
        let detector = FaceDetector::load(&dir.join("yunet.onnx")).unwrap();
        let embedder = FaceEmbedder::load(&dir.join("sface.onnx")).unwrap();

        let embed_best_face = |path: &str| -> Vec<f32> {
            let img = image::open(path).unwrap().to_rgb8();
            let faces = detector.detect(&img).unwrap();
            let landmarks_px = faces[0].landmarks.map(|(x, y)| (x * img.width() as f32, y * img.height() as f32));
            embedder.embed(&img, &landmarks_px).unwrap()
        };

        let a1 = embed_best_face("../../test_image/Smiling-woman-pink-shirt-portrait.jpg");
        let a2 = embed_best_face("../../test_image/Smiling-woman-pink-shirt-portrait.jpg");
        let b = embed_best_face("../../test_image/Red-eye-flash.jpeg");

        let dist = |x: &[f32], y: &[f32]| -> f32 {
            let dot: f32 = x.iter().zip(y).map(|(a, b)| a * b).sum();
            let na = x.iter().map(|v| v * v).sum::<f32>().sqrt();
            let nb = y.iter().map(|v| v * v).sum::<f32>().sqrt();
            1.0 - dot / (na * nb)
        };

        let same_photo_distance = dist(&a1, &a2);
        let different_photo_distance = dist(&a1, &b);
        assert!(same_photo_distance < 1e-4, "identical input must embed identically: {same_photo_distance}");
        assert!(
            different_photo_distance > same_photo_distance + 0.1,
            "a different person's face must embed further away: same={same_photo_distance} different={different_photo_distance}"
        );
    }
}
