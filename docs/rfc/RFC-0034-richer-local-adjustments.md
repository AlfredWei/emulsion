# RFC-0034: Richer local adjustments: more than Exposure, Contrast and Saturation inside a mask (M7 slice 0 of RFC-0033)

- Status: Draft for review (decided with the user 2026-10-26: slice 0 is the next step, design note first)
- Date: 2026-10-26
- Relates to: [RFC-0033](RFC-0033-adaptive-tone-presets.md) (why: the adaptive presets need it), [RFC-0025](RFC-0025-composable-masking.md) (the mask pass and modifiers), [RFC-0026](RFC-0026-click-select-segmentation.md) (segment masks), [RFC-0017](RFC-0017-tone-linear-light-correctness.md) (the maths this reuses), [RFC-0010](RFC-0010-clarity-guided-filter.md) and [RFC-0011](RFC-0011-dehaze-guided-refinement.md) (the spatial filters for 0b), [RFC-0018](RFC-0018-parametric-tone-hue-preserving-clamp.md) (the parametric tone)

## 0. Process note

Read from the code (2026-10-26), not run. The finding that shapes this RFC is that **the pointwise adjustments a mask needs already exist as one function in each twin**, so the first half is small and low-risk. The second half (spatial: Texture, Clarity, Dehaze) is not, and says so.

## 1. Problem

A mask adjusts **three** things: Exposure, Contrast, Saturation (`apply_adjustments(rgb, exposure, contrast, saturation)` in `develop_engine/tone.rs` and `apply_adjustments` in `gpu/shaders/gradeMath.js`, called per mask in `fs_mask` / `Mask::adjust`). The global stack has ten tone and colour controls plus Texture, Clarity and Dehaze. Lightroom's masks carry all of them. Every starter preset in RFC-0033 §3.3 needs at least one that is missing, and so does ordinary manual masking ("warm the face", "lift the shadows on the subject").

## 2. What is already there

- **`apply_global_adjustments`** (Rust `tone.rs:198`, WGSL `gradeMath.js`): white balance (Temperature, Tint as a linear-light gain), Exposure, Contrast, parametric tone (Highlights, Shadows, Whites, Blacks with RFC-0018's hue-preserving clamp), Saturation, in that order. It is the global pipeline's own per-pixel function. **With the new inputs at zero it equals the mask's `apply_adjustments`**, so a mask without them renders exactly as today.
- The mask pass reads the **already graded** pixel from `preMaskTex`, folds the mask weight (base shape plus modifiers), and blends `mix(rgb, adjusted, weight)`.
- Per-mask uniform stride is 16 floats (`MASK_STRIDE`), eight masks maximum; adjustments sit at `+8..+10`, modifiers' first index and count at `+12`, `+13`.
- The spatial filters (Texture, Clarity, Dehaze) are **separate passes with their own blur textures** (`detailFilters.js`, `dehazeLocalContrast.js`), driven by global amounts.

## 3. Proposal

### 3.1 Slice 0a: pointwise adjustments in masks

Add **Temperature, Tint, Highlights, Shadows, Whites, Blacks** to every weight-producing mask kind (linear, radial, brush, luminance range, colour range, segment). Not to spot (it copies pixels) or red-eye (its own correction), as today.

- **Same maths as the global stack, not a second implementation.** The mask pass calls `apply_global_adjustments` with the mask's ten values. This is the design's main decision: no new curve to tune, no drift between global and local, and a **strong test**: *a mask of weight 1 with adjustments X renders the same pixel as the global stack with X*, for each new control, on both twins.
- **Storage**: new optional numeric fields on the mask ops, default 0, absent = 0, so **old edit stacks are unchanged and no schema bump is needed** (the same way modifiers were added). Rust: one shared `LocalAdjust` struct instead of the exposure / contrast / saturation triplet repeated on each kind.
- **GPU packing**: `MASK_STRIDE` 16 to 24 floats (eight masks: 768 bytes, far from any uniform limit); the six new values appended after the existing fields (the offsets chosen when it is built, keeping the modifier fields where they are if the free slots allow it), packing and the WGSL `Mask` struct changed together and covered by the existing `maskPack` and `shaders/index` tests.
- **UI**: the mask panel keeps Exposure / Contrast / Saturation where they are and adds two groups, **Light** (Highlights, Shadows, Whites, Blacks) and **Colour** (Temperature, Tint), the same sliders, ranges and step buttons as the global panel (reuse its row component); a per-mask *Reset adjustments*. History entries and the mask's overlay are unchanged.
- **Presets and Copy Settings**: masks stay as they are today (shape masks copy, AI masks excluded); the new fields travel with the mask.
- **Cost**: Temperature / Tint add three sRGB-linear conversions per masked pixel and the parametric tone a few smoothsteps. It runs only where a mask's adjustment is non-zero (skip the call when all ten are zero, as the pass could already do for three). Measured on the real GPU in the browser harness with eight masks at full frame, against M5's ~100 ms interactive budget.

### 3.2 Slice 0b: Texture, Clarity, Dehaze (spatial)

These cannot be a per-pixel function: each needs a neighbourhood. The cheap form, **if the global filters are linear in their amount** (local contrast as `in + amount * (in - base)`), is a **detail layer** computed once per frame (`in - base`, from the blur the global pass already makes) that each mask scales by its own amount and weight: `rgb += weight * amount * detail`. One extra texture read per mask pixel; the blur is computed only when some mask uses the control (a dirty flag), so images without such masks pay nothing.

**Not yet verified**, and it decides the slice: whether Clarity (guided filter, RFC-0010) and Dehaze (transmission estimate, RFC-0011) are linear in amount the way Texture is. If not, the fallback is `mix(rgb, filtered(rgb), weight)` where `filtered` is the global filter computed at a **unit amount** once per frame, blended by weight with the amount as a mix factor: not identical to the global at partial amounts, but close, and it is stated as such. **First step of 0b: read the two filters and write the probe** (Rust, ignored test: global at amount a vs `in + a * (filtered_at_1 - in)`, max error) before any shader work.

### 3.3 Not in this RFC

Local **Sharpness** and **Noise Reduction** (the spatial cost is the same as 0b with larger kernels; no starter preset needs them); Vibrance (not a global control here); local Tone Curve, HSL, Grain, Vignette; raising the eight-mask cap.

## 4. How it is judged

- **0a**: the invariant test per control (weight-1 mask equals global), CPU and GPU parity on the existing harness for each new control, old stacks render unchanged (existing mask tests pass untouched), a screenshot in the real window of a mask with each new group, interactive time with eight masks on the real GPU.
- **0b**: the probe's maximum error in sRGB levels at amounts 25 / 50 / 100; if under about 1 level, the detail-layer form ships; otherwise the unit-amount blend, with the difference stated in the user guide.

## 5. Slice plan

| Slice | What | Done when |
|---|---|---|
| **0a** | Pointwise six: Rust `LocalAdjust`, WGSL, packing, mask-panel groups, invariant and parity tests | Tests above; real-window check; user guide line |
| **0b-probe** | Read the Clarity / Dehaze filters; the linearity probe | Numbers in this RFC |
| **0b** | Texture, Clarity (then Dehaze) in masks | Per the probe |

## 6. Not verified

- Everything: no code was changed for this RFC. The claim that the mask adjustment can simply become `apply_global_adjustments` is from reading both twins' signatures and orders, not from running them; the first test of 0a is the one that would show it false.
- Windows GPU timing of the larger mask pass.
- 0b's linearity (§3.2).
