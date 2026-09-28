# RFC-0014: Guided-filter blur for Color Noise Reduction (M5.6's fourth effect)

- Status: Draft — for review in PR (flips to Accepted once merged)
- Date: 2026-09-28
- Companion documents: [RFC-0010](RFC-0010-clarity-guided-filter.md), [RFC-0011](RFC-0011-dehaze-guided-refinement.md), [RFC-0012](RFC-0012-luma-nr-guided-filter.md), [MILESTONES §M5.6](../../PRD/MILESTONES.md#m56--develop-effects-quality--performance-research), [ADR-0004](../adr/ADR-0004-rendering-and-color-management.md), [develop_engine/detail.rs](../../app/src-tauri/src/develop_engine/detail.rs), [gpu/shaders/detailFilters.js](../../app/src/lib/gpu/shaders/detailFilters.js), [PROGRESS.md](../../PROGRESS.md)

## 0. Process note

Same discipline as RFC-0010 §0 / RFC-0011 §0 / RFC-0012 §0: scoped to one effect. RFC-0012 §0 left Color Noise Reduction as an explicitly open question rather than assuming its fix would carry over unchanged from Luma NR's: `color_nr_delta`'s own doc comment claims its per-channel reconstruction "preserve[s] luminance EXACTLY," proven via an algebraic cancellation (`chroma_delta[c] * weights[c]` summing to zero), and RFC-0012 worried that proof was "built for a linear box-mean blur specifically."

**That worry turns out to be unfounded — §3 below re-derives the cancellation from scratch and shows it never used the blur's linearity at all.** The real open question this RFC resolves is a different one, not visible until the algebra is actually redone: not *whether* the reconstruction survives a guided-filter swap (it does, for any blur), but *which* guided filter to swap in — self-guided per channel (Clarity/Luma NR's pattern) or the general two-signal filter guided by luma (Dehaze's pattern) — and that choice has a real, physically-motivated answer (§3.2) plus a real performance consequence (§5) neither prior slice had to consider, since both of those used exactly one signal.

## 1. Problem

Color Noise Reduction's blur source is a plain box mean, run independently on each of R, G, B at a fixed radius (`pipeline.rs`: `separable_mean_filter(&r, w, h, COLOR_NR_RADIUS)`, and twice more for `g`/`b`; the WGSL twin, `fs_colorNR_h`/`fs_colorNR_v` in `detailFilters.js`, does the equivalent as one joint `vec3` pass pair). Like Luma NR before RFC-0012, the code doesn't currently name this as a limitation — `color_nr_delta`'s own doc comment is entirely about the luma-preservation algebra, not about the blur's spatial behavior — so this RFC states the gap explicitly:

A plain box mean has no notion of where the image's real color boundaries are. Chroma noise in a demosaiced RAW is generally *higher-amplitude and lower-frequency* than luma noise (it survives Bayer-pattern color-channel gain mismatches and demosaicing interpolation, which luma's own three-channel weighted average partially cancels out) — exactly the noise this op exists to remove. But at a real color edge (a red flower against green leaves, a blue sky behind a yellow-lit building), the box mean mixes each channel's true, distinct color across the boundary along with the noise, and `color_nr_delta`'s reconstruction has no way to tell "this channel changed because of noise" apart from "this channel changed because I'm smearing two genuinely different colors together." The result is the well-documented raw-pipeline artifact real color-NR implementations are built to avoid: **color fringing at edges** — a thin band near the boundary where both colors' chroma has been pulled toward their local average, softening and discoloring what should be a crisp, correctly-colored edge. This is the same halo family Clarity/Dehaze/Luma NR already fixed, applied to chroma instead of luma/local-contrast/transmission.

## 2. Non-goals

