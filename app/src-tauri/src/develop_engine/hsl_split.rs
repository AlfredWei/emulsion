use super::*;

/// HSL / Color Mixer (M3): a global-only op, applied as the pipeline's new
/// final step -- exposure -> contrast -> saturation -> tone curve -> HSL
/// -- before any mask reads the graded `rgb` as its own accumulator base
/// (see `apply_edit_stack`'s call site below). Band order/centers must
/// stay in lockstep by hand with `develop.js`'s `HSL_BAND_NAMES`/
/// `HSL_BAND_CENTERS_DEG` and DevelopCanvas.svelte's WGSL twin -- this
/// module's own doc comment already establishes that parity obligation
/// generally, for every formula here.
pub(super) const HSL_BAND_NAMES: [&str; 8] = ["red", "orange", "yellow", "green", "aqua", "blue", "purple", "magenta"];

/// RFC-0019: centers sit at the hue positions of the colors the band names
/// actually refer to (yellow at 60, not 90; green at 120, not 135; ...),
/// so the gaps between neighbors are UNEQUAL (30/30/60/60/60/30/30/60) --
/// which is why `hsl_band_weight` below blends between adjacent centers
/// using each side's own gap instead of one shared half-width. Not a
/// verified match to Lightroom's own (unpublished) centers.
pub(super) const HSL_BAND_CENTERS_DEG: [f32; 8] = [0.0, 30.0, 60.0, 120.0, 180.0, 240.0, 270.0, 300.0];

#[derive(Clone, Copy)]
pub(super) struct HslBand {
    pub(super) hue: f32,
    pub(super) saturation: f32,
    pub(super) luminance: f32,
}

impl Default for HslBand {
    fn default() -> Self {
        HslBand { hue: 0.0, saturation: 0.0, luminance: 0.0 }
    }
}

/// Parses the `hsl` op's `bands` object by the fixed `HSL_BAND_NAMES`
/// list, defaulting any missing/partial band to identity -- same
/// "fall back to identity on partial/corrupt payload" contract
/// `tone_curve_points` already establishes for its own op.
pub(super) fn hsl_bands(ops: &[serde_json::Value]) -> [HslBand; 8] {
    let bands_obj = ops
        .iter()
        .find(|op| op.get("op").and_then(|v| v.as_str()) == Some("hsl"))
        .and_then(|op| op.get("bands"));

    let mut result = [HslBand::default(); 8];
    for (i, name) in HSL_BAND_NAMES.iter().enumerate() {
        let Some(band) = bands_obj.and_then(|b| b.get(name)) else { continue };
        result[i] = HslBand {
            hue: band.get("hue").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
            saturation: band.get("saturation").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
            luminance: band.get("luminance").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
        };
    }
    result
}

/// Raised-cosine blend weight of one band (by index) at a pixel's hue,
/// blending between ADJACENT centers using each side's own gap (RFC-0019).
/// Between neighbors `a` and `b`, `w_a = 0.5(1+cos(pi*t))` and
/// `w_b = 0.5(1+cos(pi*(1-t))) = 1 - w_a`, so exactly two weights are
/// nonzero and they sum to exactly 1 for ANY gap size -- no gap, no
/// double-count, no renormalization, even though the centers are no
/// longer evenly spaced. C1-continuous (zero slope at every center and
/// crossover), chosen over a triangular ramp for the same reason as
/// before: a slope kink at a band center is more likely to read as a
/// visible seam on a continuous hue gradient (sky, skin). Within an
/// interval `w_a > 0.5` iff the hue is nearer `a`, so the highest-weight
/// band is always the nearest-center band (`nearestHslBand`, develop.js).
pub(super) fn hsl_band_weight(hue_deg: f32, band: usize) -> f32 {
    let n = HSL_BAND_CENTERS_DEG.len();
    let center = HSL_BAND_CENTERS_DEG[band];
    let d = ((hue_deg - center + 180.0).rem_euclid(360.0)) - 180.0;
    let (dist, gap) = if d >= 0.0 {
        (d, (HSL_BAND_CENTERS_DEG[(band + 1) % n] - center).rem_euclid(360.0))
    } else {
        (-d, (center - HSL_BAND_CENTERS_DEG[(band + n - 1) % n]).rem_euclid(360.0))
    };
    if dist >= gap {
        0.0
    } else {
        0.5 * ((dist / gap * std::f32::consts::PI).cos() + 1.0)
    }
}

/// Combines all 8 bands into ONE hue/saturation/luminance delta, applied
/// once -- correct (not just cheaper) because at most 2 band weights are
/// ever nonzero at once (see `hsl_band_weight`), so there's no
/// compounding-order question the mask-blend pattern elsewhere in this
/// file has to solve. The hue-wraparound pitfall is avoided by combining
/// SHIFT DELTAS (not resultant angles) and wrapping only once, on the
/// summed delta -- never averaging two angles that straddle 0/360
/// directly. `chroma_fade` is a load-bearing mitigation, not a defensive
/// nicety: hue is derived from ratios/differences of near-equal channel
/// values, which is precisely where 8-bit source quantization noise is
/// largest relative to signal (low-saturation regions -- skies, skin,
/// walls) -- without fading hue/luminance shift out as saturation
/// approaches 0, a pixel-to-pixel hue estimate can jitter enough to
/// surface as visible mottling no earlier op in this pipeline can
/// produce. Saturation-delta needs no separate gate: it's naturally
/// self-fading (0 * anything = 0).
pub(super) fn apply_hsl_bands(rgb: [f32; 3], bands: &[HslBand; 8]) -> [f32; 3] {
    let (h_px, s_px, l_px) = rgb_to_hsl(rgb);

    let mut hue_acc = 0.0f32;
    let mut sat_acc = 0.0f32;
    let mut lum_acc = 0.0f32;
    for (i, band) in bands.iter().enumerate() {
        let w = hsl_band_weight(h_px, i);
        hue_acc += w * band.hue;
        sat_acc += w * band.saturation;
        lum_acc += w * band.luminance;
    }

    let chroma_fade = (s_px / 0.08).clamp(0.0, 1.0);

    let new_h = (h_px + hue_acc * chroma_fade).rem_euclid(360.0);
    let new_s = (s_px * (1.0 + sat_acc / 100.0)).clamp(0.0, 1.0);

    let lum_frac = (lum_acc / 100.0) * chroma_fade;
    let new_l = if lum_frac >= 0.0 {
        l_px + (1.0 - l_px) * lum_frac
    } else {
        l_px + l_px * lum_frac
    };

    hsl_to_rgb(new_h, new_s, new_l)
}

