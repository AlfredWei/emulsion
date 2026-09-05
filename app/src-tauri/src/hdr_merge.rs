//! HDR merge (RFC-0003): multi-exposure RAW bracket -> one radiometrically
//! merged, tone-mapped composite. Pure image processing, no catalog/SQLite
//! dependency -- `lib.rs`'s `merge_hdr_bracket` command resolves catalog
//! rows into `BracketInput`s and re-catalogs the result; this module only
//! knows about pixels and file paths.
//!
//! Deliberately NOT implemented (see RFC-0003 §2 for the full reasoning):
//! camera-response-curve estimation (RAW-only input sidesteps needing it),
//! full geometric (rotation/perspective) alignment, ghost/moving-object
//! removal, and local/detail-preserving tone mapping.

use crate::raw_decode::{self, DecodedLinear};
use image::{ImageBuffer, Luma, RgbImage};
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum HdrMergeError {
    #[error("HDR merge needs at least 2 images, got {0}")]
    NotEnoughFrames(usize),
    #[error("could not decode {0}: {1}")]
    Decode(String, String),
    #[error("couldn't read exposure info (ISO/aperture/shutter speed) for {0}")]
    MissingExposureInfo(String),
    #[error("bracket frames must all have the same dimensions (got {0}x{1} and {2}x{3})")]
    DimensionMismatch(u32, u32, u32, u32),
}

/// One bracket member's identity + the EXIF fields needed to compute its
/// exposure value. Resolved from the catalog by the caller (`lib.rs`).
pub struct BracketInput {
    pub path: PathBuf,
    pub iso: Option<u32>,
    pub aperture: Option<f32>,
    pub shutter_speed: Option<f32>,
}

/// Result of a successful merge: the tone-mapped composite plus enough
/// provenance for the caller to record an `hdr_merge_sources` row per
/// input frame (RFC-0003 §3.6) -- `evs[i]`/`offsets[i]` correspond to
/// `inputs[i]` in the caller's own original ordering.
pub struct MergedImage {
    pub image: RgbImage,
    pub reference_idx: usize,
    pub evs: Vec<f32>,
    pub offsets: Vec<(i32, i32)>,
}

/// Standard photographic exposure-value formula:
/// `EV = log2(aperture^2 / shutter_speed) - log2(ISO / 100)`.
/// `None` if any input is missing or non-positive -- callers must not
/// substitute a guessed default, since every other frame's radiometric
/// scaling in `merge_radiance` depends on every frame's own EV being
/// genuinely correct (RFC-0003 §3.2's named gap: this can legitimately
/// happen on Windows for some RAW shapes per ADR-0003).
pub fn compute_ev(iso: Option<u32>, aperture: Option<f32>, shutter_speed: Option<f32>) -> Option<f32> {
    let iso = iso?;
    let aperture = aperture?;
    let shutter_speed = shutter_speed?;
    if iso == 0 || aperture <= 0.0 || shutter_speed <= 0.0 {
        return None;
    }
    Some((aperture * aperture / shutter_speed).log2() - (iso as f32 / 100.0).log2())
}

/// Debevec-style triangle weight: peaks at 1.0 when `z == 0.5`, zero at or
/// beyond the extremes. Evaluated on a frame's OWN unscaled decoded value
/// (RFC-0003 §3.4) -- a pixel near black carries little signal above
/// sensor noise regardless of that frame's EV, and a pixel near the
/// sensor's own clipping ceiling carries no real information at all,
/// independent of how bright or dark that frame happens to be overall.
fn weight(z: f32) -> f32 {
    if z <= 0.0 || z >= 1.0 {
        0.0
    } else if z <= 0.5 {
        z * 2.0
    } else {
        (1.0 - z) * 2.0
    }
}

fn luminance(rgb: &[f32], width: u32, height: u32) -> Vec<f32> {
    (0..(width as usize * height as usize))
        .map(|i| 0.2126 * rgb[i * 3] + 0.7152 * rgb[i * 3 + 1] + 0.0722 * rgb[i * 3 + 2])
        .collect()
}

/// Anti-aliased half-resolution downsample of a single-channel buffer,
/// via the `image` crate's own generic resize (same `Triangle` filter
/// already used elsewhere in this codebase, e.g. `preview_cache.rs`) --
/// avoids hand-rolling a second box-filter implementation just for this
/// one-channel case.
fn resize_luminance(luminance: &[f32], width: u32, height: u32, new_width: u32, new_height: u32) -> Vec<f32> {
    let buf: ImageBuffer<Luma<f32>, Vec<f32>> =
        ImageBuffer::from_raw(width, height, luminance.to_vec()).expect("buffer size matches width * height");
    let resized = image::imageops::resize(&buf, new_width, new_height, image::imageops::FilterType::Triangle);
    resized.into_raw()
}

