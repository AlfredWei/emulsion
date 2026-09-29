use super::*;

/// Vignette (M3): a flat, non-nested payload (`{amount, midpoint, feather,
/// roundness}`) -- unlike Split Toning's per-zone shape above, there's no
/// natural per-element repetition here, so a plain struct with named fields
/// is simplest. Same "fall back to identity on partial/corrupt payload"
/// contract every other structured op already establishes. `roundness`
/// added by RFC-0016.
pub(super) struct Vignette {
    pub(super) amount: f32,
    pub(super) midpoint: f32,
    pub(super) feather: f32,
    pub(super) roundness: f32,
}

impl Default for Vignette {
    fn default() -> Self {
        Vignette { amount: 0.0, midpoint: 50.0, feather: 50.0, roundness: 0.0 }
    }
}

pub(super) fn vignette_op(ops: &[serde_json::Value]) -> Vignette {
    let op = ops.iter().find(|op| op.get("op").and_then(|v| v.as_str()) == Some("vignette"));
    let field = |key: &str, default: f32| -> f32 {
        op.and_then(|o| o.get(key)).and_then(|v| v.as_f64()).map(|v| v as f32).unwrap_or(default)
    };
    Vignette {
        amount: field("amount", 0.0),
        midpoint: field("midpoint", 50.0),
        feather: field("feather", 50.0),
        roundness: field("roundness", 0.0),
    }
}

/// RFC-0016: the exponent `vignette_factor`'s "more rectangular" branch
/// blends toward as `roundness` approaches -100. Chosen for a visually
/// smooth, clearly noticeable, non-degenerate transition -- there is no
/// reference signal to tune this against (unlike every guided-filter
/// `eps` constant elsewhere in this module, each tuned against a
/// measurable error metric), so this value is this RFC's own reasoned
/// choice, not a verified match to Adobe's internal algorithm.
const VIGNETTE_ROUNDNESS_MAX_P: f32 = 5.0;

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
/// shape squashed to the image bounds) at `roundness=0` -- RFC-0016 adds
/// `roundness`, a two-sided generalization of this norm: positive values
/// blend the aspect correction itself toward 1.0 (a true circle, ignoring
/// the image's own elongation -- "rounder than an ellipse" can't come
/// from changing the Lp exponent below, since p=2 is already the
/// roundest shape that family can produce); negative values generalize
/// the Euclidean (L2) norm to a superellipse (Lp) norm, blending the
/// exponent toward `VIGNETTE_ROUNDNESS_MAX_P` (more rectangular, matching
/// this module's own long-standing doc-comment wording for what
/// Roundness was always meant to do). See RFC-0016 for the full
/// reasoning and an explicit honesty caveat: the sign convention here is
/// this RFC's own reasoned design, not a verified match to Adobe's real,
/// undocumented algorithm. `roundness=0` reuses the exact original
/// formula, unmodified, so every vignette rendered before RFC-0016 is
/// bit-for-bit unchanged. `midpoint` (0-100) sets the normalized radius
/// (as a fraction of the center-to-corner distance, in whichever norm is
/// active) where the falloff begins; `feather` (0-100) widens the
/// transition zone from there out to the corner; `amount` (-100..100)
/// scales the resulting multiplicative brightness factor (negative
/// darkens, positive lightens, matching Lightroom's own sign convention).
/// `amount=0` is an exact passthrough (`vignette_factor` returns exactly
/// 1.0, skipping the geometry entirely) -- same discipline every other
/// op's identity value already gets.
pub(super) fn vignette_factor(uv: (f32, f32), aspect: f32, v: &Vignette) -> f32 {
    if v.amount == 0.0 {
        return 1.0;
    }
    let r = v.roundness.clamp(-100.0, 100.0);
    // Center-to-corner distance in this same normalized space --
    // dividing by it means `midpoint`/`feather` are always relative to
    // "how far out toward the corner", regardless of the image's own
    // aspect ratio or which of the three branches below is active.
    let norm_dist = if r == 0.0 {
        // Untouched since before RFC-0016 -- guarantees roundness=0 is
        // bit-for-bit identical to every vignette rendered before this
        // RFC, not merely numerically close to it.
        let dx = (uv.0 - 0.5) * 2.0;
        let dy = (uv.1 - 0.5) * 2.0 * aspect;
        let corner_dist = (1.0f32 + aspect * aspect).sqrt();
        (dx * dx + dy * dy).sqrt() / corner_dist
    } else if r > 0.0 {
        // RFC-0016 "rounder": blend the aspect correction itself toward
        // 1.0 (a true circle) as r approaches +100.
        let eff_aspect = aspect + (1.0 - aspect) * (r / 100.0);
        let dx = (uv.0 - 0.5) * 2.0;
        let dy = (uv.1 - 0.5) * 2.0 * eff_aspect;
        let corner_dist = (1.0f32 + eff_aspect * eff_aspect).sqrt();
        (dx * dx + dy * dy).sqrt() / corner_dist
    } else {
        // RFC-0016 "more rectangular": generalize the L2 norm to a
        // superellipse (Lp) norm, blending the exponent from 2.0 toward
        // VIGNETTE_ROUNDNESS_MAX_P as r approaches -100.
        let p = 2.0 + (-r / 100.0) * (VIGNETTE_ROUNDNESS_MAX_P - 2.0);
        let dx = (uv.0 - 0.5) * 2.0;
        let dy = (uv.1 - 0.5) * 2.0 * aspect;
        let corner_dist = (1.0f32 + aspect.powf(p)).powf(1.0 / p);
        (dx.abs().powf(p) + dy.abs().powf(p)).powf(1.0 / p) / corner_dist
    };
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
pub(super) struct Grain {
    pub(super) amount: f32,
    pub(super) size: f32,
    pub(super) roughness: f32,
}

impl Default for Grain {
    fn default() -> Self {
        Grain { amount: 0.0, size: 25.0, roughness: 50.0 }
    }
}

pub(super) fn grain_op(ops: &[serde_json::Value]) -> Grain {
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
pub(super) fn grain_hash(x: f32, y: f32) -> f32 {
    let v = (x * 12.9898 + y * 78.233).sin() * 43758.5453123;
    v - v.floor()
}

/// Bilinear-interpolated value noise over the hash lattice above -- the
/// "smooth" end of Roughness (organic, softly-varying grain rather than
/// visibly blocky cells). `coord` is already scaled by grain size (see
/// `grain_delta`'s own doc comment).
pub(super) fn grain_value_noise(coord: (f32, f32)) -> f32 {
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
pub(super) const GRAIN_MAX_CELL_PX: f32 = 6.0;

pub(super) const GRAIN_STRENGTH: f32 = 0.12;

pub(super) fn grain_delta(coord: (f32, f32), g: &Grain) -> f32 {
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
