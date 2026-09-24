use super::*;

/// A cheap 4-neighbor central-difference gradient magnitude over a
/// precomputed luma buffer -- Sharpening's Masking control needs a
/// genuine "is this pixel NEAR an edge" spatial signal, which is subtly
/// but really different from Detail's own "is THIS pixel's diff-from-blur
/// amplitude large" signal (a design review caught the two being
/// near-duplicate formulas in an earlier draft that reused the same
/// per-pixel diff for both -- a pixel one tap away from a hard edge has
/// small diff-from-blur itself but IS near an edge, which only a real
/// local-gradient measure like this one distinguishes). Deliberately NOT
/// a separate blur pass -- a plain Sobel-lite central difference is
/// cheap enough to compute directly from the buffer already needed for
/// the sharpen/NR blurs, no new whole-image pass required.
pub(super) fn local_gradient_magnitude(luma_buf: &[f32], width: usize, height: usize, x: usize, y: usize) -> f32 {
    let xm = x.saturating_sub(1);
    let xp = (x + 1).min(width - 1);
    let ym = y.saturating_sub(1);
    let yp = (y + 1).min(height - 1);
    let gx = luma_buf[y * width + xp] - luma_buf[y * width + xm];
    let gy = luma_buf[yp * width + x] - luma_buf[ym * width + x];
    (gx * gx + gy * gy).sqrt() * 0.5
}

/// Sharpening (M3): classic unsharp masking on LUMINANCE only (never
/// per-channel -- sharpening each RGB channel independently introduces
/// color fringing at edges), reconstructed via the same additive-delta-
/// preserves-chroma shape Texture/Clarity/Grain already established
/// (`delta` added equally to all three channels).
///
/// Two independent `smoothstep`-based soft gates, each nudged apart by an
/// epsilon at its own threshold (the same degenerate-equal-edges fix
/// Vignette's own `inner`/`outer` pair already established), multiply
/// together to scale the raw high-frequency signal:
/// - `detail_weight`: gates by THIS PIXEL'S OWN diff-from-blur amplitude
///   -- low Detail suppresses small-amplitude (fine-texture/noise-scale)
///   differences from being sharpened at all; high Detail passes nearly
///   every amplitude through.
/// - `mask_weight`: gates by the SPATIAL local-gradient-magnitude signal
///   above -- low Masking barely restricts where sharpening applies
///   (near 0 threshold), high Masking confines it to strong edges only,
///   protecting flat/noisy regions from being sharpened at all
///   regardless of their own diff amplitude.
///
/// `radius` is a genuine user-controlled pixel radius (mapped from the
/// 0-100 slider to `1..SHARPEN_MAX_RADIUS_PX`), unlike every fixed radius
/// constant elsewhere in this module -- the WGSL twin needs a real
/// uniform-driven loop bound for this op specifically, not a compile-time
/// `const` the way Texture/Clarity/Dehaze's own radii are.
pub(super) const SHARPEN_MAX_RADIUS_PX: i32 = 8;

pub(super) const SHARPEN_STRENGTH: f32 = 1.6;

pub(super) const SHARPEN_DETAIL_SCALE: f32 = 0.06;

pub(super) const SHARPEN_MASK_SCALE: f32 = 0.05;

pub(super) struct Sharpen {
    pub(super) amount: f32,
    pub(super) radius: f32,
    pub(super) detail: f32,
    pub(super) masking: f32,
}

impl Default for Sharpen {
    fn default() -> Self {
        Sharpen { amount: 0.0, radius: 25.0, detail: 50.0, masking: 0.0 }
    }
}

pub(super) fn sharpen_op(ops: &[serde_json::Value]) -> Sharpen {
    let op = ops.iter().find(|op| op.get("op").and_then(|v| v.as_str()) == Some("sharpen"));
    let field = |key: &str, default: f32| -> f32 {
        op.and_then(|o| o.get(key)).and_then(|v| v.as_f64()).map(|v| v as f32).unwrap_or(default)
    };
    Sharpen {
        amount: field("amount", 0.0),
        radius: field("radius", 25.0),
        detail: field("detail", 50.0),
        masking: field("masking", 0.0),
    }
}

