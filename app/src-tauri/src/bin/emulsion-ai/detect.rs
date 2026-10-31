//! Person detection, run inside the `emulsion-ai` helper process (RFC-0037): D-FINE medium (Apache-2.0, COCO) on the ONNX
//! Runtime CPU provider. It finds the people the face detector cannot (seen from behind, in silhouette, too small a face);
//! each box then goes to the click-select model as a box prompt.
//!
//! The model takes the image **squashed** to 640 x 640, RGB scaled to 0..1 with no mean / std (the Hugging Face processor's
//! `do_normalize: false`), and answers 300 queries: `logits` (1 x 300 x 80, COCO classes, person = 0) and `pred_boxes`
//! (1 x 300 x 4, centre x, centre y, width, height, normalized to the frame). A box's score is the sigmoid of its person
//! logit; the model needs no non-maximum suppression. Everything that is not the model call is a pure function, unit-tested
//! without weights; the model path has an `#[ignore]`d test that needs the real file (`DETECT_MODEL`).

use crate::protocol::PersonBox;
use ort::session::{builder::GraphOptimizationLevel, Session};
use ort::value::TensorRef;
use std::path::Path;
use std::sync::Mutex;

/// Side of the model's square input.
pub const INPUT_SIZE: usize = 640;
/// Queries the model answers with, and COCO classes per query.
const QUERIES: usize = 300;
const CLASSES: usize = 80;
const PERSON: usize = 0;

#[derive(Debug, thiserror::Error)]
pub enum DetectError {
    #[error("inference failed: {0}")]
    Ort(String),
    #[error("unexpected model output: {0}")]
    BadOutput(String),
    #[error("detector state is poisoned")]
    Poisoned,
}

impl<T> From<ort::Error<T>> for DetectError {
    fn from(e: ort::Error<T>) -> Self {
        DetectError::Ort(e.to_string())
    }
}

/// Squashes `img` to the model's square and returns CHW f32 data in 0..1.
pub fn preprocess(img: &image::RgbImage) -> Vec<f32> {
    let resized = image::imageops::resize(img, INPUT_SIZE as u32, INPUT_SIZE as u32, image::imageops::FilterType::Triangle);
    let plane = INPUT_SIZE * INPUT_SIZE;
    let mut out = vec![0f32; 3 * plane];
    for (i, px) in resized.pixels().enumerate() {
        for c in 0..3 {
            out[c * plane + i] = px.0[c] as f32 / 255.0;
        }
    }
    out
}

/// The people among the model's queries: score at least `min_score`, corners clamped to the frame, best first.
/// `logits` is `QUERIES x CLASSES`, `boxes` is `QUERIES x 4` (centre x, centre y, width, height).
pub fn people_from(logits: &[f32], boxes: &[f32], min_score: f32) -> Vec<PersonBox> {
    let mut out: Vec<PersonBox> = (0..QUERIES)
        .filter_map(|q| {
            let score = 1.0 / (1.0 + (-logits[q * CLASSES + PERSON]).exp());
            if score < min_score {
                return None;
            }
            let b = &boxes[q * 4..q * 4 + 4];
            let (x0, y0) = ((b[0] - b[2] / 2.0).clamp(0.0, 1.0), (b[1] - b[3] / 2.0).clamp(0.0, 1.0));
            let (x1, y1) = ((b[0] + b[2] / 2.0).clamp(0.0, 1.0), (b[1] + b[3] / 2.0).clamp(0.0, 1.0));
            (x1 > x0 && y1 > y0).then_some(PersonBox { x0, y0, x1, y1, score })
        })
        .collect();
    out.sort_by(|a, b| b.score.total_cmp(&a.score));
    out
}

struct Loaded {
    session: Session,
    path: std::path::PathBuf,
}

/// The lazy session, created on first use and dropped on `release()`.
#[derive(Default)]
pub struct Detector {
    loaded: Mutex<Option<Loaded>>,
}

