# RFC-0016: Vignette's Roundness parameter (M5.6's sixth effect)

- Status: Draft — for review in PR (flips to Accepted once merged)
- Date: 2026-09-29
- Companion documents: [RFC-0010](RFC-0010-clarity-guided-filter.md), [RFC-0011](RFC-0011-dehaze-guided-refinement.md), [RFC-0012](RFC-0012-luma-nr-guided-filter.md), [RFC-0014](RFC-0014-color-nr-guided-filter.md), [RFC-0015](RFC-0015-sharpen-guided-filter.md), [MILESTONES §M5.6](../../PRD/MILESTONES.md#m56--develop-effects-quality--performance-research), [develop_engine/effects.rs](../../app/src-tauri/src/develop_engine/effects.rs), [develop_engine/pipeline.rs](../../app/src-tauri/src/develop_engine/pipeline.rs), [gpu/shaders/gradeUniforms.js](../../app/src/lib/gpu/shaders/gradeUniforms.js), [gpu/shaders/premask.js](../../app/src/lib/gpu/shaders/premask.js), [api/develop.js](../../app/src/lib/api/develop.js), [PROGRESS.md](../../PROGRESS.md)

## 0. Process note

Every prior M5.6 slice (RFC-0010/0011/0012/0014/0015) fixed a **quality gap in an existing blur-based op** — a plain box mean standing in for an edge-aware guided filter. This slice is different in kind, not just degree: Vignette's gap isn't a blur-quality issue at all (Vignette has no neighbor-pixel read whatsoever — it's a pure per-pixel radial falloff, per its own doc comment in `effects.rs`). The gap is that **an entire named parameter is missing**: `effects.rs`'s own doc comment on `vignette_factor` states plainly, "real Lightroom's Roundness slider (blending toward a more rectangular shape) isn't implemented; every vignette here is roundness=0's natural ellipse," and `pipeline.rs:119` cites this same gap as a "named, accepted limitation" in the same breath as Dehaze's own approximations. This RFC is the first M5.6 research pass whose job is to actually **design** a missing parameter from scratch, rather than replace one already-present computation with a better one. `PROGRESS.md`'s own Slice 5 closing note ("None of these are neighbor-blur ops... the next research pass needs its own fresh read of the literature per effect, not an assumed guided-filter candidate") anticipated exactly this: no guided filter is involved here at all.

A second consequence of this being a genuinely new parameter: unlike every prior slice, this one requires a **new user-facing control** (`DevelopPanel.svelte` currently has no Roundness slider for Vignette at all — confirmed by inspection, not assumed).

## 1. Problem

`vignette_factor` (`effects.rs`) computes a falloff shape via a single, fixed norm:

```rust
let dx = (uv.0 - 0.5) * 2.0;
let dy = (uv.1 - 0.5) * 2.0 * aspect;
let corner_dist = (1.0f32 + aspect * aspect).sqrt();
let norm_dist = (dx * dx + dy * dy).sqrt() / corner_dist;
```

This is the Euclidean (L2) norm of an aspect-corrected offset, normalized so the image's corner always sits at `norm_dist = 1.0` regardless of aspect ratio — the level sets of `norm_dist` are aspect-corrected ellipses (matching the image's own aspect ratio, not squashed to it), and there is no way to reach any other shape. Real Lightroom's Vignette panel has a fourth slider, Roundness, absent here entirely: no struct field, no WGSL uniform field in active use (`_pad0` is unused padding), no UI control. `norm_dist`'s formula has exactly one shape, permanently.

## 2. Non-goals

- **Pixel-exact reproduction of Adobe's internal Roundness algorithm.** Adobe's implementation isn't public, and this codebase has no reference build to diff against (unlike the guided-filter slices, which could compare against a known, provable mathematical property — e.g. RFC-0010's edge-preservation proof). §3 states this uncertainty explicitly rather than presenting a guess as a verified match; this mirrors the existing precedent this codebase already has for the NR Contrast slider, a **deliberate, named reinterpretation** of undocumented Lightroom behavior rather than a false claim of exact parity.
- **`midpoint`/`feather`'s existing semantics.** Both stay "normalized radius from center, in whatever norm is currently active" — unchanged in meaning, only reinterpreted through a generalized `norm_dist`.
- **The post-crop Vignette's relationship to the separate, unrelated lens-correction vignette** (`lens.js`'s `vignette_amount`/`vignette_k1/k2/k3`, a polynomial lens-profile correction) — no connection between the two, and none is introduced here.
- **Every other not-yet-researched M5.6 effect** (white balance/auto-tone, tone curve, HSL/color mixer, split toning, grain, lens corrections) remains untouched, same standing note every M5.6 RFC carries.