/// A larger dimension than this gets downsampled before alignment even
/// starts -- MTB alignment targets handshake-scale misalignment (a few to
/// tens of pixels), not sub-pixel-at-full-resolution precision (RFC-0003
/// §2's named "translation only" limitation already sets that
/// expectation), so spending full-resolution-image time on it isn't
/// worth the cost on a 20-40MP bracket.
const ALIGN_BASE_MAX_DIM: u32 = 1024;
/// Search window radius (pixels) at every pyramid level, including the
/// coarsest (which therefore covers the largest real-world offset, scaled
/// up by every doubling on the way back down).
const ALIGN_SEARCH_RADIUS: i32 = 4;

struct MtbLevel {
    width: u32,
    height: u32,
    bitmap: Vec<bool>,
    /// True where a pixel is within a small band of this level's own
    /// median -- excluded from the mismatch count since these are the
    /// pixels most likely to flip bits from sensor noise alone, which
    /// would otherwise inject false mismatches into the alignment search
    /// (Ward 2003's own documented refinement over a plain median-only
    /// bitmap).
    exclusion: Vec<bool>,
}

fn build_mtb_level(luminance: &[f32], width: u32, height: u32) -> MtbLevel {
    let mut sorted: Vec<f32> = luminance.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median = sorted[sorted.len() / 2];
    let range = sorted[sorted.len() - 1] - sorted[0];
    let band = (range * 0.02).max(1e-6);
    MtbLevel {
        width,
        height,
        bitmap: luminance.iter().map(|&v| v > median).collect(),
        exclusion: luminance.iter().map(|&v| (v - median).abs() <= band).collect(),
    }
}

/// Builds a pyramid from finest (index 0, at most `ALIGN_BASE_MAX_DIM` on
/// its long side) to coarsest (halved repeatedly until either dimension
/// drops to 8px or smaller).
/// Exact 2x2 box-average halving for the pyramid's own repeated halving
/// steps -- NOT `resize_luminance`/`image::imageops::resize`'s general
/// Triangle filter, deliberately. Confirmed the hard way
/// (`align_bracket_recovers_a_known_pure_translation` initially landing
/// consistently one pixel off on both axes, not randomly): a general
/// resize filter's own sample-alignment convention introduces a small
/// but *consistent* (not noise-like) sub-pixel bias, which compounds
/// across several halving steps into a real integer-pixel error by the
/// time the pyramid search reaches the finest level. A plain, explicit
/// 2x2 block average has no such ambiguity -- each output pixel is
/// exactly the mean of one well-defined 2x2 input block, with the same
/// alignment at every level. `resize_luminance` stays in use only for
/// `align_bracket`'s own initial arbitrary-ratio cap-to-`ALIGN_BASE_MAX_DIM`
/// step, where "roughly this size" is all that's needed and the ratio
/// generally isn't a clean power of two anyway.
fn downsample_half(src: &[f32], width: u32, height: u32) -> (Vec<f32>, u32, u32) {
    let new_w = (width / 2).max(1);
    let new_h = (height / 2).max(1);
    let mut out = vec![0.0f32; new_w as usize * new_h as usize];
    for y in 0..new_h {
        for x in 0..new_w {
            let x0 = (x * 2).min(width - 1);
            let x1 = (x * 2 + 1).min(width - 1);
            let y0 = (y * 2).min(height - 1);
            let y1 = (y * 2 + 1).min(height - 1);
            let sum = src[(y0 * width + x0) as usize]
                + src[(y0 * width + x1) as usize]
                + src[(y1 * width + x0) as usize]
                + src[(y1 * width + x1) as usize];
            out[(y * new_w + x) as usize] = sum / 4.0;
        }
    }
    (out, new_w, new_h)
}

fn build_pyramid(base_luminance: &[f32], base_width: u32, base_height: u32) -> Vec<MtbLevel> {
    let mut levels = Vec::new();
    let mut cur = base_luminance.to_vec();
    let (mut w, mut h) = (base_width, base_height);
    loop {
        levels.push(build_mtb_level(&cur, w, h));
        if w <= 8 || h <= 8 {
            break;
        }
        let (new_cur, new_w, new_h) = downsample_half(&cur, w, h);
        cur = new_cur;
        (w, h) = (new_w, new_h);
    }
    levels
}

