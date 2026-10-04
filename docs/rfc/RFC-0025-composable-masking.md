# RFC-0025: Composable masking — add / subtract / intersect within one mask (M6 slice 1)

- Status: Draft for review (design only; no product code changed)
- Date: 2026-10-04
- Relates to: [PRD MILESTONES §M6](../../PRD/MILESTONES.md) ("Composable masking: add/subtract/intersect multiple masks (AI-generated or manual) in one edit"), [RFC-0023](RFC-0023-on-device-ai-inference.md) §5.5 ("no model; mask-engine work in M6's own scope"), [RFC-0009](RFC-0009-page-svelte-state-design.md) (mask store), M3 mask slices 5–7, M4 spot/red-eye

## 0. Scope and limits

M6 slice 0 closed with the sky model falling back to the non-ML range mask (RFC-0023 §5.2), so the first M6 build slice is the one that needs no model: let a single local adjustment be selected by *combining* shapes (e.g. "this radial area, minus the brush-painted face, intersected with the highlights").

**Everything below about today's code was read in the source, not measured.** The design is not prototyped: the product-vs-min/max choice (§3.2) is argued, not compared on photographs, and no GPU or CPU timing for the new loop exists (§8).

## 1. What exists today

- **One mask = one op = one shape + one adjustment set.** `linear_gradient_mask`, `radial_gradient_mask`, `brush_mask`, `luminance_range_mask`, `color_range_mask` each carry their own geometry/range, `feather`, `invert` and `exposure/contrast/saturation`. `spot_mask` (copies pixels) and `red_eye_mask` (redness-gated) are not "weight × adjustment" masks. (`develop.js` `MASK_OP_NAMES`; `develop_engine/masks.rs`.)
- **Masks apply in stack order, each blending into a running `rgb`.** CPU: `apply_edit_stack` Pass 8 loops `masks`, `weight = mask.weight(uv, aspect, rgb)`, `rgb = mix(rgb, local_color, weight)`. GPU: the same loop in `fs_mask` (`gpu/shaders/mask.js`), kinds packed into a `masks[8]` uniform array of 12 floats each. Range masks read the *running* `rgb`, so an earlier mask changes what a later range mask selects — documented as intended WYSIWYG behaviour (`Mask::weight`).
- **Hard budget of 8 masks across all kinds** (`MAX_MASKS`, uniform array and brush texture-array layers share it). Brush and spot masks each own one raster layer, drawn CPU-side into an `OffscreenCanvas` sized to the image's native resolution and uploaded; the layer is keyed by mask id.
- **No way to combine.** The only "composition" is order: two masks adjusting the same pixels both apply. There is no way to say "this region *except* that one", which is the common request (exclude a subject from a sky gradient; limit a gradient to the shadows).
- Presets exclude every mask kind (`PRESET_EXCLUDED_OP_NAMES`), so this feature has no preset-compatibility surface.
- Both renderers must agree: CPU (thumbnails, export, no-WebGPU fallback) and GPU (interactive) are line-for-line twins, with a parity e2e harness.

## 2. Requirements

1. A mask can be built from several shapes combined with **Add**, **Subtract** and **Intersect**, edited and previewed live.
2. Existing edits are unchanged: every stored edit stack renders pixel-identically with no migration.
3. CPU and GPU produce the same weight (same parity bar as grain/lens: a few levels of 255, not bit-exact).
4. Any weight-producing kind can take part: linear, radial, brush, luminance range, colour range. (The M6 AI masks — SAM 2 click-select — will arrive as a brush-like rasterised layer, so they need no new mechanism; see §6.)
5. The 8-mask uniform and brush-layer budgets are not silently exceeded.

## 3. Design

### 3.1 Options considered

| | Shape | Back-compat | Verdict |
|---|---|---|---|
| **A. New `mask_group` op** holding `components[]` and one adjustment set; old kinds stay as legacy | New op next to the seven old ones; UI must handle two models, or migrate | needs a migration or permanent dual model | rejected: two ways to express the same single-shape mask forever |
| **B. `modifiers[]` on the existing weight-kinds** — the existing op is the *base* shape; an optional ordered array of further shapes each with a `combine` | A mask without `modifiers` is byte-for-byte what exists today | none needed | **recommended** |
| **C. Cross-op references** — a mask op says "subtract mask #3" | No new nesting, but every op needs ids to other ops, deletion/reorder invalidates references, and a referenced mask would also apply its own adjustment | none | rejected: ordering and dangling-reference problems for no gain |

### 3.2 Option B: data model and semantics

