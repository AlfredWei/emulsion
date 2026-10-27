# RFC-0035: Region-bound preset ops: how an adaptive preset is resolved, applied and reported (M7 slice B of RFC-0033)

- Status: Draft for review (2026-10-27). **Nothing is built and nothing in §3 is measured on the product**; the numbers come from RFC-0033 §9-§10, which are by-eye looks at small sets.
- Companion documents: [RFC-0033](RFC-0033-adaptive-tone-presets.md) (scope, resolvers measured), [RFC-0034](RFC-0034-richer-local-adjustments.md) (the mask adjustments this uses), [RFC-0026](RFC-0026-click-select-segmentation.md) (`segment_mask`), [RFC-0032](RFC-0032-ai-object-removal.md) §10 (stored-result rule), [PROGRESS.md](../../PROGRESS.md)

## 1. Problem

RFC-0033 chose adaptive presets: a preset whose operations are bound to a **kind of region** ("the sky", "the faces", "the shadows") that is **found per photograph**. Slice 0 (RFC-0034) made the adjustments exist inside a mask; slice A (RFC-0033 §9-§10) looked at whether the regions can be found. This note decides what an adaptive preset *is* in the code, how a region becomes a mask on one photograph, what the user is told, and how it is undone and re-run. It is the design for slice B and stops before the editor UI (slice C).

## 2. What is already there (read from the code, 2026-10-27)

- **A preset is a stored edit stack with masks removed.** `create_preset(name, stack)` keeps whatever JSON it is given; JS's `presetEligibleOps` drops crop, lens, perspective and **every mask op** (`PRESET_EXCLUDED_OP_NAMES`), `applyPresetOps` upserts the remaining ops by `op` name. Rust never interprets a preset's ops. So **today a preset cannot carry a mask at all.**
- **`segment_mask` is a stored raster.** RFC-0026 §3.2: an 8-bit gray PNG, base64, **exactly 256 x 256** over the whole uncropped frame (`SEGMENT_SIZE` in `develop_engine/masks.rs`; a PNG of any other size is rejected), logits quantised onto 0..255 (logit in [-4, 4]), plus the prompts that made it. It renders on both twins, takes the same adjustments as any mask (RFC-0034: nine controls and Texture), and shares the **8 raster layers** with brush masks. So a resolved region can be stored as a `segment_mask` without a new mask kind. That settles RFC-0033 §10.1's question about groups of more than eight faces: **one raster holds any number of faces**.
- **Modifiers are capped at 16 across the image** (`MAX_MODIFIERS`) and cost per pixel for every one, so a face per ellipse modifier is not an option for a group of 17 to 59.
- **Masks apply in stack order** and a stored mask is never re-derived on open: the stored-result rule (RFC-0032 §10) is already how `segment_mask` works.
- A **labelled flush** writes a stack and a History row; an edit made as one flush is one undo step (RFC-0028's Auto Sync relies on the same).

## 3. Proposal

### 3.1 What an adaptive preset is

An **adaptive preset** is a list of **region rules**, each `{ region, adjustments, feather }`, plus an **Amount** (0-100 %, default 100) chosen at apply time. `region` is one of `sky`, `face`, `shadows` for the first cut (`subject` and `background` need a click on the segmenter, §3.5). `adjustments` is the mask's own adjustment object (RFC-0034's nine controls and Texture), so a rule is exactly what a mask panel could hold.

