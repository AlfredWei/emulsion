# RFC-0017: Linear-light White Balance/Exposure and S-curve Contrast (M5.6's seventh effect)

- Status: Draft — for review in PR (flips to Accepted once merged)
- Date: 2026-09-29
- Companion documents: [ADR-0004](../adr/ADR-0004-rendering-and-color-management.md), [ADR-0003](../adr/ADR-0003-raw-decoding.md), [MILESTONES §M5.6](../../PRD/MILESTONES.md#m56--develop-effects-quality--performance-research), [develop_engine/tone.rs](../../app/src-tauri/src/develop_engine/tone.rs), [develop_engine/color.rs](../../app/src-tauri/src/develop_engine/color.rs), [gpu/shaders/gradeMath.js](../../app/src/lib/gpu/shaders/gradeMath.js), [PROGRESS.md](../../PROGRESS.md)

## 0. Process note

User-directed research pass, not a self-chosen "next" candidate: the user flagged Tone (Exposure, White Balance, Contrast, and Auto Tone specifically) as visibly poor quality and asked for a proper research pass to find and fix the cause. This is also the first M5.6 slice whose target isn't drawn from the "not yet researched effects" list PROGRESS.md has been carrying forward — it's a direct redirection. The investigation below finds **two separable problems**: one fixable within this slice's scope (§3.1-§3.3, this RFC's actual content), and one genuinely bigger, already-named-elsewhere architectural gap that this RFC documents but does **not** attempt to fix (§3.4) — following this project's own "named, deferred, not silently smuggled in" discipline for a scope decision that size.

## 1. Problem

The user's complaint spans Exposure, White Balance/Temperature/Tint, Contrast, and Auto Tone. Tracing each through `develop_engine/tone.rs` and its WGSL twin (`gradeMath.js`) surfaces three concrete, fixable defects, all in the same small chain (`apply_global_adjustments`, and its local-mask sibling `apply_adjustments`):

1. **White Balance and Exposure are applied directly to gamma-encoded pixel values**, not to linear light. `apply_white_balance` multiplies each channel by a fixed coefficient of `temperature`/`tint` on the raw `[0,1]` value read straight from the decoded 8-bit preview; `apply_global_adjustments`/`apply_adjustments` do `c *= 2^exposure_ev` on that same gamma-encoded value. Both operations are genuinely multiplicative gains on *light*, not on its gamma-compressed encoding — applying them post-gamma means the visual effect no longer corresponds to a real exposure stop or a real color-temperature shift; it's a different (wrong) nonlinear transform that happens to share the same slider number.
2. **White Balance's gain formula is not luma-preserving.** `apply_white_balance`'s coefficients (`1±0.35t±0.15tint_norm` per channel) were chosen for plausible per-channel *direction* (warmer temperature increases R, decreases B) but never normalized against each other. A neutral gray `[1,1,1]` under `temperature=+100` (t=1.0) maps to `[1.35, 1.0, 0.65]`, whose luma (`0.2126*1.35 + 0.7152*1.0 + 0.0722*0.65 ≈ 1.049`) is **5% brighter than the original gray** — a supposedly hue-only Temperature slider measurably changes overall exposure as a side effect, on top of already being computed in the wrong (gamma) space.
3. **Contrast is a hard linear stretch, not a curve**: `(v - 0.5) * (1 + contrast/100) + 0.5`, with no roll-off — at any positive Contrast, values not already near the pivot get pushed straight toward (and abruptly past) 0 and 1, where the final clamp cuts them off hard rather than the smooth compression a real Contrast curve gives shadows/highlights.

**Auto Tone inherits, rather than independently causes, most of the complaint**: `computeAutoTone`/`computeAutoWhiteBalance` (`develop.js`) compute reasonable target `exposure_ev`/`temperature`/`tint` values from percentile/gray-world statistics, then hand them to the exact same `apply_global_adjustments` chain every manual slider drag goes through — so defects 1-3 above are *why* an Auto Tone click looks off, not a separate bug in Auto Tone's own heuristic.

## 2. Non-goals

