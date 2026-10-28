//! Region resolvers for adaptive presets (RFC-0035 §3.2, slice B1 of RFC-0033).
//!
//! A resolver looks at one photograph and returns `found`, `check` or `not_found` with the share of the
//! frame it covers and a reason, plus a ready-to-store mask op (no adjustments: the apply step adds those).
//! Pure functions over pixels and stored face boxes; the catalog and preview plumbing is in
//! `adaptive_commands.rs`. Every number below is a *starting value* from RFC-0033 §9-§10's small by-eye sets
//! and is judged again on photographs in B1 (RFC-0035 §4); the outcome words are the contract.

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

/// All the faces of one photograph in one `segment_mask`, any number of them. `frame` is the pixel size of the
/// image the boxes are relative to (only the aspect and the pixel floor use it).
pub fn resolve_faces(boxes: &[FaceBox], frame: (u32, u32)) -> RegionResult {
    if boxes.is_empty() {
        return RegionResult::not_found("no face in this photograph");
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
        return RegionResult::not_found(format!("{total} face(s), all under the size floor ({FACE_MIN_SIDE_PX:.0} px at a {FACE_REF_EDGE:.0} px frame)"));
    }

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
    if kept.len() <= FACE_FEW && kept.iter().any(|f| f.score.is_some_and(|s| s < FACE_LOW_SCORE)) {
        why.push("a face was detected with low confidence".to_string());
    }
    if why.is_empty() {
        RegionResult::with_mask(Outcome::Found, coverage, None, mask)
    } else {
        RegionResult::with_mask(Outcome::Check, coverage, Some(why.join("; ")), mask)
    }
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
