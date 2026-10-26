# RFC-0033: Adaptive and context-aware tone: what it could mean here, and what to build first (M7 slice 3)

- Status: Draft for review. **A scoping RFC: it frames the question, reads what is known, and proposes an order. It builds nothing and measures nothing on photographs yet.** User decisions of 2026-10-26 are in §8; they changed the order (adaptive presets first, better Auto later) and removed the personal-learning reading.
- Date: 2026-10-26
- Relates to: [MILESTONES.md](../../PRD/MILESTONES.md) M7 ("adaptive / context-aware auto-tone presets"), [RFC-0026](RFC-0026-click-select-segmentation.md) (subject and sky masks), [RFC-0028](RFC-0028-context-menus-and-develop-multiselect.md) (presets, Copy / Paste Settings, what is excluded from them), [RFC-0031](RFC-0031-intelligent-culling.md) §5 ("adaptive auto-tone presets want their own RFC after culling"), [RFC-0032](RFC-0032-ai-object-removal.md) §3.2 and §10 (stored vs regenerated results, which applies here too)

## 0. Process note

Written the way RFC-0023 and RFC-0032 were: first find out what the words can mean and what each meaning costs, then decide with the user. Web facts here come from search results, not from reading the primary sources in full (§2 marks each); **nothing was run on photographs**.

## 1. Problem

M7's scope line is "adaptive / context-aware auto-tone presets". Today the app has:

- **Auto Tone** (`computeAutoTone` in `api/develop.js`): from the rendered luma histogram's percentiles, it sets Exposure (median to 0.45), Whites, Blacks, Contrast, Highlights, Shadows with fixed targets and clamps. It is a good, explainable baseline, **blind to content**: it cannot tell a sunset, a snow scene, a night street or a backlit portrait, so it brightens the dark ones it should not and flattens the contrast it should keep.
- **Auto White Balance** (gray-world): the same kind of blindness.
- **Presets** (RFC-0028): a saved list of edit ops applied with `applyPresetOps`. **AI-mask ops cannot be in a preset**: a subject / sky mask is a raster for one photo, so it is excluded from presets and Copy Settings (PROGRESS: the segment mask is "preset-excluded"). A user cannot have "darken the sky, lift the subject" as one reusable thing.
- **Subject and sky masks** (RFC-0026), already on the device, already composable.

The gap is two separate things that the milestone line joins:

1. A **better one-click Auto** (understand the picture, then choose the global edit).
2. **Presets that act on what is in the photo** (the sky, the person, the background), not on the whole frame or on a shape drawn for one photo.

## 2. What "adaptive" can mean, and what is known

| Reading | What it is | Known cost and risk |
|---|---|---|
| **R1. Scene-aware heuristics** | Classify the picture (high key, low key, backlit, night, snow, sunset ...) from histogram shape and cheap statistics, then pick the targets. Still sliders. | No model, no download, explainable, testable. Quality depends on the rules; **only a user's judgement says whether it is better** than today's Auto. |
| **R2. Adaptive presets** | A preset's local adjustments are bound to a *kind of region* ("sky", "subject", "person", "background") and resolved to a mask when applied to each photo. This is how Lightroom Classic's Adaptive Presets (Portrait, Sky, Subject groups) work, by its own help and third-party descriptions: the preset runs an AI mask, then applies its settings to it, which is why previewing one takes a moment. | **Uses models we already ship** (RFC-0026). New: a way to store a region binding in a preset, resolve it per photo, and say plainly when it finds nothing (no sky in an indoor photo). The mask is a per-photo result (§3.2). |
| **R3. A learned auto-enhancer** | A network predicts the edit. Lightroom's own Auto is described as using Adobe's Sensei models; academic examples are image-adaptive 3D lookup tables (AdaInt, CVPR 2022: code Apache-2.0 per its repository, a 5-layer backbone on the FiveK set, real-time). | **Two problems that are not engineering ones.** (a) The *output is a colour lookup, not sliders*: the edit becomes one opaque op the user cannot adjust with the controls they know, or must be fitted back to sliders (lossy). (b) **Training data**: the usual set (MIT-Adobe FiveK) has an unclear licence in what was found (mirrors label it "adobe-mit" or "Other"; the official terms were **not** retrieved); a model trained on someone's retouching needs the same licence reading RFC-0032 §9.4 asks for Places2. |
| **R4. Personal adaptive** (**dropped by the user, §8**) | Learn from **the user's own finished edits** (on this machine, never uploaded): for a new photo, find photos like it that the user edited, and propose their typical slider values. "Edit like I usually do." | No model download and no third-party data licence; **it needs the user's edits** (hundreds of photos to be useful) and **a measurement of whether it beats the fixed rules** before anything is built. Privacy-clean, and not something Lightroom does. |

