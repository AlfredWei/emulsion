//! Region resolvers for adaptive presets (RFC-0035 §3.2, slice B1 of RFC-0033).
//!
//! A resolver looks at one photograph and returns `found`, `check` or `not_found` with the share of the
//! frame it covers and a reason, plus a ready-to-store mask op (no adjustments: the apply step adds those).
//! Pure functions over pixels and stored face boxes; the catalog and preview plumbing is in
//! `adaptive_commands.rs`. Every number below is a *starting value* from RFC-0033 §9-§10's small by-eye sets
//! and is judged again on photographs in B1 (RFC-0035 §4); the outcome words are the contract.

use crate::ai::protocol::{BBox, PersonBox};
use base64::Engine;
use image::{imageops::FilterType, GrayImage, RgbImage};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::VecDeque;

/// Side of a `segment_mask`'s stored raster (`develop_engine::masks::SEGMENT_SIZE`, RFC-0026: a PNG of any
/// other size is rejected there).
pub const RASTER: usize = 256;
/// A stored byte `0..=255` is the logit `q / 255 * 8 - 4` (same scale as `masks.rs` and `segmentRaster.js`).
const LOGIT_RANGE: f32 = 4.0;
/// Rec. 709 weights on the encoded values, exactly what the luminance-range mask's weight uses.
const LUMA: [f32; 3] = [0.2126, 0.7152, 0.0722];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Found,
    /// A mask exists but the caller should flag it for a look (RFC-0033 §3.1b).
    Check,
    NotFound,
}

#[derive(Debug, Clone, Serialize)]
pub struct RegionResult {
    pub outcome: Outcome,
    /// Share of the frame the region covers, 0..1 (0 when not found).
    pub coverage: f32,
    pub reason: Option<String>,
    /// A mask op to store, without adjustments (`segment_mask` or `luminance_range_mask`); `None` when not found.
    pub mask: Option<Value>,
}

impl RegionResult {
    fn not_found(reason: impl Into<String>) -> Self {
        RegionResult { outcome: Outcome::NotFound, coverage: 0.0, reason: Some(reason.into()), mask: None }
    }
    fn with_mask(outcome: Outcome, coverage: f32, reason: Option<String>, mask: Value) -> Self {
        RegionResult { outcome, coverage, reason, mask: Some(mask) }
    }
}

fn pct(x: f32) -> String {
    format!("{:.0} %", x * 100.0)
}

// ---------------------------------------------------------------- raster plumbing

/// A 256 x 256 gray PNG of quantised logits, base64 -- the `logits` field of a `segment_mask`.
fn encode_logits(logits: &[f32]) -> String {
    debug_assert_eq!(logits.len(), RASTER * RASTER);
    let bytes: Vec<u8> =
        logits.iter().map(|&l| ((l.clamp(-LOGIT_RANGE, LOGIT_RANGE) + LOGIT_RANGE) / (2.0 * LOGIT_RANGE) * 255.0).round() as u8).collect();
    let img = GrayImage::from_raw(RASTER as u32, RASTER as u32, bytes).expect("raster size");
    let mut png = std::io::Cursor::new(Vec::new());
    img.write_to(&mut png, image::ImageFormat::Png).expect("png encode of an in-memory gray image");
    base64::engine::general_purpose::STANDARD.encode(png.into_inner())
}

fn segment_mask(logits: &[f32], feather: f32) -> Value {
    json!({ "op": "segment_mask", "logits": encode_logits(logits), "feather": feather, "grow": 0, "invert": false })
}

fn coverage_of(logits: &[f32]) -> f32 {
    logits.iter().filter(|&&l| l > 0.0).count() as f32 / logits.len() as f32
}

// ---------------------------------------------------------------- shadows

/// "Shadows" is a range of the photograph's own brightness, so it is a `luminance_range_mask` (stored
/// geometry, no raster): from black up to the darkest quarter of this frame, never above luma code 64 of 255.
const SHADOW_PERCENTILE: f32 = 0.25;
const SHADOW_CAP: f32 = 64.0 / 255.0;
const SHADOW_MIN_SHARE: f32 = 0.03;
const SHADOW_CHECK_SHARE: f32 = 0.08;
/// More than this share under the cap means the frame is mostly shadow already (night, low key).
const SHADOW_MAX_DARK: f32 = 0.5;
const SHADOW_FEATHER: f32 = 30.0;
const ANALYSIS_EDGE: u32 = 300;

fn shrink(img: &RgbImage, edge: u32) -> RgbImage {
    let (w, h) = img.dimensions();
    let s = edge as f32 / w.max(h) as f32;
    if s >= 1.0 {
        return img.clone();
    }
    image::imageops::resize(img, ((w as f32 * s).round() as u32).max(1), ((h as f32 * s).round() as u32).max(1), FilterType::Triangle)
}

pub fn resolve_shadows(img: &RgbImage) -> RegionResult {
    let small = shrink(img, ANALYSIS_EDGE);
    let mut luma: Vec<f32> =
        small.pixels().map(|p| (0..3).map(|c| p.0[c] as f32 / 255.0 * LUMA[c]).sum()).collect();
    let n = luma.len() as f32;
    let dark = luma.iter().filter(|&&l| l <= SHADOW_CAP).count() as f32 / n;
    if dark > SHADOW_MAX_DARK {
        return RegionResult::not_found(format!("{} of the frame is already deep shadow", pct(dark)));
    }
    luma.sort_by(|a, b| a.total_cmp(b));
    let p = luma[((luma.len() - 1) as f32 * SHADOW_PERCENTILE) as usize];
    let top = p.min(SHADOW_CAP);
    let share = luma.iter().filter(|&&l| l <= top).count() as f32 / n;
    if share < SHADOW_MIN_SHARE {
        return RegionResult::not_found(format!("only {} of the frame is dark enough to be shadow", pct(share)));
    }
    let mask = json!({
        "op": "luminance_range_mask", "rangeMin": 0, "rangeMax": (top * 1000.0).round() / 10.0,
        "feather": SHADOW_FEATHER, "invert": false,
        // RFC-0036: the range was measured on the original luma, so the mask measures it too and no later edit moves it.
        "basis": "original",
    });
    if share < SHADOW_CHECK_SHARE {
        return RegionResult::with_mask(Outcome::Check, share, Some(format!("shadows are only {} of the frame", pct(share))), mask);
    }
    RegionResult::with_mask(Outcome::Found, share, None, mask)
}

// ---------------------------------------------------------------- faces

/// One stored face (normalised to the whole, uncropped frame). `score` is `None` for rows read from the
/// catalog, which keeps the box but not the detector's score.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FaceBox {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub score: Option<f32>,
}

/// A face smaller than this many pixels on its short side at a 1,600 px long edge is dropped (RFC-0033
/// §10.1: the floor is pixels, not a share of the frame).
const FACE_MIN_SIDE_PX: f32 = 24.0;
const FACE_REF_EDGE: f32 = 1600.0;
/// Rows closer than this (IoU) are one face: the catalog holds duplicate rows.
const FACE_DUP_IOU: f32 = 0.85;
const FACE_LOW_SCORE: f32 = 0.75;
/// Faces whose ellipses cover more than this share of the frame are a statue, a poster or a dog (RFC-0033 §10.1's false positives;
/// measured 48-58 %), not a portrait (25-29 % for the tightest real close-ups): look at it.
const FACE_BIG_COVERAGE: f32 = 0.40;
/// A weak score only matters when it is most of the result: in a crowd one stray blob is harmless.
const FACE_FEW: usize = 2;
/// A face box over this share of the frame is a statue, a poster or an animal, not a person to adjust. Measured on 40 portraits
/// and crowds: the false positives (a statue's head, a dog) were 33-38 %, the largest real close-up 19 %; only asks for a look.
const FACE_HUGE_AREA: f32 = 0.25;
/// The tight box ends at forehead and chin; the ellipse reaches hair and neck.
const FACE_GROW_X: f32 = 1.3;
const FACE_GROW_Y: f32 = 1.5;
const FACE_LIFT_Y: f32 = 0.08;
/// Logits per unit of normalised radius: saturated inside 0.37 radius, zero at the ellipse edge.
const FACE_GAIN: f32 = 6.0;
const FACE_FEATHER: f32 = 50.0;

fn iou(a: &FaceBox, b: &FaceBox) -> f32 {
    let w = (a.x + a.w).min(b.x + b.w) - a.x.max(b.x);
    let h = (a.y + a.h).min(b.y + b.h) - a.y.max(b.y);
    let i = w.max(0.0) * h.max(0.0);
    let u = a.w * a.h + b.w * b.h - i;
    if u > 0.0 { i / u } else { 0.0 }
}

/// The faces worth using: identical boxes dropped (the catalog holds duplicate rows) and those under the pixel floor,
/// or the `not_found` result that says why there are none. `frame` is the pixel size the boxes are relative to.
fn kept_faces(boxes: &[FaceBox], frame: (u32, u32)) -> Result<Vec<FaceBox>, RegionResult> {
    if boxes.is_empty() {
        return Err(RegionResult::not_found("no face in this photograph"));
    }
    let long = frame.0.max(frame.1) as f32;
    let side_px = |f: &FaceBox| (f.w * frame.0 as f32).min(f.h * frame.1 as f32) / long * FACE_REF_EDGE;
    let mut by_score: Vec<FaceBox> = boxes.to_vec();
    by_score.sort_by(|a, b| b.score.unwrap_or(0.0).total_cmp(&a.score.unwrap_or(0.0)));
    let mut kept: Vec<FaceBox> = Vec::new();
    for f in by_score {
        if f.w > 0.0 && f.h > 0.0 && !kept.iter().any(|k| iou(&f, k) > FACE_DUP_IOU) {
            kept.push(f);
        }
    }
    let total = kept.len();
    kept.retain(|f| side_px(f) >= FACE_MIN_SIDE_PX);
    if kept.is_empty() {
        return Err(RegionResult::not_found(format!("{total} face(s), all under the size floor ({FACE_MIN_SIDE_PX:.0} px at a {FACE_REF_EDGE:.0} px frame)")));
    }
    Ok(kept)
}

/// Few faces, one of them with a weak detector score: the detector's score is the only signal that tells a dog or a toy from a
/// face (measured 0.71-0.80 where real faces are mostly 0.85 and up, though in a crowd real ones dip to 0.65).
fn weak_detection(kept: &[FaceBox]) -> bool {
    kept.len() <= FACE_FEW && kept.iter().any(|f| f.score.is_some_and(|s| s < FACE_LOW_SCORE))
}

