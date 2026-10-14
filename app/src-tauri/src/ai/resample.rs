//! Streaming Lanczos-3 resampling (RFC-0029 §3.3): a separable resize that takes the source one row at a time
//! and hands back destination rows as soon as the taps they need have arrived, so a 96 MP result never has to
//! exist in memory. Used twice by the upscale job: to turn the model's 4x rows into the requested size (2x,
//! a long edge) and to produce the plain-enlargement baseline that Strength blends with.
//!
//! Pure (std only, shared with the helper by `#[path]`). The taps follow the usual pixel-centre convention
//! (destination pixel `i` is centred at `(i + 0.5) * src / dst` in source pixels; the support widens by the
//! scale factor when shrinking), taps that fall off the image are dropped and the rest renormalised, so the
//! edges stay flat rather than darkening.

// Shared by `#[path]` with the helper binary: each side uses a different half of these items.
#![allow(dead_code)]

use std::collections::VecDeque;

const A: f64 = 3.0;

fn lanczos(x: f64) -> f64 {
    if x == 0.0 {
        return 1.0;
    }
    if x.abs() >= A {
        return 0.0;
    }
    let px = std::f64::consts::PI * x;
    A * px.sin() * (px / A).sin() / (px * px)
}

/// The source pixels one destination pixel is built from: `weights[k]` applies to source pixel `start + k`.
#[derive(Debug, Clone, PartialEq)]
pub struct Taps {
    pub start: usize,
    pub weights: Vec<f32>,
}

impl Taps {
    /// One past the last source pixel used.
    pub fn end(&self) -> usize {
        self.start + self.weights.len()
    }
}

/// Taps for every destination index of an axis resized from `src` to `dst` pixels.
pub fn axis_taps(src: usize, dst: usize) -> Vec<Taps> {
    assert!(src > 0 && dst > 0, "cannot resample {src} -> {dst}");
    if src == dst {
        return (0..dst).map(|i| Taps { start: i, weights: vec![1.0] }).collect();
    }
    let scale = src as f64 / dst as f64;
    let filter_scale = scale.max(1.0);
    let support = A * filter_scale;
    (0..dst)
        .map(|i| {
            let center = (i as f64 + 0.5) * scale;
            let lo = ((center - support + 0.5).floor().max(0.0)) as usize;
            let hi = (((center + support + 0.5).floor()).max(0.0) as usize).min(src);
            let lo = lo.min(hi.saturating_sub(1));
            let mut weights: Vec<f64> = (lo..hi).map(|x| lanczos((x as f64 + 0.5 - center) / filter_scale)).collect();
            let sum: f64 = weights.iter().sum();
            if sum.abs() < 1e-12 {
                // Cannot happen for a positive-width window, but never divide by ~0: fall back to the nearest pixel.
                let nearest = (center.floor() as usize).min(src - 1);
                return Taps { start: nearest, weights: vec![1.0] };
            }
            weights.iter_mut().for_each(|w| *w /= sum);
            Taps { start: lo, weights: weights.into_iter().map(|w| w as f32).collect() }
        })
        .collect()
}

/// Push-style resizer: feed source rows (`src_w * 3` floats, interleaved RGB) in order with [`push`](Self::push);
/// every destination row that became computable is handed to the callback, in order.
pub struct RowResampler {
    dst_w: usize,
    dst_h: usize,
    xt: Vec<Taps>,
    yt: Vec<Taps>,
    /// Horizontally resampled source rows still needed, oldest first; `first` is the index of `rows[0]`.
    rows: VecDeque<Vec<f32>>,
    first: usize,
    pushed: usize,
    next_dst: usize,
    src_w: usize,
    src_h: usize,
}

impl RowResampler {
    pub fn new(src_w: usize, src_h: usize, dst_w: usize, dst_h: usize) -> Self {
        RowResampler {
            dst_w,
            dst_h,
            xt: axis_taps(src_w, dst_w),
            yt: axis_taps(src_h, dst_h),
            rows: VecDeque::new(),
            first: 0,
            pushed: 0,
            next_dst: 0,
            src_w,
            src_h,
        }
    }

    pub fn dst_rows(&self) -> usize {
        self.dst_h
    }

    fn horizontal(&self, src: &[f32]) -> Vec<f32> {
        if self.src_w == self.dst_w {
            return src.to_vec();
        }
        let mut out = vec![0f32; self.dst_w * 3];
        for (i, t) in self.xt.iter().enumerate() {
            let (mut r, mut g, mut b) = (0f32, 0f32, 0f32);
            for (k, &w) in t.weights.iter().enumerate() {
                let p = (t.start + k) * 3;
                r += w * src[p];
                g += w * src[p + 1];
                b += w * src[p + 2];
            }
            out[i * 3] = r;
            out[i * 3 + 1] = g;
            out[i * 3 + 2] = b;
        }
        out
    }

    /// Takes the next source row. Calls `emit` once per destination row this row completes (none, one or
    /// several, always in order).
    pub fn push(&mut self, src_row: &[f32], mut emit: impl FnMut(&[f32])) {
        assert_eq!(src_row.len(), self.src_w * 3, "source row has {} values for width {}", src_row.len(), self.src_w);
        assert!(self.pushed < self.src_h, "more than {} source rows pushed", self.src_h);
        let row = self.horizontal(src_row);
        self.rows.push_back(row);
        self.pushed += 1;
        let mut out = vec![0f32; self.dst_w * 3];
        while self.next_dst < self.dst_h && self.yt[self.next_dst].end() <= self.pushed {
            let t = &self.yt[self.next_dst];
            out.iter_mut().for_each(|v| *v = 0.0);
            for (k, &w) in t.weights.iter().enumerate() {
                let row = &self.rows[t.start + k - self.first];
                for (o, v) in out.iter_mut().zip(row) {
                    *o += w * v;
                }
            }
            emit(&out);
            self.next_dst += 1;
        }
        // Rows below the next destination row's first tap are never read again.
        let keep_from = if self.next_dst < self.dst_h { self.yt[self.next_dst].start } else { self.pushed };
        while self.first < keep_from && !self.rows.is_empty() {
            self.rows.pop_front();
            self.first += 1;
        }
    }