Not in the table, and not proposed: cloud analysis (PRD: no cloud), generative colour grading, and matching any product's proprietary "Auto".

## 3. What this RFC proposes

**Decided with the user (§8): adaptive presets (R2) first; a better Auto Tone (R1) later; personal learning (R4) dropped; the learned enhancer (R3) not now.**

### 3.0 What the code has, which changes R2's size (read from the app, 2026-10-26)

Three facts found while checking the first draft against the code; all three make R2 bigger than "bind a preset to the masks we have":

1. **A local mask adjusts only three things: Exposure, Contrast, Saturation** (`SegmentMask`, and the other mask ops, carry exactly those). Lightroom's adaptive presets move Temperature, Tint, Highlights, Shadows, Clarity, Texture, Dehaze, Sharpness and Noise inside a mask. Most of the starter set (§3.3) cannot be expressed with three sliders.
2. **There is no automatic "select the sky" or "select the subject".** The segmenter is a click-to-select tool (SAM 2: the user clicks; RFC-0026 §4 lists "sky detection" and "automatic whole-image select all subjects" as out of scope, and RFC-0023 found auto-picking the top candidate chose the wrong level in 2 of 6 tries). Sky is today a manual Luminance / Color Range mask.
3. **A face detector exists** (RFC-0005, on device, already run after import): boxes and landmarks per face. It can seed a segmentation click, so *person* can be automatic where *subject* cannot.

### 3.1 R2: region-bound presets

Shape (unchanged in spirit): a preset op carries a **region binding** instead of a mask raster; **apply** resolves it for each photo, writes the ordinary per-photo mask into the edit stack, records what was bound in History, and says so plainly when nothing is found. Details:

- **Region kinds and how each is resolved** (the part the first draft got wrong by assuming one segmenter does all):

| Kind | Resolver | Needs a model | Confidence |
|---|---|---|---|
| `sky` | A composed **colour / luminance range mask** from the photo itself (bright, blue or warm-gradient, connected to the top edge), the manual recipe RFC-0026 recommends, automated. Measured on labelled photographs in slice B | No | **Measured (§9): good on open skies, poor through branches, on night and fog; not reliable unattended.** Needs a "what was found" check (§3.1b) |
| `person` | **Face boxes (RFC-0005) to segmentation clicks** (a point on the face and one on the torso below it) through the existing SAM decoder, one mask per detected person, merged | Yes: the Select Subject model (SAM 2 Tiny, **already bundled in the installer** and seeded on first launch; RFC-0026 §3.5 proposed a download, `models/bundled-models.json` ships it). The segmenter needs GPU acceleration (the tool is disabled without it), so on a machine without one the kind is unavailable and the preset says why | Good where a face is found; none where not |
| `face` | The face box and landmarks only (an ellipse), no segmenter; for "brighten faces", where the edge does not need to be exact | No | Good; limited to what the detector finds |
| `subject` | **Not automatic in this RFC.** Resolves to `person` when a person is found; otherwise the preset asks the user to click the subject once on the open photo (a one-click Select Subject), and cannot be applied to a batch | Yes | Honest: a click is not "automatic", and the RFC says so |
| `background` | The inverse of `person` or the clicked subject | Yes | As its source |
| `shadows` / `highlights` | A **luminance range mask** (existing), feathered | No | High: it is deterministic |

- **A binding the photo cannot satisfy is skipped and reported** ("No sky found in 4 of 12 photos", "Needs GPU acceleration for the person mask"), never an empty mask. A coverage floor (for example under 2 % of the frame counts as not found) is measured in slice B, not guessed.
- **An Amount slider** scales the whole preset (0 to 100, default 50 to 70). Lightroom's adaptive presets have one, and it is the answer to "too strong on this photo". Presets are written **restrained** (§3.3).
- **Applying to many photos** is a background batch job (the segmenter takes time), cancellable, one at a time like the other helper jobs.
- Masks resolved from a preset are excluded from XMP and from Copy Settings like any AI mask. **The preset itself is a name and numbers**, small and shareable.

