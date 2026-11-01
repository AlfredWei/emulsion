# RFC-0031: Intelligent culling: similar-photo groups and a soft-shot cue (M7 slice 1)

- Status: Draft for review (slices A, B, C built; slice E surveyed in §8, desk research only)
- Date: 2026-10-20
- UX mock: [similar-photos-mockup.html](../ux/mockups/similar-photos-mockup.html)
- Relates to: [MILESTONES.md](../../PRD/MILESTONES.md) M7, PRD §7.2 (duplicate detection) and §7.3 (stacking), [RFC-0030](RFC-0030-import-and-export-speed.md) (thumbnail tiers, the background-pass pattern this reuses), [RFC-0005](RFC-0005-face-detection-people-view.md) (face landmarks, for the later blink slice), [RFC-0023](RFC-0023-on-device-ai-inference.md) (model survey method, the later generative slice), [ADR-0009](../adr/ADR-0009-ml-inference-runtime.md)

## 0. Process note

Design plus a small measurement. The measurement is the ignored test `cull_probe` ([cull_probe.rs](../../app/src-tauri/src/cull_probe.rs)): a sharpness score and a perceptual hash on 13 JPEGs from `test_image/`, with *synthetic* blur and *synthetic* edits. **No real burst, no real out-of-focus frame, no blink was available**, so what is measured is how a metric behaves, not how well it agrees with a photographer. §6 lists what only the user's own bursts can settle. No product code changes in this RFC.

Decided with the user on 2026-10-20: **culling comes first; generative fill is a later M7 slice, framed as spot removal** (§5, slice F).

## 1. Problem

M7's promise is that culling a large, burst-heavy import gets faster. Today the user has pick/reject, stars, labels, and Survey / Compare views, all manual: for a 300-frame event they still look at every frame, and the near-identical frames and the soft ones are what they spend most of the time on. Two aids, in the order the milestone lists them:

1. **Near-duplicate groups**: frames of the same moment, shown together, with the best one suggested.
2. **A soft-shot cue**: which frame in a group missed focus or shook.

Exact duplicates (same bytes) are already refused at import by the blake3 content hash; this RFC is about *different files of the same moment*.

## 2. What was measured

Setup: 13 JPEGs (panorama frames, portraits, night, and six ISO 6400 files), each decoded and reduced to a 1024 px long edge in gray; release build on the Apple M1 Pro; the whole probe (decode of 13 full-size files plus both metrics) ran in 3.2 s, decode dominating.

**Sharpness = variance of the 4-neighbour Laplacian**, whole frame and the 90th percentile over an 8×8 tile grid (so a sharp subject on a soft background still scores):

| | whole frame | 90th-pct tile |
|---|---|---|
| Score of the 13 *unblurred* photos | 50 to 1,714 | 139 to 4,029 |
| After Gaussian blur σ 0.6 px (at 1024 px) | 24 to 45 % of original | 25 to 53 % |
| σ 1.0 | 6 to 19 % | 6 to 18 % |
| σ 1.6 | 1 to 8 % | 1 to 8 % |

- **Within one photo the score is a very strong, monotonic blur signal.** Even a blur of 0.6 px at 1024 px (about 3.5 px on a 6000 px wide frame) cuts the score to under 55 %.
- **Across photos the score is useless as a number.** Sharp photos range from 50 to 1,714; the ISO 6400 files score *higher* than a clean portrait because noise has edges too (`directions` 1,036 vs the sharp portrait 70). So **no absolute "blurry below N" threshold exists**; the cue has to be *relative*: this frame against its siblings from the same moment, same lens, same scene.

**Near-duplicate hash = 256-bit difference hash** (16×16 gradient signs of a 17×16 reduction), distance = differing bits (0 to 256):

| Same photo, then… | distance (13 photos) |
|---|---|
| resized to half | 0 to 2 |
| blurred σ 1.6 | 0 to 2 |
| +25 % exposure | 1 to 17 |
| cropped 5 % | 22 to 45 |
| shifted sideways 4 % (a burst panning) | 27 to 59 |
| **a different photo (78 pairs)** | **95 to 126+ (median 126)** |

- Exposure, size and blur changes are nearly invisible to the hash; **a few percent of camera movement is not**, but even the worst shift (59) is far from the closest *unrelated* pair (95).
- **The 78 pairs are (all but one) unrelated scenes**; the exception, the two overlapping panorama frames, still sits at 95 or more. That is the easy negative. The hard one, *two different shots at the same venue* (same wall, same stage, different pose), is not in the data and will sit much closer. So the hash distance alone must not decide; **capture time** has to be a second condition (§3.2), which also makes a wrong group cheap to be wrong about.
- **Cost (measured in slice A, the ignored `cull_probe_cost`, M1 Pro, release):** sharpness plus hash take **7.9 ms per photo** on the 1024 px picture; reducing a full-size photo to that picture, needed only on the source-decode fallback, takes 107 ms. The normal path adds the thumbnail's own small JPEG decode.

