# RFC-0005: Face detection & People view

- Status: Draft — for review in PR (flips to Accepted once merged)
- Date: 2026-09-06
- Companion documents: [ADR-0003](../adr/ADR-0003-raw-decoding.md), [ADR-0005](../adr/ADR-0005-catalog-storage.md), [MILESTONES](../../PRD/MILESTONES.md#m5--performance-gpu-merges-faces), [PROGRESS.md](../../PROGRESS.md)

## 1. Problem

M5's scope names *"Face detection/recognition for a local 'People' browsing view (fully local, no cloud model dependency)"* as its own line item, alongside the GPU track, HDR merge, and Panorama merge (all shipped). M5's own exit criterion for this line item is explicit: *"Face-grouping is usable for culling/organizing a portrait- or event-heavy catalog."*

This is a different kind of slice than HDR/Panorama merge. Those were **hand-rolled, well-known, directly-testable algorithms** (MTB alignment, Harris corners, DLT homography) — no external model or inference runtime was needed, matching this codebase's own precedent of avoiding heavy dependencies for compact math it can implement and unit-test directly. Face detection and recognition are **not** compact hand-rollable algorithms — they require a trained neural network. This RFC's real work is therefore an **architecture decision** (which inference runtime, which models, how they're obtained and licensed) before any pipeline/catalog/UI design, which is why it warrants an RFC on its own rather than folding into the panorama-style "just write the math" pattern.

## 2. Non-goals

- **Open-set / named identification against an external database.** This is unsupervised **grouping** of faces detected within the user's own catalog (matching the exit criterion's own word: "face-grouping"), not identifying *who* someone is against any outside source. The user assigns their own names to clusters, entirely locally — never sent anywhere.
- **Cloud recognition APIs of any kind.** Non-negotiable per M5's own scope line ("fully local, no cloud model dependency") and this project's standing local-first constraint ([[lr-replace-project-context]]). Model *weights* may be fetched once from the internet at build/setup time (see §3.2) — inference itself never makes a network call.
- **Real-time video face tracking.** Static-photo library only; M5's separate "basic video handling" scope item (not yet started) is out of scope here regardless.
- **Age/gender/emotion/attribute estimation.** Some face-recognition crates bundle this; not part of this slice even if the chosen dependency happens to expose it.
- **Liveness/spoof detection, pose-invariant re-identification guarantees, or any claim of forensic-grade accuracy.** This is a culling/organizing aid, not a security or legal-identification feature — framed honestly in UI copy, not oversold.
- **A full "merge two people" / "split a cluster" polished UI.** v1 needs *basic* correction (a person is clearly two different people, or one person got split into two groups) since no clustering approach is perfect, but the interaction can be as simple as drag-a-face-between-groups or a "not this person" action — not a dedicated review workflow. A dedicated fast-review UI is a reasonable follow-up, not silently promised here.
- **Automatic re-clustering on every single import.** Re-running clustering over the whole catalog on every import would not scale (matches the exact O(catalog) rescan mistake §M5-Slice-"Import progress" already found and fixed for thumbnail backfill — see PROGRESS.md 2026-09-05). New faces are detected per-import-batch; full re-clustering is a separate, explicitly-triggered (or infrequent background) step, detailed in §3.5.

## 3. Design

### 3.1 Pipeline shape

Two independent stages, matching the "detect once, cluster separately" shape any face-grouping system needs:

1. **Detection + embedding** (per-image, incremental): for each image, run a face **detector** (bounding boxes + a handful of landmark points for alignment), then a face **embedding** model on each detected, aligned face crop, producing a fixed-length numeric vector ("embedding") such that two crops of the same real person's face have embeddings close together (Euclidean or cosine distance) and two different people's are far apart. This is a per-image, embarrassingly-parallel-across-images operation with no cross-image state — same shape as thumbnail generation.
2. **Clustering** (batch, cross-image): group all embeddings in the catalog into clusters, each cluster becoming a "person" the user can name. Unlike stage 1, this genuinely needs to look at the whole (or a growing subset of the) catalog at once, so it can't be scoped per-import-batch the way thumbnail generation was — see §3.5 for how this is bounded in practice.

### 3.2 Inference runtime: `tract`, not `ort` — informed directly by this project's own Windows history

Two real options exist in the Rust ecosystem for running a pretrained ONNX face model:

- **`ort`** (ONNX Runtime's official Rust bindings): mature, fast, broad op coverage, GPU execution providers available — but it FFI-links Microsoft's native `onnxruntime` C++ library, which must be present/linked per-platform.
- **`tract`**: a pure-Rust ONNX/NNEF inference engine (Sonos). No native library to link, no C/C++ toolchain or vcpkg step required — at the cost of narrower op coverage and no GPU backend (CPU only).

**Recommendation: `tract`.** This project already paid the cost of a native-library Windows linking saga once — `rsraw`/`rsraw-sys`'s vendored LibRaw needed a version-pinned vcpkg install, a build-script patch, and two real CI debugging passes to get right (ADR-0003, and the 2026-09-05 "real root cause" investigation in PROGRESS.md where a vcpkg version drift silently corrupted struct layouts). Adding a second native-library FFI dependency (`ort` → `onnxruntime`) reintroduces the exact same *class* of cross-platform linking risk this project has already been burned by once. `tract` being pure Rust sidesteps that risk entirely for this slice — matching the same "avoid a lot of dependency weight/risk for what a lighter approach can cover" instinct that led Panorama merge (RFC-0004 §3) to drop `imageproc` in favor of hand-rolling.

**Named, accepted cost of this choice**: `tract`'s op coverage must be verified empirically against whatever specific ONNX model is chosen (§3.3) before committing — not assumed. If a candidate model uses an op `tract` doesn't support, the fallback is either a different model or (only if no viable `tract`-compatible model exists) revisiting this decision, not silently forcing `ort` in through the back door. CPU-only inference is an accepted tradeoff too: face detection/embedding over a photo library is a background/batch job (§3.5), not a `<100ms` interactive path like Develop's GPU shader (ADR-0004) — throughput, not latency, is what matters here, and `tract`'s CPU performance is well-documented as adequate for small (mobile-class) detection/embedding models at that pace.

### 3.3 Models

Two small, well-known model families fit this use case:

- **Detection**: an "ultra-light" mobile-class face detector (bounding box + 5-point landmarks for eyes/nose/mouth-corners, used to align the crop before embedding) — e.g. the family of ~1MB RFB/SCRFD-style detectors already available pre-converted to ONNX in several public repos.
- **Embedding**: a compact face-embedding network (e.g. a MobileFaceNet-class model producing a 128–512-dim vector) trained with a metric-learning loss (ArcFace/CosFace-style) specifically so that Euclidean/cosine distance between embeddings is meaningful — this is the property clustering (§3.5) depends on.

**Open question, deliberately not resolved in this draft — needs an explicit decision before implementation, same as HDR merge's CC BY 4.0 bracket-license exception (PROGRESS.md, 2026-09-06) was**: exactly which model files, from which source, under which license. Unlike this codebase's existing native dependencies (LibRaw, lcms2 — both established open-source libraries with clear licenses), pretrained face-recognition model weights are a messier landscape — some widely-circulated ONNX conversions of these model families do not clearly restate the original training license, and some upstream face datasets/models carry research-only or non-commercial terms that would not be appropriate to ship in a real product. **Before any implementation commit, the specific model files' provenance and license must be verified and recorded** (mirroring `test_image/hdr-bracket-cc-by/ATTRIBUTION.md`'s precedent of a directory-local notice, e.g. a `models/FACE_MODELS.md` naming exact source URLs, versions, and licenses) — this is a research task in its own right, not a rubber-stamp.

Models are fetched **once** (at first use or as a build step) and cached/vendored locally — never re-fetched at inference time. Whether they're committed to the repo (small enough: these model families are typically 1–5MB each, well within what `test_image/` already accepts) or downloaded on first run into the app's data directory (like the `raw.pixls.us` CI sample) is a secondary decision to make once the specific models/licenses are settled — leaning toward **committing them** (simpler, no network dependency for a fresh clone/build, consistent with "fully local" framing) unless license terms specifically prohibit redistribution.

### 3.4 Catalog schema

New tables, following this codebase's existing provenance/junction-table conventions (`hdr_merge_sources`, `image_keywords`):

```sql
CREATE TABLE IF NOT EXISTS people (
    id INTEGER PRIMARY KEY,
    name TEXT,                    -- NULL until the user names it ("Person 3" shown in UI)
    cover_face_id INTEGER,        -- FK to faces.id, set after that table exists (see below)
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS faces (
    id INTEGER PRIMARY KEY,
    image_id INTEGER NOT NULL REFERENCES images(id) ON DELETE CASCADE,
    person_id INTEGER REFERENCES people(id) ON DELETE SET NULL,
    bbox_x REAL NOT NULL, bbox_y REAL NOT NULL,
    bbox_w REAL NOT NULL, bbox_h REAL NOT NULL,  -- normalized 0-1, resolution-independent
    embedding BLOB NOT NULL,       -- fixed-length f32 vector, little-endian
    detected_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_faces_image_id ON faces(image_id);
CREATE INDEX IF NOT EXISTS idx_faces_person_id ON faces(person_id);
```

`person_id` starts NULL (detected but not yet clustered/assigned) — clustering (§3.5) is what populates it. `embedding` is per-`images` row, not per-`image_versions`: a face's identity doesn't change across virtual copies or edits of the same source image, matching how `content_hash`/EXIF already live on `images` rather than `image_versions`. `remove_images` needs the same both-directions cleanup this codebase already applies to `hdr_merge_sources`/`panorama_merge_sources` (deleting an image removes its `faces` rows via `ON DELETE CASCADE`; deleting a `people` row via a merge/cleanup action sets `faces.person_id` back to NULL via `ON DELETE SET NULL`, not deleting the underlying face detections).

### 3.5 Clustering: hand-rolled, bounded, re-triggerable — not real-time

Unlike detection/embedding (which needs a trained model), clustering over a set of already-computed embeddings is exactly the kind of compact, well-understood, directly-testable algorithm this codebase's own precedent favors hand-rolling (RFC-0003/0004): a simple **greedy threshold-based** or **basic agglomerative** clustering over cosine distance is well within reach without a new dependency, and is easy to unit-test against hand-picked synthetic embedding vectors with known expected groupings (same "hand-computed expected value" testing culture as RFC-0003 §3.4/0004 §3.4's homography tests).