### 3.1a Slice 0: richer local adjustments (the real prerequisite)

Without it R2 has almost nothing to adjust. **Extend the mask adjustments** from three to the set the starter presets need: **Temperature, Tint, Highlights, Shadows, Texture, Clarity, Dehaze** (all already global ops with CPU and GPU implementations), and, if the budget allows, **Sharpness** and **Noise Reduction** (the "soften the background" and "protect the sky" uses). The work is in the mask pass: each added adjustment is another term in the WGSL `fs_mask` and its CPU twin, packed per mask, with CPU / GPU parity tests (the `develop-cpu-gpu-parity` harness) and a performance check against M5's ~100 ms interactive budget (more adjustments per mask means more work per pixel in every mask). **This is its own RFC-sized slice and is useful on its own**: it improves every manual mask, not just presets. It is scoped here, designed in its own document.

### 3.1b What a resolver that is sometimes wrong means for the design (from §9)

The sky resolver is right on open skies and wrong in the cases photographers meet often (branches, night, fog). A preset that silently paints a wrong mask is worse than none, so **the apply step shows what it found**: the coverage ("Sky: 38 % of the frame"), the mask as an overlay on hover, and three outcomes: **found** (applied), **not found** (skipped, said so), **check** (applied, but flagged for review when the mask is small, ragged or touches only part of the top edge). A flagged photo offers **"Select the sky yourself"** (the existing manual range mask, or one Select Subject click on the sky) and the preset's adjustments move to that mask. For a batch, the report lists the flagged photos; nothing is hidden.

### 3.2 It follows the stored-result rule of RFC-0032

