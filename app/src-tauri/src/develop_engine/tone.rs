use super::*;

/// White balance adjustment (Temperature and Tint). RFC-0017: a real
/// per-channel gain on LIGHT, so it's computed in linear space
/// (`srgb_to_linear`/`linear_to_srgb` bracket it) rather than applied
/// directly to the gamma-encoded value -- applying a physical gain
/// post-gamma is a different, wrong transform, not a stylistic
/// difference (see RFC-0017 SS3.1).
///
/// Temperature shifts warm (yellow) vs cool (blue). Tint shifts magenta
/// vs green. The three raw per-channel coefficients below are unchanged
/// from before RFC-0017 (they already move the right channel the right
/// direction) -- what's new is normalizing them by the LINEAR-light
/// luma they'd produce on a neutral gray, so a purely-hue-shifting WB
/// adjustment cannot ALSO change overall physical brightness as a side
/// effect. Before this normalization, `temperature=100` measurably
/// brightened a neutral gray's (gamma-space) value by ~5%
/// (`luma([1.35,1.0,0.65]) ≈ 1.049`) -- a real, described bug, not just
/// an approximation gap. See RFC-0017 SS3.2.
///
/// The guarantee this normalization actually proves is LINEAR luma
/// preservation, not gamma-space luma of the three (individually
/// re-encoded) output channels -- those are NOT the same quantity once
/// the channels diverge, since `linear_to_srgb` is a nonlinear (concave)
/// per-channel function and the weighted sum of encoded values isn't
/// the encoding of the weighted sum (Jensen's inequality). This
/// module's own tests check the linear-domain claim specifically, after
/// an initial version checked the wrong (gamma-domain) one and failed
/// at the extremes -- see that test's own doc comment for the full
/// finding.
pub(super) fn apply_white_balance(rgb: [f32; 3], temperature: f32, tint: f32) -> [f32; 3] {
    let t = temperature / 100.0;
    let tint_norm = tint / 100.0;

    let g_r = 1.0 + 0.35 * t + 0.15 * tint_norm;
    let g_g = 1.0 - 0.35 * tint_norm;
    let g_b = 1.0 - 0.35 * t + 0.15 * tint_norm;
    let luma_of_gray = g_r * 0.2126 + g_g * 0.7152 + g_b * 0.0722;
    let (g_r, g_g, g_b) = (g_r / luma_of_gray, g_g / luma_of_gray, g_b / luma_of_gray);

    [
        linear_to_srgb(srgb_to_linear(rgb[0]) * g_r),
        linear_to_srgb(srgb_to_linear(rgb[1]) * g_g),
        linear_to_srgb(srgb_to_linear(rgb[2]) * g_b),
    ]
}

/// RFC-0017: Exposure is a real gain on LIGHT (a genuine photographic
/// stop), so it's computed in linear space -- see `apply_white_balance`'s
/// own doc comment for the same reasoning. Factored out here (previously
/// duplicated inline in `apply_global_adjustments` and `apply_adjustments`
/// below) so global and local-mask Exposure are provably the same
/// function, not just visually similar formulas kept in sync by hand.
///
/// Honest, described consequence, not a hidden side effect: because
/// gamma compression is steepest near black, a nominal `+1 EV` now
/// produces a visibly GENTLER brightening of a midtone than the old
/// direct-gamma-multiply did (a mid-gray at 100/255 lands around
/// 138/255, not 200/255) -- this is the fix, not a regression to
/// compensate for by rescaling the EV numbers. A real camera stop
/// behaves exactly this way once gamma is accounted for correctly.
pub(super) fn apply_exposure(rgb: [f32; 3], exposure_ev: f32) -> [f32; 3] {
    let gain = 2f32.powf(exposure_ev);
    [
        linear_to_srgb(srgb_to_linear(rgb[0]) * gain),
        linear_to_srgb(srgb_to_linear(rgb[1]) * gain),
        linear_to_srgb(srgb_to_linear(rgb[2]) * gain),
    ]
}

