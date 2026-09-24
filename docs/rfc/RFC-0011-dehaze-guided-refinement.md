# RFC-0011: Guided-filter transmission refinement for Dehaze (M5.6's second effect)

- Status: Draft — for review in PR (flips to Accepted once merged)
- Date: 2026-09-24
- Companion documents: [RFC-0010](RFC-0010-clarity-guided-filter.md), [MILESTONES §M5.6](../../PRD/MILESTONES.md#m56--develop-effects-quality--performance-research), [ADR-0004](../adr/ADR-0004-rendering-and-color-management.md), [develop_engine/detail.rs](../../app/src-tauri/src/develop_engine/detail.rs), [gpu/shaders/dehazeLocalContrast.js](../../app/src/lib/gpu/shaders/dehazeLocalContrast.js), [PROGRESS.md](../../PROGRESS.md)

## 0. Process note

Same discipline as RFC-0010 §0: scoped to one effect, not the whole M5.6 milestone. This one was flagged as the natural next candidate in RFC-0010's own §2 ("Dehaze's transmission-map refinement... the same fix would apply almost directly") — §3 below corrects that framing once the real algorithm is worked out: "almost directly" undersells the difference, and this RFC exists partly to be honest about that before implementation, not after a second failing test catches it the way RFC-0010's `eps -> infinity` claim was caught.

## 1. Problem

Dehaze's dark-channel-prior transmission map (`t_raw`, from the dark channel via `dark_channel.iter().map(|d| 1.0 - DEHAZE_OMEGA * d)`) is refined with a plain box mean before use, and the code already names why that's a compromise:

> `dehaze_atmospheric_light`'s doc comment (detail.rs): "`DEHAZE_REFINE_RADIUS=4` (9x9 transmission-refinement window)... Radii are fixed... a plain box-mean blur stands in for a true edge-aware filter here."

And, from `apply_local_contrast`'s own doc comment (now superseded by RFC-0010's `apply_clarity`, but the transmission-refinement half of this sentence is still exactly the state of things): "the classic halo-prone Clarity look at high amounts... same named limitation as Dehaze's own transmission refinement."

Concretely: `pipeline.rs`'s `let t_refined = separable_mean_filter(&t_raw, w, h, DEHAZE_REFINE_RADIUS);` (and the WGSL twin, `fs_mean_h`/`fs_mean_v` in `dehazeLocalContrast.js`) blur the transmission map with no awareness of the scene's real edges. Haze transmission is a function of scene depth, which is genuinely discontinuous at real object boundaries — a person standing in front of a hazy mountain has a sharp transmission discontinuity at their own silhouette. A plain box blur smears that discontinuity, which shows up as a soft halo of under- or over-recovered haze right at high-contrast edges in the recovered image. This is a real, specific, already-named gap, not a speculative one — the same bar RFC-0010 set.

## 2. Non-goals

- **Every other part of Dehaze stays exactly as it is.** The atmospheric-light estimate (argmax-by-luminance, not per-channel maxima), the dark-channel computation (`separable_min_filter` at `DEHAZE_PATCH_RADIUS`), the raw-transmission formula (`1 - omega * darkChannel`), the recovery formula, and all four fixed constants (`DEHAZE_PATCH_RADIUS`, `DEHAZE_OMEGA`, `DEHAZE_T0`, and `DEHAZE_REFINE_RADIUS` itself) are unchanged — these are documented, deliberate, already-correct deviations from or matches to He et al. 2009, not touched by this slice. Only what refines `t_raw` into `t_refined` changes.
- **No change to Clarity, Texture, or any other effect.** RFC-0010 is done; this is the next, independent slice.
- **No new user-facing control.** Dehaze's own Amount slider is the only knob; this is an internal quality change to the refinement step underneath it.

## 3. Research finding — and a correction to RFC-0010's own framing