/// All the faces of one photograph in one `segment_mask`, any number of them. `frame` is the pixel size of the
/// image the boxes are relative to (only the aspect and the pixel floor use it).
pub fn resolve_faces(boxes: &[FaceBox], frame: (u32, u32)) -> RegionResult {
    let kept = match kept_faces(boxes, frame) {
        Ok(k) => k,
        Err(none) => return none,
    };
    let mut logits = vec![-LOGIT_RANGE; RASTER * RASTER];
    for f in &kept {
        let (cx, cy) = (f.x + f.w / 2.0, f.y + f.h / 2.0 - FACE_LIFT_Y * f.h);
        let (rx, ry) = (f.w / 2.0 * FACE_GROW_X, f.h / 2.0 * FACE_GROW_Y);
        let texel = |v: f32| v * RASTER as f32;
        let lo = |c: f32, r: f32| (texel(c - r * 1.6).floor().max(0.0)) as usize;
        let hi = |c: f32, r: f32| (texel(c + r * 1.6).ceil().min(RASTER as f32)) as usize;
        for iy in lo(cy, ry)..hi(cy, ry) {
            let dy = ((iy as f32 + 0.5) / RASTER as f32 - cy) / ry;
            for ix in lo(cx, rx)..hi(cx, rx) {
                let dx = ((ix as f32 + 0.5) / RASTER as f32 - cx) / rx;
                let l = (1.0 - (dx * dx + dy * dy).sqrt()) * FACE_GAIN;
                let at = &mut logits[iy * RASTER + ix];
                *at = at.max(l.clamp(-LOGIT_RANGE, LOGIT_RANGE));
            }
        }
    }
    let coverage = coverage_of(&logits);
    let mask = segment_mask(&logits, FACE_FEATHER);

    let mut why: Vec<String> = Vec::new();
    if coverage > FACE_BIG_COVERAGE {
        why.push(format!("the face covers over {} of the frame (statue, poster or animal?)", pct(FACE_BIG_COVERAGE)));
    }
    if weak_detection(&kept) {
        why.push("a face was detected with low confidence".to_string());
    }
    if why.is_empty() {
        RegionResult::with_mask(Outcome::Found, coverage, None, mask)
    } else {
        RegionResult::with_mask(Outcome::Check, coverage, Some(why.join("; ")), mask)
    }
}

// ---------------------------------------------------------------- click-select model (person, refined sky)

/// What the click-select model gives for one set of clicks: the model's own quality estimate and the 256 x 256 logits.
pub struct Cut {
    pub iou: f32,
    pub logits: Vec<f32>,
}

/// The click-select model, seen from the resolvers: clicks (normalised to the frame, `true` = part of it) in, a cut out. The real
/// one asks the AI helper; the tests use a stand-in.
pub trait ClickModel {
    fn cut(&mut self, clicks: &[(f32, f32, bool)]) -> Result<Cut, String>;
    /// The model's best cut of whatever is inside `bbox` (SAM 2's box prompt, RFC-0037: 82 % of detected people cut at IoU 0.7
    /// against 65 % with two clicks). A model without box prompts says so.
    fn cut_box(&mut self, _bbox: &BBox) -> Result<Cut, String> {
        Err("this click-select model has no box prompt".to_string())
    }
}

/// RFC-0033 section 3.1: a person is found by clicking where their face is and where their torso should be, with the
/// Select Subject model. Faces below the pixel floor are not used (a face the detector may not even have right is a
/// poor place to click), and a crowd is capped: each person is one decode.
const PERSON_MAX: usize = 6;
/// A cut the model rates under this is left out rather than merged. Measured on 37 photographs (RFC-0035 §7): cuts that
/// looked right were rated 0.90-0.98 (two scraped in at 0.70-0.71); the single subjects the model doubted were 0.62-0.65.
const PERSON_MIN_IOU: f32 = 0.6;
/// A cut rated between the two is used, but the result is `check`: the model was only moderately sure of it.
const PERSON_SURE_IOU: f32 = 0.7;
/// The torso click sits this many face-heights below the face centre.
const PERSON_TORSO_DROP: f32 = 2.0;
const PERSON_TORSO_LIMIT: f32 = 0.97;
/// The nearer retry click: about one face-height below the face centre is the chest of a bust-length portrait.
const PERSON_CHEST_DROP: f32 = 1.0;
const PERSON_MIN_COVERAGE: f32 = 0.01;
/// Single subjects that looked right covered 6-38 % of the frame; the one flood (an old cabinet card: the cut took the
/// whole mount, rated 0.92) covered 67 %. A real close-up can pass 60 % too (61 % on one photograph), so this only asks
/// for a look.
const PERSON_BIG_COVERAGE: f32 = 0.6;
const PERSON_FEATHER: f32 = 30.0;

/// Tuning aid: with `EMULSION_ADAPTIVE_TRACE=<file>` set, appends one line per decision (a person's cut: confidence and
/// size) so thresholds can be chosen from what the model reported. Does nothing otherwise.
pub(crate) fn trace(line: &str) {
    use std::io::Write;
    let Ok(path) = std::env::var("EMULSION_ADAPTIVE_TRACE") else { return };
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{line}");
    }
}

/// The people of one photograph in one `segment_mask`: for the largest faces, a click on the face and one on the
/// torso below it, the model's best cut for each, merged. `Err` is a model failure the caller shows as it is.
pub fn resolve_person(boxes: &[FaceBox], frame: (u32, u32), model: &mut dyn ClickModel) -> Result<RegionResult, String> {
    let mut faces = match kept_faces(boxes, frame) {
        Ok(k) => k,
        Err(none) => return Ok(none),
    };
    let (weak, huge) = (weak_detection(&faces), faces.iter().any(|f| f.w * f.h > FACE_HUGE_AREA));
    faces.sort_by(|a, b| (b.w * b.h).total_cmp(&(a.w * a.h)));
    let beyond = faces.len().saturating_sub(PERSON_MAX);
    faces.truncate(PERSON_MAX);

    let mut merged = vec![-LOGIT_RANGE; RASTER * RASTER];
    let (mut used, mut unsure, mut moderate) = (0usize, 0usize, 0usize);
    for f in &faces {
        let (cx, cy) = (f.x + f.w / 2.0, f.y + f.h / 2.0);
        let mut clicks = vec![(cx, cy, true)];
        let torso = cy + PERSON_TORSO_DROP * f.h;
        if torso < PERSON_TORSO_LIMIT {
            clicks.push((cx, torso, true));
        }
        let mut cut = model.cut(&clicks)?;
        // On a bust-length portrait (a cabinet card, a passport crop) the torso click can land on the mount or the frame, not on the
        // person: the model is then only moderately sure (0.65 where the cuts that looked right were 0.90 and up). Try a nearer
        // chest click and keep whichever cut the model rates higher. (The face alone is no use: it cuts just the face, rated 0.92.)
        if clicks.len() > 1 && cut.iou < PERSON_SURE_IOU {
            let near = [clicks[0], (cx, cy + PERSON_CHEST_DROP * f.h, true)];
            if let Ok(chest) = model.cut(&near) {
                trace(&format!("person chest_click iou={:.3} cut_coverage={:.3} (two clicks {:.3})", chest.iou, coverage_of(&chest.logits), cut.iou));
                if chest.iou > cut.iou {
                    cut = chest;
                }
            }
        }
        trace(&format!(
            "person face=({:.3},{:.3},{:.3}x{:.3}) score={} torso_click={} iou={:.3} cut_coverage={:.3}",
            f.x, f.y, f.w, f.h, f.score.map_or("?".to_string(), |s| format!("{s:.3}")), clicks.len() > 1, cut.iou, coverage_of(&cut.logits)
        ));
        if cut.iou < PERSON_MIN_IOU || cut.logits.len() != merged.len() {
            unsure += 1;
            continue;
        }
        used += 1;
        if cut.iou < PERSON_SURE_IOU {
            moderate += 1;
        }
        for (m, l) in merged.iter_mut().zip(&cut.logits) {
            *m = m.max(*l);
        }
    }
    if used == 0 {
        return Ok(RegionResult::not_found(if unsure > 0 {
            "the model was not sure of anyone it was pointed at".to_string()
        } else {
            "no person found".to_string()
        }));
    }
    let coverage = coverage_of(&merged);
    if coverage < PERSON_MIN_COVERAGE {
        return Ok(RegionResult::not_found(format!("the people cover only {} of the frame", pct(coverage))));
    }
    let mut why: Vec<String> = Vec::new();
    if weak {
        why.push("a face was detected with low confidence (an animal or a statue?)".to_string());
    }
    if huge {
        why.push(format!("a face fills over {} of the frame (a statue, a poster or an animal?)", pct(FACE_HUGE_AREA)));
    }
    if beyond > 0 {
        why.push(format!("{beyond} more face(s) beyond the {PERSON_MAX} largest were not used"));
    }
    if unsure > 0 {
        why.push(format!("{unsure} person(s) the model was unsure of were left out"));
    }
    if moderate > 0 {
        why.push(format!("the model was only moderately sure of {moderate} person(s)"));
    }
    if coverage > PERSON_BIG_COVERAGE {
        why.push(format!("the mask covers over {} of the frame", pct(PERSON_BIG_COVERAGE)));
    }
    let mask = segment_mask(&merged, PERSON_FEATHER);
    Ok(if why.is_empty() {
        RegionResult::with_mask(Outcome::Found, coverage, None, mask)
    } else {
        RegionResult::with_mask(Outcome::Check, coverage, Some(why.join("; ")), mask)
    })
}

// ---------------------------------------------------------------- people without a face (RFC-0037)

/// People the detector is at least this sure of are used. Measured on 700 COCO photographs (D-FINE medium, people with no usable face):
/// at 0.4 it finds 89 % of them and flags 6.5 % of the person-free photographs, at 0.5 85 % and 5.0 %, at 0.6 78 % and 2.0 %.
pub const PERSON_DETECT_MIN_SCORE: f32 = 0.5;
/// A crowd is capped like the face-based path; the largest figures are the ones worth a mask.
const PERSON_DETECT_MAX: usize = 8;
/// A figure 8 % of the frame tall and a third as wide covers about 0.2 % of the frame, and the figures of a tiny crowd (2 % wide,
/// 5 % tall) 0.1 %, so the floor for a detected person is far lower than the face-based path's 1 %.
const PERSON_DETECT_MIN_COVERAGE: f32 = 0.0005;

