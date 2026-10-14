//! Band-streamed tiled super-resolution (RFC-0029 §3.3). `tiling::run_tiled` keeps an accumulator for the whole
//! image, which is fine for a same-size denoise (about 0.4 GB at 24 MP) but not for a 4x result (over 1 GB at
//! 24 MP, 6 GB at 60). This tiler walks the tile grid row by row and keeps only the **band** of output rows the
//! current row of tiles can still change: when a row of tiles is done, every output row above the next row's
//! first window is final and is handed out; the rest is carried into the next band. Peak memory is
//! `tile * scale` output rows (`12 * tile * scale^2 * width` bytes, about 0.3 GB for a 6000 px wide input at
//! tile 256, scale 4) whatever the image height.
//!
//! Pure and model-agnostic (std only, shared with the helper by `#[path]`): the model is a closure from a
//! `tile x tile` CHW f32 tensor (0..1) to a `(tile * scale)^2` CHW tensor, so the seam arithmetic is tested
//! with stand-in models and no weights. The windows and the feather weights are `tiling.rs`'s, applied on the
//! input grid and repeated `scale` times on the output grid; the weights are separable, so the weight sums are
//! two 1-D arrays instead of one per pixel.

// Shared by `#[path]` with the helper binary: each side uses a different half of these items.
#![allow(dead_code)]

use super::tiling::{axis_weight, reflect, starts};

/// One finished tile and, when it completed a row of tiles, the output rows that became final.
pub struct Step {
    /// Tiles done so far, and in total.
    pub done: usize,
    pub total: usize,
    /// `(first output row, rows)`: `rows` is `n * width * scale * 3` floats, interleaved RGB, clamped to 0..1.
    pub rows: Option<(usize, Vec<f32>)>,
}

pub struct BandTiler<'a> {
    rgb: &'a [u8],
    w: usize,
    h: usize,
    scale: usize,
    tile: usize,
    overlap: usize,
    xs: Vec<usize>,
    ys: Vec<usize>,
    out_w: usize,
    /// Sum over windows of the horizontal feather weight, per output column (the weights are separable).
    wsum_x: Vec<f32>,
    /// Band accumulator: `tile * scale` output rows, row `r` of it is output row `band_row0 + r`.
    acc: Vec<f32>,
    /// The same band's vertical weight sums.
    wsum_y: Vec<f32>,
    band_row0: usize,
    ty: usize,
    tx: usize,
    emitted: usize,
    input: Vec<f32>,
    /// Horizontal feather weight of each window column, for the window being accumulated.
    wx: Vec<f32>,
}

impl<'a> BandTiler<'a> {
    pub fn new(rgb: &'a [u8], w: usize, h: usize, scale: usize, tile: usize, overlap: usize) -> Self {
        assert_eq!(rgb.len(), w * h * 3, "rgb buffer does not match {w}x{h}");
        assert!(scale >= 1 && w > 0 && h > 0);
        let (xs, ys) = (starts(w, tile, overlap), starts(h, tile, overlap));
        let out_w = w * scale;
        let mut wsum_x = vec![0f32; out_w];
        for &x0 in &xs {
            for t in 0..tile.min(w - x0) {
                let a = axis_weight(t, x0, tile, w, overlap);
                for s in 0..scale {
                    wsum_x[(x0 + t) * scale + s] += a;
                }
            }
        }
        let band_rows = tile * scale;
        BandTiler {
            rgb,
            w,
            h,
            scale,
            tile,
            overlap,
            xs,
            ys,
            out_w,
            wsum_x,
            acc: vec![0f32; band_rows * out_w * 3],
            wsum_y: vec![0f32; band_rows],
            band_row0: 0,
            ty: 0,
            tx: 0,
            emitted: 0,
            input: vec![0f32; 3 * tile * tile],
            wx: vec![0f32; tile],
        }
    }

    pub fn tile_count(&self) -> usize {
        self.xs.len() * self.ys.len()
    }

    /// Output size in pixels.
    pub fn out_size(&self) -> (usize, usize) {
        (self.out_w, self.h * self.scale)
    }

