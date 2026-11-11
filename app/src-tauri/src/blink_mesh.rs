//! Eye-state evidence from MediaPipe Face Mesh V2 (RFC-0031 §8.14): eyelid geometry (the eye aspect ratio) per eye, and the mesh's own
//! face-presence score, for one face. The Blendshapes model is NOT used: `tract` cannot translate it (its ReduceMean node fails to load),
//! and on the user's marks the eye aspect ratio alone found as many closed eyes at 96 px and up as ratio-or-blendshape (19 of 23 against
//! 21 of 23, inside the noise of a sample that size). Pure pixels in, numbers out, over `tract`-loaded ONNX models like `face_detect.rs`;
//! no catalog or Tauri dependency. The decision rules (thresholds, the "cannot tell" gate) live in `blink_verdict.rs`, not here.
//!
//! The maths is ported from `docs/rfc/RFC-0031-appendix/blink_measure.py`, which the unit test checks it against (goldens made by that script).
//! Left / right below mean the side of the PICTURE (a person's right eye is on the image left, and the mesh's "right eye" indices are the
//! subject's right).

use image::RgbImage;
use std::path::Path;
use tract_onnx::prelude::*;

#[derive(Debug, thiserror::Error)]
pub enum BlinkMeshError {
    #[error("could not load model {0}: {1}")]
    LoadModel(String, String),
    #[error("inference over {0} failed: {1}")]
    Inference(&'static str, String),
    #[error("the face crop must be {0} x {0} pixels, got {1} x {2}")]
    CropSize(u32, u32, u32),
}

type RunnablePlan = std::sync::Arc<TypedRunnableModel>;

/// The mesh model input side, and how much larger than the detector box the square it wants is.
pub const CROP_SIDE: u32 = 256;
pub const CROP_SCALE: f32 = 1.5; // the crop is a square this multiple of the detector box's longer side
/// Mesh points: 478, each x, y, z in crop pixels.
const MESH_POINTS: usize = 478;
const EAR_IMAGE_LEFT: [usize; 6] = [33, 160, 158, 133, 153, 144];
const EAR_IMAGE_RIGHT: [usize; 6] = [362, 385, 387, 263, 373, 380];

/// What the mesh says about one face; "left" and "right" are the picture's sides.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EyeMeshReading {
    /// 0 to 1: how sure the mesh model is that the crop holds a face.
    pub presence: f32,
    pub ear_left: f32,
    pub ear_right: f32,
}

pub struct EyeMesh {
    landmarks: RunnablePlan,
}

fn load_plan(path: &Path, shape: &[usize]) -> Result<RunnablePlan, BlinkMeshError> {
    let err = |e: TractError| BlinkMeshError::LoadModel(path.display().to_string(), e.to_string());
    tract_onnx::onnx()
        .model_for_path(path)
        .and_then(|m| m.with_input_fact(0, InferenceFact::dt_shape(f32::datum_type(), shape)))
        .and_then(|m| m.into_optimized())
        .and_then(|m| m.into_runnable())
        .map_err(err)
}

fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}

impl EyeMesh {
    pub fn load(landmarks: &Path) -> Result<Self, BlinkMeshError> {
        Ok(Self { landmarks: load_plan(landmarks, &[1, CROP_SIDE as usize, CROP_SIDE as usize, 3])? })
    }

    /// `crop` is the face as a `CROP_SIDE` square cut from the photograph, upright by the eye line: its centre is (`cx`, `cy`) in
    /// the photograph, its side `side` photograph pixels, and `angle` (radians) the rotation of the eye line, so the mesh points can be
    /// put back into photograph pixels (an eye aspect ratio does not depend on that, but a later use of the points might).
    pub fn read(&self, crop: &RgbImage, cx: f32, cy: f32, side: f32, angle: f32) -> Result<EyeMeshReading, BlinkMeshError> {
        if crop.width() != CROP_SIDE || crop.height() != CROP_SIDE {
            return Err(BlinkMeshError::CropSize(CROP_SIDE, crop.width(), crop.height()));
        }
        let inf = |e: TractError| BlinkMeshError::Inference("face mesh", e.to_string());
        let mut input = Tensor::zero::<f32>(&[1, CROP_SIDE as usize, CROP_SIDE as usize, 3]).map_err(inf)?;
        {
            let mut view = input.to_plain_array_view_mut::<f32>().map_err(inf)?;
            for (x, y, p) in crop.enumerate_pixels() {
                for c in 0..3 {
                    view[[0, y as usize, x as usize, c]] = p[c] as f32 / 255.0;
                }
            }
        }
        let out = self.landmarks.run(tvec!(input.into())).map_err(inf)?;
        let raw = out[0].to_plain_array_view::<f32>().map_err(inf)?;
        let raw: Vec<f32> = raw.iter().copied().collect();
        if raw.len() < MESH_POINTS * 3 {
            return Err(BlinkMeshError::Inference("face mesh", format!("expected {} values, got {}", MESH_POINTS * 3, raw.len())));
        }
        let presence = sigmoid(*out[1].to_plain_array_view::<f32>().map_err(inf)?.iter().next().unwrap_or(&f32::NEG_INFINITY));
        let points = mesh_points_in_photograph(&raw, cx, cy, side, angle);

        Ok(EyeMeshReading {
            presence,
            ear_left: eye_aspect_ratio(&points, &EAR_IMAGE_LEFT),
            ear_right: eye_aspect_ratio(&points, &EAR_IMAGE_RIGHT),
        })
    }
}

