/// White balance adjustment (Temperature and Tint).
/// Temperature shifts warm (yellow) vs cool (blue).
/// Tint shifts magenta vs green.
pub(super) fn apply_white_balance(rgb: [f32; 3], temperature: f32, tint: f32) -> [f32; 3] {
    let mut c = rgb;
    let t = temperature / 100.0;
    let tint_norm = tint / 100.0;

    c[0] *= 1.0 + 0.35 * t + 0.15 * tint_norm;
    c[1] *= 1.0 - 0.35 * tint_norm;
    c[2] *= 1.0 - 0.35 * t + 0.15 * tint_norm;
    c
}

/// Parametric Tone expansion (Highlights, Shadows, Whites, Blacks).
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
    [rgb[0] + delta, rgb[1] + delta, rgb[2] + delta]
}

/// Local adjustments for masks (exposure, contrast, saturation).
pub(super) fn apply_adjustments(rgb: [f32; 3], exposure_ev: f32, contrast: f32, saturation: f32) -> [f32; 3] {
    let mut c = rgb;
    for v in c.iter_mut() {
        *v *= 2f32.powf(exposure_ev);
    }
    for v in c.iter_mut() {
        *v = (*v - 0.5) * (1.0 + contrast / 100.0) + 0.5;
    }
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
    for v in c.iter_mut() {
        *v *= 2f32.powf(exposure_ev);
    }
    for v in c.iter_mut() {
        *v = (*v - 0.5) * (1.0 + contrast / 100.0) + 0.5;
    }
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