```jsonc
{ "op": "radial_gradient_mask", "id": "m1", "center": {...}, "feather": 40, "invert": false,
  "exposure": -0.5, "contrast": 0, "saturation": 0,           // adjustments stay on the mask, as today
  "modifiers": [                                              // new, optional; absent = [] = today
    { "id": "m1a", "combine": "subtract", "shape": { "op": "brush_mask", "dabs": [...] } },
    { "id": "m1b", "combine": "intersect", "shape": { "op": "luminance_range_mask", "rangeMin": 0, "rangeMax": 40, "feather": 30, "invert": false } }
  ] }
```

- `shape` is any of the five weight kinds *without* adjustment fields (adjustments are only on the mask). A modifier's own `invert` and `feather` keep their current meaning.
- **Fold, in listed order**, starting from the base shape's weight `w`; each modifier computes its own weight `c` from the *same* `rgb` the mask entered with (so range modifiers are not affected by sibling components, only by *earlier masks*, exactly as a range base is today):

  | combine | weight |
  |---|---|
  | add | `w + c − w·c` |
  | subtract | `w · (1 − c)` |
  | intersect | `w · c` |

  Order matters (`(A ∪ B) ∖ C ≠ A ∪ (B ∖ C)`); the panel lists components in evaluation order.
- **Why the product family and not max/min.** For hard (0/1) weights every convention agrees. For feathered weights, `max`/`min` have a crease where the two soft edges cross and make `subtract` need its own formula; the product forms are smooth, treat weights as opacities, and are mutually consistent (`subtract` is intersect with the inverse of `c`). The cost: intersecting two soft edges is dimmer than `min` (0.5 × 0.5 = 0.25). **This is argued, not compared on photographs** — §8.
- **Eligible kinds:** the base must be one of the five; spot and red-eye take no modifiers (not weight-gated adjustments).
- A mask with `modifiers` and the global `invert` on the base still means "invert the base shape *then* fold" (consistent: each component inverts itself). A whole-result invert is not offered (use Subtract) — open question §7.

### 3.3 Renderer changes

- **CPU** (`masks.rs`): extract each kind's weight into a `shape weight(uv, aspect, rgb)` the existing `Mask::weight` calls, add `modifiers: Vec<(Combine, Shape)>` to the five weight structs, and fold in `Mask::weight`. Brush modifiers parse their own dabs; `parse_masks` stays the single entry point. `local_color` is unchanged (adjustments stay on the base).
- **GPU** (`mask.js`, `renderFrame.js`, `common.js`): move the inline weight code for kinds 0–4 into one `component_weight(c, uv, rgb)` function; `fs_mask` computes the base via it, then loops the group's modifiers over a **second uniform array** (`mods[MAX_MODS]`, one `Mask`-shaped record plus a `combine` code each). The group record needs `mod_start`/`mod_count`; `adjustments.w` is free on kinds 0–4 but a clean 16-float stride is preferable to packing integers into it (decided at implementation; the existing WGSL concatenation test in `gpu/shaders/index.test.js` should be extended to cover it).
- **Budgets.** Modifiers get their own `MAX_MODIFIERS` (proposed 16 across the image; uniform cost 16 × ~48 B) so combining never eats mask slots. **Brush modifiers share the existing 8 brush layers** with brush and spot masks (the raster layer is keyed by `modifier.id` instead of `mask.id`); the UI shows the remaining count and disables "add brush" at the cap. Raising the layer count is a memory decision (each layer is image-native resolution) and is deliberately not made here.
- **Overlay.** The selected-mask coloured fill (today for kinds 2–4 only) should show the *combined* weight when a mask has modifiers — otherwise the user cannot see what they are building. The fill is drawn from the same final `weight` already computed in the loop, so this needs no new pass.

### 3.4 UI (slice 1b)

`MaskEditorPanel` gets a **Components** list under the selected mask: the base shape first, then each modifier as a row (kind icon, combine chip Add/Subtract/Intersect, eye-toggle, delete), plus an **Add component ▾** menu offering `Add / Subtract / Intersect with →` {Linear, Radial, Brush, Luminance range, Colour range}. Selecting a row selects that *component* for editing: its handles/brush/range controls appear exactly as for a stand-alone mask today. The selection becomes `(maskId, componentId | null)`; `MaskStore.selectedMask` keeps returning the mask, a new `selectedComponent` returns the component. This is the largest piece of UI work and is its own slice.

Mock: [composable-masking-mockup.html](../ux/mockups/composable-masking-mockup.html) (interactive; open in a browser). It uses the app's real tokens and the floating panel's real geometry (210 px, top-left), and draws the selected-mask fill with the engine's real fold, so subtract/intersect show what slice 1a renders. Walkthrough: Radial → *Add shape* → Subtract brush → Intersect gradient → pick another row. Decisions the mock fixes (review should confirm or change):

