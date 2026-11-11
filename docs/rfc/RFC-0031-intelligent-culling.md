# RFC-0031: Intelligent culling: similar-photo groups and a soft-shot cue (M7 slice 1)

- Status: Draft for review (slices A, B, C built, with the grid's Soft chips; slice E surveyed in §8, desk research only)
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
- **Grid cue:** a small **Soft** chip on cells that are soft inside a group, off by default, on from the View menu (built: **View ▸ Toggle Soft Chips on Grid**, remembered between launches; the groups it reads are the Similar view's, kept current while the chips are on).
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

### 8.7 The two candidate sets downloaded and loaded (2026-11-09)

The user gave the go-ahead (2026-11-09); the files are in `mask_training/blink/models/` (outside the repo, nothing bundled) with their model cards in `cards/`. **Sizes and SHA-256 match what the Hugging Face API lists** (the LFS object ids).

| File | Repository | Bytes | SHA-256 (first 16) | Input | Output | CPU, one call |
|---|---|---|---|---|---|---|
| `blinkcnn.onnx` (+ `.json`) | `fodorad/blink_detection` | 20,073,974 | `6ae7083ad1883c5a` | `crops` `[batch, time, 3, 64, 64]` RGB in 0..1 (ImageNet normalisation is **inside** the graph; applying it again silently degrades) | `sequence` `[batch, time, 1]`, a closed-eye probability; the sidecar gives an operating point **threshold 0.53** | 4.6 ms |
| `face_landmarks.onnx` | `fernandotonon/QtMeshEditor-facemesh-onnx` | 4,920,979 | `d16e5a55e6a48028` | `[N, 256, 256, 3]` RGB 0..1, a **rotated, cropped face** | 478 landmarks x (x, y, z) in 256-crop pixels (1434 values) and a face-presence logit | 3.6 ms |
| `face_blendshapes.onnx` | `fernandotonon/QtMeshEditor-faceblendshapes-onnx` | 1,094,706 | `37cf54d3ccf671f8` | `[1, 146, 2]` the 146-landmark subset in pixels | 52 ARKit blendshape scores in 0..1, including `eyeBlinkLeft` / `eyeBlinkRight` | 0.7 ms |

Together about **26 MB** (BlinkCNN alone 20 MB; the mesh and blendshapes 6 MB), against the survey's "about 100 MB" for the blink install (that figure was the Python package, not the weights). Timings are one call with random input, ONNX Runtime CPU on this Mac, not a measurement of accuracy.

**Read from the cards, and what it changes.**
- **Licences.** BlinkLinMulT code and weights: **MIT** (the LICENSE text was retrieved). The two MediaPipe graphs: **Apache-2.0** per the mirrors' cards, which describe them as direct conversions of Google's MediaPipe models; the **upstream Google terms are still to be read** before shipping. **The BlinkCNN training corpora (TalkingFace, RN15, RN30, CEW, MRL-Eye, HUST-LEBW, MPEblink 2.0) are not redistributed and the repository states no licence for any of them: "each corpus has its own licence and access procedure. Request access from the original authors."** So candidate (a) stays open on training-data terms, as §8.6 expected (this is the same kind of question as the sky model's CC BY data and the removal model's Places2, which the user accepted on 2026-11-02; it is the user's call, not made here).
- **Crop geometry (a risk for (a)).** The card says v2 "was trained on a square 2.0 x the eye's **corner-to-corner span**; a different crop convention degrades accuracy". YuNet gives eye **centres**, not corners, so (a) needs an estimate of the span from the inter-ocular distance, and the estimate's error is part of what the measurement must show. (b) needs a **rotated face crop** at 256 px, which the face pipeline does not produce today either (YuNet's box and the eye centres give the rotation).
- **Training data skew** (card): "frontal, indoor webcam footage; expect degradation on strong profile views, heavy occlusion, or infrared"; this is a wedding-and-event use, so the measurement matters.
- The card reports an average precision of 0.79 for blink *events* on video, and **no accuracy for eye state on a still**, as the survey found.

**Not verified:** everything about accuracy (the models were run on random input only, to see them load, take the stated shapes and time a call); upstream Google terms for the MediaPipe graphs; the training-corpus licences; the crop estimate for BlinkCNN; Windows and CoreML.

**Needs from the user (unchanged from §8.6 except the download, now done):** photographs with people for the measurement (about 150 faces, each marked open, closed or half by the user, across three face sizes), and a decision on the BlinkCNN training-data question if (a) wins.

### 8.8 The measurement tool, run once on public photographs without labels (2026-11-09)

[blink_measure.py](RFC-0031-appendix/blink_measure.py) is the §8.5 step-3 tool, ready for the user's photographs. For every face YuNet finds (the app's own detector, run at the 1600 px reference as the face scan does) it scores both candidates: **(a)** BlinkCNN on an upright square eye crop (side = `CROP_K` x the inter-ocular distance, default 1.0, i.e. 2.0 x an estimated eye span of 0.5 x that distance), **(b)** the MediaPipe Face Mesh on the card's ROI (rotated by the eye line, 1.5 x the face box, 256 px) then the Blendshapes' `eyeBlinkLeft` / `eyeBlinkRight`. It writes one record per face (size, both scores, the mesh's face-presence, times), numbered contact sheets (face crop, both eye crops, scores) and a `labels.csv` template (`o` open, `c` closed, `h` half, `x` cannot tell) to fill in.

**One run: 557 faces from 37 public photographs** (40 Commons headshots and group pictures of the earlier people studies; **no labels, so nothing below is an accuracy**).

| Face width (source px) | Faces | BlinkCNN calls "closed" (p > 0.53, either eye) | Blendshape > 0.5 (either eye) | Agree | Mesh face-presence under 0.5 |
|---|---|---|---|---|---|
| under 24 | 149 | 82 % | 53 % | 60 % | 56 % |
| 24 to 48 | 270 | 42 % | 29 % | 63 % | 17 % |
| 48 to 96 | 116 | 31 % | 12 % | 76 % | 13 % |
| 96 and over | 22 | 41 % | 18 % | 68 % | 32 % |

What it shows, and what it does not:
- **Most faces in a crowd are small** (median 37 px) **and both models speak confidently about them anyway**: BlinkCNN calls 82 % of faces under 24 px closed, which is not a blink rate, it is noise. The mesh's own face-presence score falls below 0.5 for 56 % of them, so **presence is a ready-made "cannot tell" signal** to test against labels; BlinkCNN offers none.
- **The two agree only 60 to 76 %**; the flag rates of the two differ at every size, so at least one is wrong often. Which, and at what sizes, needs labels.
- **BlinkCNN's output is a logit**, not a probability (the first run read it as one and was wrong); the 0.53 operating threshold is on the probability. Fixed in the tool.
- **YuNet's eye points can be wrong**: on old, blurry or hazy photographs the eye crops landed on the nose or mouth (seen on three of twelve large faces), so any verdict inherits the detector's error; only a mesh with presence would notice. On clear, frontal portraits both the crops and the open-eye scores look right (blendshape 0.02 to 0.45 on open eyes, BlinkCNN's logit about -2 to -4).
- **Assumptions not checked:** that ARKit's "Left" is the subject's own left (so `eyeBlinkLeft` is the eye on the image right; no wink photograph was run); the crop multiple `CROP_K` (not tuned; the card's "2.0 x corner-to-corner span" can only be approximated from eye centres).
- Times (CPU, one face, this Mac, unoptimised Python and OpenCV): BlinkCNN about 17 ms for both eyes including the crops, mesh plus blendshapes about 7 ms; the figures in §8.7 were per model call.

**Not verified:** every accuracy figure; the left / right mapping; `CROP_K`; the user's photographs (the tool is ready: `python blink_measure.py <folder>`, then fill in `labels.csv`, then a scorer that reads it, which is not written yet); Windows. **Needs from the user:** a folder of photographs with people, and about an hour to mark about 150 faces (the sheets are numbered to match the CSV).


### 8.9 Two licence questions read from the sources (2026-11-09)

- **BlinkCNN training data: still unresolved, now from the source itself.** The repository's data page ([`docs/data.md`](https://github.com/fodorad/BlinkLinMulT/blob/main/docs/data.md)) lists seven corpora (TalkingFace, RN15 and RN30, CEW, MRL-Eye, HUST-LEBW, MPEblink 2.0; its text says "six") and says each has its own licence and access procedure, to be requested from the original authors, and that the licences forbid re-hosting. **It states no licence for any corpus**, so whether commercial use of weights trained on them is allowed is not answered by the page. Resolving it means asking each corpus's authors or reading their release pages; until then candidate (a) carries the same open question as the sky model's training data.
- **MediaPipe: Apache-2.0 confirmed on Google's own model cards.** Both the Face Mesh V2 and Blendshape V2 cards list Apache-2.0 (read through a third-party mirror of the card PDFs, which cite the official `storage.googleapis.com/mediapipe-assets/` files; the official copies were not fetched). The cards also say the blendshape model is not for life-critical decisions and does not identify people or store a face representation. The community ONNX conversions are mirrors of these, so the licence follows the upstream cards. Still unread: the bundle's own licence file.
- **Left / right settled.** Google's Face Landmarker guide says the `*Left` / `*Right` blendshapes refer to the subject's own face, not the viewer's, and invert on a mirrored image. So `eyeBlinkLeft` is the eye on the image right in an unmirrored photograph, which is what `blink_measure.py` assumes. This is from the documentation, not from a wink photograph.

Consequence: on licence alone the mesh route is clean and BlinkCNN is not yet; which wins on accuracy still waits for the labelled faces.

### 8.10 The scorer (2026-11-10)

`docs/rfc/RFC-0031-appendix/blink_score.py` reads `faces.json` and the filled-in `labels.csv` from `blink_measure.py` and prints, per face-width bin (<24, 24-48, 48-96, >=96 px) and overall, for BlinkCNN (threshold 0.53, never abstains), the mesh blendshape (0.5, abstaining where the mesh's own face-presence is below 0.5) and the blendshape answering everywhere: eyes scored, **speaks** (share answered), **accuracy** where it speaks, closed-eye **recall** and **precision**, the share of **half-closed** eyes called closed (not scored right or wrong), and the share of **cannot-tell** eyes it still answers. It warns under ten closed eyes.

**Checked only for running**, on 400 labels made up from the models' own output with random noise: the code path works and the table prints; **those numbers say nothing about the models** and are not recorded. **Not verified:** everything about accuracy (no real labels exist yet), whether the bins and thresholds are the right ones (they are the §8.5 starting points), Windows. **Needs from the user:** the photographs and the marks, as in §8.8.

### 8.11 A labelling set prepared from public photographs (2026-11-11)

The user asked for the photographs to be prepared. **What was fetched** (outside the repo, `mask_training/blink/commons`): 74 JPEG renditions (at most 2,000 px wide, 142 MB) from Wikimedia Commons categories about eyes that are closed, squinting or asleep (`blink_fetch_commons.py`), only files whose licence is CC0, public domain, CC BY or CC BY-SA, with `attribution.csv` (credit needed if any is ever shown). It was rate-limited (HTTP 429) and one file failed. YuNet found 71 faces in 47 of them, 60 of them 96 px or wider, **so the closed-eye categories give large faces only**; small faces come from the earlier 37 public photographs (557 faces, mostly under 48 px).

**The sample for marking:** 141 faces drawn at random, **by size only** (never by what a model said), at most 3 per photograph: 17 under 24 px, 35 at 24 to 48, 39 at 48 to 96, 50 at 96 or more (40 of the last from the Commons set). Twelve sheets of 12 faces show each face with the two eye crops the models see, labelled **image left** and **image right**, and **no model scores on them**; the user preferred a page to a CSV, so `blink_make_labeler.py` writes `out/labeler.html` (one face at a time with its two eye crops large; keys o / c / h / x mark the highlighted eye and move on, Shift+letter marks both, Backspace steps back; marks autosave in the browser; **Export labels.csv** writes the file `blink_score.py` reads; nothing is uploaded; the pictures are embedded, so it is one self-contained file that works wherever it is opened; checked in the browser pane for the flow and the CSV, not with the user) (`BLIND=1`; an earlier sheet showed them, which would have steered the marks). `ONLY=<ids>` limits the sheets and `labels.csv` to the sample while `faces.json` keeps scoring all 628.

**What this set can and cannot say.** The large-face closed-eye cases are enriched by choice of category, so the share of closed eyes is **not** the share in a real shoot; use recall and precision per size, not an overall closed rate. Eyes closed in a sleeping or laughing face are real cases but not a representative blink. Public headshots are posed and well lit: a real event folder from the user is still the better test. **Not verified:** that the categories' photographs mostly contain closed eyes (the marks will say), the sample's closed-eye count (the scorer warns under ten), Windows.

### 8.12 First accuracy figures: the user's marks on 141 public faces (2026-11-11)

**Data.** The user marked 140 of the 141 sampled faces (§8.11) with `blink_labeler`: 282 eyes, **87 open, 32 closed, 35 half, 128 cannot tell**. Closed eyes by face width: **0 under 48 px, 9 at 48-96, 23 at 96 and up** (the 32 come from about 17 faces, so they are not independent). Scored with `blink_score.py`; extra cuts by a throwaway script (not kept). Rates below are on eyes marked open or closed unless said.

| Rule | answers | accuracy | closed eyes found | "closed" calls that were right | cannot-tell eyes it still answered |
|---|---|---|---|---|---|
| BlinkCNN, always (0.53) | 119 of 119 | 92 % | 26 of 32 | 26 of 29 | **128 of 128** |
| BlinkCNN, face >= 48 px | 101 | 91 % | 26 of 32 | 26 of 29 | 55 of 128 |
| BlinkCNN, face >= 48 px and mesh sure of a face | 89 | 91 % | 19 of 24 | 19 of 22 | 16 of 128 |
| Mesh blendshape (mesh sure of a face) | 107 | 87 % | **14 of 24** | 14 of 18 | 37 of 128 |

By face width, BlinkCNN: 24-48 px **18 open eyes, all right, no closed eye to test**; 48-96 px **88 % right, 4 of 9 closed found**; 96 px and up **94 % right, 22 of 23 closed found, 92 % of its "closed" calls right**. Threshold 0.4 to 0.7 changes little (at 0.7: 18 of 19 "closed" calls right, 18 of 24 found). Half-closed eyes were called closed about half the time (17 of 35), which is acceptable: a person decides those.

**What this says (small sample, read it that way).**
1. **BlinkCNN beats the mesh blendshape on closed eyes** here (81 % against 58 % found), on both models' accuracy among eyes marked open or closed. The clean licence of the mesh route buys a worse detector.
2. **BlinkCNN never says "cannot tell"**, and the user marked **45 % of all eyes "cannot tell"** (crowds, groups, sunglasses, turned faces). A size floor alone leaves it answering 55 of 128 of them; adding the mesh's face check cuts that to 16 but also loses 7 of the 24 closed eyes it could have found (the mesh is unsure on some closed or turned faces). **A "cannot tell" rule is the real design problem, not the closed-eye score.**
3. **The size floor matters**: below about 96 px across the face the closed-eye recall falls (4 of 9 at 48-96) and below 48 px nothing could be tested. This is in pixels of the decode the model sees, so the pass needs the full-resolution decode (§8.4), not the grid thumbnail.
4. Its misses on closed eyes were mostly small faces in group photographs (48-76 px) and one glasses-and-shadow case at 133 px; its false "closed" calls were two open eyes (51 px and 100 px).

**Not established.** Real event photographs (this set is posed, well-lit public photographs plus closed-eye category photographs, so **the closed share is inflated by design** and precision on a real shoot will differ); eyes marked from 256 px crops, so a person looking at the full-resolution file might have been able to tell more (the 128 "cannot tell" may be an over-count for the 24-48 px faces); the eye-crop geometry (approximated from YuNet's eye centres); BlinkCNN's training-data licences (§8.9: still the user's decision); the mesh route's untried alternative (eyelid geometry from the 478 landmarks, which the blendshape does not use); time cost on a full import; Windows. **Next steps that need the user:** the licence decision on BlinkCNN, and then either (a) design the "cannot tell" gate (face width floor around 96 px at full resolution, mesh face check as a second test) and re-score on a real event folder, or (b) stop here and keep only the Soft chip.

### 8.13 A second labelling batch: group photographs, enriched for blinks (2026-11-11)

**Why.** §8.12 had no closed eye under 48 px and only 9 at 48-96 px, and 128 of 282 eyes were "cannot tell"; the gate cannot be designed from that. **What was fetched** (outside the repo, `mask_training/blink/commons2b`): 130 licence-filtered JPEG renditions (1,920 px wide, 75 MB; CC0, public domain, CC BY, CC BY-SA; `attribution.csv`) from the Commons categories Group photographs, Class photographs, Graduation ceremonies, Parties and Weddings. A first attempt at 2,000 px was throttled (HTTP 429) and was thrown away with its 24 files, because its credits were lost when it was stopped; the retry used a standard thumbnail size and slower pacing. YuNet found **2,737 faces** in them (1,558 under 48 px, 738 at 48-96, 218 at 96 or more, 223 under 24).

**The sample (137 faces) is stratified, not random**, so closed eyes are not left to chance: per size band, half the faces are drawn from those where either model gave a closed score of 0.3 or more on an eye ("flagged"), half from the rest ("unflagged"), at most 3 per photograph per band. 7 under 24 px (unflagged only: nearly every face that small is flagged by noise), 40 at 24-48, 60 at 48-96, 30 at 96 or more. The strata and pool sizes are kept (`strata2.json`, `pool2.json`) so that recall and precision can be **re-weighted to the real population** when the marks come back; reading raw rates off this sample would overstate the closed share. The page is the same as §8.11's, as batch 2 (its own browser storage, exports `labels_2.csv`).

**Not verified:** that the sample holds enough closed eyes (the marks will say), the flag threshold of 0.3 (a choice, loose on purpose), any accuracy figure, Windows. **Needs from the user:** the marks, as before.

### 8.14 The licence question looked at again, and a clean route that nearly matches BlinkCNN (2026-11-11)

**Licences, from search results (the pages themselves could not be fetched from here: the fetch tool refused those hosts).**
- **BlinkCNN's training data is worse than "unstated".** The paper's list of image corpora as reported in search results is **CEW, ZJU, MRL and RT-BENE** (still images; five video sets besides). CEW's own host page is quoted as **research use only, not for commercial use**; RT-BENE's Zenodo record is **CC BY-NC-SA 4.0 (non-commercial)**; no licence was found for MRL Eye, TalkingFace, Researcher's Night or HUST-LEBW. Weights trained on non-commercial data are not a clean base for a product: treat **candidate (a) as out unless the corpus owners say otherwise**.
- **Other eye-state models found.** Intel's Open Model Zoo `open-closed-eye-0001` (Apache-2.0 repository; trained on MRL Eye, whose licence was not found; 95.84 % on its own test); a `dima806` ViT (Apache-2.0, dataset not named on the pages seen, 99 % on its own test, a full ViT-base, so large); the `MichalMlodawski` classifiers are **CC BY-NC-ND (out)**. **None has a clean data story.**
- **More recent data:** no 2024-2025 closed-eye dataset with a commercial-use licence was found; CLERA (2023, 30k images) and HUST-LEBW have no licence in what was read.

**The route that needs none of that: eyelid geometry from the Apache-2.0 mesh.** `blink_measure.py` now also records the **eye aspect ratio (EAR)** from six mesh points per eye (corners, two upper and two lower lid points). The left / right mapping was checked from the data (EAR on the image-left eye correlates -0.82 with that side's blendshape, -0.57 with the other side). On the user's 282 marks (§8.12), with **EAR < 0.18** (the best cut on this very data, so **optimistic**):

| Rule, face >= 96 px and mesh sure of a face | accuracy | closed eyes found | "closed" calls right | cannot-tell eyes still answered |
|---|---|---|---|---|
| EAR < 0.18 | 94 % | 85 % | 100 % | 5 % |
| EAR < 0.18 **or** blendshape >= 0.5 | **94 %** | **90 %** | **95 %** | **5 %** |
| BlinkCNN >= 0.53 (for reference) | 94 % | 95 % | 90 % | 5 % |

So on large faces the **mesh-only route (clean licence, no trained eye classifier) is about as good as BlinkCNN**; the gap is at 48-96 px, where EAR-or-blendshape finds 88 % at 48 px + mesh sure but with 75 % of its calls right (BlinkCNN: 79 % and 86 %). The gate (face >= 96 px and mesh sure) lets through **39 % of the open / closed eyes** in this set, whose images are at most 2,000 px wide: **at a real 24-megapixel file faces are about three times larger in pixels, so the same floor covers far more**, but that is to be measured, not assumed. Half-closed eyes are called closed 60 to 73 % of the time by the mesh rules (BlinkCNN 45-60 %), which is fine for a badge a person confirms.

**Rules frozen before batch 2** (`blink_gate.py`): EAR 0.18, blendshape 0.5, mesh presence 0.5, face floors 48 / 64 / 96 px. Batch 2 (§8.13) is then a **hold-out**, scored with its strata weights so the rates describe the population, not the enriched sample. If the floor and thresholds hold out, the next step is a build slice (E1) on the mesh route; a small logistic fit on EAR, blendshape and face size from our own labelled sets is possible and would stay licence-clean (own labels, CC photographs), but is not needed yet.

**Not verified:** EAR 0.18 out of sample, anything about real event photographs, the 24 MP claim, the cost of running the mesh on a full import, Windows, the corpora's actual terms (search snippets only).
