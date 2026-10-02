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
    /// RFC-0022 §3.3: 0 = uniform amplitude (RFC-0020 behaviour), 100 = the
    /// grain fades to nothing at pure black and pure white.
    pub(super) tone: f32,
    /// RFC-0022 §3.1: 0 = one shared (luminance) noise field added to R, G
    /// and B (RFC-0020 behaviour), 100 = three independent per-channel fields
    /// (colour-negative dye-layer grain).
    pub(super) chroma: f32,
}

impl Default for Grain {
    fn default() -> Self {
        Grain { amount: 0.0, size: 25.0, roughness: 50.0, tone: 0.0, chroma: 0.0 }
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
        tone: field("tone", 0.0),
        chroma: field("chroma", 0.0),
    }
}

/// Chris Wellons' `lowbias32` 32-bit avalanche hash (Hash Prospector).
/// Pure `u32` xor/shift/wrapping-multiply, which Rust and WGSL define
/// bit-for-bit identically -- the whole reason RFC-0020 replaced the old
/// `sin`-based hash: `sin`'s error, multiplied by ~43758 before `fract`,
/// made the CPU and GPU grain fields uncorrelated (and gave the GPU field
/// real repeated structure).
fn lowbias32(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^= x >> 16;
    x
}

/// One 32-bit hash per (lattice cell, seed). `ix as u32` is a bit
/// reinterpretation, the same as WGSL's `bitcast<u32>(i32)`. With `s == 0`,
/// `particle_hash(ix, iy, 0) >> 8` over 2^24 is RFC-0020 §3.1's `grain_hash`.
/// `grain_particle_noise` inlines this with the row term hoisted (CPU
/// speed); this un-hoisted form is the spec the golden vectors pin down, and
/// the noise's own golden test proves the hoisted form agrees with it.
#[cfg(test)]
pub(super) fn particle_hash(ix: i32, iy: i32, s: u32) -> u32 {
    lowbias32((ix as u32) ^ lowbias32((iy as u32) ^ 0x9E37_79B9 ^ s))
}

/// Particles scattered per lattice cell (RFC-0020 §3.2: K = 2 is already at
/// the stationarity floor; cost is `9 * K` hashes per pixel).
pub(super) const GRAIN_PARTICLES_PER_CELL: u32 = 2;

/// Unit-variance normalization for `grain_particle_noise` (RFC-0020 §3.2):
/// `1 / sqrt(K * E[w^2] * (pi/5) * E[r^2-factor])`, in closed form.
fn grain_particle_norm(rho: f32) -> f32 {
    let ew2 = 1.0 - rho + rho * rho / 3.0;
    let a = 0.5 * rho;
    let er2 = 1.0 - a + a * a / 3.0;
    1.0 / (GRAIN_PARTICLES_PER_CELL as f32 * ew2 * 0.2 * std::f32::consts::PI * er2).sqrt()
}

/// Stationary sparse-convolution "particle" noise (RFC-0020 §3.2): a sum of
/// compact-support kernels `w * (1 - d^2/r^2)^2` from `K` random particles
/// per lattice cell, unit variance and zero mean by construction.
/// `p` is in CELL units; `roughness01` in [0, 1] (0: identical particles,
/// 1: strengths in (0,1] and radii in (0.5,1] -- clumpier); `seed` picks an
/// independent field (tests, and per-channel layers in the follow-on
/// film-simulation milestone). Every particle lives in its own cell and has
/// radius <= 1 cell, so the 3x3 block around `floor(p)` is exact -- and a
/// one-ulp `floor` disagreement between CPU and GPU only drops particles
/// whose kernel is exactly 0 there, so it cannot change the pattern.
pub(super) fn grain_particle_noise(p: (f32, f32), roughness01: f32, seed: u32) -> f32 {
    let rho = roughness01.clamp(0.0, 1.0);
    let cx = p.0.floor() as i32;
    let cy = p.1.floor() as i32;
    // `particle_hash(ix, iy, s) = lowbias32(ix ^ lowbias32(iy ^ C ^ s))`: the
    // inner term depends only on the ROW (iy) and seed, so it is computed
    // once per (row, particle) instead of once per (cell, particle) --
    // 6 inner + 18 outer hashes per call instead of 36, bit-identical results.
    let mut inner = [[0u32; GRAIN_PARTICLES_PER_CELL as usize]; 3];
    for (bi, row) in inner.iter_mut().enumerate() {
        for (k, slot) in row.iter_mut().enumerate() {
            let s = seed.wrapping_add(k as u32 * 7919);
            *slot = lowbias32(((cy + bi as i32 - 1) as u32) ^ 0x9E37_79B9 ^ s);
        }
    }
    let mut sum = 0.0f32;
    for a in -1..=1 {
        for (bi, row) in inner.iter().enumerate() {
            let b = bi as i32 - 1;
            for &inner_hash in row {
                let h = lowbias32(((cx + a) as u32) ^ inner_hash);
                let jx = (h & 0xFF) as f32 / 256.0;
                let jy = ((h >> 8) & 0xFF) as f32 / 256.0;
                let dx = p.0 - ((cx + a) as f32 + jx);
                let dy = p.1 - ((cy + b) as f32 + jy);
                let wbits = (h >> 16) & 0xFF;
                let ur = ((h >> 24) & 0xFF) as f32 / 256.0;
                let r = 1.0 - 0.5 * rho * ur;
                let sign = if wbits & 0x80 != 0 { 1.0 } else { -1.0 };
                let um = (wbits & 0x7F) as f32 / 128.0;
                let w = sign * (1.0 - rho * um);
                // Branch-free: ~35% of the 18 particles reach `p`, at random,
                // so a data-dependent branch mispredicts constantly. `max(0)`
                // gives the same 0 outside the kernel (d2 >= 1) as the WGSL
                // `if (d2 < 1.0)`.
                let d2 = (dx * dx + dy * dy) / (r * r);
                let t = (1.0 - d2).max(0.0);
                sum += w * t * t;
            }
        }
    }
    sum * grain_particle_norm(rho)
}

