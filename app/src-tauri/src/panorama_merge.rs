//! Panorama merge (M5, RFC-0004): feature-based homography stitch of N
//! overlapping photos into one wide composite. Pure image processing, no
//! catalog/SQLite dependency -- `lib.rs`'s `merge_panorama` command
//! resolves catalog rows into file paths and re-catalogs the result; this
//! module only knows about pixels and homographies.
//!
//! Deliberately NOT implemented (see RFC-0004 §2 for the full reasoning):
//! multi-row/2D grids, cylindrical/spherical projection, automatic
//! pairwise-order discovery, bundle adjustment, rotation/scale-invariant
//! descriptors (SIFT/ORB), cross-frame exposure/color compensation,
//! seam-optimized (graph-cut) blending, and auto-crop of the output.
//!
//! Hand-rolled against plain `f32`/`image::RgbImage` throughout, not built
//! on a CV library -- a pass at using the `imageproc` crate (corner
//! detection + projective warp) was tried and reverted: its transitive
//! dependency tree (nalgebra, glam, SIMD backends) is a lot of weight for
//! two well-known, compact, directly-testable algorithms (Harris corners,
//! DLT homography via Gaussian elimination), matching this codebase's own
//! precedent of hand-rolling HDR merge's MTB alignment (`hdr_merge.rs`)
//! instead of reaching for a library.

use crate::source_decode;
use image::RgbImage;
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum PanoramaError {
    #[error("panorama merge needs at least 2 images, got {0}")]
    NotEnoughFrames(usize),
    #[error("could not decode {0}: {1}")]
    Decode(String, String),
    #[error("frames {0} and {1} don't overlap enough to stitch (got {2} inliers, need at least {3})")]
    PoorOverlap(usize, usize, usize, usize),
    #[error("could not invert the homography relating frames {0} and {1} -- degenerate alignment")]
    DegenerateHomography(usize, usize),
    #[error("stitched canvas would be {0}x{1}, exceeding the {2}x{2} sanity limit -- check frame order/overlap")]
    CanvasTooLarge(u32, u32, u32),
}

/// Result of a successful stitch: the composited image plus enough
/// provenance for the caller to record a `panorama_merge_sources` row per
/// input frame (RFC-0004 §3.6). `homographies[i]` maps a point in frame
/// `i`'s own pixel coordinates into the reference frame's coordinate
/// system (`reference_idx`), *before* the canvas's own translation offset
/// -- pure provenance, not reused by this module once `stitch` returns.
pub struct StitchedImage {
    pub image: RgbImage,
    /// Not read by `lib.rs` today (which frame is identity is already
    /// implicit in `homographies` itself) -- kept as real, ready-for-a-
    /// future-"show panorama sources"-UI provenance, same "no UI trigger
    /// yet" precedent as `Catalog::get_hdr_merge_sources`.
    #[allow(dead_code)]
    pub reference_idx: usize,
    pub homographies: Vec<[f32; 9]>,
}

// ---------------------------------------------------------------------
// 3x3 homography algebra (row-major). A private type alias for internal
// readability only -- public signatures spell out `[f32; 9]` directly so
// the alias itself never needs to be `pub`.
// ---------------------------------------------------------------------

type Mat3 = [f32; 9];

fn identity() -> Mat3 {
    [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]
}

fn matmul(a: &Mat3, b: &Mat3) -> Mat3 {
    let mut out = [0.0f32; 9];
    for r in 0..3 {
        for c in 0..3 {
            out[r * 3 + c] = (0..3).map(|k| a[r * 3 + k] * b[k * 3 + c]).sum();
        }
    }
    out
}

/// Closed-form cofactor/adjugate inverse -- exact and cheap for the fixed
/// 3x3 case, no need for a general linear-algebra crate.
fn invert(m: &Mat3) -> Option<Mat3> {
    let det = m[0] * (m[4] * m[8] - m[5] * m[7]) - m[1] * (m[3] * m[8] - m[5] * m[6])
        + m[2] * (m[3] * m[7] - m[4] * m[6]);
    if det.abs() < 1e-9 {
        return None;
    }
    let inv_det = 1.0 / det;
    Some([
        (m[4] * m[8] - m[5] * m[7]) * inv_det,
        (m[2] * m[7] - m[1] * m[8]) * inv_det,
        (m[1] * m[5] - m[2] * m[4]) * inv_det,
        (m[5] * m[6] - m[3] * m[8]) * inv_det,
        (m[0] * m[8] - m[2] * m[6]) * inv_det,
        (m[2] * m[3] - m[0] * m[5]) * inv_det,
        (m[3] * m[7] - m[4] * m[6]) * inv_det,
        (m[1] * m[6] - m[0] * m[7]) * inv_det,
        (m[0] * m[4] - m[1] * m[3]) * inv_det,
    ])
}

