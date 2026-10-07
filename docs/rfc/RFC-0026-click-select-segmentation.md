# RFC-0026: Click-to-select (SAM 2 Tiny) as a mask kind (M6 slice 2)

- Status: Draft for review
- Date: 2026-10-06
- Relates to: [RFC-0023](RFC-0023-on-device-ai-inference.md) (models and numbers), [ADR-0009](../adr/ADR-0009-ml-inference-runtime.md) (`ort`, Proposed), [RFC-0025](RFC-0025-composable-masking.md) (the mask model this plugs into), [ADR-0007](../adr/ADR-0007-face-detection-and-recognition.md) (model fetch-and-cache pattern), [PRD MILESTONES §M6](../../PRD/MILESTONES.md)

## 0. Process note

Design only. **No product code is changed and nothing is measured here**: every number below is RFC-0023's (Apple M1 Pro, plus one pair of 4-vCPU CI runners), and RFC-0023 §4.2's accuracy check was six clicks on four photos. The user chose this slice on 2026-10-06 after the composable-masking milestone-end verification (#210). Per ADR-0009 the runtime is still *Proposed*; this RFC is the first consumer and its slice 2a is what turns that into a working dependency.

## 1. Problem

M6's first bullet is "Select Subject … as one-click AI-generated masks feeding the existing mask/adjustment system". RFC-0023 §5 settled the model side: **SAM 2 Hiera-Tiny** as a *general click-to-select object tool* (not a sky detector; *Select Sky* is the existing range mask), with the UI offering the three candidates because auto-picking the top-IoU one chose the wrong level in 2 of 6 tries. What is undecided is everything around the model: where it runs, what a "segment mask" *is* in the edit stack, how it renders on the CPU and GPU twins, how the user drives it, how the weights get onto the machine, and how the work is sliced.

## 2. Facts this design rests on

- Encoder: one pass per image at 1024², ≈ 1.0 s on an M1 Pro (CPU provider), 2.8 s on a shared 4-vCPU Windows VM, 5.5 s on a shared macOS VM. Decoder: ≈ 20 ms per prompt on the M1 Pro, 60–85 ms on the VMs. (RFC-0023 §4, §4.4.)
- **CoreML is pathological for the SAM 2 encoder** (166 s vs 5.5 s on a second Mac). DirectML is unmeasured. So the provider is **CPU, always, for this model**; no provider selection logic is needed in 2a.
- `tract` cannot load either SAM 2 file, so this slice is what first puts `ort` (`=2.0.0-rc.13`) in the app crate. It already links there on both platforms (RFC-0023 §4.4 point 5, throwaway PR #199); no model was ever run inside the app.
- Encoder outputs per image (from the reference script, `acc_sam2.py`): `image_embed` 1×256×64×64, `high_res_feats_0` 1×32×256×256, `high_res_feats_1` 1×64×128×128, all f32: **≈ 16 MB**. Decoder returns 3 candidate masks of 256×256 logits plus 3 predicted-IoU scores per prompt set; it accepts several point prompts (label 1 positive, 0 negative) and the previous low-res mask as `mask_input` for refinement. The reference feeds the whole image **squashed** to 1024×1024 (not letterboxed); the mask then maps back by plain scaling.
- Weights: 134 MB encoder + 21 MB decoder, a third-party ONNX re-host (`vietanhdev/segment-anything-2-onnx-models`) of Meta's checkpoints; upstream README states Apache-2.0 for code *and* checkpoints; the export is flagged experimental. Hashes are in the RFC-0023 appendix `SHA256SUMS`.
- Mask coordinates in this app are **full-image normalized** (pre-crop; crop is applied after masks, and mask handles are hidden under a committed crop). So segmentation must run on the **uncropped** source preview, which is also what `ensure_develop_preview_for_hash` already decodes (≤ 2048 px long edge).

## 3. Decisions

### 3.1 Runtime: a long-lived `emulsion-ai` helper process (revised 2026-10-06)

*Original text (in-process `ort`, idle-drop of the sessions) was replaced after slice 2a measured ~1.1 GB resident during encode that the process does not give back when the sessions are dropped (§7). The user chose a separate process, long-lived, extensible to later AI features.*

**Split.** The app process never links or loads ONNX Runtime. A second binary of the same package, **`emulsion-ai`** (`src/bin/emulsion-ai/`), owns every `ort` session and cached embedding. The app spawns it **lazily on the first AI request** (never at launch), keeps its stdin/stdout pipes open, and talks **JSON lines** (`src/ai/protocol.rs`: `hello`, `segment_prepare`, `segment_decode`, `release`, `shutdown`; stable error codes `not_prepared`, `inference`, `image`, `bad_request`, `internal`). The helper serves requests **strictly one at a time, in order**, so SAM 2 today and denoise / super-resolution later share one queue and one memory high-water mark (the larger model, not the sum, provided they do not overlap). New AI features are new `op`s, not new processes.

**Lifetime.** Bounded above by the app: the helper exits when its stdin closes (the app quit or died). It also **exits by itself after an idle period (default 15 min, `EMULSION_AI_IDLE_SECS`, 0 = never)**: that is what returns the ~1 GB to the OS, and the next request respawns it (cost: process start + session load + encoder, the 1.3-2 s first-use figure). Keeping it alive for the whole app session is a setting value, not a design change. The supervisor (`src/ai/supervisor.rs`) retries a request **once** on a fresh process when it finds the helper dead (every request is idempotent), kills and fails a request that times out, reaps exited helpers, and never spawns just to `release`.

**Data crossing the pipe is small by design:** the helper reads the preview image from its path itself and keeps the ≈ 16 MB embedding; a click sends a few bytes and returns three ≈ 47 KB PNGs. The embedding is lost when the helper exits, so after an idle exit `segment_decode` answers `not_prepared` and the UI prepares again (the cache is per helper life, not persisted, as before).

The encoder still runs when the user **arms the Select Subject tool**, not on Develop open. Embeddings: **current image only**, in memory, never on disk.

Tauri commands: `segment_prepare(path, content_hash)`, `segment_decode(content_hash, prompts, refine?)`, `segment_release`, plus the model-file commands of §3.5 and `ai_helper_info` (diagnostics).

### 3.2 What is stored: a raster of quantised logits, plus the prompts

**Rejected: store prompts only and re-run the model on demand.** The weights may be absent (exports, a second machine, an offline restore), ORT/model-version drift would silently change old edits, and export would pay seconds of inference. An edit must render from what is in the catalog.

**Chosen: a new weight-producing mask kind `segment_mask`** (alongside linear, radial, brush, luminance range, colour range), carrying:

```json
{ "op": "segment_mask", "id": "…",
  "logits": "<base64 PNG, 8-bit gray, 256×256>",   // logit in [-4, 4], 16 levels, scaled onto 0..255
  "prompts": [{"x": 0.31, "y": 0.62, "label": 1}], // kept so the mask can be re-opened and refined
  "candidate": 1,                                   // which of the 3 was accepted
  "feather": 0, "grow": 0, "invert": false,
  "exposure": 0, "contrast": 0, "saturation": 0,
  "modifiers": [ … ] }                              // may carry RFC-0025 modifiers
```

The raster is the **decoder's 256×256 logits, quantised, not a thresholded binary mask**. At render time both twins bilinearly sample the *logit* and compute `weight = sigmoid(k · (logit − grow))`, with `k` from `feather` (small feather = steep = crisp edge). Rationale: (a) a 256² mask stretched over a 24 MP frame is blocky if thresholded first; interpolating logits then thresholding gives a smooth, resolution-independent edge; (b) *Feather* and *Grow/Shrink* become cheap, **non-destructive** sliders with identical CPU and GPU maths; (c) one raster serves any export size. Honest limit: edge *position* is still only as good as a 256² logit field (about 1/256 of the frame width per texel, so ≈ 23 px per texel on a 6000-px frame); RFC-0023 §4.6 found image-guided edge refinement does not help for sky, and it has not been tried here. Edges are therefore soft by construction; a good fit for local adjustments, not for compositing.

**Size.** A smooth 8-bit PNG of this kind is expected to be a few KB to ~20 KB (not measured). That sits inside the catalog and every undo/snapshot copy as ordinary JSON text (the stack is `Vec<serde_json::Value>`, opaque to the catalog). **Budget to verify in 2b:** ≤ 30 KB base64 per mask; if exceeded, crop the PNG to the mask's bounding box, then reconsider a side table keyed by mask id. Presets, Copy Settings and Paste already exclude every mask kind (`PRESET_EXCLUDED_OP_NAMES`), so rasters never travel between images.

### 3.3 Rendering: a raster-class mask next to brush, on both twins

- **CPU (Rust):** decode PNG once at `parse_masks`, bilinear sample at `uv`, apply the sigmoid; then the usual adjustment, and the RFC-0025 `fold_modifiers` for any modifiers.
- **GPU (WGSL):** a new `kind` in `component_weight`. The raster rides the **existing brush texture-array layers** (the layer is simply written from the decoded PNG instead of rasterized dabs), so it **shares the 8-layer cap** (RFC-0025's `MAX_MASKS` / brush budget) rather than adding a second texture array; the shader reads the layer and applies the same sigmoid. That keeps the binding count and the per-frame cost flat. A segment mask may be a **base mask or an RFC-0025 modifier shape** (so "subject minus the face" = segment base + a subtract segment/brush shape); modifier layers are keyed by modifier id exactly as brush modifiers are.
- **Parity** is easier than brush: both twins read the *same* raster, so the existing CPU/GPU dump-and-compare machinery applies (a real-GPU probe plus an e2e scenario; the brush's CPU-formula vs Canvas2D difference does not arise). Differences expected only from texture filtering vs CPU bilinear on 8-bit values.

### 3.4 UX

- A **Select Subject** tool in the tool strip, armed like Brush. First arm on an image shows "Preparing…" while the encoder runs (and the consent/download step, §3.5, on first ever use).
- **Click** = positive point; **Alt/Option-click** = negative point; each prompt re-runs only the decoder (≈ 20–80 ms), feeding the previous low-res mask as `mask_input`. The candidate is previewed with the same red overlay selected masks use.
- **The three candidates are exposed**, not auto-picked: a small switcher (1 / 2 / 3, also `Tab`) ordered by predicted IoU with the top one preselected, plus the refinement clicks. This is the product answer to RFC-0023 §4.2's part-versus-whole ambiguity.
- **Accept** (Enter / button) creates a `segment_mask` and selects it, which opens the existing mask panel (adjustments, Feather, Grow/Shrink, Invert, Shapes). Inside the panel, **Add shape ▸ Segment** (modes add/subtract/intersect) reuses the same tool to add a segment as an RFC-0025 modifier. *Esc* abandons the preview. Re-opening a segment mask can resume refinement from its stored `prompts`/`candidate` (the embedding is recomputed lazily if no longer cached).
- Honest copy in the UI/User Guide: sky selection is better done with *Luminance/Colour Range* (RFC-0023 §4.2a: clouds are left out, thin branches are lost); this tool is for objects.

### 3.5 Getting the weights onto the machine (decision needed from the user)

155 MB is too large to bundle into every installer for an optional tool. Proposed, following ADR-0007: **fetched on first use** into the app cache directory, **SHA-256 pinned** (the appendix hashes), never committed, never re-fetched once valid. Because this is **network use by an otherwise-offline app**, it needs the same kind of explicit, scoped exception M5.5 wrote down for geocoding: a dialog on first arm — "Select Subject needs a one-time 155 MB model download from <host>. Nothing about your photos is sent." — with *Download* / *Cancel* / *I have the files* (choose the two files manually; they are verified against the same hashes). Offline machines therefore have a path. The pinned URL and the **re-host's own licence tag** (RFC-0023 only read the upstream README) must be re-read and recorded before 2a ships; if the re-host's terms are unclear, fall back to exporting from Meta's checkpoint ourselves.

### 3.6 Failure behaviour

No model, download refused, or session creation fails → the tool stays visible but explains why and offers the manual-files path; **no existing edit is affected** (a stored `segment_mask` renders from its raster with no model, §3.2). Out-of-memory or an inference error aborts the click with a message, never the edit session. WebGPU unavailable → mask creation is already disabled (existing rule); existing segment masks still render on the CPU fallback.

## 4. Slices

| Slice | Content | Verification |
|---|---|---|
| **2a** runtime | `ort =2.0.0-rc.13`, linked only into the new `emulsion-ai` helper binary; protocol + supervisor; model fetch/verify/consent plumbing (commands only, no UI); `segment_prepare`/`segment_decode`; embedding cache; idle drop | CI: builds and unit tests on both OSes (hash verify, cache logic, point/mask coordinate maths with a stub session). **Local, `#[ignore]`d with an env var**: the six RFC-0023 clicks through the Rust path reproduce the Python candidates' masks (IoU ≥ 0.99 per candidate). Encoder/decoder timings re-measured **inside the app process**. |
| **2b** engine | `segment_mask` kind: parse, Rust weight, WGSL kind, raster layer upload, as base and as modifier, JSON round-trip, size budget | Rust hand-value tests; CPU/GPU dump + real-GPU probe (as RFC-0025's); an e2e scenario. Catalog size measured. |
| **2c** UI | Select Subject tool, click/Alt-click, candidate switcher, accept, consent dialog, panel Feather/Grow, Add shape ▸ Segment | browser harness on a real GPU with mocked IPC (a stub decoder), e2e for the flow with the model absent (the CI-safe path); **a real-model run locally**; USER_GUIDE |
| **2d** polish | idle-drop tuning, history labels, keyboard path, docs; the milestone-end backlog | per the user's rule, "not verified" items go to the PROGRESS backlog |

## 5. Risks and open questions

1. **The ONNX export is flagged experimental** and is a third-party re-host. Mitigation: pin by hash; the reference comparison in 2a detects drift; the manual-files path means a bad re-host is not a dead end.
2. **First-click latency** on a modest laptop (CI VMs suggest 3–6 s for the encoder; real hardware is unmeasured). Mitigation: visible "Preparing…" state; revisit prefetching if it feels bad.
3. **Memory:** ≈ 155 MB of weights in sessions plus ≈ 16 MB embedding, on top of Develop's own. Mitigation: idle drop. Peak RSS is **not measured**; 2a must measure it.
4. **Logit-raster edges** are soft and low-resolution (§3.2). If users need crisp edges, options are a higher-resolution decoder pass on a crop or a guided-filter refinement against the full-resolution image; neither is designed here.
5. **Stack/catalog size** per mask (§3.2 budget) and its effect on undo snapshots: measured in 2b, not assumed.
6. **Layer sharing:** segment masks consume the same 8 raster layers as brush masks and brush shapes; a user mixing many brush strokes and segments hits the cap sooner. The cap messages already name the limit; raising it is a separate memory question.
7. **Windows DirectML** is irrelevant here (CPU only) but ADR-0009 stays Proposed until a real Windows GPU run exists; this RFC does not close it.
8. **Hardware floor:** M6's "target machine spec" is still undefined. Slice 2a should record what the in-app timings imply.
9. **Whether the first click may prefetch after the tool has been used once** (and a user setting to disable it) — deferred.

## 6. Non-goals

Sky detection (the range masks stay *Select Sky*); automatic whole-image "select all subjects"; box prompts; video; multi-image consistency; generative fill (M7); any provider other than CPU; persisting embeddings; a trained or fine-tuned model.

## 7. Corrected during implementation (slice 2a, 2026-10-06)

- **Licence of the re-host (closes §3.5's condition):** the model card of `vietanhdev/segment-anything-2-onnx-models` declares **Apache-2.0**, with the weights credited to Meta's `facebookresearch/sam2`. The pinned commit is `071f580...`; its LFS object ids equal the SHA-256s recorded in RFC-0023's appendix (encoder `4cc015ee...`, decoder `f5a4bd65...`); a `HEAD` on both pinned URLs resolves (302 -> 200) with the expected sizes. The 155 MB download itself was **not** re-run: the live-fetch test is `#[ignore]`d.
- **As built:** `segment_models.rs` (status / streaming download with hashing / manual import, by name + checksum), `segment.rs` (`Segmenter`: lazy sessions, one cached embedding, pure pre/post-processing), `segment_commands.rs` (`segment_model_status`, `segment_download_models` with `segment-model-progress` events, `segment_import_model_files`, `segment_prepare`, `segment_decode`, `segment_release`). Encoder output order is `high_res_feats_0, high_res_feats_1, image_embed`; decoder `masks, iou_predictions` (confirmed from the model files). `ort =2.0.0-rc.13`; `base64` added. Candidates come back **best predicted IoU first**, each as the quantised-logit PNG of §3.2 (so the same bytes can be stored or fed back as the refinement `mask_input`).
- **Measured, in the app's own Rust path** (Apple M1 Pro, release build, `test_image/` photos at full size; **one machine**): first prepare (session load + resize + encoder) 1.3-1.9 s, decode 23-64 ms per click, cache hit 0 ms. **The six RFC-0023 clicks reproduce Python onnxruntime**: every candidate the model rates >= 0.3 has mask IoU >= 0.968 against it (most >= 0.99); the lowest-rated degenerate candidates >= 0.92; predicted IoUs within 0.05. The residual is the resize filter (the `image` crate's triangle vs PIL bilinear), not the runtime.
- **Memory is worse than §5.3 assumed.** Peak RSS of the process during the six-click run was **1.44 GB** with default session options; `with_memory_pattern(false)` brings it to **1.15 GB** at the same speed (shipped); disabling the CPU arena made it *worse* (2.0 GB) and a lower graph-optimisation level too (1.85 GB). Weights are only 155 MB; the rest is encoder activations at 1024². **`Segmenter::release()` does not return it:** after dropping both sessions and the embedding the process stays at ~1.1 GB (`footprint`: ~990 MB dirty MALLOC_LARGE), and repeating load/release four times grows it only ~5 MB per round, so it is retention, not an unbounded leak. Consequence: **the idle-drop mitigation of §3.1 does not reduce resident memory on macOS** as built. Not measured: Windows, and whether the retained pages are reclaimed under system memory pressure.
- **Open design question this raises (user's call):** to give the memory back after use, run inference in a **short-lived helper process** (spawned on first use, exits after the idle period), at the price of IPC for ~16 MB embeddings (or keeping them in the helper) and an extra executable to sign. Alternatives: accept ~1.1 GB resident after the first use of the tool, or find an `ort`/allocator setting that releases it (not found in this slice), or evaluate a smaller/quantised encoder.
- **Not done in 2a:** no UI; the download dialog and progress UI (2c); Windows measurements; the helper-process question above. e2e covers only the model-absent path (`segment-model.e2e.js`).

### 7.1 Process split (slice 2a, same PR; supersedes the in-process memory plan)

- **Measured with the real weights through the real helper** (`tests/ai_helper.rs`, ignored test, M1 Pro, release): the **app process stays at 2 MB RSS** before and after, the **helper holds ~984 MB**, and after the idle exit (3 s in the test) the helper is gone, so the memory is returned. First prepare round trip 2.1 s including spawn and session load (1.28 s encoder); decode round trip 23 ms for three candidates (141 KB of PNG, base64).
- **The helper's masks are unchanged**: the six-click parity test against Python onnxruntime moved with the code (`cargo test --release --bin emulsion-ai -- --ignored segment_matches`), worst rated-candidate mask IoU 0.968 as before.
- **Tests that run everywhere (CI included)** need no weights: protocol round-trips; `tests/ai_helper.rs` spawns the real helper for hello, error codes, no-spawn-on-release, idle exit then respawn, kill -9 then retry, spawn failure, exit on pipe close.
- **Build/packaging facts:** `cargo build` and `tauri build --debug --no-bundle` both emit `emulsion-ai` next to `emulsion` (checked locally, and `ai_helper_info` is asserted in e2e); the helper links ONNX Runtime **statically** (no dylib beside it on macOS; the app binary has no ORT symbols). **Not done:** bundling the helper into an installer (`bundle.externalBin`), its code signing/notarization, and Windows behaviour (`CREATE_NO_WINDOW`, process-kill paths are written but unexercised; CI runs the integration tests on `windows-latest`).
- **Costs accepted** (as listed to the user): a second executable to ship and sign, crash/hang handling, a cold restart after an idle exit.

## 8. Corrected during implementation (slice 2b, 2026-10-07)

The engine for `segment_mask` is in (CPU, WGSL, layer upload, base and modifier). Decisions that changed §3.2, each from a measurement:

- **Coding: ±4 logits, 16 mid-rise levels (not 8-bit over ±8, not a sigmoid).** Real SAM logit fields coded as 8-bit over ±8 were **26-64 KB of base64 per mask**, over the 30 KB budget in most cases. A narrower range plus coarser levels, measured on the six RFC-0023 clicks: **0.6-16 KB**. The quantiser is **mid-rise** (centres at ±0.25 ... ±3.75, no level at 0) so **the sign of every logit survives coding exactly**: selection IoU before/after coding is 1.0000 on all 18 candidates, where an odd-level (zero-centred) coding changed the selection by up to 3.6%. The PNG is coded `Best` compression with the `Sub` filter.
- **Weight = a linear ramp, not a sigmoid**: `w = clamp(0.5 + (logit + grow*0.03) / (2h), 0, 1)`, `h = 0.25 + 3.45*feather/100`. With the ±4 range a sigmoid at low steepness could not reach 0 or 1 inside the stored range; the ramp reaches exactly 0 and 1 (`h` tops out below the outermost stored logit 3.75), is cheaper (no `exp`) and is identical arithmetic on both twins. `grow` -100..100 shifts the threshold by up to ±3 logits.
- **Rendering:** GPU kind code **7** (after spot 5 and red-eye 6, so no existing code moved); the shader's weight-producing set is now `kind < 4.5 || kind > 6.5`. The 256² field is decoded asynchronously (`createImageBitmap`, no colour management), drawn once with the canvas's bilinear filter into the mask's raster layer, and `fs_mask` turns the sampled byte back into a logit; Feather/Grow only change uniforms, never the layer. It **shares the 8 raster layers** with brush and spot masks and brush shapes; a decoded field is cached (16 entries). The CPU twin interpolates the 256² logits itself (`segment_mask_weight` in `masks.rs`); a damaged or wrong-sized field is skipped like any unparseable op.
- **Verified:** 7 Rust hand-value tests (saturation, feather/grow, bilinear crossings, damaged fields, modifier and base-with-modifier folds, pixel path, JSON round trip); vitest (packing, model, layer budget, a guard that the WGSL constants equal the Rust ones); **CPU/GPU parity on a real GPU in the real window** (`develop-mask-shapes.e2e.js`, passed in two consecutive full-spec runs: a segment base with a subtract segment shape at a mid-weight patch: CPU 175/169/164 vs GPU 179/169/165).
- **A real bug the parity test found (not in the shader):** the first GPU readings were stale (CPU 175 vs GPU 148 = weight about 0.04, the black placeholder layer). Cause: the field decodes *after* the first render, the follow-up render is requested from the decode callback, and if a histogram read-back is still in flight that render **skips refreshing the histogram texture**, which the hover readout (and the histogram) read, and no later render makes up for it. The canvas itself was right. Fix: the canvas's `requestRender` waits for an in-flight read-back before rendering. This latent limitation pre-dates this slice; the new asynchronous render exposed it.
- **UI left for 2c:** segment masks have explicit names (title, chip, Shapes row) so they never fall into the "unrecognised = linear gradient" default, but no control creates one and the panel shows no Grow slider yet; "Add shape" does not offer it.
- **Still open:** the e2e scenario ran only on Apple/Metal locally (CI runners have no WebGPU); a segment mask at 8 raster layers and at native-resolution tier is covered by the shared budget logic, not exercised; the CPU path was timed only inside the debug-build e2e (no real export timing).

## 9. Corrected during implementation (slice 2c, 2026-10-07)

- **The mask is the preview.** §3.4 pictured a preview that Enter turns into a mask. As built, the **first decoded click creates the `segment_mask` (or the segment shape) in the edit stack and later clicks / candidate changes edit it in place**, so the real overlay, panel and renderer are the preview and nothing new is needed on the GPU side. **Enter / Done** keeps it and leaves the tool, **Esc / Cancel** removes what that session made (a shape is removed from its mask), toggling the tool off keeps it. Cost: every click that settles (writes are debounced by 250 ms) leaves its own *Select Subject* row in History, and a cancelled session leaves those rows plus a *Delete Mask* (or *Remove Mask Shape*) row. Undo steps through the refinements; this was not tuned.
- **Refinement feeds the chosen candidate back**: each later click sends all prompts plus the currently chosen candidate's stored `logits_png` as `refine` (the decoder's `mask_input`); the result is again sorted best-first and the top candidate preselected.
- **Prompts are stored as `{x, y, positive}`** (same as the helper protocol), not the `label` of §3.2's sketch.
- **Add shape ▸ Subject** arms the same tool with `shapeTarget` set, so the first click becomes a modifier of the selected mask (`combine` chosen in the menu); further clicks refine that shape.
- **Model dialog** (§3.5): consent text with the host and size from `segment_model_status`, *Download*, *I have the files* (native file picker, backend checks name + checksum), progress, failure with *Try again*. **A running download cannot be cancelled** (no abort handle yet): the dialog says so and hides Cancel meanwhile.
- **Keys** (Develop, tool armed, no modifier): Enter keeps, Esc discards, Tab cycles candidates. **Alt/Option-click** is the negative point.
- **Helper lifetime in the UI**: the embedding is released when the open image changes; leaving Develop relies on the helper's idle exit (§3.1).
- **Verified / not verified**: see PROGRESS (slice 2c; slice 2d adds the first real-model session, §10). In short, the flow was driven in the real component tree on a real GPU with a stub decoder and in the real window without the model, but **never with the real model through the UI**.

## 10. Corrected during implementation (slice 2d, 2026-10-07)

- **First real-model session through the UI**, in the real window (Apple M1 Pro, debug build, one 1779x2848 portrait): arm -> ready 3.1 s cold (helper spawn + session load + encoder); a click -> persisted mask in about 0.5 s; stored field 5.4 KB base64; Tab, Alt-click, Enter and resume all work. The numbers are one machine, one photo, a debug build; the release build's decode is 23-64 ms (§7).
- **Downloads can be cancelled** (`segment_cancel_download`; the partial file is deleted, a completed file is kept so the next attempt resumes at the missing one). The dialog's Esc and a *Cancel download* button use it.
- **A saved mask can be re-opened for refinement**: *Refine with clicks...* seeds the session from the stored `prompts` and the stored `logits` (used as the decoder's `mask_input`, so no live candidates exist until the next click). Cancel restores the saved selection. This closes §3.4's "resume refinement from its stored prompts/candidate".
- **Helper release**: embeddings are released on image change and on leaving Develop; the helper process itself exits by its idle timer (§3.1).
- **Still open**: Windows; quality across many photos; tuning of History rows per click; a user setting for the helper's idle time.