/// RFC-0037: the people the detector found, each cut with a box prompt, merged. Used when the face-based path found nobody, so
/// the result is always `check`: SAM's own confidence cannot tell a wrong detection from a right one (measured: 99 % of false
/// detections pass the 0.6 gate), so the detector's score is the only gate and the user is asked to look. `Err` is a model failure.
pub fn resolve_person_by_detection(people: &[PersonBox], model: &mut dyn ClickModel) -> Result<RegionResult, String> {
    let mut people: Vec<PersonBox> = people.iter().copied().filter(|p| p.score >= PERSON_DETECT_MIN_SCORE).collect();
    if people.is_empty() {
        return Ok(RegionResult::not_found("no person found"));
    }
    let area = |p: &PersonBox| (p.x1 - p.x0) * (p.y1 - p.y0);
    people.sort_by(|a, b| area(b).total_cmp(&area(a)));
    let beyond = people.len().saturating_sub(PERSON_DETECT_MAX);
    people.truncate(PERSON_DETECT_MAX);

    let mut merged = vec![-LOGIT_RANGE; RASTER * RASTER];
    let (mut used, mut unsure) = (0usize, 0usize);
    for p in &people {
        let cut = model.cut_box(&BBox { x0: p.x0, y0: p.y0, x1: p.x1, y1: p.y1 })?;
        trace(&format!(
            "person detected box=({:.3},{:.3},{:.3},{:.3}) score={:.3} iou={:.3} cut_coverage={:.3}",
            p.x0, p.y0, p.x1, p.y1, p.score, cut.iou, coverage_of(&cut.logits)
        ));
        if cut.iou < PERSON_MIN_IOU || cut.logits.len() != merged.len() || coverage_of(&cut.logits) < PERSON_DETECT_MIN_COVERAGE {
            unsure += 1;
            continue;
        }
        used += 1;
        for (m, l) in merged.iter_mut().zip(&cut.logits) {
            *m = m.max(*l);
        }
    }
    if used == 0 {
        return Ok(RegionResult::not_found("the model was not sure of anyone it was pointed at"));
    }
    let coverage = coverage_of(&merged);
    let mut why = vec![format!(
        "{} found by shape, without a face",
        if used == 1 { "1 person".to_string() } else { format!("{used} people") }
    )];
    if beyond > 0 {
        why.push(format!("{beyond} more beyond the {PERSON_DETECT_MAX} largest were not used"));
    }
    if unsure > 0 {
        why.push(format!("{unsure} the model was unsure of were left out"));
    }
    if coverage > PERSON_BIG_COVERAGE {
        why.push(format!("the mask covers over {} of the frame", pct(PERSON_BIG_COVERAGE)));
    }
    Ok(RegionResult::with_mask(Outcome::Check, coverage, Some(why.join("; ")), segment_mask(&merged, PERSON_FEATHER)))
}

/// A detected person whose cut shares at least this much of its area with what the face-based path already masked is that same
/// person (or a neighbour the face-based cut already took in); it is not a new one.
const PERSON_ALREADY_MASKED: f32 = 0.5;
/// A detected person whose box holds the centre of a face the face-based path used is that face's person, even when the box cut
/// is bigger than the face-click cut (a bust-length portrait: the clicks took the head, the box takes the shoulders).
fn holds_a_face(p: &PersonBox, faces: &[FaceBox]) -> bool {
    faces.iter().any(|f| {
        let (cx, cy) = (f.x + f.w / 2.0, f.y + f.h / 2.0);
        cx > p.x0 && cx < p.x1 && cy > p.y0 && cy < p.y1
    })
}

/// RFC-0037, mixed photographs: the face-based path found some people (`base`, a `found` or `check`), and the detector may see
/// others it has no face for (seen from behind, in silhouette, a small figure in the background). Each person the detector is sure of
/// is cut with a box prompt; a cut that mostly lies inside the mask already there is the same person and is ignored, a cut that does not
/// is a new person and is added. With no new person `base` comes back untouched (a clean `found` stays `found`); with one the result is
/// `check`, for the same reason as `resolve_person_by_detection`. A detection that holds a face the face-based path used is that
/// face's person and is not even cut. `Err` is a model failure; the caller keeps `base` then.
pub fn add_people_by_detection(
    base: RegionResult,
    people: &[PersonBox],
    boxes: &[FaceBox],
    frame: (u32, u32),
    model: &mut dyn ClickModel,
) -> Result<RegionResult, String> {
    let Some(mask) = base.mask.as_ref() else { return Ok(base) };
    let faces = kept_faces(boxes, frame).unwrap_or_default();
    let mut merged = decode_logits(mask["logits"].as_str().ok_or("the person mask has no raster")?)?;
    let mut people: Vec<PersonBox> = people.iter().copied().filter(|p| p.score >= PERSON_DETECT_MIN_SCORE && !holds_a_face(p, &faces)).collect();
    let area = |p: &PersonBox| (p.x1 - p.x0) * (p.y1 - p.y0);
    people.sort_by(|a, b| area(b).total_cmp(&area(a)));
    let beyond = people.len().saturating_sub(PERSON_DETECT_MAX);
    people.truncate(PERSON_DETECT_MAX);

    let (mut added, mut unsure) = (0usize, 0usize);
    for p in &people {
        let cut = model.cut_box(&BBox { x0: p.x0, y0: p.y0, x1: p.x1, y1: p.y1 })?;
        if cut.logits.len() != merged.len() {
            unsure += 1;
            continue;
        }
        let (size, inside) = cut.logits.iter().zip(&merged).fold((0usize, 0usize), |(n, k), (c, m)| if *c > 0.0 { (n + 1, k + usize::from(*m > 0.0)) } else { (n, k) });
        let (size_share, inside_share) = (size as f32 / merged.len() as f32, if size == 0 { 1.0 } else { inside as f32 / size as f32 });
        trace(&format!(
            "person detected-in-mixed box=({:.3},{:.3},{:.3},{:.3}) score={:.3} iou={:.3} cut_coverage={:.3} already_masked={:.3}",
            p.x0, p.y0, p.x1, p.y1, p.score, cut.iou, size_share, inside_share
        ));
        if size_share < PERSON_DETECT_MIN_COVERAGE || inside_share >= PERSON_ALREADY_MASKED {
            continue;
        }
        if cut.iou < PERSON_MIN_IOU {
            unsure += 1;
            continue;
        }
        added += 1;
        for (m, l) in merged.iter_mut().zip(&cut.logits) {
            *m = m.max(*l);
        }
    }
    if added == 0 {
        return Ok(base);
    }
    let coverage = coverage_of(&merged);
    let mut why: Vec<String> = base.reason.iter().cloned().collect();
    why.push(format!("{} found by shape, without a face", if added == 1 { "1 more person".to_string() } else { format!("{added} more people") }));
    if beyond > 0 {
        why.push(format!("{beyond} more beyond the {PERSON_DETECT_MAX} largest were not used"));
    }
    if unsure > 0 {
        why.push(format!("{unsure} the model was unsure of were left out"));
    }
    if coverage > PERSON_BIG_COVERAGE && !why.iter().any(|w| w.starts_with("the mask covers over")) {
        why.push(format!("the mask covers over {} of the frame", pct(PERSON_BIG_COVERAGE)));
    }
    Ok(RegionResult::with_mask(Outcome::Check, coverage, Some(why.join("; ")), segment_mask(&merged, PERSON_FEATHER)))
}

/// The inverse of `encode_logits`: a base64 256 x 256 gray PNG back to logits.
pub fn decode_logits(b64: &str) -> Result<Vec<f32>, String> {
    let bytes = base64::engine::general_purpose::STANDARD.decode(b64).map_err(|e| e.to_string())?;
    let img = image::load_from_memory_with_format(&bytes, image::ImageFormat::Png).map_err(|e| e.to_string())?.to_luma8();
    if img.width() as usize != RASTER || img.height() as usize != RASTER {
        return Err(format!("the cut is {} x {}, expected {RASTER} x {RASTER}", img.width(), img.height()));
    }
    Ok(img.as_raw().iter().map(|&v| v as f32 / 255.0 * 2.0 * LOGIT_RANGE - LOGIT_RANGE).collect())
}

// ---------------------------------------------------------------- sky

/// RFC-0033 §9's heuristic (port of `RFC-0033-appendix/skyheur.py`): sky is the smooth region connected to the
/// top edge whose colour changes only gradually, plus bright low-saturation textured pixels next to it
/// (clouds), with holes filled. No model; measured at 0.53-0.65 IoU against the segmenter on night, fog and
/// mist, much better on clear photographs.
const SKY_TEX: f32 = 0.035;
const SKY_STEP: f32 = 0.045;
const SKY_TEX_HI: f32 = 0.10;
const SKY_MIN_AREA: f32 = 0.02;
const SKY_MIN_TOP_TOUCH: f32 = 0.25;
const SKY_CHECK_AREA: f32 = 0.08;
const SKY_CHECK_HOLES: f32 = 0.15;
const SKY_CHECK_DARK: f32 = 0.20;
const SKY_FEATHER: f32 = 40.0;
const SKY_CLOUD_STEPS: usize = 12;

/// The result of the sky heuristic at its working size, before it becomes a raster.
pub struct SkyAnalysis {
    pub w: usize,
    pub h: usize,
    /// Row-major, `w * h`; empty when the smooth top region is too small to be sky.
    pub mask: Vec<bool>,
    /// Share of the frame the smooth region grown from the top row alone reaches (before clouds and fill).
    pub seen_share: f32,
    /// Share of the top row that region touches.
    pub top_touch: f32,
    /// Pixels added by hole filling, as a share of the sky.
    pub holes_share: f32,
    pub mean_luma: f32,
}

/// Mean over the `(2r+1)^2` window with edge replication (numpy `pad(mode="edge")` + summed-area table).
fn box_mean(a: &[f32], w: usize, h: usize, r: usize) -> Vec<f32> {
    let k = 2 * r + 1;
    let (pw, ph) = (w + 2 * r, h + 2 * r);
    let mut integ = vec![0f64; (pw + 1) * (ph + 1)];
    for y in 0..ph {
        let sy = y.saturating_sub(r).min(h - 1);
        let mut row = 0f64;
        for x in 0..pw {
            row += a[sy * w + x.saturating_sub(r).min(w - 1)] as f64;
            integ[(y + 1) * (pw + 1) + x + 1] = integ[y * (pw + 1) + x + 1] + row;
        }
    }
    let at = |y: usize, x: usize| integ[y * (pw + 1) + x];
    let mut out = vec![0f32; w * h];
    for y in 0..h {
        for x in 0..w {
            out[y * w + x] = ((at(y + k, x + k) - at(y, x + k) - at(y + k, x) + at(y, x)) / (k * k) as f64) as f32;
        }
    }
    out
}

/// numpy `np.gradient` (spacing 1): central differences inside, one-sided at the borders.
fn gradient(a: &[f32], w: usize, h: usize) -> (Vec<f32>, Vec<f32>) {
    let (mut gy, mut gx) = (vec![0f32; w * h], vec![0f32; w * h]);
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            gx[i] = if w < 2 { 0.0 } else if x == 0 { a[i + 1] - a[i] } else if x == w - 1 { a[i] - a[i - 1] } else { (a[i + 1] - a[i - 1]) / 2.0 };
            gy[i] = if h < 2 { 0.0 } else if y == 0 { a[i + w] - a[i] } else if y == h - 1 { a[i] - a[i - w] } else { (a[i + w] - a[i - w]) / 2.0 };
        }
    }
    (gy, gx)
}

fn median(mut v: Vec<f32>) -> f32 {
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(|a, b| a.total_cmp(b));
    let m = v.len() / 2;
    if v.len() % 2 == 1 { v[m] } else { (v[m - 1] + v[m]) / 2.0 }
}

fn neighbours(i: usize, w: usize, h: usize) -> impl Iterator<Item = usize> {
    let (x, y) = (i % w, i / w);
    [(y > 0).then(|| i - w), (y + 1 < h).then(|| i + w), (x > 0).then(|| i - 1), (x + 1 < w).then(|| i + 1)].into_iter().flatten()
}