**Storage: no schema change.** The five starter presets are **constants in JS** (RFC-0033 §3.3's table, start values to be judged). A user-authored adaptive preset (slice C) is stored in the existing presets table as a stack holding one `adaptive_region` op per rule; that op is understood only by the apply step in §3.3 and **is removed before anything is written to a photograph's stack**, so the renderer never sees it and Rust is unchanged. `presetEligibleOps` keeps excluding masks, so a preset made from a photograph never captures the masks it resolved.

### 3.2 Resolving a region on one photograph

A resolver takes the photograph's preview and returns **`found(mask)`, `check(mask, why)` or `not_found(why)`** (RFC-0033 §3.1b). It lives in Rust (`adaptive_regions.rs`) because the face rows and the detector are there, and it runs outside the render path.

| Region | Method | Output mask | `not_found` when | `check` when |
|---|---|---|---|---|
| `shadows` | percentile of the photograph's own luma (the darkest 25 % by default, never above a luma code of 64 of 255 without being asked) | the existing `luminance_range_mask` (stored geometry, no raster) | the dark share after the cap is under 3 % **or** the frame is mostly shadow (more than 50 % under code 64: RFC-0033 §10) | the share is 3-8 % |
| `face` | the stored `faces` rows (detect first if `faces_scanned` is 0), **identical boxes dropped** (RFC-0033 §10: the catalog holds duplicate rows), boxes below a **pixel floor** (about 24-32 px side at a 1,600 px analysis size, RFC-0033 §10.1), each enlarged to a feathered ellipse (hair and neck are outside the tight box) | a `segment_mask`: all ellipses drawn into one 256 x 256 field, any number | no face above the floor | any kept face scores under 0.75, **or** a face covers over 40 % of the frame (statues, dogs and teddy bears were the false positives, RFC-0033 §10.1) |
| `sky` | the RFC-0033 §9 heuristic ported to Rust (smooth top-connected region, colour-step-limited growth, clouds, hole fill) at a 300 px working size | a `segment_mask` from the grown region, soft-edged | the region is under 2 % of the frame, or touches under a quarter of the top edge | 2-8 % of the frame, or ragged (many holes), or the photograph is dark (night, fog: where RFC-0033 §9 found 0.53-0.65 IoU against the segmenter) |

**Limits that go with the raster:** 256 texels over the frame means a 41 px face in a 1,600 px picture is about 6 texels wide and a 0.1 % face in a 24 MP frame about 8; its mask is a soft blob, which suits a +0.3 EV lift and would not suit a hard cut. Whether the fixed size must grow (a separate size for resolver masks) is decided by looking at real group photographs in B1, not guessed here.

**Every number in the table is a starting value to be judged on photographs in B1**, from RFC-0033 §10's small sets. The outcome words are the contract; the numbers are tuning.

### 3.3 Applying a preset to a photograph

1. The apply step resolves each rule. **Only `found` and `check` rules add a mask; `not_found` rules add nothing and are listed in the report.** A preset with no found rule changes nothing and says so.
2. For each added mask: a `segment_mask` or `luminance_range_mask` with the rule's adjustments **multiplied by Amount** (Amount is applied once, at apply time; the numbers written are the real ones the mask panel shows, so the user can fine-tune them as any mask). All pointwise adjustments and Texture are exactly linear in strength (RFC-0034 §3.2), so Amount is exact for them.
3. The masks are appended in the rule order in **one labelled flush**, History row **"Adaptive: Deepen sky 60 %"** (one undo step; skipped rules appear in the row's detail, not a second row). Existing global ops and other masks are untouched.
4. **Never silently re-resolved** (RFC-0032 §10): opening the photograph later reads the stored masks. **"Re-apply"** is an explicit action that resolves again and replaces the masks the preset made (they carry `source: { preset, rule }` so the replace finds them, and a mask the user has edited asks first).

### 3.4 What the user is told (the three outcomes)

The apply reports per rule: **found** ("Sky: 38 % of the frame"), **check** (applied and flagged, with the reason and "Select the sky yourself" opening the manual route: a Luminance Range mask or one Select Subject click, the preset's adjustments moving to it), **not found** (skipped, with the reason). A hover shows the mask as the existing selected-mask overlay. The wording and layout are slice C's UX mock; B delivers the data (`outcome`, `coverage`, `reason` per rule) the UI will show.

### 3.5 What is not in this slice

- **`subject`, `background`, `person`** (they need the segmenter: GPU only, one click or faces-to-clicks): not built. A preset that asks for them is marked "needs a click" and "needs GPU acceleration", per RFC-0033 §3.1.
- **Editor UI**, hover preview, the five starter presets' wording: slice C, after a mock.
- **Batch**: B3, below.

## 4. How it is judged

- **Resolvers (B1):** the Rust port must agree with the numpy prototype (`skyheur.py`) on the 45 sky photographs within a stated mask difference and with the stored-face rows on the 37 + 60 face photographs; every number in §3.2's table is looked at again on those sets **with the overlays from the app's own code**, and the table is corrected to what is seen. The group photographs (17 to 59 faces) are judged by eye at 100 % for blobs and halos.
- **Apply (B2):** a Rust/JS test per outcome; Amount 0 changes nothing and Amount 100 equals the rule's numbers; undo removes exactly what the apply added; an edited mask is not replaced without asking.
- **In the real app (before anything is called done):** Deepen sky on three sky photographs and Brighten faces on a portrait and a group, before / after crops looked at; **CPU and GPU agreement of the stored raster** in the real window (RFC-0034 could not run that here: the WebDriver-driven WKWebView did not render masks on the GPU, so it is recorded as a known gap, not assumed).

## 5. Slice plan

| Slice | What | Done when |
|---|---|---|
| **B1 resolvers** | `adaptive_regions.rs`: shadows, face (dedupe, floor, ellipses into the raster), sky (port); a command returning `outcome / coverage / reason / mask`; tests | §4 first bullet; the table corrected |
| **B2 apply** | JS apply (Amount, ordering, labelled flush, one undo, re-apply, source tags), starter presets as constants, **no UI beyond a temporary menu row for trying them** | §4 second and third bullets |
| **B3 batch** | Library "Apply Adaptive Preset to Selection": per-photo resolve off the UI thread, a report listing flagged and not-found photographs, nothing hidden | A batch of the 37 photographs with a report that matches by-eye |
| **C** | UX mock first, then UI (RFC-0033 §6) | separate |

## 6. Open questions for the user

1. **Raster, not a new mask kind, for faces and sky** (§2, §3.2): recommended; the cost is the fixed 256 x 256 field's coarse edges. The alternative (a many-ellipse mask kind) needs new Rust, WGSL and packing for a mask that is blobby anyway.
2. **Starter presets as constants first, user-authored later** (§3.1): recommended; it avoids a preset-schema decision before anyone has used one.
3. **`check` applies the mask and flags it** (§3.4) rather than asking first; recommended because Undo is one step, but a batch may apply hundreds of `check` masks the user must then review.

## 7. Not verified

- Everything in §3 is design. The code facts in §2 are from reading `develop.js`/`presets.svelte.js`/`masks.rs` and RFC-0025 / RFC-0026, not from running them.
- Every threshold in §3.2 (floors, 0.75, 40 %, 2 % / 8 %, the 25th percentile) is a guess from RFC-0033 §9-§10's small sets.
- Whether a 256-texel raster is good enough for small faces in groups.
- Windows; any photograph of the user's own beyond RFC-0033 §10's first look.