/// Returns `(mismatches, compared)` rather than a raw mismatch count.
/// **Must** be normalized to a rate (mismatches / compared) by the
/// caller, not compared as a raw count across candidate offsets: a larger
/// `|dx|`/`|dy|` shrinks the in-bounds overlap region, so a raw count
/// systematically (and wrongly) favors the offset with the *fewest*
/// pixels actually compared -- confirmed the hard way, via
/// `align_bracket_finds_no_offset_needed_for_identical_frames` initially
/// failing with a large spurious offset before this was normalized.
///
/// `(dx, dy)` uses the SAME convention `merge_radiance` reads offsets
/// with (`b` sampled at `(x - dx, y - dy)` to align onto `a`'s `(x, y)`)
/// -- confirmed the hard way too: an earlier version of this function
/// sampled `b` at `(x + dx, y + dy)`, the opposite sign, which meant
/// `align_bracket`'s output was silently negated relative to what
/// `merge_radiance` expected. `align_bracket_recovers_a_known_pure_
/// translation` is exactly the test that caught this.
fn count_mismatches(a: &MtbLevel, b: &MtbLevel, dx: i32, dy: i32) -> (u32, u32) {
    let mut mismatches = 0u32;
    let mut compared = 0u32;
    for y in 0..a.height as i32 {
        let by = y - dy;
        if by < 0 || by >= b.height as i32 {
            continue;
        }
        for x in 0..a.width as i32 {
            let bx = x - dx;
            if bx < 0 || bx >= b.width as i32 {
                continue;
            }
            let ai = (y as u32 * a.width + x as u32) as usize;
            let bi = (by as u32 * b.width + bx as u32) as usize;
            if a.exclusion[ai] || b.exclusion[bi] {
                continue;
            }
            compared += 1;
            if a.bitmap[ai] != b.bitmap[bi] {
                mismatches += 1;
            }
        }
    }
    (mismatches, compared)
}

/// Ward (2003) hierarchical MTB search: coarsest level first (an
/// exhaustive `±ALIGN_SEARCH_RADIUS` search around (0,0)), then each
/// finer level refines by doubling the previous level's best offset and
/// searching `±ALIGN_SEARCH_RADIUS` around that. Returns the offset in
/// `reference`'s own finest-pyramid-level (base) coordinates -- scaling
/// up to full-resolution pixels is the caller's job (`align_bracket`).
fn align_pyramid(reference: &[MtbLevel], target: &[MtbLevel]) -> (i32, i32) {
    let mut best = (0i32, 0i32);
    for level in (0..reference.len()).rev() {
        let center = (best.0 * 2, best.1 * 2);
        // Tie-break on distance from `center` (smallest adjustment wins),
        // not just insertion order: a small/mostly-uniform image can have
        // several offsets that all compare zero mismatches (e.g. a small
        // bright square against an otherwise-flat background, where more
        // than one shift keeps square-vs-background entirely) -- a real
        // photograph's texture makes an exact multi-way tie like this
        // vanishingly unlikely, but "prefer no correction unless the data
        // actually supports one" is the right prior regardless, and it's
        // what makes this deterministic for a synthetic test image too.
        best = (-ALIGN_SEARCH_RADIUS..=ALIGN_SEARCH_RADIUS)
            .flat_map(|dy| (-ALIGN_SEARCH_RADIUS..=ALIGN_SEARCH_RADIUS).map(move |dx| (dx, dy)))
            .map(|(dx, dy)| {
                let candidate = (center.0 + dx, center.1 + dy);
                let (mismatches, compared) = count_mismatches(&reference[level], &target[level], candidate.0, candidate.1);
                // No overlap at all is the worst possible candidate, not
                // a free win -- see count_mismatches's own doc comment.
                let score = if compared == 0 { f64::MAX } else { mismatches as f64 / compared as f64 };
                (candidate, score, dx * dx + dy * dy)
            })
            .min_by(|(_, score_a, dist_a), (_, score_b, dist_b)| {
                score_a.partial_cmp(score_b).unwrap().then(dist_a.cmp(dist_b))
            })
            .map(|(candidate, _, _)| candidate)
            .expect("search window is non-empty");
    }
    best
}