## 3. Decisions

### 3.1 What is computed, when, and where it is stored

For every photo, once, from the **unedited 1024 px picture the Large thumbnail pass already has in hand** (RFC-0030): `sharpness` (whole) and `sharpness_tile` (90th percentile) as REAL, and `dhash` as a 32-byte BLOB.

- **Why a separate quiet pass after the Large thumbnails (changed at implementation, slice A):** the first draft measured inside the Large-tier decode. But a RAW's final thumbnail is its embedded JPEG, written at import, so RAWs never go through that decode at all. So the pass (`cull::analyse_missing`) runs after the Large pass and reads each photo's plain `{id}.jpg` (1024 px for a JPEG, the camera's embedded picture for a RAW): a few milliseconds to decode and never the 24 MP source. An edited thumbnail (`{id}-{hash8}.jpg`) is never used, so an edit cannot change a score or a hash. If the plain file is missing or its long edge is under 640 px (some cameras' embedded previews are tiny), the source is decoded instead (about 100 ms more per photo, measured). Everything is reduced to a 1024 px long edge first, so scores are comparable across sizes.
- **Storage:** a new table `cull_analysis(image_id INTEGER PRIMARY KEY REFERENCES images ON DELETE CASCADE, sharpness REAL, sharpness_tile REAL, dhash BLOB, algo INTEGER, analysed_at TEXT)`. `algo` is a version number: changing the metric bumps it and old rows are recomputed in the background. About 50 bytes a photo (2.5 MB for 50,000). Catalog migration is additive, `CREATE TABLE IF NOT EXISTS` like the others.
- **Existing catalogs:** a background backfill (the `backfill_missing_thumbnails` pattern) decodes the plain `{id}.jpg` Large thumbnail when it exists and the source otherwise, one low-priority pass that yields to import and to the Large pass (`FOREGROUND_WORK`).
- **Never changes the photo, never writes a sidecar, never leaves the machine.**

### 3.2 Grouping

Groups are **derived, not stored**: computed from `cull_analysis` plus `captured_at` and the camera on demand, in memory, so there is no stale state to repair when photos are added, removed or re-analysed. Pure function, unit-testable without a window:

1. Sort by `captured_at` (photos without one are not grouped; their hash alone is too weak, §2).
2. Walk a window (each frame is compared with at most 400 followers, so a stuck camera clock stays cheap): a photo joins a group when it is within **T seconds** of any member's capture time **and** its hash distance to that member is within **D bits**; same camera model required. Union-find over the window; groups of one are dropped.
3. Starting values: **T = 10 s** (a burst, and a posed re-take), **D = 64 of 256**. The measurement supports "D between 59 and 95"; 64 is the conservative end of that gap, to be tuned on real bursts (§6). Both are exposed as one **Similarity** slider in the UI, not as two numbers.
4. **Suggested best**: the member with the highest `sharpness_tile`, *only if* it beats the runner-up by a margin; otherwise "no clear best" (equal sharpness is information too). The suggestion is a badge, never an action.

### 3.3 The soft-shot cue is relative and only inside a group

A photo is shown as **Soft** when its `sharpness_tile` is under **50 %** of the group's best (the measurement: a 0.6 px blur at 1024 px already lands at 25 to 53 %). A photo that is in no group gets **no badge**: §2 shows a single photo cannot be judged on its own number (noise inflates it; a flat sky deflates it), and a wrong "this is blurry" on a keeper is worse than no cue. A later slice may add an image-quality model for singles (§5, slice E).

### 3.4 UX