- **The full linear-light RAW decode pipeline ADR-0004 already calls for and explicitly marks "still not done."** `raw_decode.rs` already has a real `decode_linear()` path (linear 16-bit LibRaw output, no auto-bright) — used today only by `hdr_merge.rs` for bracket alignment — but Develop's own preview/edit pipeline (both the CPU `RgbImage` path and the GPU `srcTexture`, an `rgba8unorm` texture populated from the same auto-brightened, gamma-baked 8-bit decode) still reads from the standard `decode()` path, not `decode_linear()`. This means there is **no real sensor headroom above the auto-bright white point** for Highlights/Whites to actually recover — a structural ceiling on Highlight recovery quality that this RFC's linearize-then-reencode trick (§3, §4) cannot lift, because it operates on a buffer that was already permanently tone-mapped and clipped at decode time. Fixing this for real means switching Develop's preview source to `decode_linear()` (or an un-auto-brightened, higher-bit-depth decode) plus the lcms2 input-profile→working-space transform ADR-0004 names — a milestone-level architecture change (new preview bit depth/texture format, thumbnail/export path implications, GPU texture format changes), not an M5.6 slice. Named here explicitly so it isn't silently conflated with this RFC's narrower fix.
- **`computeAutoTone`/`computeAutoWhiteBalance`'s own heuristic algorithms** (percentile targets, gray-world assumption) are not rewritten — they're a reasonable, standard choice of algorithm; this RFC's fix changes what they *drive*, not how they're computed. A future slice could still improve the heuristics themselves (e.g. gray-world AWB has no protection against a large colored object skewing the average) — out of scope here.
- **Parametric Tone's (Highlights/Shadows/Whites/Blacks) own four-window structure and magnitude constants are untouched.** A related, real issue exists there too — the additive luma delta applied identically to R/G/B can shift hue/desaturate a channel that's already near clipping while the others aren't — but it's a separable concern from linear-light correctness and is named here as a candidate for its own future slice, not folded into this one.
- **The Tone Curve, HSL/Color Mixer, and Split Toning stages** are legitimately perceptual/display-referred operations already, and stay exactly as they are — only the stages upstream of them (WB, Exposure, Contrast) change.

## 3. Research finding

### 3.1 Fix: linearize, apply the gain, re-encode — for exactly the two genuinely-multiplicative ops

White Balance and Exposure are the only two ops in this chain that are true physical gains on light. The standard, well-known fix (used by every serious raw processor, and requiring no change to the 8-bit preview buffer or its `rgba8unorm` texture format) is to bracket exactly these two operations with a real sRGB EOTF/OETF pair:

```
linear = srgb_to_linear(gamma_value)
linear' = linear * gain
gamma_value' = linear_to_srgb(linear')
```

using the exact (not `pow(2.2)`-approximated) sRGB transfer function, since its linear toe segment near black matters for shadow behavior:

```
srgb_to_linear(c) = c <= 0.04045 ? c/12.92 : ((c+0.055)/1.055)^2.4
linear_to_srgb(c) = c <= 0.0031308 ? c*12.92 : 1.055*c^(1/2.4) - 0.055
```

This is scoped precisely to WB and Exposure — Contrast, Parametric Tone, Saturation, Tone Curve, HSL, and Split Toning all continue to operate on the gamma-encoded value exactly as before, matching how a real raw processor separates "scene-referred" adjustments (WB, Exposure) from "display-referred" ones (everything downstream of the base render).

**A real, honestly-named consequence, not a hidden side effect**: because gamma compression is steepest near black, a nominal `+1 EV` computed correctly in linear light produces a visibly *gentler* brightening of a midtone gray than the old (wrong) direct-gamma-multiply did — e.g. a mid-gray at `100/255` moves to roughly `138/255` under the corrected formula, not `200/255`. This is not a regression to compensate for by rescaling the EV numbers; it is the fix. A real camera stop behaves exactly this way once gamma is accounted for correctly, and rescaling the slider to reproduce the old, wrong magnitude would just reintroduce the same bug under a different constant.