/// Aligns every frame in `frames` to `frames[reference_idx]`, returning
/// one full-resolution `(dx, dy)` pixel offset per frame in the same
/// order (the reference's own offset is always `(0, 0)`). Frames must
/// all share the same dimensions -- validated by `merge_bracket` before
/// this is called. RFC-0003 §3.3.
pub fn align_bracket(frames: &[DecodedLinear], reference_idx: usize) -> Vec<(i32, i32)> {
    let long_dim = frames[reference_idx].width.max(frames[reference_idx].height);
    let scale = long_dim.div_ceil(ALIGN_BASE_MAX_DIM).max(1);

    let pyramids: Vec<Vec<MtbLevel>> = frames
        .iter()
        .map(|f| {
            let full_lum = luminance(&f.rgb, f.width, f.height);
            if scale > 1 {
                let (base_w, base_h) = ((f.width / scale).max(1), (f.height / scale).max(1));
                let base_lum = resize_luminance(&full_lum, f.width, f.height, base_w, base_h);
                build_pyramid(&base_lum, base_w, base_h)
            } else {
                build_pyramid(&full_lum, f.width, f.height)
            }
        })
        .collect();

    let reference_pyramid = &pyramids[reference_idx];
    (0..frames.len())
        .map(|i| {
            if i == reference_idx {
                (0, 0)
            } else {
                let (dx, dy) = align_pyramid(reference_pyramid, &pyramids[i]);
                (dx * scale as i32, dy * scale as i32)
            }
        })
        .collect()
}

/// Combines `frames` (each shifted by its own `offsets[i]` onto
/// `frames[reference_idx]`'s pixel grid, and radiometrically scaled by
/// `evs[i]` relative to `evs[reference_idx]`) into one linear radiance
/// buffer at `frames[reference_idx]`'s own dimensions. Weighted
/// per-channel (not on a combined luminance) so one clipped channel
/// doesn't discard another, unclipped channel's real data at the same
/// pixel. RFC-0003 §3.4.
pub fn merge_radiance(frames: &[DecodedLinear], offsets: &[(i32, i32)], evs: &[f32], reference_idx: usize) -> Vec<f32> {
    let width = frames[reference_idx].width;
    let height = frames[reference_idx].height;
    let reference_ev = evs[reference_idx];
    // A HIGHER EV setting (bigger aperture number / faster shutter / lower
    // ISO gain) admits LESS light, so for the same scene radiance its raw
    // reading is SMALLER by a factor of 2^EV -- recovering a
    // radiance-proportional value means multiplying back UP by 2^EV, i.e.
    // `2^(ev_i - ev_ref)`, not the inverse (confirmed against the
    // hand-computed merge_radiance_matches_hand_computed_weighted_average
    // test below, which caught this sign backwards on the first pass).
    let scales: Vec<f32> = evs.iter().map(|&ev| 2f32.powf(ev - reference_ev)).collect();

    let mut out = vec![0.0f32; width as usize * height as usize * 3];
    for y in 0..height as i32 {
        for x in 0..width as i32 {
            let out_idx = (y as usize * width as usize + x as usize) * 3;
            for c in 0..3 {
                let mut sum = 0.0f32;
                let mut weight_sum = 0.0f32;
                let mut fallback_weight = -1.0f32;
                let mut fallback_value = 0.0f32;

                for (i, frame) in frames.iter().enumerate() {
                    let (dx, dy) = offsets[i];
                    let (sx, sy) = (x - dx, y - dy);
                    if sx < 0 || sy < 0 || sx >= frame.width as i32 || sy >= frame.height as i32 {
                        continue;
                    }
                    let raw = frame.rgb[(sy as usize * frame.width as usize + sx as usize) * 3 + c];
                    let w = weight(raw);
                    let radiance = raw * scales[i];

                    if w > 0.0 {
                        sum += w * radiance;
                        weight_sum += w;
                    }
                    if w > fallback_weight {
                        fallback_weight = w;
                        fallback_value = radiance;
                    }
                }

                // All frames clipped/out-of-bounds at this pixel/channel --
                // fall back to whichever frame's own weight was highest,
                // rather than leaving a zero/undefined value (RFC §3.4).
                out[out_idx + c] = if weight_sum > 0.0 { sum / weight_sum } else { fallback_value };
            }
        }
    }
    out
}

/// Global Reinhard tone-mapping (`L_out = L_in / (1 + L_in)`, per
/// channel) -- monotonic and bounded to `[0, 1)` for any non-negative
/// finite input, unlike a naive linear rescale-by-max which needs prior
/// knowledge of the buffer's own maximum. RFC-0003 §3.5.
pub fn tone_map(radiance: &[f32], width: u32, height: u32) -> RgbImage {
    let mut img = RgbImage::new(width, height);
    for (i, pixel) in img.pixels_mut().enumerate() {
        for c in 0..3 {
            let l = radiance[i * 3 + c].max(0.0);
            let mapped = l / (1.0 + l);
            pixel[c] = (mapped * 255.0).round().clamp(0.0, 255.0) as u8;
        }
    }
    img
}