Designed first as an interactive mock, [similar-photos-mockup.html](../ux/mockups/similar-photos-mockup.html) (synthetic photos; the Similarity slider really re-groups with §3.2's rule; Reject the others, Soft chips and the analysis strip all work), to be judged by use before slice C builds anything:

- **Library:** a **Similar** entry in the rail (count of groups). A group shows as one cell with a stack of up to three thumbnails behind it and the count; opening it fills the view with the group (the existing Survey view, `N`) with sharpness shown as a small bar and a **Soft** or **Suggested** badge on each frame.
- **Actions, all existing ones:** pick / reject / stars keyboard shortcuts work as today. One new bulk action in the group: **Reject the others** (the suggested or the picked frame stays). It is one confirmation with the count, never automatic, and **rejects only** (flag = reject, files untouched, reversible by changing the flag). There is no undo stack for flags in the app today, so the confirmation carries the count and the reverse is stated: *Show rejected*.
- **Grid cue:** a small **Soft** chip on cells that are soft inside a group, off by default, on from the View menu.
- **Nothing is hidden or removed by the analysis**; the grid with no filter looks as it does today.
- The analysis state is visible: "Analysing 124 / 300" in the existing background-progress strip while it runs, groups appearing as it goes (the RFC-0030 "batched events" pattern).

### 3.5 What this is not

- **Not stacking.** The PRD's stacking (§7.3) is a user-made, persistent grouping (`images.stack_id`, unused today). A derived similar-group may later offer *Stack these* but this RFC does not build stacks.
- **Not a model.** No download, no consent flow, no GPU: the two metrics are plain arithmetic on 1024 px pixels. That is deliberate for slice 1: it ships without ADR-0009's runtime, on both platforms, and is explainable ("these four were taken within 10 s and look alike").
- **Not an aesthetic judge.** It will not say which frame has the better expression or composition; it can say which is sharper.

## 4. Alternatives considered

| Alternative | Why not (now) |
|---|---|
| Learned image embeddings (CLIP / DINOv2-class) for similarity | Tens to hundreds of MB, a model download and consent flow, GPU or slow CPU; the hash plus a time window already separates unrelated scenes by a wide gap. Revisit only if real bursts show the hash failing (e.g. pose changes). |
| An IQA / aesthetic model for blur and "best frame" | Same weight; and "best" is subjective. It would be the only way to judge a *single* photo (§3.3), so it is slice E, behind a survey. |
| Perceptual hash `pHash` (DCT) instead of `dHash` | Not tried here. dHash was enough to separate; trying pHash and a multi-crop hash for the pan case is part of slice D's tuning. |
| Compute from the source file on demand | A 24 MP decode per photo is the cost RFC-0030 spent a whole RFC removing. |
| Store the groups | Stale-state bugs for no speed gain: grouping 50,000 rows in memory is milliseconds-scale list work. |

## 5. Slice plan

| Slice | What | Done when |
|---|---|---|
| **A** | `cull_analysis` table, the two metrics + hash as a pure module with tests, computed in the Large-tier pass, backfill for existing catalogs, `algo` versioning | Tests: score falls monotonically with blur, hash stable under resize / exposure, rows written once per photo, backfill resumes after quit; no change to import timing beyond noise (measured) |
| **B** | Grouping function (time window + hash + camera, union-find) and the suggested-best / soft rules; one Tauri command `list_similar_groups(similarity)` | Unit tests incl. burst, posed re-take, same venue different shots, missing time, chained windows |
| **C** | UI: **Similar** rail entry, group cell, group in Survey with Soft / Suggested badges, Similarity slider, *Reject the others*, progress strip text | Component tests; real-window pass on a generated burst folder |
| **D** | **Tuning on the user's real bursts** (§6): labelled sets, T / D / margin chosen from data; pHash and shift-robust hash tried | Numbers (agreement with the user's own picks) written into this RFC |
| **E** | Optional, each behind its own survey: **blink / closed eyes** (small classifier on the RFC-0005 face crops), and an **IQA model for single photos** | Per RFC-0023's method: size, licence, accuracy, hardware, before any build |
| **F** | **Generative fill, framed as spot removal** (user decision 2026-10-20). Its own RFC first: bundled on-device generative model vs the non-generative content-aware fill that extends M4's healing brush, which MILESTONES.md says must be decided and documented *before* implementation | RFC with licence-read candidates and real numbers; the decision recorded |

Not in this RFC and not scheduled: M7's **adaptive auto-tone presets**, which want their own RFC after culling.

M7's exit criterion ("measurably reduce time-to-cull on a large event / burst-heavy import") needs a protocol, proposed here for slice D: one folder of about 300 frames the user has *not* culled, culled once with Similar switched off and once on (different halves, same person), recording time and the share of the user's final picks that were the suggested frame.

## 6. Not verified (needs the user's photos or later work)

- **Everything about real photographs.** The blur was synthetic and the pans synthetic; real motion blur, a missed focus point, shallow depth of field and real bursts have not been scored. The "50 % of the best" rule and D = 64 are starting points, not results.
- **The hard negative** (same venue, different shot) is absent from the data.
- **Agreement with a photographer** is unmeasured; this is slice D's job and needs the user's sets: a burst, a posed re-take set, and a few deliberately soft frames, with their own pick marked.
- **RAW** was not measured at all (JPEG only); embedded-preview sizes across camera makers are unchecked.
- **Cost over a whole real import** (the metric alone is 7.9 ms a photo, see §2); the pass is quiet and yields to imports, but its total time on a large catalog has not been run.
- Windows. (Grouping time on 50,000 photos was measured in slice B: 35 ms, including a quarter of them on one stuck second.)

