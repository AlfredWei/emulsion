# RFC-0029: AI super-resolution as an export-time enlargement (M6 slice 4)

- Status: Draft for review
- Date: 2026-10-12
- Relates to: [RFC-0023](RFC-0023-on-device-ai-inference.md) (§4.7, §5.4: the only numbers and the licence caveat), [RFC-0027](RFC-0027-ai-denoise.md) (the tiled `emulsion-ai` job, the crop preview, the model manager and consent flow this reuses), [RFC-0026](RFC-0026-click-select-segmentation.md) (the helper protocol), [ADR-0009](../adr/ADR-0009-ml-inference-runtime.md) (`ort`, Proposed), [ADR-0004](../adr/ADR-0004-rendering-and-color-management.md) (Develop's pipeline)

## 0. Process note

Design only. **No product code is changed and nothing is measured here.** Every number is RFC-0023's (Apple M1 Pro, ORT, Real-ESRGAN `realesr-general-x4v3`, one input size on CPU and CoreML, eight bicubic-downscaled photos for quality). Nothing is known about memory at tile size, other models, Windows, or real low-resolution or soft files. Per the project's RFC-before practice for the AI slices, this is written before slice 4 is built; the user picks the open questions in §7. Models named in §4 as candidates are **named, not tested**, and their licences are **not yet read**.

## 1. Problem

M6 promises "AI super-resolution / upscaling". The practical reasons a photographer wants it: a crop that is too small for the print or screen size they need; an old or low-resolution file; a phone shot enlarged for a poster. A learned upscaler is crisper than bicubic (RFC-0023 §4.7, by eye) but:

1. It **invents detail.** Balcony railings turn into hard black stripes, conifer needles painterly, thin wires heavier; its PSNR was *below* bicubic on 8 of 8 photos. For a photo editor this changes what the photograph shows, so it needs an explicit framing, a before/after, and a way to dial it back.
2. It **changes the pixel dimensions.** Denoise (RFC-0027) leaves the image the same size, so it could be a cached pre-stage and an edit-stack op. A 4x enlargement cannot: crop, spot removal, lens correction, perspective, masks and the histogram all assume one pixel grid.
3. It is **expensive in output, not just input.** 2.2 s per 512x512 input tile (CPU, RFC-0023) extrapolates to about 3 minutes for a 24 MP frame at 4x, and the result is 96 MP: about 290 MB as 8-bit RGB, and several times that as the float accumulators the denoise tiler uses.

## 2. Facts this design rests on

- Real-ESRGAN `realesr-general-x4v3`: 4.9 MB, 4x only, PReLU + DepthToSpace, BSD-3-Clause for the code; **the upstream README states no separate licence for the weights.** ONNX re-host `Heliosoph/realesrgan-onnx` (a third party).
- Timing (RFC-0023): 256²→1024² 0.55 s CPU / 0.37 s CoreML; 512²→2048² 2.21 s CPU (CoreML not run). Peak memory unmeasured.
- Quality on bicubic-4x-downscaled references (§4.7): PSNR 24.8 dB vs 26.1 dB bicubic, SSIM 0.791 vs 0.788; clearly crisper by eye, over-sharpened. "Bicubic x4 of a clean photo" is an idealised degradation; real soft or low-resolution files were not tested.
- The app already has: the `emulsion-ai` helper with a stdin reader thread, tile loop, `progress` and `cancel` (RFC-0027 §9), a pinned-hash model manager with consent / download / import / resume, a crop preview that writes before / after pictures into `previews/`, and an Export dialog with a pre-flight list (RFC-0027 §3.6).
- Edits are resolution-independent: masks, crop, vignette and the rest are stored in normalised coordinates, so the same stack renders at any output size.

## 3. Decisions

### 3.1 It is an export option, not an edit-stack op

An enlargement is chosen **when a picture leaves the app** (Export, and later Print), not stored as an op on the photo. The photo keeps its one pixel grid; Develop, the histogram, every mask and the cached denoise stay as they are. At export the sequence is:

`render the stack at native resolution (denoise blend included) -> enlarge in the helper -> encode in the chosen format`

The enlargement runs on the **graded, cropped output**, not on the decode: the model was trained on display-referred pictures, and the crop is what the user wants bigger, so a 6 MP crop of a 24 MP frame costs a quarter of the full frame. Because it follows grading, sharpening and grain the user applied are enlarged with the picture; the panel says so (Sharpen after enlargement is a refinement, §4).

Rejected: an `ai_upscale` op in the stack (breaks the single pixel grid, forces every consumer to know two sizes, and would need a multi-hundred-megapixel cache per photo); a Develop-time enlargement (nothing to see at screen size, since Develop shows a downscaled preview anyway).

### 3.2 What the user chooses

In the Export dialog, an **Enlarge (AI)** section, off by default: a target (**2x**, **4x**, or *long edge N px* when the export already has a size setting), and a **Strength** (0-100) that blends the model result with a plain Lanczos enlargement of the same size, `out = lanczos + (model - lanczos) * strength / 100`. Strength exists because the model hallucinates (§1.1): at 100 it is the model, at 0 it is a normal resize, and the default is chosen by eye in slice 4c (not guessed here). The dialog shows the output size in pixels and megapixels and **an estimated time before anything runs**; the estimate is measured per machine after the first run, not hard-coded.

The model is 4x only. A 2x or an arbitrary target is the 4x result **resampled down with Lanczos** (more compute than the target needs, but one model and no second code path); whether a native 2x model is worth shipping is a 4a question.

### 3.3 The job: the helper, tiled, streamed, cancellable

- **A new helper op** `upscale_run { model, image_path, out_path, scale, strength?, region? }`, next to `denoise_run`, on the same reader thread: `progress {done, total}` between tiles, a `cancel` naming the job stops it and leaves no file, anything else waits its turn (so Select Subject waits during a job, as with denoise).
- **Streamed output.** The denoise tiler keeps a float accumulator for the whole image (about 0.4 GB at 24 MP); at 4x that would be over 1 GB for a 24 MP source. The upscale tiler therefore works in **bands of tile rows**: it blends the overlap with the previous row, then writes the finished rows to the encoder and frees them. Peak memory is a few rows of tiles plus the model session, independent of the output size. This is the one genuinely new piece of engine work.
- **Tiling and seams.** Input tiles of 256 or 512 px with an overlap of 16-32 px, linear-feather blended on the output grid (the same weights as `ai/tiling.rs`, scaled by 4). Verified the same way as RFC-0027: an identity-style stub model (nearest-neighbour 4x as a hand-written ONNX) must come out equal to a plain nearest enlargement for awkward sizes, so a seam error is the tiler's, not the model's.
- **Output.** The helper writes a lossless intermediate (PNG or 16-bit TIFF for very large results) and the export step encodes the final JPEG / TIFF / PNG with metadata, so JPEG quality settings and metadata copying stay in one place (Rust `export.rs`).

### 3.4 Judging it before committing: the crop preview

As in RFC-0027, an explicit **Preview** shows a before / after of about one megapixel at the view centre at 1:1: the plain enlargement against the model's, at the chosen Strength, with the same panel-in-`previews/` file handling. Only after judging it does the user run the export. This is where the hallucination risk is made visible, and why Strength is a live slider on an already-computed pair (the blend is the cheap part).

### 3.5 Pipeline order with denoise

`decode -> ai_denoise blend -> ... -> graded crop -> enlarge`. A noisy small file is denoised first (the cached pre-stage), graded, then enlarged; enlarging first would enlarge the noise and make the model draw it as texture. Export's pre-flight already lists photos that need denoising first (RFC-0027 §3.6); the enlargement adds its estimated time to the same list.

### 3.6 Honesty in the file

The exported file's metadata records that it was enlarged by a model (Software / a history field, the model id and Strength), and the user guide says that an enlarged picture contains detail the camera did not capture. This is not a watermark; it keeps a result from being mistaken for the original capture.

### 3.7 Models: the registry already exists

`ai_models` (RFC-0027 §3.5) gains the chosen upscaler's file(s) with a pinned SHA-256, the same consent dialog family, and the manual import path. Whether the model is bundled in the release installer (as denoise and Select Subject are, "Release builds bundle the AI models") or downloaded on first use depends on its size and licence (§4); at under 10 MB the compact model is a bundling candidate, a 60+ MB model is not.

### 3.8 Not in this RFC

A live preview of the enlargement in Develop; enlarging in Print or as a separate "Enhance" derived photo / virtual copy (open question 1 asks); face-specific restoration (GFPGAN-class); video; generative fill (M7); GPU execution providers beyond what a measured win justifies (CoreML was 1.5x faster for this model on macOS, DirectML is unmeasured on Windows).

## 4. What is deliberately not decided by a guess

- **Whether Real-ESRGAN `general-x4v3` is the model.** It is the only one measured, it is tiny, and its licence is unresolved (weights' terms not stated upstream; the ONNX is a third-party re-host). Slice 4a spends a bounded effort on the question before any product code: read the licences (the repo's LICENSE, the release notes the weights ship in, the re-host card) and check candidates *named, not tested*: the larger Real-ESRGAN `x4plus` (about 64 MB, perceptually stronger), **a fidelity-oriented transformer model such as SwinIR (Apache-2.0 upstream; heavy)**, and any compact CNN with a clear licence. **A fidelity-oriented model invents less**, which may suit a photographer better than the crisp-but-painterly GAN look; the bake-off must include one.
- **Memory** at 256 / 512 tiles (unmeasured; RFC-0027 found SCUNet at 3.8 GB only by measuring).
- **Quality on real inputs.** The only test degradation was bicubic-4x of a clean photo. Real soft files, JPEG-compressed phone images and cropped sensor noise are untested; 4a measures on them.
- **Whether strength-blending with Lanczos hides the artefacts** or merely softens everything (RFC-0023 §5.3 flagged it as worth testing, not as a result).

## 5. Slices

| Slice | Content | Verification |
|---|---|---|
| **4a** model choice (research, no product code) | Time-boxed bake-off: `general-x4v3`, `x4plus`, one fidelity model, on the RFC-0023 §4.7 set plus real soft / compressed files. Per model: time per MP, peak RSS at 256 and 512 tiles, PSNR / SSIM against references, by-eye crops, hallucination examples, Strength-blend crops. **Read each weights licence.** Downloads are stated (filename, source, size) and confirmed first. | Numbers here and in RFC-0023 §4.8; a decision recorded. Proposed go criterion: licence clear to ship, at most about 1 min per 6 MP of input on an M1 Pro, peak RSS under about 1.5 GB. |
| **4b** engine | `upscale_run` helper op, band-streamed tiler, protocol (progress / cancel), model in the registry, intermediate encoder, export step wiring in Rust | Rust hand-value tests; tiler seam test with a stub nearest-4x ONNX through the real helper (CI, no weights), including cancel leaves no file; an `#[ignore]`d real-model run; peak RSS recorded at 24 MP input |
| **4c** UI | Export dialog *Enlarge (AI)*: target, Strength, size and time estimate, crop Preview with before / after, model consent / download, progress and Cancel in the export queue | vitest with mocked IPC; a CI-safe e2e for the model-absent path; a real-model run locally |
| **4d** polish | Metadata marking (§3.6), batch estimates in the pre-flight, Print integration decision, docs, backlog | per the rule, "not verified" goes to the PROGRESS backlog |

## 6. Risks

1. **Invented detail** (§1.1). Mitigation: explicit "Enlarge (AI)" framing, Preview before / after, Strength, metadata mark, a fidelity model considered in 4a.
2. **Time and size.** 96 MP out of a full 24 MP frame is minutes and a very large file. Mitigation: the estimate before running, enlarging the crop (not the frame) being the typical case, Cancel, one job at a time.
3. **Licence.** If no model with clear weights terms passes 4a, the feature does not ship in M6 and this RFC records why; nothing else depends on it.
4. **Memory at tile size** is unknown; the streaming design keeps it independent of output size, but the model's own working set is only known after 4a.
5. **Export complexity.** The export step gains a helper round trip and a lossless intermediate; failure and cancel paths must not leave partial output files (the same rule as denoise's cache entries).

## 7. Open questions for the user

1. **Scope: export-time only, or also an "Enhance" that makes a new derived photo / virtual copy** in the catalog (so the enlarged picture can be edited, rated, and printed)? Recommendation: export-time only for 4b-4c; a derived photo is a larger catalog decision (a version with different dimensions) and can follow if wanted.
2. **Run the bake-off (4a) first, or build on `general-x4v3` now?** Recommendation: bake-off first, time-boxed to one slice, because the licence question can stop the feature and a fidelity model may be the better product.
3. **Targets:** 2x and 4x only, or also "long edge N px"? Recommendation: 2x / 4x / long edge, all resampled from the model's 4x.
4. **Default Strength** and whether the dialog remembers it between exports. Recommendation: decide by eye in 4c; remember per-session only.
5. **The metadata mark** (§3.6): always written, or a checkbox? Recommendation: always written (provenance honesty), no opt-out in the first cut.