/// Maps Sharpening's 0-100 Radius slider to an integer pixel radius --
/// a coarse, NAMED mapping (only `SHARPEN_MAX_RADIUS_PX` distinct integer
/// steps exist across the whole slider range, since `separable_mean_filter`
/// is integer-radius-only), not an accident of reusing that primitive.
pub(super) fn sharpen_radius_px(radius_slider: f32) -> i32 {
    let r = 1.0 + (radius_slider / 100.0) * (SHARPEN_MAX_RADIUS_PX as f32 - 1.0);
    r.round().max(1.0) as i32
}

/// Computes the additive luma delta for one pixel. `blurred_luma` is
/// `separable_mean_filter`'s own output at `sharpen_radius_px(radius)`,
/// looked up by the caller (this function only does the per-pixel
/// gate/scale math, not the whole-image blur).
pub(super) fn sharpen_delta(l: f32, blurred_luma: f32, grad_mag: f32, s: &Sharpen) -> f32 {
    if s.amount == 0.0 {
        return 0.0;
    }
    let diff = l - blurred_luma;
    let detail_threshold = (SHARPEN_DETAIL_SCALE * (1.0 - s.detail / 100.0)).max(f32::EPSILON);
    let detail_weight = smoothstep(0.0, detail_threshold, diff.abs());
    let mask_threshold = (SHARPEN_MASK_SCALE * (s.masking / 100.0)).max(f32::EPSILON);
    let mask_weight = smoothstep(0.0, mask_threshold, grad_mag);
    diff * (s.amount / 100.0) * detail_weight * mask_weight * SHARPEN_STRENGTH
}

/// Luminance Noise Reduction (M3): edge-preserving smoothing on luma at a
/// FIXED radius (not user-configurable, matching real Lightroom -- NR
/// exposes Amount/Detail/Contrast, never a radius). Same additive-delta
/// reconstruction as every other spatial op here.
///
/// `smooth_weight` is the OPPOSITE shape from Sharpening's `detail_weight`
/// above (`1 - smoothstep(...)`, not `smoothstep(...)`) -- by design:
/// sharpening ENHANCES existing signal, so its gate should INCREASE with
/// signal amplitude; smoothing REMOVES/protects signal, so how much
/// smoothing gets applied should DECREASE with signal amplitude (near-zero
/// diffs, presumed noise, get smoothed; large diffs, presumed real edges,
/// get protected). Detail shifts the threshold the same directional way
/// Sharpening's does (low Detail -> large threshold -> smooths broadly
/// including moderate texture; high Detail -> tiny threshold -> only
/// near-zero diffs smoothed, matching real Lightroom's own documented
/// "higher Detail may show more noise" behavior).
///
/// `contrast_restore` is a DELIBERATE, NAMED reinterpretation of real
/// Lightroom's own (undocumented) Contrast slider: rather than a second
/// edge-preservation gate (which would just duplicate Detail's role），
/// it partially reintroduces some of the high-frequency signal `Amount`
/// just removed, counteracting the flat/waxy look aggressive smoothing
/// can leave. Scaled by `amount` too (not just `contrast`) -- Contrast
/// restoring signal that was never removed in the first place (Amount=0)
/// wouldn't make sense.
///
/// RFC-0012: `blurred_luma` (the value `smooth_weight`/`contrast_restore`
/// above are computed relative to) comes from `guided_filter_self`, not a
/// plain `separable_mean_filter` -- self-guided, same specialization
/// Clarity's own `apply_clarity` uses (RFC-0010), since the guide and the
/// thing being smoothed are both `graded_luma` here too. This section's
/// own reconstruction formula (everything above this paragraph) is
/// unchanged; only what produces the blur it reads is different.
pub(super) const LUMA_NR_RADIUS: i32 = 3;

