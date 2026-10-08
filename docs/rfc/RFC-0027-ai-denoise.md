# RFC-0027: AI denoise as an explicit, cached pre-stage (M6 slice 3)

- Status: Draft for review
- Date: 2026-10-08
- Relates to: [RFC-0023](RFC-0023-on-device-ai-inference.md) (§4.1, §4.7, §5.3: the numbers and the recommendation), [RFC-0026](RFC-0026-click-select-segmentation.md) (the `emulsion-ai` helper, the model consent flow), [ADR-0009](../adr/ADR-0009-ml-inference-runtime.md) (`ort`, Proposed), [ADR-0004](../adr/ADR-0004-rendering-and-color-management.md) (Develop's pipeline), [RFC-0012](RFC-0012-luma-nr-guided-filter.md) (the existing deterministic NR)

## 0. Process note

Design only. **No product code is changed and nothing is measured here.** Every number is RFC-0023's: Apple M1 Pro, ORT CPU, SCUNet `color_real_psnr`, eight synthetic-noise photos and six real ISO 6400 scenes. Nothing is known about memory, Windows, or any other hardware. Per the project's RFC-before practice for the AI slices, this is written before slice 3 is built; the user picks the open questions in §7.

## 1. Problem

M6 promises "AI-assisted denoise (distinct from and generally higher quality than M1/M3's traditional NR)". The existing Noise Reduction (Luminance / Color) is live, cheap and deterministic. A learned denoiser is much better on high-ISO files (RFC-0023 §4.7: +6.2 dB over the noisy shot and +3 dB over the best blur on real noise) but:

1. It is **slow**: about 16 s per megapixel on CPU, so roughly 6 minutes for a 24 MP frame. It cannot be a slider.
2. It **changes the picture**: on flat finely-textured surfaces it replaces real grain with a waxy smoothness and shifts tone slightly. It needs an amount control and a before/after.
3. It sits **before** the rest of the pipeline in the sense that matters: noise should be removed from the pixels the user then pushes (exposure, shadows), not after.

## 2. Facts this design rests on (all from RFC-0023)

- SCUNet `color_real_psnr`, Apache-2.0 upstream, ONNX re-host `Heliosoph/scunet-onnx`, **3.8 MB graph + 73 MB weights**. CPU only (CoreML is 65x slower on it).
- The export accepts **only H and W that are multiples of 64**; a product wrapper must reflect-pad and tile.
- 512x512 tile: 3.85 s; 1024x1024: 16.2 s (linear in area). 24 MP is about 92 tiles of 512x512 with no overlap, so about 6 minutes before overlap.
- Quality is a **trade, not a free win**: waxy flat textures, slight tone and colour shift (magnitude not measured), 8 of 8 photos beat a blur baseline on synthetic noise, 6 of 6 beat it on PSNR for real noise but 2 of 6 lose on SSIM.
- The helper process (`emulsion-ai`) already exists, serves one request at a time, and is the only place `ort` is linked.

## 3. Decisions

### 3.1 It is an edit-stack op that reads a cached result, not a live filter

A new op `ai_denoise { amount: 0..100 }`. Applying it needs the **denoised image**, which is produced by a separate explicit job (§3.4) and stored in a cache keyed by `(content hash, model id, tile parameters)`. The op itself is only the *amount*: at render time

`out = source + (denoised - source) * amount / 100`

so the slider is live and costs one blend, and the model never runs while the user drags. If the op is present but no cached result exists, the render treats it as inactive and the UI shows "Not denoised yet" (never a silent 6-minute stall).

The cache holds an 8-bit lossless image at the decoded size (what `decode_preview` returns), not a 16-bit linear one, because that is what the pipeline carries today (ADR-0004's open linear-pipeline item is out of scope). Roughly 40-60 MB per 24 MP photo; see §7 for eviction.

### 3.2 Position in the pipeline: first

`decode -> ai_denoise blend -> lens correction -> perspective -> edit stack -> crop`. Denoising before the geometric resamples keeps the noise statistics the model was trained on, and before grading means a later +2 EV is lifting cleaned shadows. The deterministic Luminance/Color NR stays where it is (inside the edit stack) and can be combined; the panel says so.

### 3.3 Develop preview

Develop shows a downscaled decode. When a cached result exists, the same downscale is applied to it and the preview source becomes the blend; it is rebuilt when the amount **settles** (not per drag step), then re-uploaded as the source texture. During a drag the old blend stays. This keeps the GPU shader and the mask/histogram machinery unchanged. The detail needed (is it a Rust command, a JS blend, how it meets RFC-0024's preview cache) is slice 3c's to settle with a measurement.

### 3.4 The job: tiled, in the helper, cancellable, with a crop preview first

- **A new helper op** `denoise_run { model, image_path, out_path, region? }`. The helper reads the decoded source, reflect-pads to multiples of 64, runs **512x512 tiles with a 32 px overlap** blended with a linear feather (no visible seams), and writes the result. It streams `progress` lines (tiles done / total) and checks a cancel flag between tiles; a cancel leaves no partial cache entry.
- **Crop preview first**: `region` is a rectangle (about 1 MP at the view centre, about 16 s). The UI shows it as a before/after split of that rectangle; only then does it offer *Denoise whole photo*. This is RFC-0023 §5.3's "explicit action with a crop preview".
- **Whole-photo job**: runs in the background, shows progress and a Cancel, survives switching photos, and ends in a toast. Only one at a time.
- **One queue** (RFC-0026 §3.1): while a job runs the helper cannot serve a Select Subject click. Options in §7; the cheapest honest behaviour is that tile boundaries are the only places the helper looks at its queue, so a click waits at most one tile (about 4 s) and the job resumes. This needs the helper to read stdin between tiles, which today it does not.

### 3.5 Models: generalise the model manager

`segment_models.rs` is already a pinned-hash fetch/verify/import/cancel manager for one model family. Generalise it to a small registry (`ai_models.rs`: id, files, sizes, SHA-256, pinned source) with the same consent dialog wording per family. SCUNet's two files join SAM 2's; the manual "I have the files" path stays.

### 3.6 Export

Export reads the cache; it **never silently runs the model**. If a photo's stack has `ai_denoise` and no cache entry, export shows it in the existing pre-flight list ("3 photos need denoising first, about 6 min each") and offers to run the jobs first, skip the effect, or cancel. No hidden multi-hour batch.

### 3.7 Not in this RFC

Super-resolution (own RFC, Real-ESRGAN weights licence unresolved), a live denoise slider, GPU/CoreML/DirectML execution, RAW-domain denoising, and any model other than the one chosen in §5 slice 3a.

## 4. What is deliberately not decided by a guess

- **Whether SCUNet is the model.** RFC-0023 named lighter candidates (NAFNet-class, DRUNet-class) and tested none. 6 minutes per frame is the main product risk. Slice 3a spends a bounded effort on this before any product code (§5).
- **Memory.** SAM 2 measured about 1.1 GB resident in the helper. SCUNet at 512x512 is unmeasured; it is the same helper, so the high-water mark is the larger of the two only if they do not overlap (they can, §3.4).
- **Tone/colour shift magnitude** (§2): not measured; slice 3a measures it on the NIND set.

## 5. Slices

| Slice | Content | Verification |
|---|---|---|
| **3a** model choice (research, no product code) | Time-boxed bake-off: SCUNet vs one or two lighter denoisers on the RFC-0023 §4.7 sets (synthetic + NIND), same harness. Measure time per MP, peak RSS, PSNR/SSIM, and mean tone/colour shift. Confirm each re-host's weights licence. **Needs downloads: filename, source and size are stated and confirmed first.** | Numbers in RFC-0023 §4.8 and here; decision recorded. Go criterion proposed: at most about 1 min per 24 MP and within about 1 dB of SCUNet on NIND, else SCUNet ships as the background job of §3.4. |
| **3b** engine | `ai_denoise` op (parse, blend, JSON round trip, pipeline slot), `denoise_run` helper op (pad, tile, feather, progress, cancel), `ai_models.rs` registry, cache with keys, CPU render stage | Rust hand-value tests (blend, pad/unpad, tile-seam test: a constant and a ramp image come out unchanged by the tiling itself using an identity stub model); an `#[ignore]`d real-model run reproducing the Python outputs (PSNR within 0.1 dB); peak RSS recorded |
| **3c** UI | Detail panel *AI Denoise* section: Amount, crop preview with before/after, *Denoise whole photo*, progress and Cancel, state line ("Not denoised yet" / "Denoised, model X"), preview blend; consent dialog wording per model | vitest with mocked IPC; e2e for the model-absent path (CI-safe); a real-model run locally |
| **3d** export + polish | Export pre-flight (§3.6), cache eviction, clicks interleaved between tiles if chosen, docs | per the rule, "not verified" goes to the PROGRESS backlog |

## 6. Risks

1. **Six minutes per frame** may be unacceptable even as a background job on weaker laptops (an M1 Pro is a high-end laptop). Mitigation: 3a, the crop preview, the cancel.
2. **It changes the photograph** (waxy texture, tone shift). Mitigation: Amount, the before/after, default amount below 100 (value chosen in 3c from the NIND crops).
3. **Cache size and staleness**: a changed source file or a different decode must not reuse a result. The key includes the content hash and decoder version; eviction in 3d.
4. **Preview/export disagreement**: the preview blend is downscaled-then-blended, export is blended-then-processed. For a linear blend the difference is only the resampling order; 3c must show it is not visible.
5. **8-bit working image**: noise already quantised in deep shadows cannot be recovered, and a large later lift can still band. Named, not solved (ADR-0004's linear-pipeline item).

## 7. Open questions for the user

1. **Run the bake-off (3a) first, or build on SCUNet now?** Recommendation: bake-off first, time-boxed to one slice; it decides whether the feature is a 1-minute action or a 6-minute background job, which changes the UI.
2. **While a denoise job runs, may Select Subject wait?** (a) it shows "Denoising, try again when done"; (b) clicks interleave between tiles (about 4 s wait, more helper work). Recommendation: (a) for 3b-3c, (b) in 3d if it matters.
3. **Cache eviction:** an LRU cap (say 2 GB, user-visible) or keep until the photo is removed. Recommendation: LRU cap with a Settings line.
4. **Export of an un-denoised photo:** the pre-flight prompt of §3.6, or a per-export checkbox "Denoise missing photos first". Recommendation: the prompt.

## 8. Corrected after slice 3a (the bake-off, 2026-10-08)

Measured on the same machine and protocol as RFC-0023 (M1 Pro, ORT CPU; [appendix](RFC-0027-appendix/README.md)). SCUNet re-measured here **reproduces RFC-0023** (real-noise mean 32.67 dB / 0.886, the same as §4.7), so the new numbers are comparable. The one lighter candidate tried was **NAFNet-SIDD-width32** (megvii-research/NAFNet, MIT; 29.2 M parameters; the checkpoint is a third-party mirror, hash recorded, not compared with the upstream file), exported to ONNX with dynamic sizes.

| | SCUNet `color_real_psnr` | NAFNet-SIDD-w32 |
|---|---|---|
| 512x512 tile, CPU | 3.93 s | **0.84 s** |
| 1024x1024 | 15.4 s | **3.0 s** |
| **Peak RSS**, 512x512 | **3,842 MB** | **695 MB** |
| Peak RSS, 1024x1024 | **9,534 MB** | 2,016 MB |
| NIND ISO 6400, 6 scenes, PSNR / SSIM (mean) | 32.67 / 0.886 | 32.55 / **0.895** |
| High-frequency retention (std of img - blur 2; reference ISO 200 = 7.83, noisy = 10.85) | 6.41 | 5.76 |
| Tone shift (mean abs change of the 8-px-blurred image, 8-bit levels) | 1.75 | 1.77 |
| Full 24 MP, **extrapolated** (about 117 tiles of 512 with a 32 px overlap, not run) | about 7.7 min | **about 100 s** |

1. **SCUNet's memory is the finding RFC-0023 could not see.** 3.8 GB for one 512x512 tile (and 9.5 GB at 1024x1024) is more than three times the SAM 2 session's 1.1 GB in the same helper. It would make the helper the app's largest process and make "while a job runs, Select Subject waits" (§3.4, §7.2) the only safe behaviour, and it may not run at all on a 8 GB laptop. Not measured: Windows, or whether limiting ORT's memory arena helps.
2. **NAFNet is 4.7x faster and uses 5.5x less memory at the same mean quality.** Per scene it is mixed: ahead on Leonidas (+3.6 dB), stairs (+0.4) and on SSIM overall; behind on tree1 (-1.8 dB, foliage, and a 3-level colour drift on that scene), chapel (-1.5) and directions (-0.9). By eye (crops in the appendix) it keeps a fine grain on flat walls and stone where SCUNet gives a smooth blotchy surface; it is a little softer on dense foliage and leaves a trace more residual noise on the flat chapel wall. Neither is clearly better to the eye on six scenes.
3. **The proposed go criterion (§5) is met on quality (within 1 dB, here 0.12 dB) and narrowly missed on time (about 100 s per 24 MP against "about 1 min")**, an extrapolation, not a run. It is still a background job, not a slider, so §3.4 stands unchanged; only the numbers in it get better (about 100 s, with a crop preview of about 3 s per megapixel).
4. **Decision for 3b (proposed, user to confirm): NAFNet-SIDD-width32 is the first model; SCUNet is not shipped in the first cut.** Revisit SCUNet only as an optional "stronger" model if users ask for it, and only after its memory is understood.
5. **Caveats, not closed:** the NAFNet weights were trained on SIDD (smartphone sensor noise) and tested here on six NIND scenes (a mirrorless/DSLR dataset) and no RAW-domain noise; one 1024 crop per scene; the checkpoint came from a mirror, so a re-check against upstream is wanted before anything ships; the synthetic-noise table of RFC-0023 §4.7 was **not** repeated for NAFNet (SCUNet was trained on synthetic degradations, so that comparison would favour it); the ONNX export uses the legacy exporter and is only checked indirectly (the PSNR matches expectations), not against PyTorch output; no Windows run; NAFNet needs sizes that are a multiple of 16 (the wrapper pads to 64 for both, harmless).
6. **Licences:** NAFNet code and checkpoint MIT upstream (LICENSE read at the pinned commit), the SIDD dataset MIT per its own page; the mirror card says "per-file". SCUNet Apache-2.0.

Open questions §7.1 is answered (bake-off done, NAFNet proposed); §7.2-7.4 keep their recommendations. With NAFNet's 0.7 GB the "interleave clicks between tiles" option (§7.2b) becomes cheap in memory but is still extra helper work, so it stays a 3d candidate.

## 9. Corrected during implementation (slice 3b, 2026-10-08)

The engine is in, with no UI (slice 3c). Decisions that changed or settled the text above:

- **Protocol (§3.4).** Three additions to RFC-0026's JSON-lines protocol: `denoise_run {model, image_path, out_path, region?}`, `cancel {target}`, and **interim `progress {done, total}` lines** under the job's id. The helper now reads stdin on a **reader thread**; while a job runs it looks at what has arrived **between tiles**: a `cancel` naming the job stops it (the job then answers `cancelled`, nothing is written), anything else is parked and served in order afterwards. So "Select Subject waits during a job" (§7.2a) is what the code does, and a click waits at most one job. The supervisor's timeout is now the time **between** lines for a streaming request, not the job's length.
- **Tiling (§3.4) and the seam.** 512 px windows every 480 px, the **last window clamped to end at the image edge** (more overlap there, never padding; only an image smaller than one tile reflects). Weights are a product of two linear ramps over the 32 px overlap on every side that has a neighbour, `sum(w*out)/sum(w)`. Verified by an identity model returning every pixel unchanged for awkward sizes, a pointwise model equal to the whole-image result within one level, and a per-window-constant model that ramps monotonically across the overlap (`ai/tiling.rs`); and end to end through the **real helper and a real ONNX Runtime session with a hand-written one-node `Identity` ONNX file** (`tests/ai_helper.rs`, runs in CI without weights): the output PNG equals the input exactly, progress is one line per tile, a region crops exactly, a cancel after the first tile stops it and leaves no file, and a request sent mid-job waits its turn.
- **Cache key (§3.1).** `<cache root>/denoise_cache/<content hash>.<model id>.t512o32.png`: the model and the tile parameters are in the name. A render takes an entry only if it decodes and has **exactly the decoded source's pixel size**; otherwise it behaves as "not denoised yet". Source hash = blake3 of the file bytes, the preview cache's own.
- **Pipeline slot (§3.2).** `render_full_resolution` applies the blend right after decode and the hidden-panel strip, before lens correction. `ai_denoise` belongs to the **Noise Reduction** panel (`PANEL_OP_NAMES` in Rust; the JS mirror and the GPU preview follow in 3c), so hiding that panel switches it off. The stage reports `Off / Applied / Missing` for the export pre-flight of §3.6 (3d).
- **Print is not wired yet.** `print.rs` calls the render with no cache directory, so Print ignores `ai_denoise`; its raster cache key is the stack hash and would go stale when a result appears later. Slice 3d fixes both together. Nothing in the UI can create the op before 3c.
- **The model file has no download yet.** The app needs `nafnet_sidd_w32.onnx` (117,316,644 bytes, SHA-256 `5ff07228...b133d`, pinned in `denoise_models.rs`), which is **our own conversion** of MIT-licensed upstream weights; there is no published place to fetch it from. Publishing it (a GitHub release asset or a Hugging Face repo, with the MIT notice and the export script) is the user's decision and is **not done**; until then the model installs through the import path (`denoise_import_model`), which verifies name, size and checksum. Open question 5 below.
- **Measured through the real helper** (M1 Pro, release build, NAFNet-SIDD-w32, a 3840x2544 ISO 6400 photo, 48 tiles): **42.5 s, 886 ms per tile** (the Python figure was 840 ms), **peak RSS 796 MB** for the whole test process (decode, accumulators, session, plus a second cancelled run). Extrapolated to 24 MP (117 tiles): about 104 s, and the accumulators grow by about 0.4 GB, so about 1.2 GB. By eye the result has no visible seam and matches the Python one. `with_memory_pattern(false)` was kept without its own A/B (the SAM 2 finding); not measured on this model.
- **Still open:** the memory-arena question above, Windows (the CI integration test runs there; the real model does not), and the download location. New open question 5: **where is `nafnet_sidd_w32.onnx` published?** Recommendation: a release asset on this repo, because the hash and licence notice travel with the code that pins them.