### 3.2 Fix: make the White Balance gain luma-preserving

Keep the existing per-channel gain *shape* (it already moves the right channels in the right direction for Temperature/Tint) but normalize the three gains by the luma they'd produce on a neutral gray, so a purely-hue-shifting WB adjustment cannot also change overall brightness:

```
g_r = 1 + 0.35*t + 0.15*tint_norm
g_g = 1 - 0.35*tint_norm
g_b = 1 - 0.35*t + 0.15*tint_norm
luma_of_gray = 0.2126*g_r + 0.7152*g_g + 0.0722*g_b
(g_r, g_g, g_b) = (g_r, g_g, g_b) / luma_of_gray
```

By construction, `0.2126*g_r' + 0.7152*g_g' + 0.0722*g_b' == 1.0` exactly for *any* temperature/tint value — a clean, provable identity (§6), not a numerically-approximate improvement. This normalization happens in the same linear-light space as §3.1 (the gain is still applied to `srgb_to_linear`-decoded values), since a physical white-balance gain is itself a linear-light operation.

**Corrected during implementation**: this section's original draft stated the identity as "a neutral gray's luma stays exactly 1.0" without qualifying *which* luma. A first version of the test checked GAMMA-space luma of the three output bytes directly (`output[i] * weight`, summed) and failed at the extremes — a real finding, not a flaky test: once each channel is individually re-encoded through the nonlinear `linear_to_srgb`, the gamma-space weighted sum of now-unequal channels is not the same quantity as the gamma-encoding of the linear-space weighted sum (`linear_to_srgb` is concave, so this is a direct instance of Jensen's inequality, not an implementation bug in the normalization itself). The identity this normalization actually proves — and the only one that's physically meaningful, since linear luma *is* physical brightness — is **linear-light** luma preservation: `0.2126*srgb_to_linear(out_r) + 0.7152*srgb_to_linear(out_g) + 0.0722*srgb_to_linear(out_b) == 1.0` exactly. §6 states the corrected claim.

### 3.3 Fix: Contrast as a smoothstep-blended S-curve, not a hard linear stretch

Reuse this codebase's own existing `smoothstep`/`smoothstep_val` primitive (already used by Vignette, Split Toning, and Highlights/Shadows/Whites/Blacks) rather than introducing a new curve family:

```
amount = clamp(contrast / 100, -1, 1)
contrast(v) = v + amount * (smoothstep(0, 1, v) - v)
```