/// Projective apply: `(x, y) -> (x', y')` via `h`, dividing through by the
/// homogeneous `w` term.
fn apply(h: &Mat3, x: f32, y: f32) -> (f32, f32) {
    let w = h[6] * x + h[7] * y + h[8];
    ((h[0] * x + h[1] * y + h[2]) / w, (h[3] * x + h[4] * y + h[5]) / w)
}

// ---------------------------------------------------------------------
// Feature detection: Harris corners on a luma buffer (RFC-0004 §3.1).
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
struct Corner {
    x: u32,
    y: u32,
}

fn to_luma(img: &RgbImage) -> Vec<f32> {
    img.pixels()
        .map(|p| 0.2126 * p[0] as f32 + 0.7152 * p[1] as f32 + 0.0722 * p[2] as f32)
        .collect()
}

/// Sobel gradients, edge pixels clamped to the border (matches this
/// codebase's own clamp-at-edge convention, e.g. `develop_engine.rs`'s
/// sampling helpers) rather than padding with zeros, which would
/// otherwise inject a spurious high-gradient ring at the image border.
fn sobel_gradients(luma: &[f32], width: u32, height: u32) -> (Vec<f32>, Vec<f32>) {
    let (w, h) = (width as i32, height as i32);
    let get = |x: i32, y: i32| -> f32 { luma[(y.clamp(0, h - 1) * w + x.clamp(0, w - 1)) as usize] };
    let mut ix = vec![0f32; (w * h) as usize];
    let mut iy = vec![0f32; (w * h) as usize];
    for y in 0..h {
        for x in 0..w {
            let gx = -get(x - 1, y - 1) - 2.0 * get(x - 1, y) - get(x - 1, y + 1) + get(x + 1, y - 1)
                + 2.0 * get(x + 1, y)
                + get(x + 1, y + 1);
            let gy = -get(x - 1, y - 1) - 2.0 * get(x, y - 1) - get(x + 1, y - 1) + get(x - 1, y + 1)
                + 2.0 * get(x, y + 1)
                + get(x + 1, y + 1);
            ix[(y * w + x) as usize] = gx;
            iy[(y * w + x) as usize] = gy;
        }
    }
    (ix, iy)
}

fn box_blur3(buf: &[f32], width: u32, height: u32) -> Vec<f32> {
    let (w, h) = (width as i32, height as i32);
    let get = |x: i32, y: i32| -> f32 { buf[(y.clamp(0, h - 1) * w + x.clamp(0, w - 1)) as usize] };
    let mut out = vec![0f32; (w * h) as usize];
    for y in 0..h {
        for x in 0..w {
            let mut sum = 0f32;
            for dy in -1..=1 {
                for dx in -1..=1 {
                    sum += get(x + dx, y + dy);
                }
            }
            out[(y * w + x) as usize] = sum / 9.0;
        }
    }
    out
}

const HARRIS_K: f32 = 0.04;
/// Non-maximum-suppression half-window (7x7 total).
const NMS_RADIUS: i32 = 3;
/// Bounds matching cost (§3.3) regardless of input resolution.
const MAX_CORNERS: usize = 500;

fn detect_corners(luma: &[f32], width: u32, height: u32) -> Vec<Corner> {
    let (ix, iy) = sobel_gradients(luma, width, height);
    let ixx: Vec<f32> = ix.iter().map(|v| v * v).collect();
    let iyy: Vec<f32> = iy.iter().map(|v| v * v).collect();
    let ixy: Vec<f32> = ix.iter().zip(iy.iter()).map(|(a, b)| a * b).collect();
    let sxx = box_blur3(&ixx, width, height);
    let syy = box_blur3(&iyy, width, height);
    let sxy = box_blur3(&ixy, width, height);

    let (w, h) = (width as i32, height as i32);
    let response: Vec<f32> = (0..sxx.len())
        .map(|i| {
            let det = sxx[i] * syy[i] - sxy[i] * sxy[i];
            let trace = sxx[i] + syy[i];
            det - HARRIS_K * trace * trace
        })
        .collect();

    let mut candidates: Vec<(u32, u32, f32)> = Vec::new();
    for y in 0..h {
        for x in 0..w {
            let r = response[(y * w + x) as usize];
            if r <= 0.0 {
                continue;
            }
            let mut is_max = true;
            'nms: for dy in -NMS_RADIUS..=NMS_RADIUS {
                for dx in -NMS_RADIUS..=NMS_RADIUS {
                    if dx == 0 && dy == 0 {
                        continue;
                    }
                    let (nx, ny) = (x + dx, y + dy);
                    if nx < 0 || ny < 0 || nx >= w || ny >= h {
                        continue;
                    }
                    if response[(ny * w + nx) as usize] > r {
                        is_max = false;
                        break 'nms;
                    }
                }
            }
            if is_max {
                candidates.push((x as u32, y as u32, r));
            }
        }
    }
    candidates.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap());
    candidates.truncate(MAX_CORNERS);
    candidates.into_iter().map(|(x, y, _)| Corner { x, y }).collect()
}

