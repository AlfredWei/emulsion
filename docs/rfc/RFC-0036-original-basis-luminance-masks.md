# RFC-0036: A luminance mask that measures the original image, so an adaptive "shadows" region does not move with the edits (M7 slice B follow-up)

- Status: §6 decided (2026-10-28: all three proposals accepted). **R1 (CPU + schema) and R2 (GPU twin) are built and tested; R3 (resolver, panel) is not.** §3's measurement was run on local photographs (numpy, outside the app).
- Companion documents: [RFC-0035](RFC-0035-region-bound-presets.md) §3.2 and §9 (the limitation this answers), [RFC-0034](RFC-0034-richer-local-adjustments.md) (mask adjustments), [RFC-0025](RFC-0025-composable-masking.md) (modifiers), [PROGRESS.md](../../PROGRESS.md)

## 1. Problem

RFC-0035's "shadows" resolver stores a `luminance_range_mask` (a range of brightness, e.g. 0 to 25 %). The user's requirement (2026-10-28): **the pixels a resolved region selects must not depend on what the user does afterwards**: the same photograph must give the same shadows whatever the order of edits. Today it does not: the mask reads the luma of the image *after* the global edits and after every earlier mask, so raising Exposure, or applying "Brighten faces" first, moves the stored range onto different pixels.

Faces and sky are not affected: they are stored as fixed pixels (a raster). Only a selection defined by brightness has this problem.

## 2. Code facts (read 2026-10-28)

- **CPU** (`develop_engine/pipeline.rs`, pass 8): each mask's weight is `mask.weight(uv, aspect, rgb)` where `rgb` starts as `pre_mask[idx]` (the graded image) and is **updated by every preceding mask in the loop**. `luminance_mask_weight` takes its luma from that `rgb`.
- **GPU** (`shaders/mask.js`, `fs_mask`): `var rgb = textureLoad(preMaskTex, coord, 0).rgb`, the same running `rgb`; the luminance kind takes `luma = dot(rgb, ...)`. The shader comment calls the order dependence "the correct WYSIWYG behavior, not a bug". **It is deliberate for masks the user draws**, so it must stay the default.
- **The un-graded reference already exists on both sides.** GPU: `perspectiveCorrectedTex`, the geometry-corrected image the grade pass reads at binding 1 (`sourceTexture.js`); the mask pass binds only two sampled textures today (`brushMasks`, `preMaskTex`), far under WebGPU's default limit of 16. CPU: the grade loop reads the input `image` pixel before any edit, and `image` is not written until pass 8 writes each pixel's final value, so the reference luma of a pixel is still readable there with no extra buffer (to be confirmed in R1, not assumed).
- **Packing**: for `luminance_range_mask`, `maskPack.js` writes `o+0` (min), `o+1` (max), `o+4` (feather), `o+5` (invert), `o+6` (kind); `o+2` and `o+3` are unused, and so is `o+7` (the raster layer: brush and segment only).
- **Modifiers** (RFC-0025) can be luminance shapes too and go through the same `component_weight`; they need the same basis switch or an old behaviour would hide inside a new mask.

## 3. Alternatives and what was measured

1. **Resolve on the edited preview.** Right only at the moment of applying; any later edit shifts it. Rejected (it does not meet §1).
2. **A basis switch on the luminance mask (this RFC).** Below.
3. **Store the shadows as a `segment_mask` raster from the original preview.** Fixed pixels, no renderer change. **Measured (2026-10-28, best case):** the exact shadow weight stored in the 256 x 256 field (8-bit, feather 100 so the ramp reproduces it) and read back through the app's bilinear lookup, against the same weight at a 1,200 px preview, on the 35 of the 45 sky photographs where a shadows region exists: mean absolute weight error 0.059 (worst photograph 0.198); **on average 8 % of pixels are off by more than 0.25 in weight, 32 % on the worst (bare branches), 17-23 % on trees, a lake and a desert**; about 8 % of the region (31 % worst) is pixels that are truly out of the range but get more than 0.3. The mask would also lose Min / Max / Feather editing and use one of the 8 shared raster layers. Rejected as the main answer; usable as a stopgap for a subtle preset only.
4. **Leave as is.** Does not meet §1.