    /// Source rows buffered right now (for memory assertions).
    pub fn buffered_rows(&self) -> usize {
        self.rows.len()
    }

    pub fn is_done(&self) -> bool {
        self.next_dst == self.dst_h
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reference: resize a whole `w x h` RGB float image with the same taps, the slow obvious way.
    fn resize_whole(src: &[f32], w: usize, h: usize, dw: usize, dh: usize) -> Vec<f32> {
        let (xt, yt) = (axis_taps(w, dw), axis_taps(h, dh));
        let mut mid = vec![0f32; dw * h * 3];
        for y in 0..h {
            for (i, t) in xt.iter().enumerate() {
                for c in 0..3 {
                    mid[(y * dw + i) * 3 + c] = t.weights.iter().enumerate().map(|(k, &wt)| wt * src[(y * w + t.start + k) * 3 + c]).sum();
                }
            }
        }
        let mut out = vec![0f32; dw * dh * 3];
        for (j, t) in yt.iter().enumerate() {
            for x in 0..dw {
                for c in 0..3 {
                    out[(j * dw + x) * 3 + c] = t.weights.iter().enumerate().map(|(k, &wt)| wt * mid[((t.start + k) * dw + x) * 3 + c]).sum();
                }
            }
        }
        out
    }

    fn pattern(w: usize, h: usize) -> Vec<f32> {
        (0..w * h * 3).map(|i| ((i * 37 + i / 7 * 11) % 251) as f32 / 250.0).collect()
    }

    fn stream(src: &[f32], w: usize, h: usize, dw: usize, dh: usize) -> (Vec<f32>, usize) {
        let mut r = RowResampler::new(w, h, dw, dh);
        let mut out = Vec::new();
        let mut peak = 0;
        for y in 0..h {
            r.push(&src[y * w * 3..(y + 1) * w * 3], |row| out.extend_from_slice(row));
            peak = peak.max(r.buffered_rows());
        }
        assert!(r.is_done());
        (out, peak)
    }

    #[test]
    fn every_destination_pixel_gets_weights_that_sum_to_one_inside_the_source() {
        for (src, dst) in [(7, 28), (28, 7), (10, 10), (1, 5), (5, 1), (100, 37), (37, 100), (3, 4)] {
            for (i, t) in axis_taps(src, dst).iter().enumerate() {
                let sum: f32 = t.weights.iter().sum();
                assert!((sum - 1.0).abs() < 1e-5, "{src}->{dst} #{i}: sum {sum}");
                assert!(t.end() <= src && !t.weights.is_empty(), "{src}->{dst} #{i}: taps {}..{}", t.start, t.end());
            }
        }
    }

    #[test]
    fn the_same_size_is_an_exact_pass_through() {
        let src = pattern(9, 6);
        let (out, _) = stream(&src, 9, 6, 9, 6);
        assert_eq!(out, src);
    }

    #[test]
    fn a_flat_image_stays_flat_at_every_scale_so_the_edges_do_not_darken() {
        for (w, h, dw, dh) in [(8, 8, 32, 32), (32, 32, 8, 8), (9, 5, 16, 11), (40, 30, 20, 15)] {
            let src = vec![0.4f32; w * h * 3];
            let (out, _) = stream(&src, w, h, dw, dh);
            assert!(out.iter().all(|v| (v - 0.4).abs() < 1e-5), "{w}x{h}->{dw}x{dh}");
        }
    }

    #[test]
    fn streaming_gives_the_same_pixels_as_resizing_the_whole_image_and_keeps_only_a_window_of_rows() {
        for (w, h, dw, dh) in [(20, 40, 80, 160), (80, 60, 40, 30), (33, 21, 50, 14), (64, 64, 48, 96)] {
            let src = pattern(w, h);
            let (got, peak) = stream(&src, w, h, dw, dh);
            let want = resize_whole(&src, w, h, dw, dh);
            assert_eq!(got.len(), want.len());
            let worst = got.iter().zip(&want).map(|(a, b)| (a - b).abs()).fold(0f32, f32::max);
            assert!(worst < 1e-5, "{w}x{h}->{dw}x{dh}: worst {worst}");
            let scale = (h as f32 / dh as f32).max(1.0);
            let window = (2.0 * 3.0 * scale).ceil() as usize + 2;
            assert!(peak <= window, "{w}x{h}->{dw}x{dh}: buffered {peak} rows, window is about {window}");
        }
    }

    #[test]
    fn a_horizontal_ramp_stays_a_ramp_when_enlarged() {
        // Lanczos overshoots at edges but a straight ramp must stay monotone in the interior.
        let (w, h) = (16, 4);
        let src: Vec<f32> = (0..h).flat_map(|_| (0..w).flat_map(|x| [x as f32 / 15.0; 3])).collect();
        let (out, _) = stream(&src, w, h, 64, 16);
        let row: Vec<f32> = (0..64).map(|x| out[x * 3]).collect();
        assert!(row[8..56].windows(2).all(|p| p[1] >= p[0] - 1e-4), "{row:?}");
    }
}
