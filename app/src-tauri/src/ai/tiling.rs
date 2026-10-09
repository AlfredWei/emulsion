//! Tiled inference for full-size images (RFC-0027 §3.4): cut the image into overlapping square windows, run the
//! model on each, and blend the results back with a linear feather across the overlaps so no seam shows.
//!
//! Pure and model-agnostic (std only, shared with the helper by `#[path]`): the model is a closure from a
//! `tile x tile` CHW f32 tensor (0..1) to a tensor of the same shape, so the seam arithmetic is tested with
//! stand-in models and no weights.
//!
//! Windows are placed every `tile - overlap` pixels and the **last one is clamped to end at the image edge**
//! (more overlap there, never padding), so a window only reaches outside the image when the image is smaller
//! than one tile; that case reflects the image into the window. Each window's weight is a product of two 1-D
//! ramps that rise over `overlap` pixels on every side that has a neighbour and stay 1 at the image edge; the
//! result is `sum(w * out) / sum(w)`, which stays correct wherever windows overlap by more than `overlap`.

// Shared by `#[path]` with the helper binary: each side uses a different half of these items.
#![allow(dead_code)]

pub const TILE: usize = 512;
pub const OVERLAP: usize = 32;

/// Window start positions along one axis of length `len`: every `tile - overlap`, the last clamped to `len - tile`.
pub fn starts(len: usize, tile: usize, overlap: usize) -> Vec<usize> {
    assert!(tile > overlap * 2 && overlap > 0, "overlap must leave a flat middle in the window");
    if len <= tile {
        return vec![0];
    }
    let stride = tile - overlap;
    let mut v = Vec::new();
    let mut s = 0;
    while s + tile < len {
        v.push(s);
        s += stride;
    }
    v.push(len - tile);
    v
}

/// Number of model runs for a `w x h` image (for progress totals and time estimates).
pub fn tile_count(w: usize, h: usize, tile: usize, overlap: usize) -> usize {
    starts(w, tile, overlap).len() * starts(h, tile, overlap).len()
}

/// Mirror index into `0..n` (period `2n - 2`), so a window larger than the image sees reflected pixels.
pub fn reflect(i: isize, n: usize) -> usize {
    if n == 1 {
        return 0;
    }
    let period = 2 * (n as isize - 1);
    let m = i.rem_euclid(period);
    (if m >= n as isize { period - m } else { m }) as usize
}

/// Weight of window position `p` along an axis: 1 in the middle, a linear ramp over `overlap` pixels on each
/// side that borders another window, 1 on a side that is the image edge.
fn axis_weight(p: usize, start: usize, tile: usize, len: usize, overlap: usize) -> f32 {
    let mut w = 1.0f32;
    let ramp = (overlap + 1) as f32;
    if start > 0 && p < overlap {
        w = w.min((p + 1) as f32 / ramp);
    }
    if start + tile < len {
        let q = tile - 1 - p;
        if q < overlap {
            w = w.min((q + 1) as f32 / ramp);
        }
    }
    w
}