// ---------------------------------------------------------------------
// Descriptor: normalized fixed patch (RFC-0004 §3.2).
// ---------------------------------------------------------------------

const PATCH_RADIUS: i32 = 7;

fn extract_descriptor(luma: &[f32], width: u32, height: u32, corner: Corner) -> Option<Vec<f32>> {
    let (w, h) = (width as i32, height as i32);
    let (cx, cy) = (corner.x as i32, corner.y as i32);
    if cx - PATCH_RADIUS < 0 || cy - PATCH_RADIUS < 0 || cx + PATCH_RADIUS >= w || cy + PATCH_RADIUS >= h {
        return None;
    }
    let mut patch = Vec::with_capacity(((2 * PATCH_RADIUS + 1) * (2 * PATCH_RADIUS + 1)) as usize);
    for dy in -PATCH_RADIUS..=PATCH_RADIUS {
        for dx in -PATCH_RADIUS..=PATCH_RADIUS {
            patch.push(luma[((cy + dy) * w + (cx + dx)) as usize]);
        }
    }
    let mean: f32 = patch.iter().sum::<f32>() / patch.len() as f32;
    let variance: f32 = patch.iter().map(|v| (v - mean) * (v - mean)).sum::<f32>() / patch.len() as f32;
    let std_dev = variance.sqrt().max(1e-6);
    for v in patch.iter_mut() {
        *v = (*v - mean) / std_dev;
    }
    Some(patch)
}

struct FrameFeatures {
    corners: Vec<Corner>,
    descriptors: Vec<Vec<f32>>,
}

fn compute_features(img: &RgbImage) -> FrameFeatures {
    let luma = to_luma(img);
    let mut corners = Vec::new();
    let mut descriptors = Vec::new();
    for corner in detect_corners(&luma, img.width(), img.height()) {
        if let Some(descriptor) = extract_descriptor(&luma, img.width(), img.height(), corner) {
            corners.push(corner);
            descriptors.push(descriptor);
        }
    }
    FrameFeatures { corners, descriptors }
}

// ---------------------------------------------------------------------
// Matching: ratio-test nearest neighbor (RFC-0004 §3.3).
// ---------------------------------------------------------------------

const RATIO_TEST_THRESHOLD: f32 = 0.8;

fn ssd(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| (x - y) * (x - y)).sum()
}

/// Returns `(point_in_x, point_in_y)` pairs -- one per surviving `y`
/// corner matched back to its best `x` corner, direction chosen entirely
/// by which side the caller passes as `x`/`y`.
fn match_features(x_features: &FrameFeatures, y_features: &FrameFeatures) -> Vec<((f32, f32), (f32, f32))> {
    let mut matches = Vec::new();
    for (iy, dy) in y_features.descriptors.iter().enumerate() {
        let mut best = f32::MAX;
        let mut second = f32::MAX;
        let mut best_ix = None;
        for (ix, dx) in x_features.descriptors.iter().enumerate() {
            let d = ssd(dy, dx);
            if d < best {
                second = best;
                best = d;
                best_ix = Some(ix);
            } else if d < second {
                second = d;
            }
        }
        if let Some(ix) = best_ix {
            if best < RATIO_TEST_THRESHOLD * second {
                let px = x_features.corners[ix];
                let py = y_features.corners[iy];
                matches.push(((px.x as f32, px.y as f32), (py.x as f32, py.y as f32)));
            }
        }
    }
    matches
}

// ---------------------------------------------------------------------
// Homography estimation: 4-point DLT via Gaussian-eliminated normal
// equations, RANSAC-wrapped (RFC-0004 §3.4).
// ---------------------------------------------------------------------

/// Accumulates one correspondence's contribution to the 8x8 normal-
/// equations system for `dst ~= H . src` with `h33` fixed to 1 -- the
/// same accumulation works unchanged whether called for the minimal
/// 4-point case (RANSAC's own candidate solve) or the many-point
/// least-squares refit over an inlier set, so there's only one solver
/// path to test, not two.
fn accumulate_normal_equations(
    src: (f32, f32),
    dst: (f32, f32),
    ata: &mut [[f64; 8]; 8],
    atb: &mut [f64; 8],
) {
    let (x, y) = (src.0 as f64, src.1 as f64);
    let (xp, yp) = (dst.0 as f64, dst.1 as f64);
    let rows: [([f64; 8], f64); 2] =
        [([x, y, 1.0, 0.0, 0.0, 0.0, -x * xp, -y * xp], xp), ([0.0, 0.0, 0.0, x, y, 1.0, -x * yp, -y * yp], yp)];
    for (row, rhs) in rows {
        for i in 0..8 {
            atb[i] += row[i] * rhs;
            for j in 0..8 {
                ata[i][j] += row[i] * row[j];
            }
        }
    }
}

