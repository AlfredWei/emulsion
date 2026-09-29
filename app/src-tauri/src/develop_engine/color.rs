/// Edit-stack ops have no meaningful array order (both this and the WGSL
/// shader always apply exposure -> contrast -> saturation regardless of
/// how they're stored) -- look each one up by name, defaulting to a no-op
/// value so an image never opened in Develop still renders as a clean
/// passthrough.
pub(super) fn op_value(ops: &[serde_json::Value], name: &str) -> f32 {
    ops.iter()
        .find(|op| op.get("op").and_then(|v| v.as_str()) == Some(name))
        .and_then(|op| op.get("value"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0) as f32
}

/// Standard HSL, not this module's own perceptual luma (used elsewhere for
/// global Saturation and luminance-range-mask selection) -- the HSL->RGB
/// reconstruction below is only mathematically valid for the L it was
/// derived from (`(max+min)/2`), so substituting perceptual luma in would
/// produce colorimetrically wrong output, not just a stylistic
/// difference. No consistency obligation exists between this and the
/// other, unrelated uses of luma elsewhere in this file.
/// Returns (hue_degrees 0..360, saturation 0..1, lightness 0..1).
pub(super) fn rgb_to_hsl(rgb: [f32; 3]) -> (f32, f32, f32) {
    let [r, g, b] = rgb;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let l = (max + min) / 2.0;
    if delta == 0.0 {
        return (0.0, 0.0, l);
    }
    let s = delta / (1.0 - (2.0 * l - 1.0).abs());
    let mut h = if max == r {
        60.0 * (((g - b) / delta).rem_euclid(6.0))
    } else if max == g {
        60.0 * (((b - r) / delta) + 2.0)
    } else {
        60.0 * (((r - g) / delta) + 4.0)
    };
    h = h.rem_euclid(360.0);
    (h, s, l)
}

pub(super) fn hsl_to_rgb(h: f32, s: f32, l: f32) -> [f32; 3] {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - (((h / 60.0).rem_euclid(2.0)) - 1.0).abs());
    let m = l - c / 2.0;
    let (r1, g1, b1) = if h < 60.0 {
        (c, x, 0.0)
    } else if h < 120.0 {
        (x, c, 0.0)
    } else if h < 180.0 {
        (0.0, c, x)
    } else if h < 240.0 {
        (0.0, x, c)
    } else if h < 300.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };
    [r1 + m, g1 + m, b1 + m]
}

/// GLSL/WGSL-standard smoothstep (`t*t*(3-2t)` of the clamped 0..1
/// position between the two edges) -- Rust's std has no builtin, and this
/// exact formula is what WGSL's own `smoothstep` computes, so the CPU and
/// GPU vignette falloff curves match, not just approximate each other.
pub(super) fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Standard Rec. 709 luma weights -- the same constant this module
/// already uses inline in half a dozen places (dehaze_atmospheric_light,
/// apply_local_contrast, etc.), pulled into one named function for
/// Sharpening/Noise Reduction below, which all need it repeatedly against
/// a precomputed whole-image buffer rather than one-off per call site.
pub(super) fn luma3(c: [f32; 3]) -> f32 {
    c[0] * 0.2126 + c[1] * 0.7152 + c[2] * 0.0722
}

pub(super) fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// RFC-0017: the exact sRGB EOTF (decode side) -- NOT a `pow(2.2)`
/// approximation. The piecewise linear toe below `0.04045` matters for
/// shadow behavior specifically because `apply_exposure`/
/// `apply_white_balance` (tone.rs) both round-trip through this pair to
/// do their actual gain in linear light, and a pure-power approximation
/// diverges most right where shadows live. Develop's own preview buffer
/// is still `decode()`'s auto-brightened, gamma-baked 8-bit output, not
/// the linear-light working space ADR-0004 calls for (see RFC-0017 SS2/
/// SS3.4) -- this function makes the MATH applied to that buffer
/// correct, it does not change where the buffer's own values come from.
pub(super) fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// RFC-0017: the exact sRGB OETF (encode side), the inverse of
/// `srgb_to_linear` above. `.max(0.0)` guards a negative input (possible
/// after a large negative Exposure or a White Balance gain below 1.0
/// applied to an already-near-zero linear value) -- `powf` on a negative
/// base with a non-integer exponent is NaN in Rust, same underlying
/// concern RFC-0016's WGSL `abs()` calls were load-bearing for.
pub(super) fn linear_to_srgb(c: f32) -> f32 {
    let c = c.max(0.0);
    if c <= 0.0031308 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}
