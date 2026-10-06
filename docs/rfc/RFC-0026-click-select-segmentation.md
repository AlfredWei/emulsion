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

### 3.1 Runtime: a Rust `ml` module, CPU provider, lazily loaded

A new `ml` module in the Rust core owns the two `ort` sessions. Sessions are created on first use of the tool (not at launch, not on Develop open) on a blocking thread, and dropped after an idle period (proposed 5 minutes; to be tuned in 2a). The encoder runs when the user **arms the Select Subject tool** on an image whose embedding is not cached, not on every Develop open: opening an image must not cost 1–5 s of CPU on a feature most sessions never use. The first click therefore waits up to ≈ 1–3 s on the user's machine; the tool shows a visible "Preparing…" state. (A prefetch-after-first-use heuristic is a later optimisation, not part of this RFC.)

Embeddings are cached **in memory, for the current image only** (≈ 16 MB, keyed by content hash, so switching away and back within the idle window is free). **Not persisted to disk**: 16 MB per image would grow without bound for a catalog of tens of thousands, and re-encoding costs about a second.

Tauri commands, sketch: `segment_prepare(content_hash)` → status/progress events; `segment_decode(content_hash, points, mask_input?)` → three candidate rasters + scores. The frontend never sees the embeddings.

### 3.2 What is stored: a raster of quantised logits, plus the prompts

**Rejected: store prompts only and re-run the model on demand.** The weights may be absent (exports, a second machine, an offline restore), ORT/model-version drift would silently change old edits, and export would pay seconds of inference. An edit must render from what is in the catalog.

**Chosen: a new weight-producing mask kind `segment_mask`** (alongside linear, radial, brush, luminance range, colour range), carrying:

```json
{ "op": "segment_mask", "id": "…",
  "logits": "<base64 PNG, 8-bit gray, 256×256>",   // logit in [-8, 8] quantised to 0..255
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
| **2a** runtime | `ort =2.0.0-rc.13` in the app crate; `ml` module; model fetch/verify/consent plumbing (commands only, no UI); `segment_prepare`/`segment_decode`; embedding cache; idle drop | CI: builds and unit tests on both OSes (hash verify, cache logic, point/mask coordinate maths with a stub session). **Local, `#[ignore]`d with an env var**: the six RFC-0023 clicks through the Rust path reproduce the Python candidates' masks (IoU ≥ 0.99 per candidate). Encoder/decoder timings re-measured **inside the app process**. |
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