## 4. Proposal

A `luminance_range_mask` (and a luminance *modifier*) may carry **`basis`: `"original"` or `"edited"`**; absent means `"edited"`, exactly today's behaviour, so no stored mask and no user-drawn mask changes.

- **`"original"`**: the luma compared with `[rangeMin, rangeMax]` (with the feather) is the luma of the **geometry-corrected, un-graded image**: the image after lens correction and perspective, before every tonal, colour, detail and mask operation. White balance, Exposure, curves, HSL, Texture, Dehaze, noise reduction, sharpening and every earlier mask therefore cannot move the selection. The adjustments the mask applies are still applied to the running `rgb` as now; only the *weight* is measured on the original.
- **GPU**: bind `perspectiveCorrectedTex` into the mask pass's bind group at a new binding and `textureLoad` it at the same `coord` as `preMaskTex`; read it only when a mask's basis flag is set (no cost for a photograph that has none). The flag goes in `o+2` of the packed shape (no layout change).
- **CPU**: pass the reference pixel's luma into `weight`, `fold_modifiers` and `luminance_mask_weight` (a new argument on the mask-weight dispatch, which every kind ignores but luminance).
- **Resolver (RFC-0035)**: the shadows resolver emits `basis: "original"`; its percentile is measured on the same original luma it already analyses, so resolver and renderer now agree by construction.
- **Panel**: the luminance mask block gets a "Measured on: Original / As edited" switch (default As edited for a mask the user draws; Original on one the resolver made). The switch moves the selection, which is stated in its label.

### 4.1 What "regardless of the user's behaviour" still does not cover

Said plainly, so the claim is not larger than the fix:

- **Geometry**: lens correction and perspective move pixels, so they move the selected region with the pixels (the mask follows the picture); crop does not change which pixels are selected.
- **Resolution**: the stored range is measured on the Develop preview at resolve time; the export renders the full-resolution source. Thresholding the luma of a downscaled and of a full-resolution image differs on fine texture (foliage, grass). The selection is the same *tonal* region, not the same pixels to the last one.
- **AI denoise** (`ai_denoise`) replaces the source it reads; the reference follows the denoised source. Not designed here; to be checked in R2.
- **Raw decode defaults** (the camera's default rendering) are the "original"; changing them (a different decoder setting) is not a user edit in this app today.

## 5. Slices

| Slice | What | Done when |
|---|---|---|
| **R1** CPU + schema (built, §8) | `basis` parsed on the luminance mask and modifier; reference luma passed through the weight dispatch; Rust tests: with `basis: "original"` the selected pixels are identical (to the bit, on a synthetic image) before and after Exposure +2, a tone curve, and a preceding mask; with the default they still move (the old behaviour is pinned) | tests pass; every existing mask test unchanged |
| **R2** GPU twin (built, §9) | binding, WGSL branch, `maskPack` flag, a CPU/GPU parity case | parity test passes in Chromium WebGPU (the local WebDriver window cannot render masks on the GPU: still the known gap, so GPU is verified there, not in the app window) |
| **R3** resolver + panel | shadows emits `basis: "original"`; the switch; the RFC-0035 §9 limitation closed; e2e: apply Cool the shadows, change Exposure, the persisted mask is unchanged and the CPU render selects the same pixels | e2e + the by-eye before / after |

Per the project's RFC practice for GPU-heavy slices, R2 gets a short ADR note after it lands.

## 6. Open questions for the user

1. **Does "original" include geometry?** Proposed: yes (lens correction and perspective applied, so the mask follows the pixels; the simplest reference on both twins). The alternative is the raw un-warped source, which makes the selection drift off the pixels whenever a lens or perspective edit is on.
2. **Should *every* luminance mask the user draws get the switch, or only resolver-made ones?** Proposed: every one (same code), default "As edited".
3. **Is the full-resolution vs preview difference (§4.1) acceptable?** The alternative is to resolve at export resolution, which costs a full decode per apply and is not proposed.

## 7. Not verified

- Every §2 fact is from reading the code; the CPU "reference pixel is still readable at pass 8" claim and the GPU binding count are to be confirmed in R1 / R2.
- §3's measurement is a numpy simulation of option 3 on 35 photographs (the other 10 are night or low-key frames where no shadows region is found); it did not use the app's renderer.
- Nothing about the cost in the render hot path (one extra `textureLoad` per masked pixel only when a mask asks for it) has been measured.
- Windows.

## 8. What R1 found (2026-10-28)

Built: `original_basis` on `LuminanceRangeMask` (parsed from `"basis": "original"`; absent, `"edited"` and any other word are the default), an `original: [f32; 3]` argument through `Mask::weight`, `Shape::weight` and `fold_modifiers`, and in pass 8 of `apply_edit_stack` the pixel's own value before any edit (`image` is written only at the end of each pixel's iteration, so it is still the geometry-corrected input there: the §2 CPU claim is **confirmed by reading and by test**, with no extra buffer). Six new tests in `tests/original_basis_masks.rs`: the basis decides which pixel is measured (both directions, and `"edited"` / an unknown word are the default); the dark pixel is selected and the bright one never, for global exposure -1, -0.5, 0, 0.7, 1.5 and 3 EV; the default basis still misses the pixel after +1.5 EV (the old behaviour, pinned beside the new); a full-weight +2 EV mask above does not move an original-basis selection but does move a default one; a luminance **modifier** with the basis takes it too; a stored mask without a basis parses to the default. `cargo test --lib`: 680 pass, none of the existing mask tests changed except the argument added at their two call sites.

