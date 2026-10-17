# RFC-0029: AI super-resolution: Enlarge (AI) creates a connected photo (M6 slice 4)

- Status: Draft for review (scope decided with the user 2026-10-12; UX designed first as a mock, §3.2)
- Date: 2026-10-12
- Relates to: [RFC-0023](RFC-0023-on-device-ai-inference.md) (§4.7, §5.4: the only numbers and the licence caveat), [RFC-0027](RFC-0027-ai-denoise.md) (the tiled `emulsion-ai` job, the crop preview, the model manager and consent flow this reuses), [RFC-0026](RFC-0026-click-select-segmentation.md) (the helper protocol), [RFC-0003](RFC-0003-hdr-merge.md) / [RFC-0004](RFC-0004-panorama-merge.md) (the "new photo with provenance" precedent), [ADR-0009](../adr/ADR-0009-ml-inference-runtime.md) (`ort`, Proposed), [ADR-0004](../adr/ADR-0004-rendering-and-color-management.md) (Develop's pipeline)
- UX mock: [ai-enlarge-mockup.html](../ux/mockups/ai-enlarge-mockup.html)

## 0. Process note

Design only. **No product code is changed and nothing is measured here.** Every number is RFC-0023's (Apple M1 Pro, ORT, Real-ESRGAN `realesr-general-x4v3`, one input size on CPU and CoreML, eight bicubic-downscaled photos for quality). Nothing is known about memory at tile size, other models, Windows, or real low-resolution or soft files. Models named in §4 as candidates are **named, not tested**, and their licences are **not yet read**.

Decided with the user on 2026-10-12 (answers to the first draft's §7): the result is **a new photo in the catalog** (a virtual copy or a connected image, with a well-designed UX); **model bake-off first**; targets **2x / 4x / long edge**; default Strength **chosen by eye later**; the metadata note is a **check box**. The UX was then designed first as an interactive mock (§3.2), the RFC-0028 way.

## 1. Problem

M6 promises "AI super-resolution / upscaling". The practical reasons a photographer wants it: a crop that is too small for the print or screen size they need; an old or low-resolution file; a phone shot enlarged for a poster. A learned upscaler is crisper than bicubic (RFC-0023 §4.7, by eye) but:

1. It **invents detail.** Balcony railings turn into hard black stripes, conifer needles painterly, thin wires heavier; its PSNR was *below* bicubic on 8 of 8 photos. For a photo editor this changes what the photograph shows, so it needs an explicit framing, a before/after, and a way to dial it back.
2. It **changes the pixel dimensions.** Denoise (RFC-0027) leaves the image the same size, so it could be a cached pre-stage and an edit-stack op. A 4x enlargement cannot: crop, spot removal, lens correction, perspective, masks and the histogram all assume one pixel grid. The result has to be a different picture, with its own size.
3. It is **expensive in output, not just input.** 2.2 s per 512x512 input tile (CPU, RFC-0023) extrapolates to about 3 minutes for a 24 MP frame at 4x, and the result is 96 MP.

## 2. Facts this design rests on

- Real-ESRGAN `realesr-general-x4v3`: 4.9 MB, 4x only, BSD-3-Clause for the code; **the upstream README states no separate licence for the weights.** ONNX re-host `Heliosoph/realesrgan-onnx` (a third party).
- Timing (RFC-0023): 256²→1024² 0.55 s CPU / 0.37 s CoreML; 512²→2048² 2.21 s CPU. Peak memory unmeasured. Quality on bicubic-4x-downscaled references: PSNR 24.8 dB vs 26.1 dB bicubic, by eye crisper but over-sharpened.
- **The catalog opens RAW and JPEG only** (`source_decode.rs`; the `image` crate is built with the `png` and `jpeg` features, and PNG is only written, never imported). So an enlargement that is to become a catalog photo is a **JPEG** in this RFC. JPEG holds at most **65,535 px on a side**.
- **The precedent exists.** HDR and panorama merge write their result **next to the source file** (`<stem>-Panorama`, collision-avoided with `export::unique_output_path`), catalog it as a new `images` row with an empty edit stack through `add_image_with_edit_stack`, and record provenance in `hdr_merge_sources` / `panorama_merge_sources` (never read by a render path). A **virtual copy** (`image_versions.is_virtual_copy`) means several edit stacks over **one file**, so it cannot hold a picture of a different size; `images.stack_id` is unused.
- The app already has: the `emulsion-ai` helper with a stdin reader thread, tile loop, `progress` and `cancel` (RFC-0027 §9), a pinned-hash model manager with consent / download / import / resume and a bundling path for release builds, a crop preview that writes before / after pictures into `previews/`, and an Export dialog with a pre-flight list.
- Edits are resolution-independent (normalised coordinates), so the same stack renders at any size.

## 3. Decisions

### 3.1 The enlargement is a connected photo, not a virtual copy and not an edit-stack op

**Enlarge (AI)** renders the selected photo's stack at native resolution (the cached denoise blend included), enlarges the **graded, cropped** result in the helper, and **adds the result to the catalog as a new photo** linked to the original. This is the HDR / panorama merge pattern, with one more table:

- **File:** next to the original, `<stem>-Enlarged-<2x|4x|NNNNpx>.jpg`, never overwriting (collision-avoided: `-2`, `-3`). The name is editable in the dialog.
- **Catalog row:** a new `images` row (`add_image_with_edit_stack`, **empty edit stack**: the original's edits are baked in), so it is rated, edited, collected, exported and printed like any photo. Width / height are the new size.
- **Inherits** the capture metadata (camera, lens, exposure, date, GPS), IPTC (caption, copyright, contact) and keywords. **Does not inherit** rating, flag or colour label (they judged the original), nor the collections (an option, §7.4).
- **Provenance:** `upscale_sources(result_image_id PRIMARY KEY REFERENCES images ON DELETE CASCADE, source_image_id, source_version_id, scale, strength, model_id, source_stack_hash, created_at)`. `source_image_id` has no FK, like the merge tables: removing the original later must not delete the enlargement. Pure provenance, never read by a render path.
- **Connected, in both directions.** From the enlargement: "Enlarged ×4 from <original>", *Go to original*. From the original: "1 enlargement: ×4", *Show enlargements*. A removed original shows "original removed" (the link text, not an error). **Edited since:** the row stores the original's stack hash at the time; when it differs now, both sides say so and offer **Enlarge again**, which makes another connected photo (never overwrites the first).
- An enlargement can be enlarged again (it is a normal photo), but the menu entry is disabled on it with a reason while a model-enlarged photo is the source, to avoid compounding invented detail by accident (a one-line rule; revisit if users ask).

Why not a **virtual copy**: it shares the source file, so it cannot carry a different pixel size; adding a size-bearing version would touch every consumer of `image_versions` (thumbnails, histogram, the Develop decode, export). Why not an **edit-stack op**: it breaks the single pixel grid (§1.2). Why not **export-only**: the user asked for the result to live in the catalog, and a catalog photo is what makes it printable and ratable.

### 3.2 UX (designed first, in a mock)

The interactive mock [ai-enlarge-mockup.html](../ux/mockups/ai-enlarge-mockup.html) implements the behaviour so it can be judged by use before any app code. What it fixes:

- **Entry points:** *Enlarge (AI)…* in the photo right-click menu (RFC-0028) and a button in the Library / Develop info panel; disabled with a reason on an enlargement. When the connection exists the menu also offers *Go to Original* / *Show Enlargements*.
- **One dialog, nothing written until Enlarge.** Left: a **1:1 before / after** of about 1 MP with a draggable split and a **navigator** to move the region (the model blended with a plain resize on the right, labelled with the Strength). Right: **Scale** (2x / 4x / *Long edge* with a px field, capped at 65,535), **Strength** with the hallucination hint ("look at the railings"), a summary card with **output pixels, megapixels, time on this computer and file size before anything runs**, a **very-large warning** (above 100 MP: several GB, most editors cannot open it), **Save as** JPEG 95 / JPEG 100 and the editable file name with a note that it is saved next to the original and linked, and the **metadata check box** (§3.6, on by default).
- **A job, not a modal wait.** *Enlarge* closes the dialog at once; progress ("tile 7 of 20") and **Cancel** live in the status strip, the Library stays usable, one job at a time; Cancel leaves no file and no row. On success: a toast with *Show*, the new photo selected and pulsed in the grid.
- **Library cues:** the enlargement carries an **×N chip**, the original a **link glyph**; a rail entry / filter **AI enlarged**; a **Connected photos** block in the info panel on both sides.
- **Model not installed:** the consent state inside the same dialog, in the AI Denoise / Select Subject wording family (what, how big, from where, checksum; *Import…* as the offline path).
- **Keyboard:** the menu opens with Shift+F10, the dialog is reachable by Tab, the scale control is a radio group, Esc closes.
- **Not in the mock** (checked at implementation): the real preview, real thumbnails of very large JPEGs, drag and drop, Windows.

### 3.3 The job: the helper, tiled, streamed, cancellable

- **A new helper op** `upscale_run { model, image_path, out_path, scale, strength?, region? }`, next to `denoise_run`, on the same reader thread: `progress {done, total}` between tiles, a `cancel` naming the job stops it and leaves no file, anything else waits its turn (Select Subject waits during a job, as with denoise).
- **Input is the rendered, graded, cropped picture** at native resolution, produced by the existing render path with the denoise blend (RFC-0027 §3.2) and handed to the helper as a lossless intermediate. The model runs on display-referred pixels, as it was trained; sharpening and grain the user applied are enlarged with the picture.
- **Streamed output.** The denoise tiler keeps a float accumulator for the whole image (about 0.4 GB at 24 MP); at 4x that would be over 1 GB for a 24 MP source. The upscale tiler works in **bands of tile rows**: blend the overlap with the previous row, write finished rows to the encoder, free them. Peak memory is a few rows of tiles plus the model session, independent of the output size.
- **Tiling and seams.** Input tiles of 256 or 512 px with 16-32 px overlap, linear-feather blended on the output grid (the weights of `ai/tiling.rs`, scaled by 4). Verified like RFC-0027's: a hand-written nearest-neighbour 4x ONNX through the real helper must equal a plain nearest enlargement for awkward sizes, so a seam error is the tiler's, not the model's.
- **2x and long edge** are the model's 4x result resampled down with Lanczos (more compute than the target needs, one model, one code path); whether a native 2x model is worth shipping is a 4a question.
- **Strength** is `out = lanczos + (model - lanczos) * strength / 100`, applied per band, so the Preview's slider is the same blend on an already-computed pair.
- **Encoding:** the rows go to a JPEG encoder (quality 95 or 100, 4:4:4) into `<name>.jpg.part`, renamed on success. **Open for 4b:** whether the encoder in use accepts rows (a streaming encoder) or needs the whole image; if the latter, the first cut caps the output at about 200 MP (a 200 MP RGB buffer is 600 MB) and says so in the dialog.

### 3.4 Judging it before committing: the crop preview

An explicit **Preview** shows the before / after of §3.2. It reuses RFC-0027's crop-preview machinery (a rectangle of about 1 MP at the chosen centre, the plain enlargement and the model result written as before / after pictures into `previews/`, so the webview can load them), about 3 s per megapixel on an M1 Pro for the model.

### 3.5 Pipeline order with denoise

`decode -> ai_denoise blend -> ... -> graded crop -> enlarge`. A noisy small file is denoised first (the cached pre-stage), graded, then enlarged; enlarging first would enlarge the noise and make the model draw it as texture. If the stack has `ai_denoise` and no cached copy exists, the dialog says so (as Export's pre-flight does, RFC-0027 §3.6) and offers to denoise first.

### 3.6 The metadata note (check box)

The check box (default on, in the dialog) controls whether the **JPEG's own metadata** records that the file was enlarged by a model (Software / a description line with the model id and Strength). The catalog row's `upscale_sources` record is always written regardless of the check box, because it is the connection itself and drives the Connected photos block. The user guide says that an enlarged picture contains detail the camera did not capture.

### 3.7 Models: the registry already exists

`ai_models` (RFC-0027 §3.5) gains the chosen upscaler's file(s) with a pinned SHA-256, the consent dialog family and the manual import path. Whether the model is bundled in the release installer (as denoise and Select Subject are) or downloaded on first use depends on its size and licence (§4); at under 10 MB the compact model is a bundling candidate, a 60+ MB model is not.

### 3.8 Not in this RFC

TIFF / PNG output (the catalog cannot open them; adding a TIFF decoder and a 16-bit path is its own slice, and would give a lossless result); a live enlargement preview in Develop; Print integration; face-specific restoration; video; generative fill (M7); grouping or stacking a photo with its enlargements in the grid (the chip, the glyph and the filter are the first cut); GPU execution providers beyond what a measured win justifies.

## 4. What is deliberately not decided by a guess

- **Whether Real-ESRGAN `general-x4v3` is the model.** It is the only one measured, it is tiny, and its licence is unresolved. Slice 4a spends a bounded effort on the question before any product code: read the licences (the repo's LICENSE, the release notes the weights ship in, the re-host card) and check candidates *named, not tested*: the larger Real-ESRGAN `x4plus` (about 64 MB, perceptually stronger), **a fidelity-oriented transformer model such as SwinIR (Apache-2.0 upstream; heavy)**, and any compact CNN with a clear licence. **A fidelity-oriented model invents less**, which may suit a photographer better than the crisp-but-painterly GAN look; the bake-off must include one.
- **Memory** at 256 / 512 tiles (unmeasured; RFC-0027 found SCUNet at 3.8 GB only by measuring).
- **Quality on real inputs.** The only test degradation was bicubic-4x of a clean photo. Real soft files, JPEG-compressed phone images and cropped sensor noise are untested; 4a measures on them.
- **Whether strength-blending with Lanczos hides the artefacts** or merely softens everything.
- **The app's own handling of very large JPEGs** (Library thumbnails, the Develop decode, the preview cache, Export of a 60-200 MP source): unmeasured, and the connected photo is exactly such a source. 4c measures it before the feature is offered in the UI.

## 5. Slices

| Slice | Content | Verification |
|---|---|---|
| **4a** model choice (research, no product code) | Time-boxed bake-off: `general-x4v3`, `x4plus`, one fidelity model, on the RFC-0023 §4.7 set plus real soft / compressed files. Per model: time per MP, peak RSS at 256 and 512 tiles, PSNR / SSIM, by-eye crops, hallucination examples, Strength-blend crops. **Read each weights licence.** Downloads are stated (filename, source, size) and confirmed first. | Numbers here and in RFC-0023 §4.8; a decision recorded. Proposed go criterion: licence clear to ship, at most about 1 min per 6 MP of input on an M1 Pro, peak RSS under about 1.5 GB. |
| **4b** engine | `upscale_run` helper op, band-streamed tiler, protocol (progress / cancel), model in the registry, the streaming-or-capped JPEG writer, the render-to-intermediate step | Rust hand-value tests; tiler seam test with a stub nearest-4x ONNX through the real helper (CI, no weights), cancel leaves no file; an `#[ignore]`d real-model run; peak RSS recorded at 24 MP input |
| **4c** the catalog connection | `upscale_sources` table (migration), `add_enlargement` (file naming, row, inherited metadata, provenance), `get_upscale_links` both directions, stack-hash "edited since", removal semantics; a **large-JPEG check** of thumbnails / Develop / preview cache (§4) | catalog tests (inherits what it should and not rating, source removal keeps the enlargement, edited-since, Enlarge-again naming); measurements for 60 and 150 MP |
| **4d** the dialog | Enlarge (AI) dialog per the mock: scale, Strength, summary card with time and size, Preview with split and navigator, format and name, metadata check box, consent state, status-strip job with Cancel, toast | vitest with mocked IPC; a CI-safe e2e for the model-absent path; a real-model run locally; screenshots reviewed |
| **4e** the connection in the UI | Chip and glyph, Connected photos block, rail filter, menu entries (*Enlarge (AI)…*, *Go to Original*, *Show Enlargements*), Enlarge again, disabled-with-reason on an enlargement | vitest for the builders; e2e for the links in both directions; docs |
| **4f** polish | Batch ("Enlarge" on several photos with a total estimate), user guide, backlog items | per the rule, "not verified" goes to the PROGRESS backlog |

## 6. Risks

1. **Invented detail** (§1.1). Mitigation: explicit "Enlarge (AI)" framing, the Preview before / after, Strength, the metadata note, the Connected photos block (the file always says what it is), a fidelity model considered in 4a.
2. **Time and size.** Minutes of work and a very large file. Mitigation: the estimate before running, enlarging the crop (not the frame) being the typical case, the very-large warning, Cancel, one job at a time.
3. **Licence.** If no model with clear weights terms passes 4a, the feature does not ship in M6 and this RFC records why; nothing else depends on it.
4. **Very large JPEGs in the rest of the app** (§4). A 60+ MP photo in the grid and in Develop may be slow or memory-hungry; 4c measures before the UI is offered, and the cap in §3.3 is the fallback.
5. **Catalog clutter.** Each enlargement is another photo in the grid. Mitigation: the chip, the *AI enlarged* filter and *Show enlargements*; grouping with the original is a possible follow-up, not in this RFC.
6. **Disk use.** A 60 MP JPEG is about 70 MB, a 150 MP one far more, written next to the original; the dialog shows the size first.
7. **JPEG only** means the result is lossy and 8-bit. Acceptable for the stated use (prints, sharing); the lossless route is §3.8's follow-up.

## 7. Decisions and what is left

Decided with the user on 2026-10-12 (first round): scope = a new catalog photo, a **connected image** (§3.1); bake-off first; targets 2x / 4x / long edge; default Strength by eye (session-only memory); the metadata note a check box. **Second round (same day, "go with the recommendations")**:

1. **Connected image, not a virtual copy** (§3.1).
2. **Rating, flag and colour label are not inherited** by the enlargement.
3. **Keywords, caption, copyright, contact and GPS are inherited.**
4. **Collections:** a dialog check box "Add to the same collections as the original", **off by default**.
5. **Lossless output (TIFF / 16-bit):** not in this RFC; decide after 4d, when it is clear how the result is used.
6. **Enlarging an enlargement is disabled** in the first cut.

Nothing is left open for the user before slice 4a; the open items are measurements (§4).

## 8. Slice 4a, step 1: the licence reading (2026-10-12)

Read from the public pages (no downloads, nothing run); **not legal advice**, and nothing here is a measurement.

- **Real-ESRGAN** (xinntao, github.com/xinntao/Real-ESRGAN): the repository is **BSD-3-Clause**. The README and the v0.2.5.0 release notes (where `realesr-general-x4v3` was published) say **nothing about the weights' licence**. The model-zoo page returned HTTP 503, so it was not read. The third-party ONNX re-host `Heliosoph/realesrgan-onnx` declares **BSD-3-Clause** and ships a `LICENSE`, says it is an ONNX export of the upstream weights "re-hosted for distribution stability" and names xinntao's releases as the authoritative source; it does not describe the export. One other re-packager describes `general-x4v3` as a *deep network interpolation* of two official checkpoints, which, if true, makes it a derivative of both (unverified).
- **Training-data terms:** no source read states which images the Real-ESRGAN checkpoints were trained on or under what terms. The BSD-3 text covers the weights only as far as the repository's licence is read to cover them.
- **SwinIR** (JingyunLiang/SwinIR): the repository is **Apache-2.0**; the README says nothing separate about the weights, asks users to also follow the licences of the Swin Transformer and KAIR code it is based on, and lists the real-world 4x models `003_realSR_BSRGAN_DFO_s64w8_SwinIR-M_x4_GAN.pth` (middle) and `003_realSR_BSRGAN_DFOWMFC_s64w8_SwinIR-L_x4_GAN.pth` (large) without sizes. Not read: the classical (fidelity-oriented) 4x model's page, its training data terms.
- **What this does and does not settle.** The code licences are permissive; for both families the weights' own terms are *implicit* (read from the repository licence), and the training data's are unstated. That is the same position RFC-0023 §5.4 flagged for Real-ESRGAN. It does not stop a prototype or the bake-off, but **shipping it in the installer is a judgment call for the project owner** (accept the implicit reading, ask the maintainers, or prefer a model whose card states terms for weights and data).

**Needs the user's go-ahead before 4a continues (downloads, none done):**

| File | Source | Size | Why |
|---|---|---|---|
| `realesr-general-x4v3.onnx` | huggingface.co/Heliosoph/realesrgan-onnx | about 5 MB (card) | the model already measured in RFC-0023; its inputs were not kept, so it is needed again for same-protocol numbers |
| `RealESRGAN_x4plus.pth` | github.com/xinntao/Real-ESRGAN releases | about 64 MB (from memory, unchecked) | the larger Real-ESRGAN, to be converted to ONNX locally |
| a SwinIR checkpoint (real-world `SwinIR-M_x4_GAN`, and the classical `SwinIR-M x4` as the fidelity-oriented one) | github.com/JingyunLiang/SwinIR releases | not stated on the README (to be read from the release page) | the fidelity-oriented candidate, to be converted to ONNX locally |
| a Python environment with PyTorch and ONNX Runtime | PyPI (via `uv`, in a throw-away directory) | several hundred MB | the machine has neither; RFC-0023 / RFC-0027's measurements used a scratch environment that is gone |

## 9. Slice 4a, step 2: the bake-off (2026-10-12)

Measured on an M1 Pro, ORT CPU, the eight RFC-0023 §4.7 photos (CC0), [appendix](RFC-0029-appendix/README.md). Three models, converted from the upstream `.pth` files by us (each ONNX checked against PyTorch: max abs difference 4e-6, 2e-6, 2e-6). Two degradations: **ideal** (bicubic /4, RFC-0023's protocol) and **realistic** (blur 0.8, /4, JPEG quality 50: a soft, compressed source). The comparison baseline is a **Lanczos** enlargement, since that is what Strength blends against. Means over 8 photos; HF is the fine-texture energy relative to the reference (1.0 = the same; the plain resize is about 0.25-0.3).

| | Lanczos | `general-x4v3` (Real-ESRGAN compact, 4.9 MB) | `x4plus` (Real-ESRGAN RRDB, 67 MB) | SwinIR-M real-world **PSNR** (52 MB) |
|---|---|---|---|---|
| ideal: PSNR dB / SSIM / HF | 26.19 / 0.793 / 0.29 | 24.80 / 0.791 / 0.72 | 24.26 / 0.768 / 0.85 | 25.88 / **0.812** / 0.57 |
| realistic: PSNR / SSIM / HF | 24.49 / 0.732 / 0.24 | 23.52 / 0.732 / 0.66 | 23.24 / 0.711 / 0.78 | **24.76 / 0.750** / 0.46 |
| best Strength blend vs Lanczos, ideal (PSNR / SSIM) | - | 30%: 26.44 / 0.805 | 30%: 26.35 / 0.802 | 50%: **26.57 / 0.813** |
| best blend, realistic | - | 30%: 24.75 / 0.744 | 30%: 24.75 / 0.740 | 50%: **25.10 / 0.751** |
| time, 256x256 input tile (-> 1024x1024) | - | **0.55 s** | 9.0 s | about 7.8 s (16 tiles of 64x64 at 0.49 s, no overlap) |
| peak RSS | - | 161 MB (256 tile), 438 MB (512 tile) | 1.6 GB (256), **5.0 GB (512)** | 350 MB (64 tile) |
| **6 MP input, extrapolated** | - | **about 50 s** | about 14 min | about 21 min with the tiling overlap |

1. **`x4plus` is out.** Worst on every metric, 16x slower than the compact model and up to 5 GB of memory at a 512 px tile: nothing justifies it in a laptop app.
2. **The GAN-trained models invent detail; the fidelity-trained one does not.** By eye (crop sheets in the appendix; 2 of the 16 sheets were examined closely, the rest only skimmed): `general-x4v3` and `x4plus` are crisper but draw hard edges and colour fringes that were not there (conifer branch tips in warm colours, a hard-edged structure on a soft, compressed source). SwinIR-PSNR stays close to the plain resize, just cleaner, and is softer. This matches the metrics: only SwinIR-PSNR beats the plain resize at full strength on the realistic case, and it has the best SSIM everywhere.
3. **The Strength blend is validated, and the metric optimum is low.** For every model a blend of 30-50% beats both the plain resize and the pure model on PSNR and SSIM (`general-x4v3`: 26.44 dB at 30% against 26.19 plain and 24.80 pure). Metrics favour restraint, and the RFC's own default is to be set by eye (§7), so the metrics are only a prior: **start the by-eye default near 40-50%** and let the 1:1 Preview decide.
4. **Only `general-x4v3` passes the proposed go criterion** (§5: at most about 1 min per 6 MP of input, under about 1.5 GB). SwinIR-PSNR is about 25x slower on CPU, so as a second model it would be an optional "High fidelity (slow)" choice, not the first cut. A GPU / CoreML run was not tried and could change that; SwinIR's ONNX export is fixed-size (64) because it pads with Python integers.
5. **Licences (§8) apply to all three**: code under BSD-3 / Apache-2.0, weights' terms implicit, training-data terms unstated.
6. **Caveats, not closed:** eight CC0 photos and two synthetic degradations (no real phone or scanned file, no real sensor noise, no JPEG-compressed photo at full size); the high-frequency ratio is crude and a hard painterly edge scores like real texture; the by-eye part is two sheets; SwinIR runs as 64 px tiles with an 8 px overlap and a simple feather (seams were not inspected), so its quality figures may differ slightly from an untiled run; the GAN SwinIR checkpoint and SwinIR-L were not tested; no GPU, no Windows; peak memory is the model session in one process, not the app's.

**Proposed decision for 4b (user to confirm):** `realesr-general-x4v3` is the first-cut model (converted by us from the upstream `.pth`, not the third-party ONNX; 4.9 MB, so it can be bundled), shipped only if the project owner accepts the **implicit weights licence** of §8; the Strength default starts at 40-50% and is set by eye in 4d. SwinIR-PSNR is recorded as the optional fidelity model for later, not built. `x4plus` is dropped.

## 10. Slice 4b, part 1: the engine (2026-10-15)

Branch `feature/upscale-engine`. The helper op `upscale_run` and everything it needs, tested without weights in CI and once with the real model. **Not yet done from §5's 4b row** (needs the user's licence decision, §9 last paragraph, and a hosted file): the model's entry in the registry (pinned hash, consent / download / import, bundling) and the app-side "render the graded crop to a lossless intermediate" step; both follow in 4b part 2.

- **Wire:** `UpscaleRun { model, image_path, out_path, out_width, out_height, strength 1..=100, jpeg_quality, region }`, answers `UpscaleResult { width, height, tiles, ms }`, streams `progress`, honours `cancel` (code `cancelled`), invalid input is `bad_request` (strength, quality, a side of 0 or over 65,535). The caller decides what 2x / 4x / *long edge* mean in pixels and sends the exact size, so rounding rules live in one place (4d). `region` is the crop for the Preview (the Preview's *two* pictures, plain and model, are 4d).
- **Tiler:** `ai/upscale_tiling.rs`, input tiles of **256 px with a 32 px overlap** (the compact model's receptive field is about 65 px, so a tile edge is disturbed about 33 px in; the feather covers that), feathered by `tiling.rs`'s weights, which are separable, so the weight sums are two 1-D arrays instead of one per pixel. It keeps a **band** of `tile x scale` output rows and hands out every row above the next tile row's first window. Verified against a plain nearest enlargement for awkward sizes (1x1 up to 150x70, with a stand-in 4x model: any difference is the tiler's), feathered across horizontal and vertical seams, and band size independent of the image height.
- **Resampling:** `ai/resample.rs`, a streaming Lanczos-3 (rows in, rows out as their taps arrive). It makes 2x and *long edge* from the model's 4x rows and the plain enlargement Strength blends with; checked equal to a whole-image resize and to keep only a window of rows. `ai/upscale_pipeline.rs` ties them: `out = plain + (model - plain) * strength`, per row, one rounding at the end.
- **Writer (the §3.3 open question, answered):** the `image` crate's JPEG encoder needs the whole buffer, so the helper now uses **`jpeg-encoder` 0.6.1** (pure Rust, no dependencies, licence "(MIT OR Apache-2.0) AND IJG"), which pulls one row at a time. **No size cap is needed**; the limit is JPEG's 65,535 px a side. 4:4:4 from quality 90 up. A cancel, a model error or a full disk makes the writer refuse further bytes so the encoder stops at once; the `.part` file is removed; the result appears by a rename only when complete.
- **Not in the helper (4c / 4d):** the JPEG's metadata note (§3.6; an EXIF step with the existing metadata writer after the file exists), an ICC profile, the file naming.
- **Verified:** 14 pure tests (lib) and 8 helper tests (JPEG round trip within 1.5 grey levels of a nearest enlargement, an odd target size, cancel leaves no file, model error, limits); **through the real helper binary and ONNX Runtime with a hand-written one-node nearest-4x ONNX file** (CI, no weights): progress per tile, a quality-100 result equal to a plain nearest enlargement within the JPEG's error, 2x with a blend, a region, a cancel mid-job that leaves no file and a helper that carries on, bad requests that leave nothing behind, a request arriving during a job waiting its turn. **Real model** (`realesr-general-x4v3`, our conversion from 4a, release build, M1 Pro, CPU): a 3960x2640 photo (10.5 MP) to **7920x5280 (42 MP) in 216 tiles, 119 s (550 ms per tile, 11.4 s per input MP), peak RSS 771 MB**, Strength 50, quality 95; looked at the whole picture and a 1:1 crop across several tile boundaries: no seam visible. A cancelled second run wrote nothing.
- **Against the go criterion (§5):** memory is well under 1.5 GB at 10.5 MP in / 42 MP out. Time is about **68 s per 6 MP** rather than 4a's extrapolated 50 s: the 32 px overlap costs 30% more tiles than 4a's no-overlap figure. Peak memory grows with the input *width* (the band), about 0.3 GB at 6000 px wide; a 24 MP input was not run.
- **Not verified:** a 24 MP or larger input (memory and time are extrapolated), a 100+ MP output, GPU / CoreML, Windows, a by-eye comparison of the tile overlap choice (32 px was chosen from the receptive field, not compared with 16 or 64), the `jpeg-encoder` licence (IJG part) against the project's own licence policy.

## 11. Slice 4b, part 2a: the app-side job (2026-10-15)

Same branch / PR as part 1. `upscale_job.rs`, free of Tauri and the catalog: `output_size` (2x and 4x are exact; a long edge keeps the aspect ratio and rounds the short side; refused when not larger than the photo or past 65,535 px; any other scale refused), `megapixels` / `is_very_large` (above 100 MP, §3.2), `tile_count` (the helper's grid, for the progress total and a time estimate), and `run_enlargement`, which renders **`export::render_full_resolution`** (the pixels Export and Print use: lens, perspective, grading, crop, cached denoise blend) to a PNG in a scratch directory, sends `UpscaleRun` through a closure standing in for the helper round trip, and removes the intermediate whatever happens. So the "render-to-intermediate step" needed no new renderer.
- **Verified:** 8 lib tests (size rules incl. portrait, thin strip and the JPEG limit; the request carries the planned size, strength, quality and the model path; **the intermediate is the cropped picture**; the intermediate is gone after success and after a failure; an unplannable size or a missing source sends nothing).
- **Still parked (4b part 2b):** the model registry entry (pinned hash, consent / download / import, bundling), which waits for the licence decision (§9) and a hosted file; the Tauri commands, which need the model lookup and the catalog row (4c / 4d).

## 12. Slice 4c: the catalog connection and the large-JPEG measurement (2026-10-16)

Branch `feature/upscale-catalog`. #238 (4b: engine and app-side job) is merged.

**Catalog (`catalog/upscale_sources.rs`).** Table `upscale_sources(result_image_id PK -> images ON DELETE CASCADE, source_image_id, source_version_id, scale, strength, model_id, source_stack_hash, created_at)` as §3.1, with one change: **`source_image_id` and `source_version_id` are nullable and are set to NULL when the original is removed** (`remove_images`), instead of being left dangling. SQLite reuses the largest row id, so a dangling id could later point at an unrelated photo; NULL is also what the UI reads as "original removed". `add_enlargement` is one transaction: a new `images` row (the original's camera / lens / exposure / GPS / date, copyright and contact; the new file, hash, size and pixel size), an empty-stack version carrying the original version's caption, the keywords, the manual collections only when asked (§7.4; smart collections have no members to copy), and the provenance row. Rating, flag and colour label are not copied. `get_enlargement_origin` / `get_enlargements_of` read both sides (the Tauri command `get_upscale_links(image_id)` returns both) with **`edited_since`**: the original's current stack hash against the stored one, `None` when the version is gone. `edit_stack_hash` is blake3 of the stack with ops ordered by name (stable): the renderer finds each op by name, and the Develop store re-appends a touched op on every change, so a slider moved away and back is not an edit (the thumbnail file names hash the raw order and live with that, see `import.rs`). Also `is_enlargement` (for §7.6) and, in `upscale_job.rs`, `result_stem` (`<stem>-Enlarged-4x` / `-2x` / `-6000px`) and a streamed `fingerprint` (hash and size without reading a 150 MP file into memory).
- **Verified:** 10 lib tests (inherits what it should and not rating / flag / label; collections only when asked and not the smart one; both directions; edited-since true after a change and false for a reorder; enlarge-again makes a second connected photo; removing the original keeps the enlargement and blanks the link, and a new photo reusing the id is not mistaken for the original; removing a result drops its row only; a missing version adds nothing; a path clash rolls everything back).
- **Not yet used by anything:** `add_enlargement` waits for the job command (4d); the Library chip and glyph need `list_images` to carry the link (4e).

**Large JPEGs in the rest of the app (§4), measured** (release build, M1 Pro, `large_jpeg_probe` in `upscale_job.rs`, one step per process, peak RSS from `/usr/bin/time -l`). Test files: a photo resized to **60 MP (9000x6667, 8.6 MB)** and **150 MP (14142x10607, 20.7 MB)** with mild grain; they compress far better than a real 150 MP camera file, so decode times are optimistic, pixel-proportional memory is not.

| step | 60 MP | 150 MP |
|---|---|---|
| full decode (`decode_preview`) | 0.23 s, 0.38 GB | 0.53 s, 0.94 GB |
| Library thumbnail (decode + downscale) | 0.38 s, 0.43 GB | 0.91 s, 1.0 GB |
| Develop draft preview (2048 px PNG) | 0.48 s, 0.62 GB | 1.0 s, 0.95 GB |
| Develop 1:1 preview (full-size PNG) | 0.58 s, 0.46 GB | 1.5 s, 1.1 GB |
| **Export / Print render** (`render_full_resolution`, a 3-op stack) | **19.6 s, 3.8 GB** | **49.6 s, 8.1 GB** |
| **Export** (render + JPEG 90, no resize) | **19.8 s, 4.0 GB** | **50.7 s, 6.8 GB** |

- The Library and the Develop *draft* are fine at 150 MP (about 1 GB, about 1 s). **Export and Print are the problem**: about 55-63 MB of peak memory per megapixel and about 0.33 s per megapixel, so a 150 MP enlargement needs about 8 GB and nearly a minute to export. That is existing behaviour (the same code renders a 24 MP RAW with about 1.4 GB), not something this slice changed, but an enlargement is the first thing that makes it routine. It also bounds the enlarge job itself: the job's own render step is the original's (small), and the enlargement is written by the streaming path, so **making** a 150 MP file is cheap and **exporting** it is not.
- **Consequences for 4d:** the very-large warning (§3.2, above 100 MP) should say what it means for later work (about 55 MB per megapixel to export or print, so about 5.5 GB at 100 MP), and the dialog's size summary should carry that figure. A tiled / streamed export render is the real fix and is **not** in this RFC; recorded as a follow-up to decide on after 4d.
- **Not verified:** Develop in the real window with a 60 / 150 MP JPEG (the 1:1 tier goes through `sourceTexture.js`, which downscales past the GPU's texture limit, by reading, not by running); a real camera-sized 150 MP file (these compress better); the edit stack here was three simple ops (masks, lens correction and AI denoise cost more per pixel); Windows; a 16 GB machine's behaviour at 8 GB.

## 13. Slice 4e: the connection in the Library UI (2026-10-17)

Branch `feature/upscale-links-ui` (stacked on #239). Everything §3.2 and §5 list for 4e that does not need the dialog or the job:
- **Grid:** `list_images` now carries `enlarged_scale`, `enlarged_from` and `enlargement_count` (two scalar sub-selects of `upscale_sources`). `GridCell` shows a persistent **"×N" chip** on an enlargement and a **link glyph** on an original that has some (top right; the file name's bar leaves room for them). The chip survives the original's removal.
- **Filter:** an **AI enlarged** toggle in the filter bar (its own group, `aria-pressed`, not one of the file-type radio choices); part of "Reset filters".
- **Photo menu:** single photo only: *Go to Original* on an enlargement (greyed with "The original was removed from the catalog" when the link is blank), *Show Enlargement(s)* on an original.
- **Connected photos block** (`ConnectedPhotos.svelte`, in the Library info panel under File): "Enlarged x4 (AI) from <name>" + *Go to original*; "1 enlargement: x4" / "2 enlargements: x2.5, x4" + *Show enlargement(s)*; the "edited since" notes; "original removed" as text with no button. It asks the backend only for photos that have a link, and re-reads on photo change, link change and return from Develop. The wording is the pure `connectedLines` (`lib/upscaleLinks.js`).
- **Navigation** (`actions/upscaleActions.js`): `revealPhotos` selects the targets, leaves Develop / loupe / survey for the grid, and clears the folder / collection / person / map scope and the filters **only when they would hide the target**; *Show enlargements* selects all of them.
- **Not in 4e (needs 4d):** *Enlarge (AI)…* in the menu and the info panel, and *Enlarge again* on the edited-since note. The Develop info bar does not show the block yet (Library only).
- **Verified:** vitest 1017 (26 new: chip text, every line of the block, filter alone and combined, the menu entries in each state, the navigation actions incl. scope clearing only when needed); svelte-check 0 errors; `cargo test --lib` incl. the extended catalog test.
- **Not verified:** how any of it looks or behaves in the real app. There is no way to make an enlargement yet (the job command is 4d), so no e2e or screenshot was possible; the chip, the glyph, the filter button in the bar at 1360 px, the block's spacing and the grid's selection / scroll-to-target after *Show enlargements* are untested. Windows; a screen reader on the chip (it is a span with a title) and on the glyph (`role=img` with a label).