pub fn analyse_sky(img: &RgbImage) -> SkyAnalysis {
    let small = shrink(img, ANALYSIS_EDGE);
    let (w, h) = (small.width() as usize, small.height() as usize);
    let plane = |c: usize| -> Vec<f32> { small.pixels().map(|p| p.0[c] as f32 / 255.0).collect() };
    let (r, g, b) = (plane(0), plane(1), plane(2));
    let y_plane: Vec<f32> = (0..w * h).map(|i| r[i] * LUMA[0] + g[i] * LUMA[1] + b[i] * LUMA[2]).collect();
    let (gy, gx) = gradient(&box_mean(&y_plane, w, h, 1), w, h);
    let mag: Vec<f32> = gx.iter().zip(&gy).map(|(x, y)| x.hypot(*y)).collect();
    let tex: Vec<f32> = box_mean(&mag, w, h, 2).into_iter().map(|t| t * 4.0).collect();
    let c = [box_mean(&r, w, h, 1), box_mean(&g, w, h, 1), box_mean(&b, w, h, 1)];
    let step = |a: usize, b: usize| (0..3).map(|k| (c[k][a] - c[k][b]).abs()).fold(0f32, f32::max);

    // Smooth pixels connected to the top row, grown while the colour changes by less than SKY_STEP per step.
    let mut seen = vec![false; w * h];
    let mut q: VecDeque<usize> = VecDeque::new();
    for x in 0..w {
        if tex[x] < SKY_TEX {
            seen[x] = true;
            q.push_back(x);
        }
    }
    while let Some(i) = q.pop_front() {
        for n in neighbours(i, w, h) {
            if !seen[n] && tex[n] < SKY_TEX && step(n, i) < SKY_STEP {
                seen[n] = true;
                q.push_back(n);
            }
        }
    }
    let count = |m: &[bool]| m.iter().filter(|&&v| v).count();
    let seen_share = count(&seen) as f32 / (w * h) as f32;
    let top_touch = seen[..w].iter().filter(|&&v| v).count() as f32 / w as f32;
    let none = SkyAnalysis { w, h, mask: Vec::new(), seen_share, top_touch, holes_share: 0.0, mean_luma: 0.0 };
    if seen_share < SKY_MIN_AREA {
        return none;
    }

    // Clouds: textured but bright, low-saturation pixels touching the sky, grown a few steps.
    let med_y = median((0..w * h).filter(|&i| seen[i]).map(|i| y_plane[i]).collect());
    let sat: Vec<f32> = (0..w * h).map(|i| r[i].max(g[i]).max(b[i]) - r[i].min(g[i]).min(b[i])).collect();
    let med_sat = median((0..w * h).filter(|&i| seen[i]).map(|i| sat[i]).collect());
    let cloud: Vec<bool> = (0..w * h).map(|i| tex[i] < SKY_TEX_HI && y_plane[i] > med_y - 0.02 && sat[i] < med_sat + 0.12).collect();
    let mut m = seen.clone();
    for _ in 0..SKY_CLOUD_STEPS {
        let prev = m.clone();
        for i in 0..w * h {
            if !m[i] && cloud[i] && neighbours(i, w, h).any(|n| prev[n]) {
                m[i] = true;
            }
        }
    }

    // Fill holes: background reachable from the bottom row or the side columns is not a hole.
    let before = count(&m);
    let mut reach = vec![false; w * h];
    let mut q: VecDeque<usize> = VecDeque::new();
    let border = (0..w).map(|x| (h - 1) * w + x).chain((0..h).flat_map(|y| [y * w, y * w + w - 1]));
    for i in border {
        if !m[i] && !reach[i] {
            reach[i] = true;
            q.push_back(i);
        }
    }
    while let Some(i) = q.pop_front() {
        for n in neighbours(i, w, h) {
            if !m[n] && !reach[n] {
                reach[n] = true;
                q.push_back(n);
            }
        }
    }
    for i in 0..w * h {
        if !m[i] && !reach[i] {
            m[i] = true;
        }
    }
    let after = count(&m);
    let mean_luma = (0..w * h).filter(|&i| m[i]).map(|i| y_plane[i]).sum::<f32>() / after as f32;
    SkyAnalysis { w, h, mask: m, seen_share, top_touch, holes_share: (after - before) as f32 / after as f32, mean_luma }
}

/// The sky mask at the stored raster size, soft-edged, as logits.
fn sky_logits(a: &SkyAnalysis) -> Vec<f32> {
    let gray = GrayImage::from_raw(a.w as u32, a.h as u32, a.mask.iter().map(|&v| if v { 255 } else { 0 }).collect()).expect("mask size");
    let sq = image::imageops::resize(&gray, RASTER as u32, RASTER as u32, FilterType::Triangle);
    let blurred = image::imageops::blur(&sq, 1.2);
    blurred.pixels().map(|p| (p.0[0] as f32 / 255.0 - 0.5) * 2.0 * 3.75).collect()
}

/// One square-window pass over a boolean raster: `any` = dilation (inside if any texel of the window is), otherwise erosion
/// (inside only if all are). The frame's edge counts as "inside" for erosion so the outline does not eat in from the border.
fn window_pass(src: &[bool], r: isize, any: bool) -> Vec<bool> {
    let n = RASTER as isize;
    let mut out = vec![false; src.len()];
    for y in 0..n {
        for x in 0..n {
            let mut result = !any;
            'win: for dy in -r..=r {
                for dx in -r..=r {
                    let (yy, xx) = (y + dy, x + dx);
                    let v = if yy < 0 || xx < 0 || yy >= n || xx >= n { !any } else { src[(yy * n + xx) as usize] };
                    if any && v {
                        result = true;
                        break 'win;
                    }
                    if !any && !v {
                        result = false;
                        break 'win;
                    }
                }
            }
            out[(y * n + x) as usize] = result;
        }
    }
    out
}

/// Morphological closing of a raster's inside (logit above 0) with a square of radius `r` texels: holes and gaps up to
/// about `2r` wide are filled, the outline elsewhere stays. The edge is then softened the way the colour rule's is.
fn close_inside(logits: &[f32], r: usize) -> Vec<f32> {
    let inside: Vec<bool> = logits.iter().map(|&l| l > 0.0).collect();
    let closed = window_pass(&window_pass(&inside, r as isize, true), r as isize, false);
    let gray = GrayImage::from_raw(RASTER as u32, RASTER as u32, closed.iter().map(|&v| if v { 255 } else { 0 }).collect()).expect("raster size");
    image::imageops::blur(&gray, 1.2).pixels().map(|p| (p.0[0] as f32 / 255.0 - 0.5) * 2.0 * 3.75).collect()
}

pub fn resolve_sky(img: &RgbImage) -> RegionResult {
    let a = analyse_sky(img);
    if a.mask.is_empty() {
        return RegionResult::not_found(format!("no smooth region reaches the top of the frame ({} reached)", pct(a.seen_share)));
    }
    if a.top_touch < SKY_MIN_TOP_TOUCH {
        return RegionResult::not_found(format!("the smooth region touches only {} of the top edge", pct(a.top_touch)));
    }
    let logits = sky_logits(&a);
    let coverage = coverage_of(&logits);
    let mask = segment_mask(&logits, SKY_FEATHER);
    let mut why: Vec<String> = Vec::new();
    if coverage < SKY_CHECK_AREA {
        why.push(format!("only {} of the frame", pct(coverage)));
    }
    if a.holes_share > SKY_CHECK_HOLES {
        why.push("the edge is ragged".to_string());
    }
    if a.mean_luma < SKY_CHECK_DARK {
        why.push("a dark sky (night or heavy cloud) is where this method is weakest".to_string());
    }
    // RFC-0035 §7 (B1 measurement): on 45 photographs against the segmenter, 12 of 33 `found` masks overlapped under 0.7 (patchy in real
    // sky, or on a hazy hillside or the black ground at night), and no cheap feature separated them from the good ones. So a sky from
    // this method is never `found`: it is always flagged for a look until a model-based sky replaces it.
    why.push("the sky is estimated without a model: look before keeping it".to_string());
    RegionResult::with_mask(Outcome::Check, coverage, Some(why.join("; ")), mask)
}


// ---------------------------------------------------------------- sky, refined by the click-select model

/// The colour rule's sky (above) is only ever an estimate (it was never `found`: 12 of 33 overlapped the segmenter under 0.7).
/// With the click-select model installed its confident region becomes the model's clicks, and the model's cut, which follows
/// real edges, replaces it when the model is sure of it. The two are compared: they agree, and the sky is `found`; they do
/// not, and it is flagged with how far apart they are.
const SKY_MODEL_MIN_IOU: f32 = 0.7;
/// The model's cut and the colour rule's must overlap at least this (IoU) for the sky to be `found`.
const SKY_AGREE: f32 = 0.75;
const SKY_MODEL_MIN_COVERAGE: f32 = 0.02;
/// The model's cut is patchy inside clouds (a dither-like texture on the one cloudy photograph looked at); holes up to about
/// this many texels wide (x2) are closed so a sky edit does not show them.
const SKY_CLOSE_RADIUS: usize = 3;
/// A band of the frame is clicked only if the sky covers at least this share of it.
const SKY_BAND_MIN: f32 = 0.03;
const SKY_BANDS: usize = 3;
/// The ground click comes from the lowest quarter of the frame, if enough of it is not sky.
const SKY_GROUND_ROWS: f32 = 0.25;
const SKY_GROUND_MIN: f32 = 0.02;

/// The sky cell of `cells` nearest to the mean position of them all (a mean can fall outside a ragged region).
fn representative(cells: &[(usize, usize)]) -> (usize, usize) {
    let n = cells.len() as f32;
    let (mx, my) = (cells.iter().map(|c| c.0 as f32).sum::<f32>() / n, cells.iter().map(|c| c.1 as f32).sum::<f32>() / n);
    *cells.iter().min_by(|a, b| ((a.0 as f32 - mx).powi(2) + (a.1 as f32 - my).powi(2)).total_cmp(&((b.0 as f32 - mx).powi(2) + (b.1 as f32 - my).powi(2)))).expect("non-empty")
}

/// Clicks for the model from the colour rule's raster: one positive click inside the sky in each band of columns that has
/// enough of it, one negative in the lowest rows where it is not sky. `None` when there is no sky to click.
fn sky_clicks(h: &[f32]) -> Option<Vec<(f32, f32, bool)>> {
    let to_unit = |c: (usize, usize)| ((c.0 as f32 + 0.5) / RASTER as f32, (c.1 as f32 + 0.5) / RASTER as f32);
    let band_w = RASTER / SKY_BANDS;
    let mut clicks: Vec<(f32, f32, bool)> = Vec::new();
    for b in 0..SKY_BANDS {
        let (x0, x1) = (b * band_w, if b + 1 == SKY_BANDS { RASTER } else { (b + 1) * band_w });
        let cells: Vec<(usize, usize)> = (0..RASTER).flat_map(|y| (x0..x1).map(move |x| (x, y))).filter(|&(x, y)| h[y * RASTER + x] > 0.0).collect();
        if cells.len() as f32 >= SKY_BAND_MIN * ((x1 - x0) * RASTER) as f32 {
            let (u, v) = to_unit(representative(&cells));
            clicks.push((u, v, true));
        }
    }
    if clicks.is_empty() {
        return None;
    }
    let y0 = ((1.0 - SKY_GROUND_ROWS) * RASTER as f32) as usize;
    let ground: Vec<(usize, usize)> = (y0..RASTER).flat_map(|y| (0..RASTER).map(move |x| (x, y))).filter(|&(x, y)| h[y * RASTER + x] <= 0.0).collect();
    if ground.len() as f32 >= SKY_GROUND_MIN * ((RASTER - y0) * RASTER) as f32 {
        let (u, v) = to_unit(representative(&ground));
        clicks.push((u, v, false));
    }
    Some(clicks)
}