## 7. Questions for the user

1. **Reject the others:** is a bulk reject with one confirmation right, or would you rather the group only *marks* suggestions and you press reject yourself?
2. **Similarity:** one slider ("looser ↔ stricter") is the plan. Is a fixed value enough for the first version?
3. **Real bursts:** can you provide (or point to) a folder of unculled burst-heavy photos, ideally RAW and JPEG, for slice D? Without them the thresholds stay unproven.
4. **Order of E:** blink detection first or the single-photo quality model first, after slice D?

## 8. Slice E survey: blink detection and a single-photo quality model (desk research, 2026-11-01)

Per §5 and RFC-0023's method (size, licence, accuracy, hardware, **before any build**). **This is desk research only: no model was downloaded and nothing was run**, because downloading weights needs the user's go-ahead (§8.6). Sources are model cards and repositories read on 2026-11-01; where a card does not say, this section says "not stated" rather than guessing. §7's question 4 (which first) is answered in §8.5.

### 8.1 What the app already has for blinks

- The face pipeline (RFC-0005, `face_detect.rs`) runs **YuNet** at 640 px input and returns, per face, a box, a score and **five landmarks: both eye centres, nose, mouth corners**. Eye centres are enough to cut an eye crop; **eyelid points are not available**, so a geometric eye-aspect-ratio is not possible with it.
- **The catalog does not keep the landmarks** (`faces`: box, score, embedding, person, excluded; `schema.rs`). A blink pass needs either a re-detection per photograph (YuNet is the cheap part of the scan) or new columns for the two eye centres. Either way **a blink result needs the face scan to have run**, which the culling analysis (§3.1) does not require today.
- Slice B's groups give the natural place for the result: "closed eyes" only matters when choosing between near-duplicates, and the Suggested frame should not be one with a blink if another frame has open eyes.

### 8.2 Candidates for closed-eyes detection