/// The mesh's points (x, y, z in crop pixels, flattened) as photograph pixels: undo the crop's centre, rotation and scale.
fn mesh_points_in_photograph(raw: &[f32], cx: f32, cy: f32, side: f32, angle: f32) -> Vec<(f32, f32)> {
    let (c, s) = (angle.cos(), angle.sin());
    (0..MESH_POINTS)
        .map(|i| {
            let (dx, dy) = ((raw[i * 3] / CROP_SIDE as f32 - 0.5) * side, (raw[i * 3 + 1] / CROP_SIDE as f32 - 0.5) * side);
            (cx + dx * c - dy * s, cy + dx * s + dy * c)
        })
        .collect()
}

/// Eye aspect ratio from six points (corner, two upper lid, corner, two lower lid): lid opening over eye width; about 0.25 open, 0.1 closed.
fn eye_aspect_ratio(points: &[(f32, f32)], ix: &[usize; 6]) -> f32 {
    let d = |a: usize, b: usize| ((points[ix[a]].0 - points[ix[b]].0).powi(2) + (points[ix[a]].1 - points[ix[b]].1).powi(2)).sqrt();
    (d(1, 5) + d(2, 4)) / (2.0 * d(0, 3) + 1e-9)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ear_of_a_flat_eye_is_zero_and_of_a_round_one_is_large() {
        let mut pts = vec![(0.0f32, 0.0f32); 478];
        let ix = EAR_IMAGE_LEFT;
        pts[ix[0]] = (0.0, 0.0);
        pts[ix[3]] = (10.0, 0.0);
        for (a, b, x) in [(1, 5, 3.0), (2, 4, 7.0)] {
            pts[ix[a]] = (x, 0.0);
            pts[ix[b]] = (x, 0.0);
        }
        assert!(eye_aspect_ratio(&pts, &ix) < 1e-6);
        for (a, b, x) in [(1, 5, 3.0), (2, 4, 7.0)] {
            pts[ix[a]] = (x, -3.0);
            pts[ix[b]] = (x, 3.0);
        }
        assert!((eye_aspect_ratio(&pts, &ix) - 0.6).abs() < 1e-5);
    }

    #[test]
    fn mesh_points_put_the_crop_centre_at_the_face_centre_and_rotate_with_the_eye_line() {
        let mut raw = vec![0.0f32; MESH_POINTS * 3];
        raw[0] = 128.0;
        raw[1] = 128.0; // the crop's centre
        raw[3] = 192.0;
        raw[4] = 128.0; // a quarter of the side to the right of centre
        let upright = mesh_points_in_photograph(&raw, 500.0, 400.0, 200.0, 0.0);
        assert_eq!(upright[0], (500.0, 400.0));
        assert!((upright[1].0 - 550.0).abs() < 1e-3 && (upright[1].1 - 400.0).abs() < 1e-3);
        let turned = mesh_points_in_photograph(&raw, 500.0, 400.0, 200.0, std::f32::consts::FRAC_PI_2);
        assert!((turned[1].0 - 500.0).abs() < 1e-3 && (turned[1].1 - 450.0).abs() < 1e-3);
    }

    /// Needs the mesh model (`models/face_landmarks.onnx`, or the folder in BLINK_MODELS) and the
    /// goldens made by `blink_measure.py`: the same face crops through this code must give the same readings.
    #[test]
    fn matches_the_python_tool_on_two_real_faces() {
        let dir = std::env::var("BLINK_MODELS").map(std::path::PathBuf::from).unwrap_or_else(|_| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../models"));
        let lm = dir.join("face_landmarks.onnx");
        if !lm.exists() {
            eprintln!("skipped: the mesh model is not in {}", dir.display());
            return;
        }
        let mesh = EyeMesh::load(&lm).expect("tract loads the mesh model");
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/blink");
        let golden: serde_json::Value = serde_json::from_slice(&std::fs::read(fixtures.join("golden.json")).unwrap()).unwrap();
        for case in golden.as_array().unwrap() {
            let name = case["name"].as_str().unwrap();
            let crop = image::open(fixtures.join(format!("{name}_face.png"))).unwrap().to_rgb8();
            let f = |k: &str| case[k].as_f64().unwrap() as f32;
            let got = mesh.read(&crop, f("cx"), f("cy"), f("box_side"), f("ang")).unwrap();
            assert!((got.presence - f("mesh_presence")).abs() < 0.02, "{name}: presence {} vs {}", got.presence, f("mesh_presence"));
            assert!((got.ear_left - f("ear_left")).abs() < 0.01, "{name}: ear left {} vs {}", got.ear_left, f("ear_left"));
            assert!((got.ear_right - f("ear_right")).abs() < 0.01, "{name}: ear right {} vs {}", got.ear_right, f("ear_right"));
        }
    }
}