/// Film grain (M3, reworked by RFC-0020, sized per RFC-0022 §3.4): a pure
/// per-pixel procedural noise overlay -- like Vignette, no neighboring-pixel
/// data needed, so it folds directly into the existing final per-pixel
/// loop. Applied globally right after Vignette, before any mask (matching
/// real Lightroom's own Effects-panel order: Post-Crop Vignette, then
/// Grain, both above Local Adjustments).
///
/// `size` is FRAME-RELATIVE (RFC-0022 §3.4): the particle-lattice cell is
/// `GRAIN_CELL_UM_MIN + size/100 * GRAIN_CELL_UM_SPAN` micrometres of a
/// 36 mm frame, converted to pixels with the image's own long edge, so the
/// 2048-px interactive preview and a 6000-px export show the same grain
/// relative to the picture. Grain runs before crop, so a crop or straighten
/// carries the grain with the picture, as a real negative would. At
/// `long_edge = 6000` (24 MP) Size 25 is 2.25 px, exactly the pre-RFC-0022
/// default, so a 24 MP frame keeps its look.
///
/// `grain_footprint_ratio` makes each pixel's std equal the std of the
/// continuous field integrated over that pixel (what an export downsampled
/// from a high-resolution render holds), so sub-pixel grain in the preview
/// is not overstated by point sampling. It matches amplitude, not spectrum
/// (RFC-0022 §3.4).
///
/// `roughness` makes the particles more heterogeneous (even <-> clumpy) and
/// does NOT change the strength: the noise is unit variance at every
/// roughness. `amount` scales the resulting additive luminance delta (added
/// equally to all three channels, same "preserve chroma via an additive
/// delta" shape Texture/Clarity's own formula uses). `GRAIN_SIGMA` is the
/// std of the CONTINUOUS field at amount=100: the pixel std at the default
/// Size and Roughness on a 24 MP frame is `GRAIN_SIGMA * r(2.25, 0.5)` = 0.0511, the
/// pre-RFC-0020 field's measured value.
pub(super) const GRAIN_FRAME_UM: f32 = 36_000.0;
pub(super) const GRAIN_CELL_UM_MIN: f32 = 6.0;
pub(super) const GRAIN_CELL_UM_SPAN: f32 = 30.0;

/// Footprint-compensation constants of `r(c, rho) = c / sqrt(c^2 + K2)`,
/// `K2 = K2_BASE + K2_PER_ROUGHNESS * rho`: fitted to the box-averaged std
/// ratio measured against the supersampled field at rho 0, 0.5 and 1
/// (RFC-0022 §3.4 as corrected during implementation: rougher particles are
/// smaller, so they lose more amplitude to pixel integration).
pub(super) const GRAIN_FOOTPRINT_K2_BASE: f32 = 0.6;
pub(super) const GRAIN_FOOTPRINT_K2_PER_ROUGHNESS: f32 = 0.3;

pub(super) const GRAIN_SIGMA: f32 = 0.0548;

/// The app's grain is the same pattern for every image (unseeded).
const GRAIN_SEED: u32 = 0;

