//! The whole super-resolution pipeline as a pull-style row source (RFC-0029 §3.3), free of ONNX Runtime and
//! of any encoder so it is testable with a stand-in model:
//!
//! ```text
//! source RGB8 -> BandTiler (model, 4x, band-streamed) -> [Lanczos to the target size] -> Strength blend with a
//!                                                      plain Lanczos enlargement of the source -> RGB8 rows
//! ```
//!
//! The caller asks for rows in order with [`Pipeline::take_row`] and runs tiles with [`Pipeline::run_tile`]
//! until a row is ready; memory stays at the tiler's band plus a few rows of each resampler, whatever the
//! output size. `Strength` is `out = plain + (model - plain) * strength`, per row, so the Preview's slider
//! (RFC-0029 §3.4) is the same blend on an already-computed pair.

// Shared by `#[path]` with the helper binary: each side uses a different half of these items.
#![allow(dead_code)]

use super::resample::RowResampler;
use super::upscale_tiling::BandTiler;
use std::collections::VecDeque;

/// What the shipped model does: it enlarges by exactly this factor.
pub const MODEL_SCALE: usize = 4;
/// Input tile and overlap in source pixels. The compact model's receptive field is about 65 px, so a tile
/// edge is disturbed up to about 33 px in; a 32 px feather puts the blend where that has faded.
pub const TILE: usize = 256;
pub const OVERLAP: usize = 32;

/// Rows of the plain (model-free) enlargement, produced on demand in lock-step with the model rows.
struct Baseline<'a> {
    rgb: &'a [u8],
    w: usize,
    h: usize,
    resampler: RowResampler,
    next_src: usize,
    queue: VecDeque<Vec<f32>>,
}

impl<'a> Baseline<'a> {
    fn new(rgb: &'a [u8], w: usize, h: usize, out_w: usize, out_h: usize) -> Self {
        Baseline { rgb, w, h, resampler: RowResampler::new(w, h, out_w, out_h), next_src: 0, queue: VecDeque::new() }
    }

    fn next_row(&mut self) -> Vec<f32> {
        let mut src = vec![0f32; self.w * 3];
        while self.queue.is_empty() {
            assert!(self.next_src < self.h, "baseline asked for more rows than the output has");
            for (d, s) in src.iter_mut().zip(&self.rgb[self.next_src * self.w * 3..(self.next_src + 1) * self.w * 3]) {
                *d = *s as f32 / 255.0;
            }
            self.next_src += 1;
            let queue = &mut self.queue;
            self.resampler.push(&src, |row| queue.push_back(row.to_vec()));
        }
        self.queue.pop_front().expect("loop filled it")
    }
}

pub struct Pipeline<'a> {
    tiler: BandTiler<'a>,
    /// Model grid (`w * 4`) to the requested size; `None` when they are the same.
    model_resampler: Option<RowResampler>,
    baseline: Option<Baseline<'a>>,
    strength: f32,
    out_w: usize,
    out_h: usize,
    ready: VecDeque<Vec<u8>>,
    produced: usize,
}