/// RFC-0012's `eps`: deliberately smaller than `CLARITY_GUIDED_EPS`.
/// Clarity's `eps` was tuned as a *contrast-enhancement* working point
/// (He, Sun, Tang's own detail-enhancement example, where `a` staying
/// away from 0 even in fairly flat regions is part of the desired local-
/// contrast boost) -- but Luminance NR's blur exists purely to estimate
/// "what this pixel would be without noise," so it should behave like a
/// near-plain box mean in genuinely flat/noisy regions (matching today's
/// box-mean baseline there) and back off only where local variance is
/// large enough to be a real edge rather than noise. A smaller `eps`
/// moves that flat-vs-edge threshold down into the noise-variance regime
/// this op actually operates in.
pub(super) const LUMA_NR_GUIDED_EPS: f32 = 0.0009;

pub(super) const NR_DETAIL_SCALE: f32 = 0.05;

pub(super) const NR_CONTRAST_STRENGTH: f32 = 0.6;

pub(super) struct LumaNr {
    pub(super) amount: f32,
    pub(super) detail: f32,
    pub(super) contrast: f32,
}

impl Default for LumaNr {
    fn default() -> Self {
        LumaNr { amount: 0.0, detail: 50.0, contrast: 0.0 }
    }
}

pub(super) fn luma_nr_op(ops: &[serde_json::Value]) -> LumaNr {
    let op = ops.iter().find(|op| op.get("op").and_then(|v| v.as_str()) == Some("luma_nr"));
    let field = |key: &str, default: f32| -> f32 {
        op.and_then(|o| o.get(key)).and_then(|v| v.as_f64()).map(|v| v as f32).unwrap_or(default)
    };
    LumaNr {
        amount: field("amount", 0.0),
        detail: field("detail", 50.0),
        contrast: field("contrast", 0.0),
    }
}

pub(super) fn luma_nr_delta(l: f32, blurred_luma: f32, n: &LumaNr) -> f32 {
    if n.amount == 0.0 {
        return 0.0;
    }
    let diff = l - blurred_luma;
    let edge_threshold = (NR_DETAIL_SCALE * (1.0 - n.detail / 100.0)).max(f32::EPSILON);
    let smooth_weight = 1.0 - smoothstep(0.0, edge_threshold, diff.abs());
    let smooth_delta = -diff * (n.amount / 100.0) * smooth_weight;
    let contrast_restore = diff * (n.contrast / 100.0) * NR_CONTRAST_STRENGTH * (n.amount / 100.0);
    smooth_delta + contrast_restore
}

/// Color Noise Reduction (M3): blurs full RGB together at a FIXED radius
/// (box-mean is linear per-channel, so blurring all three channels in one
/// pass is EXACTLY equivalent to blurring them independently -- not an
/// approximation), then reconstructs a per-channel delta from how much
/// each channel's own OFFSET FROM LUMA changed due to blurring (its
/// "chroma content"), not how much its raw value changed (which would
/// also capture luminance smoothing this op deliberately doesn't want).
///
/// `chroma_delta[c] = d[c] - weighted_mean(d)` where `d[c]` is the raw
/// per-channel blur delta and `weighted_mean` uses the SAME Rec. 709
/// weights `luma3` does -- this exact construction is what makes the
/// reconstruction preserve luminance EXACTLY (verified algebraically in
/// this slice's own design review, not just empirically): summing
/// `chroma_delta[c] * weights[c]` telescopes to zero by construction,
/// since `weighted_mean(d)` IS `weights . d`, so subtracting it out
/// before scaling removes the entire luma-changing component before any
/// scalar `k` is ever applied.
///
/// **Critical invariant, easy to break by accident**: `k` (`amount/100 *
/// color_smooth_weight`) MUST be the exact same scalar applied to R, G,
/// AND B for one pixel -- computing `color_smooth_weight` per-channel
/// instead of once from the joint `chroma_delta` magnitude would break
/// the cancellation above and let luma drift.
pub(super) const COLOR_NR_RADIUS: i32 = 4;