- **Every other part of Color Noise Reduction stays exactly as it is.** `color_nr_delta`'s reconstruction formula — the raw per-channel delta, the weighted-mean subtraction, the joint-magnitude Detail gate (`color_smooth_weight`), the single shared scalar `k` — is unchanged in shape. Only the function that produces `blurred` changes, same scope discipline as RFC-0010/0011/0012.
- **Luma NR is not touched here** — already fixed in RFC-0012, and its blur source (`graded_luma`, self-guided) is read-only input to this slice, not modified by it.
- **No new user-facing control.** Color NR keeps its existing Amount/Detail sliders; `COLOR_NR_RADIUS` stays a fixed, non-user-exposed constant, same as before.
- **Sharpen, HSL, Split Toning, Grain, Vignette, Lens Corrections, white balance/auto-tone, Tone Curve** remain "not yet researched," same standing note every prior M5.6 RFC has carried.

## 3. Research finding

### 3.1 Correcting RFC-0012 §0: the luma-preservation proof needs no re-derivation for the reconstruction itself

Restate `color_nr_delta`'s construction precisely. Let `w = [0.2126, 0.7152, 0.0722]` (`Σw_c = 1`, matching `luma3`), `d[c] = blurred[c] - orig[c]` for `c ∈ {R,G,B}`, `weighted_mean(d) = Σ_c w_c·d[c]`, and:

```
chroma_delta[c] = d[c] - weighted_mean(d)
final[c]        = orig[c] + chroma_delta[c] * k
```

New luma:

```
Σ_c w_c·final[c] = Σ_c w_c·orig[c]  +  k · Σ_c w_c·chroma_delta[c]
                  = luma(orig)      +  k · Σ_c w_c·(d[c] - weighted_mean(d))
                  = luma(orig)      +  k · (Σ_c w_c·d[c]  -  weighted_mean(d)·Σ_c w_c)
                  = luma(orig)      +  k · (weighted_mean(d) - weighted_mean(d)·1)
                  = luma(orig)      +  k · 0
                  = luma(orig)
```