/// Split Toning (M3): a global-only op, the pipeline's new final step --
/// exposure -> contrast -> saturation -> tone curve -> HSL -> split
/// toning -- before any mask reads the graded `rgb` as its own
/// accumulator base. Reuses `rgb_to_hsl`/`hsl_to_rgb` as-is -- no new
/// color-space math needed. Unlike `apply_hsl_bands`, this formula never
/// reads a pixel's own hue or saturation, only its lightness, so the
/// `chroma_fade` near-gray mitigation that formula needs does NOT apply
/// here (there's no ratio-of-near-equal-channels computation to be
/// numerically unstable) -- and porting it over anyway would actively
/// break this feature, since tinting near-neutral tones is split toning's
/// entire purpose (classic teal-orange grading relies on exactly that).
pub(super) struct SplitToning {
    pub(super) shadow_hue: f32,
    pub(super) shadow_saturation: f32,
    pub(super) highlight_hue: f32,
    pub(super) highlight_saturation: f32,
    pub(super) balance: f32,
}

impl Default for SplitToning {
    fn default() -> Self {
        SplitToning {
            shadow_hue: 0.0,
            shadow_saturation: 0.0,
            highlight_hue: 0.0,
            highlight_saturation: 0.0,
            balance: 0.0,
        }
    }
}

/// Parses the `split_toning` op's nested `shadows`/`highlights`/`balance`
/// fields, defaulting any missing piece to identity -- same "fall back to
/// identity on partial/corrupt payload" contract `tone_curve_points`/
/// `hsl_bands` already establish for their own ops.
pub(super) fn split_toning(ops: &[serde_json::Value]) -> SplitToning {
    let op = ops
        .iter()
        .find(|op| op.get("op").and_then(|v| v.as_str()) == Some("split_toning"));
    let zone = |key: &str| -> (f32, f32) {
        let z = op.and_then(|o| o.get(key));
        (
            z.and_then(|z| z.get("hue")).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
            z.and_then(|z| z.get("saturation")).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
        )
    };
    let (shadow_hue, shadow_saturation) = zone("shadows");
    let (highlight_hue, highlight_saturation) = zone("highlights");
    let balance = op.and_then(|o| o.get("balance")).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
    SplitToning {
        shadow_hue,
        shadow_saturation,
        highlight_hue,
        highlight_saturation,
        balance,
    }
}

/// Smoothstep(0,1,t) of the balance-shifted lightness -- C1-continuous
/// (zero slope at both ends) so the shadow/highlight transition has no
/// slope-kink seam on a smooth luminance gradient, same reasoning
/// `hsl_band_weight` used raised-cosine for. `weight_shadows` (the
/// complement, computed at the call site) partitions to exactly 1 with
/// this weight for every `l`/`balance` -- no separate normalization
/// needed, by construction.
pub(super) fn split_tone_highlight_weight(l: f32, balance: f32) -> f32 {
    let t = (l + balance / 200.0).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A single simultaneous 3-way convex combination, NOT two sequential
/// blends -- a real bug a design review caught before this was written:
/// sequentially blending shadow-then-highlight (or the reverse) is
/// order-dependent whenever both zone weights are nonzero, which given
/// the full-range smoothstep transition above is true for nearly every
/// pixel, not a rare edge case. This formula is order-independent by
/// construction (one sum) and provably a valid convex combination with no
/// clamp needed: `weight_shadows + weight_highlights == 1` always, and
/// both `saturation/100 <= 1`, so `a_sh + a_hi` (a weighted average of two
/// values each <=1) is itself always <=1, guaranteeing
/// `1 - a_sh - a_hi >= 0` -- three non-negative weights summing to
/// exactly 1.
pub(super) fn apply_split_toning(rgb: [f32; 3], st: &SplitToning) -> [f32; 3] {
    let (_, _, l_px) = rgb_to_hsl(rgb); // hue/saturation discarded -- only lightness needed
    let w_hi = split_tone_highlight_weight(l_px, st.balance);
    let w_sh = 1.0 - w_hi;
    let a_sh = w_sh * (st.shadow_saturation / 100.0);
    let a_hi = w_hi * (st.highlight_saturation / 100.0);
    let tint_sh = hsl_to_rgb(st.shadow_hue, 1.0, l_px);
    let tint_hi = hsl_to_rgb(st.highlight_hue, 1.0, l_px);
    let mut out = [0.0f32; 3];
    for c in 0..3 {
        out[c] = rgb[c] * (1.0 - a_sh - a_hi) + tint_sh[c] * a_sh + tint_hi[c] * a_hi;
    }
    out
}