/// Plain Gaussian elimination with partial pivoting over an 8x8 system.
fn solve_8x8(mut ata: [[f64; 8]; 8], mut atb: [f64; 8]) -> Option<[f64; 8]> {
    for col in 0..8 {
        let mut pivot_row = col;
        let mut max_val = ata[col][col].abs();
        for r in (col + 1)..8 {
            if ata[r][col].abs() > max_val {
                max_val = ata[r][col].abs();
                pivot_row = r;
            }
        }
        if max_val < 1e-12 {
            return None;
        }
        ata.swap(col, pivot_row);
        atb.swap(col, pivot_row);
        let pivot = ata[col][col];
        for r in (col + 1)..8 {
            let factor = ata[r][col] / pivot;
            if factor == 0.0 {
                continue;
            }
            for c in col..8 {
                ata[r][c] -= factor * ata[col][c];
            }
            atb[r] -= factor * atb[col];
        }
    }
    let mut x = [0.0f64; 8];
    for row in (0..8).rev() {
        let mut sum = atb[row];
        for c in (row + 1)..8 {
            sum -= ata[row][c] * x[c];
        }
        x[row] = sum / ata[row][row];
    }
    Some(x)
}

fn estimate_homography(pairs: &[((f32, f32), (f32, f32))]) -> Option<Mat3> {
    if pairs.len() < 4 {
        return None;
    }
    let mut ata = [[0f64; 8]; 8];
    let mut atb = [0f64; 8];
    for &(src, dst) in pairs {
        accumulate_normal_equations(src, dst, &mut ata, &mut atb);
    }
    let h = solve_8x8(ata, atb)?;
    Some([
        h[0] as f32, h[1] as f32, h[2] as f32, h[3] as f32, h[4] as f32, h[5] as f32, h[6] as f32, h[7] as f32, 1.0,
    ])
}

/// True if any 3 of the given (source-side) points are near-collinear --
/// a degenerate DLT input, checked only on the source side as a
/// deliberate simplification (RFC-0004 §3.4's own named limitation:
/// skipping a full SVD-based solve means an exactly-degenerate destination
/// configuration isn't separately guarded against, though it's a rare
/// case in practice for a real handheld sweep).
fn is_degenerate_sample(points: &[(f32, f32)]) -> bool {
    for i in 0..points.len() {
        for j in (i + 1)..points.len() {
            for k in (j + 1)..points.len() {
                let area = (points[j].0 - points[i].0) * (points[k].1 - points[i].1)
                    - (points[k].0 - points[i].0) * (points[j].1 - points[i].1);
                if area.abs() < 1.0 {
                    return true;
                }
            }
        }
    }
    false
}

/// Small deterministic xorshift64* PRNG -- RANSAC's own randomness has no
/// need to be cryptographic or even seeded per-run: a fixed seed keeps
/// this module's behavior (and its unit tests) fully reproducible, and
/// avoids a new `rand` crate dependency for what's a handful of bounded
/// random draws.
struct SimpleRng(u64);

