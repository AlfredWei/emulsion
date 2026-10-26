# RFC-0033: Adaptive and context-aware tone: what it could mean here, and what to build first (M7 slice 3)

- Status: Draft for review. **A scoping RFC: it frames the question, reads what is known, and proposes an order. It builds nothing and measures nothing yet** (§6 says what the first measurement needs).
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
| **R4. Personal adaptive** | Learn from **the user's own finished edits** (on this machine, never uploaded): for a new photo, find photos like it that the user edited, and propose their typical slider values. "Edit like I usually do." | No model download and no third-party data licence; **it needs the user's edits** (hundreds of photos to be useful) and **a measurement of whether it beats the fixed rules** before anything is built. Privacy-clean, and not something Lightroom does. |

Not in the table, and not proposed: cloud analysis (PRD: no cloud), generative colour grading, and matching any product's proprietary "Auto".

## 3. What this RFC proposes

**Order: R2 first, a measured R1 beside it, R4 as a study, R3 not now.**

### 3.1 R2 first: it is the part nobody can get from the current app

R2 needs no new model, builds on RFC-0026, and gives the user something visible: presets such as "Sky: deepen and cool", "Subject: lift and warm", "Portrait: soften background". Shape:

- A preset op may carry a **region binding** (`sky`, `subject`, `person`, `background` = the inverse of the subject) instead of a mask raster. The binding, the adjustments and the feather live in the preset; **no raster is stored in the preset**.
- **Apply** resolves the binding for each target photo with the existing segmenters, writes the resulting mask into that photo's edit stack as the ordinary segment mask it already is (per-photo, excluded from copy as today), with a **history entry that says what was bound** ("Sky: deepen and cool: sky found, 31 % of the frame").
- **When nothing is found**, the op is skipped and the result says so ("No sky found in 4 of 12 photos"), never a silent empty mask. A coverage floor (for example under 2 % of the frame) counts as not found; the number is measured in slice B, not guessed.
- **Applying to many photos** is a batch job in the background (the segmenter takes time), cancellable, and one at a time like the other helper jobs.
- **Presets are excluded from XMP** as masks already are, and the binding is a name plus numbers, so a preset file is small and shareable.

### 3.2 It follows the stored-result rule of RFC-0032

The mask a segmenter produced for one photo is a result the user may have fine-tuned (RFC-0026's modifier). Per RFC-0032 §10 (same machine, same model: identical; other providers: small differences), the **resolved mask is stored with the photo's edit stack and never silently re-resolved**; "Re-apply preset" is an explicit action. This avoids a preset re-run changing an edit the user already accepted.

### 3.3 R1: a better Auto Tone, judged by a person, not by a metric

- **Keep the current Auto Tone as the baseline and the fallback.** Any new rule set ships only if the user, shown pairs without labels, prefers it on enough photos (§5).
- Inputs are cheap: the histogram, the percentiles already used, plus **segment-mask statistics where masks exist** ("do not brighten the sky to the subject's median"; "protect a snow scene's whites"). Output stays the existing seven sliders, so the result is explainable and editable and nothing new is stored.
- The rule set must be small enough to be described in the user guide in a paragraph, or it is the wrong design.

### 3.4 R4: a study, not a slice

The idea is attractive (a personalised Auto, privacy-clean) and its viability is unknown. It is cheap to measure **offline** once there is data: leave one photo out, predict its sliders from its nearest neighbours' sliders (neighbour = a few image statistics, no model), and compare the error with today's Auto Tone and with "all zeros" on the same photos. **If it does not clearly beat Auto Tone on the user's own edits, it is dropped**, and nothing was built. §6 says what data is needed.

### 3.5 R3, not now

A learned enhancer fits poorly here for the two reasons in §2: a lookup output does not fit a slider edit stack, and the training-data licence question repeats RFC-0032's. **Revisit if R1 and R4 are judged not good enough**, with a survey in the RFC-0023 manner (size, licence, accuracy, hardware) before any build.

## 4. Not in this RFC

- Cloud, generative grading, colour grading by reference image, film-stock colour emulation (M5.8's grain only is built; colour is a separate question).
- **Auto crop / straighten / auto-subject framing.**
- **Auto White Balance changes**: the gray-world rule has the same blindness, but it is a separate decision (a scene-aware WB can use the same classifier as R1 if R1 ships).
- Anything in Library (culling is RFC-0031).

## 5. How it is judged

- **R2**: it must do what its name says on a set of real photographs: sky found / not found correct on a labelled set, the coverage floor chosen from that set, the result judged at 100 % for halos on the mask edge (the failure RFC-0026's modifier exists for). Time per photo and for a 30-photo batch.
- **R1**: **blind pairs**. The same 30 photographs (the user's, across scene types), Auto Tone as it is vs the new rules, shown without labels, the user picks or says "same". The new rules ship only if they win clearly and **lose nowhere badly** (a regression on a photo type is worse than no gain). Slider agreement with the user's own finished edits is recorded as a second signal, not the decision.
- **R4**: leave-one-out error against Auto Tone and against zeros, on the user's edited photos, in slider space and in the resulting histogram percentiles.

## 6. Slice plan

| Slice | What | Needs |
|---|---|---|
| **A** | **The measurement kit**: an exporter that writes (image statistics, slider values) for every edited photo in a catalog, plus the leave-one-out and blind-pair tools. Reports today's Auto Tone vs the user's own edits | **The user's edited photos** (their catalog, or an export of edit stacks with thumbnails). Nothing leaves the machine |
| **B** | R2: region-bound preset ops: the model, apply with the existing segmenters, not-found handling, history entry, batch job, tests | Slice A's labelled sky / subject set for the coverage floor |
| **C** | R2 UI: the preset editor's "bind to region" control, a small starter set (Sky, Subject, Portrait), preview on hover (as presets do) | UX mock first, the RFC-0028 / 0029 / 0031 / 0032 way |
| **D** | R1: scene-aware Auto Tone, only if the blind-pair test says it wins | Slice A |
| **E** | R4: only if slice A's leave-one-out shows a clear win | Slice A |

Slices B and C do not depend on D or E: adaptive presets are useful whatever Auto does.

## 7. Not verified

- **Everything about quality.** No photograph was processed for this RFC. Whether scene rules beat Auto Tone, whether neighbours predict a user's sliders, and whether the coverage floor is 2 % or 5 % are all open.
- **Lightroom's behaviour** is from help pages and third-party descriptions, not from using it here.
- **FiveK's licence and AdaInt's model sizes and accuracy**: not read in the primary sources (§2).
- **Windows**: segmenters on the Windows runner are as verified as RFC-0026 left them.

## 8. Questions for the user

1. **Order**: R2 (adaptive, mask-bound presets) first, with the measurement kit beside it, as §3 proposes? Or the better-Auto first?
2. **Your edits as data** (slice A): may the measurement kit read your own catalog (read-only, on this machine) to build the evidence for R1 and R4? If you would rather not, a folder of about 100 edited photos with their settings works.
3. **R4**: is "learns from my own edits, locally" something you would want, or is a fixed good Auto enough? (If not wanted, slice E is removed now.)
4. **Starter presets** (slice C): which region presets would you actually use? Candidates: deepen the sky, lift the subject, soften the background, brighten faces, cool the shadows.
