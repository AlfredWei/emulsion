# RFC-0010: Edge-aware local contrast for Clarity (M5.6's first effect)

- Status: Draft — for review in PR (flips to Accepted once merged)
- Date: 2026-09-23
- Companion documents: [MILESTONES §M5.6](../../PRD/MILESTONES.md#m56--develop-effects-quality--performance-research), [ADR-0004](../adr/ADR-0004-rendering-and-color-management.md), [develop_engine/detail.rs](../../app/src-tauri/src/develop_engine/detail.rs), [gpu/shaders/dehazeLocalContrast.js](../../app/src/lib/gpu/shaders/dehazeLocalContrast.js), [PROGRESS.md](../../PROGRESS.md)

## 0. Process note

M5.6's scope is a research pass across roughly a dozen independent Develop effects, not one coherent feature built in slices the way RFC-0007 was. So unlike that RFC, this one is scoped to a **single effect** (Clarity's local-contrast blur) rather than acting as an umbrella for the whole milestone. Each later M5.6 finding gets its own RFC (RFC-0011, …) when it reaches the same "concrete gap + implementable fix" bar this one does — a finding that's just "already good, no change" (per the milestone's own exit criteria) doesn't need one; it's recorded directly in PROGRESS.md instead.

## 1. Problem

M5.6 asks for "a per-effect research pass … comparing this project's current algorithm/mapping against published professional-grade references … to find concrete, specific quality gaps — not a vague 'make it better' pass" (MILESTONES.md). Texture & Clarity already has one on record, self-reported in its own implementation:

> `apply_local_contrast`'s doc comment (detail.rs): "`CLARITY_RADIUS=24` (coarse 'midtone contrast', **the classic halo-prone Clarity look at high amounts** — a plain box-mean blur stands in for a true edge-aware filter here, same named limitation as Dehaze's own transmission refinement)."

Concretely: both the Rust CPU path (`apply_local_contrast`, detail.rs:285) and its WGSL GPU twin (`fs_clarity_h`/`fs_clarity_v`, dehazeLocalContrast.js) compute `blurred = box_mean(luma, radius=24)` and then push each pixel toward `luma + (luma - blurred) * amount`. A plain box blur doesn't know where the image's real edges are, so at high Clarity amounts a strong edge (e.g. a dark silhouette against a bright sky) gets a visible bright/dark fringe on either side of it — the "halo" every unsharp-mask-style local-contrast tool produces at large radii, and exactly what real Lightroom's Clarity is known not to do nearly as badly, because Adobe's implementation is edge-aware, not a plain blur.

This is the concrete, specific gap M5.6 asks for — not speculative, already named in the code that has it.

## 2. Non-goals

- **Texture (`TEXTURE_RADIUS=6`) is unchanged.** Its own doc comment already gives the reason it's lower-risk: "small window so it doesn't touch big tonal transitions" — at a 6px radius there's much less low-frequency tonal structure for a plain blur to leak across in the first place, and there's no equivalent "classic halo-prone" callout for Texture the way there is for Clarity. Revisit only if a concrete case shows otherwise.
- **Dehaze's transmission-map refinement** (`DEHAZE_REFINE_RADIUS`, same box-mean-standing-in-for-a-true-edge-aware-filter limitation, named in both detail.rs and dehazeLocalContrast.js) is a real, related gap and — see §3 — the *same* fix applies to it almost directly. Deliberately **not** built here: bundling two effects into one slice makes this harder to review and re-scopes past what was just agreed as "one effect end-to-end first." Recorded so it isn't lost; a natural, low-effort follow-up once this slice's primitive exists and has proven itself in review/production.
- **No user-facing change.** Clarity's slider, its range, its default, and its amount-to-effect response stay exactly as they are — this is an internal quality change to the blur underneath it, not a UX change.
- **No change to any other effect** — white balance, tone, HSL, sharpening, noise reduction, grain, vignette, lens corrections all keep their current M5.6 status of "not yet researched."

## 3. Research finding

**Reference**: He, Sun, Tang, *"Guided Image Filtering,"* ECCV 2010 / IEEE TPAMI 2013 — an edge-preserving smoothing filter, computable in O(N) entirely from box filters (mean/sum passes), which is the standard drop-in replacement anywhere a blur is used as "smooth but don't cross real edges." Two things make it an unusually direct fit here, not just "a" published alternative among many:

- It's the **same lead author's own follow-up work** to He et al. 2009's dark-channel-prior dehaze — the algorithm this codebase already implements faithfully (`dehaze_atmospheric_light` and neighbors, detail.rs). This project is already in that paper's lineage, not adopting an unrelated technique.
- The guided-filter paper's own §5.4, *"detail enhancement,"* is structurally this exact use case: compute an edge-aware base layer via **self-guided filtering** (guidance image = input image), take `detail = input - base`, then boost: `output = input + amount * detail`. That is `apply_local_contrast`'s existing `delta = (luma - blurred) * factor; rgb += delta` shape, verbatim — the only change is what "blurred" means.
- Guided-filter refinement of a dark-channel-prior transmission map (exactly `DEHAZE_REFINE_RADIUS`'s job) is the standard combination in the dehaze literature that followed both papers — which is why §2 calls fixing Clarity's blur a near-direct win for Dehaze's named limitation too, once this exists.

### The algorithm (self-guided special case: guidance = input = luma)

The general guided filter takes a guidance image `I` and an input `p` to be filtered, windowed at radius `r` with a regularization `eps`. In the self-guided case (`I = p`), the cross terms simplify:

```
mean_p = boxfilter(p, r)
corr_p = boxfilter(p * p, r)
var_p  = corr_p - mean_p * mean_p
a      = var_p / (var_p + eps)
b      = mean_p - a * mean_p            // = mean_p * (1 - a)
mean_a = boxfilter(a, r)
mean_b = boxfilter(b, r)
q      = mean_a * p + mean_b
```

`q` replaces `blurred` in the existing `delta = (luma - blurred) * factor` formula — nothing else about the op changes (the additive-around-luma chroma-preserving reconstruction, the amount scaling, the exact-passthrough at `amount == 0`, all as documented in `apply_local_contrast`'s own comment today).

Why this fixes the halo: `a` is a per-pixel edge indicator computed from local variance. In a flat region, `var_p ≈ 0`, so `a ≈ 0` and `q ≈ mean_p` — a plain local average, same as today's box blur. Near a strong edge, `var_p` is large relative to `eps`, so `a ≈ 1` and `q ≈ p` — the filter backs off and returns something close to the original pixel, rather than blending across the edge the way an unweighted box mean does. `eps` is the only new knob, and it's exactly the thing a plain box mean has no equivalent of at all: it sets the variance scale at which the filter treats a region as "edge" vs. "flat."

Two boundary sanity checks worth stating up front (both become unit tests in §6 — the first implementation pass, corrected below, caught a real error in the second one):
- **Constant input**: `var_p = 0` everywhere → `a = 0`, `b = mean_p = p` → `q = p`. A flat image is returned unchanged (matches a box mean's own trivial behavior on a constant input).
- **`eps → 0`** (given `var_p > 0` at every pixel, i.e. no perfectly flat local window anywhere): `a → 1`, `b → 0` pointwise, hence `mean_a → 1`, `mean_b → 0`, giving `q → 1·p + 0 = p` — an exact identity, no smoothing at all, once there's no regularization telling the filter a region is "flat." **Correction**: an earlier draft of this RFC additionally claimed the opposite limit, `eps → ∞`, degrades exactly to `separable_mean_filter`'s own output. That's wrong, and a failing unit test caught it during implementation: `a → 0` and `b → mean_p` pointwise as claimed, but `mean_b` then box-filters `b` a *second* time, so the true `eps → ∞` limit is `boxfilter(boxfilter(p))` — a double box mean, not a single one. Dropped as a claim; the `eps → 0` identity above is the one boundary condition this RFC relies on, and it's the more directly useful one for a unit test besides.

## 4. Design — CPU (Rust)

New function in `detail.rs`, next to `separable_mean_filter` (which it's built entirely out of — no new blur primitive needed):

```rust
/// Self-guided image filter (He, Sun, Tang, ECCV 2010/TPAMI 2013 -- the
/// guidance image and the input being filtered are the same signal). See
/// this RFC's derivation for why: edge-aware generalization of
/// `separable_mean_filter`, degrading to it exactly as `eps -> infinity`.
/// Four `separable_mean_filter` calls (mean_p, corr_p, mean_a, mean_b) plus
/// two cheap per-pixel passes (a/b, then the final compose) -- no new
/// windowed-reduction primitive, reuses the existing O(1)-per-pixel
/// sliding-window implementation throughout.
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
```

`apply_local_contrast` (existing, box-mean) stays completely untouched and keeps serving Texture. A new, separate `apply_clarity` (or similar name, finalized during implementation) does the same additive-delta reconstruction but computes `blurred` via `guided_filter_self(luma, ..., CLARITY_RADIUS, CLARITY_GUIDED_EPS)` instead of `separable_mean_filter`. Two functions, not one function with a branch — Texture's code path and its cost are then provably unaffected by this change, which matters for a milestone whose own exit criteria is "no silent gaps" per effect: nothing about this slice should be able to silently perturb an effect it isn't about.

`CLARITY_GUIDED_EPS` is a fixed, named constant (not user-exposed) — same "fix the knob, expose only Amount" choice this module already made for every other internal parameter (Dehaze's four constants, Split Toning's transition width). Real Lightroom doesn't expose an eps-equivalent for Clarity either. Working value to be tuned empirically against the visual/quantitative tests in §6 (the guided-filter paper's own examples use `eps` on the order of `(0.1)²`–`(0.2)²` for normalized `[0,1]` inputs, luma here is already `[0,1]`-normalized, so that's the starting range, not a final answer).

`pipeline.rs`'s two call sites change from two calls to the same function to one call each to the (now differently-named) Texture and Clarity functions — a one-line change per call site, same shape as today.

## 5. Design — GPU (WGSL)

`dehazeLocalContrast.js`'s `fs_clarity_h`/`fs_clarity_v` pair (today: one box-mean pass pair, with the delta-apply folded into the V pass) becomes a longer pass sequence mirroring the CPU function above, following the exact pattern the file's own Dehaze passes already establish for multi-stage separable filtering (`fs_min_h`/`fs_min_v`, `fs_mean_h`/`fs_mean_v`):

1. `fs_clarity_meanp_h` / `_v` — box mean of luma (`mean_p`), reusing today's H/V shape.
2. `fs_clarity_corrp_h` / `_v` — box mean of `luma * luma` (`corr_p`).
3. `fs_clarity_ab` — one per-pixel pass (no blur) computing `a`, `b` from `mean_p`/`corr_p`, written to a two-channel intermediate (`a` in one channel, `b` in the other — a single `rg32float`-style texture, avoiding a second full pass just to split them).
4. `fs_clarity_meanab_h` / `_v` — box mean of `a` and `b` together (same two-channel texture, one filter pass does both since a box filter is per-channel-independent), giving `mean_a`, `mean_b`.
5. Final V pass (folding the compose + delta-apply, same shape `fs_clarity_v` already uses today): `q = mean_a * luma + mean_b`, `delta = (luma - q) * clarity_amount`, write `rgb + delta` to `gradedTex`.

That's roughly 4 box-filter pairs (8 passes) plus one cheap per-pixel pass, versus today's 1 pair (2 passes) — more passes, but each is the same cheap separable-box-filter shape already proven at `CLARITY_RADIUS=24` today, and Dehaze's existing pipeline already runs a comparable number of passes per frame without threatening the render budget (§6). Exact bind-group/texture-format layout is an implementation detail, finalized against the existing `r32float`-texture, `textureLoad`-only convention this file already establishes (see its own doc comment on why: `r32float`/`rgba16float` aren't filterable by default in WebGPU).

## 6. Testability

**Rust unit tests** (new, in detail.rs's own test module):
- Constant input → `guided_filter_self` returns the input unchanged (the `var_p = 0` identity from §3).
- Large `eps` (e.g. `1e6`) → `guided_filter_self` output is within float tolerance of `separable_mean_filter`'s own output on the same buffer (the `eps → ∞` limit from §3) — this is the test that most directly pins "strict generalization, not a different algorithm."
- A synthetic step edge (flat low region, flat high region, hard boundary) at a realistic working `eps`: assert the guided-filter output near the boundary is closer to the true (unblurred) pixel value than `separable_mean_filter`'s own output at the same radius is — a concrete, numeric regression test for "less overshoot/halo than the plain box blur," not just an eyeballed claim.
- Existing `apply_local_contrast`/Texture tests are untouched (proving Texture's path and output are unaffected, per §4).

**CPU/GPU parity**: extend `e2e/specs/develop-cpu-gpu-parity.e2e.js` with a Clarity-at-a-high-amount scenario (the file doesn't appear to have one specifically isolating Clarity today — confirm during implementation and add one if not), at the suite's existing tolerance bounds (`TOLERANCE = 4`, or `HUE_SENSITIVE_TOLERANCE` if warranted — to be determined once the real GPU output is measured, not assumed).

**Performance**: extend `e2e/specs/develop-performance.e2e.js` (ADR-0004's own harness) with a Clarity-heavy scenario, and report the real measured number the way ADR-0004's own "Update" section already does for every other op it measured (e.g. "Exposure 25ms … Sharpen 3ms"). M5.6's own exit criteria requires staying under the ~100ms budget — this is the check that proves it empirically rather than assuming the extra passes are cheap enough.

**Visual**: at least one real test image with a strong edge (a dark silhouette against a bright sky is the classic case) at a high Clarity amount, before/after screenshot comparison in the browser/computer-use verification this project already does for UI-adjacent work, called out explicitly in the PROGRESS.md entry this slice adds — satisfying M5.6's own "measurable quality improvement" exit criterion with something concrete, not just "looks better."

## 7. Exit criteria (this slice)

- `guided_filter_self` implemented and unit-tested per §6 (identity-on-constant, box-mean-limit-at-large-`eps`, less-overshoot-than-box-mean-at-an-edge).
- Clarity's CPU path and WGSL GPU path both switched to it; Texture's path is provably untouched (existing tests unchanged and passing).
- CPU/GPU parity harness passing at an established, justified tolerance.
- Real measured render latency for a Clarity-heavy scenario, reported and confirmed under the ~100ms budget (or this RFC is revisited before anything ships, per M5.6's own discipline).
- A documented before/after comparison showing reduced haloing on at least one real image with a strong edge, at a high Clarity amount.
- PROGRESS.md gets an M5.6 entry recording this as the milestone's first finding, in the same "finding → plan → implementation → verification" shape this RFC itself follows — establishing the pattern for whichever effect gets researched next.
