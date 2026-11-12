//! Blink evidence for one photograph (RFC-0031 §8.14, §8.15): find its faces, cut each one the way the mesh model wants it, read the
//! eyelids, and say per face whether the eyes are open, closed, one closed, or **cannot be told**. Pure pixels and models, no catalog:
//! `blink_commands.rs` owns where the faces come from and where the results go.
//!
//! The crop is the one `blink_measure.py` uses (and the goldens were made with): a square 1.5 x the detector box's longer side, centred
//! on the box, turned upright by the eye line, 256 px. The detector sees the photograph reduced to its own 640 px input, the crop is cut
//! from the full-resolution decode, so an eyelid has all the pixels the photograph has.

use crate::blink_mesh::{EyeMesh, BlinkMeshError, CROP_SCALE, CROP_SIDE};
use crate::blink_verdict::{face_eyes, FaceEyes};
use crate::face_detect::{FaceDetectError, FaceDetector};
use image::{Rgb, RgbImage};
use serde::{Deserialize, Serialize};

/// Version of the rules (ratio cut, face floor, presence floor, crop). Rows of an older version are measured again.
pub const ALGO_VERSION: i64 = 1;

#[derive(Debug, thiserror::Error)]
pub enum BlinkPassError {
    #[error(transparent)]
    Detect(#[from] FaceDetectError),
    #[error(transparent)]
    Mesh(#[from] BlinkMeshError),
}

/// What is said of one face; the box is in fractions of the photograph, like the `faces` table's.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FaceBlink {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub eyes: EyesWord,
}

/// `FaceEyes` as it is stored and sent to the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EyesWord {
    Open,
    Closed,
    OneClosed,
    CannotTell,
}

impl From<FaceEyes> for EyesWord {
    fn from(e: FaceEyes) -> Self {
        match e {
            FaceEyes::Open => EyesWord::Open,
            FaceEyes::Closed => EyesWord::Closed,
            FaceEyes::OneClosed => EyesWord::OneClosed,
            FaceEyes::CannotTell => EyesWord::CannotTell,
        }
    }
}

/// Counts for a photograph's badge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
pub struct BlinkCounts {
    pub faces: u32,
    /// Faces the gate let through (eyes told apart).
    pub judged: u32,
    pub closed: u32,
    pub one_closed: u32,
}

pub fn counts(faces: &[FaceBlink]) -> BlinkCounts {
    let mut c = BlinkCounts { faces: faces.len() as u32, ..Default::default() };
    for f in faces {
        match f.eyes {
            EyesWord::CannotTell => {}
            EyesWord::Open => c.judged += 1,
            EyesWord::Closed => {
                c.judged += 1;
                c.closed += 1;
            }
            EyesWord::OneClosed => {
                c.judged += 1;
                c.one_closed += 1;
            }
        }
    }
    c
}

/// A `size` x `size` crop of the `side` x `side` square centred (`cx`, `cy`) and turned by `angle` radians, bilinear, black outside the
/// photograph: the same map as OpenCV's `warpAffine` with `WARP_INVERSE_MAP` in `blink_measure.py`.
pub fn rotated_crop(img: &RgbImage, cx: f32, cy: f32, side: f32, angle: f32, size: u32) -> RgbImage {
    let (c, s) = (angle.cos(), angle.sin());
    let k = side / size as f32;
    let (ox, oy) = (cx - 0.5 * (c * side - s * side), cy - 0.5 * (s * side + c * side));
    let (w, h) = (img.width() as i64, img.height() as i64);
    let at = |x: i64, y: i64| -> [f32; 3] {
        if x < 0 || y < 0 || x >= w || y >= h {
            [0.0; 3]
        } else {
            let p = img.get_pixel(x as u32, y as u32);
            [p[0] as f32, p[1] as f32, p[2] as f32]
        }
    };
    RgbImage::from_fn(size, size, |u, v| {
        let (u, v) = (u as f32, v as f32);
        let (sx, sy) = (c * k * u - s * k * v + ox, s * k * u + c * k * v + oy);
        let (x0, y0) = (sx.floor(), sy.floor());
        let (fx, fy) = (sx - x0, sy - y0);
        let (x0, y0) = (x0 as i64, y0 as i64);
        let (p00, p10, p01, p11) = (at(x0, y0), at(x0 + 1, y0), at(x0, y0 + 1), at(x0 + 1, y0 + 1));
        let mut out = [0u8; 3];
        for ch in 0..3 {
            let top = p00[ch] * (1.0 - fx) + p10[ch] * fx;
            let bottom = p01[ch] * (1.0 - fx) + p11[ch] * fx;
            out[ch] = (top * (1.0 - fy) + bottom * fy).round().clamp(0.0, 255.0) as u8;
        }
        Rgb(out)
    })
}

