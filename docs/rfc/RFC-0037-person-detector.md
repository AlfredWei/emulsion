# RFC-0037: Finding people who show no usable face (M7 adaptive tone, `person` region)

- Status: **Accepted 2026-10-31: go, D-FINE medium, never `found` from a detector box alone (§8); being built on `feature/person-detector` (§10).** Models were compared on COCO val2017 (§3). The comparison ran offline in Python, with no app launches; the scripts are in [RFC-0037-appendix](RFC-0037-appendix/). Model sizes were measured on the downloaded files. Licences are taken from the Hugging Face model cards (Apache-2.0) and still need a check against each upstream repository's LICENSE file.
- Companion documents: [RFC-0033](RFC-0033-adaptive-tone-presets.md) §3.1 (the face-to-click design this extends), [RFC-0035](RFC-0035-region-bound-presets.md) (region-bound presets), [PROGRESS.md](../../PROGRESS.md) (entries of 2026-10-30 and 2026-10-31)

## 1. Problem

The `person` region finds people **from their faces**: the stored face box becomes a click on the face and one on the torso, and the click-select model (SAM 2 Tiny) cuts each person. A photograph where nobody shows a usable face (seen from behind, a silhouette, a head turned away, a crowd of tiny faces under the 24 px floor) returns `not_found`. "Lift people" then does nothing, which is the honest answer, but it is a gap for landscapes with a figure, street and event photographs.

## 2. How big is the gap on our own photographs (2026-10-31)

On the 40 portraits and crowds of `mask_training/people_eval_hires`, ten photographs were `not_found`. Looked at one by one, **two of 40** are real misses (28, a tiny silhouette on a beach; 24, a tiny crowd); the others are correct (a card back, cats and a dog), arguable (a child behind a teddy bear) or face / model problems. That set has almost no people seen from behind, so it cannot size the problem. §3 measures it on a larger public set instead.

Two cheap answers were tried and rejected: a blind click trusting the click-select model's confidence (it was confident and wrong for the sky, IoU 0.86-0.98 for right and wrong alike), and lowering the face detector's score (it already fires on a dog, teddy bears and a statue at 0.71-0.80).

## 3. Measured: which detector, and how to cut from its box

**Set.** COCO val2017, 700 images, each person with a COCO box and mask. A person is *faceless* when the app's face detector (the numpy port of YuNet, with the app's 24 px floor at a 1600 px frame) finds no face centred in the upper half of the person's box, and the person is at least 8 % of the frame tall. 300 images have no kept face at all (the app answers `not_found` today), 200 have a face elsewhere plus a faceless person, 200 have no person (half animals and toys). That is 1,248 faceless target persons; by height 438 / 406 / 265 / 139 for 8-15 / 15-30 / 30-60 / over 60 % of the frame. All Flickr licences are used: this is a local evaluation, nothing is trained, shipped or redistributed; the licence id is kept per image so a run on the permissive subset (ids 4, 7, 8) is possible. COCO images are about 640 px, smaller than the app's 1600 px preview, which makes small figures harder than they would be in the app.

**Detectors** (20 scored; every file or weight set under the 200 MB target). `found` is the share of faceless persons detected (box IoU 0.5) at the lowest threshold that flags at most 10 % of the person-free images; `ms` is per image (torch models on the Apple GPU, YOLOX on the ONNX Runtime CPU: indicative, not in-app numbers).

| Model | Licence (card) | Size | ms | AP50 | found | 8-15 % tall |
|---|---|---|---|---|---|---|
| D-FINE medium | Apache-2.0 | 79 MB | 73 | 0.879 | **0.95** | 0.91 |
| RT-DETRv2 r50 | Apache-2.0 | 172 MB | 119 | 0.887 | 0.94 | 0.90 |
| Faster R-CNN v2 (torchvision) | BSD-3 | 175 MB | 275 | 0.884 | 0.94 | 0.90 |
| D-FINE small | Apache-2.0 | **42 MB** | 48 | 0.860 | 0.93 | 0.87 |
| RT-DETRv2 r18 | Apache-2.0 | 81 MB | 57 | 0.866 | 0.92 | 0.88 |
| YOLOX-m | Apache-2.0 | 101 MB | 252 | 0.867 | 0.92 | 0.87 |
| Mask R-CNN v2 (torchvision) | BSD-3 | 185 MB | 375 | 0.881 | 0.92 | 0.89 |
| YOLOX-s | Apache-2.0 | 36 MB | 107 | 0.826 | 0.88 | 0.82 |
| YOLOX-tiny | Apache-2.0 | 20 MB | 33 | 0.760 | 0.82 | 0.72 |
| YOLOX-nano | Apache-2.0 | 3.7 MB | 16 | 0.684 | 0.75 | 0.59 |
| SSDlite320 MobileNetV3 | BSD-3 | 14 MB | 79 | 0.471 | 0.65 | 0.37 |

The remaining nine (RT-DETR r18 / r34 / r50, YOLOX-l, YOLOS tiny / small, RetinaNet v2, FCOS, Faster R-CNN MobileNetV3) fall between these rows (found 0.82-0.94); the full output is reproducible with the appendix scripts. DETR ResNet-50 (167 MB) was not scored: it needs `timm`, which was installed too late to run it.