/// Grain cell width in pixels for a `size` slider value on an image whose
/// long edge is `long_edge` pixels.
pub(super) fn grain_cell_px(size: f32, long_edge: f32) -> f32 {
    (GRAIN_CELL_UM_MIN + (size / 100.0) * GRAIN_CELL_UM_SPAN) * long_edge / GRAIN_FRAME_UM
}

/// RFC-0022 §3.4: per-pixel std ratio of the pixel-integrated field to the
/// point-sampled one, for a grain cell `c` pixels wide at roughness `rho` (0..1).
pub(super) fn grain_footprint_ratio(c: f32, rho: f32) -> f32 {
    c / (c * c + GRAIN_FOOTPRINT_K2_BASE + GRAIN_FOOTPRINT_K2_PER_ROUGHNESS * rho).sqrt()
}

pub(super) fn grain_delta(coord: (f32, f32), g: &Grain, long_edge: f32) -> f32 {
    if g.amount == 0.0 {
        return 0.0;
    }
    let cell = grain_cell_px(g.size, long_edge);
    let noise = grain_particle_noise((coord.0 / cell, coord.1 / cell), g.roughness / 100.0, GRAIN_SEED);
    noise * (g.amount / 100.0) * GRAIN_SIGMA * grain_footprint_ratio(cell, g.roughness / 100.0)
}

/// RFC-0022 §3.3 tonal response: the grain delta's weight at pre-grain luma
/// `luma` (encoded, clamped to 0..1): `(1 - t) + t * 4 * Y * (1 - Y)` with
/// `t = tone / 100`. 1 at mid-gray for every `t` (so Amount keeps its
/// mid-tone meaning), 0 at pure black and pure white for `t = 1`. Real grain
/// is density noise seen through the print/scan tone curve, whose slope falls
/// toward the toe and shoulder; a dome is the simplest one-parameter
/// stand-in (reasoning, not data -- RFC-0022 §3.3). Where the weight
/// vanishes the delta cannot be clipped, which also closes RFC-0020's named
/// "additive grain lifts pure black / darkens pure white" item. `tone = 0`
/// returns exactly 1.0 (RFC-0020's uniform amplitude).
pub(super) fn grain_tone_weight(luma: f32, g: &Grain) -> f32 {
    if g.tone == 0.0 {
        return 1.0;
    }
    let t = g.tone / 100.0;
    let y = luma.clamp(0.0, 1.0);
    (1.0 - t) + t * 4.0 * y * (1.0 - y)
}

/// RFC-0022 §3.2: the three per-channel noise seeds (R, G, B). Fixed
/// constants, not exposed; distinct from the shared field's seed 0 and from
/// each other by far more than the `k * 7919` per-particle offsets
/// `grain_particle_noise` adds, so no two particle streams coincide. (The
/// first 32 bits of the fractional parts of sqrt(2), sqrt(3), sqrt(5).)
pub(super) const GRAIN_CHANNEL_SEEDS: [u32; 3] = [0x6A09_E667, 0xBB67_AE85, 0x3C6E_F372];

/// RFC-0022 §3.1 colour grain: the per-channel delta (before the tonal
/// weight). `m = chroma / 100`; every channel is
/// `SIGMA * amount * r * (sqrt(1-m) * n_shared + sqrt(m) * n_c)`. The two
/// noise terms are independent unit-variance fields, so `a^2 + b^2 = 1` keeps
/// every channel at unit variance at every `m` (the slider moves colour
/// character, never strength), and the cross-channel correlation is exactly
/// `1 - m`. `chroma = 0` evaluates ONE noise field (identical to `grain_delta`
/// on all channels -- bit-for-bit, the existing cost); `chroma = 100` skips the
/// shared field and evaluates three; anything between evaluates four.
pub(super) fn grain_delta_rgb(coord: (f32, f32), g: &Grain, long_edge: f32) -> [f32; 3] {
    if g.amount == 0.0 {
        return [0.0; 3];
    }
    let m = (g.chroma / 100.0).clamp(0.0, 1.0);
    if m == 0.0 {
        return [grain_delta(coord, g, long_edge); 3];
    }
    let cell = grain_cell_px(g.size, long_edge);
    let rho = g.roughness / 100.0;
    let p = (coord.0 / cell, coord.1 / cell);
    let scale = (g.amount / 100.0) * GRAIN_SIGMA * grain_footprint_ratio(cell, rho);
    let shared = if m < 1.0 { (1.0 - m).sqrt() * grain_particle_noise(p, rho, GRAIN_SEED) } else { 0.0 };
    let b = m.sqrt();
    let mut out = [0.0f32; 3];
    for (o, seed) in out.iter_mut().zip(GRAIN_CHANNEL_SEEDS) {
        *o = (shared + b * grain_particle_noise(p, rho, seed)) * scale;
    }
    out
}