fn overlap(a: &[f32], b: &[f32]) -> f32 {
    let (mut i, mut u) = (0usize, 0usize);
    for (x, y) in a.iter().zip(b) {
        let (p, q) = (*x > 0.0, *y > 0.0);
        i += (p && q) as usize;
        u += (p || q) as usize;
    }
    if u == 0 { 0.0 } else { i as f32 / u as f32 }
}

/// The sky of one photograph, outlined by the click-select model from the colour rule's clicks. Falls back to the colour rule's
/// own result, unchanged, when there is nothing to click, the model fails, is unsure, or cuts almost nothing.
pub fn resolve_sky_with_model(img: &RgbImage, model: &mut dyn ClickModel) -> RegionResult {
    let base = resolve_sky(img);
    let Some(h) = base.mask.as_ref().and_then(|m| m["logits"].as_str()).and_then(|b| decode_logits(b).ok()) else { return base };
    let Some(clicks) = sky_clicks(&h) else { return base };
    let Ok(cut) = model.cut(&clicks) else { return base };
    let coverage = coverage_of(&cut.logits);
    if cut.iou < SKY_MODEL_MIN_IOU || cut.logits.len() != h.len() || coverage < SKY_MODEL_MIN_COVERAGE {
        return base;
    }
    let logits = close_inside(&cut.logits, SKY_CLOSE_RADIUS);
    let coverage = coverage_of(&logits);
    let agree = overlap(&h, &logits);
    let mask = segment_mask(&logits, SKY_FEATHER);
    let mut why: Vec<String> = Vec::new();
    if agree < SKY_AGREE {
        why.push(format!("the model and the colour rule outline the sky differently ({} overlap)", pct(agree)));
    }
    if coverage < SKY_CHECK_AREA {
        why.push(format!("only {} of the frame", pct(coverage)));
    }
    if why.is_empty() {
        RegionResult::with_mask(Outcome::Found, coverage, None, mask)
    } else {
        RegionResult::with_mask(Outcome::Check, coverage, Some(why.join("; ")), mask)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgb;

    fn noise(seed: &mut u32) -> u8 {
        *seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        (*seed >> 24) as u8
    }

    fn decode(mask: &Value) -> Vec<f32> {
        let b64 = mask["logits"].as_str().unwrap();
        let bytes = base64::engine::general_purpose::STANDARD.decode(b64).unwrap();
        let img = image::load_from_memory_with_format(&bytes, image::ImageFormat::Png).unwrap().to_luma8();
        assert_eq!((img.width(), img.height()), (RASTER as u32, RASTER as u32));
        img.as_raw().iter().map(|&q| q as f32 / 255.0 * 8.0 - 4.0).collect()
    }

    fn at(l: &[f32], u: f32, v: f32) -> f32 {
        l[(v * RASTER as f32) as usize * RASTER + (u * RASTER as f32) as usize]
    }

    /// A stand-in for the click-select model: a person is a column of `half` width around the face's x and from the
    /// face's top to the bottom of the frame, rated `iou`; it records every set of clicks it is given.
    struct FakeModel {
        iou: f32,
        half: f32,
        seen: Vec<Vec<(f32, f32, bool)>>,
        fail: bool,
    }
    impl FakeModel {
        fn new(iou: f32) -> Self {
            FakeModel { iou, half: 0.06, seen: Vec::new(), fail: false }
        }
    }
    impl ClickModel for FakeModel {
        fn cut(&mut self, clicks: &[(f32, f32, bool)]) -> Result<Cut, String> {
            if self.fail {
                return Err("the click-select model is not installed".to_string());
            }
            self.seen.push(clicks.to_vec());
            let (cx, cy, _) = clicks[0];
            let mut logits = vec![-LOGIT_RANGE; RASTER * RASTER];
            for iy in 0..RASTER {
                for ix in 0..RASTER {
                    let (u, v) = ((ix as f32 + 0.5) / RASTER as f32, (iy as f32 + 0.5) / RASTER as f32);
                    if (u - cx).abs() < self.half && v > cy - 0.05 {
                        logits[iy * RASTER + ix] = LOGIT_RANGE;
                    }
                }
            }
            Ok(Cut { iou: self.iou, logits })
        }
    }

    fn big_face(x: f32, y: f32) -> FaceBox {
        FaceBox { x, y, w: 0.06, h: 0.09, score: Some(0.9) }
    }

    /// A stand-in with box prompts: a cut is exactly the box, rated `iou`; it records the boxes it is given.
    struct BoxModel {
        iou: f32,
        seen: Vec<BBox>,
    }
    impl ClickModel for BoxModel {
        fn cut(&mut self, _clicks: &[(f32, f32, bool)]) -> Result<Cut, String> {
            Err("clicks are not used for a detected person".to_string())
        }
        fn cut_box(&mut self, b: &BBox) -> Result<Cut, String> {
            self.seen.push(*b);
            let mut logits = vec![-LOGIT_RANGE; RASTER * RASTER];
            for iy in 0..RASTER {
                for ix in 0..RASTER {
                    let (u, v) = ((ix as f32 + 0.5) / RASTER as f32, (iy as f32 + 0.5) / RASTER as f32);
                    if u > b.x0 && u < b.x1 && v > b.y0 && v < b.y1 {
                        logits[iy * RASTER + ix] = LOGIT_RANGE;
                    }
                }
            }
            Ok(Cut { iou: self.iou, logits })
        }
    }

    fn person(x0: f32, y0: f32, x1: f32, y1: f32, score: f32) -> PersonBox {
        PersonBox { x0, y0, x1, y1, score }
    }

    #[test]
    fn a_detected_person_is_cut_with_its_box_and_is_only_ever_a_check() {
        let mut model = BoxModel { iou: 0.95, seen: Vec::new() };
        let r = resolve_person_by_detection(&[person(0.4, 0.3, 0.5, 0.7, 0.9)], &mut model).unwrap();
        assert_eq!(r.outcome, Outcome::Check, "the detector's score is the only gate, so a person without a face is never `found`");
        assert_eq!(model.seen, vec![BBox { x0: 0.4, y0: 0.3, x1: 0.5, y1: 0.7 }]);
        assert!((r.coverage - 0.04).abs() < 0.005, "{}", r.coverage);
        assert!(r.reason.as_deref().unwrap().contains("1 person") && r.reason.unwrap().contains("without a face"));
        let logits = decode(&r.mask.unwrap());
        assert!(at(&logits, 0.45, 0.5) > 0.0 && at(&logits, 0.2, 0.5) < 0.0);
    }

    #[test]
    fn detections_under_the_score_are_ignored_and_none_left_is_not_found() {
        let mut model = BoxModel { iou: 0.95, seen: Vec::new() };
        let r = resolve_person_by_detection(&[person(0.4, 0.3, 0.5, 0.7, PERSON_DETECT_MIN_SCORE - 0.01)], &mut model).unwrap();
        assert_eq!(r.outcome, Outcome::NotFound);
        assert!(model.seen.is_empty(), "a weak detection is not even cut");
        assert_eq!(resolve_person_by_detection(&[], &mut model).unwrap().outcome, Outcome::NotFound);
    }

    #[test]
    fn a_crowd_keeps_the_largest_figures_and_says_how_many_were_left_out() {
        let mut model = BoxModel { iou: 0.95, seen: Vec::new() };
        // Ten figures of growing height, side by side.
        let people: Vec<PersonBox> = (0..10).map(|i| person(0.05 + 0.09 * i as f32, 0.9 - 0.02 * (i + 1) as f32, 0.09 + 0.09 * i as f32, 0.9, 0.8)).collect();
        let r = resolve_person_by_detection(&people, &mut model).unwrap();
        assert_eq!(model.seen.len(), PERSON_DETECT_MAX);
        assert!(model.seen.iter().all(|b| b.y1 - b.y0 > 0.04 + 0.001), "the two smallest are the ones dropped");
        let why = r.reason.unwrap();
        assert!(why.contains("8 people") && why.contains("2 more beyond"), "{why}");
    }

    #[test]
    fn a_cut_the_model_doubts_or_that_is_a_speck_is_left_out() {
        let mut doubtful = BoxModel { iou: PERSON_MIN_IOU - 0.05, seen: Vec::new() };
        let r = resolve_person_by_detection(&[person(0.4, 0.3, 0.5, 0.7, 0.9)], &mut doubtful).unwrap();
        assert_eq!(r.outcome, Outcome::NotFound);
        // A box far smaller than the floor (0.03 x 0.01 of the frame = 0.03 %).
        let mut sure = BoxModel { iou: 0.95, seen: Vec::new() };
        let speck = resolve_person_by_detection(&[person(0.4, 0.3, 0.43, 0.31, 0.9)], &mut sure).unwrap();
        assert_eq!(speck.outcome, Outcome::NotFound);
        // A good one next to a doubtful one: used, with the doubtful one counted.
        struct Mixed(usize);
        impl ClickModel for Mixed {
            fn cut(&mut self, _: &[(f32, f32, bool)]) -> Result<Cut, String> {
                Err("no clicks".into())
            }
            fn cut_box(&mut self, b: &BBox) -> Result<Cut, String> {
                self.0 += 1;
                BoxModel { iou: if self.0 == 1 { 0.95 } else { 0.4 }, seen: Vec::new() }.cut_box(b)
            }
        }
        let both = resolve_person_by_detection(&[person(0.1, 0.2, 0.3, 0.9, 0.9), person(0.6, 0.3, 0.7, 0.7, 0.9)], &mut Mixed(0)).unwrap();
        assert_eq!(both.outcome, Outcome::Check);
        assert!(both.reason.unwrap().contains("1 the model was unsure of"));
    }

    #[test]
    fn a_model_without_box_prompts_fails_loudly_instead_of_guessing() {
        let mut model = FakeModel::new(0.9);
        assert!(resolve_person_by_detection(&[person(0.4, 0.3, 0.5, 0.7, 0.9)], &mut model).is_err());
    }

    /// A photograph where the face-based path found one person (a column around x = 0.43, from y = 0.15 down).
    fn one_person_by_face() -> RegionResult {
        let r = resolve_person(&[big_face(0.4, 0.2)], (1600, 1200), &mut FakeModel::new(0.9)).unwrap();
        assert_eq!(r.outcome, Outcome::Found);
        r
    }

    /// `add_people_by_detection` for the photograph of `one_person_by_face`.
    fn add_in_the_face_photo(base: RegionResult, people: &[PersonBox], model: &mut dyn ClickModel) -> Result<RegionResult, String> {
        add_people_by_detection(base, people, &[big_face(0.4, 0.2)], (1600, 1200), model)
    }

    #[test]
    fn a_detection_holding_a_face_the_path_used_is_that_persons_and_is_not_even_cut() {
        // A bust-length portrait: the box takes the shoulders the face clicks did not, which is not a new person.
        let base = one_person_by_face();
        let mut model = BoxModel { iou: 0.95, seen: Vec::new() };
        let r = add_in_the_face_photo(base.clone(), &[person(0.2, 0.15, 0.7, 0.95, 0.95)], &mut model).unwrap();
        assert!(model.seen.is_empty());
        assert_eq!((r.outcome, r.mask), (Outcome::Found, base.mask));
    }

    #[test]
    fn a_detection_of_the_person_the_face_path_already_found_changes_nothing() {
        let base = one_person_by_face();
        let mut model = BoxModel { iou: 0.95, seen: Vec::new() };
        let r = add_in_the_face_photo(base.clone(), &[person(0.38, 0.3, 0.48, 0.9, 0.9)], &mut model).unwrap();
        assert_eq!(model.seen.len(), 1, "the detection was cut, to compare it with what is masked");
        assert_eq!(r.outcome, Outcome::Found, "no new person: a clean found stays found");
        assert_eq!((r.coverage, r.reason), (base.coverage, base.reason));
        assert_eq!(r.mask, base.mask);
    }

    #[test]
    fn a_figure_the_face_path_missed_is_added_and_the_result_asks_for_a_look() {
        let base = one_person_by_face();
        let mut model = BoxModel { iou: 0.95, seen: Vec::new() };
        let r = add_in_the_face_photo(base.clone(), &[person(0.38, 0.3, 0.48, 0.9, 0.9), person(0.7, 0.4, 0.8, 0.9, 0.8)], &mut model).unwrap();
        assert_eq!(r.outcome, Outcome::Check);
        assert!(r.coverage > base.coverage + 0.04, "{} vs {}", r.coverage, base.coverage);
        let why = r.reason.unwrap();
        assert!(why.contains("1 more person found by shape, without a face"), "{why}");
        let logits = decode(&r.mask.unwrap());
        assert!(at(&logits, 0.43, 0.5) > 0.0 && at(&logits, 0.75, 0.6) > 0.0 && at(&logits, 0.6, 0.6) < 0.0);
    }

    #[test]
    fn an_added_figure_keeps_what_the_face_path_already_said() {
        // A moderately sure face cut is a check with a reason; the added figure's note comes after it.
        let base = resolve_person(&[big_face(0.4, 0.2)], (1600, 1200), &mut FakeModel::new(0.65)).unwrap();
        assert_eq!(base.outcome, Outcome::Check);
        let r = add_in_the_face_photo(base.clone(), &[person(0.7, 0.4, 0.8, 0.9, 0.8)], &mut BoxModel { iou: 0.95, seen: Vec::new() }).unwrap();
        let why = r.reason.unwrap();
        assert!(why.starts_with(base.reason.unwrap().as_str()) && why.contains("1 more person found by shape"), "{why}");
    }

    #[test]
    fn weak_doubtful_and_speck_detections_add_nothing_to_a_face_result() {
        let base = one_person_by_face();
        let mut model = BoxModel { iou: 0.95, seen: Vec::new() };
        let r = add_in_the_face_photo(base.clone(), &[person(0.7, 0.4, 0.8, 0.9, PERSON_DETECT_MIN_SCORE - 0.01)], &mut model).unwrap();
        assert!(model.seen.is_empty() && r.mask == base.mask, "a weak detection is not even cut");
        let r = add_in_the_face_photo(base.clone(), &[person(0.7, 0.4, 0.8, 0.9, 0.9)], &mut BoxModel { iou: PERSON_MIN_IOU - 0.05, seen: Vec::new() }).unwrap();
        assert!(r.mask == base.mask && r.outcome == Outcome::Found, "a cut the model doubts is left out");
        let r = add_in_the_face_photo(base.clone(), &[person(0.7, 0.4, 0.73, 0.41, 0.9)], &mut BoxModel { iou: 0.95, seen: Vec::new() }).unwrap();
        assert!(r.mask == base.mask, "a speck is left out");
    }

    #[test]
    fn two_boxes_on_the_same_new_figure_count_once_and_a_not_found_base_is_returned_as_it_is() {
        let base = one_person_by_face();
        let twin = [person(0.7, 0.4, 0.8, 0.9, 0.9), person(0.71, 0.41, 0.8, 0.9, 0.8)];
        let r = add_in_the_face_photo(base, &twin, &mut BoxModel { iou: 0.95, seen: Vec::new() }).unwrap();
        assert!(r.reason.unwrap().contains("1 more person"), "the second box is already masked by the first");
        let none = RegionResult::not_found("no person found");
        let r = add_in_the_face_photo(none, &[person(0.7, 0.4, 0.8, 0.9, 0.9)], &mut BoxModel { iou: 0.95, seen: Vec::new() }).unwrap();
        assert_eq!(r.outcome, Outcome::NotFound, "that case is resolve_person_by_detection's");
    }

    #[test]
    fn a_model_failure_while_looking_for_more_people_is_an_error_the_caller_can_ignore() {
        assert!(add_in_the_face_photo(one_person_by_face(), &[person(0.7, 0.4, 0.8, 0.9, 0.9)], &mut FakeModel::new(0.9)).is_err());
    }

    #[test]
    fn a_person_is_clicked_on_the_face_and_on_the_torso_below_it() {
        let mut model = FakeModel::new(0.9);
        let r = resolve_person(&[big_face(0.4, 0.2)], (1600, 1200), &mut model).unwrap();
        assert_eq!(r.outcome, Outcome::Found);
        assert_eq!(model.seen.len(), 1);
        let clicks = &model.seen[0];
        assert_eq!(clicks.len(), 2);
        assert!((clicks[0].0 - 0.43).abs() < 1e-4 && (clicks[0].1 - 0.245).abs() < 1e-4, "face centre {:?}", clicks[0]);
        assert!((clicks[1].0 - 0.43).abs() < 1e-4 && (clicks[1].1 - (0.245 + 2.0 * 0.09)).abs() < 1e-4, "torso {:?}", clicks[1]);
        let l = decode(r.mask.as_ref().unwrap());
        assert!(at(&l, 0.43, 0.5) > 1.0, "the person is selected");
        assert!(at(&l, 0.9, 0.5) < -1.0, "elsewhere is not");
    }

    #[test]
    fn a_face_near_the_bottom_gets_only_the_face_click() {
        let mut model = FakeModel::new(0.9);
        resolve_person(&[FaceBox { x: 0.4, y: 0.86, w: 0.06, h: 0.09, score: Some(0.9) }], (1600, 1200), &mut model).unwrap();
        assert_eq!(model.seen[0].len(), 1);
    }

    #[test]
    fn people_are_merged_and_the_smallest_faces_beyond_the_cap_are_left_out_and_said() {
        let faces: Vec<FaceBox> = (0..8).map(|i| FaceBox { x: 0.05 + i as f32 * 0.11, y: 0.2, w: 0.05 + 0.001 * (8 - i) as f32, h: 0.08, score: Some(0.9) }).collect();
        let mut model = FakeModel::new(0.9);
        let r = resolve_person(&faces, (1600, 1200), &mut model).unwrap();
        assert_eq!(model.seen.len(), PERSON_MAX);
        assert_eq!(r.outcome, Outcome::Check);
        assert!(r.reason.as_deref().unwrap().contains("2 more face(s) beyond the 6 largest"));
        let l = decode(r.mask.as_ref().unwrap());
        assert!(at(&l, 0.05 + 0.025, 0.6) > 1.0 && at(&l, 0.05 + 0.11 * 3.0 + 0.025, 0.6) > 1.0, "two of the merged people");
    }

    #[test]
    fn a_cut_the_model_is_unsure_of_is_not_merged_and_nothing_found_says_why() {
        let mut model = FakeModel::new(0.4);
        let r = resolve_person(&[big_face(0.4, 0.2)], (1600, 1200), &mut model).unwrap();
        assert_eq!(r.outcome, Outcome::NotFound);
        assert!(r.reason.as_deref().unwrap().contains("not sure"));
        assert!(r.mask.is_none());
    }

    #[test]
    fn a_weakly_detected_lone_face_or_a_huge_one_asks_for_a_look_but_a_crowd_does_not() {
        let at = |face: FaceBox| resolve_person(&[face], (1600, 1200), &mut FakeModel::new(0.9)).unwrap();
        assert_eq!(at(big_face(0.4, 0.2)).outcome, Outcome::Found);
        let weak = at(FaceBox { score: Some(0.71), ..big_face(0.4, 0.2) });
        assert_eq!(weak.outcome, Outcome::Check);
        assert!(weak.reason.as_deref().unwrap().contains("low confidence"), "{weak:?}");
        let huge = at(FaceBox { x: 0.2, y: 0.1, w: 0.6, h: 0.63, score: Some(0.95) });
        assert_eq!(huge.outcome, Outcome::Check);
        assert!(huge.reason.as_deref().unwrap().contains("fills over 25 %"), "{huge:?}");
        // Three faces, one of them weak: in a group one stray detection is harmless.
        let group = [big_face(0.1, 0.2), big_face(0.4, 0.2), FaceBox { score: Some(0.7), ..big_face(0.7, 0.2) }];
        assert_eq!(resolve_person(&group, (1600, 1200), &mut FakeModel::new(0.9)).unwrap().outcome, Outcome::Found);
        // A face row from before scores were kept (None) is never held against it.
        assert_eq!(at(FaceBox { score: None, ..big_face(0.4, 0.2) }).outcome, Outcome::Found);
    }

    /// Rates its cuts in the order they are asked for: the first is the face and torso, the second the nearer chest click.
    struct ByCall {
        rates: Vec<f32>,
        calls: Vec<Vec<(f32, f32, bool)>>,
    }
    impl ClickModel for ByCall {
        fn cut(&mut self, clicks: &[(f32, f32, bool)]) -> Result<Cut, String> {
            let rate = self.rates[self.calls.len().min(self.rates.len() - 1)];
            self.calls.push(clicks.to_vec());
            FakeModel::new(rate).cut(clicks)
        }
    }

    #[test]
    fn a_moderately_sure_two_click_cut_is_retried_with_a_nearer_chest_click_and_the_surer_cut_kept() {
        let face = [big_face(0.4, 0.2)];
        let mut spoiled = ByCall { rates: vec![0.65, 0.92], calls: Vec::new() };
        let r = resolve_person(&face, (1600, 1200), &mut spoiled).unwrap();
        assert_eq!(spoiled.calls.len(), 2);
        let (far, near) = (spoiled.calls[0][1], spoiled.calls[1][1]);
        assert!(near.1 < far.1 && (near.1 - (0.245 + 0.09)).abs() < 1e-4, "the retry clicks one face-height below the face centre: {near:?}");
        assert_eq!(r.outcome, Outcome::Found, "the chest-click cut was sure: {r:?}");
        let mut worse = ByCall { rates: vec![0.65, 0.5], calls: Vec::new() };
        let r = resolve_person(&face, (1600, 1200), &mut worse).unwrap();
        assert_eq!(r.outcome, Outcome::Check, "the first cut stays when it is the better one: {r:?}");
        let mut sure = ByCall { rates: vec![0.9, 0.95], calls: Vec::new() };
        resolve_person(&face, (1600, 1200), &mut sure).unwrap();
        assert_eq!(sure.calls.len(), 1, "a sure cut costs no second decode");
    }

    #[test]
    fn a_cut_between_the_two_confidence_limits_is_used_but_asks_for_a_look() {
        let mut model = FakeModel::new(0.65);
        let r = resolve_person(&[big_face(0.4, 0.2)], (1600, 1200), &mut model).unwrap();
        assert_eq!(r.outcome, Outcome::Check);
        assert!(r.reason.as_deref().unwrap().contains("only moderately sure of 1 person"));
        assert!(r.mask.is_some());
        let mut model = FakeModel::new(0.75);
        assert_eq!(resolve_person(&[big_face(0.4, 0.2)], (1600, 1200), &mut model).unwrap().outcome, Outcome::Found);
    }

    #[test]
    fn no_usable_face_is_not_found_without_asking_the_model() {
        let mut model = FakeModel::new(0.9);
        let r = resolve_person(&[], (1600, 1200), &mut model).unwrap();
        assert_eq!(r.outcome, Outcome::NotFound);
        let tiny = FaceBox { x: 0.5, y: 0.5, w: 0.005, h: 0.007, score: Some(0.9) };
        let r = resolve_person(&[tiny], (1600, 1200), &mut model).unwrap();
        assert_eq!(r.outcome, Outcome::NotFound);
        assert!(model.seen.is_empty());
    }

    #[test]
    fn a_model_failure_reaches_the_caller_unchanged() {
        let mut model = FakeModel::new(0.9);
        model.fail = true;
        assert_eq!(resolve_person(&[big_face(0.4, 0.2)], (1600, 1200), &mut model).err().as_deref(), Some("the click-select model is not installed"));
    }

    #[test]
    fn a_mask_over_most_of_the_frame_is_flagged() {
        let mut model = FakeModel::new(0.9);
        model.half = 0.45;
        let r = resolve_person(&[big_face(0.45, 0.05)], (1600, 1200), &mut model).unwrap();
        assert_eq!(r.outcome, Outcome::Check);
        assert!(r.reason.as_deref().unwrap().contains("covers over 60 %"));
    }

    #[test]
    fn decode_logits_undoes_encode_logits() {
        let mut logits = vec![-LOGIT_RANGE; RASTER * RASTER];
        logits[RASTER * 10 + 20] = LOGIT_RANGE;
        logits[RASTER * 11 + 20] = 0.0;
        let back = decode_logits(&encode_logits(&logits)).unwrap();
        assert!((back[RASTER * 10 + 20] - LOGIT_RANGE).abs() < 0.05);
        assert!((back[RASTER * 11 + 20]).abs() < 0.05);
        assert!((back[0] + LOGIT_RANGE).abs() < 0.05);
        assert!(decode_logits("not base64!").is_err());
    }

    /// Evaluation harness, not a regression test: `EMULSION_ADAPTIVE_EVAL_IN=<dir of photos> EMULSION_ADAPTIVE_EVAL_OUT=<dir>
    /// cargo test --lib adaptive_eval -- --ignored --nocapture` writes, per photograph, the sky and shadow masks as PNGs and prints
    /// one JSON line of outcomes. Used to compare the port with `RFC-0033-appendix/skyheur.py` and to look at the masks by eye.
    #[test]
    #[ignore]
    fn adaptive_eval() {
        let (Ok(inp), Ok(out)) = (std::env::var("EMULSION_ADAPTIVE_EVAL_IN"), std::env::var("EMULSION_ADAPTIVE_EVAL_OUT")) else { return };
        std::fs::create_dir_all(&out).unwrap();
        let mut names: Vec<_> = std::fs::read_dir(&inp).unwrap().filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("jpg") || x.eq_ignore_ascii_case("jpeg"))).collect();
        names.sort();
        for p in names {
            let stem = p.file_stem().unwrap().to_string_lossy().to_string();
            let img = image::open(&p).unwrap().to_rgb8();
            let a = analyse_sky(&img);
            if !a.mask.is_empty() {
                GrayImage::from_raw(a.w as u32, a.h as u32, a.mask.iter().map(|&v| if v { 255 } else { 0 }).collect()).unwrap().save(format!("{out}/{stem}.sky.png")).unwrap();
            }
            let sky = resolve_sky(&img);
            let sh = resolve_shadows(&img);
            if let Some(m) = &sky.mask {
                let l = decode(m);
                GrayImage::from_raw(RASTER as u32, RASTER as u32, l.iter().map(|&v| ((v + 4.0) / 8.0 * 255.0).round() as u8).collect()).unwrap().save(format!("{out}/{stem}.skyraster.png")).unwrap();
            }
            println!("{}", json!({ "file": stem, "seen": a.seen_share, "top": a.top_touch, "holes": a.holes_share, "luma": a.mean_luma,
                "sky": { "outcome": sky.outcome, "coverage": sky.coverage, "reason": sky.reason },
                "shadows": { "outcome": sh.outcome, "coverage": sh.coverage, "reason": sh.reason, "mask": sh.mask } }));
        }
    }

    /// `EMULSION_ADAPTIVE_FACES_JSON=<yunet_scan.json: [{file, size: [w, h], faces: [[x, y, w, h, score]]}]>
    /// EMULSION_ADAPTIVE_EVAL_OUT=<dir> cargo test --lib adaptive_eval_faces -- --ignored --nocapture`: the face resolver on a scan.
    #[test]
    #[ignore]
    fn adaptive_eval_faces() {
        let (Ok(scan), Ok(out)) = (std::env::var("EMULSION_ADAPTIVE_FACES_JSON"), std::env::var("EMULSION_ADAPTIVE_EVAL_OUT")) else { return };
        std::fs::create_dir_all(&out).unwrap();
        let rows: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(scan).unwrap()).unwrap();
        for r in rows {
            let stem = r["file"].as_str().unwrap().rsplit_once('.').unwrap().0.to_string();
            let size = (r["size"][0].as_u64().unwrap() as u32, r["size"][1].as_u64().unwrap() as u32);
            let faces: Vec<FaceBox> = r["faces"].as_array().unwrap().iter().map(|f| {
                let v: Vec<f32> = f.as_array().unwrap().iter().map(|x| x.as_f64().unwrap() as f32).collect();
                FaceBox { x: v[0], y: v[1], w: v[2], h: v[3], score: v.get(4).copied() }
            }).collect();
            let res = resolve_faces(&faces, size);
            if let Some(m) = &res.mask {
                let l = decode(m);
                GrayImage::from_raw(RASTER as u32, RASTER as u32, l.iter().map(|&v| ((v + 4.0) / 8.0 * 255.0).round() as u8).collect()).unwrap().save(format!("{out}/{stem}.faceraster.png")).unwrap();
            }
            println!("{}", json!({ "file": stem, "n": faces.len(), "outcome": res.outcome, "coverage": res.coverage, "reason": res.reason }));
        }
    }

    // ---- shadows

    #[test]
    fn shadows_follow_the_photographs_own_dark_end() {
        // Left third dark (luma ~0.1), the rest bright: the darkest quarter is the dark third.
        let img = RgbImage::from_fn(300, 200, |x, _| if x < 100 { Rgb([25, 25, 25]) } else { Rgb([200, 200, 200]) });
        let r = resolve_shadows(&img);
        assert_eq!(r.outcome, Outcome::Found);
        assert!((r.coverage - 0.333).abs() < 0.02, "coverage {}", r.coverage);
        let m = r.mask.unwrap();
        assert_eq!(m["op"], "luminance_range_mask");
        assert_eq!(m["basis"], "original", "the range is measured on the original, so the mask must measure it there");
        assert!(m["rangeMax"].as_f64().unwrap() <= SHADOW_CAP as f64 * 100.0 + 0.1);
    }

    #[test]
    fn the_shadows_mask_selects_the_same_pixels_whatever_is_edited_after_it() {
        // The resolver's own mask, through the real CPU renderer: a dark left third, a bright rest.
        use crate::catalog::EditStack;
        use crate::develop_engine::apply_edit_stack;
        let img = RgbImage::from_fn(30, 20, |x, _| if x < 10 { Rgb([25, 25, 25]) } else { Rgb([200, 200, 200]) });
        let mut mask = resolve_shadows(&img).mask.unwrap();
        for (k, v) in [("id", json!("s")), ("exposure", json!(-2.0)), ("contrast", json!(0.0)), ("saturation", json!(0.0))] {
            mask[k] = v;
        }
        let selected = |global_ev: f64, with_mask: bool| {
            let mut out = img.clone();
            let mut ops = vec![json!({ "op": "exposure", "value": global_ev })];
            if with_mask {
                ops.push(mask.clone());
            }
            apply_edit_stack(&mut out, &EditStack { schema_version: 1, ops });
            out
        };
        for ev in [-1.0, 0.0, 1.0, 2.0, 3.0, 4.0] {
            let (with, without) = (selected(ev, true), selected(ev, false));
            for x in [0u32, 9, 10, 29] {
                let changed = with.get_pixel(x, 5) != without.get_pixel(x, 5);
                assert_eq!(changed, x < 10, "pixel {x} at global {ev} EV");
            }
        }
    }

    #[test]
    fn shadows_not_found_in_a_bright_frame_or_a_night_frame() {
        let bright = RgbImage::from_pixel(120, 80, Rgb([180, 190, 200]));
        assert_eq!(resolve_shadows(&bright).outcome, Outcome::NotFound);
        let night = RgbImage::from_fn(120, 80, |x, _| if x < 90 { Rgb([8, 8, 10]) } else { Rgb([200, 200, 200]) });
        let r = resolve_shadows(&night);
        assert_eq!(r.outcome, Outcome::NotFound);
        assert!(r.reason.unwrap().contains("deep shadow"));
    }

    #[test]
    fn a_sliver_of_shadow_is_flagged_for_a_check() {
        let img = RgbImage::from_fn(300, 100, |x, _| if x < 18 { Rgb([20, 20, 20]) } else { Rgb([190, 190, 190]) });
        let r = resolve_shadows(&img);
        assert_eq!(r.outcome, Outcome::Check, "{r:?}");
        assert!(r.mask.is_some());
    }

    // ---- faces

    fn face(x: f32, y: f32, w: f32, h: f32) -> FaceBox {
        FaceBox { x, y, w, h, score: Some(0.9) }
    }

    #[test]
    fn no_faces_is_not_found() {
        assert_eq!(resolve_faces(&[], (4000, 3000)).outcome, Outcome::NotFound);
    }

    #[test]
    fn a_face_becomes_a_soft_ellipse_in_one_raster() {
        let r = resolve_faces(&[face(0.4, 0.2, 0.15, 0.2)], (4000, 3000));
        assert_eq!(r.outcome, Outcome::Found);
        let m = r.mask.unwrap();
        assert_eq!(m["op"], "segment_mask");
        let l = decode(&m);
        assert!(at(&l, 0.475, 0.30) > 3.0, "centre should be saturated inside");
        assert!(at(&l, 0.05, 0.9) < -3.9, "far corner outside");
        assert!(r.coverage > 0.02 && r.coverage < 0.12, "coverage {}", r.coverage);
    }

    #[test]
    fn duplicate_catalog_rows_are_one_face() {
        let one = resolve_faces(&[face(0.4, 0.2, 0.15, 0.2)], (4000, 3000));
        let dup = resolve_faces(&[face(0.4, 0.2, 0.15, 0.2), face(0.401, 0.2, 0.15, 0.2)], (4000, 3000));
        assert_eq!(one.coverage, dup.coverage);
    }

    #[test]
    fn faces_under_the_pixel_floor_are_dropped_by_pixels_not_by_share() {
        // 20 px at a 1,600 px frame is under the floor; 40 px is over it, though both are a tiny share of the frame.
        let tiny = face(0.5, 0.5, 20.0 / 1600.0, 20.0 / 1200.0);
        let small = face(0.5, 0.5, 40.0 / 1600.0, 40.0 / 1200.0);
        assert_eq!(resolve_faces(&[tiny], (1600, 1200)).outcome, Outcome::NotFound);
        assert_eq!(resolve_faces(&[small], (1600, 1200)).outcome, Outcome::Found);
        // The same face on a 6,400 px frame is the same 40 px at the reference size.
        assert_eq!(resolve_faces(&[small], (6400, 4800)).outcome, Outcome::Found);
    }

    #[test]
    fn fifty_nine_faces_are_one_mask() {
        let faces: Vec<FaceBox> = (0..59).map(|i| face(0.04 + 0.15 * (i % 6) as f32, 0.05 + 0.09 * (i / 6) as f32, 0.04, 0.05)).collect();
        let r = resolve_faces(&faces, (6000, 4000));
        assert_eq!(r.outcome, Outcome::Found, "{r:?}");
        let l = decode(&r.mask.unwrap());
        assert!(at(&l, 0.04 + 0.02, 0.05 + 0.025) > 2.0);
        assert!(at(&l, 0.04 + 0.15 * 4.0 + 0.02, 0.05 + 0.09 * 9.0 + 0.025) > 2.0, "the last face is in the same raster");
    }

    #[test]
    fn a_statue_sized_face_or_a_weak_score_asks_for_a_check() {
        let big = resolve_faces(&[face(0.1, 0.05, 0.7, 0.8)], (4000, 3000));
        assert_eq!(big.outcome, Outcome::Check);
        assert!(big.reason.unwrap().contains("covers over"));
        let weak = resolve_faces(&[FaceBox { score: Some(0.62), ..face(0.4, 0.2, 0.15, 0.2) }], (4000, 3000));
        assert_eq!(weak.outcome, Outcome::Check);
        let crowd: Vec<FaceBox> = (0..6).map(|i| FaceBox { score: Some(if i == 0 { 0.62 } else { 0.9 }), ..face(0.05 + 0.15 * i as f32, 0.3, 0.06, 0.08) }).collect();
        assert_eq!(resolve_faces(&crowd, (4000, 3000)).outcome, Outcome::Found, "one weak face in a crowd is not a reason to flag");
        let unscored = resolve_faces(&[FaceBox { score: None, ..face(0.4, 0.2, 0.15, 0.2) }], (4000, 3000));
        assert_eq!(unscored.outcome, Outcome::Found, "the catalog keeps no scores, so a missing score is not a reason to flag");
    }

    // ---- sky

    #[test]
    fn a_smooth_top_over_a_textured_ground_is_sky() {
        let mut seed = 7u32;
        let img = RgbImage::from_fn(300, 200, |x, y| {
            if y < 80 {
                Rgb([60 + (y / 4) as u8, 120 + (y / 4) as u8, 220])
            } else {
                let _ = x;
                Rgb([noise(&mut seed), noise(&mut seed), noise(&mut seed)])
            }
        });
        let r = resolve_sky(&img);
        assert_eq!(r.outcome, Outcome::Check, "{r:?}");
        assert!(r.reason.as_deref().unwrap().contains("without a model"));
        assert!((r.coverage - 0.4).abs() < 0.05, "coverage {}", r.coverage);
        let l = decode(&r.mask.unwrap());
        assert!(at(&l, 0.5, 0.1) > 3.0);
        assert!(at(&l, 0.5, 0.8) < -3.0);
    }

    /// A stand-in model that cuts "the sky" as everything above `edge` (a share of the frame's height).
    struct SkyModel {
        edge: f32,
        iou: f32,
        fail: bool,
        seen: Vec<Vec<(f32, f32, bool)>>,
    }
    impl ClickModel for SkyModel {
        fn cut(&mut self, clicks: &[(f32, f32, bool)]) -> Result<Cut, String> {
            if self.fail {
                return Err("helper died".to_string());
            }
            self.seen.push(clicks.to_vec());
            let logits = (0..RASTER * RASTER).map(|i| if ((i / RASTER) as f32 + 0.5) / (RASTER as f32) < self.edge { LOGIT_RANGE } else { -LOGIT_RANGE }).collect();
            Ok(Cut { iou: self.iou, logits })
        }
    }

    fn sky_over_ground() -> RgbImage {
        let mut seed = 7u32;
        RgbImage::from_fn(300, 200, |_, y| if y < 80 { Rgb([60 + (y / 4) as u8, 120 + (y / 4) as u8, 220]) } else { Rgb([noise(&mut seed), noise(&mut seed), noise(&mut seed)]) })
    }

    #[test]
    fn a_model_that_agrees_with_the_colour_rule_makes_the_sky_found_and_its_cut_the_mask() {
        let mut model = SkyModel { edge: 0.42, iou: 0.9, fail: false, seen: Vec::new() };
        let r = resolve_sky_with_model(&sky_over_ground(), &mut model);
        assert_eq!(r.outcome, Outcome::Found, "{r:?}");
        assert!(r.reason.is_none());
        assert!((r.coverage - 0.42).abs() < 0.02, "the model's own coverage: {}", r.coverage);
        let clicks = &model.seen[0];
        let (pos, neg): (Vec<&(f32, f32, bool)>, Vec<&(f32, f32, bool)>) = clicks.iter().partition(|c| c.2);
        assert_eq!(pos.len(), 3, "one positive per band of columns: {clicks:?}");
        assert!(pos.iter().all(|c| c.1 < 0.4), "positives are in the sky");
        assert_eq!(neg.len(), 1);
        assert!(neg[0].1 > 0.75, "the negative is in the lowest quarter: {neg:?}");
    }

    #[test]
    fn close_inside_fills_small_holes_and_keeps_the_outline() {
        let mut l = vec![-LOGIT_RANGE; RASTER * RASTER];
        for y in 0..100 {
            for x in 0..RASTER {
                l[y * RASTER + x] = LOGIT_RANGE;
            }
        }
        for (x, y) in [(50, 40), (51, 40), (52, 41), (120, 60), (121, 61), (122, 61), (200, 10)] {
            l[y * RASTER + x] = -LOGIT_RANGE; // a scatter of 1-3 texel holes
        }
        let closed = close_inside(&l, SKY_CLOSE_RADIUS);
        for (x, y) in [(50, 40), (51, 40), (52, 41), (120, 60), (121, 61), (200, 10)] {
            assert!(closed[y * RASTER + x] > 0.0, "hole at ({x},{y}) closed");
        }
        assert!(closed[150 * RASTER + 128] < -3.0, "far below the sky stays outside");
        assert!(closed[60 * RASTER + 128] > 3.0, "the sky stays inside");
        assert!(closed[104 * RASTER + 128] < 0.0, "the lower edge did not creep down (4 texels below it)");
    }

    #[test]
    fn a_model_that_outlines_the_sky_differently_is_flagged_with_the_overlap() {
        let mut model = SkyModel { edge: 0.75, iou: 0.9, fail: false, seen: Vec::new() };
        let r = resolve_sky_with_model(&sky_over_ground(), &mut model);
        assert_eq!(r.outcome, Outcome::Check);
        assert!(r.reason.as_deref().unwrap().contains("outline the sky differently"), "{r:?}");
        assert!((r.coverage - 0.75).abs() < 0.02, "the mask is still the model's");
    }

    #[test]
    fn an_unsure_or_failing_model_leaves_the_colour_rule_result_alone() {
        let plain = resolve_sky(&sky_over_ground());
        for (iou, fail) in [(0.5, false), (0.9, true)] {
            let mut model = SkyModel { edge: 0.42, iou, fail, seen: Vec::new() };
            let r = resolve_sky_with_model(&sky_over_ground(), &mut model);
            assert_eq!(r.outcome, plain.outcome);
            assert_eq!(r.reason, plain.reason);
            assert_eq!(r.coverage, plain.coverage);
        }
    }

    #[test]
    fn no_sky_for_the_colour_rule_means_the_model_is_not_asked() {
        let mut seed = 3u32;
        let all_texture = RgbImage::from_fn(240, 160, |_, _| Rgb([noise(&mut seed), noise(&mut seed), noise(&mut seed)]));
        let mut model = SkyModel { edge: 0.4, iou: 0.9, fail: false, seen: Vec::new() };
        assert_eq!(resolve_sky_with_model(&all_texture, &mut model).outcome, Outcome::NotFound);
        assert!(model.seen.is_empty());
    }

    #[test]
    fn no_smooth_region_on_top_is_not_sky() {
        let mut seed = 3u32;
        let all_texture = RgbImage::from_fn(240, 160, |_, _| Rgb([noise(&mut seed), noise(&mut seed), noise(&mut seed)]));
        assert_eq!(resolve_sky(&all_texture).outcome, Outcome::NotFound);
        // A smooth patch at the bottom (a lake) does not reach the top edge.
        let mut seed = 5u32;
        let lake = RgbImage::from_fn(240, 160, |_, y| if y > 100 { Rgb([40, 90, 160]) } else { Rgb([noise(&mut seed), noise(&mut seed), noise(&mut seed)]) });
        assert_eq!(resolve_sky(&lake).outcome, Outcome::NotFound);
    }

    #[test]
    fn a_dark_sky_is_flagged() {
        let mut seed = 11u32;
        let img = RgbImage::from_fn(300, 200, |_, y| if y < 100 { Rgb([8, 10, 28]) } else { Rgb([noise(&mut seed), noise(&mut seed), noise(&mut seed)]) });
        let r = resolve_sky(&img);
        assert_eq!(r.outcome, Outcome::Check, "{r:?}");
        assert!(r.reason.unwrap().contains("dark sky"));
    }

    #[test]
    fn box_mean_and_gradient_match_numpy_conventions() {
        let a: Vec<f32> = (0..9).map(|v| v as f32).collect(); // 3x3: 0..8 row-major
        let m = box_mean(&a, 3, 3, 1);
        assert!((m[4] - 4.0).abs() < 1e-5);
        assert!((m[0] - (0.0 + 0.0 + 1.0 + 0.0 + 0.0 + 1.0 + 3.0 + 3.0 + 4.0) / 9.0).abs() < 1e-5, "edge replication at the corner: {}", m[0]);
        let (gy, gx) = gradient(&a, 3, 3);
        assert_eq!((gx[0], gx[1], gx[2]), (1.0, 1.0, 1.0));
        assert_eq!((gy[0], gy[3], gy[6]), (3.0, 3.0, 3.0));
    }
}