/// RFC-0017: Contrast as a smoothstep-blended S-curve, replacing the old
/// hard linear stretch (`(v-0.5)*(1+contrast/100)+0.5`), which ran
/// straight into the final [0,1] clamp with no roll-off -- an abrupt cut
/// rather than the smooth compression a real Contrast curve gives
/// shadows/highlights. Reuses this module's own `smoothstep` primitive
/// (already used by Vignette/Split Toning/Parametric Tone) rather than
/// introducing a new curve family. At `amount=0` this is the exact
/// identity; at `amount=1` (Contrast=100) it reduces exactly to
/// `smoothstep(0,1,v)`, a real S-curve whose derivative is 0 at both
/// v=0 and v=1 (smooth roll-off at black/white) and 1.5 at the pivot
/// (steeper midtones -- the actual "more contrast" effect). Stays in
/// the same gamma/perceptual domain Contrast always operated in --
/// unlike White Balance/Exposure above, Contrast is not a linear-light
/// gain, so it is NOT bracketed by srgb_to_linear/linear_to_srgb.
/// Honesty caveat (RFC-0017 SS3.3, same class as RFC-0016's): this
/// specific curve family is a reasoned, well-behaved choice, not a
/// verified match to Lightroom's own unpublished Contrast curve shape.
pub(super) fn apply_contrast(rgb: [f32; 3], contrast: f32) -> [f32; 3] {
    let amount = (contrast / 100.0).clamp(-1.0, 1.0);
    rgb.map(|v| v + amount * (smoothstep(0.0, 1.0, v) - v))
}

/// Parametric Tone expansion (Highlights, Shadows, Whites, Blacks).
///
/// RFC-0018: the four windows below combine into a single scalar `delta`
/// added identically to R/G/B -- hue-preserving in isolation (adding the
/// same constant to all three channels never changes their pairwise
/// differences, which is what determines hue), but this op is precisely
/// the one used on pixels already near the tonal extremes. The pipeline
/// clamps to `[0,1]` only once, at the very final byte conversion
/// (`pipeline.rs`), after the whole unclamped-float op chain runs -- so
/// without the shared-scale step below, one channel of a near-clip
/// saturated pixel could get silently truncated more than its siblings
/// at that final clamp, breaking the pairwise-difference equality and
/// shifting hue rather than desaturating predictably. See the RFC's own
/// worked example (an orange `[0.95,0.70,0.50]` at `highlights=100`)
/// for the exact before/after numbers.
pub(super) fn apply_parametric_tone(
    rgb: [f32; 3],
    highlights: f32,
    shadows: f32,
    whites: f32,
    blacks: f32,
) -> [f32; 3] {
    let luma = rgb[0] * 0.2126 + rgb[1] * 0.7152 + rgb[2] * 0.0722;

    // Highlights: smooth transition above 0.5
    let w_h = if luma > 0.5 {
        let t = (luma - 0.5) / 0.5;
        t * t * (3.0 - 2.0 * t)
    } else {
        0.0
    };
    let delta_h = (highlights / 100.0) * w_h * (1.0 - luma) * 0.6;

    // Shadows: smooth transition below 0.5
    let w_s = if luma < 0.5 {
        let t = (0.5 - luma) / 0.5;
        t * t * (3.0 - 2.0 * t)
    } else {
        0.0
    };
    let delta_s = (shadows / 100.0) * w_s * luma * 0.6;

    // Whites: extreme highlights (above 0.7)
    let w_w = if luma > 0.7 {
        let t = (luma - 0.7) / 0.3;
        t * t * (3.0 - 2.0 * t)
    } else {
        0.0
    };
    let delta_w = (whites / 100.0) * w_w * 0.35;

    // Blacks: extreme deep shadows (below 0.3)
    let w_b = if luma < 0.3 {
        let t = (0.3 - luma) / 0.3;
        t * t * (3.0 - 2.0 * t)
    } else {
        0.0
    };
    let delta_b = (blacks / 100.0) * w_b * 0.35;

    let delta = delta_h + delta_s + delta_w + delta_b;
    if delta == 0.0 {
        return rgb;
    }

    // RFC-0018: a shared scale, not an independent per-channel clamp --
    // the SAME (possibly reduced) delta must reach every channel to keep
    // pairwise differences (hence hue) exact. `headroom` is how far this
    // one channel can move in `delta`'s own direction before hitting
    // 0 or 1; a channel already past that edge in the same direction
    // (possible since an earlier op like Exposure can overshoot before
    // the pipeline's single final clamp) yields a negative headroom,
    // forcing `scale` to 0 -- this op simply contributes nothing further
    // there rather than adding to an overshoot that isn't its own to fix.
    let mut scale = 1.0f32;
    for &c in &rgb {
        let headroom = if delta > 0.0 { 1.0 - c } else { c };
        scale = scale.min(headroom / delta.abs());
    }
    scale = scale.max(0.0);

    let applied = delta * scale;
    [rgb[0] + applied, rgb[1] + applied, rgb[2] + applied]
}