- **Incremental-friendly, not O(catalog²) on every run**: a new face's embedding is compared only against each existing **cluster's centroid** (mean of its members' embeddings), not every individual face — assigned to the nearest cluster if under a distance threshold, otherwise seeds a new singleton cluster. This degrades gracefully (a full pass is `O(faces × clusters)`, not `O(faces²)`) and matches the "batch-scoped, not whole-catalog" lesson already learned the hard way for thumbnail backfill (PROGRESS.md, 2026-09-05: a whole-catalog-scoped operation against a real accumulated backlog took 5.5+ minutes).
- **Trigger points**: after a per-import-batch detection pass finishes (new faces from that batch get clustered against existing centroids immediately — cheap, since it's bounded by that batch's own face count), plus a user-initiated "Find people" / re-cluster action in the People view for when the automatic incremental assignment needs a fuller pass (e.g. after the user has manually corrected several clusters, or wants the whole catalog reconsidered from scratch with an adjusted threshold).
- **Threshold is a real, named tuning risk**: too tight over-splits one person into many small clusters (annoying but recoverable — the user can merge them); too loose merges two different people (worse — silently wrong). No amount of design reasoning alone settles the right number; it must be tuned empirically against a real, varied test set (§4) before shipping, and surfaced as a per-catalog adjustable setting if a single fixed default proves not to generalize (an explicit fallback, not assumed necessary from the start).

### 3.6 Frontend: People view

- New top-level view/module (alongside Library/Develop/Print — `+page.svelte`'s existing `activeModule` switch), showing one grid cell per `people` row (cover face crop + name-or-"Person N" + count), reusing `LibraryGrid.svelte`'s existing virtualization approach rather than a new one.
- Clicking a person filters the Library grid to every image containing at least one face assigned to them (a new Library filter dimension, alongside the existing rating/flag/color/date/camera/lens filters already in the titlebar).
- Naming: click a person's name to edit inline, persisted via a new `rename_person` command.
- Basic correction (§2's named non-goal boundary: basic only) — a face can be moved to a different existing person or marked "not a face"/"unknown" (excludes it from clustering without deleting the detection row, in case of a bad crop rather than a bad cluster assignment).
- New Tauri commands: `list_people`, `get_faces_for_image`, `rename_person`, `reassign_face(face_id, person_id | null)`, `recluster_faces` (the explicit re-run action).

### 3.7 Performance / background execution

Detection+embedding runs as a background job per import batch, same shape as `generate_missing_thumbnails_with_progress` (PROGRESS.md, 2026-09-05): a new `face-detection-progress` event, a startup catch-up pass for images cataloged before this feature existed, sequential (not a worker pool, matching the preview-cache precedent's own accepted CPU-spike-avoidance tradeoff — RFC-0003's own non-goals don't cover this, but M1 Slice 4's `preview_cache.rs` already made this exact call and named the same tradeoff explicitly).

## 4. Testability

- **Rust unit tests**, no model/network needed:
  - Clustering: hand-picked synthetic embedding vectors with known expected groupings (tight cluster of near-identical vectors vs. clearly-separated groups; a borderline pair exactly at the threshold boundary) — verifies the centroid-assignment and threshold logic in isolation from any real model.
  - Catalog: `faces`/`people` CRUD, `remove_images`'s cascade/set-null behavior, `reassign_face` round-trips.
  - Bbox/embedding blob encode-decode round-trip.
- **Real-model integration test**, gated behind an env var pointing at real committed (or locally-cached) model files, same pattern as `EMULSION_TEST_HDR_BRACKET_DIR` — confirms the chosen `tract` + model combination actually loads and runs, producing embeddings of the expected dimensionality on a known test image.
- **Real-photo end-to-end clustering test — a genuinely different problem than HDR/Panorama's test-fixture sourcing, flagged explicitly, not glossed over**: HDR merge and Panorama merge both needed *some* real photo, and the hard part was **licensing** (RFC-0003/0004's own sourcing sagas, PROGRESS.md 2026-09-06). Face-grouping's real end-to-end test needs multiple real photos of the *same, distinguishable real people, several times each* — which raises a **privacy/consent** question that a CC0 landscape or a CC BY bracket of static objects never did, licensing terms aside. Concretely: even a CC0-licensed photo of a real, identifiable person does not automatically mean it's appropriate to commit into this repo's test fixtures for a face-*recognition* feature specifically, or to run through third-party model weights. **Before sourcing any such fixture, this needs an explicit decision from the user** — options include: (a) the user's own photos, used purely locally (env-var-gated to a local, never-committed directory, the same pattern already used for `EMULSION_TEST_PANORAMA_DIR`), never checked into the repo; (b) a small number of consenting individuals' photos with explicit written permission for this specific use, if a committed CI fixture is wanted; (c) skip a real-photo CI fixture entirely and rely on unit tests + local-only manual verification, same honesty framing as RFC-0003 §4's "no such directory exists in this environment" for the HDR real-bracket test before one was found. This RFC does not pick one of these — it is called out as the first open question to resolve with the user before any real-photo testing work begins.
- **No new e2e (WebdriverIO) spec for the ML pipeline itself** (same reasoning as HDR/Panorama — a multi-second real-model pipeline isn't suited to a UI-level e2e test), but the People view's own UI affordances (module switch, rename, click-to-filter) do warrant a new `people-view.e2e.js` once the UI exists, per this project's post-2026-08-31 practice of e2e-covering UI-affecting slices — driven against a fixture with pre-seeded `people`/`faces` rows inserted directly (bypassing the real model, same "drive the real Tauri command, mock only what can't be driven" precedent `golden-path.e2e.js` already established for native file pickers).

## 5. Open questions requiring an explicit user decision before implementation

All three resolved 2026-09-08 — see [`models/FACE_MODELS.md`](../../models/FACE_MODELS.md) for the full research and reasoning behind #1 and #3.

1. **~~Exact detection + embedding model files, source, and license~~ (§3.3)** — RESOLVED: **YuNet** (detection, MIT, `opencv/opencv_zoo`) + **SFace** (embedding, Apache 2.0, same repo). SFace's training data (MS1MV2/MS-Celeb-1M) carries a known consent controversy shared by nearly the entire pre-2020 face-embedding model family, and its Apache-2.0 grant traces to the original author's own contribution rather than an independent third-party statement — both accepted as named, documented tradeoffs rather than silently ignored. InsightFace's `buffalo_l`/`antelopev2` family was ruled out: pretrained weights are "non-commercial research only," incompatible with this repo's own MIT license.
2. **~~Real-photo test data for end-to-end clustering verification~~ (§4)** — RESOLVED: option (a) — the user's own photos, env-var-gated to a local, never-committed directory (same pattern as `EMULSION_TEST_PANORAMA_DIR`). No committed real-photo fixture for this feature, ever.
3. **~~Whether model weights are committed to the repo or fetched-once-and-cached locally~~ (§3.3)** — RESOLVED: fetched once on first use into the app's data directory, cached there, **not** committed to the repo. SFace alone is ~37MB (this RFC originally assumed 1-5MB each) — nearly half this repo's current total `.git` size for one binary asset, and there's no Git LFS configured — so the RFC's own "unless license terms specifically prohibit redistribution" default lean toward committing didn't survive contact with the real file size.

## 6. ADR implications once this ships

No existing ADR governs "which ML inference runtime/models this project uses" — ADR-0003 (raw decoding) and ADR-0004 (rendering/color) are both adjacent but don't cover this decision space. Recommend a **new** `ADR-0007-face-detection-and-recognition.md` once this slice ships (not a dated update bolted onto an unrelated existing ADR), recording the `tract` decision and its Windows-linking-history rationale (§3.2) for future reference — this is a genuinely new architecture axis for the project, not an extension of one already on record.