| Candidate | What it is | Size | Licence (read) | Evidence of accuracy | Fit |
|---|---|---|---|---|---|
| **BlinkLinMulT / `blinkcnn-onnx`** ([model card](https://huggingface.co/fodorad/blink_detection), [repo](https://github.com/fodorad/BlinkLinMulT)) | Eye-state per **64 x 64 eye crop** (input `(B, T, 3, 64, 64)`, `T = 1` is one frame); four ONNX models, one PyTorch | not stated per file; ONNX-only install about 100 MB; CPU 4.15 ms per frame (card, own machine) | **Code and weights MIT.** Trained on seven public corpora (CEW and MRL-Eye stills among them) that "each carry their own licence" and are not redistributed; **those licences are not stated on the pages read** | Card reports **no accuracy or F1 for eye state**; the one figure is average precision for *blink events* on a video set (blinkcnn 0.79, with a wide interval). Export parity with PyTorch is checked | Plausible, cheap, reuses YuNet's eye centres. The eye-state score on a still is the use (the blink-event part needs video and is **not** what culling needs). Licence of the training data is the open point |
| **MediaPipe Face Mesh V2 + Blendshapes (ONNX conversions)** ([landmarks](https://huggingface.co/fernandotonon/QtMeshEditor-facemesh-onnx), [blendshapes](https://huggingface.co/fernandotonon/QtMeshEditor-faceblendshapes-onnx)) | A 256 x 256 face crop to 478 landmarks (with eyelids), a 146-landmark subset to 52 blendshape scores including `eyeBlinkLeft` / `eyeBlinkRight` (0 to 1) | not stated | **Apache-2.0 code and weights** (Google's model cards, community ONNX conversions repeat it); the conversions are third-party mirrors, so **upstream terms must be checked before shipping** | No accuracy given; a parity check against the Python reference is linked, results not shown | Two models in series, so heavier and more to get wrong; but it also tells **half-closed** from closed, and works from eyelid geometry, not a trained eye classifier. Cleanest licence |
| Open / closed eye ONNX from Hugging Face (`MichalMlodawski/open-closed-eye-detection`) | Eye classifier | not stated | **CC BY-NC-SA 4.0** (non-commercial, share-alike) | not stated | **Out** (licence) |
| EAR threshold on landmarks (classical) | Ratio of eyelid distances | none | the one paper found is GPL-licensed code; the method itself is free | thresholds are person- and camera-dependent (the paper says so) | Needs eyelid landmarks, so only with the MediaPipe mesh; usable as a cross-check, not alone |
| Commercial culling tools (Imagen, Aftershoot) | Closed / partly closed per eye, folded into a score; shots are demoted, not deleted | n/a | n/a | **Vendor claims only** (99.2 % and 96 % in two posts, no method, no independent test) | Not evidence; it does show the **demote, never delete** behaviour users expect, which matches "Suggested" and the existing confirm-before-reject |

### 8.3 Candidates for a single-photo quality model

[A Hugging Face repo](https://huggingface.co/86Cao/IQA-ONNX-Models) holds ONNX exports (from IQA-PyTorch v0.1.15) of the usual no-reference models, all at 224 x 224 input: **MUSIQ about 104 MB** (single-scale simplification), **CLIP-IQA about 146 MB**, DBCNN about 59 MB, HyperIQA about 105 MB, MANIQA about 437 MB, TReS about 575 MB, LIQE about 675 MB. The repo is tagged Apache-2.0, says "original model weights are subject to their respective licences" and **names none**; all were **pre-trained on KonIQ-10k, which the dataset page calls "freely available to the research community" and gives no commercial terms**. No quality-prediction accuracy is reported (only export parity). Not stated either: speed. So **licence is unresolved for every one**, and the quality of a 224 px resize of a 24-megapixel photograph for judging sharpness is **doubtful by construction** (a soft-focus miss is a pixel-level defect; §2 shows the relative sharpness score already does that job).

### 8.4 Risks that apply to any blink model (read from the app, not measured)

- **Face size.** Faces under 24 px at a 1600 px reference are already dropped (`FACE_MIN_SIDE_PX`). An eye is roughly a quarter of a face's width (an estimate): at that floor it is about 6 px wide, stretched to a 64 px crop. A group photograph (the app's own set has frames with 17 to 59 faces) is mostly such faces. **A blink verdict needs a minimum eye size and must say "cannot tell" under it**, in the project's own style (found / check / not found).
- **What is a blink:** a person looking down, squinting in sun, laughing, wearing glasses with glare or sunglasses are all "eyes not open" to a classifier. Every tool above says nothing about these. The cue must be a **relative badge inside a group** ("eyes closed in 2 of 5 frames"), like Soft (§3.3), never an absolute reject.
- **Which pixels:** the grid thumbnails (1024 px) are too small for eye crops from a group; the Develop preview or the source is needed for a blink pass, which costs a decode that the sharpness pass avoided (§3.1).
- **Windows:** nothing here was run on it.

### 8.5 Recommendation

1. **Blink first, the quality model not at all for now** (§7 question 4). Closed eyes is a decision people actually make between near-duplicates and a model's wrong answer is cheap to check by eye; "best frame" is subjective and the single-photo scores have no usable licence and no accuracy evidence for this use.
2. **Two candidates go to a measurement, not to a build:** (a) YuNet eye centres + the BlinkCNN eye-state model (smallest and simplest, MIT, training-data licences to be read in the repository's `docs/data.md`); (b) MediaPipe Face Mesh + Blendshapes (Apache-2.0, eyelid-level, half-closed). The measurement decides between them, or neither.
3. **The measurement** (needs the user's photographs, like slice D, and a download go-ahead): about 150 faces from the user's own events, each marked open / closed / half by the user, at three face sizes; report per size: accuracy, "cannot tell" rate, time per face, and the model files' sizes. A model is worth building only if it is right on clearly open and clearly closed eyes at the sizes where it speaks and says "cannot tell" at the rest.
4. **If built (slice E1):** blink results stored per face (or recomputed from stored eye centres), shown as a badge on group members, folded into "Suggested" as a tiebreak only; nothing rejects by itself.

### 8.6 What the next step needs from the user

- **Go-ahead to download** the two candidate sets into the scratch area (`mask_training/`-style, outside the repo): the BlinkCNN ONNX file(s) (sizes to be read from the repository when downloading) and the MediaPipe face landmark and blendshape ONNX files. Nothing is bundled by this.
- **Photographs with people** for the measurement (the slice D bursts, if they include portraits and groups, would do).
- The training-data licences of the BlinkCNN corpora are to be read from the repository's `docs/data.md` (not reachable in this survey); if any is research-only, candidate (a) falls under the same question as the sky model's training data (`mask_training/README.md`).

**Not verified in this section:** everything about accuracy (no model run), every file size marked "not stated", the training-data licences, upstream terms behind the community ONNX conversions, the eye-width estimate, Windows.