/// Every face the detector finds in `image`, each with its eyes' verdict.
pub fn analyse_image(detector: &FaceDetector, mesh: &EyeMesh, image: &RgbImage) -> Result<Vec<FaceBlink>, BlinkPassError> {
    let (iw, ih) = (image.width() as f32, image.height() as f32);
    let mut out = Vec::new();
    for face in detector.detect(image)? {
        let (bx, by, bw, bh) = (face.bbox_x * iw, face.bbox_y * ih, face.bbox_w * iw, face.bbox_h * ih);
        // landmarks[0] is the person's right eye (picture left), landmarks[1] the left eye
        let (re, le) = ((face.landmarks[0].0 * iw, face.landmarks[0].1 * ih), (face.landmarks[1].0 * iw, face.landmarks[1].1 * ih));
        let angle = (le.1 - re.1).atan2(le.0 - re.0);
        let side = bw.max(bh) * CROP_SCALE;
        let (cx, cy) = (bx + bw / 2.0, by + bh / 2.0);
        let crop = rotated_crop(image, cx, cy, side, angle, CROP_SIDE);
        let reading = mesh.read(&crop, cx, cy, side, angle)?;
        out.push(FaceBlink { x: face.bbox_x, y: face.bbox_y, w: face.bbox_w, h: face.bbox_h, eyes: face_eyes(&reading, bw).into() });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn face(eyes: EyesWord) -> FaceBlink {
        FaceBlink { x: 0.0, y: 0.0, w: 0.1, h: 0.1, eyes }
    }

    #[test]
    fn the_counts_only_judge_faces_the_gate_let_through() {
        let c = counts(&[face(EyesWord::Open), face(EyesWord::Closed), face(EyesWord::OneClosed), face(EyesWord::CannotTell), face(EyesWord::CannotTell)]);
        assert_eq!(c, BlinkCounts { faces: 5, judged: 3, closed: 1, one_closed: 1 });
        assert_eq!(counts(&[]), BlinkCounts::default());
    }

    #[test]
    fn an_upright_crop_of_the_whole_picture_is_the_picture() {
        let img = RgbImage::from_fn(64, 64, |x, y| Rgb([(x * 4) as u8, (y * 4) as u8, 7]));
        let crop = rotated_crop(&img, 32.0, 32.0, 64.0, 0.0, 64);
        assert_eq!(crop, img);
    }

    #[test]
    fn a_crop_outside_the_picture_is_black() {
        let img = RgbImage::from_pixel(64, 64, Rgb([200, 100, 50]));
        let off = rotated_crop(&img, -500.0, -500.0, 64.0, 0.0, 16);
        assert!(off.pixels().all(|p| p[0] == 0 && p[1] == 0 && p[2] == 0));
    }

    /// OpenCV's `warpAffine` (the Python tool's crop) on a real face crop, turned 0.35 rad and cut small: the same picture to within
    /// bilinear rounding.
    #[test]
    fn a_turned_crop_matches_opencv() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/blink");
        let src = image::open(dir.join("case0_face.png")).unwrap().to_rgb8();
        let expected = image::open(dir.join("rot_expected.png")).unwrap().to_rgb8();
        let got = rotated_crop(&src, 120.0, 140.0, 150.0, 0.35, 64);
        let worst = got.pixels().zip(expected.pixels()).flat_map(|(a, b)| (0..3).map(move |c| (a[c] as i32 - b[c] as i32).abs())).max().unwrap();
        assert!(worst <= 2, "largest channel difference from OpenCV: {worst}");
    }

    /// The whole pass on a real portrait, against the Python tool: one face, in the same place, and the same verdict (the left eye of this
    /// smiling woman reads 0.165 and the right 0.233, so one eye counts as closed). Needs the YuNet file (env YUNET, or the one the app
    /// caches) and the mesh model; skipped when either is missing.
    #[test]
    fn a_real_portrait_gets_the_python_tools_verdict() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let yunet = std::env::var("YUNET").map(std::path::PathBuf::from).unwrap_or_else(|_| {
            std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default()).join("Library/Application Support/dev.alfredwei.emulsion/face_models/face_detection_yunet_2023mar.onnx")
        });
        let mesh_file = root.join("../../models/face_landmarks.onnx");
        let photo = root.join("../../test_image/Smiling-woman-pink-shirt-portrait.jpg");
        if !yunet.exists() || !mesh_file.exists() || !photo.exists() {
            eprintln!("skipped: needs {} and the mesh model", yunet.display());
            return;
        }
        let (detector, mesh) = (FaceDetector::load(&yunet).unwrap(), EyeMesh::load(&mesh_file).unwrap());
        let image = image::open(&photo).unwrap().to_rgb8();
        let t = std::time::Instant::now();
        let faces = analyse_image(&detector, &mesh, &image).unwrap();
        eprintln!("eye check of one {} x {} photograph with 1 face: {:?} (this build's profile)", image.width(), image.height(), t.elapsed());
        assert_eq!(faces.len(), 1, "{faces:?}");
        let f = &faces[0];
        for (got, want) in [(f.x, 0.403), (f.y, 0.113), (f.w, 0.359), (f.h, 0.291)] {
            // the app's detector sees the picture stretched to 640 x 640, OpenCV's at its own shape: boxes differ by a few percent
            assert!((got - want).abs() < 0.06, "box {f:?}");
        }
        assert_eq!(f.eyes, EyesWord::OneClosed, "{f:?}");
    }

    #[test]
    fn the_words_round_trip_through_json() {
        let f = face(EyesWord::OneClosed);
        let s = serde_json::to_string(&f).unwrap();
        assert!(s.contains("\"one_closed\""));
        assert_eq!(serde_json::from_str::<FaceBlink>(&s).unwrap(), f);
    }
}
