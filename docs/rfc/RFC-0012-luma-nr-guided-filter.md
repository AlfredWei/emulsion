# RFC-0012: Guided-filter blur for Luminance Noise Reduction (M5.6's third effect)

- Status: Draft — for review in PR (flips to Accepted once merged)
- Date: 2026-09-24
- Companion documents: [RFC-0010](RFC-0010-clarity-guided-filter.md), [RFC-0011](RFC-0011-dehaze-guided-refinement.md), [MILESTONES §M5.6](../../PRD/MILESTONES.md#m56--develop-effects-quality--performance-research), [ADR-0004](../adr/ADR-0004-rendering-and-color-management.md), [develop_engine/detail.rs](../../app/src-tauri/src/develop_engine/detail.rs), [gpu/shaders/detailFilters.js](../../app/src/lib/gpu/shaders/detailFilters.js), [PROGRESS.md](../../PROGRESS.md)

## 0. Process note

Same discipline as RFC-0010 §0 and RFC-0011 §0: scoped to one effect, not the whole M5.6 milestone. Color Noise Reduction (`color_nr_delta`, right next to Luma NR in both `detail.rs` and the pipeline) has the identical box-mean gap and is the natural next candidate after this one, but its per-channel chroma-delta reconstruction relies on an algebraic cancellation property (see `color_nr_delta`'s own doc comment: "summing `chroma_delta[c] * weights[c]` telescopes to zero by construction") that was derived specifically for a *linear* box-mean blur. Whether that cancellation still holds when each channel is blurred by a guided filter instead needs its own derivation and its own RFC, not an assumption carried over from this one — left as an explicitly open question for the next slice, not silently folded in here.

## 1. Problem

Luminance Noise Reduction's blur source is a plain box mean, and — unlike Clarity/Dehaze before their fixes — the code doesn't yet name this as a limitation anywhere, so this RFC states the gap explicitly rather than quoting an existing comment:

`pipeline.rs`: `let luma_nr_blur = ... Some(separable_mean_filter(&graded_luma, w, h, LUMA_NR_RADIUS))`, and the WGSL twin, `fs_lumaNR_h`/`fs_lumaNR_v` in `detailFilters.js`.

`luma_nr_delta`'s own reconstruction already has an edge-protection mechanism — `smooth_weight = 1 - smoothstep(0, edge_threshold, diff.abs())`, gating how much of the smoothing delta gets applied based on how far *this pixel's own value* is from the box-blurred value. But that gate only decides whether to use the blur result, not what the blur result itself is. The blur itself has no spatial awareness of the scene's real edges: every pixel within `LUMA_NR_RADIUS` of a genuine edge — not just pixels sitting exactly on it — gets a blurred value that already mixes signal across that edge, before the amplitude gate ever runs. A flat-noise pixel a few pixels away from a strong edge (a shadow boundary, a skin-to-background transition) gets a box mean pulled toward the edge's own value, and because Detail's gate is amplitude-only, it can't tell that contamination apart from real local variance — reducing Amount's effective denoising there, or (at low Detail, where the gate is looser) partially leaking the edge itself into flat regions on both sides.

This is exactly the canonical application guided filtering's own source paper opens with: He, Sun, Tang, *"Guided Image Filtering"* (ECCV 2010/TPAMI 2013) §1 lists noise reduction as one of the filter's motivating uses, and it is the same self-guided case Clarity already uses (guidance and input are the same signal — `graded_luma` — unlike Dehaze's two-signal case), so this slice reuses RFC-0010's own `guided_filter_self` directly; no new CPU primitive is needed.

## 2. Non-goals

- **Every other part of Luminance NR stays exactly as it is.** `luma_nr_delta`'s reconstruction formula — the amplitude gate (`smooth_weight`), Detail's threshold shift, and Contrast's post-smoothing restoration (`contrast_restore`) — is unchanged. Only the blur that produces `blurred_luma` changes, same scope discipline as RFC-0010/0011 (only the box-mean call site is replaced, nothing downstream of it).
- **Color Noise Reduction is not touched here** — flagged in §0 as the natural next slice, with its own open algebraic question.
- **Sharpen is not touched here.** Its blur has the same box-mean gap, but it already has a genuine spatial edge-protection mechanism (`local_gradient_magnitude`-based `mask_weight`, distinct from Detail's amplitude gate) that Clarity/Dehaze/Luma NR didn't have before their fixes — worth its own research pass, not assumed to need the identical treatment.
- **No new user-facing control.** Luminance NR keeps its existing Amount/Detail/Contrast sliders; `LUMA_NR_RADIUS` stays a fixed, non-user-exposed constant, same as before.

## 3. Design — CPU (Rust)

No new primitive. `pipeline.rs`'s Luma NR blur call:

```rust
Some(separable_mean_filter(&graded_luma, w, h, LUMA_NR_RADIUS))
```

becomes:

```rust
Some(guided_filter_self(&graded_luma, w, h, LUMA_NR_RADIUS, LUMA_NR_GUIDED_EPS))
```

`LUMA_NR_GUIDED_EPS` is a new fixed constant in `detail.rs`, next to `CLARITY_GUIDED_EPS`, same "fix the knob" discipline. It should sit closer to zero than `CLARITY_GUIDED_EPS`: Clarity's `eps` was tuned as a *contrast-enhancement* working point (He, Sun, Tang's own §5.4 example, where `a` staying away from 0 even in fairly flat regions is part of the desired local-contrast boost), but Luminance NR's blur exists purely to estimate "what this pixel would be without noise" — it should behave like a near-plain box mean in genuinely flat/noisy regions (small `var_guide`, `a -> 0`, `mean_b -> mean_p`, matching today's box-mean baseline almost exactly there) and back off sharply only where `var_guide` is large enough to be a real edge rather than noise variance. A smaller `eps` than Clarity's makes the transition between those two regimes happen at a lower variance threshold, appropriate for an op whose whole job is operating in the noise-variance regime. The exact value needs the same empirical tuning against this module's own tests that `CLARITY_GUIDED_EPS`/`DEHAZE_GUIDED_EPS` got, not a value frozen into this RFC — `(0.02)²`–`(0.04)²` is a reasonable starting search range (between `DEHAZE_GUIDED_EPS`'s `0.0001` and `CLARITY_GUIDED_EPS`'s `0.01`), to be settled by the unit tests in §5.

## 4. Design — GPU (WGSL)

`detailFilters.js`'s `fs_lumaNR_h`/`fs_lumaNR_v` pair (today: box-means `luma(gradedTex)` at `LUMA_NR_RADIUS`, writing to `lumaNrBlurFinal`, which `premask.js`'s `luma_nr_delta` port already reads) becomes the same 11-pass shape RFC-0010 §5 already established for Clarity's self-guided case (four box-filter pairs, two cheap per-pixel compose passes, reusing the identical rebinding-scratch pattern this codebase now has three precedents for):

1. `fs_lumaNR_meanp_h`/`_v` — box mean of `luma(gradedTex)` at `LUMA_NR_RADIUS` (this is today's `fs_lumaNR_h`/`_v`, renamed for the new pass sequence — same computation, same output).
2. `fs_lumaNR_corrp_h`/`_v` — box mean of `luma(gradedTex)^2`.
3. `fs_lumaNR_a` — one per-pixel pass: `a = (corr_p - mean_p*mean_p) / (corr_p - mean_p*mean_p + eps)`.
4. `fs_lumaNR_b` — one per-pixel pass: `b = mean_p - a*mean_p`.
5. `fs_lumaNR_meana_h`/`_v`, `fs_lumaNR_meanb_h`/`_v` — box mean of `a` and `b`.
6. Final (replaces today's `fs_lumaNR_v`'s role as the thing bound to `lumaNrBlurFinal`): `q = mean_a*luma(gradedTex) + mean_b`.

That's 11 new passes against today's 2 — the same jump RFC-0010 made for Clarity, and for the identical reason (self-guided general algorithm, four box-filter pairs collapsing to two distinct quantities instead of Dehaze's four). Six new persistent single-channel (`r32float`) intermediates, new bindings following the existing sequence (Dehaze's slice ended at binding 42; this slice's new textures/uniforms start at 43), wired through `pipelines.js`, `gpuHandles.js`, `sourceTexture.js`, `renderFrame.js`, `DevelopCanvas.svelte`, `shaders/index.test.js` — same five-file wiring checklist both prior slices used, and the same "grep for backticks in any new WGSL comment before committing" check RFC-0010's own postmortem established.

At `LUMA_NR_RADIUS=3` (7×7 window), this is a smaller radius than either prior slice (Clarity's 24, Dehaze's 4) — closer to Dehaze's own small-radius performance risk than to Clarity's, so §6 calls this out as the same class of open question RFC-0011 §5 named rather than assuming radius 3 makes 11 small passes free.

## 5. Testability

Same shape as RFC-0010 §6 / RFC-0011 §6:

- **Rust unit tests**: no new primitive to test (reuses `guided_filter_self` as-is), but new tests at the call site — `pipeline.rs`'s Luma NR block, or a small end-to-end helper, should verify that a synthetic noisy-flat region gets smoothed close to the box-mean baseline (small `var_guide` everywhere, `LUMA_NR_GUIDED_EPS` chosen so `a` stays near 0 there) while a synthetic step edge shows the blur source itself preserving more of the edge than `separable_mean_filter` does at the same radius, mirroring RFC-0010's own step-edge test shape. This is also where `LUMA_NR_GUIDED_EPS`'s actual value gets settled empirically, per §3.
- **CPU/GPU parity**: extend `develop-cpu-gpu-parity.e2e.js` with a Luma NR scenario at a patch containing real edge content — the same still-open gap both prior slices flagged (the existing flat-patch fixtures validate nothing about an edge-aware change specifically) applies identically here.
- **Performance**: extend `develop-performance.e2e.js` with a Luma-NR-at-amount scenario; given §4's own small-radius flag, treat this the same way RFC-0011 §5/§7 treated Dehaze's 15-pass risk, not as a formality.
- **Visual**: a real noisy photo with both a flat sky/wall-type region and a clear foreground edge, before/after at a meaningful Amount, checking for less edge-adjacent softening/haloing at the edge compared to today's box-mean version, while flat-region denoising strength looks comparable to today's baseline.

## 6. Exit criteria (this slice)

- Luma NR's CPU blur source switched from `separable_mean_filter` to `guided_filter_self` with a tuned `LUMA_NR_GUIDED_EPS`; every other part of `luma_nr_delta`'s reconstruction (§2) provably untouched.
- WGSL GPU path switched to the equivalent 11-pass sequence (§4).
- CPU/GPU parity harness passing at an established, justified tolerance, exercised at a real-edge-content patch.
- Real measured render latency for a Luma-NR-at-amount scenario, reported and confirmed under the ~100ms budget, given §4's own small-radius risk flag.
- A documented before/after comparison on a real noisy photo with both flat and edge content, showing preserved edges with comparable flat-region denoising strength.
- PROGRESS.md gets an M5.6 entry for this slice, same shape as RFC-0010/0011's own.