## 3. Research finding

### 3.1 The technique: generalizing the L2 norm to a variable-exponent (superellipse / Lᵖ) norm

Other open-source raw processors that expose an adjustable vignette shape (darktable's and RawTherapee's vignette modules, among others) use the same underlying idea this RFC adopts: generalize the fixed Euclidean distance to a **superellipse** (Lamé curve) family, `|x|ᵖ + |y|ᵖ = rᵖ`, where `p = 2` is exactly today's ellipse, `p → ∞` approaches a rectangle, and `p < 2` moves toward a diamond/star shape. This is a well-known, well-behaved generalization (convex and smooth for any `p ≥ 1`), and it slots into `norm_dist`'s existing structure with no change to what `norm_dist` *means* (0 at center, 1 at the image corner) — only to the shape of its level sets.

### 3.2 The asymmetry this codebase's own wording implies, and why one exponent isn't enough

`effects.rs`'s own doc comment describes Roundness as "blending toward a **more rectangular** shape" — describing only one direction. That phrasing is consistent with a *p > 2* blend (toward rectangle). But a single exponent knob can't also produce something *rounder than today's ellipse*, because **`p = 2` is already the roundest shape the Lᵖ family can produce** — moving `p` below 2 does not make the shape rounder, it makes it *pointier* (a diamond at `p = 1`). If Roundness is meant to be a two-sided slider (as real Lightroom's -100..100 range suggests), its "rounder" half needs a mechanism that isn't "change `p`."

The mechanism chosen for that half is **blending the aspect correction itself toward 1.0** — i.e., toward a shape that ignores the image's own aspect ratio and becomes a true circle. This is the natural meaning of "rounder than an aspect-corrected ellipse": an aspect-corrected ellipse is already elongated to match a non-square frame, so making it *rounder* means making it *less* elongated, independent of `p`. This gives one parameter, two distinct mechanisms, meeting exactly at today's ellipse:

- **`roundness = 0`**: today's exact formula, byte-for-byte (see §4 for why this is a hard identity, not just "close").
- **`roundness > 0` ("rounder")**: blend the aspect factor applied to `dy` from `aspect` (at `roundness = 0`) toward `1.0` (at `roundness = 100`) — the shape becomes a true circle at the positive extreme, regardless of the image's own aspect ratio.
- **`roundness < 0` ("more rectangular")**: blend the exponent `p` from `2.0` (at `roundness = 0`) toward a fixed `VIGNETTE_ROUNDNESS_MAX_P` (at `roundness = -100`) — the shape becomes a rounded superellipse closer to the frame's own rectangle.

**Honesty caveat, stated plainly**: the sign convention (positive = rounder, negative = more rectangular) and the two-mechanism split are this RFC's own reasoned design, chosen because it is internally consistent (both halves meet exactly at the existing ellipse, and "rounder than an ellipse" is mathematically meaningful only via the aspect-blend route) and because it matches this codebase's own pre-existing doc-comment wording for the rectangular direction — **not** a verified match to Adobe's actual sign convention or algorithm, which this project has no way to confirm. `VIGNETTE_ROUNDNESS_MAX_P` is chosen for a visually smooth, clearly noticeable, non-degenerate transition (proposed: `5.0`) — there is no reference signal to tune it against the way `SHARPEN_GUIDED_EPS` etc. were tuned against a measurable error metric in prior slices, so §6's testability is necessarily about *provable shape properties* (identity at 0, corner normalization, monotonicity), not a numeric-accuracy check against ground truth.

## 4. Design — CPU (Rust)

`Vignette` gets a fourth field, matching real Lightroom's own -100..100 range:

```rust
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
```

`vignette_op`'s field parsing gets the matching `field("roundness", 0.0)` line, same "fall back to identity on partial/corrupt payload" contract every other field here already has.

`vignette_factor` branches on the sign of `roundness`, with the `roundness == 0.0` branch reusing the **exact original three lines**, unmodified — this is a deliberate implementation choice, not an optimization: it guarantees `roundness = 0` (the default, and every vignette ever rendered before this RFC) produces bit-for-bit identical output, with zero floating-point risk from routing the default through the new generalized `powf`-based math:

```rust
pub(super) fn vignette_factor(uv: (f32, f32), aspect: f32, v: &Vignette) -> f32 {
    if v.amount == 0.0 {
        return 1.0;
    }
    let r = v.roundness.clamp(-100.0, 100.0);
    let norm_dist = if r == 0.0 {
        // Untouched since before RFC-0016 -- guarantees roundness=0 is
        // bit-for-bit identical to every vignette rendered before this
        // RFC, not merely numerically close to it.
        let dx = (uv.0 - 0.5) * 2.0;
        let dy = (uv.1 - 0.5) * 2.0 * aspect;
        let corner_dist = (1.0f32 + aspect * aspect).sqrt();
        (dx * dx + dy * dy).sqrt() / corner_dist
    } else if r > 0.0 {
        // RFC-0016 "rounder": p=2 (today's ellipse) is already the
        // roundest shape the Lp family below can produce, so this side
        // can't come from changing p -- it blends the ASPECT CORRECTION
        // itself toward 1.0 (a true circle, ignoring the image's own
        // elongation) as r approaches +100. See RFC-0016 SS3.2.
        let eff_aspect = aspect + (1.0 - aspect) * (r / 100.0);
        let dx = (uv.0 - 0.5) * 2.0;
        let dy = (uv.1 - 0.5) * 2.0 * eff_aspect;
        let corner_dist = (1.0f32 + eff_aspect * eff_aspect).sqrt();
        (dx * dx + dy * dy).sqrt() / corner_dist
    } else {
        // RFC-0016 "more rectangular": generalizes the L2 norm to a
        // superellipse (Lp) norm, blending the exponent from 2.0 toward
        // VIGNETTE_ROUNDNESS_MAX_P as r approaches -100. See RFC-0016 SS3.1.
        let p = 2.0 + (-r / 100.0) * (VIGNETTE_ROUNDNESS_MAX_P - 2.0);
        let dx = (uv.0 - 0.5) * 2.0;
        let dy = (uv.1 - 0.5) * 2.0 * aspect;
        let corner_dist = (1.0f32 + aspect.powf(p)).powf(1.0 / p);
        (dx.abs().powf(p) + dy.abs().powf(p)).powf(1.0 / p) / corner_dist
    };
    let inner = (v.midpoint / 100.0).clamp(0.0, 0.999);
    let outer = (inner + (v.feather / 100.0).max(0.001) * (1.0 - inner)).clamp(inner + 0.001, 1.0);
    let t = smoothstep(inner, outer, norm_dist);
    1.0 + (v.amount / 100.0) * t
}
```

`VIGNETTE_ROUNDNESS_MAX_P: f32 = 5.0` is a new constant next to `vignette_factor`, with a doc comment carrying forward §3.2's honesty caveat verbatim (no reference signal exists to tune it against, unlike every guided-filter `eps`).

`midpoint`/`feather`'s own clamping and `smoothstep` call are completely unchanged — they still operate on `norm_dist`, whatever norm produced it.

## 5. Design — GPU (WGSL)

`gradeUniforms.js`'s `Vignette` struct repurposes its already-unused `_pad0` slot — **no layout change, no new binding, no other pass affected**:

```wgsl
struct Vignette {
  amount: f32,
  midpoint: f32,
  feather: f32,
  roundness: f32, // RFC-0016; was _pad0 (unused)
};
@group(0) @binding(15) var<uniform> vignette: Vignette;
```

`premask.js`'s `fs_premask` gets the same three-way branch as §4, replacing today's fixed two lines (`cornerDist`/`normDist`). WGSL's `pow()` is undefined for a negative base with a non-integer exponent (unlike Rust's `powf`, which handles it via `abs()` the same way this design already does explicitly), so the `abs()` calls in the `roundness < 0` branch are load-bearing, not stylistic:

```wgsl
let r = clamp(vignette.roundness, -100.0, 100.0);
var normDist: f32;
if (r == 0.0) {
  // Untouched -- bit-for-bit identical to every vignette before RFC-0016.
  let dx = centered.x;
  let dy = centered.y * vAspect;
  let cornerDist = sqrt(1.0 + vAspect * vAspect);
  normDist = sqrt(dx * dx + dy * dy) / cornerDist;
} else if (r > 0.0) {
  let effAspect = vAspect + (1.0 - vAspect) * (r / 100.0);
  let dx = centered.x;
  let dy = centered.y * effAspect;
  let cornerDist = sqrt(1.0 + effAspect * effAspect);
  normDist = sqrt(dx * dx + dy * dy) / cornerDist;
} else {
  let p = 2.0 + (-r / 100.0) * (VIGNETTE_ROUNDNESS_MAX_P - 2.0);
  let dx = centered.x;
  let dy = centered.y * vAspect;
  let cornerDist = pow(1.0 + pow(vAspect, p), 1.0 / p);
  normDist = pow(pow(abs(dx), p) + pow(abs(dy), p), 1.0 / p) / cornerDist;
}
```