impl SimpleRng {
    fn new(seed: u64) -> Self {
        SimpleRng(seed | 1)
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn next_range(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }
}

fn pick_random_sample(
    pairs: &[((f32, f32), (f32, f32))],
    rng: &mut SimpleRng,
) -> Option<Vec<((f32, f32), (f32, f32))>> {
    for _ in 0..50 {
        let idxs = [
            rng.next_range(pairs.len()),
            rng.next_range(pairs.len()),
            rng.next_range(pairs.len()),
            rng.next_range(pairs.len()),
        ];
        let mut sorted = idxs;
        sorted.sort_unstable();
        if sorted.windows(2).any(|w| w[0] == w[1]) {
            continue;
        }
        let src_points: Vec<(f32, f32)> = idxs.iter().map(|&i| pairs[i].0).collect();
        if is_degenerate_sample(&src_points) {
            continue;
        }
        return Some(idxs.iter().map(|&i| pairs[i]).collect());
    }
    None
}

const RANSAC_ITERATIONS: usize = 1000;
const RANSAC_INLIER_THRESHOLD: f32 = 3.0;
const RANSAC_MIN_INLIERS: usize = 8;
const RANSAC_MIN_INLIER_FRACTION: f32 = 0.4;

/// Fixed 4-point minimal solve + reprojection-error inlier scoring,
/// repeated for `RANSAC_ITERATIONS`, then a least-squares refit (same
/// `estimate_homography` machinery, now over the full inlier set) --
/// RFC-0004 §3.4.
fn ransac_homography(pairs: &[((f32, f32), (f32, f32))], seed: u64) -> Option<(Mat3, usize)> {
    if pairs.len() < 4 {
        return None;
    }
    let mut rng = SimpleRng::new(seed);
    let mut best: Option<(Mat3, usize)> = None;
    for _ in 0..RANSAC_ITERATIONS {
        let Some(sample) = pick_random_sample(pairs, &mut rng) else { continue };
        let Some(h) = estimate_homography(&sample) else { continue };
        let inliers = pairs
            .iter()
            .filter(|&&(src, dst)| {
                let (px, py) = apply(&h, src.0, src.1);
                ((px - dst.0).powi(2) + (py - dst.1).powi(2)).sqrt() < RANSAC_INLIER_THRESHOLD
            })
            .count();
        if best.is_none_or(|(_, best_count)| inliers > best_count) {
            best = Some((h, inliers));
        }
    }
    let (best_h, best_count) = best?;
    let inlier_pairs: Vec<_> = pairs
        .iter()
        .copied()
        .filter(|&(src, dst)| {
            let (px, py) = apply(&best_h, src.0, src.1);
            ((px - dst.0).powi(2) + (py - dst.1).powi(2)).sqrt() < RANSAC_INLIER_THRESHOLD
        })
        .collect();
    let refit = estimate_homography(&inlier_pairs).unwrap_or(best_h);
    Some((refit, best_count))
}

// ---------------------------------------------------------------------
// Canvas fitting + blending (RFC-0004 §3.5).
// ---------------------------------------------------------------------

/// Bilinear sample plus this pixel's own blend weight -- a triangular
/// ramp over `img`'s normalized x-position (peak at the center, zero at
/// the left/right edge), `None` when `(x, y)` falls outside `img`'s own
/// bounds. Floored well above zero so a pixel at the *outer* edge of the
/// whole canvas (covered by exactly one frame, at that frame's own edge)
/// still gets its real color back after the weighted-average division,
/// rather than a 0/0 producing a dropped pixel.
fn sample_weighted(img: &RgbImage, x: f32, y: f32) -> Option<([f32; 3], f32)> {
    let (width, height) = (img.width() as i32, img.height() as i32);
    if x < 0.0 || y < 0.0 || x > (width - 1) as f32 || y > (height - 1) as f32 {
        return None;
    }
    let x0 = x.floor() as i32;
    let y0 = y.floor() as i32;
    let x1 = (x0 + 1).min(width - 1);
    let y1 = (y0 + 1).min(height - 1);
    let fx = x - x0 as f32;
    let fy = y - y0 as f32;
    let p00 = img.get_pixel(x0 as u32, y0 as u32).0;
    let p10 = img.get_pixel(x1 as u32, y0 as u32).0;
    let p01 = img.get_pixel(x0 as u32, y1 as u32).0;
    let p11 = img.get_pixel(x1 as u32, y1 as u32).0;
    let mut color = [0f32; 3];
    for c in 0..3 {
        let top = p00[c] as f32 * (1.0 - fx) + p10[c] as f32 * fx;
        let bottom = p01[c] as f32 * (1.0 - fx) + p11[c] as f32 * fx;
        color[c] = top * (1.0 - fy) + bottom * fy;
    }
    let norm_x = if width > 1 { x / (width - 1) as f32 } else { 0.5 };
    let weight = (1.0 - (2.0 * norm_x - 1.0).abs()).max(1e-3);
    Some((color, weight))
}

// ---------------------------------------------------------------------
// Orchestration.
// ---------------------------------------------------------------------

/// RANSAC's own seed is fixed (see `SimpleRng`'s doc comment) but offset
/// per adjacent pair so two pairs in the same stitch don't draw an
/// identical random sequence.
const RANSAC_BASE_SEED: u64 = 0x9E37_79B9_7F4A_7C15;

/// Pure-buffer stitch pipeline -- no file I/O, so this is what the unit
/// tests below exercise directly against small synthetic frames. `stitch`
/// (below) is the thin decode-from-disk wrapper `lib.rs` actually calls,
/// mirroring `hdr_merge::merge_bracket`'s own "decode, then delegate to
/// pure buffers" layering.
fn stitch_images(images: &[RgbImage]) -> Result<StitchedImage, PanoramaError> {
    if images.len() < 2 {
        return Err(PanoramaError::NotEnoughFrames(images.len()));
    }
    let n = images.len();
    let features: Vec<FrameFeatures> = images.iter().map(compute_features).collect();

    // pairwise[i] maps frame (i+1)'s coords into frame i's coords.
    let mut pairwise: Vec<Mat3> = Vec::with_capacity(n - 1);
    for i in 0..(n - 1) {
        // `estimate_homography`/`ransac_homography` solve `dst ~= H . src`
        // from a pair's `(pair.0, pair.1)`, and `pairwise[i]` needs to map
        // frame (i+1) -> frame i, so `x_features` (the `src` side) must be
        // frame i+1 and `y_features` (the `dst` side) frame i -- i.e.
        // reversed from the natural "i, then i+1" reading order.
        let matches = match_features(&features[i + 1], &features[i]);
        let needed = ((RANSAC_MIN_INLIER_FRACTION * matches.len() as f32).ceil() as usize).max(RANSAC_MIN_INLIERS);
        match ransac_homography(&matches, RANSAC_BASE_SEED.wrapping_add(i as u64)) {
            Some((h, inliers)) if inliers >= needed => pairwise.push(h),
            Some((_, inliers)) => return Err(PanoramaError::PoorOverlap(i, i + 1, inliers, needed)),
            None => return Err(PanoramaError::PoorOverlap(i, i + 1, 0, needed)),
        }
    }

    // Reference = the middle frame, spreading accumulated perspective
    // distortion toward both ends rather than concentrating it at one
    // (RFC-0004 §3.4).
    let reference_idx = n / 2;
    let mut to_ref: Vec<Mat3> = vec![identity(); n];
    for k in (reference_idx + 1)..n {
        to_ref[k] = matmul(&to_ref[k - 1], &pairwise[k - 1]);
    }
    for k in (0..reference_idx).rev() {
        let inv = invert(&pairwise[k]).ok_or(PanoramaError::DegenerateHomography(k, k + 1))?;
        to_ref[k] = matmul(&to_ref[k + 1], &inv);
    }

    let mut min_x = f32::MAX;
    let mut max_x = f32::MIN;
    let mut min_y = f32::MAX;
    let mut max_y = f32::MIN;
    for (k, img) in images.iter().enumerate() {
        let (w, h) = (img.width() as f32, img.height() as f32);
        for &(cx, cy) in &[(0.0, 0.0), (w - 1.0, 0.0), (0.0, h - 1.0), (w - 1.0, h - 1.0)] {
            let (px, py) = apply(&to_ref[k], cx, cy);
            min_x = min_x.min(px);
            max_x = max_x.max(px);
            min_y = min_y.min(py);
            max_y = max_y.max(py);
        }
    }

    let canvas_width = (max_x - min_x).ceil() as u32 + 1;
    let canvas_height = (max_y - min_y).ceil() as u32 + 1;
    let input_dim_sum: u32 = images.iter().map(|img| img.width().max(img.height())).sum::<u32>().max(1);
    let sanity_limit = input_dim_sum.saturating_mul(4).max(4096);
    if canvas_width > sanity_limit || canvas_height > sanity_limit {
        return Err(PanoramaError::CanvasTooLarge(canvas_width, canvas_height, sanity_limit));
    }

    let (tx, ty) = (-min_x, -min_y);
    let mut inv_placements: Vec<Mat3> = Vec::with_capacity(n);
    for k in 0..n {
        inv_placements.push(invert(&to_ref[k]).ok_or(PanoramaError::DegenerateHomography(k, reference_idx))?);
    }

    let mut out = RgbImage::new(canvas_width, canvas_height);
    for cy in 0..canvas_height {
        for cx in 0..canvas_width {
            let ref_x = cx as f32 - tx;
            let ref_y = cy as f32 - ty;
            let mut acc = [0f32; 3];
            let mut total_weight = 0f32;
            for (k, img) in images.iter().enumerate() {
                let (sx, sy) = apply(&inv_placements[k], ref_x, ref_y);
                if let Some((color, weight)) = sample_weighted(img, sx, sy) {
                    for c in 0..3 {
                        acc[c] += color[c] * weight;
                    }
                    total_weight += weight;
                }
            }
            if total_weight > 0.0 {
                let px = [
                    (acc[0] / total_weight).round().clamp(0.0, 255.0) as u8,
                    (acc[1] / total_weight).round().clamp(0.0, 255.0) as u8,
                    (acc[2] / total_weight).round().clamp(0.0, 255.0) as u8,
                ];
                out.put_pixel(cx, cy, image::Rgb(px));
            }
        }
    }

    Ok(StitchedImage { image: out, reference_idx, homographies: to_ref })
}

/// Decodes each of `paths` (any format `source_decode` supports -- unlike
/// HDR merge, panorama stitching needs no linear/RAW-only decode) and
/// delegates to `stitch_images`.
pub fn stitch(paths: &[PathBuf]) -> Result<StitchedImage, PanoramaError> {
    if paths.len() < 2 {
        return Err(PanoramaError::NotEnoughFrames(paths.len()));
    }
    let images: Vec<RgbImage> = paths
        .iter()
        .map(|path| decode_to_rgb_image(path))
        .collect::<Result<_, _>>()?;
    stitch_images(&images)
}

fn decode_to_rgb_image(path: &Path) -> Result<RgbImage, PanoramaError> {
    let decoded = source_decode::decode_preview(path)
        .map_err(|e| PanoramaError::Decode(path.display().to_string(), e.to_string()))?;
    RgbImage::from_raw(decoded.width, decoded.height, decoded.rgb)
        .ok_or_else(|| PanoramaError::Decode(path.display().to_string(), "decoded buffer size mismatch".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checkerboard(width: u32, height: u32) -> RgbImage {
        let mut img = RgbImage::new(width, height);
        for y in 0..height {
            for x in 0..width {
                let cell = (x / 8 + y / 8) % 2;
                let v = if cell == 0 { 220u8 } else { 30u8 };
                img.put_pixel(x, y, image::Rgb([v, v, v]));
            }
        }
        img
    }

    /// A deterministic but non-repeating (unlike `checkerboard`'s own
    /// perfectly periodic pattern) texture: each 4x4 block gets its own
    /// pseudo-random gray value from a position hash. A perfectly
    /// periodic pattern gives every corner a near-identical descriptor to
    /// every other corner one period away, which starves the ratio test
    /// (§3.3) of any confident match at all -- found empirically while
    /// writing the end-to-end stitch test below, which needs frames with
    /// genuinely locally-unique texture, the same way a real photo has.
    fn textured_world(width: u32, height: u32) -> RgbImage {
        let mut img = RgbImage::new(width, height);
        for y in 0..height {
            for x in 0..width {
                let (bx, by) = (x / 4, y / 4);
                let h = bx.wrapping_mul(374_761_393).wrapping_add(by.wrapping_mul(668_265_263));
                let h = (h ^ (h >> 13)).wrapping_mul(2_654_435_761);
                let v = (h % 200 + 20) as u8;
                img.put_pixel(x, y, image::Rgb([v, v, v]));
            }
        }
        img
    }

    #[test]
    fn matmul_with_identity_is_a_no_op_and_invert_of_identity_is_identity() {
        let id = identity();
        assert_eq!(matmul(&id, &id), id);
        assert_eq!(invert(&id), Some(id));
    }

    #[test]
    fn invert_and_matmul_round_trip_a_general_homography_back_to_the_identity() {
        let m: Mat3 = [2.0, 0.0, 3.0, 0.0, 1.0, -1.0, 0.0004, -0.0002, 1.0];
        let inv = invert(&m).expect("well-conditioned matrix inverts");
        let round = matmul(&m, &inv);
        for (a, b) in round.iter().zip(identity().iter()) {
            assert!((a - b).abs() < 1e-3, "round-trip {:?} != identity", round);
        }
    }

    #[test]
    fn apply_matches_hand_computed_values_for_a_pure_translation_and_a_pure_scale() {
        let translate: Mat3 = [1.0, 0.0, 10.0, 0.0, 1.0, -5.0, 0.0, 0.0, 1.0];
        assert_eq!(apply(&translate, 3.0, 4.0), (13.0, -1.0));

        let scale: Mat3 = [2.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 1.0];
        assert_eq!(apply(&scale, 3.0, 4.0), (6.0, 8.0));
    }

    #[test]
    fn estimate_homography_recovers_a_known_projective_transform_from_four_points() {
        let h_true: Mat3 = [1.1, 0.05, 20.0, -0.02, 0.95, 10.0, 0.0004, -0.0002, 1.0];
        let src_points = [(0.0, 0.0), (100.0, 0.0), (0.0, 80.0), (100.0, 80.0)];
        let pairs: Vec<_> = src_points
            .iter()
            .map(|&(x, y)| {
                let dst = apply(&h_true, x, y);
                ((x, y), dst)
            })
            .collect();
        let h_est = estimate_homography(&pairs).expect("4 non-degenerate points solve exactly");
        for (a, b) in h_est.iter().zip(h_true.iter()) {
            assert!((a - b).abs() < 1e-2, "expected {:?}, got {:?}", h_true, h_est);
        }
    }

    #[test]
    fn estimate_homography_returns_none_for_fewer_than_four_points() {
        let pairs = [((0.0, 0.0), (1.0, 1.0)), ((1.0, 0.0), (2.0, 1.0))];
        assert!(estimate_homography(&pairs).is_none());
    }

    #[test]
    fn is_degenerate_sample_flags_three_collinear_points_but_not_a_generic_quad() {
        let collinear = [(0.0, 0.0), (10.0, 0.0), (20.0, 0.0), (5.0, 30.0)];
        assert!(is_degenerate_sample(&collinear));
        let generic = [(0.0, 0.0), (10.0, 0.0), (0.0, 10.0), (10.0, 10.0)];
        assert!(!is_degenerate_sample(&generic));
    }

    #[test]
    fn ransac_homography_ignores_outlier_correspondences() {
        let h_true: Mat3 = [1.0, 0.0, 15.0, 0.0, 1.0, -5.0, 0.0, 0.0, 1.0];
        let mut pairs = Vec::new();
        for i in 0..20 {
            let (x, y) = ((i as f32) * 10.0, (i as f32 % 5.0) * 8.0);
            pairs.push(((x, y), apply(&h_true, x, y)));
        }
        // Deliberate outliers, nowhere near the true transform.
        pairs.push(((5.0, 5.0), (500.0, 500.0)));
        pairs.push(((50.0, 50.0), (-200.0, 300.0)));

        let (h_est, inliers) = ransac_homography(&pairs, 42).expect("mostly-consistent correspondences solve");
        assert!(inliers >= 20, "expected >= 20 inliers, got {inliers}");
        for (a, b) in h_est.iter().zip(h_true.iter()) {
            assert!((a - b).abs() < 1.0, "expected {:?}, got {:?}", h_true, h_est);
        }
    }

    #[test]
    fn detect_corners_finds_a_strong_response_near_a_synthetic_squares_corner() {
        let (w, h) = (60u32, 60u32);
        let mut luma = vec![0.0f32; (w * h) as usize];
        for y in 20..40 {
            for x in 20..40 {
                luma[(y * w + x) as usize] = 255.0;
            }
        }
        let corners = detect_corners(&luma, w, h);
        assert!(!corners.is_empty(), "expected at least one corner on a square's edge");
        let near_corner = corners.iter().any(|c| {
            let (dx, dy) = (c.x as i32 - 20, c.y as i32 - 20);
            dx * dx + dy * dy <= 9
        });
        assert!(
            near_corner,
            "expected a detected corner near (20, 20), got {:?}",
            corners.iter().map(|c| (c.x, c.y)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn detect_corners_on_a_flat_field_returns_nothing() {
        let luma = vec![128.0f32; 40 * 40];
        assert!(detect_corners(&luma, 40, 40).is_empty());
    }

    #[test]
    fn to_ref_chaining_composes_pairwise_homographies_correctly_for_three_frames() {
        // pairwise[0] maps frame1 -> frame0; pairwise[1] maps frame2 -> frame1.
        let p01: Mat3 = [1.0, 0.0, 10.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
        let p12: Mat3 = [1.0, 0.0, 5.0, 0.0, 1.0, 3.0, 0.0, 0.0, 1.0];
        let pairwise = vec![p01, p12];
        let (n, reference_idx) = (3, 1);

        let mut to_ref: Vec<Mat3> = vec![identity(); n];
        for k in (reference_idx + 1)..n {
            to_ref[k] = matmul(&to_ref[k - 1], &pairwise[k - 1]);
        }
        for k in (0..reference_idx).rev() {
            let inv = invert(&pairwise[k]).unwrap();
            to_ref[k] = matmul(&to_ref[k + 1], &inv);
        }

        assert_eq!(to_ref[1], identity());
        for (a, b) in to_ref[2].iter().zip(p12.iter()) {
            assert!((a - b).abs() < 1e-4);
        }
        let expected0 = invert(&p01).unwrap();
        for (a, b) in to_ref[0].iter().zip(expected0.iter()) {
            assert!((a - b).abs() < 1e-4);
        }
    }

    #[test]
    fn sample_weighted_peaks_at_center_and_is_none_out_of_bounds() {
        let img = checkerboard(20, 20);
        assert!(sample_weighted(&img, -1.0, 5.0).is_none());
        assert!(sample_weighted(&img, 5.0, 25.0).is_none());
        let (_, center_weight) = sample_weighted(&img, 9.5, 10.0).unwrap();
        let (_, edge_weight) = sample_weighted(&img, 0.0, 10.0).unwrap();
        assert!(center_weight > edge_weight);
    }

    #[test]
    fn stitch_images_rejects_fewer_than_two_frames() {
        let Err(err) = stitch_images(&[checkerboard(40, 40)]) else {
            panic!("expected NotEnoughFrames");
        };
        assert!(matches!(err, PanoramaError::NotEnoughFrames(1)));
    }

    #[test]
    fn stitch_images_recovers_a_pure_horizontal_pan_from_two_overlapping_textured_frames() {
        let world = textured_world(180, 100);
        let frame_a = image::imageops::crop_imm(&world, 0, 0, 120, 100).to_image();
        let frame_b = image::imageops::crop_imm(&world, 60, 0, 120, 100).to_image();

        let stitched = stitch_images(&[frame_a, frame_b]).expect("a real, textured overlap should stitch cleanly");

        // Union of [0,120) and [60,180) is [0,180) -- canvas should match
        // that width (small slack for RANSAC/rounding), not either frame's
        // own width alone.
        assert!(
            (stitched.image.width() as i32 - 180).abs() <= 4,
            "expected canvas width close to 180, got {}",
            stitched.image.width()
        );
        assert!(
            (stitched.image.height() as i32 - 100).abs() <= 4,
            "expected canvas height close to 100, got {}",
            stitched.image.height()
        );

        // A pixel well inside frame A's exclusive region (not covered by
        // B at all) should pass through looking like the original world
        // pixel there, not some blended/garbage value.
        let world_px = world.get_pixel(10, 50);
        let stitched_px = stitched.image.get_pixel(10, 50);
        assert!(
            (world_px[0] as i32 - stitched_px[0] as i32).abs() < 40,
            "expected exclusive-region pixel to pass through, world={:?} stitched={:?}",
            world_px,
            stitched_px
        );
    }

    #[test]
    fn stitch_images_reports_poor_overlap_for_two_unrelated_flat_frames() {
        // Flat, featureless frames have no corners to match at all.
        let a = RgbImage::from_pixel(80, 80, image::Rgb([50, 50, 50]));
        let b = RgbImage::from_pixel(80, 80, image::Rgb([200, 200, 200]));
        let Err(err) = stitch_images(&[a, b]) else {
            panic!("expected PoorOverlap");
        };
        assert!(matches!(err, PanoramaError::PoorOverlap(0, 1, _, _)));
    }
}