- **The shadows resolver is deliberately not switched to `"original"` yet.** Until R2 gives the GPU the same reference, emitting the flag would make the canvas (GPU, measures the edited pixel) and the export (CPU, measures the original) select different pixels. It switches in R3, after R2.
- **Not verified:** R2 and R3; that the GPU's reference texture holds exactly the 8-bit values the CPU reads (it is the same source, but lens / perspective resampling is a float pass); the cost; the export at full resolution (§4.1).

## 9. What R2 found (2026-10-28)

Built: `originalTex` (binding 64) in `fs_mask`, bound to `perspectiveCorrectedTex` in the mask pass's bind group; `component_weight` takes the pixel `coord` and, for a luminance shape with `start_end.z > 0.5`, measures `textureLoad(originalTex, coord)` instead of the running `rgb`; `maskPack.js` writes the flag to `o+2` for a base mask and a modifier alike (one `packShape`); `basis` typed on `LuminanceRangeMask`. The §2 GPU claims are **confirmed**: the mask pass bound 2 sampled textures and now binds 3; auto layout kept the new binding out of every other pipeline; the shader compiled with no messages.

**Verified in a real WebGPU device** (Chromium 152 on Metal, the shipped WGSL and the shipped packer, rgba16float textures, a 64-pixel luma ramp; `app/src/lib/gpu/originalBasisHarness.dev.js`, run from the dev server's console): original-basis weight equals the trapezoid formula on the original luma to within 0.0086 (read back through the selected-mask overlay, so this includes half-float rounding) for global edits of -1, 0, +1, +2, +3 EV; the default basis equals the formula on the edited luma, so it still moves; with a +1 EV full-weight mask above (18 pixels on the ramp), the original basis stays within 0.0003 of the formula while the default deviates by up to 0.50; a luminance modifier with the basis intersected with a base mask matches within 0.0037. Unit tests pin the packing (flag at `o+2`, modifiers too, nothing else in the record changes) and the WGSL text.

- **What this is not:** a comparison with the Rust binary. The check is the GPU against the trapezoid formula re-written in the harness plus the invariance; the Rust side is covered by R1's tests, and the two share that formula. A bit-level CPU/GPU comparison on the same photograph is still open.
- **Not verified:** the app window (WebDriver cannot draw masks on the GPU; a regression run of the existing specs is recorded in PROGRESS); that the lens / perspective resampling float pass gives the GPU the same 8-bit values the CPU reads; cost on a large photograph; Windows/WebView2.