**Reference**: same paper, He, Sun, Tang, *"Guided Image Filtering"* (ECCV 2010/TPAMI 2013) — and, directly on point this time rather than by analogy, the paper's own §4, *"haze removal,"* which demonstrates guided-filter refinement of a dark-channel-prior transmission map as a faster, edge-preserving alternative to the soft matting He et al. 2009's own original paper used. This is not this project reaching for a technique from an adjacent application (the way RFC-0010's "detail enhancement" analogy was) — transmission-map refinement is the guided filter paper's own worked example, using the exact scene image as guidance for the exact kind of map this codebase already computes.

**The correction**: RFC-0010 §2 described this as reusing "this same primitive" (`guided_filter_self`, built for Clarity). That's wrong in a way worth stating plainly rather than quietly fixing. Clarity's case is *self-guided*: the guidance image and the thing being smoothed are the same signal (luma), which is what let `guided_filter_self` skip half the general algorithm's work (`corr_I` and `corr_Ip` collapse to the same quantity, `corr_p`, when `I = p`). Dehaze's case is **not** self-guided: the guidance image is the scene's own luma (`gradedTex`), but the thing being smoothed is `t_raw`, the transmission map — a genuinely different signal. This needs the *general* two-signal guided filter, of which `guided_filter_self` is the specialization, not a second caller of the same function.

### The general algorithm (guidance `I`, input `p`, two different signals)

```
mean_I  = boxfilter(I, r)
mean_p  = boxfilter(p, r)
corr_I  = boxfilter(I * I, r)
corr_Ip = boxfilter(I * p, r)
var_I   = corr_I - mean_I * mean_I
cov_Ip  = corr_Ip - mean_I * mean_p
a       = cov_Ip / (var_I + eps)
b       = mean_p - a * mean_I
mean_a  = boxfilter(a, r)
mean_b  = boxfilter(b, r)
q       = mean_a * I + mean_b
```

`q` replaces `t_refined` in `pipeline.rs`'s existing `let t = t_refined[idx].max(DEHAZE_T0);` recovery step — nothing else about Dehaze's math changes. Same shape of edge behavior as RFC-0010's self case: where `var_I` is small (a flat, textureless region of the scene), `a -> 0` and `q -> mean_p`, a plain smoothing of the transmission map, same as today. Where `var_I` is large (a real scene edge — exactly the silhouette case from §1), `a` moves toward `cov_Ip / var_I`, and the filter respects that edge instead of blurring the transmission discontinuity across it, because it's now conditioned on where the *scene* actually has structure, not just on the transmission map's own (already-smoothed-once) values.

This also explains why `guided_filter_self` and this new general function should **coexist as two separate functions, not one parameterized by "pass `I` twice for the self case"**: naively calling a general `guided_filter(I, I, ...)` for Clarity would recompute `mean_I` (redundant with `mean_p`) and `corr_Ip` (redundant with `corr_I`, since `I * I == I * p` when `I = p`) — two wasted box-filter passes RFC-0010's specialized 4-box-filter-pair derivation already avoids. Keeping both is the more efficient design for both call sites, not needless duplication.

## 4. Design — CPU (Rust)

New function in `detail.rs`, next to `guided_filter_self`:

```rust
/// General two-signal guided filter (RFC-0011; He, Sun, Tang, ECCV
/// 2010/TPAMI 2013 -- guidance `guide` and input `p` are DIFFERENT
/// signals, unlike `guided_filter_self`'s specialization above). Six
/// `separable_mean_filter` calls (mean_guide, mean_p, corr_guide,
/// corr_guide_p, mean_a, mean_b) plus two cheap per-pixel passes, same
/// primitive-reuse shape as `guided_filter_self`.
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
```

`pipeline.rs`'s dehaze block gains a `graded_luma` buffer (the same `luma = c[0]*0.2126 + c[1]*0.7152 + c[2]*0.0722` formula `apply_clarity`/`apply_local_contrast` already compute from `graded`, here computed once for this block) and replaces `separable_mean_filter(&t_raw, w, h, DEHAZE_REFINE_RADIUS)` with `guided_filter(&graded_luma, &t_raw, w, h, DEHAZE_REFINE_RADIUS, DEHAZE_GUIDED_EPS)`. `DEHAZE_GUIDED_EPS` is a new fixed constant, same "fix the knob" discipline as `CLARITY_GUIDED_EPS` — the guided-filter paper's own §4 haze-removal example uses a notably smaller `eps` (on the order of `(0.001)²`–`(0.01)²` on `[0,1]`-normalized inputs) than its detail-enhancement example, since a transmission map's own natural variance is much smaller than an image's luma variance; the real working value needs the same empirical tuning against this module's own tests RFC-0010's `CLARITY_GUIDED_EPS` got, not a guess frozen into this RFC.

## 5. Design — GPU (WGSL)

`dehazeLocalContrast.js`'s `fs_mean_h`/`fs_mean_v` pair (today: reads `filterInput`, rebound to `t_raw`'s own H-scratch texture, box-means it) becomes a longer sequence, same shape as RFC-0010's Clarity pass list but for two signals instead of one:

