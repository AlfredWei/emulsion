# RFC-0015: Guided-filter blur for Sharpening (M5.6's fifth effect)

- Status: Draft — for review in PR (flips to Accepted once merged)
- Date: 2026-09-28
- Companion documents: [RFC-0010](RFC-0010-clarity-guided-filter.md), [RFC-0011](RFC-0011-dehaze-guided-refinement.md), [RFC-0012](RFC-0012-luma-nr-guided-filter.md), [RFC-0014](RFC-0014-color-nr-guided-filter.md), [MILESTONES §M5.6](../../PRD/MILESTONES.md#m56--develop-effects-quality--performance-research), [ADR-0004](../adr/ADR-0004-rendering-and-color-management.md), [develop_engine/detail.rs](../../app/src-tauri/src/develop_engine/detail.rs), [gpu/shaders/detailFilters.js](../../app/src/lib/gpu/shaders/detailFilters.js), [PROGRESS.md](../../PROGRESS.md)

## 0. Process note

Same discipline as every prior M5.6 RFC: scoped to one effect. Unlike RFC-0010/0012/0014, whose target ops had NO spatial edge-awareness at all before their fix, Sharpening already has one — `local_gradient_magnitude`-based `mask_weight`, explicitly named in RFC-0012 §2 as the reason Sharpen wasn't touched there: "it already has a genuine spatial edge-protection mechanism... worth its own research pass, not assumed to need the identical treatment." This RFC is that research pass. Its finding: **Masking does not actually protect against the artifact a guided filter fixes** — it solves a different, unrelated problem (flat-region noise suppression), so this is not "already good, no change" the way M5.6's own exit criteria allows for an effect with no gap; it is a real, distinct gap, addressed the same way Clarity's was.

## 1. Problem

Sharpening's blur source is a plain box mean of luma (`separable_mean_filter(&graded_luma, w, h, sharpen_radius_px(radius))`, WGSL twin `fs_sharpen_h`/`fs_sharpen_v` in `detailFilters.js`), then `sharpen_delta` reconstructs `diff = l - blurred_luma`, scaled by two independent gates (`detail_weight`, `mask_weight`) and added back to all three channels — classic unsharp masking. This is the *identical* computational shape Clarity's `apply_local_contrast` had before RFC-0010 (`delta = (luma - blurred) * factor`, additive around luma) — Sharpening is unsharp masking at a small, user-controlled radius; Clarity is the same operation at a large, fixed one (`CLARITY_RADIUS = 24`). Unsharp masking's own halo artifact (a bright/dark fringe on either side of a strong edge, worse at larger radii but present at any radius > 0) is exactly RFC-0010 §1's finding, restated for this op: a plain box mean mixes signal across the real edge, so `diff` at pixels *near* (not just on) the edge is large not because of real local texture but because the blur is dragging in the other side's value.

**Does `mask_weight` already prevent this?** No — and this is the concrete finding this RFC adds, not previously stated anywhere in the code. `mask_weight` gates on `local_gradient_magnitude`, a genuine, blur-independent spatial edge detector (a 4-neighbor central difference computed directly from `graded_luma`, `detail.rs`'s own doc comment: "is this pixel NEAR an edge," distinct from `detail_weight`'s "is THIS pixel's diff-from-blur amplitude large"). Its real, documented purpose (matching real Lightroom's own Masking slider) is to **protect flat/noisy regions from being sharpened at all** — at high Masking, sharpening is *confined* to pixels with a large local gradient, i.e., pixels at or near a real edge. That is the opposite of what would be needed to suppress a halo: it *concentrates* sharpening's own delta exactly where the box mean's cross-edge contamination is largest, rather than reducing that contamination. At low Masking, the same distortion applies more broadly (little spatial gating) rather than being fixed at its source. Either way, `mask_weight` changes *where* the (already-inaccurate) delta gets applied, never *how accurate* the delta itself is — the two gates in `sharpen_delta` do not, and structurally cannot, correct the blur they both read from.

## 2. Non-goals

- **`sharpen_delta`'s reconstruction is completely unchanged**: `detail_weight`, `mask_weight`, `SHARPEN_STRENGTH`, and the additive-around-luma delta shape all stay exactly as they are. Only the function producing `blurred_luma` changes, same scope discipline as every prior M5.6 slice.
- **`sharpen_radius_px`'s slider mapping is unchanged.** Sharpening's Radius stays a genuine, user-controlled, runtime value (unlike every other radius in this module) — see §4/§5 for what that implies for this slice specifically, a genuinely new wrinkle none of RFC-0010/0011/0012/0014 had to handle.
- **No new user-facing control.** Sharpening keeps its existing Amount/Radius/Detail/Masking sliders.
- **Every other effect** (white balance/auto-tone, tone curve, HSL/color mixer, split toning, grain, vignette, lens corrections) remains "not yet researched," same standing note every M5.6 RFC carries.

## 3. Research finding

### 3.1 Which guided filter

Sharpening blurs `graded_luma` using `graded_luma` itself as the only available reference signal — the guide and the input being filtered are the same signal, exactly Clarity's and Luma NR's own specialization (RFC-0010, RFC-0012), not Dehaze's or Color NR's two-signal case. **No new CPU primitive is needed**: this slice reuses `guided_filter_self` (RFC-0010) directly, the same way RFC-0012 did for Luma NR.

### 3.2 The genuinely new wrinkle: a runtime, not compile-time, radius

Every guided filter shipped so far (Clarity, Dehaze, Luma NR, Color NR) uses a FIXED radius — a WGSL compile-time `const`, baked into each box-filter pass's `for` loop bound at shader-authoring time. Sharpening's radius is different by original design (`sharpen_radius_px`'s own doc comment: "a genuine USER slider, not a compile-time const the way every other radius in this shader is... a deliberate, new precedent") — `fs_sharpen_h`/`fs_sharpen_v` already loop from a *runtime* value, `sharpenRadiusPx(sharpenParams.radius)`, read from the `sharpenParams` uniform buffer each pass.

This is mechanically fine — WGSL permits a runtime loop bound, and this shader already proves it works for exactly this op — but it means **every** one of this guided filter's box-filter passes (not just today's one H/V pair) needs the `sharpenParams` uniform bound and needs to call `sharpenRadiusPx` for its own loop, where Clarity/Luma NR/Color NR's own guided-filter passes each hard-code their fixed radius directly. A secondary, real consequence: `eps` must behave reasonably across the **entire runtime range** `sharpen_radius_px` can produce (1 to `SHARPEN_MAX_RADIUS_PX = 8`), not just at one fixed working point the way `CLARITY_GUIDED_EPS`/`LUMA_NR_GUIDED_EPS`/`DEHAZE_GUIDED_EPS`/`COLOR_NR_GUIDED_EPS` each only had to satisfy at their own single fixed radius. §6 turns this into an explicit multi-radius test, not an assumption.

## 4. Design — CPU (Rust)

No new primitive. `pipeline.rs`'s Sharpen blur call:

```rust
let sharpen_blur = if sharpen.amount != 0.0 {
    Some(separable_mean_filter(&graded_luma, w, h, sharpen_radius_px(sharpen.radius)))
} else {
    None
};
```

becomes:

```rust
let sharpen_blur = if sharpen.amount != 0.0 {
    // RFC-0015: guided_filter_self, not a plain box mean -- self-guided,
    // same specialization Clarity's own apply_clarity (RFC-0010) and Luma
    // NR (RFC-0012) use, since the guide and the thing being filtered are
    // both graded_luma here too. Unlike those two, radius is a genuine
    // runtime value (sharpen_radius_px(sharpen.radius)), not a fixed
    // constant -- guided_filter_self already takes radius as a plain i32
    // parameter, so this is a direct drop-in on the CPU side; see
    // SHARPEN_GUIDED_EPS's own doc comment for why the GPU side isn't as
    // simple.
    Some(guided_filter_self(&graded_luma, w, h, sharpen_radius_px(sharpen.radius), SHARPEN_GUIDED_EPS))
} else {
    None
};
```

`sharpen_delta` itself is untouched — it already receives `blurred_luma: f32` as an opaque input.

`SHARPEN_GUIDED_EPS` is a new fixed constant in `detail.rs`, next to `CLARITY_GUIDED_EPS`/`LUMA_NR_GUIDED_EPS`/`DEHAZE_GUIDED_EPS`/`COLOR_NR_GUIDED_EPS`. Sharpening *enhances* existing high-frequency detail rather than denoising — the same category `CLARITY_GUIDED_EPS` (0.01) was tuned for, not the smaller "denoise, don't enhance" values (`LUMA_NR_GUIDED_EPS`/`COLOR_NR_GUIDED_EPS`, 0.0009/0.0001) — but Sharpening's own radius (1-8px) sits far below Clarity's fixed 24px, closer to Luma NR's 3px/Color NR's 4px range where the smaller values were needed. Given RFC-0014's own correction (a same-category guess measurably underperforming once actually tested), this RFC does **not** assume either precedent value carries over — §6's multi-radius tests are what settle this empirically, with a starting search range spanning both (`0.0001`-`0.01`), not a single guess frozen into this document.

## 5. Design — GPU (WGSL)

`detailFilters.js`'s `fs_sharpen_h`/`fs_sharpen_v` pair becomes the same 11-pass self-guided shape RFC-0010/0012 already established (`meanp`, `corrp`, `a`, `b`, `meana`, `meanb`, each H+V where it's a box filter, plus the final compose) — **with one difference from every prior instance of this pattern**: every one of these 11 passes needs `sharpenParams` (binding 19, already declared) bound into its own bind group, and every H/V pass's loop bound is `sharpenRadiusPx(sharpenParams.radius)`, computed the same way today's `fs_sharpen_h`/`_v` already do, rather than a bare compile-time constant. `fs_sharpen_a`/`fs_sharpen_b` (the two no-blur compose passes) need no radius at all, matching Clarity's/Luma NR's own `_a`/`_b` passes.

New persistent single-channel (`r32float`) intermediates, six of them (same count as Clarity's/Luma NR's own six), at the next free binding range following Color NR's own 49-56 (i.e., starting at 57) — final numbers to be confirmed against whatever else has landed on `main` by implementation time, per this codebase's own "confirm the real next free binding, don't assume" discipline. `sharpenBlurHTex` (already existing) is reused as the shared H-scratch across all four box-filter pairs, same rebinding trick every guided-filter slice already uses; `sharpenBlurTex` (already existing) stays the final output, written by the new final compose pass instead of the old `fs_sharpen_v` — the SAME binding `premask.js`'s own `sharpen_delta` port already reads, so that downstream consumer needs no change, matching every prior slice's own precedent.

## 6. Testability

Same shape as every prior M5.6 RFC's §6, plus the multi-radius check §3.2 calls for:

- **Rust unit tests**:
  - No new primitive-level test needed for `guided_filter_self` itself (already proven generically by RFC-0010's own tests) — but a NEW test confirming it behaves reasonably (less step-edge overshoot than `separable_mean_filter` at the same radius) at **multiple** radii spanning `sharpen_radius_px`'s real range (e.g., 1, 4, 8), not just one, since this is the first guided-filter slice whose radius varies at runtime. This is also where `SHARPEN_GUIDED_EPS`'s actual value gets settled empirically across that range, per §4 — not assumed to be reasonable at every radius just because it works at one.
  - A flat-noisy-region test (mirroring RFC-0012/0014's own), at a representative mid-range radius, confirming the guided blur doesn't regress flat-region behavior relative to the box mean baseline.
  - An end-to-end test through `sharpen_delta`/`apply_edit_stack`, at a real edge, specifically **at a high Masking value** — the setting §1 argues is most exposed (sharpening concentrated exactly where the box mean's own cross-edge error is largest) — confirming less haloing with the guided-filter blur than the old box-mean version there.
- **CPU/GPU parity**: extend `develop-cpu-gpu-parity.e2e.js` with a Sharpening-at-a-real-edge scenario, at more than one Radius setting given §3.2's own multi-radius concern — the same still-open "flat-patch fixtures validate nothing about an edge-aware change" gap every prior slice flagged, now compounded by radius being a real variable to check across.
- **Performance**: extend `develop-performance.e2e.js` with a Sharpening-at-amount scenario, at both a small and a large Radius setting (this filter's own pass count is fixed at 11 regardless of radius, but each pass's *cost* scales with radius the same way `separable_mean_filter`'s always has) — treat this with the same seriousness every prior slice's own pass-count/radius risk got, not as a formality.
- **Visual**: a real photo with a strong edge, before/after at a meaningful Amount and, separately, at a high Masking value specifically (§1's own claim), checking for reduced haloing with comparable edge-enhancement strength elsewhere in the frame.

## 7. Exit criteria (this slice)

- Sharpening's CPU blur source switched from `separable_mean_filter` to `guided_filter_self` with a tuned `SHARPEN_GUIDED_EPS`, confirmed reasonable across `sharpen_radius_px`'s real runtime range (not just one radius); `sharpen_delta`'s reconstruction provably untouched.
- WGSL GPU path switched to the equivalent 11-pass sequence (§5), with every pass correctly reading the runtime radius from `sharpenParams` rather than a compile-time constant.
- CPU/GPU parity harness passing at an established, justified tolerance, exercised at more than one Radius setting.
- Real measured render latency for a Sharpening-at-amount scenario, reported and confirmed under the ~100ms budget at both a small and large Radius setting.
- A documented before/after comparison on a real image with a strong edge, specifically including a high-Masking scenario, showing reduced haloing with comparable enhancement strength.
- PROGRESS.md gets an M5.6 entry for this slice, explicitly recording that Masking's existing mechanism was checked and found NOT to already solve this gap (§1) — the same "why this wasn't already fine" question RFC-0012 §0 left open for this specific op, now answered.
