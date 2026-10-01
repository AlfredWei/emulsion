# RFC-0021: Lens Corrections — apply the profile in the correcting direction, in linear light (M5.6's eleventh effect)

- Status: Accepted (merged in PR #188; implemented in the follow-up PR — see "Corrected during implementation" at the end)
- Date: 2026-10-01
- Companion documents: [PRD/MILESTONES §M5.6](../../PRD/MILESTONES.md#m56--develop-effects-quality--performance-research), [develop_engine/lens.rs](../../app/src-tauri/src/develop_engine/lens.rs), [lens_profile.rs](../../app/src-tauri/src/lens_profile.rs), [gpu/shaders/lens.js](../../app/src/lib/gpu/shaders/lens.js), [RFC-0017](RFC-0017-tone-linear-light-correctness.md) (the linear-light convention reused here), [RFC-0002](RFC-0002-develop-gpu-cpu-fallback.md), [Appendix A](#appendix-a-how-the-findings-were-verified), [PROGRESS.md](../../PROGRESS.md)

## 0. Process note

Last of the "not yet researched effects" list (Split Toning was read during RFC-0019's pass and its math judged sound). Lens Corrections is the only effect whose algorithms are a *port* of an external library (`lensfun`), so the research question was different from the other nine: not "is there a better algorithm" but "does the port do what the library does". It was checked by running the app's mapping and the upstream `lensfun` crate's own `Modifier` side by side on the same lens, focal length and image size. That comparison found a defect that is a plain sign/direction error, not a matter of taste (§1.1), plus two smaller findings (§1.2, §1.3).

## 1. Problem

### 1.1 Profile distortion and TCA are applied in the opposite direction (measured)

`apply_lens_correction` maps each *output* pixel to the *source* pixel to sample (`lens_correct_coord`). For profile distortion it uses `lens_undist_poly3/poly5/ptlens` (a Newton-iteration inverse) and for TCA it uses the Newton inverse `lens_undist_tca_poly3` and, for the linear model, `rescale_tca` bakes in `kr → 1/kr, kb → 1/kb`. The module's header says these are a "direct, hand-verified port" of `lensfun`'s kernels, and they are — but of the kernels that `lensfun` uses when `Modifier::new(…, reverse = true)`.

**The lensfun crate documents `reverse` backwards.** Its doc comment on `Modifier::new` says "`false` simulates the distortion (forward), `true` corrects it". Its code and tests say the opposite: every correction test in the crate (`tests/integration/modifier.rs`) builds `Modifier::new(…, false)` and then calls `enable_distortion_correction`/`enable_tca_correction`/`enable_vignetting_correction`; the dispatcher's own comment reads "distortion forward → 250 (reverse=false → Undist)"; and `reverse = false` is the upstream default. The vignetting kernel in this app was ported from the code (gain `1/gain`, i.e. `reverse=false`); distortion and TCA were ported from the doc comment (`reverse=true`). So **one lens correction is applied correctly and two are applied backwards**.

**Measured** (the checked-in harness in [Appendix A](#appendix-a-how-the-findings-were-verified); lensfun's bundled database; Canon EF 24-70mm f/2.8L II on an EOS 5D Mark III; 6000×4000 image). The app's source-sampling radius for an output pixel was compared with `Modifier::new(…, reverse=false)` and `reverse=true` for the same pixel:

| Focal | Output pixel (r from centre) | Source radius: **upstream `reverse=false` (the correction)** | Source radius: **app** | Upstream `reverse=true` |
|---|---|---|---|---|
| 24 mm | (100, 100) — corner, r = 3467 | **3360.6** | 3572.4 | 3572.4 |
| 24 mm | (1000, 700), r = 2385 | **2325.5** | 2449.2 | 2449.2 |
| 70 mm | (100, 100), r = 3467 | **3563.5** | 3378.3 | 3378.3 |
| 70 mm | (1000, 700), r = 2385 | **2420.3** | 2351.5 | 2351.5 |

The app equals `reverse=true` at every point tested (to 0.1 px), never `reverse=false`. Source/output radius ratios along a diagonal ray, 0.25 → 1.0 of the half-diagonal: at 24 mm upstream-correct is 1.000, 0.985, 0.971, 0.971 and the app is 1.000, 1.015, 1.031, 1.028 (the app is on the other side of 1.0 — it is approximately the inverse mapping); at 70 mm upstream-correct is 1.007, 1.011, 1.017, 1.031 and the app 0.993, 0.989, 0.983, 0.972.

**Which direction is correct is not left to the naming argument.** The sign test: the calibrated coefficients for this lens at 24 mm give `Rd/Ru = 0.971` near the frame edge (the lens renders edge content closer to the centre — barrel distortion; see §A.4 for how far this was checked) and `1.03` at 70 mm (pincushion). A correcting output pixel must sample the source at the *lens's* distorted radius for that output radius — closer to the centre at 24 mm (3360 < 3467, upstream) and further out at 70 mm (3563 > 3467, upstream). The app does the reverse in both cases. Equivalently: composing the lens with the app's "correction" leaves a radial error of `3572/3361 = 1.063` at the 24 mm corner and `3378/3563 = 0.948` at 70 mm — about **twice the distortion the correction was meant to remove**, never zero.

**TCA is the same sign error**, smaller in pixels: at the 24 mm corner pixel the correct red-channel source radius is 3467.49 (+0.50 px relative to the output radius) and the app's is 3466.49 (−0.50 px); blue +0.18 vs −0.19. The channels are displaced the wrong way, so a fringe the lens produces is doubled (≈ 1 px apart at a 6000 px width) rather than cancelled.

**Both engines.** The WGSL port (`lensUndistPoly3/Poly5/Ptlens`, `lensUndistTcaPoly3`) is a line-for-line twin, so the CPU export/thumbnail path and the GPU interactive path agree — and are both wrong, which is why the CPU/GPU parity harness could never have noticed it (and has no lens scenario).

**User-visible consequence.** With "Enable Profile Corrections" on and a matched lens, Distortion Amount 100 makes lines *more* curved at a rate of ≈ 2× the original distortion; the Amount slider at 0 (no correction) is the best-looking setting. The manual Distortion slider is not affected (§2); the code comment on its hand-picked constants says it is the part that was confirmed against a real image in M3, and nothing in the repo records a profile-path check on a real photo.

### 1.2 The vignetting gain is multiplied in gamma-encoded space (measured, with a primary source)

`apply_lens_correction` multiplies the encoded 8-bit sRGB values by `1/gain` (CPU: `c as f32 * mult`; GPU: `rgb * mult`). The `gain = 1 + k1·r² + k2·r⁴ + k3·r⁶` model describes the *irradiance* fall-off of the optics — a linear-light quantity. lensfun's own manual says so directly (Library architecture, "Linear RGB"): its transformations "at least those involving TCA and vignetting correction – must be applied to linear RGB data", and "if you do vignetting correction after a gamma has been applied, the corners probably will look too bright." That is exactly what the app does. (`apply_edit_stack`'s own Exposure and White Balance were moved into linear light by [RFC-0017](RFC-0017-tone-linear-light-correctness.md) for the same reason; the lens pass was not part of that slice.)

Same multiplier, today's encoded-space result vs linear-light (sRGB decode → multiply → encode), 8-bit output (`vignette_space.py`):

| Gain | Input 32 | 64 | 128 | 192 |
|---|---|---|---|---|
| 1.25 | 40 vs 36 | 80 vs 72 | 160 vs 142 | 240 vs 212 |
| 1.5 | 48 vs 40 | 96 vs 78 | 192 vs 154 | 255 vs 230 |
| 2.0 | 64 vs 47 | 128 vs 90 | 255 vs 176 | 255 vs 255 |
| 4.1 | 131 vs 69 | 255 vs 126 | 255 vs 242 | 255 vs 255 |

(encoded / linear). The 24 mm f/2.8 profile above produces a **corner gain of 4.1** (and 1.52 at 2385 px from centre): a corner pixel of encoded value 64 goes to 255 (clipped white) where linear light gives 126; a mid-gray at gain 1.5 is 38/255 too bright. Corners are over-brightened and clip early, and the over-correction is worst exactly where vignetting is strongest.

### 1.3 Out-of-frame samples render differently on CPU and GPU (read from the code; not yet measured on pixels)

When the source coordinate falls outside the frame — the corners of a corrected pincushion lens, or of a barrel lens when the sign is wrong — `sample_bilinear` (CPU) returns **black** by design (its doc comment: "rather than clamping to the nearest edge pixel … leaving them honestly blank"). The GPU path samples with `srcSampler`, created without an `addressMode` (default clamp-to-edge), so it **smears the edge pixels** into the void. `lens.js`'s own comment says the sampler's "clamp-to-edge addressing matches `sample_bilinear`'s edge behavior", which contradicts `sample_bilinear`'s own documentation. How much frame this affects is lens-dependent: measured with the *correct* mapping (§A.3), the fraction of output pixels whose source is outside the frame is 0 % at 24 mm, 0.44 % at 35 mm, 0.98 % at 50 mm and **4.19 % at 70 mm**; with the app's current backwards mapping it is 5.75 % at 24 mm and 0 % at the others. Fixing §1.1 therefore changes *which* lenses show void corners — so the CPU/GPU disagreement over what the void looks like becomes user-visible for pincushion lenses and must be settled in the same slice. There is also no auto-scale (the crate has no `get_auto_scale`), see §2.

## 2. Non-goals

- **Manual Distortion and Manual CA** (the `manual_distortion`/`manual_ca` sliders). They are a self-contained, hand-picked single-coefficient correction (constants calibrated by eye), not a port of a library direction. They stay as they are; their sign convention (positive removes barrel) is already the Lightroom convention. A later slice could replace their Newton inverse with a closed form for speed; not here.
- **Auto-scale / "Constrain Crop"** (zooming the corrected image so no void shows). lensfun has the concept; the Rust crate does not implement it, so it is a feature to design, not a bug to port. §3.4 only settles what the void looks like.
- **Correcting on RAW-linear data.** lensfun wants the *sensor's* linear RGB (its manual: TCA on sRGB data "the colour fringes are still visible", because the colour matrix has already mixed channels). This app runs lens correction on the already-developed 8-bit sRGB image for every source, so per-channel TCA on sRGB is a known limitation of the architecture. Moving the lens pass before demosaic→sRGB is its own, much larger slice.
- **Higher-precision intermediate.** The lens pass output is 8-bit; a ×4 corner multiply of dark pixels amplifies 8-bit quantisation. Keeping a float intermediate between the lens pass and grading is an architecture change, not folded in.
- **Resampling in gamma space.** Bilinear interpolation of encoded values for the distortion/TCA resample is the project-wide convention (rotate, crop, perspective) and a second-order effect next to §1.1.
- **No change to profile matching** (`match_profile`, aperture/focal interpolation, `rescale_*` for distortion and vignetting). The distortion and vignetting rescale formulas are a faithful port of upstream's forward model; only the *kernel direction* applied to them is wrong. (The TCA *linear* rescale bakes in the wrong direction and is changed; §3.2.)

## 3. Design

### 3.1 Distortion: use the forward polynomial for correction

`lensfun`'s own correction (`reverse = false`) evaluates the closed-form forward model on the output coordinate: `Rd = Ru·(1 + k1·Ru²)` (poly3), `Rd = Ru·(1 + k1·Ru² + k2·Ru⁴)` (poly5), `Rd = Ru·(a·Ru³ + b·Ru² + c·Ru + 1)` (ptlens) — the crate's `dist_poly3/dist_poly5/dist_ptlens`. Replace the three `lens_undist_*` calls in `lens_correct_coord`'s profile branch with these (ports of the upstream closed forms; `Rd = 0` returns the input). Side effects, all improvements: **no Newton iteration** (the current kernels run up to 6 f64 steps with a `sqrt` per pixel per channel — the closed form is a handful of f32 multiplies), **no non-convergence/NaN fallback** for the profile path, and the same on the GPU.

`lens_undist_poly3` is still needed by *Manual Distortion* (§2) and stays for that; the poly5/ptlens/TCA Newton kernels become unused and are removed rather than left as dead code.

### 3.2 TCA: use the forward kernels

- **poly3 TCA**: closed-form forward `Rd = Ru·(b·Ru² + c·Ru + v)` (the crate's `tca_poly3_forward`), per channel, instead of the Newton `lens_undist_tca_poly3`.
- **linear TCA**: `rescale_tca` no longer inverts (`kr`, `kb` are passed through; the kernel multiplies by `k` as it does today). The existing test `rescale_tca_linear_inverts_for_correction` encoded the inverted direction and is replaced with its opposite and a comment saying why.

### 3.3 Vignetting: multiply in linear light

Replace `c·mult` with `OETF(EOTF(c)·mult)` using the sRGB transfer functions the project already uses for RFC-0017 (`color.rs`/`gradeMath.js`): decode via the existing 256-entry table, multiply in float, encode and round. On the GPU, the same two functions around `rgb * mult` (the lens shader gets the helper functions the grade shader already has, or they move into `common.js`). `lerp(1, 1/gain, amount)` is kept as the *linear-light* multiplier, so Amount 0 is an exact passthrough and Amount 100 the full correction — blending the multiplier, not the encoded result.

### 3.4 Out-of-frame samples: black on both paths

Settle on the CPU's documented behaviour ("honestly blank", matching `rotate_image` and the crop pipeline's own blank-corner convention): the GPU lens pass returns black when any of a channel's source coordinates falls outside `[0, dims−1]/dims`, with the same inclusive bound the CPU uses (`x > width−1` ⇒ black), instead of relying on the sampler. `lens.js`'s comment is corrected. No change to `fs_perspective`, whose out-of-bounds handling is a separate, already-documented tradeoff.

### 3.5 A real, named consequence

Edits that already have lens correction enabled (stored profile + `distortion_amount`/`ca_amount` > 0) **re-render differently**: the distortion and TCA now go the other way. This is the fix working as intended — the stored intent was "correct my lens", and the old render did the opposite — and no migration is attempted (there is no faithful mapping from the wrong mapping to the right one; the app is pre-release and the catalog is local-only). Vignetting-corrected images will look *less* bright in the corners for the same Amount. And for pincushion lenses the corners of a corrected image can now show black voids (§1.3) where, with the old sign, barrel lenses did.

## 4. Testability

- **An independent oracle, in the test suite**: for a set of lenses/focal lengths (the harness's Canon 24-70 at 24/35/50/70 mm, plus at least one `poly3` and one `poly5` lens found in the bundled database, and one linear-TCA lens), assert that `lens_correct_coord` for the profile path equals `lensfun::Modifier::new(…, reverse=false)` + `enable_distortion_correction`/`apply_geometry_distortion`/`apply_subpixel_distortion` to within 0.01 px over a grid of output pixels (`lensfun` is already a dependency, so this adds no new crate). This test would have failed at M3.
- **Direction by construction (no library in the loop)**: composing the correction with the forward lens model `g` (the closed forms above) recovers the identity radius: `|c(P)| = g(|P|)` for output pixels `P`, within float tolerance; the old code gives `g⁻¹` and fails.
- **Existing hand-derived kernel tests are updated, not deleted**: tests that pin the Newton kernels' outputs move to the closed forms with fresh derived values (computed by a separate script, not by the code under test).
- **TCA linear**: `rescale_tca` no longer inverts; the old inverting test becomes the opposite assertion; a pixel-level test that a known `kr = 1.001` moves the red channel *outward* by the expected amount at a known radius.
- **Vignetting in linear light**: a hand-computed value (gain 1.5 on input 128 → 154, not 192; gain 1.25 on 192 → 212, not 240 — the §1.2 table) asserted exactly; Amount 0 is a bit-exact passthrough; Amount 100 at a gain of exactly 1 is a bit-exact passthrough (OETF∘EOTF round-trips every `u8`, asserted over all 256 values).
- **CPU/GPU parity**: a direct numeric browser check of the WGSL kernels against the Rust values at the §1.1 pixels (the same technique the HSL slice used), including a pixel whose source falls outside the frame (black on both). An e2e scenario is only possible if the fixture's EXIF matches a lens in the database; if not, the e2e lens scenario uses the *manual* distortion path (CPU/GPU parity of the shared sampling and out-of-frame rule) and the profile path is covered by the direct browser check.
- **Out-of-frame rule**: a corrected pincushion case (70 mm) asserts the corner pixel is exactly black on the CPU, and the WGSL check asserts the same.
- **Performance**: record the CPU time of `apply_lens_correction` on a 24 MP image before and after (the closed form should be cheaper than Newton; this is a prediction, not a claim), against M5's ~100 ms interactive budget for the GPU path.

## 5. Exit criteria (this slice)

- Profile distortion and TCA use the forward (correcting) kernels in Rust and WGSL; the upstream-oracle test passes for every lens in the set, in both engines' value check.
- Vignetting multiplies in linear light in Rust and WGSL, with the §1.2 table values asserted.
- The CPU and GPU agree on out-of-frame samples (black), proven by tests, with `lens.js`'s comment corrected.
- Dead Newton kernels removed; `lens_undist_poly3` kept only for manual distortion, with a comment saying so.
- PROGRESS.md records the §1.1 table, the lensfun-crate doc-comment finding (and that the app's earlier "hand-verified" claim was about the port, not the direction), the §3.5 re-render consequence, and the §2 limitations (sRGB-domain TCA, 8-bit intermediate, no auto-scale).

## Appendix A: how the findings were verified

Everything in §1 can be re-run from [`RFC-0021-appendix/`](RFC-0021-appendix/).

### A.1 `lens_direction_check.rs` — the app against upstream, same lens, same pixel

A Rust test file (not built by default; the header lists the three commands to copy it into the test tree, run it, and remove it). It loads lensfun's bundled database, matches the lens through the app's own `match_profile` (so the coefficients are exactly what the app bakes into an edit), builds the app's `LensCorrection` from that match exactly as the render path does (`lens_correction_op` on the JSON), and then, for the same output pixels, compares:

1. **Distortion** — `lens_correct_coord` (the app) vs `Modifier::new(…, reverse)` + `enable_distortion_correction` + `apply_geometry_distortion` for **both** values of `reverse`. Reported as source radius (distance from the image centre) so a reader can see the sign of the error at a glance.
2. **TCA and vignetting** — the same, with `apply_subpixel_distortion` (per-channel coordinates) and `apply_color_modification_f32` on a pixel of value 1.0 (the multiplier).
3. **Radial ratio and out-of-frame fraction** over a 120×80 grid of output pixels, for the correct mapping and for the app's.

The result that decides the question: **the app matches `reverse=true` for distortion and TCA and `reverse=false` for vignetting, at every point.** Nothing else in the comparison is a judgement call.

### A.2 Why `reverse=false` is the correcting direction (the part the crate's doc comment gets wrong)

Four independent indications, none of which is the naming of `undist`/`dist`:

1. **The crate's own tests**: every correction test in the upstream-derived suite constructs `Modifier::new(…, false)` and calls `enable_distortion_correction` / `enable_tca_correction` / `enable_vignetting_correction`.
2. **Vignetting is a witness inside the same call**: with `reverse=false` the multiplier at the corner pixel is **4.10** (brightening, i.e. correcting a real darkening); with `reverse=true` it is **0.24**. Vignetting direction is unambiguous (a correction must brighten corners), and the same flag controls it.
3. **Sign of the calibrated coefficients**: the 24 mm ptlens coefficients (`a = 0.233, b = −0.308, c = 0.058`, rescaled) give `Rd/Ru = 0.971` near the frame edge — the observed image is compressed at the edge (barrel). A correcting map must therefore sample *inside* the output radius there, which is what `reverse=false` does (3360.6 < 3467.0) and the app does not (3572.4).
4. **Convention in practice** (recalled from darktable's lens module, **not re-checked in this session**): darktable constructs the modifier with `reverse` false to correct and sets it true only for its "distort" mode. Points 1–3 stand without this one.

### A.3 `vignette_space.py`

Pure Python, no dependencies; prints the §1.2 table (today's encoded-space multiply vs sRGB-decode → multiply → encode, 8-bit rounded).

### A.4 Limits of this verification (stated plainly)

- **No photograph of straight lines was used.** The direction argument is the four points in §A.2 and the numbers in §A.1, not an image-based residual-curvature measurement. The implementation PR's oracle and composition tests are what turn the argument into a regression test; a before/after on a real photo of a gridded subject is a recommended manual check there.
- **Lens behaviour claim.** That this lens is barrel at 24 mm and pincushion at 70 mm is taken from the sign of the *calibration data in lensfun's own database* (point 3 above), not independently from lens reviews; it is consistent with the commonly-reported character of that lens but was not checked against a published measurement in this session.
- **One lens and one body.** The mapping comparison used the Canon 24-70/2.8L II; the implementation's oracle test widens it to poly3/poly5 distortion and linear TCA lenses. The direction conclusion does not depend on the lens (it is a property of which kernel is called), but the magnitudes in §1.1 do.
- **The CPU out-of-frame behaviour is read from `sample_bilinear`'s source and documentation; the GPU's from the sampler descriptor's absence of an `addressMode`.** Neither was rendered and compared pixel by pixel; the implementation's tests do that.
- **Colour-space argument.** §1.2 quotes lensfun's manual and tabulates the arithmetic; it is a statement about what the model assumes, not a visual comparison on a vignetted photo.

## Corrected during implementation

- **Oracle lens set.** The RFC asked for "at least one poly3 and one poly5 lens and a linear-TCA lens". `lens_correct_coord` now equals `lensfun::Modifier(reverse = false)` to within 0.05 px over 5 pixels × 8 lens/focal combinations covering **all five** supported model kinds (distortion poly3 / poly5 / ptlens, TCA linear / poly3), using lens names and focal lengths from the lensfun crate's own regression tests; the test fails if fewer than three model kinds were actually compared, so it cannot pass by silently comparing nothing. (`lens_undist_poly3` was *not* removed — Manual Distortion still uses it, as §3.1 said; the poly5, ptlens and TCA Newton kernels were removed.)
- **GPU/CPU comparison done on real pixels, not only kernel values.** The RFC planned a "direct numeric browser check of the WGSL kernels". What was done is stronger: the app's real `fs_lens_correct` (the whole shader module, compiled with zero errors) is rendered on a real GPU (Apple / Metal 3) and compared pixel-for-pixel with `apply_lens_correction`'s output (harness: `lens_cpu_reference.rs` + `gpu_lens_check.js` in the appendix) for four cases on a 240×160 image, 49 sampled pixels each: **24 mm with every correction, max difference 2 / mean 0.33 (of 255); 70 mm with out-of-frame corners (24 of the 49 sampled pixels are black on the CPU), max 1 / mean 0.14; 24 mm vignetting only on uniform gray 128, max 3 / mean 0.78; manual distortion + CA, max 1 / mean 0.12.** The out-of-frame case is the §3.4 rule: black on both. The residual is 8-bit rounding and the half-texel coordinate convention the two paths already differed in.
- **A half-texel slack on the GPU in-frame test.** `lensInFrame` allows 0.01 texel beyond `[0.5, dims − 0.5]`: at identity `uv × dims` can land a rounding error outside that range on the edge texels, which must not turn them black.
- **Stored Linear-TCA profiles.** `rescale_tca` stored `1/kr, 1/kb` for the linear model, and that baked value lives in existing edit stacks. Develop re-resolves the profile on every open (`navigation.js`) and rewrites it when it differs, so any photo reopened in Develop picks up the corrected, non-inverted value. A photo with a linear-TCA lens that has an edit with lens correction on and is exported or thumbnailed *without* being reopened keeps the old inverted coefficient (still the wrong direction for that one model) until it is. Poly3 TCA and every distortion model are unaffected (their baked values are raw calibrations; only the kernel changed). No migration was attempted; named here instead.
- **CPU cost, measured (single run each, release, single thread, 24 MP, Canon 24 mm profile, distortion + TCA + vignetting):** **2.09 s now vs 2.71 s before (−23%)**. The RFC predicted "cheaper"; it is, but modestly — the cost is dominated by the three bilinear resamples and the per-channel f64 coordinate normalisation, not the Newton solve. Recorded by an opt-in `lens_cpu_cost_report` test. This is the CPU export/thumbnail path; the interactive GPU path was not timed.
- **Not done:** no e2e scenario. The fixture photo's EXIF does not match a lens in the database, and the manual-distortion path on a flat test patch would assert nothing about this change; the pixel comparison above is the GPU/CPU parity evidence instead.
