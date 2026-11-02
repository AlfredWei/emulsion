# ADR-0010: Object removal: LaMa, bundled in the installer, patches stored in the catalog

- Status: **Accepted** by the product owner on 2026-11-02 (engine and licence, bundling); the patch-storage part is the design RFC-0032 §10 favoured, adopted here as the default (see "Not settled")
- Date: 2026-11-02
- Relates to: [RFC-0032](../rfc/RFC-0032-ai-object-removal.md) (the bake-off, §9, and the measurement of §10), [ADR-0009](ADR-0009-ml-inference-runtime.md) (ONNX Runtime), [RFC-0033](../rfc/RFC-0033-adaptive-tone-presets.md) §10 and `models/bundled-models.json` (the bundling mechanism), [ADR-0006](ADR-0006-edit-representation.md) (the edit stack), [ADR-0005](ADR-0005-catalog-storage.md)

## Context

M7 asks for a fill approach documented before implementation. RFC-0032's bake-off (60 hole cases, 20 photographs) found LaMa closest to the original in 46 of 60, the only engine that continued structure, and one that did not invent objects; MI-GAN invented objects above small holes; the non-model baseline lost on everything above a small spot. Three questions were left to the owner: is the by-eye ruling right (A), is a model trained on Places2 acceptable (B), bundle or download the 208 MB file (C). The owner answered on 2026-11-02: **accept; bundle**.

## Decision

1. **Engine: LaMa (fp32 ONNX) for removal.** MI-GAN is not used above small holes. Large holes get a guard in the UI (RFC-0032 §3.4), not a promise. The existing clone / heal stays as the manual tool and the "Use Heal instead" answer. No diffusion or prompted fill in M7 (slice G stays unscheduled).
2. **Licence accepted by the owner.** LaMa's code and the `big-lama` weights are Apache-2.0 (the ONNX port's card and the original's say so; the upstream `LICENSE` text was not retrievable during the bake-off). The model was **trained on Places2, whose terms were not read** (the page was unreachable; from memory, unverified, it is a research dataset of third-party photographs). The owner accepts that risk for this product, as the sky model's CC BY training data was accepted. What this ADR does **not** claim is that Places2 permits commercial use of derived weights; it records that the owner decided to ship regardless. Slice D adds the model's own page, `models/INPAINT_MODEL.md` (source, revision, SHA-256, licence text, training-data note), like `DETECT_MODEL.md`, and the licence text is shown with the other bundled models' notices.
3. **Bundled, not downloaded.** The file ships inside the installer through the existing mechanism (`models/bundled-models.json` entry, `app/scripts/fetch-bundled-models.mjs`, seeded on first launch and verified by SHA-256 in `bundled_models.rs`), so Remove works with no download and no consent step. File: `lama_fp32.onnx`, 208,044,816 bytes, SHA-256 `1faef5301d78db7dda502fe59966957ec4b79dd64e16f03ed96913c7a4eb68d6` (Hugging Face `Carve/LaMa-ONNX`; the exact revision is pinned when the manifest entry is written). **Installer growth: bundled models go from about 356 MB (SAM 2 Tiny 155 MB, NAFNet denoise 117 MB, D-FINE 79 MB, Real-ESRGAN 4.9 MB) to about 564 MB.** The entry is added with the engine (slice D), not now, so no release carries 208 MB the app cannot use yet. Hosting: the denoise and detector files are release assets of this repository; LaMa's first manifest entry may point at the pinned Hugging Face revision, and mirroring it as a release asset of this repository is a publication the owner is asked about when the entry is written.
4. **The removal is an edit, and its patch is stored inside the catalog database.** One `remove` op per removal (mask, engine and version, patch id), applied first in the pipeline, per-photo (not copied by presets or Auto Sync), as RFC-0032 §3.2. **Where the patch lives is decided here:** a table of PNG blobs (region plus context, feathered alpha) keyed by patch id, in the catalog, deleted with its op. Reason, found while reading the code: **the catalog backup (PRD §7.6) is one SQLite file made with SQLite's backup API; nothing next to the catalog is in it**, and the cache folder the user may relocate (Settings ▸ Storage) is for regenerable data (thumbnails, previews, denoised copies that are evicted past 2 GB). A patch in a folder would therefore not be backed up, contradicting RFC-0032 §3.2's "backed up with it", and a copied catalog would lose its removals. As a blob it travels with every backup, restore and catalog copy, costs a few hundred KB per removal (not measured in the app), and the row is read only when its photograph is opened.
5. **The saved result is the truth; nothing re-runs on open.** The patch is also *recomputable*: if a blob is missing the op shows "needs regenerating" with one button that re-runs LaMa from the stored mask, and says so when the result is not guaranteed identical (RFC-0032 §10: bit-identical on one machine and provider, texture-level differences across providers). Never silently.
6. **CPU provider** for LaMa: CoreML was far slower in the bake-off (§9.2) and ended a run without a result. 2.3 s per removal on the M1 Pro CPU; Windows (CPU, DirectML) is unmeasured.

## Consequences

- The installer is about 208 MB larger; downloads and updates grow with it. A smaller file (fp16 or int8) is a later option, **not measured**; a quality loss would have to be judged by eye like the bake-off.
- The catalog grows with removals (a blob each), and a catalog backup does too; `VACUUM` in the backup's optimise step already rewrites it.
- Slice D builds against this: `inpaint_run` helper op (LaMa, crop and blend, detail transfer for the soft large holes), the `remove` op and the patch table (a catalog migration), CPU and GPU compositing of a stored image. Slice E is the UI. Per the M5 practice, slice D (GPU compositing) gets its RFC section first and this ADR is updated after.
- The by-eye ruling covered the bake-off's random blobs on 20 web photographs. **Real removals, the user's own photographs, faces and text, and Windows are not verified** (RFC-0032 §7); slice F is where the bake-off's weak spots (soft large fills, halos) are worked.

## Not settled

- The small study with people (RFC-0032 slice C2, appendix protocol) was not run. Items 4 and 5 adopt the design its measured part (§10) favoured, so slice D proceeds; the study can still change the "needs regenerating" wording or the default before the UI (slice E) ships.
- Places2's terms remain unread (item 2). If the owner later wants them read, this ADR is where the answer is recorded; a different, differently trained model would replace LaMa behind the same op.

## Alternatives considered

- **Download on first use** (the SAM 2 and denoise pattern before bundling): rejected by the owner for the sake of a first run that needs no network and no consent step.
- **MI-GAN (28 MB) as the engine**: rejected, it invented objects on medium and large holes (RFC-0032 §9.3).
- **Patches as files beside the catalog or in the cache folder**: rejected (item 4): not in the backup, and the cache may be relocated or evicted.
- **Regenerate on open, no stored patch**: rejected as the default: 2.3 s per removal on every open, and it is not identical across providers; it remains the fallback for a missing blob.
- **PatchMatch alone** (no model): lost by a wide margin above small spots in the bake-off, though the implementation there was a first one (RFC-0032 §9.3).
