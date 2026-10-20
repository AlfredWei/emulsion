# RFC-0031: Intelligent culling: similar-photo groups and a soft-shot cue (M7 slice 1)

- Status: Draft for review
- Date: 2026-10-20
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
- The analysis cost is small next to decoding (the probe's time is nearly all JPEG decode, and §3.1 takes the pixels from a decode that already happens). The metric alone was not timed separately.

## 3. Decisions

### 3.1 What is computed, when, and where it is stored

For every photo, once, from the **unedited 1024 px picture the Large thumbnail pass already has in hand** (RFC-0030): `sharpness` (whole) and `sharpness_tile` (90th percentile) as REAL, and `dhash` as a 32-byte BLOB.

- **Why in the thumbnail pass:** no extra decode of a 24 MP source, and the pixels are guaranteed unedited (a thumbnail regenerated after an edit, `{id}-{hash8}.jpg`, has the edit baked in; a crop would change the hash and a sharpen the score). For a RAW whose embedded JPEG is the final thumbnail, the embedded picture is analysed; if its long edge is under 640 px (some cameras' embedded previews are tiny) the pass falls back to the draft decode, as the thumbnail code already does.
- **Storage:** a new table `cull_analysis(image_id INTEGER PRIMARY KEY REFERENCES images ON DELETE CASCADE, sharpness REAL, sharpness_tile REAL, dhash BLOB, algo INTEGER, analysed_at TEXT)`. `algo` is a version number: changing the metric bumps it and old rows are recomputed in the background. About 50 bytes a photo (2.5 MB for 50,000). Catalog migration is additive, `CREATE TABLE IF NOT EXISTS` like the others.
- **Existing catalogs:** a background backfill (the `backfill_missing_thumbnails` pattern) decodes the plain `{id}.jpg` Large thumbnail when it exists and the source otherwise, one low-priority pass that yields to import and to the Large pass (`FOREGROUND_WORK`).
- **Never changes the photo, never writes a sidecar, never leaves the machine.**

### 3.2 Grouping

Groups are **derived, not stored**: computed from `cull_analysis` plus `captured_at` and the camera on demand, in memory, so there is no stale state to repair when photos are added, removed or re-analysed. Pure function, unit-testable without a window:

1. Sort by `captured_at` (photos without one are not grouped; their hash alone is too weak, §2).
2. Walk a window: a photo joins a group when it is within **T seconds** of any member's capture time **and** its hash distance to that member is within **D bits**; same camera model required. Union-find over the window; groups of one are dropped.
3. Starting values: **T = 10 s** (a burst, and a posed re-take), **D = 64 of 256**. The measurement supports "D between 59 and 95"; 64 is the conservative end of that gap, to be tuned on real bursts (§6). Both are exposed as one **Similarity** slider in the UI, not as two numbers.
4. **Suggested best**: the member with the highest `sharpness_tile`, *only if* it beats the runner-up by a margin; otherwise "no clear best" (equal sharpness is information too). The suggestion is a badge, never an action.

### 3.3 The soft-shot cue is relative and only inside a group

A photo is shown as **Soft** when its `sharpness_tile` is under **50 %** of the group's best (the measurement: a 0.6 px blur at 1024 px already lands at 25 to 53 %). A photo that is in no group gets **no badge**: §2 shows a single photo cannot be judged on its own number (noise inflates it; a flat sky deflates it), and a wrong "this is blurry" on a keeper is worse than no cue. A later slice may add an image-quality model for singles (§5, slice E).

### 3.4 UX

Not a mock yet; the shape, to be judged by the user before slice C builds anything:

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
- **Cost of the metric** inside the Large pass is expected to be small but was not timed; slice A measures it against RFC-0030's import numbers.
- Windows, and a 50,000-photo catalog's grouping time.

## 7. Questions for the user

1. **Reject the others:** is a bulk reject with one confirmation right, or would you rather the group only *marks* suggestions and you press reject yourself?
2. **Similarity:** one slider ("looser ↔ stricter") is the plan. Is a fixed value enough for the first version?
3. **Real bursts:** can you provide (or point to) a folder of unculled burst-heavy photos, ideally RAW and JPEG, for slice D? Without them the thresholds stay unproven.
4. **Order of E:** blink detection first or the single-photo quality model first, after slice D?