- **The nano-class models from the first draft of this RFC lose clearly**: YOLOX-nano finds 75 %, and only 59 % of the small figures, where the 200 MB budget buys D-FINE at 93-95 %.
- **Detection is not the limit; D-FINE small, D-FINE medium and RT-DETRv2 r18 cut the same.** Their SAM cuts are within a point of each other, so the choice is size and speed.
- **ONNX:** D-FINE medium exports to a 78.6 MB ONNX (opset 17, fixed 640 x 640 input) whose output matches the torch model exactly on real photographs, so it can run in the AI helper's ONNX Runtime. The first comparison on random noise looked wrong only because the decoder's top-k picks differ on ties. CPU timing in the helper was not measured cleanly (the machine was busy).

**From box to mask.** The detector box goes to SAM 2 Tiny (the app's own preprocessing and decoder, ONNX Runtime), scored against the COCO mask on the faceless persons that were detected (D-FINE small, score at least 0.5, 869 persons):

| Prompt | IoU at least 0.7 | IoU at least 0.5 | mean IoU |
|---|---|---|---|
| one click, middle of the box | 0.46 | 0.61 | 0.56 |
| two clicks, 35 % and 65 % of the height (today's face-based style) | 0.65 | 0.82 | 0.68 |
| **the box itself** (SAM 2 box prompt: two corner points, labels 2 and 3) | **0.82** | **0.97** | **0.80** |
| box plus a click in the middle | 0.81 | 0.95 | 0.79 |

The box prompt is clearly the best and needs a small protocol change: `Prompt` carries a `positive` flag only, so it needs a label (positive, negative, box corner 1, box corner 2).

**SAM's own confidence cannot reject a wrong detection.** At the app's gate (predicted IoU 0.6), 91 % of the cuts of real people pass but so do 80-88 % of the cuts of false detections (with the box prompt 100 % and 99 %). A detection that is not a person is cut as confidently as one that is. **The detector's own score has to be the gate.**

**Combined estimate (D-FINE small, detector score 0.4, box prompt).** Faceless persons detected: 0.87 (0.80 / 0.87 / 0.95 / 0.95 by size). The cut statistics above were taken at detector score 0.5, so this is approximate at 0.4. Of the detected ones, 0.82 get a mask at IoU 0.7 or better and 0.97 at IoU 0.5 or better, so about **0.71 of all faceless people get a mask at IoU 0.7 (0.84 at IoU 0.5)**. Person-free photographs flagged: **4.0 %** (animals and toys 4.0 %); at score 0.5 it is 1.5 % with 0.81 found. D-FINE medium at 0.5: 0.85 found, 5.0 % flagged. This meets the §7 bar (at least 70 % usable, at most 1 in 10 flagged) **on COCO**, with IoU 0.7 as the strict reading of "usable"; it has not yet been shown on our own photographs.

## 4. Code facts (read 2026-10-31)

- `adaptive_regions::resolve_person(boxes, frame, model)` takes `FaceBox` rows and a `ClickModel`; every person is one face-and-torso cut. It has no other source of people.
- The click-select protocol (`ai/protocol.rs`: `SegmentPrepare`, `SegmentDecode`) carries **point prompts only** (`Prompt { x, y, positive }`). SAM 2's decoder takes box corners as points with labels 2 and 3 and the helper already feeds `point_labels`, so the change is a label field in `Prompt` and in `pack_prompts` (and no padding point when a box is present).
- Two inference runtimes exist: `tract-onnx` in the Rust core (the face detector, ADR-0007) and `ort` (ONNX Runtime) in the AI helper (SAM, NAFNet, Real-ESRGAN). A transformer-decoder detector belongs in the helper.
- Models: three are bundled today (`sam2_hiera_tiny` 155 MB, `nafnet_sidd_w32` 117 MB, `realesr_general_x4v3` 4.9 MB), listed in `models/bundled-models.json`, seeded on first launch.

## 5. Alternatives

1. **Leave as is.** Honest `not_found`; the user selects the person by hand. Cost: none. The measured gap on COCO (§3) is not small: of the people the face path cannot see, about 70 % could get a usable mask.
2. **A COCO person detector in the AI helper, boxes cut with a SAM box prompt (recommended if go).** D-FINE small (42 MB) or medium (79 MB); RT-DETRv2 r18 (81 MB) is an equal alternative on quality.
3. **YOLOX / NanoDet nano models** (the draft's first candidates): rejected by §3, too weak on small figures.
4. **YOLOv8 / Ultralytics: rejected**, AGPL-3.0.
5. **Mask R-CNN as detector and cutter in one** (185 MB): not better (found 0.92) and bigger than D-FINE plus SAM, which is already installed.
6. **Pose keypoints, SAM automatic masks:** more cost, no measured need.

## 6. Proposal (if the answer to §8 is go)

- The AI helper gets a `detect_people` request returning boxes with scores (D-FINE, fixed 640 x 640 squashed input, sigmoid on the person logit, score floor 0.4). The model follows the Select Subject download flow, or is bundled if the choice is the 42 MB small one.
- `SegmentDecode` accepts box prompts (label field on `Prompt`).
- `resolve_person` gains a second source: detector boxes without an overlapping face-based person (IoU of the box with the face-based cut over 0.3 drops it). Each is cut with the box prompt; boxes the cut covers under 1 % of the frame are dropped, as for faces.
- **Never `found` from a detector box alone at first**: a person with no face is reported as `check` ("found without a face"), because SAM's confidence cannot tell a wrong detection from a right one. The detector's score is the only gate (§3).
- A photograph with faces never calls the detector unless the face path leaves the frame with a large uncovered person; the first version calls it only when the face path returns `not_found`, so the common case costs nothing.
- Face-based behaviour and its tests stay as they are. Thresholds come from the trace hook (`EMULSION_ADAPTIVE_TRACE`) on our own photographs, as for the face-based ones.

## 7. Evaluation still to do before the code is trusted

COCO is a proxy: small images, a public benchmark the detectors were trained on the train split of. Before shipping: run the same thing on our own photographs, about 30 people seen from behind, in silhouette or small in a landscape plus 20 with no person (freely licensed, like the existing sets), by eye on a contact sheet, in one app launch. Success: at least 70 % of the people found with a mask worth keeping, at most 1 in 10 no-person photographs flagged (COCO estimate: 71 % and 4 %). The two real misses of §2 (28 and 24) are the first cases to look at: the smoke test finds the beach silhouette at 0.91-0.99 and 21-34 people in the tiny crowd.

## 8. Decisions needed from the user

1. **Go or no-go on a person detector.** The comparison removed the doubt about feasibility; what is left is whether the feature is worth a 42-79 MB model and a protocol change. Recommendation: **go**, as `check` only, behind the §7 check on our own photographs.
2. **Which model and how it ships:** D-FINE small (42 MB, bundled) or medium (79 MB, downloaded; 5 points more recall at the same false-alarm rate). Recommendation: small, bundled, with medium as a later upgrade.
3. Whether a person found without a face may ever be `found`, or stays `check`.

### Decisions (2026-10-31)

1. **Go.**
2. **D-FINE medium** (79 MB), shipped in the installer beside the Select Subject files like the other models (no download dialog in this slice).
3. **Never `found` from a detector box alone**: the result is `check` (the default proposed in §6, not objected to).

## 9. What this does not change

The face-based path, the face score rules (2026-10-31), the chest-click retry, and the sky are untouched. The catalog schema does not change.

## 10. Build notes (2026-10-31)

- **The helper's ONNX Runtime (ort 2.0.0-rc.13) has no CPU kernel for `Cos`**, which D-FINE's positional embedding needs; the Python runtime (1.30) ran the same file. The shipped file is therefore the export constant-folded with ORT's basic graph optimisation (3431 to 1085 nodes, only standard ai.onnx ops, 78,809,647 bytes, output equal to the original to 1e-5). `export_onnx.py` does both steps.
- **A box prompt carries no padding point** (SAM's convention; the earlier Python test had one). Measured on D-FINE medium, 559 faceless people: 82.1 % of cuts at IoU 0.7 without it, 81.6 % with it.
- **Per-person coverage floor 0.05 %** (not the face path's 1 %): the figures of the tiny crowd (photo 24) are 2 % wide and 5 % tall, about 0.1 % of the frame each.
- **Detector score floor 0.5**, from the D-FINE medium curve: 0.4 finds 89 % and flags 6.5 % of person-free photographs, 0.5 finds 85 % and flags 5.0 %, 0.6 finds 78 % and flags 2.0 %.
- Real weights through the real helper (no app): the beach silhouette (28) is detected at 0.93 and its box cut is rated 0.94; the tiny crowd (24) gives 15 people, cuts rated 0.85-0.90.
- **In the app, one launch over the 37 portraits and crowds (the `develop-adaptive-eval` harness, region `person`):** the five photographs with a real person and no usable face now give a `check` with a good outline on a contact sheet: 28, the beach silhouette (1 person, reflection not selected); 24, the tiny crowd (8 of 15 figures); 11, the group of ministers (8 largest of 16; the face path had given up); 21, the festival crowd (8 of 12, the seated people in front); 23, the basketball game (4). The five photographs with nobody to find stay `not_found`, and the detector returned **no candidate at all** on them: the back of a cabinet card (03), two cats (31, 32), a dog (37) and the child hidden behind a teddy bear (35). Every photograph that had a face path result keeps it (the detector runs only after `not_found`). A call that falls through to the detector takes 3.3-5.4 s (the Select Subject encoder about 2-3 s, the detector about 1 s, and up to 8 cuts of about 50 ms).
- **Release asset not yet published.** `models/bundled-models.json` names `https://github.com/AlfredWei/emulsion/releases/download/person-detector-dfine-medium-coco-v1/dfine_medium_coco.onnx`; the release (a public upload of the 79 MB file) has to be created before `npm run build:release` can fetch it.
