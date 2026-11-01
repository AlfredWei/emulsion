# RFC-0035: Region-bound preset ops: how an adaptive preset is resolved, applied and reported (M7 slice B of RFC-0033)

- Status: §6's three questions decided (2026-10-27: raster, constants first, `check` applies and flags). **B1 (the resolvers) and B2 (the apply step, with a temporary menu row) are built; B1 was measured on photographs and B2 run in the real window (§8, §9)**; B3 and C are design only.
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

The apply reports per rule: **found** ("Sky: 38 % of the frame"), **check** (applied and flagged, with the reason and "Select the sky yourself" opening the manual route: a Luminance Range mask or one Select Subject click, the preset's adjustments moving to it) (built: the first Select Subject click replaces the flagged mask and keeps its adjustments as they stand then, minus the preset's `source`, so a Re-apply leaves it alone; Esc restores the flagged mask; the Luminance Range route is not offered; **for a flagged `person` mask the click is added instead (2026-11-01):** the button reads "Add a person yourself", the click becomes an *add* shape of the flagged mask, which keeps its id, its adjustments and the people the model found, Esc takes the shape off, and the mask now counts as edited so a Re-apply asks first. Measured on a five-person mask (11 % of the frame): replacing it with one click left 0.2 % of the frame, the other four people gone), **not found** (skipped, with the reason). A hover shows the mask as the existing selected-mask overlay. The wording and layout are slice C's UX mock; B delivers the data (`outcome`, `coverage`, `reason` per rule) the UI will show.

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
| **B1 resolvers** (built, §8) | `adaptive_regions.rs`: shadows, face (dedupe, floor, ellipses into the raster), sky (port); command `resolve_adaptive_region` returning `outcome / coverage / reason / mask`; tests | §4 first bullet; the table corrected |
| **B2 apply** (built, §9) | JS apply (Amount, ordering, labelled flush, one undo, re-apply, source tags), starter presets as constants, **no UI beyond a temporary menu row for trying them** | §4 second and third bullets |
| **B3 batch** | Library "Apply Adaptive Preset to Selection": per-photo resolve off the UI thread, a report listing flagged and not-found photographs, nothing hidden | A batch of the 37 photographs with a report that matches by-eye |
| **C** | UX mock first, then UI (RFC-0033 §6) | separate |

## 6. Open questions for the user

1. **Raster, not a new mask kind, for faces and sky** (§2, §3.2): recommended; the cost is the fixed 256 x 256 field's coarse edges. The alternative (a many-ellipse mask kind) needs new Rust, WGSL and packing for a mask that is blobby anyway.
2. **Starter presets as constants first, user-authored later** (§3.1): recommended; it avoids a preset-schema decision before anyone has used one.
3. **`check` applies the mask and flags it** (§3.4) rather than asking first; recommended because Undo is one step, but a batch may apply hundreds of `check` masks the user must then review.

**Decided 2026-10-27: all three recommendations accepted.**

## 7. Not verified

- Everything in §3 is design. The code facts in §2 are from reading `develop.js`/`presets.svelte.js`/`masks.rs` and RFC-0025 / RFC-0026, not from running them.
- Every threshold in §3.2 (floors, 0.75, 40 %, 2 % / 8 %, the 25th percentile) is a guess from RFC-0033 §9-§10's small sets.
- Whether a 256-texel raster is good enough for small faces in groups.
- Windows; any photograph of the user's own beyond RFC-0033 §10's first look.

## 8. What B1 found (2026-10-27)

Built: `app/src-tauri/src/adaptive_regions.rs` (pure resolvers, 13 unit tests, two ignored evaluation harnesses) and `adaptive_commands.rs` (`resolve_adaptive_region(image_id, path, content_hash, region)`; nothing in the UI calls it yet). All numbers here are by-eye and overlap measures on the local photo sets, not in the running app.