(`centered`/`vAspect` are the pre-existing local variables from today's shader; only the `cornerDist`/`normDist` computation changes shape.) `VIGNETTE_ROUNDNESS_MAX_P` becomes a WGSL `const` mirroring the Rust constant, declared next to `DEHAZE_T0`-style existing constants in the same file.

This is a **single-pass, uniform-only change** — `fs_premask` already reads `vignette` every call; no new texture, no new bind group entry, no new pipeline, no readiness-guard change, and no entry in `shaders/index.test.js`'s `entryPoints` list (no new entry point). This slice has none of the six-file GPU wiring checklist every guided-filter slice needed.

### 5.1 JS/UI plumbing (mechanical, not research)

- `develop.js`: `IDENTITY_VIGNETTE` gains `roundness: 0`; `buildVignetteUniformData` writes `v.roundness` instead of the literal `0` in its fourth slot.
- `DevelopPanel.svelte`: a new Roundness slider (-100..100, default 0) alongside the existing Amount/Midpoint/Feather sliders — the first new user-facing Effects control any M5.6 slice has added.

## 6. Testability

No guided filter is involved, so there is no numeric-accuracy check against a measurable ground truth the way every prior slice had (`eps` tuned against a known error metric). Testability here is about **provable shape properties**:

- **Identity at default**: `roundness = 0.0` produces output identical to calling the pre-RFC-0016 formula directly, at several `(uv, aspect)` combinations including off-center points and a non-1.0 aspect ratio — this should be a hard equality given §4's branch reuses the exact original expression.
- **Corner normalization holds at both extremes**: at `uv` mapping to the image's literal corner, `norm_dist == 1.0` (within float tolerance) at `roundness = +100`, `roundness = -100`, and a mid-value on each side, for at least one non-1.0 aspect ratio — proving the "0 at center, 1 at corner" contract §3.1 claims is preserved is a real, checkable property, not an assumption.
- **`roundness = +100` reduces to a true circle**: for a non-1.0 aspect ratio, confirm `eff_aspect == 1.0` and that two points equidistant from center along the x-axis and the (unscaled) y-axis produce equal `norm_dist` — the defining property of a circle, distinguishing it from the default ellipse at the same points.
- **`roundness = -100` is monotonically "squarer" than default**: at a fixed off-axis point strictly inside the corner (not on an axis, not at the corner itself), confirm `norm_dist` at `p = VIGNETTE_ROUNDNESS_MAX_P` differs from `norm_dist` at `p = 2.0` in the direction consistent with a superellipse bulging toward the frame's rectangle (closer to the corner's own bounding box) — a qualitative-but-checkable directional property, not a numeric-accuracy claim.
- **`amount = 0` still short-circuits before any of this new geometry runs** — same fast-path/identity discipline every other slice's `amount = 0`/`strength = 0` check already has; this new branch structure must not run at all when Vignette is off.
- **CPU/GPU parity**: extend `develop-cpu-gpu-parity.e2e.js` with a Vignette-at-nonzero-roundness scenario on both sides of zero (one positive, one negative `roundness`), on a non-square test image (aspect ratio matters for every branch here).
- **Visual**: a real photo at a strong Amount, comparing `roundness = 0` (today's ellipse, unchanged), a strongly positive value (visibly more circular), and a strongly negative value (visibly flatter/more rectangular toward the frame edges).

## 7. Exit criteria (this slice)

- `Vignette` struct (Rust) and its WGSL twin both gain `roundness`, defaulting to `0.0`/reusing `_pad0`; `vignette_op`'s parsing updated to match every other field's fallback contract.
- `vignette_factor` (Rust) and `fs_premask`'s vignette block (WGSL) both implement the three-way branch in §4/§5, with the `roundness == 0.0` branch provably identical to the pre-RFC-0016 formula.
- `VIGNETTE_ROUNDNESS_MAX_P` defined once in Rust and mirrored once in WGSL, with its doc comment carrying the §3.2 honesty caveat (no reference signal to tune against) rather than presenting the chosen value as verified.
- `develop.js`'s `IDENTITY_VIGNETTE`/`buildVignetteUniformData` and a new Roundness slider in `DevelopPanel.svelte` — the first new Effects-panel control any M5.6 slice has added, not merely an internal quality change.
- The §6 shape-property tests (identity, corner normalization at both extremes, circle-at-+100, squarer-at--100 direction, amount=0 short-circuit) all passing; CPU/GPU parity extended for both signs of `roundness`.
- A documented before/after comparison on a real, non-square photo showing the three named shapes (default ellipse, rounder, more rectangular) are visually distinct and each internally consistent (falloff still centered, corner still reached at `norm_dist=1`).
- PROGRESS.md gets an M5.6 entry for this slice, explicitly recording that this is the first slice adding a genuinely new parameter/control rather than fixing an existing blur, and restating the §3.2 honesty caveat about Adobe's real sign convention being unverified.