1. `fs_dehaze_meanguide_h`/`_v` — box mean of scene luma (`gradedTex`, binding 8) at `DEHAZE_REFINE_RADIUS`, giving `mean_guide`.
2. `fs_dehaze_meanp_h`/`_v` — box mean of `t_raw` (unchanged from today's own `fs_mean_h`/`_v` in everything but name), giving `mean_p`.
3. `fs_dehaze_corrguide_h`/`_v` — box mean of `luma(gradedTex)^2`, giving `corr_guide`.
4. `fs_dehaze_corrguidep_h`/`_v` — box mean of `luma(gradedTex) * t_raw`, giving `corr_guide_p`.
5. `fs_dehaze_a` — one per-pixel pass, no window loop: `a = (corr_guide_p - mean_guide*mean_p) / (corr_guide - mean_guide*mean_guide + eps)`.
6. `fs_dehaze_b` — one per-pixel pass: `b = mean_p - a*mean_guide`.
7. `fs_dehaze_meana_h`/`_v`, `fs_dehaze_meanb_h`/`_v` — box mean of `a` and `b`.
8. Final (replaces today's `fs_mean_v`'s role, writing to the same `transmissionTexFinal`-bound output `premask.js` already reads downstream, unchanged): `q = mean_a*luma(gradedTex) + mean_b`.

That's **15 passes** (4 box-filter pairs at 2 passes each = 8, plus 2 cheap per-pixel passes, plus 2 more box-filter pairs = 4, plus 1 final = 15), against Clarity's 11 and today's 2 — a real, larger jump than RFC-0010's own change, because the general two-signal algorithm genuinely needs four distinct filtered quantities (`mean_guide`, `mean_p`, `corr_guide`, `corr_guide_p`) where the self-guided case only needed two (`mean_p`, `corr_p`). Every one of those passes is a small, fixed-radius (`DEHAZE_REFINE_RADIUS=4`, a 9×9 window — far smaller than Clarity's 49×49) separable box filter, individually cheap, but **named as a real, open risk, not assumed away**: at a small radius, per-pass fixed overhead (draw call, bind group switch) is proportionally larger relative to the window-loop cost itself than it was for Clarity's much bigger radius, so 15 small passes is not obviously cheaper than 11 big ones just because the window is smaller — §7's performance check is what actually answers this, not intuition.

## 6. Testability

Same shape as RFC-0010 §6:

- **Rust unit tests**: `guided_filter`'s own boundary conditions — constant `guide` and constant `p` both reduce it to plain identity math the same way `guided_filter_self`'s did (worth re-deriving explicitly for the two-signal case rather than assuming it carries over unchanged, given RFC-0010's own `eps` mistake); calling `guided_filter(p, p, ...)` should numerically match `guided_filter_self(p, ...)` (a genuine cross-check between the two functions, catching a mismatched formula in either). A synthetic scene-with-a-real-edge-but-flat-transmission-elsewhere case, and the reverse (transmission edge with no scene edge, where the filter should NOT preserve it) — Dehaze's edge-awareness claim is specifically that it follows the *guide* image's edges, not the input's own, so a test needs to distinguish the two, unlike Clarity's self-guided case where there was only one signal to have edges in.
- **CPU/GPU parity**: extend `develop-cpu-gpu-parity.e2e.js` with a Dehaze scenario at a patch containing a real scene edge (RFC-0010's own still-open gap for Clarity applies here identically — a flat patch would validate nothing about the guided-filter change specifically, though it would still catch a gross Dehaze regression).
- **Performance**: extend `develop-performance.e2e.js` with a Dehaze-at-a-real-amount scenario, and treat the 15-pass count's own risk (§5) as the reason this measurement matters more here than it did for Clarity, not less.
- **Visual**: a real hazy photo with a clear foreground silhouette, before/after at a meaningful Dehaze amount, checking specifically for reduced halo/fringing right at the silhouette edge compared to today's box-mean version.

## 7. Exit criteria (this slice)

- `guided_filter` (general, two-signal) implemented and unit-tested per §6, including the cross-check against `guided_filter_self`.
- Dehaze's CPU path and WGSL GPU path both switched to it; every other Dehaze constant/formula (§2) is provably untouched.
- CPU/GPU parity harness passing at an established, justified tolerance, exercised at a real scene-edge patch.
- Real measured render latency for a Dehaze-at-amount scenario, reported and confirmed under the ~100ms budget — given §5's own 15-pass count, this is not a formality here; if it doesn't fit the budget, this RFC gets revisited (a reduced-radius or fewer-pass approximation) before anything ships, per M5.6's own discipline.
- A documented before/after comparison on a real hazy photo with a foreground silhouette, showing reduced edge fringing.
- PROGRESS.md gets an M5.6 entry for this slice, same shape as RFC-0010's own.
