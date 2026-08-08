//! The develop engine (M3 Slice 4) — the single canonical, CPU-side
//! reference implementation of edit-stack interpretation, extracted from
//! `export.rs` where it previously lived despite that module's actual
//! concern being JPEG file export, not op interpretation. Two real call
//! sites already relied on it independently before this move
//! (`export::export_one`'s full-resolution render, `import.rs`'s
//! post-edit thumbnail regeneration) — that ad hoc sharing was the actual
//! smell this module fixes.
//!
//! **This is not the only implementation of this formula, and can't be**:
//! `DevelopCanvas.svelte`'s WGSL fragment shader applies the identical
//! exposure -> contrast -> saturation math for the *interactive* Develop
//! preview, and per ADR-0004 (validated by the M0 spike — streaming
//! Rust/wgpu-rendered frames into the webview over IPC measured ~300ms/
//! frame, 3x over the <100ms interactive budget) that path is
//! architecturally required to keep running in-webview via the browser's
//! real WebGPU API. There is no way to unify the two into one executable
//! implementation without introducing native `wgpu` for a headless/
//! offscreen export render, which was considered and deliberately
//! deferred to M5 (which already owns "GPU pipeline... with correct,
//! seamless CPU fallback" as dedicated scope) rather than done here as a
//! side effect of this basic-infra cleanup slice — see ADR-0004's dated
//! update for the full reasoning.
//!
//! **The parity obligation is real but manual**: this module and
//! `DevelopCanvas.svelte`'s `fs_main` must be kept in sync by hand
//! whenever the formula changes. The test table below is the actual
//! safety net for that — broader than a single data point specifically
//! so a future change to one side has a real reference to check against,
//! not just "doesn't crash."

use crate::catalog::EditStack;
use image::RgbImage;