1. **A "Shapes" section in the existing floating panel**, not a new panel: base shape row (labelled *base*, no combine chip, not deletable), then one row per modifier with an inline combine select (+ Add / − Subtract / ∩ Intersect, colour-coded) and ×. Mask-level Exposure/Contrast/Saturation stay above and apply to the combined selection.
2. **Add shape = mode switch + one short list.** A flat grouped menu (3 modes × 5 shapes = 15 entries) was tried first and overflows the panel; the segmented mode (remembered between uses) above five shapes fits and is two small decisions.
3. **One shape is edited at a time**: the selected row's own controls appear under the list and only its handles are drawn on the canvas; the rest stay quiet.
4. **Caps are visible** in the section header (`n / 16 · brush n / 8`) and Add entries grey out with the reason.
5. **Left out of the mock, to decide in 1b:** painting/arming a brush shape, the colour-range click, handle dragging, reordering (row order is evaluation order; no drag yet), an enable/disable eye per row (the §3.4 sketch above lists one but the 1a data model has no `enabled` field), and an *Invert result* toggle (§7.2).

## 4. Slice plan

| Slice | Content | Verifiable how |
|---|---|---|
| **1a (engine, no UI)** | Data model + validation in `develop.js` (`addModifier`/`updateModifier`/`removeModifier`, budget checks), Rust fold + parity-tested kernels, WGSL `component_weight` refactor + modifiers loop, brush-layer keying, overlay uses the combined weight | `cargo test`: fold hand values (add/sub/intersect at 0, ½, 1; ordering; hard-mask equals set algebra); **stack without `modifiers` renders identically to before** (pinned golden on the existing mask tests); vitest for the pure model; real-GPU probe comparing `fs_mask` with the CPU path on a modifiers scene (the method used for grain and lens) |
| **1b (UI)** | Components list, Add/Subtract/Intersect menu, component selection, per-component handles | browser harness with the real panel; e2e scenario for CPU/GPU parity with one subtract |
| **1c (polish)** | History entry names (`Mask: subtract brush`), copy/paste & snapshots round-trip, keyboard, docs/USER_GUIDE | — |

An RFC-update (corrections) and an ADR-free close, per the M5 practice for GPU-heavy slices.

## 5. Risks

1. **Uniform/shader limits.** The fs_mask loop is already long, and a nested loop adds register pressure; the WGSL must compile on both Metal and D3D (reserved-word bugs like `shared`/`common` in the grain slice were only caught by running the real shader). The GPU probe is mandatory, not optional.
2. **Order-dependent range masks.** A range *modifier* reads the `rgb` entering the mask, a range *base* reads the same. This is consistent, but a user who expects "intersect with highlights" to see the result of this mask's own adjustment will not get it. Document it.
3. **Brush layer budget** (8 shared) is already tight for brush users; modifiers can exhaust it quickly. The cap is shown, not hidden.
4. **Edit-stack compatibility the other way.** Older app builds ignore `modifiers` and would render the base shape alone. There is no downgrade story today for any new field; stated here so it is a conscious choice.
5. **Copy/paste and snapshots** carry masks as opaque JSON, so modifiers should round-trip, but id uniqueness for modifier ids on paste (M4.5 `applyPresetOps` path) is unchecked — slice 1c.

## 6. How this serves the AI masks

SAM 2 click-select (RFC-0023 §5.1) yields a raster mask per click with three candidates. Landing it as a brush-like rasterised layer lets it be a base shape or a modifier through exactly this mechanism, so "click subject, subtract the face" needs no second feature. The `ml`/`ort` runtime itself is a separate slice (ADR-0009 stays Proposed until then).

## 7. Open questions (need a decision or a measurement)

1. **Product vs min/max** for add/intersect — measure on a real photograph with two soft edges overlapping before the 1a kernels are frozen; the choice is cheap to change only before stored edits exist.
2. **Whole-result invert** on a composite — add a single `invertResult` or rely on Subtract?
3. **Per-component adjustments** (LR allows only per-mask adjustments, so assumed out) — confirm.
4. **Modifier cap** (16) and whether to raise the brush-layer cap; needs a native-resolution memory figure for the largest supported frame.
5. Should **spot/red-eye** ever combine (e.g. restrict a heal to a region)? Assumed no.

## 8. What this RFC does not establish

No prototype, no timing (CPU per-pixel cost of the fold, GPU cost of the nested loop at 8 masks × 16 modifiers), no visual comparison of the combine formulas, no e2e. All source facts in §1 are from reading the code on `main` at b5b9b68; line-level claims should be re-checked when slice 1a starts.