/// Runs `model` over `rgb` (`w * h * 3` bytes, interleaved) in overlapping tiles and returns the blended result
/// as the same kind of buffer. `progress(done, total)` is called after every tile and returns `false` to stop;
/// a stopped run returns `Ok(None)`. A model error aborts with that error.
pub fn run_tiled<E>(
    rgb: &[u8],
    w: usize,
    h: usize,
    tile: usize,
    overlap: usize,
    mut model: impl FnMut(&[f32]) -> Result<Vec<f32>, E>,
    mut progress: impl FnMut(usize, usize) -> bool,
) -> Result<Option<Vec<u8>>, E> {
    assert_eq!(rgb.len(), w * h * 3, "rgb buffer does not match {w}x{h}");
    let (xs, ys) = (starts(w, tile, overlap), starts(h, tile, overlap));
    let total = xs.len() * ys.len();
    let plane = tile * tile;
    let mut acc = vec![0f32; 3 * w * h];
    let mut wsum = vec![0f32; w * h];
    let mut input = vec![0f32; 3 * plane];
    let mut done = 0;
    for &y0 in &ys {
        for &x0 in &xs {
            for ty in 0..tile {
                let sy = reflect((y0 + ty) as isize, h);
                for tx in 0..tile {
                    let sx = reflect((x0 + tx) as isize, w);
                    let src = (sy * w + sx) * 3;
                    for c in 0..3 {
                        input[c * plane + ty * tile + tx] = rgb[src + c] as f32 / 255.0;
                    }
                }
            }
            let out = model(&input)?;
            assert_eq!(out.len(), 3 * plane, "model returned {} values for a {tile}x{tile} tile", out.len());
            for ty in 0..tile.min(h.saturating_sub(y0)) {
                let wy = axis_weight(ty, y0, tile, h, overlap);
                for tx in 0..tile.min(w.saturating_sub(x0)) {
                    let weight = wy * axis_weight(tx, x0, tile, w, overlap);
                    let idx = (y0 + ty) * w + x0 + tx;
                    wsum[idx] += weight;
                    for c in 0..3 {
                        acc[idx * 3 + c] += weight * out[c * plane + ty * tile + tx];
                    }
                }
            }
            done += 1;
            if !progress(done, total) {
                return Ok(None);
            }
        }
    }
    let mut result = vec![0u8; w * h * 3];
    for i in 0..w * h {
        for c in 0..3 {
            result[i * 3 + c] = (acc[i * 3 + c] / wsum[i] * 255.0 + 0.5).clamp(0.0, 255.0) as u8;
        }
    }
    Ok(Some(result))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::convert::Infallible;

    fn ramp_image(w: usize, h: usize) -> Vec<u8> {
        (0..w * h * 3).map(|i| ((i * 7 + i / 3 * 13) % 256) as u8).collect()
    }

    fn identity(t: &[f32]) -> Result<Vec<f32>, Infallible> {
        Ok(t.to_vec())
    }

    #[test]
    fn window_starts_cover_the_axis_with_the_last_clamped_to_the_edge() {
        assert_eq!(starts(100, 512, 32), vec![0]);
        assert_eq!(starts(512, 512, 32), vec![0]);
        assert_eq!(starts(513, 512, 32), vec![0, 1]);
        assert_eq!(starts(1000, 512, 32), vec![0, 480, 488]);
        let s = starts(6000, 512, 32);
        assert_eq!(*s.first().unwrap(), 0);
        assert_eq!(*s.last().unwrap(), 6000 - 512);
        assert!(s.windows(2).all(|p| p[1] > p[0] && p[1] - p[0] <= 512 - 32), "consecutive windows must overlap by at least 32: {s:?}");
        assert_eq!(tile_count(6000, 4000, 512, 32), s.len() * starts(4000, 512, 32).len());
    }

    #[test]
    fn reflect_mirrors_without_repeating_the_edge_pixel() {
        let got: Vec<usize> = (-3..9).map(|i| reflect(i, 4)).collect();
        assert_eq!(got, vec![3, 2, 1, 0, 1, 2, 3, 2, 1, 0, 1, 2]);
        assert_eq!(reflect(5, 1), 0);
        assert_eq!(reflect(-7, 1), 0);
    }

    #[test]
    fn an_identity_model_returns_the_image_unchanged_for_awkward_sizes() {
        for (w, h) in [(1, 1), (3, 5), (16, 16), (17, 16), (40, 23), (64, 100), (150, 70), (97, 131)] {
            let img = ramp_image(w, h);
            let out = run_tiled(&img, w, h, 16, 4, identity, |_, _| true).unwrap().unwrap();
            assert_eq!(out, img, "{w}x{h}");
        }
    }

    #[test]
    fn a_pointwise_model_gives_exactly_the_full_image_result_so_the_blend_adds_no_seam() {
        let (w, h) = (90, 77);
        let img = ramp_image(w, h);
        let out = run_tiled(&img, w, h, 32, 8, |t| Ok::<_, Infallible>(t.iter().map(|v| 0.5 * v + 0.1).collect()), |_, _| true).unwrap().unwrap();
        let want: Vec<u8> = img.iter().map(|&b| ((0.5 * (b as f32 / 255.0) + 0.1) * 255.0 + 0.5) as u8).collect();
        // One level of slack: the blend is a float weighted mean of equal values, rounded once.
        let worst = out.iter().zip(&want).map(|(a, b)| (*a as i32 - *b as i32).abs()).max().unwrap();
        assert!(worst <= 1, "worst difference {worst}");
    }

    #[test]
    fn a_model_that_differs_per_window_is_feathered_not_stepped_at_the_overlap() {
        // Window k outputs the constant k/10; across the overlap the blend must move monotonically from one to the next.
        let (w, h) = (200, 8);
        let img = vec![128u8; w * h * 3];
        let mut k = 0;
        let out = run_tiled(&img, w, h, 64, 16, |t| { k += 1; Ok::<_, Infallible>(vec![k as f32 / 10.0; t.len()]) }, |_, _| true).unwrap().unwrap();
        let row: Vec<u8> = (0..w).map(|x| out[x * 3]).collect();
        let first = starts(w, 64, 16)[1]; // second window's start: its left ramp begins here
        let seam = &row[first..first + 16];
        assert!(seam.windows(2).all(|p| p[1] >= p[0]), "not monotone across the overlap: {seam:?}");
        assert!(seam.first().unwrap() < seam.last().unwrap(), "{seam:?}");
        assert!(seam.windows(2).all(|p| p[1] - p[0] <= 6), "a step, not a ramp: {seam:?}");
    }

    #[test]
    fn progress_counts_every_tile_and_returning_false_stops_the_run() {
        let img = ramp_image(100, 100);
        let mut seen = Vec::new();
        let out = run_tiled(&img, 100, 100, 32, 8, identity, |d, t| { seen.push((d, t)); true }).unwrap();
        assert!(out.is_some());
        let total = tile_count(100, 100, 32, 8);
        assert_eq!(seen.len(), total);
        assert_eq!(*seen.last().unwrap(), (total, total));
        assert!(seen.windows(2).all(|p| p[1].0 == p[0].0 + 1));

        let mut calls = 0;
        let stopped = run_tiled(&img, 100, 100, 32, 8, |t| { calls += 1; identity(t) }, |d, _| d < 3).unwrap();
        assert!(stopped.is_none());
        assert_eq!(calls, 3, "no tile may run after the stop");
    }

    #[test]
    fn a_model_error_aborts_the_run_with_that_error() {
        let img = ramp_image(64, 64);
        let r = run_tiled(&img, 64, 64, 32, 8, |_| Err::<Vec<f32>, &str>("boom"), |_, _| true);
        assert_eq!(r.unwrap_err(), "boom");
    }
}