- **Sky: the port is faithful, the method is not good enough to be `found`.** Against `skyheur.py` on the 45 sky photographs the Rust port has mean IoU 0.997 (worst 0.924). Against the segmenter's masks 12 of the 33 masks it called `found` overlapped under 0.7, and looking at them: photos 8, 21, 36, 37 are patchy blotches inside obvious sky, 30 takes a hazy green hillside for sky, 26 (night) takes the black ground, 10 includes the water reflection; photos 22, 23, 25, 29 (bare branches, one night) were `not_found` though there is sky (a safe miss). A per-column contiguity measure and the clouds/hole shares were tried as flags and none separated the bad masks from the good ones, so no threshold was fitted to 45 photographs. **Decision implemented: a heuristic sky is never `found`; it is always `check`** ("estimated without a model: look before keeping it"), which under §6.3 applies the mask and flags it. A model-based sky is the way to make `found` possible, and is not in this slice.
- **Shadows behave as designed.** Looked at on the 45 photographs: the overlay lands on dark foregrounds, trunks and water; the ten night/low-key frames are `not_found` with "n % of the frame is already deep shadow" instead of tinting everything (31 found, 4 check, 10 not found). The band's soft edge (`feather` 30) is a guess to be judged in B2 on real edits.
- **Faces: one raster holds a 59-face group cleanly**, with soft ellipses reaching hair and neck, and the smallest faces in the groups are covered. On the 37 full-resolution photographs: 25 found, 4 check, 8 not found (six have no face). Two corrections to the table from this: **(a)** the check is on the mask's own coverage (>40 % of the frame), not on the tight box's area, because the statue (58 %) and the dog (48 %) slipped under the box rule while real close-up portraits stay at 25-29 %; **(b)** a weak detector score (<0.75) flags only when there are at most two faces: with scores present nearly every large group was flagged by one stray blob.
- **The catalog stores no detector scores**, only the box. So in the app a weak-score check never fires and the dog or the statue is caught only by coverage; the score rule exists for callers that have scores. Storing the score would need a schema change and a re-scan: not done.
- **Not looked at:** the 60 low-resolution COCO photographs (640 px frames put most faces under the pixel floor by construction); person (no resolver); how an adjustment looks on these masks in the running app; CPU/GPU agreement of the stored raster in the real window (still the known gap from §4); duplicate face rows in a real catalog (the dedupe is tested on synthetic rows only); whether the Develop preview and the face detector's source decode share the same orientation (a rotated photo would put the ellipses in the wrong place: to be checked in B2 on a portrait-orientation file).

## 9. What B2 found (2026-10-27)

Built: `adaptivePresets.js` (pure: Amount scaling, ordering, the 8-mask cap, replace the preset's own earlier mask, keep an edited one), `actions/adaptiveActions.js` (face scan first when the photo has not been scanned, a guard against the photo changing while Rust looks, one labelled flush, one-at-a-time), `api/adaptive.js`, and a temporary "Adaptive (trying out)" block (Amount slider and one button per starter) in the History panel's Presets tab. The three starters are JS constants: Deepen sky, Brighten faces, Cool the shadows. Lift subject and Soften background need the segmenter and are not there.

- **Unit-tested:** 17 new vitest cases (Amount 0 changes nothing; 60 % scales -0.3 EV to -0.18; a found mask is added after the existing ones; a check mask is applied and described as "check it"; not-found adds nothing and says why; re-apply replaces rather than stacks; an edited mask is kept and the rule reported as not applied; the 8-mask limit; one flush under "Adaptive: Cool the shadows 60 %"; faces scanned first only for a preset with a face rule; a photo switch mid-run writes nothing; a second request while running is ignored; a failure is reported). Full vitest 1,146 pass, svelte-check 0 errors.
- **Real window** (macOS debug build, WebDriver, throwaway HOME, new `develop-adaptive-presets.e2e.js`, 5 / 5): Cool the shadows at 60 % writes one luminance-range mask with Temperature -4.8 and Saturation -3 and exactly one new History row with the expected label; applying again at 100 % replaces it (still one mask, Temperature -8); Deepen sky writes a segment mask with Exposure -0.3 / Contrast +12 whose stored raster decodes to 256 x 256 in the browser; the CPU render of the stack differs from the unmasked one; on a portrait the face resolver reads the real catalog rows after a real scan: 1 face, found, 18 % of the frame. Looked at: on the corn-field photo the Deepen-sky change is gentle (at most 20 of 255 levels), follows the ridge line cleanly, and leaves the two large cumulus clouds alone (the heuristic's known miss).
- **Not verified:** the GPU path of these masks in the window (the WebDriver WKWebView does not render masks on the GPU here: the known gap), so what the user sees in the canvas was not checked, only the CPU render; the notification text in the real window (asserted in unit tests only); undo of the apply in the window (it is one History row by the test, not exercised); the face case on a group or a rotated photograph (orientation agreement between the Develop preview and the detector's decode is still unchecked); batch (B3); any visual judgement beyond one photograph.
- **Limitation, decided 2026-10-28, closed by RFC-0036 R3 (2026-10-29; bounded in RFC-0036 §4.1):** shadows are resolved on the **unedited** preview, but a luminance mask reads the image after the global edits and every earlier mask, so the stored range moves with the user's edits. The requirement is that a resolved region selects the same pixels whatever the user does afterwards; resolving on the edited preview is rejected, and storing a 256 x 256 raster was measured and is too coarse (RFC-0036 §3). **[RFC-0036](RFC-0036-original-basis-luminance-masks.md)** adds a `basis: "original"` to the luminance mask (measured on the un-graded, geometry-corrected image on both renderers). The shadows resolver now stores that basis, so the preset's selection no longer depends on the order of edits (within RFC-0036 §4.1: geometry, preview vs full resolution, AI denoise).
