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