    /// Bytes held by the band (the dominant allocation), for estimates and memory assertions.
    pub fn band_bytes(&self) -> usize {
        (self.acc.len() + self.wsum_y.len() + self.wsum_x.len()) * 4
    }

    /// Starts tile row `ty`: moves the rows the previous row left unfinished to the top of the band and adds
    /// this row's vertical weights.
    fn begin_row(&mut self) {
        let y0 = self.ys[self.ty];
        let new_row0 = y0 * self.scale;
        let rows_w = self.out_w * 3;
        if self.ty > 0 {
            let old_row0 = self.band_row0;
            let old_end = (self.ys[self.ty - 1] + self.tile).min(self.h) * self.scale;
            let carried = old_end.saturating_sub(new_row0);
            let shift = new_row0 - old_row0;
            self.acc.copy_within(shift * rows_w..(shift + carried) * rows_w, 0);
            self.acc[carried * rows_w..].fill(0.0);
            self.wsum_y.copy_within(shift..shift + carried, 0);
            self.wsum_y[carried..].fill(0.0);
        }
        self.band_row0 = new_row0;
        for t in 0..self.tile.min(self.h - y0) {
            let a = axis_weight(t, y0, self.tile, self.h, self.overlap);
            for s in 0..self.scale {
                self.wsum_y[t * self.scale + s] += a;
            }
        }
    }

    /// Runs the model on the next tile. `Ok(None)` once every tile has been done.
    pub fn next<E>(&mut self, mut model: impl FnMut(&[f32]) -> Result<Vec<f32>, E>) -> Result<Option<Step>, E> {
        if self.ty >= self.ys.len() {
            return Ok(None);
        }
        if self.tx == 0 {
            self.begin_row();
        }
        let (tile, scale, plane) = (self.tile, self.scale, self.tile * self.tile);
        let (x0, y0) = (self.xs[self.tx], self.ys[self.ty]);
        for ty in 0..tile {
            let sy = reflect((y0 + ty) as isize, self.h);
            for tx in 0..tile {
                let sx = reflect((x0 + tx) as isize, self.w);
                let src = (sy * self.w + sx) * 3;
                for c in 0..3 {
                    self.input[c * plane + ty * tile + tx] = self.rgb[src + c] as f32 / 255.0;
                }
            }
        }
        let out = model(&self.input)?;
        let (ot, oplane) = (tile * scale, tile * scale * tile * scale);
        assert_eq!(out.len(), 3 * oplane, "model returned {} values for a {tile}x{tile} tile at {scale}x", out.len());

        let cols = tile.min(self.w - x0);
        for t in 0..cols {
            self.wx[t] = axis_weight(t, x0, tile, self.w, self.overlap);
        }
        for r in 0..tile.min(self.h - y0) * scale {
            let wy = axis_weight(r / scale, y0, tile, self.h, self.overlap);
            let acc_row = &mut self.acc[r * self.out_w * 3..(r + 1) * self.out_w * 3];
            for c in 0..cols * scale {
                let weight = wy * self.wx[c / scale];
                let dst = (x0 * scale + c) * 3;
                let src = r * ot + c;
                acc_row[dst] += weight * out[src];
                acc_row[dst + 1] += weight * out[oplane + src];
                acc_row[dst + 2] += weight * out[2 * oplane + src];
            }
        }

        self.tx += 1;
        let done = self.ty * self.xs.len() + self.tx;
        let mut rows = None;
        if self.tx == self.xs.len() {
            let last = self.ty + 1 == self.ys.len();
            let bound = if last { self.h * scale } else { self.ys[self.ty + 1] * scale };
            rows = Some((self.emitted, self.finish_rows(bound)));
            self.emitted = bound;
            self.ty += 1;
            self.tx = 0;
        }
        Ok(Some(Step { done, total: self.tile_count(), rows }))
    }