/// Local adjustments for masks (exposure, contrast, saturation). RFC-0017:
/// Exposure/Contrast now go through the exact same `apply_exposure`/
/// `apply_contrast` the global chain below uses -- previously each had
/// its own inline copy of the same formula, kept in sync by hand rather
/// than provably identical; a test now checks the two paths agree on
/// the same input rather than assuming it from the refactor alone.
pub(super) fn apply_adjustments(rgb: [f32; 3], exposure_ev: f32, contrast: f32, saturation: f32) -> [f32; 3] {
    let mut c = apply_exposure(rgb, exposure_ev);
    c = apply_contrast(c, contrast);
    let luma = c[0] * 0.2126 + c[1] * 0.7152 + c[2] * 0.0722;
    for v in c.iter_mut() {
        *v = luma + (*v - luma) * (1.0 + saturation / 100.0);
    }
    c
}

/// Global adjustments (white balance, exposure, contrast, parametric tone, saturation).
pub(super) fn apply_global_adjustments(
    rgb: [f32; 3],
    exposure_ev: f32,
    contrast: f32,
    saturation: f32,
    temperature: f32,
    tint: f32,
    highlights: f32,
    shadows: f32,
    whites: f32,
    blacks: f32,
) -> [f32; 3] {
    let mut c = apply_white_balance(rgb, temperature, tint);
    c = apply_exposure(c, exposure_ev);
    c = apply_contrast(c, contrast);
    c = apply_parametric_tone(c, highlights, shadows, whites, blacks);
    let luma = c[0] * 0.2126 + c[1] * 0.7152 + c[2] * 0.0722;
    for v in c.iter_mut() {
        *v = luma + (*v - luma) * (1.0 + saturation / 100.0);
    }
    c
}

/// Tone Curve (M3): a global-only op applied as the pipeline's new final
/// step -- exposure -> contrast -> saturation -> tone curve -- before any
/// mask reads the graded `rgb` as its own accumulator base (see
/// `apply_edit_stack`'s call site below and `Mask::weight`'s own doc
/// comment on that ordering). Number of samples in the shared LUT both
/// this module and DevelopCanvas.svelte's WGSL shader build and consume --
/// see `build_curve_lut`'s doc comment for why both sides build the exact
/// same discretized LUT rather than each evaluating the spline exactly.
pub(super) const CURVE_LUT_SAMPLES: usize = 256;

/// Parses the `tone_curve` op's `points` array, defaulting to the 2-point
/// identity line `(0,0)-(1,1)` -- the same "always a clean passthrough for
/// an untouched image" contract `op_value` already establishes for the
/// scalar ops. Falls back to identity (not a panic) if fewer than 2 points
/// parse, e.g. a corrupt/partial payload.
pub(super) fn tone_curve_points(ops: &[serde_json::Value]) -> Vec<(f32, f32)> {
    let parsed = ops
        .iter()
        .find(|op| op.get("op").and_then(|v| v.as_str()) == Some("tone_curve"))
        .and_then(|op| op.get("points"))
        .and_then(|v| v.as_array())
        .map(|arr| {
            let mut pts: Vec<(f32, f32)> = arr
                .iter()
                .filter_map(|p| Some((p.get("x")?.as_f64()? as f32, p.get("y")?.as_f64()? as f32)))
                .collect();
            pts.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
            pts
        });
    match parsed {
        Some(pts) if pts.len() >= 2 => pts,
        _ => vec![(0.0, 0.0), (1.0, 1.0)],
    }
}