impl Detector {
    /// Blocking and CPU-heavy (about a second on a laptop CPU): the helper serves one request at a time.
    pub fn detect(&self, model: &Path, img: &image::RgbImage, min_score: f32) -> Result<Vec<PersonBox>, DetectError> {
        let input = preprocess(img);
        let mut guard = self.loaded.lock().map_err(|_| DetectError::Poisoned)?;
        if guard.as_ref().is_none_or(|l| l.path != model) {
            // Same session settings as the segmenter (RFC-0026 §7): memory-pattern planning off.
            let session = Session::builder()?
                .with_optimization_level(GraphOptimizationLevel::Level3)?
                .with_memory_pattern(false)?
                .commit_from_file(model)?;
            *guard = Some(Loaded { session, path: model.to_path_buf() });
        }
        let loaded = guard.as_mut().expect("just filled");
        let outputs = loaded.session.run(ort::inputs!["pixel_values" => TensorRef::from_array_view(([1usize, 3, INPUT_SIZE, INPUT_SIZE], &input[..]))?])?;
        let (logit_shape, logits) = outputs[0].try_extract_tensor::<f32>()?;
        let (box_shape, boxes) = outputs[1].try_extract_tensor::<f32>()?;
        if logits.len() != QUERIES * CLASSES || boxes.len() != QUERIES * 4 {
            return Err(DetectError::BadOutput(format!("logits have shape {logit_shape:?} and boxes {box_shape:?}, expected 1x{QUERIES}x{CLASSES} and 1x{QUERIES}x4")));
        }
        Ok(people_from(logits, boxes, min_score))
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

    fn queries(rows: &[(f32, [f32; 4])]) -> (Vec<f32>, Vec<f32>) {
        let mut logits = vec![-10.0f32; QUERIES * CLASSES];
        let mut boxes = vec![0.5f32, 0.5, 0.0, 0.0].repeat(QUERIES);
        for (q, (person_logit, b)) in rows.iter().enumerate() {
            logits[q * CLASSES + PERSON] = *person_logit;
            boxes[q * 4..q * 4 + 4].copy_from_slice(b);
        }
        (logits, boxes)
    }

    #[test]
    fn preprocess_squashes_to_the_model_square_and_scales_to_zero_one_in_chw_order() {
        let img = image::RgbImage::from_pixel(30, 10, image::Rgb([255, 0, 51]));
        let data = preprocess(&img);
        let plane = INPUT_SIZE * INPUT_SIZE;
        assert_eq!(data.len(), 3 * plane);
        assert!((data[0] - 1.0).abs() < 1e-6 && data[plane].abs() < 1e-6 && (data[2 * plane] - 0.2).abs() < 1e-6);
    }

    #[test]
    fn only_person_queries_over_the_score_come_back_best_first_with_corners_not_centres() {
        // sigmoid(2.0) = 0.88, sigmoid(0.0) = 0.5, sigmoid(-3.0) = 0.047
        let (logits, boxes) = queries(&[(0.0, [0.5, 0.5, 0.2, 0.4]), (2.0, [0.3, 0.6, 0.1, 0.2]), (-3.0, [0.9, 0.9, 0.1, 0.1])]);
        let people = people_from(&logits, &boxes, 0.4);
        assert_eq!(people.len(), 2, "the weak query and the empty ones are dropped");
        assert!(people[0].score > people[1].score);
        assert!((people[0].x0 - 0.25).abs() < 1e-6 && (people[0].y0 - 0.5).abs() < 1e-6 && (people[0].x1 - 0.35).abs() < 1e-6 && (people[0].y1 - 0.7).abs() < 1e-6);
    }

    #[test]
    fn a_class_other_than_person_never_counts_and_a_box_off_the_frame_is_clamped() {
        let (mut logits, boxes) = queries(&[(-10.0, [0.5, 0.5, 0.2, 0.2]), (5.0, [1.0, 0.05, 0.4, 0.2])]);
        logits[1] = 9.0; // query 0 is very sure of class 1 (a bicycle), not of a person
        let people = people_from(&logits, &boxes, 0.4);
        assert_eq!(people.len(), 1);
        let p = people[0];
        assert!((p.x1 - 1.0).abs() < 1e-6 && p.y0.abs() < 1e-6, "{p:?}");
    }

    #[test]
    fn a_degenerate_box_is_dropped() {
        let (logits, boxes) = queries(&[(8.0, [0.5, 0.5, 0.0, 0.3])]);
        assert!(people_from(&logits, &boxes, 0.4).is_empty());
    }

    /// Needs the real file: `DETECT_MODEL=<dfine_medium_coco.onnx> DETECT_IMAGE=<photo> cargo test --bin emulsion-ai -- --ignored detector_model`.
    #[test]
    #[ignore]
    fn detector_model_finds_people_in_a_photograph() {
        let model = std::path::PathBuf::from(std::env::var("DETECT_MODEL").expect("DETECT_MODEL"));
        let img = image::open(std::env::var("DETECT_IMAGE").expect("DETECT_IMAGE")).unwrap().to_rgb8();
        let people = Detector::default().detect(&model, &img, 0.4).unwrap();
        eprintln!("{people:?}");
        assert!(!people.is_empty());
    }
}