**Nothing about this derivation uses any property of `blurred` at all** — not linearity, not that it came from a box mean, not even that R/G/B were blurred by the *same kind* of filter. It only uses two facts about the reconstruction itself: `Σw_c = 1`, and the same `w`/`k` are used for all three channels (already the file's own "critical invariant" comment). `blurred[c]` can be literally anything — a box mean, three independently self-guided filters, three guided-by-luma filters — and the cancellation still holds exactly, up to floating-point rounding.

A concrete numeric check (not just symbolic): `orig = [0.3, 0.3, 0.3]`, an adversarial asymmetric `blurred = [0.4, 0.3, 0.3]` (a "blur" that only touched R, which no real per-channel filter would do symmetrically, but the proof doesn't care): `d = [0.1, 0, 0]`, `weighted_mean(d) = 0.02126`, `chroma_delta = [0.07874, -0.02126, -0.02126]`, `Σw_c·chroma_delta[c] = 0.2126·0.07874 + 0.7152·(-0.02126) + 0.0722·(-0.02126) = 0.016744 - 0.015205 - 0.001535 = 0.000004 ≈ 0` (residual is float rounding in this by-hand check, not a real defect — §6 turns this into an exact-arithmetic unit test).

So RFC-0012 §0's specific worry — that the box mean's *linearity* was load-bearing for luma preservation — was a reasonable thing to flag without re-deriving, but doesn't survive actually redoing the algebra. What box-mean linearity *is* used for, per `color_nr_delta`'s own doc comment, is a completely different, narrower claim: that blurring R, G, B jointly in one pass gives bit-identical results to blurring them independently (relevant to the WGSL side's existing single-`vec3`-pass implementation, §5 below) — an implementation-equivalence fact, not a correctness precondition for luma preservation.

### 3.2 The real open question: which guided filter, and why it isn't the self-guided one

RFC-0010/0012 used `guided_filter_self` (guide = input = the same signal) because in both cases the thing being filtered — luma, for local contrast and for denoising — *is* the guide: there is no cleaner reference signal available, and self-guidance is exactly He/Sun/Tang §5.4's own worked case.

Color NR is different: each channel being filtered (R, G, or B) is **not** the cleanest available reference for "where are this image's real edges" — it's specifically the *noisiest* one, since chroma noise is what this op exists to remove in the first place. Using a channel as its own guide (three independent `guided_filter_self` calls, one per channel) would compute each channel's edge-indicator `a` from that same channel's own local variance — variance that's inflated by the very chroma noise the filter is supposed to smooth through. In a flat-but-noisy region, `var_p` stays elevated by noise alone, `a` stays away from 0, and the filter backs off denoising exactly where it should be denoising hardest — the opposite failure mode from RFC-0010/0012's own use of self-guidance, where the guide (luma) is comparatively much cleaner to begin with.

`graded_luma` is the better guide here, for the same reason it's the better guide for Dehaze's transmission map (RFC-0011 §3, §III): it's a genuinely different, cleaner signal — a weighted sum of R/G/B whose per-channel sensor/demosaic noise partially cancels in the average — carrying real edge information (where colors/tones actually change) without being dominated by the chroma noise this op targets. This is also the standard choice in the wider denoising literature: luminance-guided or luminance-joint chroma denoising (distinct from filtering each chroma channel by its own statistics) is the common way raw pipelines avoid exactly the color-fringing failure §1 describes, because it lets the filter answer "is this pixel near a real scene edge" from a signal where that question is answerable, instead of from the noisy signal being cleaned.

**Design decision: reuse RFC-0011's general two-signal `guided_filter(guide, p, ...)` three times — once per channel, all three sharing `graded_luma` as `guide`** — rather than three calls to `guided_filter_self`, and rather than a new primitive. No new CPU function is needed; `guided_filter` already exists, is already unit-tested generically (constant-`p` identity, `guided_filter(p,p,...) == guided_filter_self(p,...)`, edge-preservation-vs-box-mean, and RFC-0011 §3's own corrected "does not falsely preserve a `p`-only edge" case), and none of that existing proof needs to change — this slice is a new *call site*, not a new *primitive*.

### 3.3 Instantiating §3.1 for this design

Substituting the design decision into §3.1's proof: `blurred[c] = guided_filter(graded_luma, channel_c, w, h, COLOR_NR_RADIUS, COLOR_NR_GUIDED_EPS)` for `c ∈ {R,G,B}`, each an independent call sharing the same `guide`. §3.1's derivation applies unchanged (it never inspected what `blurred[c]` was) — luma is still preserved exactly. The three calls *can* legitimately produce three different `a_R, a_G, a_B` coefficient fields (each channel's own covariance with luma differs), which is fine and expected: §3.1 already proved the reconstruction doesn't care.

## 4. Design — CPU (Rust)

No new primitive. `pipeline.rs`'s Color NR blur block:

```rust
let color_nr_blur = if color_nr.amount != 0.0 {
    let r: Vec<f32> = graded.iter().map(|c| c[0]).collect();
    let g: Vec<f32> = graded.iter().map(|c| c[1]).collect();
    let b: Vec<f32> = graded.iter().map(|c| c[2]).collect();
    Some((
        separable_mean_filter(&r, w, h, COLOR_NR_RADIUS),
        separable_mean_filter(&g, w, h, COLOR_NR_RADIUS),
        separable_mean_filter(&b, w, h, COLOR_NR_RADIUS),
    ))
} else {
    None
};
```

becomes:

```rust
let color_nr_blur = if color_nr.amount != 0.0 {
    let r: Vec<f32> = graded.iter().map(|c| c[0]).collect();
    let g: Vec<f32> = graded.iter().map(|c| c[1]).collect();
    let b: Vec<f32> = graded.iter().map(|c| c[2]).collect();
    // RFC-0014: guided by the graded image's own luma (already computed
    // above for Luma NR/Dehaze), not each channel's own noisy statistics
    // -- see guided_filter's own doc comment and this RFC's §3.2 for why
    // this is the general two-signal case, called once per channel,
    // sharing one guide -- not three independent guided_filter_self calls.
    Some((
        guided_filter(&graded_luma, &r, w, h, COLOR_NR_RADIUS, COLOR_NR_GUIDED_EPS),
        guided_filter(&graded_luma, &g, w, h, COLOR_NR_RADIUS, COLOR_NR_GUIDED_EPS),
        guided_filter(&graded_luma, &b, w, h, COLOR_NR_RADIUS, COLOR_NR_GUIDED_EPS),
    ))
} else {
    None
};
```

`color_nr_delta` itself (the reconstruction: raw delta, weighted-mean subtraction, `chroma_delta`, the joint-magnitude Detail gate, the single shared `k`) is **completely untouched**, per §3.1/§3.3 and §2's own scope line — it already receives `blurred: [f32; 3]` as an opaque input and has no idea what produced it, on the box-mean path or this one.

`COLOR_NR_GUIDED_EPS` is a new fixed constant in `detail.rs`, next to `LUMA_NR_GUIDED_EPS`/`DEHAZE_GUIDED_EPS`/`CLARITY_GUIDED_EPS`, same "fix the knob" discipline as every prior slice. `eps` here gates on `var_guide` (luma's local variance, per `guided_filter`'s formula — see §3 in RFC-0011: `a = cov_guide_p / (var_guide + eps)`).

**Corrected during implementation, same discipline as RFC-0010's own `eps` correction**: this RFC's original draft argued `LUMA_NR_GUIDED_EPS` (`0.0009`) was the right starting point, since both ops threshold the same guide signal (luma) for "is this a real edge." That reasoning was plausible but wrong in practice: at `0.0009`, `COLOR_NR_RADIUS`'s own small radius (4) left the edge indicator `a` too far from 1 near a modest, realistic color edge, letting a measurably *worse* result than the plain box mean leak through — caught by a failing version of §6's correlated-edge test, not assumed. The value that actually passes is `DEHAZE_GUIDED_EPS` (`0.0001`) — the other op sharing both this exact `graded_luma` guide *and* `COLOR_NR_RADIUS`'s own radius (4), which in hindsight is the more relevant precedent than Luma NR's self-guided case (RFC-0012 (radius 3) and Clarity (radius 24) are both self-guided, so the guide's own variance scale is a less direct comparison than Dehaze's shared-guide, shared-radius one).

## 5. Design — GPU (WGSL)

`detailFilters.js`'s `fs_colorNR_h`/`fs_colorNR_v` pair (today: one joint `vec3` box-mean pass pair, exploiting box-mean's linearity to blur R/G/B together in the same 2 passes Luma NR's single channel used before RFC-0012) is replaced by a pass sequence that keeps exploiting that same linearity — for the *box-filter* stages only, not for the per-pixel `a`/`b` compose, which genuinely differs per channel since each channel has its own covariance with the shared guide:

1. `fs_colorNR_meanGuide_h`/`_v` — box mean of `luma(gradedTex)` (**shared** across all three channels — computed once, not per-channel).
2. `fs_colorNR_corrGuide_h`/`_v` — box mean of `luma(gradedTex)^2` (**shared**).
3. `fs_colorNR_meanP_h`/`_v` — box mean of `gradedTex.rgb` as one `vec3` (today's existing joint pass, kept as-is, reused here as `mean_p` for all three channels at once — same linearity trick, still valid, since `mean_p` doesn't depend on the guide).
4. `fs_colorNR_corrGuideP_h`/`_v` — box mean of `luma(gradedTex) * gradedTex.rgb` (one `vec3` pass: `(luma*R, luma*G, luma*B)` — the per-channel covariance-with-guide term, packed jointly).
5. `fs_colorNR_a`, `fs_colorNR_b` — two per-pixel (no blur) passes computing, per channel: `var_guide = corrGuide - meanGuide²` (shared scalar, same for all three channels), `cov_guide_p[c] = corrGuideP[c] - meanGuide·meanP[c]`, `a[c] = cov_guide_p[c] / (var_guide + eps)`, `b[c] = meanP[c] - a[c]·meanGuide` — both `a` and `b` are `vec3` (one value per channel), written to two separate `rgba16float`/`rgba32float`-style intermediates (two passes, not one multi-output pass, matching this codebase's established "no multi-output passes" convention from RFC-0010 §5 onward).
6. `fs_colorNR_meanA_h`/`_v`, `fs_colorNR_meanB_h`/`_v` — box mean of the `vec3` `a` and `vec3` `b` textures (joint per-channel, same linearity trick as step 3/4).
7. Final compose (replaces today's role of `fs_colorNR_v` as the thing `premask.js`'s `color_nr_delta` port reads): `q = meanA * luma(gradedTex) + meanB`, one `vec3` output — per-channel `q[c] = meanA[c]·luma + meanB[c]`, all three in one pass since it's a pointwise vector op with no further blurring.

That's **6 box-filter pairs (12 passes)** plus **3 per-pixel compose passes** (`a`, `b`, final) = **15 passes total** against today's 2 — the same order of magnitude as Dehaze's own 15-pass jump (RFC-0011 §4), despite filtering three channels instead of one, because the guide-only statistics (`meanGuide`, `corrGuide`) are computed exactly once and shared, and every per-channel stage is packed into a single joint `vec3` pass rather than run three separate times — the same box-filter-linearity trick today's existing joint pass already relies on, applied one level deeper. (Three fully independent per-channel `guided_filter` calls, run as three separate single-channel pass sequences with no sharing, would instead cost roughly `3 × 15 = 45` passes — the sharing here is a real, load-bearing design choice, not an incidental optimization.)

New persistent intermediates: two shared single-channel (`meanGuide`, `corrGuide`) plus four shared three-channel/`vec3`-packed (`meanP`, `corrGuideP`, `a`, `b`, `meanA`, `meanB` — six `vec3` textures) following Luma NR's binding sequence (ended at 48; this slice's new textures/uniforms start at 49), wired through the same five-file checklist RFC-0010/0011/0012 each used (`pipelines.js`, `gpuHandles.js`, `sourceTexture.js`, `renderFrame.js`, `shaders/index.test.js`), plus the H-scratch-texture rebinding convention already established for every multi-pass box filter in this file.

At `COLOR_NR_RADIUS = 4` (a 9×9 window) — between Luma NR's 3 and Dehaze's own small-radius risk — §6/§7 flag the same "don't assume 15 small-radius passes are free" open performance question RFC-0011 §5 and RFC-0012 §4 already carried for their own slices, now compounded by this being the third-largest pass count shipped in this module after Dehaze and Luma NR.

## 6. Testability

Same shape as RFC-0010 §6 / RFC-0011 §6 / RFC-0012 §5, plus one test category unique to this slice (§3.1's corrected claim):

- **Rust unit tests**:
  - **A generalized luma-preservation test that does NOT assume a box-mean blur** — the concrete regression test for §3.1's corrected finding: construct an arbitrary, deliberately non-box-mean, per-channel-asymmetric synthetic `blurred` triple (not derived from any real filter — exactly the adversarial shape in §3.1's own worked numeric example) and assert `color_nr_delta`'s output still preserves luma exactly (within float tolerance). This one test is what actually proves RFC-0012 §0's worry doesn't apply, rather than asserting it in prose.
  - The same test, instantiated concretely with `guided_filter(luma, channel, ...)` outputs for a synthetic image with both flat-noisy and real-color-edge regions, confirming luma preservation holds for the *actual* shipped blur, not just the adversarial abstract case above.
  - A synthetic color-step-edge test — **corrected during implementation**: an earlier version of this bullet called for "two flat regions... sharing the same luma so the edge is chroma-only," which turns out to describe the *isoluminant* case below (§3.2's own worst case for this design, not its best one) — a same-luma edge gives the guide zero variance to detect, so there is nothing for it to do better than a box mean there. The real regression test uses a MODEST *correlated* edge instead (different, but non-identical, `luma3` values — the case §3.2 actually argues is common and favorable), checked end-to-end through `color_nr_delta` at a small enough raw amplitude that `color_smooth_weight`'s own gate stays open for both the guided and box-mean paths (a first attempt using a strongly saturated edge instead saturated that gate to fully-closed for the box-mean path, making its "error" trivially zero and the comparison meaningless — caught by a failing assertion). At the working `COLOR_NR_GUIDED_EPS` this confirms, the guided-filter path shows less chroma bleed into the neighboring region than the old three-independent-`separable_mean_filter`-calls version at the same radius.
  - A flat-noisy-region test (uniform luma and color plus per-pixel noise) confirming the guided blur tracks the plain per-channel box mean closely (small `var_guide` -> `a` near 0 -> `q` near `mean_p`), the same "doesn't regress flat-region denoising strength" check RFC-0012 §5 ran for Luma NR, with tolerance honestly measured rather than guessed (RFC-0012's own `0.005` finding is the precedent for not assuming a round number in advance).
  - `color_nr_delta` itself needs no new tests — §3.3 argues, and the first bullet above confirms, that it's unaffected by what produces `blurred`.
- **CPU/GPU parity**: extend `develop-cpu-gpu-parity.e2e.js` with a Color-NR-at-a-color-edge scenario — the same still-open gap every prior slice flagged (flat-patch fixtures validate nothing about an edge-aware change) applies here too, and matters more than usual since this op is the first one whose *guide* and *filtered signal* are computed and packed as `vec3`s on the GPU side, a new wiring shape worth parity-checking specifically.
- **Performance**: extend `develop-performance.e2e.js` with a Color-NR-at-amount scenario; given §5's own 15-pass/small-radius flag, treat this with the same seriousness RFC-0011 §5/§7 and RFC-0012 §4/§6 gave their own pass-count risks, not as a formality.
- **Visual**: a real noisy photo containing a real saturated color edge (not just a luma edge), before/after at a meaningful Amount, checking for reduced color fringing/bleed at the boundary while flat-region chroma-noise suppression looks comparable to today's baseline.

## 7. Exit criteria (this slice)

- The corrected §3.1 finding is captured as a passing unit test (the adversarial-blur luma-preservation case), not left as prose-only.
- Color NR's CPU blur source switched from three `separable_mean_filter` calls to three `guided_filter` calls sharing one `graded_luma` guide, with a tuned `COLOR_NR_GUIDED_EPS`; `color_nr_delta`'s reconstruction provably untouched (existing tests unchanged and passing, plus the new ones above).
- WGSL GPU path switched to the equivalent ~15-pass sequence (§5), with the guide-sharing/joint-`vec3`-packing design actually implemented, not just planned.
- CPU/GPU parity harness passing at an established, justified tolerance, exercised at a real color-edge-content patch.
- Real measured render latency for a Color-NR-at-amount scenario, reported and confirmed under the ~100ms budget (or this RFC is revisited before anything ships, per M5.6's own discipline), given §5's own pass-count risk flag.
- A documented before/after comparison on a real noisy photo with a real saturated color edge, showing reduced fringing with comparable flat-region denoising strength.
- PROGRESS.md gets an M5.6 entry for this slice, same shape as RFC-0010/0011/0012's own, explicitly noting the RFC-0012 §0 worry was checked and found not to apply (§3.1), so a future reader doesn't have to re-derive it a third time.