At `amount = 0` this is the identity exactly. At `amount = 1` (Contrast = 100) it reduces exactly to `smoothstep(0, 1, v)` — a real S-curve whose derivative is 0 at both `v=0` and `v=1` (a smooth roll-off at black/white, instead of the old formula's hard linear run straight into the clamp) and 1.5 at the pivot (steeper midtones, the actual "more contrast" effect). Negative `amount` blends toward the same curve's mirror, flattening the midtone slope while necessarily steepening near the endpoints to keep them anchored at `(0,0)`/`(1,1)` — an inherent property of any curve anchored at both ends, not a bug. **Honesty caveat, same class as RFC-0016's**: this specific curve family (blend-toward-smoothstep) is a reasoned, well-behaved choice reusing an existing primitive, not a verified match to Lightroom's own Contrast curve shape, which isn't published.

### 3.4 Named, deferred: the deeper cause (see Non-goals)

Repeated from §2 for visibility: none of the above changes where the pixel data *comes from*. Develop's preview is still the auto-brightened, permanently-clipped 8-bit `decode()` output, not `decode_linear()`. This RFC makes the math *applied to* that buffer correct; it cannot manufacture highlight headroom the buffer never had. That is ADR-0004's own still-open item, and a real architecture change, not a slice.

## 4. Design — CPU (Rust)

`color.rs` gains the transfer-function pair, next to `smoothstep`/`luma3`:

```rust
pub(super) fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

pub(super) fn linear_to_srgb(c: f32) -> f32 {
    let c = c.max(0.0);
    if c <= 0.0031308 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }
}
```

`tone.rs`'s `apply_white_balance` becomes self-contained (linearize → luma-preserving gain → re-encode), same signature:

```rust
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
```

A new `apply_exposure`, factored out since both `apply_global_adjustments` and the local-mask `apply_adjustments` need the identical linear-light stop:

```rust
pub(super) fn apply_exposure(rgb: [f32; 3], exposure_ev: f32) -> [f32; 3] {
    let gain = 2f32.powf(exposure_ev);
    [
        linear_to_srgb(srgb_to_linear(rgb[0]) * gain),
        linear_to_srgb(srgb_to_linear(rgb[1]) * gain),
        linear_to_srgb(srgb_to_linear(rgb[2]) * gain),
    ]
}
```

And a new `apply_contrast`, reusing `smoothstep`:

```rust
pub(super) fn apply_contrast(rgb: [f32; 3], contrast: f32) -> [f32; 3] {
    let amount = (contrast / 100.0).clamp(-1.0, 1.0);
    rgb.map(|v| v + amount * (smoothstep(0.0, 1.0, v) - v))
}
```

`apply_global_adjustments` and `apply_adjustments` both reduce to calling these three (plus the untouched `apply_parametric_tone`/saturation), removing the duplicated inline exposure/contrast math each previously had:

```rust
pub(super) fn apply_global_adjustments(rgb, exposure_ev, contrast, saturation, temperature, tint, highlights, shadows, whites, blacks) -> [f32; 3] {
    let mut c = apply_white_balance(rgb, temperature, tint);
    c = apply_exposure(c, exposure_ev);
    c = apply_contrast(c, contrast);
    c = apply_parametric_tone(c, highlights, shadows, whites, blacks);
    let luma = luma3(c);
    c.map(|v| luma + (v - luma) * (1.0 + saturation / 100.0))
}

pub(super) fn apply_adjustments(rgb, exposure_ev, contrast, saturation) -> [f32; 3] {
    let mut c = apply_exposure(rgb, exposure_ev);
    c = apply_contrast(c, contrast);
    let luma = luma3(c);
    c.map(|v| luma + (v - luma) * (1.0 + saturation / 100.0))
}
```

## 5. Design — GPU (WGSL)

`gradeMath.js` gets the same two transfer functions and the same `apply_exposure`/`apply_contrast` factoring, mirroring the Rust side exactly:

```wgsl
fn srgbToLinear(c: f32) -> f32 {
  if (c <= 0.04045) { return c / 12.92; }
  return pow((c + 0.055) / 1.055, 2.4);
}

fn linearToSrgb(cIn: f32) -> f32 {
  let c = max(cIn, 0.0);
  if (c <= 0.0031308) { return c * 12.92; }
  return 1.055 * pow(c, 1.0 / 2.4) - 0.055;
}

fn applyExposure(rgb: vec3<f32>, exposureEv: f32) -> vec3<f32> {
  let gain = pow(2.0, exposureEv);
  return vec3<f32>(
    linearToSrgb(srgbToLinear(rgb.x) * gain),
    linearToSrgb(srgbToLinear(rgb.y) * gain),
    linearToSrgb(srgbToLinear(rgb.z) * gain),
  );
}

fn applyContrast(rgb: vec3<f32>, contrast: f32) -> vec3<f32> {
  let amount = clamp(contrast / 100.0, -1.0, 1.0);
  return rgb + amount * (vec3<f32>(
    smoothstep_val(0.0, 1.0, rgb.x),
    smoothstep_val(0.0, 1.0, rgb.y),
    smoothstep_val(0.0, 1.0, rgb.z),
  ) - rgb);
}
```

`apply_global_adjustments`'s and `apply_adjustments`'s WGSL bodies are updated the same way as §4's Rust versions — White Balance's gain computation gains the same luma-of-gray normalization, then linearizes/re-encodes; the inline `c * pow(2.0, exposure_ev)` and `(c - 0.5) * (1.0 + contrast/100.0) + 0.5` lines are replaced by calls to `applyExposure`/`applyContrast`.

## 6. Testability

- **Transfer-function round trip**: `linear_to_srgb(srgb_to_linear(x)) ≈ x` across a representative sample of `x` in `[0,1]` including near-black and near-white, within float tolerance — the basic correctness check any EOTF/OETF pair needs before anything built on top of it can be trusted.
- **White Balance is linear-light-luma-preserving, exactly, not approximately**: for a representative spread of `(temperature, tint)` values including the extremes `±100`, decoding each output channel of `apply_white_balance([1,1,1], temperature, tint)` back to linear and summing with the standard luma weights equals `1.0` within float tolerance — the corrected claim from §3.2 (linear luma, not gamma-space luma of the raw output bytes, which is a different quantity once the channels diverge), checked at the boundary values where the old formula's error was largest (5% at `temperature=100`).
- **Existing qualitative WB tests preserved**: `white_balance_temperature_positive_warms_image`/`white_balance_tint_positive_shifts_magenta` (direction-only assertions) must still pass — confirms the fix changes magnitude/luma-preservation, not which channel moves which way.
- **Exposure is now genuinely gentler at a midtone than before, and this is asserted, not just described**: a new test computing the actual output for a hand-representative gray at `+1 EV` and confirming it lands meaningfully below the old (wrong) `200/255` — a regression guard against silently reverting to the direct-gamma-multiply shape.
- **Contrast identity/extremes**: `amount=0` is an exact passthrough; `amount=1` matches `smoothstep(0,1,v)` exactly (a hand-derivable closed form, same discipline as this module's other "hand-derived exact value" tests); a representative midtone at a moderate positive Contrast should show LESS extreme displacement than the old hard-linear-stretch formula would have produced at the same nominal slider value (the concrete "no more abrupt clip" claim from §3.3).
- **Existing hard-coded exact-value tests updated, not deleted**: `apply_edit_stack_pure_exposure_doubles_toward_white`, `apply_edit_stack_pure_negative_exposure_halves_toward_black`, `apply_edit_stack_matches_the_shaders_hand_derived_combined_value`, and `apply_edit_stack_pure_contrast_pushes_a_bright_pixel_brighter` all hard-code exact output bytes derived from the *old*, incorrect formulas — every one gets a new hand-derived (or explicitly computed-and-verified, where a clean closed form isn't practical) expected value under the corrected math, with a doc comment explaining the new number is not a typo relative to the old one.
- **CPU/GPU parity**: extend `develop-cpu-gpu-parity.e2e.js` with an Exposure+WB+Contrast combined scenario, since this is the first slice where the CPU/GPU formulas both change shape (not just a blur source) — parity here is really testing that both sides implement the same nonlinear transfer functions identically, not just the same linear formula.
- **Local mask adjustments stay consistent with global**: a test confirming `apply_adjustments`'s Exposure/Contrast (used by brush/gradient masks) and `apply_global_adjustments`'s Exposure/Contrast produce the same result for the same inputs — the two were already meant to agree and shared no code before this refactor; now they share the actual functions, and a passing test proves it rather than assuming it from the refactor alone.

## 7. Exit criteria (this slice)

- `srgb_to_linear`/`linear_to_srgb` (Rust) and their WGSL twins added, round-trip tested.
- `apply_white_balance` fixed to be luma-preserving and linear-light, with the exact-identity test in §6 passing.
- `apply_exposure`/`apply_contrast` extracted and shared between `apply_global_adjustments` and `apply_adjustments` (global and local-mask adjustments verifiably consistent, not just visually similar).
- WGSL mirrors updated identically; CPU/GPU parity harness extended and passing at an established tolerance for the new combined scenario.
- Every existing hard-coded exact-value test that encoded the old, incorrect formula's output is updated with a new, honestly-derived expected value and a doc comment explaining why the number changed.
- PROGRESS.md gets an M5.6 entry recording this as a user-directed slice (not a self-chosen "next" pick), naming both the fixed-in-scope defects (§3.1-§3.3) and the explicitly-deferred, bigger architectural gap (§3.4/§2) — so a future reader doesn't mistake this slice for having solved highlight-recovery headroom, which it does not and cannot.