/// Orchestrates the full pipeline: EV computation (§3.2) -> linear decode
/// (§3.1) -> alignment (§3.3) -> radiometric merge (§3.4) -> tone mapping
/// (§3.5). The catalog/file-writing half (§3.6) is the caller's job
/// (`lib.rs`'s `merge_hdr_bracket` command) -- this function has no
/// catalog dependency and produces an in-memory `RgbImage`.
pub fn merge_bracket(inputs: &[BracketInput]) -> Result<MergedImage, HdrMergeError> {
    if inputs.len() < 2 {
        return Err(HdrMergeError::NotEnoughFrames(inputs.len()));
    }

    let evs: Vec<f32> = inputs
        .iter()
        .map(|input| {
            compute_ev(input.iso, input.aperture, input.shutter_speed)
                .ok_or_else(|| HdrMergeError::MissingExposureInfo(input.path.display().to_string()))
        })
        .collect::<Result<_, _>>()?;

    let frames: Vec<DecodedLinear> = inputs
        .iter()
        .map(|input| {
            raw_decode::decode_linear(&input.path)
                .map_err(|e| HdrMergeError::Decode(input.path.display().to_string(), e.to_string()))
        })
        .collect::<Result<_, _>>()?;

    let (w0, h0) = (frames[0].width, frames[0].height);
    for f in &frames[1..] {
        if f.width != w0 || f.height != h0 {
            return Err(HdrMergeError::DimensionMismatch(w0, h0, f.width, f.height));
        }
    }

    // Reference = frame whose EV is closest to the group's median --
    // avoids picking either extreme, which tend to have the least
    // reliable midtone detail to align other frames against.
    let mut sorted_evs = evs.clone();
    sorted_evs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median_ev = sorted_evs[sorted_evs.len() / 2];
    let reference_idx = evs
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| (**a - median_ev).abs().partial_cmp(&(**b - median_ev).abs()).unwrap())
        .map(|(i, _)| i)
        .expect("evs is non-empty, checked by NotEnoughFrames above");

    let offsets = align_bracket(&frames, reference_idx);
    let radiance = merge_radiance(&frames, &offsets, &evs, reference_idx);
    let image = tone_map(&radiance, w0, h0);

    Ok(MergedImage { image, reference_idx, evs, offsets })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_ev_matches_hand_computed_values() {
        // ISO 100, f/8, 1/125s -- a common "sunny 16"-ish daylight exposure.
        // EV = log2(8^2 / (1/125)) - log2(100/100) = log2(64 * 125) - 0
        //    = log2(8000) ≈ 12.9658
        let ev = compute_ev(Some(100), Some(8.0), Some(1.0 / 125.0)).unwrap();
        assert!((ev - 12.9658).abs() < 0.001, "got {ev}");
    }

    #[test]
    fn compute_ev_is_one_stop_apart_for_a_one_stop_shutter_change() {
        let base = compute_ev(Some(100), Some(4.0), Some(1.0 / 100.0)).unwrap();
        let one_stop_brighter = compute_ev(Some(100), Some(4.0), Some(1.0 / 50.0)).unwrap();
        assert!((base - one_stop_brighter - 1.0).abs() < 0.001);
    }

    #[test]
    fn compute_ev_is_none_when_any_input_is_missing_or_invalid() {
        assert!(compute_ev(None, Some(4.0), Some(0.01)).is_none());
        assert!(compute_ev(Some(100), None, Some(0.01)).is_none());
        assert!(compute_ev(Some(100), Some(4.0), None).is_none());
        assert!(compute_ev(Some(0), Some(4.0), Some(0.01)).is_none());
        assert!(compute_ev(Some(100), Some(0.0), Some(0.01)).is_none());
        assert!(compute_ev(Some(100), Some(4.0), Some(0.0)).is_none());
    }

    #[test]
    fn weight_peaks_at_midtone_and_is_zero_at_the_extremes() {
        assert_eq!(weight(0.5), 1.0);
        assert_eq!(weight(0.0), 0.0);
        assert_eq!(weight(1.0), 0.0);
        assert_eq!(weight(-0.1), 0.0);
        assert_eq!(weight(1.1), 0.0);
        assert!(weight(0.25) > 0.0 && weight(0.25) < 1.0);
        assert!((weight(0.25) - weight(0.75)).abs() < 1e-6, "should be symmetric around 0.5");
    }

    fn synthetic_frame(width: u32, height: u32, pattern: impl Fn(u32, u32) -> f32) -> DecodedLinear {
        let mut rgb = Vec::with_capacity(width as usize * height as usize * 3);
        for y in 0..height {
            for x in 0..width {
                let v = pattern(x, y);
                rgb.extend_from_slice(&[v, v, v]);
            }
        }
        DecodedLinear { width, height, rgb }
    }

    /// A bright square over a *textured* (not flat) background. A flat
    /// background would put every background pixel exactly at the whole
    /// image's median, so the exclusion band (meant to drop only the thin
    /// slice of pixels near the median, i.e. likely-noise) would instead
    /// exclude the ENTIRE background on both sides -- leaving only
    /// whatever the two squares' footprints happen to overlap as
    /// "comparable," which can produce a false zero-mismatch score at the
    /// wrong offset purely from that overlap, never actually exercising
    /// the misalignment signal the background pixels should be
    /// contributing (confirmed the hard way: this test originally used a
    /// flat background and failed for exactly this reason). Good enough
    /// for `align_bracket_finds_no_offset_needed_for_identical_frames`,
    /// which only needs SOME non-degenerate content, not multi-scale
    /// robustness -- see `multiscale_frame` for the pyramid-depth-aware
    /// pattern `align_bracket_recovers_a_known_pure_translation` needs.
    fn bright_square(x0: i32, y0: i32, x1: i32, y1: i32) -> impl Fn(u32, u32) -> f32 {
        move |x: u32, y: u32| {
            let (xi, yi) = (x as i32, y as i32);
            if xi >= x0 && xi < x1 && yi >= y0 && yi < y1 {
                0.8
            } else {
                0.1 + 0.02 * ((x * 7 + y * 13) % 5) as f32
            }
        }
    }

    /// Concentric squares of decreasing size (largest/dimmest to
    /// smallest/brightest, all centered on the image), rather than one
    /// small square over a background texture. Built specifically for
    /// `align_bracket_recovers_a_known_pure_translation`, whose pyramid
    /// downsamples all the way to an 8x8 coarsest level: a single small
    /// square (or worse, a fine periodic texture -- an earlier version of
    /// this test tried both, see git history) simply disappears or
    /// aliases into incoherent noise after a few 2x2 box-average
    /// halvings, leaving the coarse levels nothing real to search against
    /// (confirmed the hard way with a temporary `HDR_MERGE_DEBUG_ALIGN`
    /// env-gated eprintln in `align_pyramid`: coarse levels either locked
    /// onto aliased-texture noise unrelated to the true shift, or -- with
    /// a smooth monotonic gradient, tried in between -- had no interior
    /// minimum at all and walked the search window out to its edge every
    /// level). Nested squares fix both problems at once: at any pyramid
    /// level, whichever squares haven't yet shrunk below a pixel or two
    /// still contribute a real, correctly-positioned edge for the MTB
    /// bitmap to key on, so there's always a genuine multi-scale signal
    /// to refine from coarsest to finest.
    ///
    /// A smooth, low-amplitude 2D wave is added on top of the flat ring
    /// values for a THIRD reason, found after the above two were already
    /// fixed: each ring here is otherwise a large area of one EXACT
    /// constant value, and `build_mtb_level`'s exclusion band is a
    /// numeric range around the whole level's median (not a fixed pixel
    /// count) -- so if that median happens to land exactly on one ring's
    /// constant value (as it did here: at level3 in this test, every
    /// pixel in the 0.2 ring, roughly 30% of the image, sat exactly at
    /// the level's own median and got entirely excluded as one single
    /// solid buffer, wide enough to fully absorb the test's true few-
    /// pixel shift with zero mismatches at every candidate offset --
    /// confirmed the hard way via a temporary per-candidate score-grid
    /// dump gated on `HDR_MERGE_DEBUG_ALIGN_LEVEL`). Real photographs
    /// don't have this problem because real tonal regions are never
    /// perfectly flat over a wide area; adding gentle continuous
    /// variation here breaks the exact-value plateau the same way, so the
    /// excluded band only ever touches a thin, genuinely-near-median
    /// slice of pixels again, not an entire ring.
    fn multiscale_frame(width: u32, height: u32) -> DecodedLinear {
        let (cx, cy) = (width as i32 / 2, height as i32 / 2);
        let rings: [(i32, f32); 5] = [
            (width as i32 * 3 / 4, 0.2),
            (width as i32 / 2, 0.35),
            (width as i32 / 4, 0.5),
            (width as i32 / 8, 0.65),
            (width as i32 / 16, 0.8),
        ];
        synthetic_frame(width, height, move |x, y| {
            let (xi, yi) = (x as i32, y as i32);
            let mut value = 0.1;
            for &(size, v) in &rings {
                let half = size / 2;
                if xi >= cx - half && xi < cx + half && yi >= cy - half && yi < cy + half {
                    value = v;
                }
            }
            let wave = (0.5 + 0.5 * (x as f32 * 0.13).sin()) * (0.5 + 0.5 * (y as f32 * 0.11).sin());
            value + 0.05 * wave
        })
    }

    /// Produces `target(x, y) = base(x - shift_x, y - shift_y)` (edge-
    /// clamped) by actually re-sampling `base`'s own pixel buffer, rather
    /// than recomputing a *formula* at the shifted position -- confirmed
    /// the hard way that those two are NOT the same thing whenever the
    /// formula mixes an absolute-coordinate term (this file's own
    /// `bright_square`'s background texture, `(x*7 + y*13) % 5`, is keyed
    /// to absolute pixel position and is not itself shift-invariant). An
    /// earlier version of this test built "reference" and "target" from
    /// two separate `bright_square(...)` calls with only the rectangle
    /// bounds shifted -- which moved the square but left the *background*
    /// texture unshifted, silently violating the test's own claimed
    /// `target(p) = reference(p - shift)` ground truth outside the
    /// square. That mismatched background texture pattern was actually
    /// the dominant "signal" `align_pyramid` was correctly finding the
    /// best alignment for (reproducibly landing on (-3, 2), not the
    /// intended (-2, 1)) -- a bug in the test's synthetic data, not in
    /// the alignment algorithm itself. Re-sampling the same buffer avoids
    /// the problem entirely: every pixel, texture included, genuinely
    /// satisfies the shift relation.
    fn shift_frame(base: &DecodedLinear, shift_x: i32, shift_y: i32) -> DecodedLinear {
        let (width, height) = (base.width, base.height);
        let mut rgb = vec![0.0f32; base.rgb.len()];
        for y in 0..height as i32 {
            for x in 0..width as i32 {
                let sx = (x - shift_x).clamp(0, width as i32 - 1) as u32;
                let sy = (y - shift_y).clamp(0, height as i32 - 1) as u32;
                let src_idx = (sy as usize * width as usize + sx as usize) * 3;
                let dst_idx = (y as usize * width as usize + x as usize) * 3;
                rgb[dst_idx..dst_idx + 3].copy_from_slice(&base.rgb[src_idx..src_idx + 3]);
            }
        }
        DecodedLinear { width, height, rgb }
    }

    #[test]
    fn align_bracket_recovers_a_known_pure_translation() {
        // 1024x1024 -- matching ALIGN_BASE_MAX_DIM exactly (so `scale ==
        // 1` in `align_bracket` and the pyramid built here is structurally
        // identical to what a real, already-1024-capped bracket frame
        // would get), with a `multiscale_frame` pattern that keeps real
        // structure at every pyramid octave down to the 8x8 coarsest
        // level (see its own doc comment for why a single small square or
        // a fine texture doesn't survive that).
        //
        // `target` is built by re-sampling the reference buffer itself so
        // that `target(p) = reference(p - shift)` holds EXACTLY,
        // everywhere, with shift = (30, -18) (see `shift_frame`'s doc
        // comment for why that has to be a literal re-sample, not a
        // second formula evaluation) -- a couple percent of the image's
        // own size, in line with the "handshake-scale" misalignment this
        // feature targets (RFC-0003 §2). Per `merge_radiance`'s own (dx,
        // dy) convention (reading frame_i at (x - dx, y - dy) to align
        // onto the reference), the offset that should be recovered is
        // (-shift.x, -shift.y) = (-30, 18).
        let reference = multiscale_frame(1024, 1024);
        let target = shift_frame(&reference, 30, -18);

        let offsets = align_bracket(&[reference, target], 0);

        assert_eq!(offsets[0], (0, 0), "reference's own offset must always be (0, 0)");
        assert_eq!(offsets[1], (-30, 18));
    }

    #[test]
    fn align_bracket_finds_no_offset_needed_for_identical_frames() {
        let a = synthetic_frame(32, 32, bright_square(8, 8, 16, 16));
        let b = synthetic_frame(32, 32, bright_square(8, 8, 16, 16));
        let offsets = align_bracket(&[a, b], 0);
        assert_eq!(offsets, vec![(0, 0), (0, 0)]);
    }

    #[test]
    fn merge_radiance_matches_hand_computed_weighted_average() {
        // Two 1x1 "frames": a mid-gray reference frame at EV 0, and a
        // frame at EV -1 (half the exposure) whose own raw value happens
        // to be exactly double the reference's (i.e. the same true scene
        // radiance, captured one stop darker) -- both frames should
        // therefore agree once EV-scaled, and the merge should reproduce
        // that same value.
        let reference = DecodedLinear { width: 1, height: 1, rgb: vec![0.4, 0.4, 0.4] };
        let darker = DecodedLinear { width: 1, height: 1, rgb: vec![0.2, 0.2, 0.2] };
        let offsets = vec![(0, 0), (0, 0)];
        let evs = vec![0.0, 1.0]; // darker frame is 1 stop under-exposed relative to reference

        let radiance = merge_radiance(&[reference, darker], &offsets, &evs, 0);

        for c in 0..3 {
            assert!(
                (radiance[c] - 0.4).abs() < 0.01,
                "channel {c}: expected ~0.4, got {}",
                radiance[c]
            );
        }
    }

    #[test]
    fn merge_radiance_excludes_an_out_of_bounds_shifted_sample() {
        // A large positive offset pushes every source sample out of
        // bounds for a 1x1 frame -- the merge must fall back to the
        // other (in-bounds) frame's value, not read garbage or panic.
        let reference = DecodedLinear { width: 1, height: 1, rgb: vec![0.5, 0.5, 0.5] };
        let unreachable = DecodedLinear { width: 1, height: 1, rgb: vec![0.9, 0.9, 0.9] };
        let offsets = vec![(0, 0), (100, 100)];
        let evs = vec![0.0, 0.0];

        let radiance = merge_radiance(&[reference, unreachable], &offsets, &evs, 0);
        for c in 0..3 {
            assert!((radiance[c] - 0.5).abs() < 1e-6, "should fall back to the only in-bounds frame");
        }
    }

    #[test]
    fn tone_map_is_monotonic_and_bounded() {
        let radiance = vec![0.0, 0.5, 1.0, 5.0, 100.0, 1000.0];
        let width = radiance.len() as u32;
        // Repeat each value across 3 channels for a valid RGB buffer.
        let rgb: Vec<f32> = radiance.iter().flat_map(|&v| [v, v, v]).collect();

        let img = tone_map(&rgb, width, 1);

        // L/(1+L) is mathematically strictly < 1.0 for any finite L, but
        // that doesn't survive rounding to the nearest u8 for large L
        // (e.g. L=1000 -> 0.999*255 = 254.745, which rounds UP to 255) --
        // the real invariant worth asserting is monotonicity and staying
        // in u8 range, not an unrounded mathematical bound the u8 output
        // type can't actually represent anyway.
        let mut prev = -1i32;
        for pixel in img.pixels() {
            let v = pixel[0] as i32;
            assert!(v >= prev, "tone mapping should be monotonically non-decreasing in input");
            assert!((0..=255).contains(&v));
            prev = v;
        }
    }

    #[test]
    fn merge_bracket_rejects_fewer_than_two_frames() {
        let inputs = vec![BracketInput {
            path: PathBuf::from("/nonexistent.dng"),
            iso: Some(100),
            aperture: Some(8.0),
            shutter_speed: Some(0.01),
        }];
        let result = merge_bracket(&inputs);
        assert!(matches!(result, Err(HdrMergeError::NotEnoughFrames(1))));
    }

    #[test]
    fn merge_bracket_rejects_missing_exposure_info_before_decoding_anything() {
        // Both paths are nonexistent -- if this returned a Decode error
        // instead of MissingExposureInfo, it would mean decode was
        // attempted before exposure validation, which is the wrong order
        // (RFC-0003 §3.2: reject fast on a known-bad input set before
        // paying for any real decode work).
        let inputs = vec![
            BracketInput { path: PathBuf::from("/a.dng"), iso: Some(100), aperture: Some(8.0), shutter_speed: Some(0.01) },
            BracketInput { path: PathBuf::from("/b.dng"), iso: None, aperture: Some(8.0), shutter_speed: Some(0.005) },
        ];
        let result = merge_bracket(&inputs);
        assert!(matches!(result, Err(HdrMergeError::MissingExposureInfo(_))));
    }
}