/// Edit-stack ops have no meaningful array order (both this and the WGSL
/// shader always apply exposure -> contrast -> saturation regardless of
/// how they're stored) -- look each one up by name, defaulting to a no-op
/// value so an image never opened in Develop still renders as a clean
/// passthrough.
fn op_value(ops: &[serde_json::Value], name: &str) -> f32 {
    ops.iter()
        .find(|op| op.get("op").and_then(|v| v.as_str()) == Some(name))
        .and_then(|op| op.get("value"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0) as f32
}

/// One global exposure/contrast/saturation application -- factored out so
/// both the global pass and each mask's local pass (below) share the exact
/// same formula, mirroring how the WGSL shader factors its own
/// `apply_adjustments` helper for the same reason (M3 Slice 5).
fn apply_adjustments(rgb: [f32; 3], exposure_ev: f32, contrast: f32, saturation: f32) -> [f32; 3] {
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

/// Tone Curve (M3): a global-only op applied as the pipeline's new final
/// step -- exposure -> contrast -> saturation -> tone curve -- before any
/// mask reads the graded `rgb` as its own accumulator base (see
/// `apply_edit_stack`'s call site below and `Mask::weight`'s own doc
/// comment on that ordering). Number of samples in the shared LUT both
/// this module and DevelopCanvas.svelte's WGSL shader build and consume --
/// see `build_curve_lut`'s doc comment for why both sides build the exact
/// same discretized LUT rather than each evaluating the spline exactly.
const CURVE_LUT_SAMPLES: usize = 256;

/// Parses the `tone_curve` op's `points` array, defaulting to the 2-point
/// identity line `(0,0)-(1,1)` -- the same "always a clean passthrough for
/// an untouched image" contract `op_value` already establishes for the
/// scalar ops. Falls back to identity (not a panic) if fewer than 2 points
/// parse, e.g. a corrupt/partial payload.
fn tone_curve_points(ops: &[serde_json::Value]) -> Vec<(f32, f32)> {
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
fn compute_tangents(pts: &[(f32, f32)]) -> Vec<f32> {
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
fn hermite_at(pts: &[(f32, f32)], m: &[f32], x: f32) -> f32 {
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
fn build_curve_lut(points: &[(f32, f32)]) -> [f32; CURVE_LUT_SAMPLES] {
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
fn sample_lut(lut: &[f32], v: f32) -> f32 {
    let idx = v.clamp(0.0, 1.0) * (lut.len() - 1) as f32;
    let i0 = idx.floor() as usize;
    let i1 = (i0 + 1).min(lut.len() - 1);
    let frac = idx - i0 as f32;
    lut[i0] * (1.0 - frac) + lut[i1] * frac
}

/// HSL / Color Mixer (M3): a global-only op, applied as the pipeline's new
/// final step -- exposure -> contrast -> saturation -> tone curve -> HSL
/// -- before any mask reads the graded `rgb` as its own accumulator base
/// (see `apply_edit_stack`'s call site below). Band order/centers must
/// stay in lockstep by hand with `develop.js`'s `HSL_BAND_NAMES`/
/// `HSL_BAND_CENTERS_DEG` and DevelopCanvas.svelte's WGSL twin -- this
/// module's own doc comment already establishes that parity obligation
/// generally, for every formula here.
const HSL_BAND_NAMES: [&str; 8] = ["red", "orange", "yellow", "green", "aqua", "blue", "purple", "magenta"];
const HSL_BAND_CENTERS_DEG: [f32; 8] = [0.0, 45.0, 90.0, 135.0, 180.0, 225.0, 270.0, 315.0];

#[derive(Clone, Copy)]
struct HslBand {
    hue: f32,
    saturation: f32,
    luminance: f32,
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
fn hsl_bands(ops: &[serde_json::Value]) -> [HslBand; 8] {
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

/// Standard HSL, not this module's own perceptual luma (used elsewhere for
/// global Saturation and luminance-range-mask selection) -- the HSL->RGB
/// reconstruction below is only mathematically valid for the L it was
/// derived from (`(max+min)/2`), so substituting perceptual luma in would
/// produce colorimetrically wrong output, not just a stylistic
/// difference. No consistency obligation exists between this and the
/// other, unrelated uses of luma elsewhere in this file.
/// Returns (hue_degrees 0..360, saturation 0..1, lightness 0..1).
fn rgb_to_hsl(rgb: [f32; 3]) -> (f32, f32, f32) {
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

fn hsl_to_rgb(h: f32, s: f32, l: f32) -> [f32; 3] {
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

/// Raised-cosine blend weight between a pixel's hue and one band center --
/// C1-continuous (zero slope at the center and at each 50/50 crossover),
/// chosen over a triangular ramp specifically because a triangular ramp
/// has a slope kink exactly at every band center, which on a continuous
/// hue gradient (sky, skin) is more likely to read as a visible seam.
/// Band centers are exactly 45 degrees apart, so at most 2 bands are ever
/// nonzero at any hue, and they sum to exactly 1 by the cosine identity
/// cos(x) + cos(pi-x) = 0 -- no gap, no double-count, no renormalization
/// needed anywhere this is used.
fn hsl_band_weight(hue_deg: f32, center_deg: f32) -> f32 {
    let d = ((hue_deg - center_deg + 180.0).rem_euclid(360.0)) - 180.0;
    let dist = d.abs();
    if dist >= 45.0 {
        0.0
    } else {
        0.5 * ((dist / 45.0 * std::f32::consts::PI).cos() + 1.0)
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
fn apply_hsl_bands(rgb: [f32; 3], bands: &[HslBand; 8]) -> [f32; 3] {
    let (h_px, s_px, l_px) = rgb_to_hsl(rgb);

    let mut hue_acc = 0.0f32;
    let mut sat_acc = 0.0f32;
    let mut lum_acc = 0.0f32;
    for (i, band) in bands.iter().enumerate() {
        let w = hsl_band_weight(h_px, HSL_BAND_CENTERS_DEG[i]);
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
struct SplitToning {
    shadow_hue: f32,
    shadow_saturation: f32,
    highlight_hue: f32,
    highlight_saturation: f32,
    balance: f32,
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
fn split_toning(ops: &[serde_json::Value]) -> SplitToning {
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

/// Vignette (M3): a flat, non-nested payload (`{amount, midpoint, feather}`)
/// -- unlike Split Toning's per-zone shape above, there's no natural
/// per-element repetition here, so a plain struct with three named fields
/// is simplest. Same "fall back to identity on partial/corrupt payload"
/// contract every other structured op already establishes.
struct Vignette {
    amount: f32,
    midpoint: f32,
    feather: f32,
}

impl Default for Vignette {
    fn default() -> Self {
        Vignette { amount: 0.0, midpoint: 50.0, feather: 50.0 }
    }
}

fn vignette_op(ops: &[serde_json::Value]) -> Vignette {
    let op = ops.iter().find(|op| op.get("op").and_then(|v| v.as_str()) == Some("vignette"));
    let field = |key: &str, default: f32| -> f32 {
        op.and_then(|o| o.get(key)).and_then(|v| v.as_f64()).map(|v| v as f32).unwrap_or(default)
    };
    Vignette {
        amount: field("amount", 0.0),
        midpoint: field("midpoint", 50.0),
        feather: field("feather", 50.0),
    }
}

/// GLSL/WGSL-standard smoothstep (`t*t*(3-2t)` of the clamped 0..1
/// position between the two edges) -- Rust's std has no builtin, and this
/// exact formula is what WGSL's own `smoothstep` computes, so the CPU and
/// GPU vignette falloff curves match, not just approximate each other.
fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Post-crop vignette (M3): a pure per-pixel radial brightness falloff --
/// unlike Dehaze/Texture/Clarity above, this needs no neighboring-pixel
/// data at all, so it folds directly into the existing final per-pixel
/// loop rather than adding a new buffer pass. Applied last among the
/// GLOBAL ops (after Dehaze, before any mask reads `rgb` as its own
/// accumulator base) -- matches real Lightroom's own Effects-panel
/// ordering and this pipeline's established "spatial ops go last, masks
/// paint on top of everything" convention.
///
/// Shape: an ASPECT-CORRECTED ELLIPSE matching the image's own aspect
/// ratio (so the vignette looks like a natural circular falloff, not a
/// shape squashed to the image bounds) -- named, deferred simplification:
/// real Lightroom's Roundness slider (blending toward a more rectangular
/// shape) isn't implemented; every vignette here is roundness=0's natural
/// ellipse. `midpoint` (0-100) sets the normalized radius (as a fraction
/// of the center-to-corner distance) where the falloff begins; `feather`
/// (0-100) widens the transition zone from there out to the corner;
/// `amount` (-100..100) scales the resulting multiplicative brightness
/// factor (negative darkens, positive lightens, matching Lightroom's own
/// sign convention). `amount=0` is an exact passthrough (`vignette_factor`
/// returns exactly 1.0, skipping the geometry entirely) -- same discipline
/// every other op's identity value already gets.
fn vignette_factor(uv: (f32, f32), aspect: f32, v: &Vignette) -> f32 {
    if v.amount == 0.0 {
        return 1.0;
    }
    let dx = (uv.0 - 0.5) * 2.0;
    let dy = (uv.1 - 0.5) * 2.0 * aspect;
    // Center-to-corner distance in this same aspect-corrected space --
    // normalizing by it means `midpoint`/`feather` are always relative to
    // "how far out toward the corner", regardless of the image's own
    // aspect ratio.
    let corner_dist = (1.0f32 + aspect * aspect).sqrt();
    let norm_dist = (dx * dx + dy * dy).sqrt() / corner_dist;
    // `inner`/`outer` are nudged apart by a small epsilon and clamped away
    // from touching -- smoothstep's own definition is only well-behaved
    // for edge0 < edge1, and midpoint=100 or feather=0 would otherwise
    // collapse them to equal.
    let inner = (v.midpoint / 100.0).clamp(0.0, 0.999);
    let outer = (inner + (v.feather / 100.0).max(0.001) * (1.0 - inner)).clamp(inner + 0.001, 1.0);
    let t = smoothstep(inner, outer, norm_dist);
    1.0 + (v.amount / 100.0) * t
}

/// Grain (M3): a flat, non-nested payload, same reasoning as Vignette's
/// own struct. Defaults match real Lightroom's own Grain defaults exactly
/// (Amount 0 -- off, Size 25, Roughness 50) -- Amount=0 is this op's
/// identity, same "off by default, sensible if just turned on" contract
/// every other amount-style op already has.
struct Grain {
    amount: f32,
    size: f32,
    roughness: f32,
}

impl Default for Grain {
    fn default() -> Self {
        Grain { amount: 0.0, size: 25.0, roughness: 50.0 }
    }
}

fn grain_op(ops: &[serde_json::Value]) -> Grain {
    let op = ops.iter().find(|op| op.get("op").and_then(|v| v.as_str()) == Some("grain"));
    let field = |key: &str, default: f32| -> f32 {
        op.and_then(|o| o.get(key)).and_then(|v| v.as_f64()).map(|v| v as f32).unwrap_or(default)
    };
    Grain {
        amount: field("amount", 0.0),
        size: field("size", 25.0),
        roughness: field("roughness", 50.0),
    }
}

/// A classic GLSL/WGSL-portable pseudo-random hash: deterministic given
/// the same input coordinate (unlike a seeded RNG, needs no state), which
/// is exactly what a STATIC grain pattern needs -- re-evaluating this at
/// the same pixel on every render must return the same value, or the
/// grain would visibly "boil"/shimmer on every unrelated slider tweak
/// instead of looking like a fixed film-grain texture. Not bit-exact
/// across Rust's `f32::sin` and WGSL's own `sin` (different underlying
/// implementations), but both are IEEE-754 single precision evaluating
/// the exact same formula -- close enough for this module's own
/// established "±2/255, not byte-identical" parity bar.
fn grain_hash(x: f32, y: f32) -> f32 {
    let v = (x * 12.9898 + y * 78.233).sin() * 43758.5453123;
    v - v.floor()
}

/// Bilinear-interpolated value noise over the hash lattice above -- the
/// "smooth" end of Roughness (organic, softly-varying grain rather than
/// visibly blocky cells). `coord` is already scaled by grain size (see
/// `grain_delta`'s own doc comment).
fn grain_value_noise(coord: (f32, f32)) -> f32 {
    let ix = coord.0.floor();
    let iy = coord.1.floor();
    let fx = coord.0 - ix;
    let fy = coord.1 - iy;
    let a = grain_hash(ix, iy);
    let b = grain_hash(ix + 1.0, iy);
    let c = grain_hash(ix, iy + 1.0);
    let d = grain_hash(ix + 1.0, iy + 1.0);
    let ux = fx * fx * (3.0 - 2.0 * fx);
    let uy = fy * fy * (3.0 - 2.0 * fy);
    let ab = a + (b - a) * ux;
    let cd = c + (d - c) * ux;
    ab + (cd - ab) * uy
}

/// Film grain (M3): a pure per-pixel procedural noise overlay -- like
/// Vignette, no neighboring-pixel data needed, so it folds directly into
/// the existing final per-pixel loop. Applied globally right after
/// Vignette, before any mask (matching real Lightroom's own Effects-panel
/// order: Post-Crop Vignette, then Grain, both above Local Adjustments).
///
/// `size` maps to the lattice cell width in PIXELS (fixed range 1..
/// GRAIN_MAX_CELL_PX, same "fixed absolute pixel scale, not resolution-
/// scaled" named limitation Dehaze/Texture/Clarity's own radii already
/// accept) -- larger cells read as coarser, chunkier grain particles.
/// `roughness` blends between the bilinear-interpolated value noise above
/// (smooth, roughness=0) and the RAW lattice-cell hash with no
/// interpolation at all (blocky/uncorrelated between adjacent cells,
/// roughness=100) -- two ends of the same underlying hash lattice, not
/// two unrelated noise functions. `amount` scales the resulting additive
/// luminance delta (added equally to all three channels, same "preserve
/// chroma via an additive delta" shape Texture/Clarity's own formula
/// uses) -- real film grain is predominantly a luminance/density effect,
/// not per-channel chromatic noise. `GRAIN_STRENGTH` is a fixed constant
/// (not user-exposed) capping the visual amplitude at amount=100, the
/// same "fix the knob" choice this module's other constants already
/// make.
const GRAIN_MAX_CELL_PX: f32 = 6.0;
const GRAIN_STRENGTH: f32 = 0.12;

fn grain_delta(coord: (f32, f32), g: &Grain) -> f32 {
    if g.amount == 0.0 {
        return 0.0;
    }
    let cell = 1.0 + (g.size / 100.0) * (GRAIN_MAX_CELL_PX - 1.0);
    let scaled = (coord.0 / cell, coord.1 / cell);
    let smooth = grain_value_noise(scaled);
    let rough = grain_hash(scaled.0.floor(), scaled.1.floor());
    let noise = smooth + (rough - smooth) * (g.roughness / 100.0).clamp(0.0, 1.0);
    (noise * 2.0 - 1.0) * (g.amount / 100.0) * GRAIN_STRENGTH
}

/// Smoothstep(0,1,t) of the balance-shifted lightness -- C1-continuous
/// (zero slope at both ends) so the shadow/highlight transition has no
/// slope-kink seam on a smooth luminance gradient, same reasoning
/// `hsl_band_weight` used raised-cosine for. `weight_shadows` (the
/// complement, computed at the call site) partitions to exactly 1 with
/// this weight for every `l`/`balance` -- no separate normalization
/// needed, by construction.
fn split_tone_highlight_weight(l: f32, balance: f32) -> f32 {
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
fn apply_split_toning(rgb: [f32; 3], st: &SplitToning) -> [f32; 3] {
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

/// Texture & Clarity (M3): local-contrast (unsharp-mask-on-luminance)
/// sliders, real Lightroom's own "Presence" pair -- **texture -> clarity**,
/// inserted right before Dehaze: exposure -> contrast -> saturation -> tone
/// curve -> HSL -> split toning -> **texture -> clarity** -> dehaze. Same
/// family as Dehaze (needs neighboring pixels' graded values, not a pure
/// per-pixel remap), so both operate on `apply_edit_stack`'s own `graded`
/// buffer, sequentially, before Dehaze's maps are computed from it -- by the
/// time Dehaze reads `graded`, Texture and Clarity are already baked in,
/// matching Lightroom's own Texture -> Clarity -> Dehaze slider order.
///
/// Core transform (identical shape for both, differing only in radius):
/// blur the per-pixel luminance at `radius` via the existing
/// `separable_mean_filter`, then push each pixel's luminance toward
/// `luma + (luma - blurred) * (amount / 100)` (positive amount sharpens
/// local contrast, negative smooths toward the blurred version) by adding
/// that SAME delta to all three channels -- not by rescaling the RGB
/// triple by a luma ratio. A design review caught a real bug in the
/// ratio-based version: `graded` is never clamped between ops (Contrast
/// alone already pushes near-black channels negative, see
/// `apply_adjustments`), so a ratio `newLuma / max(luma, epsilon)` blows up
/// and scrambles hue whenever luma is near zero or negative. The additive
/// form has no division at all, and it's exact: `newC - newLuma == C -
/// luma` for every channel, so chroma (each channel's distance from luma)
/// is preserved exactly regardless of how extreme `graded`'s values are --
/// the same additive-around-luma shape `apply_adjustments`'s own saturation
/// step already uses, not a new idiom.
///
/// Radii are fixed, not user-exposed (same "fix the knob" choice Dehaze's
/// own constants made) -- absolute pixel counts, NOT scaled to image
/// resolution, the same named/deferred limitation Dehaze's own radii
/// accepted: `TEXTURE_RADIUS=6` (fine detail, small window so it doesn't
/// touch big tonal transitions), `CLARITY_RADIUS=24` (coarse "midtone
/// contrast", the classic halo-prone Clarity look at high amounts -- a
/// plain box-mean blur stands in for a true edge-aware filter here, same
/// named limitation as Dehaze's own transmission refinement).
const TEXTURE_RADIUS: i32 = 6;
const CLARITY_RADIUS: i32 = 24;

/// Applies one local-contrast pass (Texture or Clarity, selected by
/// `radius`) to `graded` in place -- see the doc comment above for the
/// additive-delta formula and why it was chosen over a luma-ratio rescale.
/// `amount` is expected in -100..100; callers skip this entirely at
/// `amount == 0.0` (exact passthrough, no wasted blur pass), same
/// discipline `apply_edit_stack` already applies to Dehaze.
fn apply_local_contrast(graded: &mut [[f32; 3]], width: usize, height: usize, radius: i32, amount: f32) {
    let luma: Vec<f32> = graded
        .iter()
        .map(|c| c[0] * 0.2126 + c[1] * 0.7152 + c[2] * 0.0722)
        .collect();
    let blurred = separable_mean_filter(&luma, width, height, radius);
    let factor = amount / 100.0;
    for (i, rgb) in graded.iter_mut().enumerate() {
        let delta = (luma[i] - blurred[i]) * factor;
        for c in rgb.iter_mut() {
            *c += delta;
        }
    }
}

/// Dehaze (M3): dark-channel-prior haze removal (He et al. 2009), the
/// pipeline's new final GLOBAL step -- exposure -> contrast -> saturation ->
/// tone curve -> HSL -> split toning -> texture -> clarity -> **dehaze** --
/// before any mask reads the graded `rgb` as its own accumulator base.
/// Unlike every op above it,
/// dark-channel-prior fundamentally needs NEIGHBORING pixels' already-graded
/// values (a local minimum filter for the dark channel, a whole-image
/// reduction for atmospheric light) -- it cannot be folded into the
/// existing single per-pixel loop the way every earlier op was; see
/// `apply_edit_stack`'s own restructuring below, now a genuine multi-pass
/// pipeline over intermediate buffers, not one loop.
///
/// Algorithm constants are fixed, not user-exposed (same "fix the knob,
/// expose only Amount" choice Split Toning made for its own transition
/// width): `DEHAZE_PATCH_RADIUS=7` (15x15 dark-channel window, the textbook
/// value), `DEHAZE_OMEGA=0.95` (keep 5% residual haze for a natural look,
/// textbook value), `DEHAZE_T0=0.1` (transmission floor, prevents noise
/// blowup where transmission is near zero, textbook value),
/// `DEHAZE_REFINE_RADIUS=4` (9x9 transmission-refinement window).
///
/// **Two deliberate, named deviations from He et al.'s own algorithm** -- a
/// design review caught real bugs in earlier drafts of both before this was
/// written; these are the corrected versions:
/// - Atmospheric light is estimated via ARGMAX-BY-LUMINANCE (the whole RGB
///   triple of whichever pixel has the highest luminance in the graded
///   image), NOT independent per-channel maxima -- component-wise max can
///   synthesize a color no pixel in the image actually has, and is
///   unboundedly sensitive to a single outlier channel (e.g. a sensor speck
///   bright only in R). This is also not gated by "top 0.1% of the dark
///   channel" the way He et al.'s own refinement is (closer to Tan 2008's
///   simpler pre-DCP baseline) -- an accepted, named scope cut, not
///   silently dropped; a bright non-atmospheric object (white car, snow)
///   could still throw off the estimate. Deferred to a later slice.
/// - The per-channel `I^c/A^c` normalization happens BEFORE the windowed
///   min (`dehaze_atmospheric_light` runs first, then `min_channel` divides
///   per-channel, matching He et al. exactly) -- an earlier draft collapsed
///   the cross-channel min FIRST and divided the resulting scalar by a
///   single scalar representative of A afterward, which is not a
///   numerically-close approximation but a structurally different (and
///   generally wrong) result once the cross-channel min has already picked
///   a channel.
/// - The transmission-refinement box-mean filter (`separable_mean_filter`)
///   stands in for He et al.'s edge-preserving guided filter -- named
///   limitation: mild haloing near strong contrast edges a real guided
///   filter would avoid. Deferred to a later slice (cheap to add given the
///   box-filter primitive already built here).
const DEHAZE_PATCH_RADIUS: i32 = 7;
const DEHAZE_OMEGA: f32 = 0.95;
const DEHAZE_T0: f32 = 0.1;
const DEHAZE_REFINE_RADIUS: i32 = 4;

/// Atmospheric light: the whole RGB triple of the graded image's own
/// highest-LUMINANCE pixel -- a single full-image scan. No bounded-pass
/// reduction chain needed here the way the GPU side needs one (see this
/// group's own doc comment above); the CPU path has no such constraint.
fn dehaze_atmospheric_light(graded: &[[f32; 3]]) -> [f32; 3] {
    let mut best_luma = -1.0f32;
    let mut best = [1.0f32, 1.0, 1.0];
    for &rgb in graded {
        let luma = rgb[0] * 0.2126 + rgb[1] * 0.7152 + rgb[2] * 0.0722;
        if luma > best_luma {
            best_luma = luma;
            best = rgb;
        }
    }
    best
}

/// Separable box-MIN filter (the dark-channel step): a horizontal pass then
/// a vertical pass over a single-channel buffer -- correct (not just
/// cheaper) because a rectangular window's min is associative/commutative
/// over its two axes independently (min over a 2D box == min over rows of
/// (min over each row's own 1D box)). Edge taps clamp to the buffer's own
/// bounds, same reasoning the GPU shader's own manual `textureLoad` clamp
/// needs (an out-of-range read must never pull in a phantom value).
fn separable_min_filter(buf: &[f32], width: usize, height: usize, radius: i32) -> Vec<f32> {
    let mut h_pass = vec![0.0f32; buf.len()];
    for y in 0..height {
        for x in 0..width {
            let mut m = f32::INFINITY;
            for dx in -radius..=radius {
                let sx = (x as i32 + dx).clamp(0, width as i32 - 1) as usize;
                m = m.min(buf[y * width + sx]);
            }
            h_pass[y * width + x] = m;
        }
    }
    let mut v_pass = vec![0.0f32; buf.len()];
    for y in 0..height {
        for x in 0..width {
            let mut m = f32::INFINITY;
            for dy in -radius..=radius {
                let sy = (y as i32 + dy).clamp(0, height as i32 - 1) as usize;
                m = m.min(h_pass[sy * width + x]);
            }
            v_pass[y * width + x] = m;
        }
    }
    v_pass
}

/// Separable box-MEAN filter (the transmission-refinement step): unlike the
/// min filter above, a sum can be tracked with an exact O(1)-per-pixel
/// sliding-window accumulator (add the entering tap, subtract the leaving
/// one) -- an operation min/max has no equivalent for, since neither can be
/// "un-added". Correct even where the window clamps at an edge (multiple
/// taps mapping to the same clamped index): each step still adds/removes
/// exactly one conceptual tap, so the running sum stays exact regardless of
/// how many taps on either side happen to share a clamped index.
fn separable_mean_filter(buf: &[f32], width: usize, height: usize, radius: i32) -> Vec<f32> {
    let window = (2 * radius + 1) as f32;
    let mut h_pass = vec![0.0f32; buf.len()];
    for y in 0..height {
        let row = &buf[y * width..(y + 1) * width];
        let mut sum = 0.0f32;
        for dx in -radius..=radius {
            let sx = dx.clamp(0, width as i32 - 1) as usize;
            sum += row[sx];
        }
        h_pass[y * width] = sum / window;
        for x in 1..width {
            let leave = (x as i32 - 1 - radius).clamp(0, width as i32 - 1) as usize;
            let enter = (x as i32 + radius).clamp(0, width as i32 - 1) as usize;
            sum += row[enter] - row[leave];
            h_pass[y * width + x] = sum / window;
        }
    }
    let mut v_pass = vec![0.0f32; buf.len()];
    for x in 0..width {
        let mut sum = 0.0f32;
        for dy in -radius..=radius {
            let sy = dy.clamp(0, height as i32 - 1) as usize;
            sum += h_pass[sy * width + x];
        }
        v_pass[x] = sum / window;
        for y in 1..height {
            let leave = (y as i32 - 1 - radius).clamp(0, height as i32 - 1) as usize;
            let enter = (y as i32 + radius).clamp(0, height as i32 - 1) as usize;
            sum += h_pass[enter * width + x] - h_pass[leave * width + x];
            v_pass[y * width + x] = sum / window;
        }
    }
    v_pass
}

/// A `linear_gradient_mask` op (M3 Slice 5), parsed from its opaque JSON
/// shape -- see MaskToolStrip.svelte/develop.js for how the frontend
/// creates these. Coordinates are normalized (0..1, matching the WGSL
/// shader's own `in.uv`), so they stay valid across preview resolutions.
struct LinearGradientMask {
    start: (f32, f32),
    end: (f32, f32),
    feather: f32,
    invert: bool,
    exposure: f32,
    contrast: f32,
    saturation: f32,
}

fn parse_linear_gradient_mask(op: &serde_json::Value) -> Option<LinearGradientMask> {
    let start = op.get("start")?;
    let end = op.get("end")?;
    Some(LinearGradientMask {
        start: (
            start.get("x")?.as_f64()? as f32,
            start.get("y")?.as_f64()? as f32,
        ),
        end: (
            end.get("x")?.as_f64()? as f32,
            end.get("y")?.as_f64()? as f32,
        ),
        feather: op.get("feather").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
        invert: op.get("invert").and_then(|v| v.as_bool()).unwrap_or(false),
        exposure: op.get("exposure").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
        contrast: op.get("contrast").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
        saturation: op
            .get("saturation")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0) as f32,
    })
}

/// Same projection-onto-segment parametrization as the WGSL shader: `t` is
/// 0 at `start`, 1 at `end`, extrapolated linearly beyond both (then
/// clamped). `feather` widens the transition band symmetrically around the
/// midpoint -- at `feather=50`, the pins themselves move to weight 0.25/
/// 0.75 rather than staying at 0/1 -- a deliberate choice matching real
/// Lightroom's own gradient-feather model (its feather handles are
/// separate outer lines beyond the pins), not a corner-only softening.
fn mask_weight(uv: (f32, f32), mask: &LinearGradientMask) -> f32 {
    let dx = mask.end.0 - mask.start.0;
    let dy = mask.end.1 - mask.start.1;
    let len2 = (dx * dx + dy * dy).max(0.000_001);
    let t = ((uv.0 - mask.start.0) * dx + (uv.1 - mask.start.1) * dy) / len2;
    let softness = (mask.feather / 100.0).clamp(0.0, 0.999);
    let mut weight = (t + softness) / (1.0 + 2.0 * softness);
    weight = weight.clamp(0.0, 1.0);
    if mask.invert {
        weight = 1.0 - weight;
    }
    weight
}

/// A `radial_gradient_mask` op (M3 Slice 6). `radius_x`/`radius_y` are
/// independent normalized fractions (of image width/height respectively)
/// so an on-screen-circular placement (equal *native pixel* radius on both
/// axes, computed by the frontend) round-trips correctly regardless of the
/// image's own aspect ratio.
struct RadialGradientMask {
    center: (f32, f32),
    radius_x: f32,
    radius_y: f32,
    feather: f32,
    invert: bool,
    exposure: f32,
    contrast: f32,
    saturation: f32,
}

fn parse_radial_gradient_mask(op: &serde_json::Value) -> Option<RadialGradientMask> {
    let center = op.get("center")?;
    Some(RadialGradientMask {
        center: (
            center.get("x")?.as_f64()? as f32,
            center.get("y")?.as_f64()? as f32,
        ),
        radius_x: op.get("radiusX")?.as_f64()? as f32,
        radius_y: op.get("radiusY")?.as_f64()? as f32,
        feather: op.get("feather").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
        invert: op.get("invert").and_then(|v| v.as_bool()).unwrap_or(false),
        exposure: op.get("exposure").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
        contrast: op.get("contrast").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
        saturation: op
            .get("saturation")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0) as f32,
    })
}

/// Same ellipse-distance parametrization as the WGSL shader: `d` is 0 at
/// the center, 1 at the ellipse boundary, growing beyond it outside.
/// `insideWeight` is 1 at/near the center regardless of feather; at
/// feather=0 the transition band is `d` in `[0.999, 1.0]` (width 0.001,
/// sitting just inside the boundary, not symmetric around it), widening to
/// roughly `[0.001, 1.999]` as feather approaches 100. Default
/// (`invert=false`) applies the effect OUTSIDE the ellipse -- real
/// Lightroom's own Radial Filter convention (its classic vignette use
/// case); `invert=true` applies it inside (spotlight/subject use case).
fn radial_mask_weight(uv: (f32, f32), mask: &RadialGradientMask) -> f32 {
    let dx = (uv.0 - mask.center.0) / mask.radius_x;
    let dy = (uv.1 - mask.center.1) / mask.radius_y;
    let d = (dx * dx + dy * dy).sqrt();
    let softness = (mask.feather / 100.0).clamp(0.0, 0.999);
    let denom = (2.0 * softness).max(0.001);
    let inside_weight = ((1.0 + softness - d) / denom).clamp(0.0, 1.0);
    if mask.invert {
        inside_weight
    } else {
        1.0 - inside_weight
    }
}

/// A single paint dab within a `brush_mask` op (M3 Slice 7). `radius` is a
/// normalized fraction of image WIDTH only (a single scalar, unlike
/// radial's independent `radius_x`/`radius_y`) -- the frontend rasterizes
/// dabs directly in an offscreen canvas sized to the image's own native
/// pixel resolution, where `radius * nativeWidth` used for both dimensions
/// of `ctx.arc()` is inherently a true circle with no separate axis scaling
/// needed. This CPU path has no offscreen canvas, so it reconstructs the
/// same true-circle-in-pixel-space behavior via `aspect` (height/width) --
/// see `dab_falloff`. `hardness`/`flow` are baked in per-dab at paint time
/// from whatever the brush tool's settings were when that dab was placed
/// (real Lightroom's own brush-options model), not globally editable after
/// the fact.
#[derive(Clone, Copy)]
enum DabMode {
    Add,
    Erase,
}

struct Dab {
    x: f32,
    y: f32,
    radius: f32,
    hardness: f32,
    flow: f32,
    mode: DabMode,
}

struct BrushMask {
    dabs: Vec<Dab>,
    invert: bool,
    exposure: f32,
    contrast: f32,
    saturation: f32,
}

fn parse_brush_mask(op: &serde_json::Value) -> Option<BrushMask> {
    let dabs = op
        .get("dabs")?
        .as_array()?
        .iter()
        .filter_map(|d| {
            Some(Dab {
                x: d.get("x")?.as_f64()? as f32,
                y: d.get("y")?.as_f64()? as f32,
                radius: d.get("radius")?.as_f64()? as f32,
                hardness: d.get("hardness").and_then(|v| v.as_f64()).unwrap_or(50.0) as f32,
                flow: d.get("flow").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32,
                mode: if d.get("mode").and_then(|v| v.as_str()) == Some("erase") {
                    DabMode::Erase
                } else {
                    DabMode::Add
                },
            })
        })
        .collect();
    Some(BrushMask {
        dabs,
        invert: op.get("invert").and_then(|v| v.as_bool()).unwrap_or(false),
        exposure: op.get("exposure").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
        contrast: op.get("contrast").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
        saturation: op
            .get("saturation")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0) as f32,
    })
}

/// One dab's own contribution at `uv`: 0 outside its radius, 1 within the
/// `hardness`-controlled inner stop, linearly fading to 0 at the radius
/// (matching the frontend's white-center-to-black-edge radial-gradient
/// rasterization), scaled by `flow`. `aspect` (image height/width)
/// converts the dab's width-only `radius` into a true circle in pixel
/// space: a y-distance in normalized uv space covers more actual pixels
/// than an equal x-distance whenever the image isn't square, so it must be
/// scaled up by `aspect` before comparing against a radius that's only
/// ever expressed as a fraction of width.
fn dab_falloff(uv: (f32, f32), dab: &Dab, aspect: f32) -> f32 {
    if dab.radius <= 0.0 {
        return 0.0;
    }
    let dx = uv.0 - dab.x;
    let dy = (uv.1 - dab.y) * aspect;
    let d = (dx * dx + dy * dy).sqrt();
    let normalized_d = d / dab.radius;
    if normalized_d >= 1.0 {
        return 0.0;
    }
    let hard_stop = (dab.hardness / 100.0).clamp(0.0, 1.0);
    let base = if normalized_d <= hard_stop {
        1.0
    } else {
        let denom = (1.0 - hard_stop).max(0.0001);
        (1.0 - (normalized_d - hard_stop) / denom).clamp(0.0, 1.0)
    };
    base * dab.flow.clamp(0.0, 1.0)
}

/// Dabs are accumulated in stack (paint) order, not just unioned as a set --
/// `add` dabs take the max with the running weight (matches the frontend's
/// `"lighter"` canvas compositing: overlapping add dabs build up coverage
/// but don't exceed what a single fully-opaque dab would give), `erase`
/// dabs multiplicatively reduce the running weight toward 0 (matches the
/// frontend's `"multiply"` compositing for erase) -- the same formula both
/// renderers agree on, not an approximation of one by the other.
fn brush_mask_weight(uv: (f32, f32), mask: &BrushMask, aspect: f32) -> f32 {
    let mut weight = 0.0f32;
    for dab in &mask.dabs {
        let falloff = dab_falloff(uv, dab, aspect);
        match dab.mode {
            DabMode::Add => weight = weight.max(falloff),
            DabMode::Erase => weight *= 1.0 - falloff,
        }
    }
    if mask.invert {
        1.0 - weight
    } else {
        weight
    }
}

/// A `luminance_range_mask` op -- the first mask kind whose weight depends
/// on pixel VALUE (luminance) rather than pixel POSITION (every earlier
/// kind computed weight from `uv`/`aspect` alone). `range_min`/`range_max`/
/// `feather` are stored 0-100, matching linear/radial's own `feather`
/// scale convention rather than a separate 0-1 scale -- `feather` here
/// means something different from theirs (a band WIDTH around each of two
/// edges, not a single boundary), so it's edited via a dedicated Min/Max/
/// Feather block in MaskEditorPanel.svelte, not the shared Feather row.
struct LuminanceRangeMask {
    range_min: f32,
    range_max: f32,
    feather: f32,
    invert: bool,
    exposure: f32,
    contrast: f32,
    saturation: f32,
}

fn parse_luminance_range_mask(op: &serde_json::Value) -> Option<LuminanceRangeMask> {
    Some(LuminanceRangeMask {
        range_min: op.get("rangeMin")?.as_f64()? as f32,
        range_max: op.get("rangeMax")?.as_f64()? as f32,
        feather: op.get("feather").and_then(|v| v.as_f64()).unwrap_or(20.0) as f32,
        invert: op.get("invert").and_then(|v| v.as_bool()).unwrap_or(false),
        exposure: op.get("exposure").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
        contrast: op.get("contrast").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
        saturation: op
            .get("saturation")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0) as f32,
    })
}

/// Trapezoidal falloff around `[range_min, range_max]` (both 0-100,
/// divided to 0-1 here to compare against luma), same "raw expression,
/// clamp exactly once at the end" style as `mask_weight`/`radial_mask_weight`
/// -- taking `min()` of the two UNCLAMPED slope expressions before the
/// single final clamp is what keeps this correct outside the range (a
/// naive per-term-clamped version would incorrectly clamp a
/// far-outside-range luma back up toward 1 instead of 0, since a clamped
/// "rising" term alone doesn't know it's also past the falling edge).
/// `feather=0` gives `feather_width=0`, and `denom` floors to `0.001` --
/// a near-hard step exactly at the range boundaries, matching the same
/// `0.001`-floor pattern `radial_mask_weight` already established at its
/// own `feather=0`.
fn luminance_mask_weight(rgb: [f32; 3], mask: &LuminanceRangeMask) -> f32 {
    let luma = rgb[0] * 0.2126 + rgb[1] * 0.7152 + rgb[2] * 0.0722;
    let range_min = mask.range_min / 100.0;
    let range_max = mask.range_max / 100.0;
    let softness = (mask.feather / 100.0).clamp(0.0, 0.999);
    let feather_width = softness * 0.5;
    let denom = feather_width.max(0.001);
    let rising = (luma - (range_min - feather_width)) / denom;
    let falling = (range_max + feather_width - luma) / denom;
    let mut weight = rising.min(falling).clamp(0.0, 1.0);
    if mask.invert {
        weight = 1.0 - weight;
    }
    weight
}

/// A `color_range_mask` op -- like luminance range, weight depends on pixel
/// VALUE (color) rather than position, but the shape is different: one
/// reference color and one tolerance, not two edges, so this is a
/// single-sided falloff (closer to `radial_mask_weight`'s inside/outside
/// shape, but in RGB-distance space instead of image space) rather than
/// luminance's two-sided `min(rising, falling)` band. `range`/`feather`
/// stored 0-100, matching every other mask kind's own scale convention.
struct ColorRangeMask {
    ref_color: [f32; 3],
    range: f32,
    feather: f32,
    invert: bool,
    exposure: f32,
    contrast: f32,
    saturation: f32,
}

fn parse_color_range_mask(op: &serde_json::Value) -> Option<ColorRangeMask> {
    let rc = op.get("refColor")?;
    Some(ColorRangeMask {
        ref_color: [
            rc.get("r")?.as_f64()? as f32,
            rc.get("g")?.as_f64()? as f32,
            rc.get("b")?.as_f64()? as f32,
        ],
        range: op.get("range").and_then(|v| v.as_f64()).unwrap_or(25.0) as f32,
        feather: op.get("feather").and_then(|v| v.as_f64()).unwrap_or(20.0) as f32,
        invert: op.get("invert").and_then(|v| v.as_bool()).unwrap_or(false),
        exposure: op.get("exposure").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
        contrast: op.get("contrast").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
        saturation: op
            .get("saturation")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0) as f32,
    })
}

/// Single-sided trapezoid in RGB-distance space, same "raw expression,
/// clamp exactly once" style as `radial_mask_weight`/`luminance_mask_weight`.
/// `max_dist = sqrt(3)` is the largest possible Euclidean distance in the
/// RGB unit cube (e.g. black to white).
///
/// **A real bug caught by design review before implementation, not by
/// testing**: an earlier draft used
/// `weight = clamp((threshold + feather_width - dist) / denom, 0, 1)` --
/// algebraically tidier-looking, but wrong at `feather=0`
/// (`feather_width=0`, `denom` floors to `0.001`): at the exact sampled
/// pixel (`dist=0`), that formula evaluates to
/// `clamp((threshold + 0 - 0) / 0.001, 0, 1)`, and since the numerator's
/// `feather_width` term is what the formula relied on to guarantee full
/// weight right at the boundary, diluting it against the same `denom`
/// floor meant to prevent divide-by-zero meant the mask could select
/// LESS than the clicked pixel itself at low feather -- the single most
/// reachable "I want a strict color match" setting a user would pick,
/// silently producing an empty mask. Fixed below by removing
/// `feather_width` from the numerator entirely (`denom` is the transition
/// width added AFTER the threshold via the `+ 1.0`, not blended into it),
/// so `dist <= threshold` always yields exactly 1 no matter how small
/// `denom` is.
fn color_mask_weight(rgb: [f32; 3], mask: &ColorRangeMask) -> f32 {
    let dx = rgb[0] - mask.ref_color[0];
    let dy = rgb[1] - mask.ref_color[1];
    let dz = rgb[2] - mask.ref_color[2];
    let dist = (dx * dx + dy * dy + dz * dz).sqrt();
    let max_dist = 3f32.sqrt();
    let threshold = (mask.range / 100.0).clamp(0.0, 1.0) * max_dist;
    let softness = (mask.feather / 100.0).clamp(0.0, 0.999);
    let feather_width = softness * max_dist * 0.5;
    let denom = feather_width.max(0.001);
    let mut weight = ((threshold - dist) / denom + 1.0).clamp(0.0, 1.0);
    if mask.invert {
        weight = 1.0 - weight;
    }
    weight
}

/// Wraps either mask kind so `parse_masks` can preserve the edit stack's
/// TRUE op order across mixed kinds -- the frontend's `masks` array (built
/// from one unfiltered pass over `stack.ops`, see `develop.js`'s
/// `listMasks`) and the WGSL shader's packed array both already do this;
/// parsing linear and radial into two separate `Vec`s and applying "all
/// linear, then all radial" would silently diverge from that order
/// whenever a user interleaves the two kinds, which is a real parity gap,
/// not just a style choice.
enum Mask {
    Linear(LinearGradientMask),
    Radial(RadialGradientMask),
    Brush(BrushMask),
    LuminanceRange(LuminanceRangeMask),
    ColorRange(ColorRangeMask),
}

impl Mask {
    /// `aspect` (image height/width) is only consumed by `Brush`; `rgb` is
    /// only consumed by `LuminanceRange` -- the first mask kind whose
    /// weight depends on pixel VALUE, not just position. Because `rgb` is
    /// the same mutating accumulator `apply_edit_stack`'s pixel loop
    /// threads through every mask in stack order, a luminance-range mask's
    /// effective selection now depends on which masks precede it in the
    /// stack (their adjustments have already been blended into `rgb` by
    /// the time this mask's own weight is evaluated) -- the correct
    /// WYSIWYG behavior (select pixels as currently graded, matching what
    /// the user sees), not an oversight; see the parity test exercising
    /// this explicitly below.
    fn weight(&self, uv: (f32, f32), aspect: f32, rgb: [f32; 3]) -> f32 {
        match self {
            Mask::Linear(m) => mask_weight(uv, m),
            Mask::Radial(m) => radial_mask_weight(uv, m),
            Mask::Brush(m) => brush_mask_weight(uv, m, aspect),
            Mask::LuminanceRange(m) => luminance_mask_weight(rgb, m),
            Mask::ColorRange(m) => color_mask_weight(rgb, m),
        }
    }

    fn adjustments(&self) -> (f32, f32, f32) {
        match self {
            Mask::Linear(m) => (m.exposure, m.contrast, m.saturation),
            Mask::Radial(m) => (m.exposure, m.contrast, m.saturation),
            Mask::Brush(m) => (m.exposure, m.contrast, m.saturation),
            Mask::LuminanceRange(m) => (m.exposure, m.contrast, m.saturation),
            Mask::ColorRange(m) => (m.exposure, m.contrast, m.saturation),
        }
    }
}

fn parse_masks(ops: &[serde_json::Value]) -> Vec<Mask> {
    ops.iter()
        .filter_map(|op| match op.get("op").and_then(|v| v.as_str()) {
            Some("linear_gradient_mask") => parse_linear_gradient_mask(op).map(Mask::Linear),
            Some("radial_gradient_mask") => parse_radial_gradient_mask(op).map(Mask::Radial),
            Some("brush_mask") => parse_brush_mask(op).map(Mask::Brush),
            Some("luminance_range_mask") => parse_luminance_range_mask(op).map(Mask::LuminanceRange),
            Some("color_range_mask") => parse_color_range_mask(op).map(Mask::ColorRange),
            _ => None,
        })
        .collect()
}

/// Same formula as `DevelopCanvas.svelte`'s WGSL fragment shader, in the
/// same order, kept in `f32` to track the shader's precision. Not required
/// to be byte-identical to the shader's GPU output (their source
/// resolutions differ anyway -- full-res export vs. the downsampled
/// Develop preview) -- sourced from the same formula, tested against the
/// same hand-derived expected values, same tolerance the shader's own
/// numeric smoke test already uses.
///
/// Local adjustments (mask ops) are applied AFTER the global exposure/
/// contrast/saturation pass, in true stack order (via the unified `Mask`
/// enum -- see its own doc comment for why parsing kinds into separate
/// `Vec`s and applying "all of one kind, then all of the other" would be a
/// real parity gap once a user interleaves linear and radial masks) --
/// matches real Lightroom's own layering (local adjustments grade on top
/// of the globally-graded image) and the WGSL shader's own order.
pub(crate) fn apply_edit_stack(image: &mut RgbImage, stack: &EditStack) {
    let exposure_ev = op_value(&stack.ops, "exposure");
    let contrast = op_value(&stack.ops, "contrast");
    let saturation = op_value(&stack.ops, "saturation");
    let masks = parse_masks(&stack.ops);
    // Built once per call, not per pixel -- see build_curve_lut's own doc
    // comment for why this must be the same discretized LUT the WGSL
    // shader consumes, not an exact per-pixel spline evaluation.
    let curve_lut = build_curve_lut(&tone_curve_points(&stack.ops));
    let bands = hsl_bands(&stack.ops);
    let split = split_toning(&stack.ops);
    // Reuses the generic scalar-op reader `op_value` already establishes
    // for exposure/contrast/saturation -- `{op:"dehaze", value: amount}` is
    // the exact same single-scalar shape, no dedicated parser needed.
    let dehaze_amount = op_value(&stack.ops, "dehaze");
    let texture_amount = op_value(&stack.ops, "texture");
    let clarity_amount = op_value(&stack.ops, "clarity");
    let vignette = vignette_op(&stack.ops);
    let grain = grain_op(&stack.ops);

    let (width, height) = (image.width(), image.height());
    // Only consumed by brush masks (see Mask::weight) -- converts a dab's
    // width-only `radius` into a true circle in pixel space regardless of
    // the image's own aspect ratio.
    let aspect = height as f32 / width as f32;
    let (w, h) = (width as usize, height as usize);

    // Pass 1: the existing global chain (exposure -> ... -> split toning),
    // written into an intermediate buffer rather than the image directly.
    // Dehaze (below) is the first op in this pipeline that needs
    // NEIGHBORING pixels' graded value -- an in-place single loop can't
    // provide that without a race (a pixel's neighbor would already be
    // overwritten with final output before this pixel could read its
    // still-graded, not-yet-finalized value). Every op above this one was a
    // pure per-pixel remap and fit directly in one loop; this one can't.
    let mut graded = vec![[0.0f32; 3]; w * h];
    for (x, y, pixel) in image.enumerate_pixels() {
        let mut rgb = apply_adjustments(
            [
                pixel[0] as f32 / 255.0,
                pixel[1] as f32 / 255.0,
                pixel[2] as f32 / 255.0,
            ],
            exposure_ev,
            contrast,
            saturation,
        );
        rgb = [
            sample_lut(&curve_lut, rgb[0]),
            sample_lut(&curve_lut, rgb[1]),
            sample_lut(&curve_lut, rgb[2]),
        ];
        rgb = apply_hsl_bands(rgb, &bands);
        rgb = apply_split_toning(rgb, &split);
        graded[y as usize * w + x as usize] = rgb;
    }

    // Texture -> Clarity: each independently skipped at amount=0 (exact
    // passthrough, see apply_local_contrast's own doc comment), applied
    // sequentially so Clarity's blur sees Texture's already-adjusted
    // luminance -- matches Lightroom's own slider order and the GPU
    // side's pass order.
    if texture_amount != 0.0 {
        apply_local_contrast(&mut graded, w, h, TEXTURE_RADIUS, texture_amount);
    }
    if clarity_amount != 0.0 {
        apply_local_contrast(&mut graded, w, h, CLARITY_RADIUS, clarity_amount);
    }

    // Passes 2-6: Dehaze's dark-channel-prior maps (see the doc comment
    // above `dehaze_atmospheric_light` for the corrected algorithm and its
    // two named deviations from He et al.). Skipped entirely at amount=0 --
    // not just cheap-but-computed-anyway, since these are several full-image
    // passes and most photos won't use Dehaze; this also keeps "amount=0 is
    // an exact passthrough" a structural guarantee (the final loop below
    // simply has nothing to blend toward), not a numerically-near-zero one.
    let dehaze_maps = if dehaze_amount != 0.0 {
        let a = dehaze_atmospheric_light(&graded);
        let min_channel: Vec<f32> = graded
            .iter()
            .map(|c| (c[0] / a[0]).min(c[1] / a[1]).min(c[2] / a[2]))
            .collect();
        let dark_channel = separable_min_filter(&min_channel, w, h, DEHAZE_PATCH_RADIUS);
        let t_raw: Vec<f32> = dark_channel.iter().map(|d| 1.0 - DEHAZE_OMEGA * d).collect();
        let t_refined = separable_mean_filter(&t_raw, w, h, DEHAZE_REFINE_RADIUS);
        Some((a, t_refined))
    } else {
        None
    };

    // Pass 7 (final): Dehaze recovery + amount blend, then Vignette (also
    // per-pixel, no neighbor data needed, folds directly into this same
    // loop), then the existing mask loop -- reading the post-vignette
    // value as its own accumulator base, the same contract every mask
    // already has with every op above.
    for (x, y, pixel) in image.enumerate_pixels_mut() {
        let idx = y as usize * w + x as usize;
        let graded_rgb = graded[idx];
        let mut rgb = graded_rgb;
        if let Some((a, t_refined)) = &dehaze_maps {
            let t = t_refined[idx].max(DEHAZE_T0);
            for c in 0..3 {
                let recovered = (graded_rgb[c] - a[c]) / t + a[c];
                rgb[c] = graded_rgb[c] + (recovered - graded_rgb[c]) * (dehaze_amount / 100.0);
            }
        }

        // Pixel-center sampling, matching how a texture lookup samples at
        // the middle of a texel -- not required to be exact, same "not
        // byte-identical, tested to tolerance" bar as the rest of this
        // module. Computed unconditionally now (Vignette needs it too,
        // not just masks).
        let uv = (
            (x as f32 + 0.5) / width as f32,
            (y as f32 + 0.5) / height as f32,
        );

        let vf = vignette_factor(uv, aspect, &vignette);
        for c in rgb.iter_mut() {
            *c *= vf;
        }

        let gd = grain_delta((x as f32, y as f32), &grain);
        for c in rgb.iter_mut() {
            *c += gd;
        }

        if !masks.is_empty() {
            for mask in &masks {
                let weight = mask.weight(uv, aspect, rgb);
                let (m_exposure, m_contrast, m_saturation) = mask.adjustments();
                let local = apply_adjustments(rgb, m_exposure, m_contrast, m_saturation);
                for c in 0..3 {
                    rgb[c] += (local[c] - rgb[c]) * weight;
                }
            }
        }

        for (channel, value) in pixel.0.iter_mut().zip(rgb.iter()) {
            *channel = (value.clamp(0.0, 1.0) * 255.0).round() as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stack_with(ops: &[(&str, f32)]) -> EditStack {
        EditStack {
            schema_version: 1,
            ops: ops
                .iter()
                .map(|(op, value)| serde_json::json!({ "op": op, "value": value }))
                .collect(),
        }
    }

    /// Applies `apply_edit_stack` to a single pixel and asserts against a
    /// hand-derived expected value (±2/255, matching the WGSL shader's own
    /// established tolerance in m1-slice3-smoke) -- the parity table this
    /// module's doc comment promises.
    fn assert_pixel(input: [u8; 3], ops: &[(&str, f32)], expected: [i32; 3]) {
        let mut image = RgbImage::from_pixel(1, 1, image::Rgb(input));
        apply_edit_stack(&mut image, &stack_with(ops));
        let pixel = image.get_pixel(0, 0);
        for (actual, expected) in pixel.0.iter().zip(expected.iter()) {
            assert!(
                (*actual as i32 - expected).abs() <= 2,
                "expected ~{expected:?}, got {actual} (full pixel {:?}, ops {ops:?})",
                pixel.0
            );
        }
    }

    #[test]
    fn apply_edit_stack_is_a_passthrough_with_no_ops() {
        assert_pixel([100, 120, 140], &[], [100, 120, 140]);
    }

    /// The combined case from the WGSL shader's own numeric smoke test
    /// (m1-slice3-smoke): (153,51,51) + exposure+0.5/contrast+10/
    /// saturation+30 -> (255,56,56).
    #[test]
    fn apply_edit_stack_matches_the_shaders_hand_derived_combined_value() {
        assert_pixel(
            [153, 51, 51],
            &[("exposure", 0.5), ("contrast", 10.0), ("saturation", 30.0)],
            [255, 56, 56],
        );
    }

    #[test]
    fn apply_edit_stack_pure_exposure_doubles_toward_white() {
        assert_pixel([100, 100, 100], &[("exposure", 1.0)], [200, 200, 200]);
    }

    #[test]
    fn apply_edit_stack_pure_negative_exposure_halves_toward_black() {
        assert_pixel([200, 200, 200], &[("exposure", -1.0)], [100, 100, 100]);
    }

    #[test]
    fn apply_edit_stack_pure_contrast_pushes_a_bright_pixel_brighter() {
        assert_pixel([200, 200, 200], &[("contrast", 50.0)], [236, 236, 236]);
    }

    #[test]
    fn apply_edit_stack_full_desaturation_collapses_to_luma() {
        assert_pixel([200, 100, 50], &[("saturation", -100.0)], [118, 118, 118]);
    }

    #[test]
    fn apply_edit_stack_clamps_past_white_instead_of_wrapping() {
        assert_pixel([250, 250, 250], &[("exposure", 2.0)], [255, 255, 255]);
    }

    /// Builds an EditStack with the given scalar global ops (exposure/
    /// contrast/saturation, same shape `stack_with` uses) plus one
    /// `tone_curve` op -- `stack_with` alone can't express a curve's
    /// structured `points` payload.
    fn stack_with_curve(
        scalar_ops: &[(&str, f32)],
        points: &[(f32, f32)],
    ) -> EditStack {
        let mut stack = stack_with(scalar_ops);
        stack.ops.push(serde_json::json!({
            "op": "tone_curve",
            "points": points.iter().map(|(x, y)| serde_json::json!({ "x": x, "y": y })).collect::<Vec<_>>(),
        }));
        stack
    }

    fn assert_pixel_with_curve(input: [u8; 3], points: &[(f32, f32)], expected: [i32; 3]) {
        let mut image = RgbImage::from_pixel(1, 1, image::Rgb(input));
        apply_edit_stack(&mut image, &stack_with_curve(&[], points));
        let pixel = image.get_pixel(0, 0);
        for (actual, expected) in pixel.0.iter().zip(expected.iter()) {
            assert!(
                (*actual as i32 - expected).abs() <= 2,
                "expected ~{expected:?}, got {actual} (full pixel {:?}, points {points:?})",
                pixel.0
            );
        }
    }

    /// The default 2-point identity curve `(0,0)-(1,1)` must be an exact
    /// passthrough -- a Hermite cubic with tangents matching the line's own
    /// slope exactly reproduces that line, so this should hold well within
    /// the module's usual ±2/255 tolerance, not just approximately.
    #[test]
    fn tone_curve_identity_is_a_passthrough() {
        assert_pixel_with_curve([100, 120, 140], &[(0.0, 0.0), (1.0, 1.0)], [100, 120, 140]);
    }

    /// A 2-point curve degenerates to an exact straight line (see this
    /// module's own doc comment on `compute_tangents`/Hermite evaluation),
    /// so the expected values here are computed by hand exactly, not
    /// approximated: curve(x) = 0.2 + 0.8*x. Applied to r=0/255=0,
    /// g=128/255, b=255/255=1: curve(0)=0.2 -> 51, curve(128/255)=
    /// 0.2+0.8*0.501960784=0.601568627 -> 153.4 -> 153, curve(1)=1.0 ->
    /// 255. Each channel maps through the SAME function independently
    /// (this is a master curve, not a luma-weighted saturation-style
    /// adjustment) -- confirmed by these three genuinely different inputs
    /// landing at their own independently-correct outputs, not all
    /// shifted toward one shared luma value.
    #[test]
    fn tone_curve_applies_per_channel_independently() {
        assert_pixel_with_curve([0, 128, 255], &[(0.0, 0.2), (1.0, 1.0)], [51, 153, 255]);
    }

    /// The tone curve is the global pass's new final step, applied BEFORE
    /// any mask reads `rgb` as its own accumulator base -- mirrors the
    /// existing `luminance_range_mask_selection_depends_on_preceding_masks_effect`
    /// pattern, but proves the curve (not another mask) is what shifts the
    /// value a later luminance-range mask's own weight formula reads.
    ///
    /// Hand-derived: gray=73 -> luma=73/255=0.286275. A 2-point curve
    /// degenerates to an exact straight line (no spline approximation to
    /// worry about): points (0,0.3)-(1,1) give curve(x)=0.3+0.7x, so
    /// curve(0.286275)=0.3+0.7*0.286275=0.500392 -- landing comfortably
    /// inside the luminance mask's [0.45,0.55] range (5 percentage points
    /// of margin on either side, well clear of any rounding noise), so its
    /// feather=0 weight is exactly 1.0. The mask's own +0.5EV then applies
    /// at full weight: 0.500392 * 2^0.5 = 0.707745 -> round(0.707745*255)
    /// = round(180.475) = 180. If the curve applied AFTER (or was skipped
    /// by) the mask loop instead, the mask would read the pre-curve
    /// luma=0.286275 -- clearly outside [0.45,0.55] -- weight would be 0,
    /// and the pixel would stay at ~73, not jump to ~180.
    #[test]
    fn tone_curve_applies_after_saturation_before_masks() {
        let mut stack = stack_with_curve(&[], &[(0.0, 0.3), (1.0, 1.0)]);
        stack.ops.push(serde_json::json!({
            "op": "luminance_range_mask",
            "id": "curve-order-test",
            "rangeMin": 45.0,
            "rangeMax": 55.0,
            "feather": 0.0,
            "invert": false,
            "exposure": 0.5,
            "contrast": 0.0,
            "saturation": 0.0,
        }));
        let mut image = RgbImage::from_pixel(1, 1, image::Rgb([73, 73, 73]));
        apply_edit_stack(&mut image, &stack);
        let pixel = image.get_pixel(0, 0);
        for actual in pixel.0.iter() {
            assert!(
                (*actual as i32 - 180).abs() <= 2,
                "expected ~180 (curve lifts gray=73 into the mask's range, triggering its +0.5EV), got {:?}",
                pixel.0
            );
        }
    }

    fn stack_with_hsl_bands(bands: &[(&str, f32, f32, f32)]) -> EditStack {
        let bands_obj: serde_json::Map<String, serde_json::Value> = bands
            .iter()
            .map(|(name, hue, sat, lum)| {
                (
                    name.to_string(),
                    serde_json::json!({ "hue": hue, "saturation": sat, "luminance": lum }),
                )
            })
            .collect();
        EditStack {
            schema_version: 1,
            ops: vec![serde_json::json!({ "op": "hsl", "bands": bands_obj })],
        }
    }

    fn assert_pixel_with_hsl(input: [u8; 3], bands: &[(&str, f32, f32, f32)], expected: [i32; 3]) {
        let mut image = RgbImage::from_pixel(1, 1, image::Rgb(input));
        apply_edit_stack(&mut image, &stack_with_hsl_bands(bands));
        let pixel = image.get_pixel(0, 0);
        for (actual, expected) in pixel.0.iter().zip(expected.iter()) {
            assert!(
                (*actual as i32 - expected).abs() <= 2,
                "expected ~{expected:?}, got {actual} (full pixel {:?}, bands {bands:?})",
                pixel.0
            );
        }
    }

    /// All-zero (absent) bands must be an exact passthrough -- confirmed
    /// algebraically in this module's own `apply_hsl_bands` doc comment
    /// (hue_acc=sat_acc=lum_acc=0 collapses the whole formula to a
    /// round-trip through rgb_to_hsl/hsl_to_rgb of the same value).
    #[test]
    fn hsl_bands_absent_is_a_passthrough() {
        assert_pixel_with_hsl([200, 100, 50], &[], [200, 100, 50]);
    }

    /// (200,50,50) has hue exactly 0 degrees (script-verified: max=r,
    /// g==b so the numerator (g-b) is 0) -- landing exactly on the Red
    /// band's own center, where hsl_band_weight gives Red weight 1.0 and
    /// every other band (Orange is the nearest neighbor, 45 degrees away)
    /// weight exactly 0. Only Red's own hue=+50/saturation=+50/
    /// luminance=+30 should have any effect. Expected value computed via
    /// script running the exact same formula as apply_hsl_bands (not
    /// eyeballed): (246, 219, 82).
    #[test]
    fn hsl_single_band_at_its_own_center_isolates_that_band() {
        assert_pixel_with_hsl([200, 50, 50], &[("red", 50.0, 50.0, 30.0)], [246, 219, 82]);
    }

    /// (179,115,77)'s hue (script-verified) is ~22.35 degrees -- inside
    /// the Red/Orange boundary region, roughly equidistant from both
    /// centers (weight ~0.505 Red / ~0.495 Orange). Red's luminance is set
    /// to +40, Orange's to -40 -- opposite signs specifically so a hard
    /// nearest-band-only cutoff (a real bug this test would catch) would
    /// produce a LARGE shift in one direction, while the correct smooth
    /// blend nearly cancels the two (net lum_acc ~= 0.4, not +/-40).
    /// Expected value computed via script running the exact same formula:
    /// (179, 116, 78) -- a tiny shift, not the large one a hard-cutoff bug
    /// would produce.
    #[test]
    fn hsl_boundary_hue_blends_both_neighboring_bands() {
        assert_pixel_with_hsl(
            [179, 115, 77],
            &[("red", 0.0, 0.0, 40.0), ("orange", 0.0, 0.0, -40.0)],
            [179, 116, 78],
        );
    }

    /// (133,128,122) is a near-gray pixel (script-verified saturation
    /// ~0.043, well under the chroma_fade denominator of 0.08) -- proves
    /// chroma_fade meaningfully suppresses the hue/luminance shift instead
    /// of applying it at full strength. Both Red and Orange set to an
    /// identical (80,80,80) so the blend-vs-cutoff distinction (covered by
    /// the previous test) doesn't confound this one -- this test is
    /// specifically about the fade, not the blend. Expected value computed
    /// via script running the exact same formula: (185, 188, 177).
    #[test]
    fn hsl_near_gray_pixel_shift_is_suppressed_by_chroma_fade() {
        assert_pixel_with_hsl(
            [133, 128, 122],
            &[("red", 80.0, 80.0, 80.0), ("orange", 80.0, 80.0, 80.0)],
            [185, 188, 177],
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn stack_with_split_toning(
        shadow_hue: f32,
        shadow_saturation: f32,
        highlight_hue: f32,
        highlight_saturation: f32,
        balance: f32,
    ) -> EditStack {
        EditStack {
            schema_version: 1,
            ops: vec![serde_json::json!({
                "op": "split_toning",
                "shadows": { "hue": shadow_hue, "saturation": shadow_saturation },
                "highlights": { "hue": highlight_hue, "saturation": highlight_saturation },
                "balance": balance,
            })],
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn assert_pixel_with_split_toning(
        input: [u8; 3],
        shadow_hue: f32,
        shadow_saturation: f32,
        highlight_hue: f32,
        highlight_saturation: f32,
        balance: f32,
        expected: [i32; 3],
    ) {
        let mut image = RgbImage::from_pixel(1, 1, image::Rgb(input));
        apply_edit_stack(
            &mut image,
            &stack_with_split_toning(shadow_hue, shadow_saturation, highlight_hue, highlight_saturation, balance),
        );
        let pixel = image.get_pixel(0, 0);
        for (actual, expected) in pixel.0.iter().zip(expected.iter()) {
            assert!(
                (*actual as i32 - expected).abs() <= 2,
                "expected ~{expected:?}, got {actual} (full pixel {:?})",
                pixel.0
            );
        }
    }

    /// Both saturations at 0 must be an exact passthrough regardless of
    /// hue/balance -- confirmed algebraically in apply_split_toning's own
    /// doc comment (a_sh=a_hi=0 whenever both saturations are 0,
    /// independent of hue/balance, which only feed the now-zero-weighted
    /// tint terms).
    #[test]
    fn split_toning_zero_saturation_is_a_passthrough() {
        assert_pixel_with_split_toning([100, 150, 200], 200.0, 0.0, 30.0, 0.0, 50.0, [100, 150, 200]);
    }

    /// Dark gray (30,30,30) has lightness 30/255=0.1176 -- comfortably in
    /// the shadow-dominated region even at balance=0 (weight_shadows ~=
    /// 0.962). Only the Shadow zone is set (hue=200, saturation=80);
    /// Highlight is left at identity. Expected value computed via script
    /// running the exact same formula: (7, 38, 53).
    #[test]
    fn split_toning_pure_shadow_pixel_isolates_the_shadow_zone() {
        assert_pixel_with_split_toning([30, 30, 30], 200.0, 80.0, 0.0, 0.0, 0.0, [7, 38, 53]);
    }

    /// Bright gray (220,220,220) has lightness 220/255=0.8627 -- comfortably
    /// in the highlight-dominated region. Only the Highlight zone is set
    /// (hue=30, saturation=70); Shadow is left at identity. Expected value
    /// computed via script running the exact same formula: (243, 220, 197).
    #[test]
    fn split_toning_pure_highlight_pixel_isolates_the_highlight_zone() {
        assert_pixel_with_split_toning([220, 220, 220], 0.0, 0.0, 30.0, 70.0, 0.0, [243, 220, 197]);
    }

    /// Mid-gray (128,128,128) at balance=0 has weight_shadows~=0.497 and
    /// weight_highlights~=0.503 -- both zones meaningfully active at once,
    /// with deliberately NON-complementary hues (0=red, 90=chartreuse) and
    /// unequal saturations (70 vs 40) so shadow-first-sequential and
    /// highlight-first-sequential blending would each produce a
    /// DIFFERENT, wrong answer from the correct single-shot combination --
    /// script-verified: the correct simultaneous blend gives (172,109,58),
    /// while a sequential shadow-then-highlight blend of the exact same
    /// inputs gives (163,118,67), a difference well outside this test's
    /// own tolerance. This is a real regression test for the exact bug a
    /// design review caught before this formula was implemented, not a
    /// hypothetical.
    #[test]
    fn split_toning_blends_both_zones_simultaneously_not_sequentially() {
        assert_pixel_with_split_toning([128, 128, 128], 0.0, 70.0, 90.0, 40.0, 0.0, [172, 109, 58]);
    }

    /// A 1x1 test image always samples at uv=(0.5,0.5) (pixel-center
    /// sampling) -- so these cases vary the MASK's start/end to place that
    /// fixed point at the desired relative position (before/after/at the
    /// gradient), rather than varying the image. Each expected value
    /// hand-computed precisely via script (not eyeballed) before writing
    /// the assertion.
    fn mask_stack(
        start: (f32, f32),
        end: (f32, f32),
        feather: f32,
        exposure: f32,
    ) -> EditStack {
        EditStack {
            schema_version: 1,
            ops: vec![serde_json::json!({
                "op": "linear_gradient_mask",
                "id": "test-mask",
                "start": { "x": start.0, "y": start.1 },
                "end": { "x": end.0, "y": end.1 },
                "feather": feather,
                "invert": false,
                "exposure": exposure,
                "contrast": 0.0,
                "saturation": 0.0,
            })],
        }
    }

    fn assert_mask_pixel(stack: EditStack, expected: [i32; 3]) {
        let mut image = RgbImage::from_pixel(1, 1, image::Rgb([100, 100, 100]));
        apply_edit_stack(&mut image, &stack);
        let pixel = image.get_pixel(0, 0);
        for (actual, expected) in pixel.0.iter().zip(expected.iter()) {
            assert!(
                (*actual as i32 - expected).abs() <= 2,
                "expected ~{expected:?}, got {actual} (full pixel {:?})",
                pixel.0
            );
        }
    }

    #[test]
    fn linear_gradient_mask_before_start_gets_no_local_adjustment() {
        assert_mask_pixel(mask_stack((0.6, 0.5), (0.9, 0.5), 0.0, 1.0), [100, 100, 100]);
    }

    #[test]
    fn linear_gradient_mask_after_end_gets_full_local_adjustment() {
        assert_mask_pixel(mask_stack((0.1, 0.5), (0.4, 0.5), 0.0, 1.0), [200, 200, 200]);
    }

    #[test]
    fn linear_gradient_mask_midpoint_blends_halfway() {
        assert_mask_pixel(mask_stack((0.2, 0.5), (0.8, 0.5), 0.0, 1.0), [150, 150, 150]);
    }

    /// Feather widens the transition band around the midpoint rather than
    /// only softening the corners -- at feather=50 the pixel sitting
    /// exactly AT the start point (t=0) gets weight 0.25, not 0. A
    /// deliberate choice matching real Lightroom's own feather model (see
    /// `mask_weight`'s doc comment) -- this test pins that behavior down.
    #[test]
    fn linear_gradient_mask_feather_moves_the_anchor_off_zero() {
        assert_mask_pixel(mask_stack((0.5, 0.5), (0.8, 0.5), 50.0, 1.0), [125, 125, 125]);
    }

    /// Radial gradient masks (M3 Slice 6). A 1x1 test image always samples
    /// at uv=(0.5,0.5) -- these cases vary the mask's center/radius so that
    /// fixed point lands at the desired position relative to the ellipse
    /// (center, well outside, etc). Each expected value hand-computed
    /// precisely via script (not eyeballed), same pattern as the linear
    /// cases above.
    fn radial_mask_stack(
        center: (f32, f32),
        radius_x: f32,
        radius_y: f32,
        feather: f32,
        invert: bool,
        exposure: f32,
    ) -> EditStack {
        EditStack {
            schema_version: 1,
            ops: vec![serde_json::json!({
                "op": "radial_gradient_mask",
                "id": "test-radial-mask",
                "center": { "x": center.0, "y": center.1 },
                "radiusX": radius_x,
                "radiusY": radius_y,
                "feather": feather,
                "invert": invert,
                "exposure": exposure,
                "contrast": 0.0,
                "saturation": 0.0,
            })],
        }
    }

    /// Default (invert=false) applies OUTSIDE the ellipse -- a pixel at
    /// dead center is fully "inside", so gets NO local adjustment.
    #[test]
    fn radial_gradient_mask_center_default_outside_gets_no_local_adjustment() {
        assert_mask_pixel(
            radial_mask_stack((0.5, 0.5), 0.3, 0.3, 0.0, false, 1.0),
            [100, 100, 100],
        );
    }

    /// Same geometry, invert=true (applies INSIDE) -- center gets the full
    /// local adjustment.
    #[test]
    fn radial_gradient_mask_center_inverted_gets_full_local_adjustment() {
        assert_mask_pixel(
            radial_mask_stack((0.5, 0.5), 0.3, 0.3, 0.0, true, 1.0),
            [200, 200, 200],
        );
    }

    /// Well outside the ellipse, default (outside) -- gets the full local
    /// adjustment (the opposite of the center case above).
    #[test]
    fn radial_gradient_mask_outside_default_gets_full_local_adjustment() {
        assert_mask_pixel(
            radial_mask_stack((0.1, 0.1), 0.05, 0.05, 0.0, false, 1.0),
            [200, 200, 200],
        );
    }

    /// Center is always fully "inside" regardless of feather -- a heavily
    /// feathered, inverted mask still gives the center the full effect.
    #[test]
    fn radial_gradient_mask_center_stays_fully_inside_even_when_feathered() {
        assert_mask_pixel(
            radial_mask_stack((0.5, 0.5), 0.3, 0.3, 50.0, true, 1.0),
            [200, 200, 200],
        );
    }

    /// Well outside, feathered, inverted (inside-only effect) -- stays
    /// unaffected, confirming feathering doesn't leak the inside effect
    /// arbitrarily far outward.
    #[test]
    fn radial_gradient_mask_outside_feathered_inverted_stays_unaffected() {
        assert_mask_pixel(
            radial_mask_stack((0.1, 0.1), 0.05, 0.05, 50.0, true, 1.0),
            [100, 100, 100],
        );
    }

    /// Brush masks (M3 Slice 7). A 1x1 test image always samples at
    /// uv=(0.5,0.5) -- these cases place dabs at hand-computed positions so
    /// that fixed point lands at the desired distance/falloff, same
    /// pattern as the linear/radial cases above. `aspect` is 1.0 for a 1x1
    /// image, so dab.radius (width-only) behaves as a plain circle here.
    #[allow(clippy::too_many_arguments)]
    fn dab(x: f32, y: f32, radius: f32, hardness: f32, flow: f32, mode: &str) -> serde_json::Value {
        serde_json::json!({ "x": x, "y": y, "radius": radius, "hardness": hardness, "flow": flow, "mode": mode })
    }

    fn brush_mask_stack(dabs: Vec<serde_json::Value>, invert: bool, exposure: f32) -> EditStack {
        EditStack {
            schema_version: 1,
            ops: vec![serde_json::json!({
                "op": "brush_mask",
                "id": "test-brush-mask",
                "dabs": dabs,
                "invert": invert,
                "exposure": exposure,
                "contrast": 0.0,
                "saturation": 0.0,
            })],
        }
    }

    /// A hard-edged dab (hardness=100) dead center gets the full local
    /// adjustment -- same shape as the radial "center" case.
    #[test]
    fn brush_mask_dab_at_center_gets_full_local_adjustment() {
        assert_mask_pixel(
            brush_mask_stack(vec![dab(0.5, 0.5, 0.3, 100.0, 1.0, "add")], false, 1.0),
            [200, 200, 200],
        );
    }

    /// Exactly at the dab's radius boundary -- normalized_d=1.0 is
    /// excluded (falloff=0), matching `dab_falloff`'s `>= 1.0` cutoff.
    #[test]
    fn brush_mask_dab_exactly_at_radius_boundary_gets_no_local_adjustment() {
        assert_mask_pixel(
            brush_mask_stack(vec![dab(0.2, 0.5, 0.3, 100.0, 1.0, "add")], false, 1.0),
            [100, 100, 100],
        );
    }

    /// Well outside any dab's radius -- no local adjustment.
    #[test]
    fn brush_mask_pixel_outside_every_dab_gets_no_local_adjustment() {
        assert_mask_pixel(
            brush_mask_stack(vec![dab(0.1, 0.1, 0.05, 100.0, 1.0, "add")], false, 1.0),
            [100, 100, 100],
        );
    }

    /// Two overlapping ADD dabs at the identical position/radius/hardness
    /// (hardness=0, so the fixed sample point at distance 0.15 from a
    /// radius-0.3 dab center falls exactly halfway through the falloff
    /// band, giving weight 0.5) must union via max, not sum -- two
    /// identical partial-coverage dabs still give weight 0.5, not 1.0.
    #[test]
    fn brush_mask_overlapping_add_dabs_union_via_max_not_sum() {
        assert_mask_pixel(
            brush_mask_stack(
                vec![
                    dab(0.35, 0.5, 0.3, 0.0, 1.0, "add"),
                    dab(0.35, 0.5, 0.3, 0.0, 1.0, "add"),
                ],
                false,
                1.0,
            ),
            [150, 150, 150],
        );
    }

    /// An ERASE dab reduces the running weight MULTIPLICATIVELY, not by
    /// subtraction -- an add dab giving weight 0.5, followed by an erase
    /// dab with its own falloff 0.5 at the same spot, gives weight
    /// 0.5*(1-0.5)=0.25 (multiplicative). A subtractive formula
    /// (0.5-0.5=0) would produce a visibly different pixel ([100,100,100]
    /// instead of [125,125,125]), so this test distinguishes the two.
    #[test]
    fn brush_mask_erase_dab_reduces_weight_multiplicatively() {
        assert_mask_pixel(
            brush_mask_stack(
                vec![
                    dab(0.35, 0.5, 0.3, 0.0, 1.0, "add"),
                    dab(0.35, 0.5, 0.3, 0.0, 1.0, "erase"),
                ],
                false,
                1.0,
            ),
            [125, 125, 125],
        );
    }

    /// Luminance range masks -- the first kind whose weight depends on
    /// pixel VALUE, not just uv position. Unlike the geometric masks
    /// above, the sample point (uv=(0.5,0.5) via `assert_mask_pixel`'s
    /// fixed 1x1 image) never moves -- these cases instead vary the
    /// STARTING pixel's own gray value to land its luma at the desired
    /// position relative to the range. Each expected value hand-computed
    /// precisely (not eyeballed), same discipline as every mask kind above.
    fn luminance_mask_stack(
        range_min: f32,
        range_max: f32,
        feather: f32,
        invert: bool,
        exposure: f32,
    ) -> EditStack {
        EditStack {
            schema_version: 1,
            ops: vec![serde_json::json!({
                "op": "luminance_range_mask",
                "id": "test-luminance-mask",
                "rangeMin": range_min,
                "rangeMax": range_max,
                "feather": feather,
                "invert": invert,
                "exposure": exposure,
                "contrast": 0.0,
                "saturation": 0.0,
            })],
        }
    }

    fn assert_luminance_pixel(gray: u8, stack: EditStack, expected: [i32; 3]) {
        let mut image = RgbImage::from_pixel(1, 1, image::Rgb([gray, gray, gray]));
        apply_edit_stack(&mut image, &stack);
        let pixel = image.get_pixel(0, 0);
        for (actual, expected) in pixel.0.iter().zip(expected.iter()) {
            assert!(
                (*actual as i32 - expected).abs() <= 2,
                "expected ~{expected:?}, got {actual} (full pixel {:?})",
                pixel.0
            );
        }
    }

    /// gray=26 -> luma~0.102, well below range [0.3,0.7] with feather=0 --
    /// no local adjustment.
    #[test]
    fn luminance_range_mask_below_range_gets_no_local_adjustment() {
        assert_luminance_pixel(26, luminance_mask_stack(30.0, 70.0, 0.0, false, 1.0), [26, 26, 26]);
    }

    /// gray=128 -> luma~0.502, inside [0.3,0.7] with feather=0 -- full
    /// local adjustment (exposure+1.0 doubles toward white, clamped).
    #[test]
    fn luminance_range_mask_inside_range_gets_full_local_adjustment() {
        assert_luminance_pixel(128, luminance_mask_stack(30.0, 70.0, 0.0, false, 1.0), [255, 255, 255]);
    }

    /// gray=230 -> luma~0.902, well above range [0.3,0.7] with feather=0 --
    /// no local adjustment.
    #[test]
    fn luminance_range_mask_above_range_gets_no_local_adjustment() {
        assert_luminance_pixel(230, luminance_mask_stack(30.0, 70.0, 0.0, false, 1.0), [230, 230, 230]);
    }

    /// gray=51 -> luma=0.2 exactly, feather=40 -> feather_width=0.2, so
    /// this sits exactly halfway through the rising edge below range_min
    /// (0.3-0.2=0.1 at weight 0, 0.3 at weight 1, 0.2 is the midpoint) --
    /// weight=0.5, half-strength local adjustment.
    #[test]
    fn luminance_range_mask_feathered_edge_blends_halfway() {
        assert_luminance_pixel(51, luminance_mask_stack(30.0, 70.0, 40.0, false, 1.0), [76, 76, 76]);
    }

    /// Order-dependency (design point 4): a linear mask with full weight
    /// at the test point doubles gray=64 (luma~0.251) to ~0.502 BEFORE the
    /// luminance-range mask (range [45,55], feather=0) evaluates its own
    /// weight against the ALREADY-DOUBLED rgb -- 0.502 falls INSIDE
    /// [0.45,0.55], so the luminance mask's own +0.5EV boost also applies,
    /// landing at ~181. If mask order didn't matter (an incorrect
    /// implementation evaluating luminance weight against the ORIGINAL,
    /// pre-linear-mask rgb=0.251, which falls OUTSIDE the range), the
    /// luminance mask would have no effect at all and the result would
    /// stop at ~128 (just the linear mask's own doubling) -- a value this
    /// test's ±2 tolerance cannot accidentally satisfy alongside 181,
    /// making this a real, discriminating test, not just a smoke check.
    #[test]
    fn luminance_range_mask_selection_depends_on_preceding_masks_effect() {
        let stack = EditStack {
            schema_version: 1,
            ops: vec![
                serde_json::json!({
                    "op": "linear_gradient_mask",
                    "id": "order-test-linear",
                    "start": { "x": 0.1, "y": 0.5 },
                    "end": { "x": 0.4, "y": 0.5 },
                    "feather": 0.0,
                    "invert": false,
                    "exposure": 1.0,
                    "contrast": 0.0,
                    "saturation": 0.0,
                }),
                serde_json::json!({
                    "op": "luminance_range_mask",
                    "id": "order-test-luminance",
                    "rangeMin": 45.0,
                    "rangeMax": 55.0,
                    "feather": 0.0,
                    "invert": false,
                    "exposure": 0.5,
                    "contrast": 0.0,
                    "saturation": 0.0,
                }),
            ],
        };
        assert_luminance_pixel(64, stack, [181, 181, 181]);
    }

    /// Color range masks -- single reference color + tolerance, single-sided
    /// falloff (unlike luminance's two-sided band). These cases use a GRAY
    /// reference color and a gray starting pixel so `dist = |gray - ref| *
    /// sqrt(3)` (all three channels differ from the reference by the same
    /// amount), keeping the distance arithmetic simple to hand-verify.
    fn color_mask_stack(ref_gray: f32, range: f32, feather: f32, invert: bool, exposure: f32) -> EditStack {
        EditStack {
            schema_version: 1,
            ops: vec![serde_json::json!({
                "op": "color_range_mask",
                "id": "test-color-mask",
                "refColor": { "r": ref_gray, "g": ref_gray, "b": ref_gray },
                "range": range,
                "feather": feather,
                "invert": invert,
                "exposure": exposure,
                "contrast": 0.0,
                "saturation": 0.0,
            })],
        }
    }

    fn assert_color_pixel(gray: u8, stack: EditStack, expected: [i32; 3]) {
        let mut image = RgbImage::from_pixel(1, 1, image::Rgb([gray, gray, gray]));
        apply_edit_stack(&mut image, &stack);
        let pixel = image.get_pixel(0, 0);
        for (actual, expected) in pixel.0.iter().zip(expected.iter()) {
            assert!(
                (*actual as i32 - expected).abs() <= 2,
                "expected ~{expected:?}, got {actual} (full pixel {:?})",
                pixel.0
            );
        }
    }

    /// The exact regression case for the bug the design review caught:
    /// `range=0, feather=0` (the most obvious "strict color match" setting)
    /// against the EXACT reference color (`ref_gray = 128/255`, matching
    /// the pixel's own f32 conversion bit-for-bit, so `dist=0` exactly) --
    /// must still select the clicked pixel at full weight, not zero.
    #[test]
    fn color_range_mask_exact_match_at_zero_feather_gets_full_local_adjustment() {
        let ref_gray = 128.0 / 255.0;
        assert_color_pixel(128, color_mask_stack(ref_gray, 0.0, 0.0, false, 1.0), [255, 255, 255]);
    }

    /// White (gray=255) against a black reference (`ref_gray=0`) is
    /// `dist = sqrt(3)` -- the maximum possible distance -- well outside
    /// `range=25, feather=20`'s `threshold + denom` (~0.606): no local
    /// adjustment.
    #[test]
    fn color_range_mask_far_pixel_gets_no_local_adjustment() {
        assert_color_pixel(255, color_mask_stack(0.0, 25.0, 20.0, false, 1.0), [255, 255, 255]);
    }

    /// `ref_gray=0.3`, pixel gray=153/255=0.6 exactly -> `dist = 0.3*sqrt(3)
    /// = 0.5196152`. With `range=25` (`threshold=0.4330127`) and `feather=20`
    /// (`denom=0.1732051`), that's exactly `threshold + 0.5*denom` -- the
    /// midpoint of the transition band -- so `weight=0.5`, a half-strength
    /// local adjustment (exposure+1.0 doubles toward white at half
    /// strength: `0.6 + (1.2-0.6)*0.5 = 0.9` -> byte 230).
    #[test]
    fn color_range_mask_feathered_edge_blends_halfway() {
        assert_color_pixel(153, color_mask_stack(0.3, 25.0, 20.0, false, 1.0), [230, 230, 230]);
    }

    /// Order-dependency, mirroring luminance range's own such test: a
    /// linear mask (full weight at the test point) boosts gray=128/255 by
    /// +0.3EV to ~0.617987 BEFORE the color-range mask (ref_gray=0.6,
    /// range=5 -> threshold=0.0866025, feather=0 -> near-hard edge) checks
    /// its own distance -- `dist(0.617987, 0.6) = 0.031152`, well inside
    /// threshold, so the color-range mask's own +0.5EV also applies, landing
    /// at ~0.873962 (byte ~223). If mask order didn't matter (evaluated
    /// against the ORIGINAL rgb=0.501961 instead), `dist = 0.169809` is far
    /// outside threshold -- weight=0, and the result would stop at just the
    /// linear mask's own boost (~0.617987, byte ~158) -- a value this test's
    /// ±2 tolerance cannot accidentally satisfy alongside 223.
    #[test]
    fn color_range_mask_selection_depends_on_preceding_masks_effect() {
        let stack = EditStack {
            schema_version: 1,
            ops: vec![
                serde_json::json!({
                    "op": "linear_gradient_mask",
                    "id": "color-order-test-linear",
                    "start": { "x": 0.1, "y": 0.5 },
                    "end": { "x": 0.4, "y": 0.5 },
                    "feather": 0.0,
                    "invert": false,
                    "exposure": 0.3,
                    "contrast": 0.0,
                    "saturation": 0.0,
                }),
                serde_json::json!({
                    "op": "color_range_mask",
                    "id": "color-order-test-color",
                    "refColor": { "r": 0.6, "g": 0.6, "b": 0.6 },
                    "range": 5.0,
                    "feather": 0.0,
                    "invert": false,
                    "exposure": 0.5,
                    "contrast": 0.0,
                    "saturation": 0.0,
                }),
            ],
        };
        assert_color_pixel(128, stack, [223, 223, 223]);
    }

    // Dehaze -- the first op in this module needing a real multi-pixel
    // image (every earlier op's test used a 1x1 image, since every earlier
    // op is purely per-pixel; dark-channel-prior genuinely needs spatial
    // extent to exercise its windowed min-filter).

    /// A small "sky" patch (near-white, brighter than everything else) in
    /// one corner plus a uniform "scene" fill everywhere else -- crafted so
    /// `dehaze_atmospheric_light` picks the sky patch's own exact color
    /// (highest luminance) and the DARK-CHANNEL WINDOW (15x15, radius 7) is
    /// always much larger than the 3x3 patch, so every pixel's own
    /// dark-channel value is dominated by the scene's (lower) minChannel
    /// value -- confirmed by hand: even a window centered at the very
    /// corner (0,0), clamped to an 8x8 region, still reaches scene pixels
    /// at x/y >= 3, so no window is ever entirely contained within the
    /// patch. This makes the dark channel/transmission maps uniform across
    /// the whole image, which is what makes this test's expected values
    /// hand-computable exactly rather than needing a real reference
    /// implementation to check against.
    fn dehaze_test_image(width: u32, height: u32, scene: [u8; 3], sky: [u8; 3]) -> RgbImage {
        image::ImageBuffer::from_fn(width, height, |x, y| {
            if x < 3 && y < 3 {
                image::Rgb(sky)
            } else {
                image::Rgb(scene)
            }
        })
    }

    /// amount=0 must be an EXACT passthrough regardless of the rest of the
    /// algorithm -- not just numerically close to identity, structurally
    /// guaranteed by `apply_edit_stack` skipping the entire dark-channel/
    /// transmission computation at amount=0 (see its own doc comment), so
    /// this also doubles as a check that the skip path doesn't panic/
    /// misbehave on a real multi-pixel image.
    #[test]
    fn dehaze_amount_zero_is_an_exact_passthrough() {
        let mut image = dehaze_test_image(20, 20, [176, 171, 166], [242, 242, 242]);
        apply_edit_stack(&mut image, &stack_with(&[("dehaze", 0.0)]));
        assert_eq!(*image.get_pixel(19, 19), image::Rgb([176, 171, 166]));
        assert_eq!(*image.get_pixel(0, 0), image::Rgb([242, 242, 242]));
    }

    /// Hand-derived recovery at amount=100, chosen so `A=(1,1,1)` (the sky
    /// patch is pure white) -- this eliminates the `I^c/A^c` division's
    /// denominator entirely, keeping the arithmetic exact rather than
    /// approximated. Scene = (204,153,102) = (0.8, 0.6, 0.4) exactly (each
    /// is a clean multiple of 1/255).
    ///
    /// minChannel(scene) = min(0.8,0.6,0.4)/1 = 0.4 (blue channel) --
    /// dominates the whole image's dark channel per `dehaze_test_image`'s
    /// own doc comment (patch too small to ever fill an entire 15x15
    /// window), so darkChannel = 0.4 everywhere, EXACTLY (not approximated
    /// -- every window, including ones centered on the patch itself,
    /// includes scene pixels, and scene's own minChannel is the global
    /// minimum).
    ///
    /// t_raw = 1 - 0.95*0.4 = 0.62 exactly, uniform -> t_refined = 0.62
    /// exactly too (box-mean of a constant field is that same constant).
    ///
    /// Recovery (a pixel far from the patch, e.g. (19,19)):
    /// r: (0.8-1.0)/0.62 + 1.0 = 1 - 0.2/0.62 = 1 - 10/31 = 0.677419 -> 172.7 -> 173
    /// g: (0.6-1.0)/0.62 + 1.0 = 1 - 0.4/0.62 = 1 - 20/31 = 0.354839 -> 90.5 -> 90
    /// b: (0.4-1.0)/0.62 + 1.0 = 1 - 0.6/0.62 = 1 - 30/31 = 0.032258 -> 8.2 -> 8
    #[test]
    fn dehaze_amount_100_matches_hand_derived_recovery() {
        let mut image = dehaze_test_image(20, 20, [204, 153, 102], [255, 255, 255]);
        apply_edit_stack(&mut image, &stack_with(&[("dehaze", 100.0)]));
        let pixel = image.get_pixel(19, 19);
        for (actual, expected) in pixel.0.iter().zip([173, 90, 8].iter()) {
            assert!(
                (*actual as i32 - expected).abs() <= 2,
                "expected ~{expected:?}, got {actual} (full pixel {:?})",
                pixel.0
            );
        }
    }

    /// A sky pixel that already equals atmospheric light exactly (I=A)
    /// should recover UNCHANGED -- physically sensible (a clear-sky pixel
    /// needs no correction) and a real property of the formula (`(A-A)/t +
    /// A = A` for any t), worth asserting explicitly rather than only
    /// checking the scene-pixel case above.
    #[test]
    fn dehaze_pixel_already_at_atmospheric_light_is_unchanged() {
        let mut image = dehaze_test_image(20, 20, [204, 153, 102], [255, 255, 255]);
        apply_edit_stack(&mut image, &stack_with(&[("dehaze", 100.0)]));
        assert_eq!(*image.get_pixel(0, 0), image::Rgb([255, 255, 255]));
    }

    /// `dehaze_atmospheric_light` must pick a REAL pixel's whole RGB triple
    /// (argmax-by-luminance), not a synthesized independent-per-channel-max
    /// color -- the real bug a design review caught before this was
    /// written. `[1,0,0]` and `[0,1,0]` are each brightest in a DIFFERENT
    /// single channel; a per-channel-max implementation would incorrectly
    /// synthesize `[1,1,0]`, a color present in neither input pixel. The
    /// correct argmax-by-luminance picks `[0,1,0]` (luma 0.7152, the
    /// highest of the three candidates -- [1,0,0] is 0.2126, [0.5,0.5,0.5]
    /// is 0.5) as the WHOLE winning triple.
    #[test]
    fn dehaze_atmospheric_light_picks_a_real_pixel_not_a_synthesized_color() {
        let pixels = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.5, 0.5, 0.5]];
        assert_eq!(dehaze_atmospheric_light(&pixels), [0.0, 1.0, 0.0]);
    }

    /// `separable_min_filter` against a small hand-computed 5x1 array,
    /// radius=1 (3-tap window). Height=1 makes the vertical pass a no-op
    /// (each column's own single value, min'd with itself 3 times via
    /// clamped taps), isolating the horizontal pass's own correctness:
    /// x=0: taps (clamped) at indices 0,0,1 -> min(5,5,3)=3
    /// x=1: indices 0,1,2 -> min(5,3,8)=3
    /// x=2: indices 1,2,3 -> min(3,8,1)=1
    /// x=3: indices 2,3,4 -> min(8,1,9)=1
    /// x=4: indices 3,4,4 -> min(1,9,9)=1
    #[test]
    fn separable_min_filter_matches_hand_computed_values() {
        let buf = [5.0f32, 3.0, 8.0, 1.0, 9.0];
        let result = separable_min_filter(&buf, 5, 1, 1);
        assert_eq!(result, vec![3.0, 3.0, 1.0, 1.0, 1.0]);
    }

    /// `separable_mean_filter` against the same 5x1 array/radius, same
    /// no-op-vertical-pass isolation as the min-filter test above:
    /// x=0: (5+5+3)/3 = 4.333..
    /// x=1: (5+3+8)/3 = 5.333..
    /// x=2: (3+8+1)/3 = 4.0
    /// x=3: (8+1+9)/3 = 6.0
    /// x=4: (1+9+9)/3 = 6.333..
    #[test]
    fn separable_mean_filter_matches_hand_computed_values() {
        let buf = [5.0f32, 3.0, 8.0, 1.0, 9.0];
        let result = separable_mean_filter(&buf, 5, 1, 1);
        let expected = [13.0 / 3.0, 16.0 / 3.0, 4.0, 6.0, 19.0 / 3.0];
        for (actual, expected) in result.iter().zip(expected.iter()) {
            assert!((actual - expected).abs() < 0.001, "expected {expected}, got {actual}");
        }
    }

    /// amount=0 must be an EXACT passthrough (the additive-delta formula
    /// makes this true even without a skip -- `delta = (luma-blurred)*0.0
    /// == 0.0` always), and `apply_edit_stack` also skips the call
    /// entirely at amount=0 for the same reason Dehaze does. Uses a
    /// non-uniform image (so the blur itself is doing real, non-trivial
    /// work) to make sure passthrough isn't just a degenerate side effect
    /// of a uniform field.
    #[test]
    fn apply_local_contrast_amount_zero_is_exact_passthrough() {
        let mut graded = vec![[0.0, 0.0, 0.0], [0.3, 0.3, 0.3], [0.9, 0.9, 0.9]];
        let before = graded.clone();
        apply_local_contrast(&mut graded, 3, 1, 1, 0.0);
        assert_eq!(graded, before);
    }

    /// A perfectly uniform field's box-mean equals that same constant
    /// everywhere (exact, not approximate), so `luma - blurred == 0` at
    /// every pixel regardless of amount -- Texture/Clarity must leave a
    /// flat-color image untouched at ANY amount, positive or negative.
    #[test]
    fn apply_local_contrast_uniform_field_is_unaffected_by_any_amount() {
        let mut graded = vec![[0.47, 0.31, 0.16]; 9];
        apply_local_contrast(&mut graded, 3, 3, 6, 100.0);
        apply_local_contrast(&mut graded, 3, 3, 24, -100.0);
        for rgb in &graded {
            assert!((rgb[0] - 0.47).abs() < 1e-5, "{rgb:?}");
            assert!((rgb[1] - 0.31).abs() < 1e-5, "{rgb:?}");
            assert!((rgb[2] - 0.16).abs() < 1e-5, "{rgb:?}");
        }
    }

    /// Hand-derived delta on a 3x1 row (radius=1, so every column's window
    /// spans the whole row after edge-clamping -- clamped taps repeat the
    /// boundary value, so each column's mean is a DIFFERENT weighted
    /// average, not a plain 3-way split). Gray pixels (r=g=b) make
    /// luma == the shared channel value exactly, sidestepping the luma
    /// weight constants entirely.
    ///
    /// col0: taps (clamped) at indices 0,0,1 -> mean(0.0,0.0,0.3) = 0.1;
    ///       luma=0.0; delta = (0.0-0.1)*1.0 = -0.1 -> expected -0.1
    /// col1: taps at indices 0,1,2 -> mean(0.0,0.3,0.9) = 0.4;
    ///       luma=0.3; delta = (0.3-0.4)*1.0 = -0.1 -> expected 0.2
    /// col2: taps (clamped) at indices 1,2,2 -> mean(0.3,0.9,0.9) = 0.7;
    ///       luma=0.9; delta = (0.9-0.7)*1.0 = 0.2 -> expected 1.1
    #[test]
    fn apply_local_contrast_matches_hand_derived_delta() {
        let mut graded = vec![[0.0, 0.0, 0.0], [0.3, 0.3, 0.3], [0.9, 0.9, 0.9]];
        apply_local_contrast(&mut graded, 3, 1, 1, 100.0);
        let expected = [-0.1f32, 0.2, 1.1];
        for (rgb, expected) in graded.iter().zip(expected.iter()) {
            for c in rgb {
                assert!((c - expected).abs() < 1e-4, "expected {expected}, got {rgb:?}");
            }
        }
    }

    /// Same setup as above at amount=50 -- the delta must scale linearly
    /// with amount (factor = amount/100), confirming `amount` isn't just a
    /// binary on/off switch.
    #[test]
    fn apply_local_contrast_amount_scales_delta_linearly() {
        let mut graded = vec![[0.0, 0.0, 0.0], [0.3, 0.3, 0.3], [0.9, 0.9, 0.9]];
        apply_local_contrast(&mut graded, 3, 1, 1, 50.0);
        let expected = [-0.05f32, 0.25, 1.0];
        for (rgb, expected) in graded.iter().zip(expected.iter()) {
            for c in rgb {
                assert!((c - expected).abs() < 1e-4, "expected {expected}, got {rgb:?}");
            }
        }
    }

    /// End-to-end through `apply_edit_stack` (not just the direct
    /// `apply_local_contrast` unit above): confirms the `op_value` wiring
    /// itself, and that a nonzero Texture/Clarity plus Dehaze all
    /// coexisting at amount=0 for the other two still leaves the image
    /// untouched -- the same "everything off is an exact passthrough"
    /// contract every other op in this stack already guarantees.
    #[test]
    fn texture_and_clarity_amount_zero_is_exact_passthrough_through_edit_stack() {
        let mut image = dehaze_test_image(20, 20, [176, 171, 166], [242, 242, 242]);
        apply_edit_stack(&mut image, &stack_with(&[("texture", 0.0), ("clarity", 0.0)]));
        assert_eq!(*image.get_pixel(19, 19), image::Rgb([176, 171, 166]));
        assert_eq!(*image.get_pixel(0, 0), image::Rgb([242, 242, 242]));
    }

    /// amount=0 must be an EXACT passthrough regardless of midpoint/
    /// feather/uv -- `vignette_factor` returns exactly 1.0 without even
    /// touching the geometry, a structural guarantee, not a numerically-
    /// near-one one.
    #[test]
    fn vignette_amount_zero_is_an_exact_passthrough() {
        let v = Vignette { amount: 0.0, midpoint: 10.0, feather: 90.0 };
        assert_eq!(vignette_factor((0.0, 0.0), 0.6667, &v), 1.0);
        assert_eq!(vignette_factor((0.5, 0.5), 0.6667, &v), 1.0);
    }

    /// The dead-center pixel (normDist=0) sits below ANY positive midpoint
    /// -- smoothstep(inner, outer, 0) is exactly 0 whenever inner > 0, so
    /// the center is fully unaffected regardless of how strong `amount`
    /// is, matching a real vignette's own "corners only" shape.
    #[test]
    fn vignette_center_pixel_is_unaffected_when_midpoint_is_positive() {
        let v = Vignette { amount: -100.0, midpoint: 50.0, feather: 50.0 };
        assert_eq!(vignette_factor((0.5, 0.5), 0.6667, &v), 1.0);
    }

    /// Hand-derived corner case: midpoint=0, feather=100 puts `inner=0`,
    /// `outer=1.0` exactly, and a corner pixel's normDist is EXACTLY 1.0
    /// by `corner_dist`'s own definition (the corner IS the normalizing
    /// distance) -- smoothstep(0,1,1)=1.0 exactly, so the factor reduces
    /// to `1 + amount/100` with no partial falloff to account for.
    #[test]
    fn vignette_corner_pixel_at_full_feather_matches_hand_derived_factor() {
        let darken = Vignette { amount: -100.0, midpoint: 0.0, feather: 100.0 };
        let lighten = Vignette { amount: 60.0, midpoint: 0.0, feather: 100.0 };
        let aspect = 0.6667;
        assert!((vignette_factor((0.0, 0.0), aspect, &darken) - 0.0).abs() < 1e-4);
        assert!((vignette_factor((1.0, 1.0), aspect, &darken) - 0.0).abs() < 1e-4);
        assert!((vignette_factor((0.0, 0.0), aspect, &lighten) - 1.6).abs() < 1e-4);
    }

    /// A negative amount must never brighten and a positive amount must
    /// never darken -- monotonicity of the sign, checked at a real
    /// intermediate point (not just the corner/center extremes above).
    #[test]
    fn vignette_sign_of_amount_matches_darken_vs_lighten() {
        let aspect = 0.6667;
        let darken = Vignette { amount: -80.0, midpoint: 20.0, feather: 60.0 };
        let lighten = Vignette { amount: 80.0, midpoint: 20.0, feather: 60.0 };
        let uv = (0.9, 0.9);
        assert!(vignette_factor(uv, aspect, &darken) < 1.0);
        assert!(vignette_factor(uv, aspect, &lighten) > 1.0);
    }

    /// End-to-end through `apply_edit_stack`: a real image's corner darkens
    /// and its dead center stays untouched with a strong negative Amount
    /// and midpoint=0 -- the same real-image contract Dehaze's own
    /// end-to-end test already establishes, applied to Vignette.
    #[test]
    fn vignette_darkens_corners_and_leaves_center_untouched_through_edit_stack() {
        let mut image = RgbImage::from_pixel(21, 21, image::Rgb([200, 150, 100]));
        apply_edit_stack(
            &mut image,
            &EditStack {
                schema_version: 1,
                ops: vec![serde_json::json!({ "op": "vignette", "amount": -100.0, "midpoint": 0.0, "feather": 100.0 })],
            },
        );
        assert_eq!(*image.get_pixel(10, 10), image::Rgb([200, 150, 100]));
        let corner = image.get_pixel(0, 0);
        assert!(corner[0] < 50, "expected a near-black corner, got {corner:?}");
    }

    /// Hand-derived exact value: `sin(0.0) == 0.0` exactly, so
    /// `grain_hash(0.0, 0.0)` collapses to `fract(0.0 * 43758.5453123) ==
    /// 0.0` exactly -- one real closed-form point on an otherwise
    /// opaque-looking hash function.
    #[test]
    fn grain_hash_at_origin_is_exactly_zero() {
        assert_eq!(grain_hash(0.0, 0.0), 0.0);
    }

    /// amount=0 must be an EXACT passthrough regardless of coord/size/
    /// roughness -- `grain_delta` returns exactly 0.0 without even
    /// touching the noise lattice, same structural guarantee every other
    /// op's identity value already gets.
    #[test]
    fn grain_amount_zero_is_an_exact_passthrough() {
        let g = Grain { amount: 0.0, size: 80.0, roughness: 90.0 };
        assert_eq!(grain_delta((123.0, 456.0), &g), 0.0);
        assert_eq!(grain_delta((0.0, 0.0), &g), 0.0);
    }

    /// The grain pattern must be STATIC -- the same coordinate evaluated
    /// twice (e.g. across two separate renders) returns the identical
    /// delta, not a freshly-randomized one. `grain_delta` is a pure
    /// function of its inputs, but this pins that contract down
    /// explicitly rather than leaving it implicit.
    #[test]
    fn grain_delta_is_deterministic_for_the_same_coordinate() {
        let g = Grain { amount: 60.0, size: 40.0, roughness: 30.0 };
        let a = grain_delta((17.0, 42.0), &g);
        let b = grain_delta((17.0, 42.0), &g);
        assert_eq!(a, b);
    }

    /// At roughness=100 (pure lattice-cell hash, no bilinear
    /// interpolation), two DIFFERENT pixel coordinates that fall in the
    /// SAME lattice cell must produce the EXACT SAME delta -- this is
    /// `size`'s whole visible effect (grouping pixels into same-value
    /// blocks), and it's exactly derivable: size=100 -> cell = 1 +
    /// 1.0*(6.0-1.0) = 6.0px, so (0,0) and (3,3) both floor-divide to
    /// lattice cell (0,0), while (7,7) floor-divides to a different cell
    /// (1,1).
    #[test]
    fn grain_size_groups_pixels_into_matching_blocky_cells_at_full_roughness() {
        let g = Grain { amount: 100.0, size: 100.0, roughness: 100.0 };
        let a = grain_delta((0.0, 0.0), &g);
        let b = grain_delta((3.0, 3.0), &g);
        let c = grain_delta((7.0, 7.0), &g);
        assert_eq!(a, b, "same 6px lattice cell must match exactly");
        assert_ne!(a, c, "a different lattice cell should (almost certainly) differ");
    }

    /// Roughness actually changes the result -- blending smooth
    /// (bilinear) and rough (nearest-cell) noise at the two extremes must
    /// generally disagree at a non-lattice-aligned point, confirming the
    /// blend isn't a no-op.
    #[test]
    fn grain_roughness_zero_and_one_hundred_generally_differ() {
        let smooth = Grain { amount: 100.0, size: 50.0, roughness: 0.0 };
        let rough = Grain { amount: 100.0, size: 50.0, roughness: 100.0 };
        let coord = (11.3, 27.9);
        assert_ne!(grain_delta(coord, &smooth), grain_delta(coord, &rough));
    }

    /// End-to-end through `apply_edit_stack`: amount=0 (the default when
    /// the op is absent entirely) leaves a real image byte-for-byte
    /// unchanged, the same "everything off is an exact passthrough"
    /// contract every other op in this stack already guarantees.
    #[test]
    fn grain_absent_op_is_exact_passthrough_through_edit_stack() {
        let mut image = RgbImage::from_pixel(20, 20, image::Rgb([120, 80, 40]));
        apply_edit_stack(&mut image, &stack_with(&[]));
        assert_eq!(*image.get_pixel(5, 5), image::Rgb([120, 80, 40]));
    }
}
