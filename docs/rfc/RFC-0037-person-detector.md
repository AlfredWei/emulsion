# RFC-0037: Finding people who show no usable face (M7 adaptive tone, `person` region)

- Status: **Draft, 2026-10-31. Nothing built. Awaiting a go / no-go (§7).** The measurements in §2 were taken on local photographs with the trace hook (`EMULSION_ADAPTIVE_TRACE`); the figures about candidate models in §4 come from web search results and are marked where they were not confirmed.
- Companion documents: [RFC-0033](RFC-0033-adaptive-tone-presets.md) §3.1 (the face-to-click design this extends), [RFC-0035](RFC-0035-region-bound-presets.md) (region-bound presets), [PROGRESS.md](../../PROGRESS.md) (entries of 2026-10-30 and 2026-10-31)

## 1. Problem

The `person` region finds people **from their faces**: the stored face box becomes a click on the face and one on the torso, and the click-select model (SAM 2 Tiny) cuts each person. A photograph where nobody shows a usable face (seen from behind, a silhouette, a head turned away, a crowd of tiny faces under the 24 px floor) returns `not_found`. "Lift people" then does nothing, which is the honest answer, but it is a gap for landscapes with a figure, street and event photographs.

## 2. How big is the gap (measured 2026-10-31)

On the 40 portraits and crowds of `mask_training/people_eval_hires` (the person region traced end to end), ten photographs were `not_found`. Looked at one by one:

| Photograph | Why `not_found` | A real miss? |
|---|---|---|
| 03 | the back of a cabinet card, no person | no (correct) |
| 31, 32, 37 | cats and a dog | no (correct) |
| 35 | a child hidden behind a teddy bear | arguable |
| 21, 11 | faces found but under the model's confidence | no (a face problem, not this one) |
| 23 | a basketball game, the model is unsure of the one face | no (a model problem) |
| 28 | a tiny silhouette walking on a beach | **yes** |
| 24 | a tiny crowd in front of a building | **yes** |

So **two of 40 photographs** are real misses, and **the set contains almost no people seen from behind**, which is exactly the case this RFC is about. The evidence is too thin to size the problem; it shows only that the cases exist. Building the evaluation set (§6) is the first step whatever is decided.

Two experiments argue against the cheap answers:

- **No model, a blind click** (tried for the sky, 2026-10-31, rejected): clicking where a region is expected and trusting the click-select model's confidence does not work; it was confident and wrong (the branches, not the sky; 0.86-0.98 for right and wrong alike). The same would happen for a person.
- **Face detector scores** cannot be lowered to find more people: the detector already fires on a dog, teddy bears and a statue at 0.71-0.80 (kept now as `faces.score`).

## 3. Code facts (read 2026-10-31)

- `adaptive_regions::resolve_person(boxes, frame, model)` takes `FaceBox` rows and a `ClickModel`; every person is one face-and-torso cut. It has no other source of people.
- The click-select protocol (`ai/protocol.rs`: `SegmentPrepare`, `SegmentDecode`) carries **point prompts only** (`Prompt { x, y, positive }`). SAM 2's decoder takes a box prompt too, but nothing here sends one; using it needs a protocol and helper change.
- Two inference runtimes exist: `tract-onnx` in the Rust core (the face detector, ADR-0007) and `ort` (ONNX Runtime) in the AI helper process (SAM, NAFNet, Real-ESRGAN).
- Models: three are bundled today (`sam2_hiera_tiny` 155 MB, `nafnet_sidd_w32` 117 MB, `realesr_general_x4v3` 4.9 MB), listed in `models/bundled-models.json`, seeded on first launch.

## 4. Alternatives

1. **Leave as is.** Honest `not_found`, no new model, the user selects the person by hand (Select Subject) after the report says "not found". Cost: none. Recommended unless the evaluation set (§6) shows the gap is large.
2. **A COCO person detector in the AI helper**, giving person boxes; each box goes to the click-select model (centre and torso clicks inside the box, or a box prompt after a protocol change); boxes that overlap an existing face-based person are dropped.
   - **YOLOX-Nano** (Megvii, Apache-2.0): about 0.91 M parameters, 416 px input, 25.3-25.8 mAP on COCO; an ONNX export is reported at about 3.5 MB (**unconfirmed**: a third-party listing; the official weights are `.pth`, so the export would be ours to produce and to check).
   - **NanoDet-Plus-m** (RangiLyu): 30.4 mAP at 416 px, 1.17 M parameters, about 2.3 MB at FP16; has an ONNX export script. **Licence unconfirmed from upstream** (only a fork's metadata said Apache-2.0); read the upstream LICENSE first.
   - **YOLOv8 / Ultralytics: rejected**, AGPL-3.0, which a closed or MIT-licensed distribution cannot take on without a separate commercial licence.
   - Costs: a new model file and its licence and attribution, a second inference per photograph (tens of milliseconds for a nano model; not measured here), the risk of a person detected with no good cut (the same "moderately sure" band as now), and a failure mode on small figures (a nano detector is weak under about 32 px).
3. **Pose or body-part keypoints** (to click on the torso accurately): more accuracy for the torso click, but a bigger model, and the detector alone does not need it. Not pursued.
4. **SAM automatic mask generation** (a grid of clicks, keep person-like shapes): needs a classifier to say which cuts are people, so it needs a second model anyway, and a grid is many decodes. Rejected for cost.

## 5. Proposal (if the answer to §7 is go)

Alternative 2 with the smallest change set, behind the same trace hook so thresholds come from measurement as the face-based ones did (RFC-0035 §8):

- The AI helper gets a `detect_people` request returning boxes with scores (`score >= 0.5`, non-max suppressed). The model is bundled like the others only if its size stays in the single megabytes; otherwise it follows the Select Subject download flow.
- `resolve_person` gains a second source: person boxes without an overlapping face-based person (IoU of the box with the face-based cut over 0.3 drops it). Each becomes a centre click and a click at the lower-middle of the box, cut by the same `ClickModel`; the same confidence band (`PERSON_MIN_IOU`, `PERSON_SURE_IOU`) applies.
- **Never `found` from a detector box alone at first**: a person with no face is reported as `check` ("found without a face"), as the model-only sky would have been, until the evaluation set shows the rate of wrong cuts.
- Face-based behaviour and its tests stay as they are; a photograph with faces never calls the detector (no new cost for the common case).

## 6. Evaluation before any code

A set that does not exist yet: roughly 30 photographs with people **seen from behind, in silhouette, turned away, small in a landscape, and in crowds**, plus 20 with no person (landscapes, animals, statues, mannequins, posters), from freely licensed sources like the existing sets, each with a person yes/no and, for the yes ones, a hand-checked mask or at least a pass/fail by eye on a contact sheet. Success means: at least 70 % of the people found with a mask worth keeping, and at most 1 in 10 of the no-person photographs getting a `check`. The same harness (`develop-adaptive-eval.e2e.js`, one launch for the whole set) is used; it opens the app once per run.

## 7. Decisions needed from the user

1. **Go or no-go on a person detector at all.** Recommendation: **build the set first (§6); decide with its numbers.** Without it the case for a new model is two photographs.
2. If go: **YOLOX-Nano or NanoDet-Plus** (licences to be confirmed first, §4), and whether the detector is **bundled or downloaded**.
3. Whether a person found without a face may ever be `found`, or stays `check`.

## 8. What this does not change

The face-based path, the face score rules (2026-10-31), the chest-click retry, and the sky are untouched. The catalog schema does not change.