    /// Normalises and returns output rows `emitted..bound`.
    fn finish_rows(&self, bound: usize) -> Vec<f32> {
        let mut out = Vec::with_capacity((bound - self.emitted) * self.out_w * 3);
        for r in self.emitted..bound {
            let local = r - self.band_row0;
            let row = &self.acc[local * self.out_w * 3..(local + 1) * self.out_w * 3];
            for c in 0..self.out_w {
                let inv = 1.0 / (self.wsum_y[local] * self.wsum_x[c]);
                for ch in 0..3 {
                    out.push((row[c * 3 + ch] * inv).clamp(0.0, 1.0));
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::convert::Infallible;

    fn photoish(w: usize, h: usize) -> Vec<u8> {
        (0..w * h * 3).map(|i| ((i * 7 + i / 3 * 13 + i / (w * 3) * 5) % 256) as u8).collect()
    }

    /// A stand-in 4x model: every input pixel becomes a `scale x scale` block of itself.
    fn nearest(scale: usize, tile: usize) -> impl FnMut(&[f32]) -> Result<Vec<f32>, Infallible> {
        move |t| {
            let (o, plane, oplane) = (tile * scale, tile * tile, tile * scale * tile * scale);
            let mut out = vec![0f32; 3 * oplane];
            for c in 0..3 {
                for y in 0..o {
                    for x in 0..o {
                        out[c * oplane + y * o + x] = t[c * plane + (y / scale) * tile + x / scale];
                    }
                }
            }
            Ok(out)
        }
    }

    /// Runs the whole job and returns the assembled output plus the (first, count) of each row batch.
    fn run_all(rgb: &[u8], w: usize, h: usize, scale: usize, tile: usize, overlap: usize, mut model: impl FnMut(&[f32]) -> Result<Vec<f32>, Infallible>) -> (Vec<f32>, Vec<(usize, usize)>) {
        let mut t = BandTiler::new(rgb, w, h, scale, tile, overlap);
        let mut out = Vec::new();
        let mut batches = Vec::new();
        let mut tiles = 0;
        while let Some(step) = t.next(&mut model).unwrap() {
            tiles += 1;
            assert_eq!(step.done, tiles);
            assert_eq!(step.total, t.tile_count());
            if let Some((first, rows)) = step.rows {
                assert_eq!(first * w * scale * 3, out.len(), "rows must arrive in order with no gap or repeat");
                batches.push((first, rows.len() / (w * scale * 3)));
                out.extend(rows);
            }
        }
        assert_eq!(tiles, t.tile_count());
        (out, batches)
    }

    #[test]
    fn a_nearest_neighbour_model_through_awkward_sizes_equals_a_plain_nearest_enlargement() {
        // The seam test of RFC-0029 §3.3: any difference from a plain enlargement is the tiler's, not a model's.
        for (w, h) in [(1, 1), (3, 5), (16, 16), (17, 16), (40, 23), (64, 100), (97, 131), (150, 70)] {
            let img = photoish(w, h);
            let (out, _) = run_all(&img, w, h, 4, 16, 4, nearest(4, 16));
            assert_eq!(out.len(), w * 4 * h * 4 * 3, "{w}x{h}");
            for oy in 0..h * 4 {
                for ox in 0..w * 4 {
                    for c in 0..3 {
                        let want = img[((oy / 4) * w + ox / 4) * 3 + c] as f32 / 255.0;
                        let got = out[(oy * w * 4 + ox) * 3 + c];
                        assert!((got - want).abs() < 1e-5, "{w}x{h} at ({ox},{oy}) channel {c}: {got} vs {want}");
                    }
                }
            }
        }
    }

    #[test]
    fn scale_two_works_the_same_way() {
        let (w, h) = (45, 38);
        let img = photoish(w, h);
        let (out, _) = run_all(&img, w, h, 2, 16, 4, nearest(2, 16));
        for oy in 0..h * 2 {
            for ox in 0..w * 2 {
                let want = img[((oy / 2) * w + ox / 2) * 3 + 1] as f32 / 255.0;
                assert!((out[(oy * w * 2 + ox) * 3 + 1] - want).abs() < 1e-5, "({ox},{oy})");
            }
        }
    }

    #[test]
    fn rows_are_handed_out_as_soon_as_a_row_of_tiles_finishes_and_cover_the_output_exactly_once() {
        let (w, h) = (50, 200);
        let img = photoish(w, h);
        let (_, batches) = run_all(&img, w, h, 4, 32, 8, nearest(4, 32));
        assert!(batches.len() > 1, "a tall image must stream in several batches: {batches:?}");
        let mut next = 0;
        for &(first, n) in &batches {
            assert_eq!(first, next);
            assert!(n > 0);
            next = first + n;
        }
        assert_eq!(next, h * 4);
        // Each batch ends where the next row of windows begins: that is what lets the band be freed.
        let ys = starts(h, 32, 8);
        for (i, &(first, n)) in batches.iter().enumerate().take(batches.len() - 1) {
            assert_eq!(first + n, ys[i + 1] * 4);
        }
    }

    #[test]
    fn the_band_is_a_few_tile_rows_however_tall_the_image_is() {
        let img_a = photoish(60, 100);
        let img_b = photoish(60, 4000);
        let a = BandTiler::new(&img_a, 60, 100, 4, 32, 8).band_bytes();
        let b = BandTiler::new(&img_b, 60, 4000, 4, 32, 8).band_bytes();
        assert_eq!(a, b, "memory must not grow with the height");
        // tile * scale rows of the output width, 3 floats and a weight each.
        assert!(a < 32 * 4 * 60 * 4 * 4 * 3 + 100_000, "{a}");
    }

    #[test]
    fn a_model_that_differs_per_window_is_feathered_not_stepped_at_the_overlap() {
        let (w, h) = (200, 8);
        let img = vec![128u8; w * h * 3];
        let mut k = 0;
        let (out, _) = run_all(&img, w, h, 2, 64, 16, |_| {
            k += 1;
            Ok(vec![k as f32 / 10.0; 3 * 128 * 128])
        });
        let row: Vec<f32> = (0..w * 2).map(|x| out[x * 3]).collect();
        let start = starts(w, 64, 16)[1] * 2;
        let seam = &row[start..start + 32];
        assert!(seam.windows(2).all(|p| p[1] >= p[0] - 1e-6), "not monotone across the overlap: {seam:?}");
        assert!(seam.first().unwrap() < seam.last().unwrap(), "{seam:?}");
        assert!(seam.windows(2).all(|p| p[1] - p[0] < 0.02), "a step, not a ramp: {seam:?}");
    }

    #[test]
    fn vertical_seams_are_feathered_too() {
        let (w, h) = (8, 200);
        let img = vec![128u8; w * h * 3];
        let mut k = 0;
        let (out, _) = run_all(&img, w, h, 2, 64, 16, |_| {
            k += 1;
            Ok(vec![k as f32 / 10.0; 3 * 128 * 128])
        });
        let col: Vec<f32> = (0..h * 2).map(|y| out[y * w * 2 * 3]).collect();
        let start = starts(h, 64, 16)[1] * 2;
        let seam = &col[start..start + 32];
        assert!(seam.windows(2).all(|p| p[1] >= p[0] - 1e-6), "{seam:?}");
        assert!(seam.windows(2).all(|p| p[1] - p[0] < 0.02), "{seam:?}");
    }

    #[test]
    fn a_model_error_comes_back_and_the_tiler_can_be_dropped() {
        let img = photoish(40, 40);
        let mut t = BandTiler::new(&img, 40, 40, 4, 16, 4);
        let r = t.next(|_| Err::<Vec<f32>, &str>("boom"));
        assert_eq!(r.err(), Some("boom"));
    }

    #[test]
    fn an_exhausted_tiler_returns_none() {
        let img = photoish(8, 8);
        let mut t = BandTiler::new(&img, 8, 8, 4, 16, 4);
        let mut m = nearest(4, 16);
        assert!(t.next(&mut m).unwrap().is_some());
        assert!(t.next(&mut m).unwrap().is_none());
    }
}