/// Fritsch-Carlson monotonic cubic Hermite tangents -- the Rust twin of
/// `develop.js`'s `computeTangents` (same algorithm, kept in lockstep by
/// hand; see this module's own doc comment on the parity obligation with
/// the WGSL shader, which applies equally here). Step C (necessary-
/// condition zeroing at local extrema) is NOT redundant with Step D's
/// magnitude-only rescale: a plain averaged tangent can have the wrong
/// SIGN at an extremum, which only Step C catches. `pts` must already be
/// sorted by x.
pub(super) fn compute_tangents(pts: &[(f32, f32)]) -> Vec<f32> {
    let n = pts.len();
    let d: Vec<f32> = (0..n - 1)
        .map(|k| (pts[k + 1].1 - pts[k].1) / (pts[k + 1].0 - pts[k].0))
        .collect();
    let mut m = vec![0.0f32; n];
    m[0] = d[0];
    m[n - 1] = d[n - 2];
    for k in 1..n - 1 {
        m[k] = (d[k - 1] + d[k]) / 2.0;
    }
    for k in 1..n - 1 {
        if d[k - 1] == 0.0 || d[k] == 0.0 || d[k - 1].signum() != d[k].signum() {
            m[k] = 0.0;
        }
    }
    for k in 0..n - 1 {
        if d[k] == 0.0 {
            m[k] = 0.0;
            m[k + 1] = 0.0;
            continue;
        }
        let alpha = m[k] / d[k];
        let beta = m[k + 1] / d[k];
        let s = alpha * alpha + beta * beta;
        if s > 9.0 {
            let tau = 3.0 / s.sqrt();
            m[k] = tau * alpha * d[k];
            m[k + 1] = tau * beta * d[k];
        }
    }
    m
}

/// Evaluates the Hermite cubic through `pts` (with tangents `m`) at `x`,
/// clamped to the curve's own domain.
pub(super) fn hermite_at(pts: &[(f32, f32)], m: &[f32], x: f32) -> f32 {
    let n = pts.len();
    let xc = x.clamp(pts[0].0, pts[n - 1].0);
    let mut k = 0;
    while k < n - 2 && xc > pts[k + 1].0 {
        k += 1;
    }
    let h = pts[k + 1].0 - pts[k].0;
    let t = if h == 0.0 { 0.0 } else { (xc - pts[k].0) / h };
    let (t2, t3) = (t * t, t * t * t);
    let h00 = 2.0 * t3 - 3.0 * t2 + 1.0;
    let h10 = t3 - 2.0 * t2 + t;
    let h01 = -2.0 * t3 + 3.0 * t2;
    let h11 = t3 - t2;
    h00 * pts[k].1 + h10 * h * m[k] + h01 * pts[k + 1].1 + h11 * h * m[k + 1]
}

/// The Rust twin of `develop.js`'s `buildToneCurveLut` -- must build the
/// SAME discretized `CURVE_LUT_SAMPLES`-sample LUT the WGSL shader
/// consumes, not evaluate the spline exactly, so parity between the
/// interactive preview and this module's own CPU export/thumbnail-regen
/// output is by construction rather than relying on this module's own
/// `±2/255` test tolerance to paper over two structurally different
/// evaluation methods near a curve's extrema.
pub(super) fn build_curve_lut(points: &[(f32, f32)]) -> [f32; CURVE_LUT_SAMPLES] {
    let m = compute_tangents(points);
    let mut lut = [0.0f32; CURVE_LUT_SAMPLES];
    for (i, entry) in lut.iter_mut().enumerate() {
        let x = i as f32 / (CURVE_LUT_SAMPLES - 1) as f32;
        *entry = hermite_at(points, &m, x).clamp(0.0, 1.0);
    }
    lut
}

/// Same linear-interpolation-between-samples formula as `develop.js`'s
/// `sampleCurveLut` and the WGSL shader's `sampleCurveLut`.
pub(super) fn sample_lut(lut: &[f32], v: f32) -> f32 {
    let idx = v.clamp(0.0, 1.0) * (lut.len() - 1) as f32;
    let i0 = idx.floor() as usize;
    let i1 = (i0 + 1).min(lut.len() - 1);
    let frac = idx - i0 as f32;
    lut[i0] * (1.0 - frac) + lut[i1] * frac
}