The mask a segmenter produced for one photo is a result the user may have fine-tuned (RFC-0026's modifier). Per RFC-0032 §10 (same machine, same model: identical; other providers: small differences), the **resolved mask is stored with the photo's edit stack and never silently re-resolved**; "Re-apply preset" is an explicit action. This avoids a preset re-run changing an edit the user already accepted.

### 3.3 The starter set, and what "modern" means for it

The user asked for **all five candidates** and for **a study of modern photography** to inform them. The study is a reading, not a measurement; what the sources found (search results, not primary sources) and what it changes:

- **Restraint.** Several 2026 trend pieces describe a move away from the hyper-polished look (synthetic skin, impossible lighting) toward natural colour, soft tones and minimal retouching, especially for documentary and street work; film grain and faded colour are popular as a counter to the "too perfect" look. **So the presets are written restrained, and every one has the Amount slider** (default well under full). No skin smoothing preset; skin is left as it is.
- **What Lightroom's adaptive set does** (help pages and tutorials): three groups, Portrait, Sky and Subject, a Blur-Background one, Amount slider, previewed on hover; Portrait has component presets (Whiten Teeth, Darken Eyebrows) and some build several masks (one Glamour preset creates seven). **Teeth, iris, lips and eyebrows need a facial-feature segmenter we do not have** (RFC-0005 gives face landmarks, not part masks): they are out of this RFC, and may come as landmark-shaped masks later.
- **What the study could not find**: current practice for dehaze, subject separation and skin-tone calibration (the searches returned nothing on them). The numbers below are therefore **starting values to be judged on photographs in slice C, not derived values**, labelled as that in the preset's description, the RFC-0022 way ("aesthetic choice, no claim to match any product").

The five starter presets, with the adjustments each needs and whether today's three-slider masks could do it:

| Preset | Region | Adjustments (restrained start values, Amount 100 %) | Needs slice 0? |
|---|---|---|---|
| **Deepen sky** | `sky` | Exposure -0.3, Contrast +12, Saturation +10 (no Dehaze: it is not planned for masks) | No: today |
| **Lift subject** | `subject` / `person` | Exposure +0.25, Shadows +15, Texture +8 | Shadows: done (0a); Texture: done (0b-1) |
| **Soften background** | `background` | Texture -35, Saturation -8, Exposure -0.1 (a real blur is out of scope: no blur op exists; Clarity in masks is not built, RFC-0034 §3.2) | Texture: **done (0b-1)** |
| **Brighten faces** | `face` | Exposure +0.3, Shadows +12, Temperature +3 (a little warmth) | Shadows, Temperature: yes |
| **Cool the shadows** | `shadows` | Temperature -8, Saturation -5 | Temperature: yes |

With slices 0a and 0b-1 built, **all five are expressible** (Deepen sky without Dehaze, Soften background with Texture instead of Clarity). What is missing is the region binding (slice B), not the adjustments.

### 3.4 R1 later: a better Auto Tone, judged by a person, not by a metric

Unchanged from the first draft and **deferred by the user**: keep today's Auto Tone as the baseline and fallback; inputs are the histogram and, where masks exist, mask statistics (do not brighten the sky to the subject's median; protect a snow scene's whites); output stays the existing seven sliders; the rule set must fit in a paragraph of the user guide; it ships only if blind pairs say it wins and loses nowhere badly (§5). It reuses slice 0 and the region resolvers.

### 3.5 R4 dropped, R3 not now

**R4 (learning from the user's own edits) is dropped by the user.** For the record, the catalog was looked at (read-only copy, with permission): it holds 24 photographs, 12 with any edit and mostly test edits, so it could not have supported the study anyway. **R3** (a learned enhancer) fits poorly for the reasons in §2 (a lookup output does not fit a slider edit stack; the training-data licence question repeats RFC-0032's); revisit only if R1 is judged not good enough, with a survey in the RFC-0023 manner first.

## 4. Not in this RFC

- Cloud, generative grading, colour grading by reference image, film-stock colour emulation (M5.8's grain only is built).
- **Part-of-face masks** (teeth, eyes, lips, eyebrows, hair), skin smoothing, **a real background blur**, automatic "select all subjects", sky replacement.
- **Auto crop / straighten / framing.**
- **Auto White Balance changes** (the gray-world rule has the same blindness; a separate decision).
- Anything in Library (culling is RFC-0031).

## 5. How it is judged

- **Region resolvers**: on labelled photographs (sky present or absent, person present or absent, the user's own and web photographs, as in RFC-0032's bake-off), the resolver's *found / not found* is right, the **coverage floor** is chosen from the data, and the mask edge is looked at 100 % for halos (the failure RFC-0026's feather and grow exist for). **The sky resolver is the main risk and is measured first.**
- **Presets**: each starter preset is applied to a set of 20 or more photographs across scene types and **looked at by the user** at Amount 100 % and at the default; a preset that looks wrong on a photo type is narrowed or dropped, not shipped with a caveat. Time per photo and for a 30-photo batch.
- **Slice 0**: CPU / GPU parity on every added adjustment, interactive time with eight masks (RFC-0025's budget).
- **R1 (later)**: blind pairs, as before: the same 30 photographs, today's Auto vs the new rules, no labels, win clearly and lose nowhere badly.

## 6. Slice plan

| Slice | What | Needs |
|---|---|---|
| **0** | **Richer local adjustments**: Temperature, Tint, Highlights, Shadows, Texture, Clarity, Dehaze (then Sharpness, Noise) inside masks; its own design document, parity tests, performance check | Nothing from the user. Starts with a short design note on the mask pass |
| **A** | **Region resolvers measured**: `sky` (heuristic) **done, §9**; `face`, `person` (faces to clicks), `shadows` still to do; found / not found, the coverage floor; no UI | Web photographs (the user's too if wanted) |
| **B** | **Region-bound preset ops**: the model, apply, not-found handling, History entry, Amount, batch job, tests | A, and 0 for the non-sky presets |
| **C** | **UX mock, then UI**: the preset editor's "bind to region" control, the five starter presets, hover preview, Amount, "needs GPU acceleration" and "no sky found" states | B; a mock first (the RFC-0028 / 0029 / 0031 / 0032 way) |
| **D** | **R1: scene-aware Auto Tone**, only if blind pairs say it wins | A and 0 |

0 and A are independent and can run in either order; the sky resolver (A) is the thing most likely to change the plan.

## 7. Not verified

- **Everything about quality.** No photograph was processed for this RFC. Whether a heuristic sky mask is good enough, what the coverage floor is, whether the starting values look right, and whether scene rules beat Auto Tone are all open.
- **The three code facts in §3.0** are from reading `develop.js`, `segment.js` and RFC-0026 / RFC-0023, not from running the app.
- **Lightroom's behaviour** is from help pages and third-party descriptions, not from using it here. **FiveK and AdaInt** details were not read in the primary sources (§2). **Modern-photography trends** are from a handful of blog and news pieces, not a survey.
- **Windows**: the segmenter and the mask pass on the Windows runner are as verified as RFC-0026 and RFC-0025 left them.

## 8. Decisions and questions

Answered by the user on 2026-10-26 (first round):

1. **Order: adaptive presets first, a better Auto later.** (§3, §6: R1 is slice D.)
2. **Reading the catalog: yes.** Done, read-only, on a copy: 24 photographs, 12 with edits, mostly test edits (§3.5). Not enough to measure with; web photographs are the plan (slice A).
3. **R4 (learning from the user's own edits): dropped.**
4. **Starter presets: all five candidates, and study modern photography too** (§3.3).

Answered by the user on 2026-10-26 (second round):

5. **Slice 0 (richer local adjustments) is the next step, with its own design note first:** yes. The note is [RFC-0034](RFC-0034-richer-local-adjustments.md).
6. **A `subject` preset that asks for one click is acceptable:** yes (§3.1).
7. **Bundle the Select Subject model with the installer.** **It already is** (SAM 2 Tiny encoder and decoder, 155 MB, listed in `models/bundled-models.json` and seeded on first launch; the release smoke test in PROGRESS confirmed it). This RFC's first draft wrongly called it an optional download, following RFC-0026 §3.5's older proposal; §3.1 is corrected. What remains true: the segmenter needs GPU acceleration, so the person / subject / background presets are unavailable, with a message, on a machine without it.

Open: none for this RFC. (RFC-0032's question C, bundling LaMa's 208 MB, is separate and still open: unlike SAM it would roughly add 208 MB to a 283 MB installer.)

## 9. Slice A, the sky resolver measured (2026-10-26)

**What was run.** A numpy prototype ([skyheur.py](RFC-0033-appendix/skyheur.py), outside the app): the smooth region connected to the top edge, grown pixel to pixel with a colour-step limit; bright low-saturation textured pixels next to it (clouds) taken in; holes filled. 300-px working size. Compared with **skyseg's masks** on the 45 Wikimedia Commons photographs of RFC-0023 §4.6 (12 scene types, CC0 / PD / CC BY). **skyseg is a reference, not ground truth** (its provenance is unexplained and it cannot ship, RFC-0023 §4.3), but it was preferred in 32 of 45 blind comparisons against the project's own trained sky model (RFC-0023 §4.6). The tuning was two variants on the same 45 photographs (the second adds a per-row colour match); that is a small overfit risk, so the numbers are best read as the ceiling of this approach.

| | Photos with more than 5 % of the frame different from skyseg | Mean IoU (photos where skyseg finds sky) |
|---|---|---|
| **Heuristic** (best of two variants) | **34 of 45** | **0.65** |
| The project's trained U-Net (rejected in RFC-0023 §4.6), same comparison | 19 of 45 | 0.76 |
| The bar set before the trained model's last run (RFC-0023 §4.6) | at most 8 | 0.80 |

By scene (variant 1, mean IoU): **desert 0.90, clear blue sky 0.88, skyline 0.79, sunset 0.72**, then lake 0.65, fog 0.65, storm 0.61, sea horizon 0.60, trees 0.57, night 0.53, **bare branches 0.15** (it finds almost nothing: the sky between branches is not connected to the top). [Overlays for twelve photos](RFC-0033-appendix/sky-heuristic-vs-skyseg.png) (left skyseg, right heuristic): clean skies, sunsets and storm skies over open ground are good, including the cloud edges; reflections in water, fog, trees and branches, and the Milky Way are not.

**What it says.** A non-model sky mask is a good *first guess* for open skies and not a resolver to trust unattended: by the project's own earlier bar it fails, and it is worse than the trained model overall. So §3.1b: show what was found, flag the doubtful, offer the manual route. The measurement did **not** try: a model-based sky (SAM 2 prompted with a point the heuristic picks; RFC-0026's rejection was for a *click* tool used as a sky detector, with the user choosing the candidate), which is the next thing to measure if the flagged-photo rate is too high in use; the project's own trained U-Net as the resolver (rejected on its numbers, and its CC BY training-data question is open); a confidence signal (the heuristic's own features may predict its failures; not measured).

**Not verified:** `face`, `person`, `shadows` resolvers (not run); Windows (a pure numpy prototype); how the Rust port behaves (this is a prototype, ~60 lines; the port is slice B's); any photograph of the user's own.