pub(super) const COLOR_NR_DETAIL_SCALE: f32 = 0.08;

pub(super) struct ColorNr {
    pub(super) amount: f32,
    pub(super) detail: f32,
}

impl Default for ColorNr {
    fn default() -> Self {
        ColorNr { amount: 0.0, detail: 50.0 }
    }
}

pub(super) fn color_nr_op(ops: &[serde_json::Value]) -> ColorNr {
    let op = ops.iter().find(|op| op.get("op").and_then(|v| v.as_str()) == Some("color_nr"));
    let field = |key: &str, default: f32| -> f32 {
        op.and_then(|o| o.get(key)).and_then(|v| v.as_f64()).map(|v| v as f32).unwrap_or(default)
    };
    ColorNr {
        amount: field("amount", 0.0),
        detail: field("detail", 50.0),
    }
}

pub(super) fn color_nr_delta(orig: [f32; 3], blurred: [f32; 3], n: &ColorNr) -> [f32; 3] {
    if n.amount == 0.0 {
        return [0.0, 0.0, 0.0];
    }
    const WEIGHTS: [f32; 3] = [0.2126, 0.7152, 0.0722];
    let d = [blurred[0] - orig[0], blurred[1] - orig[1], blurred[2] - orig[2]];
    let weighted_mean: f32 = (0..3).map(|i| WEIGHTS[i] * d[i]).sum();
    let chroma_delta = [d[0] - weighted_mean, d[1] - weighted_mean, d[2] - weighted_mean];
    let mag = (chroma_delta[0].powi(2) + chroma_delta[1].powi(2) + chroma_delta[2].powi(2)).sqrt();
    let color_threshold = (COLOR_NR_DETAIL_SCALE * (1.0 - n.detail / 100.0)).max(f32::EPSILON);
    let color_smooth_weight = 1.0 - smoothstep(0.0, color_threshold, mag);
    let k = (n.amount / 100.0) * color_smooth_weight;
    [chroma_delta[0] * k, chroma_delta[1] * k, chroma_delta[2] * k]
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
/// touch big tonal transitions -- still a plain box-mean blur; at this
/// radius there is much less low-frequency tonal structure for it to leak
/// across, so it hasn't earned the same fix Clarity gets below),
/// `CLARITY_RADIUS=24` (coarse "midtone contrast").
pub(super) const TEXTURE_RADIUS: i32 = 6;

pub(super) const CLARITY_RADIUS: i32 = 24;

/// RFC-0010's `eps`: the variance scale, in normalized `[0,1]` luma units,
/// at which `guided_filter_self` treats a region as "edge" (leave it alone)
/// vs. "flat" (blend it like a plain box mean). `(0.1)^2` is He, Sun, Tang's
/// own worked example for this exact application (their ECCV 2010 paper's
/// §5.4 "detail enhancement," a self-guided base/detail split at a
/// comparable radius) -- a real published starting point, not a guessed
/// constant, though tunable against this module's own halo-reduction tests
/// if a different working point proves better in practice.
pub(super) const CLARITY_GUIDED_EPS: f32 = 0.01;

/// Applies Texture's local-contrast pass to `graded` in place -- see the
/// doc comment above for the additive-delta formula and why it was chosen
/// over a luma-ratio rescale. `amount` is expected in -100..100; callers
/// skip this entirely at `amount == 0.0` (exact passthrough, no wasted
/// blur pass), same discipline `apply_edit_stack` already applies to
/// Dehaze. Still the plain box-mean blur -- see `TEXTURE_RADIUS`'s own doc
/// comment for why Texture didn't get Clarity's guided-filter fix here.
pub(super) fn apply_local_contrast(graded: &mut [[f32; 3]], width: usize, height: usize, radius: i32, amount: f32) {
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

/// Applies Clarity's local-contrast pass to `graded` in place (RFC-0010) --
/// same additive-delta formula and reasoning as `apply_local_contrast`
/// above, but `blurred` comes from `guided_filter_self` instead of a plain
/// box mean, so Clarity backs off near real edges instead of haloing them.
/// A separate function, not a branch inside `apply_local_contrast`, so
/// Texture's own code path and cost are provably unaffected by this change
/// (see this module's own tests). `amount` is expected in -100..100;
/// callers skip this entirely at `amount == 0.0`, same discipline as above.
pub(super) fn apply_clarity(graded: &mut [[f32; 3]], width: usize, height: usize, amount: f32) {
    let luma: Vec<f32> = graded
        .iter()
        .map(|c| c[0] * 0.2126 + c[1] * 0.7152 + c[2] * 0.0722)
        .collect();
    let blurred = guided_filter_self(&luma, width, height, CLARITY_RADIUS, CLARITY_GUIDED_EPS);
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
/// - The transmission-refinement filter (RFC-0011): a plain box mean
///   (`separable_mean_filter`) used to stand in for He et al.'s own
///   edge-preserving guided filter here -- named limitation: mild haloing
///   near strong contrast edges a real guided filter would avoid. Fixed
///   in this slice via `guided_filter` (general, two-signal -- see its
///   own doc comment for why this is a genuinely different function from
///   `guided_filter_self`, not a second caller of the same one), guided
///   by the graded image's own luma against the transmission map itself.
pub(super) const DEHAZE_PATCH_RADIUS: i32 = 7;

pub(super) const DEHAZE_OMEGA: f32 = 0.95;

pub(super) const DEHAZE_T0: f32 = 0.1;

pub(super) const DEHAZE_REFINE_RADIUS: i32 = 4;

/// RFC-0011's `eps` for the transmission-refinement guided filter --
/// analogous to `CLARITY_GUIDED_EPS`, but a transmission map's own natural
/// variance is much smaller than an image's luma variance (it's already a
/// smoothed derived quantity, not raw pixel data), so this sits well below
/// that constant -- He, Sun, Tang's own haze-removal worked example (ECCV
/// 2010 §4) uses an eps on this order for the same application.
pub(super) const DEHAZE_GUIDED_EPS: f32 = 0.0001;

/// Atmospheric light: the whole RGB triple of the graded image's own
/// highest-LUMINANCE pixel -- a single full-image scan. No bounded-pass
/// reduction chain needed here the way the GPU side needs one (see this
/// group's own doc comment above); the CPU path has no such constraint.
pub(super) fn dehaze_atmospheric_light(graded: &[[f32; 3]]) -> [f32; 3] {
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
pub(super) fn separable_min_filter(buf: &[f32], width: usize, height: usize, radius: i32) -> Vec<f32> {
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
pub(super) fn separable_mean_filter(buf: &[f32], width: usize, height: usize, radius: i32) -> Vec<f32> {
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

/// Self-guided image filter (RFC-0010; He, Sun, Tang, "Guided Image
/// Filtering," ECCV 2010 / IEEE TPAMI 2013 -- the guidance image and the
/// input being filtered are the same signal, the special case the RFC
/// derives). An edge-aware generalization of `separable_mean_filter`
/// above: on a flat region (`var_p` near zero) it reduces to that same
/// plain box mean; near a real edge (`var_p` large relative to `eps`) it
/// backs off toward the original, unblurred value instead of blending
/// across the edge the way a plain box mean does. Formally, `eps -> 0`
/// (given no perfectly flat local window anywhere) degrades this exactly
/// to the identity (no smoothing at all) -- see RFC-0010 §3 and this
/// module's own tests for both that limit and the constant-input
/// identity. (An earlier draft claimed the opposite limit, `eps ->
/// infinity`, degrades to a single `separable_mean_filter` pass -- a real
/// bug a failing test caught: it actually degrades to a DOUBLE box mean,
/// `mean_b` box-filtering an already-box-filtered `b`. See RFC-0010 §3's
/// own correction.)
///
/// Four `separable_mean_filter` calls (`mean_p`, `corr_p`, `mean_a`,
/// `mean_b`) plus two cheap per-pixel passes (the `a`/`b` compose, then
/// the final `q` compose) -- no new windowed-reduction primitive, reuses
/// the existing O(1)-per-pixel sliding-window filter throughout.
pub(super) fn guided_filter_self(buf: &[f32], width: usize, height: usize, radius: i32, eps: f32) -> Vec<f32> {
    let mean_p = separable_mean_filter(buf, width, height, radius);
    let sq: Vec<f32> = buf.iter().map(|v| v * v).collect();
    let corr_p = separable_mean_filter(&sq, width, height, radius);
    let mut a = vec![0.0f32; buf.len()];
    let mut b = vec![0.0f32; buf.len()];
    for i in 0..buf.len() {
        let var_p = corr_p[i] - mean_p[i] * mean_p[i];
        a[i] = var_p / (var_p + eps);
        b[i] = mean_p[i] - a[i] * mean_p[i];
    }
    let mean_a = separable_mean_filter(&a, width, height, radius);
    let mean_b = separable_mean_filter(&b, width, height, radius);
    (0..buf.len()).map(|i| mean_a[i] * buf[i] + mean_b[i]).collect()
}

/// General, two-signal guided image filter (RFC-0011; He, Sun, Tang, ECCV
/// 2010/TPAMI 2013) -- `guide` and `p` are DIFFERENT signals here (Dehaze:
/// the graded image's own luma guides a refinement of the transmission
/// map), unlike `guided_filter_self` above, where they're the same one.
/// Deliberately a SEPARATE function, not `guided_filter_self` generalized
/// with `guide == p` passed in: when the two signals really are the same,
/// `corr_guide` and `corr_guide_p` collapse to the identical quantity
/// (`corr_p`), and `mean_guide`/`mean_p` do too -- a naive general call
/// would recompute both redundantly, wasting two of `guided_filter_self`'s
/// four box-filter passes. Six `separable_mean_filter` calls here (vs.
/// four for the self case) plus the same two cheap per-pixel passes.
pub(super) fn guided_filter(guide: &[f32], p: &[f32], width: usize, height: usize, radius: i32, eps: f32) -> Vec<f32> {
    let mean_guide = separable_mean_filter(guide, width, height, radius);
    let mean_p = separable_mean_filter(p, width, height, radius);
    let guide_sq: Vec<f32> = guide.iter().map(|v| v * v).collect();
    let corr_guide = separable_mean_filter(&guide_sq, width, height, radius);
    let guide_p: Vec<f32> = guide.iter().zip(p.iter()).map(|(g, v)| g * v).collect();
    let corr_guide_p = separable_mean_filter(&guide_p, width, height, radius);
    let mut a = vec![0.0f32; guide.len()];
    let mut b = vec![0.0f32; guide.len()];
    for i in 0..guide.len() {
        let var_guide = corr_guide[i] - mean_guide[i] * mean_guide[i];
        let cov_guide_p = corr_guide_p[i] - mean_guide[i] * mean_p[i];
        a[i] = cov_guide_p / (var_guide + eps);
        b[i] = mean_p[i] - a[i] * mean_guide[i];
    }
    let mean_a = separable_mean_filter(&a, width, height, radius);
    let mean_b = separable_mean_filter(&b, width, height, radius);
    (0..guide.len()).map(|i| mean_a[i] * guide[i] + mean_b[i]).collect()
}
