# RFC-0018: Parametric Tone's Hue-Preserving Clamp (M5.6's eighth effect)

- Status: Draft — for review in PR (flips to Accepted once merged)
- Date: 2026-09-30
- Companion documents: [RFC-0017](RFC-0017-tone-linear-light-correctness.md) (names this exact concern in its own §2, deferred rather than folded in), [PRD/MILESTONES §M5.6](../../PRD/MILESTONES.md#m56--develop-effects-quality--performance-research), [develop_engine/tone.rs](../../app/src-tauri/src/develop_engine/tone.rs), [develop_engine/pipeline.rs](../../app/src-tauri/src/develop_engine/pipeline.rs), [gpu/shaders/gradeMath.js](../../app/src/lib/gpu/shaders/gradeMath.js), [PROGRESS.md](../../PROGRESS.md)

## 0. Process note

Self-chosen "next" pick, drawn from a specific named item rather than the general "not yet researched effects" list: RFC-0017 §2 already found and explicitly deferred this exact concern while fixing White Balance/Exposure/Contrast — "the additive luma delta applied identically to R/G/B can shift hue/desaturate a channel that's already near clipping while the others aren't... named here as a candidate for its own future slice." This RFC is that slice.

## 1. Problem

`apply_parametric_tone` (Highlights/Shadows/Whites/Blacks) computes a single scalar `delta` from the pixel's own luma (a blend of four smoothstep-weighted windows) and adds that *same* scalar to R, G, and B identically:

```rust
let delta = delta_h + delta_s + delta_w + delta_b;
[rgb[0] + delta, rgb[1] + delta, rgb[2] + delta]
```

In isolation, adding an identical constant to all three channels is actually hue-preserving: hue is determined entirely by the *pairwise differences* between channels (`R-G`, `G-B`, `R-B`), and adding the same constant to all three leaves every pairwise difference unchanged. The problem is not this op in isolation — it's what happens next. `develop_engine`'s pipeline applies its entire op chain (WB → Exposure → Contrast → Parametric Tone → Saturation → Tone Curve → HSL → Split Toning → masks) in unclamped floating point, and clamps to `[0,1]` exactly once, at the very final byte conversion (`pipeline.rs:297`, `value.clamp(0.0, 1.0)`). Parametric Tone's whole purpose is adjusting pixels that are *already* near the tonal extremes — that's what "Highlights"/"Whites" recovery and "Shadows"/"Blacks" lift are *for* — so it is the op most likely, in ordinary use, to push one channel of an already-near-clip, saturated pixel past `0`/`1` while its sibling channels still have headroom. When that eventually reaches the single final clamp, the channel that overshot gets truncated while the others that received the identical delta do not — breaking the very pairwise-difference equality that made the identical-delta approach hue-preserving in the first place. The visible result: a saturated highlight or shadow color measurably shifts hue (not just desaturates predictably) as Highlights/Whites/Shadows/Blacks is pushed harder, rather than the smooth, hue-stable recovery a real tone-mapping tool gives.

**Concretely** (worked example, hand-verified, not just illustrative): an orange pixel `[0.95, 0.70, 0.50]` (`R-G=0.25, G-B=0.20, R-B=0.45`) at `highlights=100` (rest of the sliders at `0`) has luma `≈0.7387`, giving a raw combined delta `delta_h ≈ 0.07308` before any clamp. Unclamped, the pixel becomes `[1.0231, 0.7731, 0.5731]` — differences still exactly `0.25/0.20/0.45`, hue exactly preserved. But R can only actually reach `1.0` (the pixel's *effective* final color, once the whole chain reaches the pipeline's single final clamp, has R capped at `1.0` while G/B keep their full uncapped values) — so the delta R actually "keeps" is only `0.05` (R's own headroom), while G/B keep the full `≈0.07308`. New effective differences: `1.0-0.7731≈0.2269` (was `0.25`), `0.7731-0.5731=0.20` (unchanged), `1.0-0.5731≈0.4269` (was `0.45`). `R-G` shrank relative to the unchanged `G-B` — the ratio between the two differences changed, which is exactly a hue shift (toward yellow, since R's growth was capped more than G's).

## 2. Non-goals

- **The four smoothstep-weighted windows and their magnitude constants (`0.6`, `0.35`, the `0.5`/`0.7`/`0.3` breakpoints) are untouched.** This RFC does not re-derive Lightroom's actual Highlights/Shadows/Whites/Blacks curve shapes — that's the same already-named, honestly-caveated approximation RFC-0016's Vignette Roundness and RFC-0017's Contrast curve carry, and is out of scope here.
- **The pipeline's single-final-clamp architecture is not changed.** Clamping once at the very end (rather than after every op) is a real, load-bearing design choice elsewhere in this codebase (e.g. it's what lets Exposure meaningfully overshoot 1.0 before Contrast/Saturation still see the true unclamped value) — this RFC does not touch it. The fix below makes Parametric Tone's *own* contribution well-behaved regardless of what the final clamp eventually does; it does not and cannot prevent a *different*, later op (Saturation, Tone Curve, HSL, Split Toning) from separately introducing its own clamp-adjacent distortion — each of those remains its own, separately-scoped concern (RFC-0017 §2's own list of what's left).
- **Not a full "soft-clip"/gamut-mapping system.** A complete fix would keep the pixel in-gamut through every remaining stage of the chain; this RFC only guarantees that Parametric Tone's own delta never, by itself, causes one channel to receive a different effective delta than its siblings once everything downstream is accounted for by the final clamp. That is the specific, real defect named in RFC-0017 §2 — not a general promise about the whole pipeline's clip behavior.

## 3. Research finding

### 3.1 The fix is a shared, symmetric scale — not a per-channel independent clamp

The identical-additive-delta approach is hue-preserving by construction (§1) as long as the *same* delta actually reaches every channel. The defect is that the eventual final clamp can silently reduce the delta each channel *keeps* by a different amount. The fix is to make that reduction happen symmetrically, ourselves, before it can differ per channel: compute the single largest scale factor `s ∈ [0, 1]` such that applying `s * delta` (the *same* scaled value) to **all three** channels keeps every one of them within `[0, 1]`, then apply that one shared, scaled delta.

For a given channel value `c` and the pixel's own `delta`:

- If `delta > 0`: the binding constraint is `c + s*delta <= 1`, i.e. `s <= (1 - c) / delta`.
- If `delta < 0`: the binding constraint is `c + s*delta >= 0`, i.e. `s <= c / (-delta)`.
- `s` is the minimum of that bound across all three channels, clamped to `[0, 1]` (a channel that's already past the edge in `delta`'s own direction — e.g. `c > 1` with `delta > 0`, which can happen because an earlier op like Exposure already overshot — yields a negative bound, forcing `s = 0`: Parametric Tone simply contributes nothing further in that direction for that pixel, rather than adding to an overshoot that's already someone else's problem).

Because the *same* `s` is used for every channel, `R+s*delta`, `G+s*delta`, `B+s*delta` still differ from each other by exactly the same amounts as `R`, `G`, `B` did — pairwise differences (hence hue) are preserved **exactly**, by construction, not approximately. And by definition of `s`, this op's own output is already guaranteed inside `[0,1]` for every channel — the final clamp downstream becomes a no-op for *this op's own contribution*, so it can no longer truncate one channel of Parametric Tone's own delta more than another.

**Re-deriving the worked example from §1** with this fix: `[0.95, 0.70, 0.50]`, `delta ≈ 0.07308` (`highlights=100`, verified above). Bounds: `R: (1-0.95)/0.07308 ≈ 0.6842`, `G: (1-0.70)/0.07308 ≈ 4.105`, `B: (1-0.50)/0.07308 ≈ 6.843`. `s = min(0.6842, 4.105, 6.843, 1.0) ≈ 0.6842`. Applied delta `= delta * s ≈ 0.05` to all three (exactly R's own headroom, by construction — whenever the raw delta exceeds the tightest channel's headroom, `s` is defined precisely so the applied delta equals that headroom): `[1.0, 0.75, 0.55]`. Differences: `1.0-0.75=0.25`, `0.75-0.55=0.20`, `1.0-0.55=0.45` — **exactly** the original `0.25/0.20/0.45`. Hue is preserved exactly; R lands precisely at its own ceiling (as intended — Highlights recovery pushing a pixel exactly to white is a legitimate, correct outcome) rather than overshooting into an asymmetric truncation.

### 3.2 Saturation is affected, predictably and boundedly — not a hidden trade-off

The visible consequence of `s < 1` is that the pixel receives *less* of the requested Highlights/Shadows/Whites/Blacks push than the raw slider value would otherwise produce, whenever any channel is close enough to an extreme that the full delta would have overshot. This is the correct, intended trade-off, not a bug to work around: it is precisely the "headroom-limited recovery" behavior a real tone tool exhibits — a slider gets less effect specifically on pixels that have less room left to give, and that limit is now shared evenly across channels instead of landing arbitrarily on whichever channel happens to hit the wall first.

## 4. Design — CPU (Rust)

`apply_parametric_tone` in `tone.rs` keeps its existing four-window delta computation unchanged, and adds the shared-scale step before combining:

```rust
pub(super) fn apply_parametric_tone(
    rgb: [f32; 3],
    highlights: f32,
    shadows: f32,
    whites: f32,
    blacks: f32,
) -> [f32; 3] {
    let luma = luma3(rgb);
    // ... delta_h/delta_s/delta_w/delta_b computation: UNCHANGED ...
    let delta = delta_h + delta_s + delta_w + delta_b;
    if delta == 0.0 {
        return rgb;
    }

    // RFC-0018: a shared scale, not an independent per-channel clamp --
    // see the RFC's own worked example for why applying the SAME reduced
    // delta to every channel is what keeps hue exact, while clamping each
    // channel independently (or relying on the pipeline's own single
    // final clamp) is exactly the bug this fixes.
    let mut scale = 1.0f32;
    for &c in &rgb {
        let headroom = if delta > 0.0 { 1.0 - c } else { c };
        scale = scale.min(headroom / delta.abs());
    }
    scale = scale.max(0.0);

    let applied = delta * scale;
    [rgb[0] + applied, rgb[1] + applied, rgb[2] + applied]
}
```

No other function's signature or call site changes — `apply_global_adjustments` continues to call `apply_parametric_tone` exactly as before.

## 5. Design — GPU (WGSL)

`gradeMath.js`'s `apply_global_adjustments` inlines the identical shared-scale computation in place of the current `c = c + vec3<f32>(delta_h + delta_s + delta_w + delta_b);` line:

```wgsl
// Parametric Tone
let luma_val = dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
let w_h = smoothstep_val(0.5, 1.0, luma_val);
let delta_h = (highlights / 100.0) * w_h * (1.0 - luma_val) * 0.6;
let w_s = smoothstep_val(0.5, 0.0, luma_val);
let delta_s = (shadows / 100.0) * w_s * luma_val * 0.6;
let w_w = smoothstep_val(0.7, 1.0, luma_val);
let delta_w = (whites / 100.0) * w_w * 0.35;
let w_b = smoothstep_val(0.3, 0.0, luma_val);
let delta_b = (blacks / 100.0) * w_b * 0.35;
let delta = delta_h + delta_s + delta_w + delta_b;

// RFC-0018: same shared-scale hue-preserving clamp as tone.rs's
// apply_parametric_tone -- see that function's own doc comment.
if (delta != 0.0) {
  let headroom = select(vec3<f32>(c.x, c.y, c.z), vec3<f32>(1.0 - c.x, 1.0 - c.y, 1.0 - c.z), delta > 0.0);
  let s = clamp(min(headroom.x, min(headroom.y, headroom.z)) / abs(delta), 0.0, 1.0);
  c = c + vec3<f32>(delta * s);
}
```

(WGSL's `select(f, t, cond)` takes the false-value first, matching the snippet above: `delta > 0.0` selects the `1.0 - c` branch, otherwise falls through to plain `c`.) No new uniform, binding, or bind group — this is purely a per-pixel math change inside the existing `fs_grade` entry point, same shape as RFC-0017's own GPU changes.

## 6. Testability

- **Exact hue preservation at a near-clip saturated highlight, the concrete regression case**: for a representative near-white-clip saturated color (e.g. `[0.95, 0.70, 0.50]`, matching §3.1's worked example) with a strong positive Highlights value, decode both the pre- and post-adjustment pixel's hue via this module's own `rgb_to_hsl`-equivalent (or check pairwise channel differences directly, which is what hue reduces to here) and assert they match within float tolerance — the direct test for the bug this RFC fixes, using the exact numbers derived by hand in §3.1.
- **Symmetric fix matches the hand-derived worked example exactly**: `apply_parametric_tone([0.95, 0.70, 0.50], 100.0, 0.0, 0.0, 0.0)` should land at `[1.0, 0.75, 0.55]` (§3.1's derived result, checked numerically by hand, not just argued) within float tolerance — an exact-value test, not just a qualitative "still roughly orange" check.
- **No channel ever exceeds `[0,1]` from this op's own contribution alone**: for a spread of near-extreme starting pixels (near-white, near-black, saturated near each) crossed with the full range of each of the four sliders, `apply_parametric_tone`'s own output stays within `[0,1]` for every channel whenever the *input* was already within `[0,1]` — the direct proof that this op's own delta no longer needs the pipeline's downstream final clamp to stay in range.
- **Existing qualitative tests preserved unchanged**: `parametric_tone_highlights_boost_bright_pixels`/`parametric_tone_shadows_lift_dark_pixels` use gray, non-near-clip pixels (`[0.8,0.8,0.8]`/`[0.2,0.2,0.2]`) where headroom always exceeds the delta magnitude — `scale` stays exactly `1.0` for these, so both tests' existing assertions and exact behavior are unaffected; verified by running them, not just argued from the math.
- **`delta == 0` short-circuit is exact, not just close**: all four sliders at `0` returns the input pixel completely unchanged (bit-identical floats), confirming the early-return path is exercised and correct, not merely that the scaled math happens to round to the same value.
- **CPU/GPU parity**: extend `develop-cpu-gpu-parity.e2e.js` with a Highlights-at-near-clip scenario (a saturated, near-white-clip color with a strong Highlights value) — the first Parametric Tone-specific parity scenario, since RFC-0017's own combined scenario only covered Exposure/WB/Contrast.

## 7. Exit criteria (this slice)

- `apply_parametric_tone` (Rust) computes the shared, symmetric scale factor before combining the four window deltas, per §4; the WGSL twin in `gradeMath.js` mirrors it exactly, per §5.
- The near-clip hue-preservation test from §6 passes with an exact, hand-derived expected value (not a qualitative "looks about right" assertion).
- Existing qualitative Parametric Tone tests (`parametric_tone_highlights_boost_bright_pixels`, `parametric_tone_shadows_lift_dark_pixels`) still pass unchanged, confirming the fix is additive and doesn't alter ordinary (non-near-clip) behavior.
- CPU/GPU parity harness extended with a Parametric-Tone-at-near-clip scenario and passing.
- PROGRESS.md gets an M5.6 entry naming this as the slice that closes the concern RFC-0017 §2 explicitly deferred, with the worked-example numbers from §3.1/§6 as the concrete before/after.