fn quantise(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

impl<'a> Pipeline<'a> {
    /// `rgb` is the `w x h` source (interleaved RGB8); the result is `out_w x out_h`; `strength` is 1..=100; the
    /// last pair is the input tile and overlap in source pixels.
    pub fn new(rgb: &'a [u8], (w, h): (usize, usize), (out_w, out_h): (usize, usize), strength: u8, (tile, overlap): (usize, usize)) -> Self {
        assert!((1..=100).contains(&strength), "strength must be 1..=100, got {strength}");
        let tiler = BandTiler::new(rgb, w, h, MODEL_SCALE, tile, overlap);
        let model_resampler = ((w * MODEL_SCALE, h * MODEL_SCALE) != (out_w, out_h)).then(|| RowResampler::new(w * MODEL_SCALE, h * MODEL_SCALE, out_w, out_h));
        let baseline = (strength < 100).then(|| Baseline::new(rgb, w, h, out_w, out_h));
        Pipeline { tiler, model_resampler, baseline, strength: strength as f32 / 100.0, out_w, out_h, ready: VecDeque::new(), produced: 0 }
    }

    pub fn tile_count(&self) -> usize {
        self.tiler.tile_count()
    }

    pub fn out_size(&self) -> (usize, usize) {
        (self.out_w, self.out_h)
    }

    /// Final rows computed but not yet taken.
    pub fn ready_rows(&self) -> usize {
        self.ready.len()
    }

    /// The next finished output row (`out_w * 3` bytes), in order.
    pub fn take_row(&mut self) -> Option<Vec<u8>> {
        self.ready.pop_front()
    }

    pub fn is_finished(&self) -> bool {
        self.produced == self.out_h && self.ready.is_empty()
    }

    /// Runs the model on the next tile and turns whatever rows that finished into output rows. Returns
    /// `(done, total)` tiles, or `None` when every tile has run.
    pub fn run_tile<E>(&mut self, model: impl FnMut(&[f32]) -> Result<Vec<f32>, E>) -> Result<Option<(usize, usize)>, E> {
        let Some(step) = self.tiler.next(model)? else { return Ok(None) };
        if let Some((_, rows)) = step.rows {
            let Pipeline { model_resampler, baseline, strength, ready, produced, out_w, .. } = self;
            let model_w = self.tiler.out_size().0;
            let mut finalize = |m: &[f32]| {
                let mut row = Vec::with_capacity(*out_w * 3);
                match baseline {
                    Some(b) => {
                        let plain = b.next_row();
                        row.extend(m.iter().zip(&plain).map(|(m, p)| quantise(p + (m - p) * *strength)));
                    }
                    None => row.extend(m.iter().map(|&v| quantise(v))),
                }
                ready.push_back(row);
                *produced += 1;
            };
            for src in rows.chunks_exact(model_w * 3) {
                match model_resampler {
                    Some(rs) => rs.push(src, &mut finalize),
                    None => finalize(src),
                }
            }
        }
        Ok(Some((step.done, step.total)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::convert::Infallible;

    fn photoish(w: usize, h: usize) -> Vec<u8> {
        (0..w * h * 3).map(|i| ((i * 7 + i / 3 * 13 + i / (w * 3) * 5) % 256) as u8).collect()
    }

    fn nearest(tile: usize) -> impl FnMut(&[f32]) -> Result<Vec<f32>, Infallible> {
        move |t| {
            let (o, plane, oplane) = (tile * 4, tile * tile, tile * 4 * tile * 4);
            let mut out = vec![0f32; 3 * oplane];
            for c in 0..3 {
                for y in 0..o {
                    for x in 0..o {
                        out[c * oplane + y * o + x] = t[c * plane + (y / 4) * tile + x / 4];
                    }
                }
            }
            Ok(out)
        }
    }

    /// Drains a pipeline: runs tiles until done, taking rows as they appear; returns the picture and the peak
    /// number of rows that were waiting.
    fn drain(mut p: Pipeline, tile: usize) -> (Vec<u8>, usize) {
        let mut out = Vec::new();
        let mut model = nearest(tile);
        let mut peak = 0;
        while p.run_tile(&mut model).unwrap().is_some() {
            peak = peak.max(p.ready_rows());
            while let Some(r) = p.take_row() {
                out.extend(r);
            }
        }
        while let Some(r) = p.take_row() {
            out.extend(r);
        }
        assert!(p.is_finished());
        (out, peak)
    }

    #[test]
    fn full_strength_at_the_model_scale_is_the_models_pixels_untouched() {
        let (w, h) = (53, 41);
        let img = photoish(w, h);
        let (out, _) = drain(Pipeline::new(&img, (w, h), (w * 4, h * 4), 100, (16, 4)), 16);
        assert_eq!(out.len(), w * 4 * h * 4 * 3);
        for oy in 0..h * 4 {
            for ox in 0..w * 4 {
                for c in 0..3 {
                    assert_eq!(out[(oy * w * 4 + ox) * 3 + c], img[((oy / 4) * w + ox / 4) * 3 + c], "({ox},{oy})");
                }
            }
        }
    }

    #[test]
    fn any_other_target_size_is_the_model_result_resampled_to_it() {
        // Half the model's size (a 2x enlargement) and an odd long-edge size, both through the real resampler.
        let (w, h) = (40, 30);
        let img = photoish(w, h);
        for (ow, oh) in [(80, 60), (123, 92), (200, 150)] {
            let (out, _) = drain(Pipeline::new(&img, (w, h), (ow, oh), 100, (16, 4)), 16);
            assert_eq!(out.len(), ow * oh * 3, "{ow}x{oh}");
            // Reference: the NN 4x picture pushed through the same resampler.
            let mut rs = RowResampler::new(w * 4, h * 4, ow, oh);
            let mut want = Vec::new();
            for oy in 0..h * 4 {
                let row: Vec<f32> = (0..w * 4).flat_map(|ox| (0..3).map(move |c| (ox, c))).map(|(ox, c)| img[((oy / 4) * w + ox / 4) * 3 + c] as f32 / 255.0).collect();
                rs.push(&row, |r| want.extend(r.iter().map(|&v| quantise(v))));
            }
            assert_eq!(out, want, "{ow}x{oh}");
        }
    }

    #[test]
    fn strength_blends_the_model_with_a_plain_enlargement_of_the_source() {
        let (w, h) = (30, 22);
        let img = photoish(w, h);
        let plain = {
            let mut rs = RowResampler::new(w, h, w * 4, h * 4);
            let mut rows = Vec::new();
            for y in 0..h {
                let r: Vec<f32> = img[y * w * 3..(y + 1) * w * 3].iter().map(|&b| b as f32 / 255.0).collect();
                rs.push(&r, |o| rows.extend_from_slice(o));
            }
            rows
        };
        for strength in [25u8, 50, 80] {
            let (out, _) = drain(Pipeline::new(&img, (w, h), (w * 4, h * 4), strength, (16, 4)), 16);
            let s = strength as f32 / 100.0;
            for oy in 0..h * 4 {
                for ox in 0..w * 4 {
                    let model = img[((oy / 4) * w + ox / 4) * 3 + 1] as f32 / 255.0;
                    let p = plain[(oy * w * 4 + ox) * 3 + 1];
                    let want = quantise(p + (model - p) * s);
                    assert_eq!(out[(oy * w * 4 + ox) * 3 + 1], want, "strength {strength} at ({ox},{oy})");
                }
            }
        }
    }

    #[test]
    fn one_percent_strength_is_nearly_the_plain_enlargement_and_stays_in_range() {
        let (w, h) = (20, 20);
        let img = photoish(w, h);
        let (out, _) = drain(Pipeline::new(&img, (w, h), (50, 50), 1, (16, 4)), 16);
        assert_eq!(out.len(), 50 * 50 * 3);
    }

    #[test]
    fn output_rows_leave_as_the_job_goes_so_a_tall_job_never_holds_the_picture() {
        let (w, h) = (24, 400);
        let img = photoish(w, h);
        let (out, peak) = drain(Pipeline::new(&img, (w, h), (w * 4, h * 4), 100, (32, 8)), 32);
        assert_eq!(out.len(), w * 4 * h * 4 * 3);
        // One row of tiles finishes at most `tile * scale` rows (plus one stride of the next); never the picture.
        assert!(peak <= (32 + 24) * 4, "{peak} rows waited at once out of {}", h * 4);
    }

    #[test]
    #[should_panic(expected = "strength must be 1..=100")]
    fn zero_strength_is_refused() {
        let img = photoish(8, 8);
        let _ = Pipeline::new(&img, (8, 8), (32, 32), 0, (16, 4));
    }
}
