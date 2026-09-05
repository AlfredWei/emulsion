# RFC-0003: HDR merge (radiometric bracket merge + auto-alignment)

- Status: Draft — for review in PR (flips to Accepted once merged)
- Date: 2026-09-05
- Companion documents: [ADR-0003](../adr/ADR-0003-raw-decoding.md), [ADR-0005](../adr/ADR-0005-catalog-storage.md), [ADR-0006](../adr/ADR-0006-edit-representation.md), [MILESTONES](../../PRD/MILESTONES.md#m5--performance-gpu-merges-faces), [PROGRESS.md](../../PROGRESS.md)

## 1. Problem

M5's scope names *"HDR merge (multi-exposure → single high-bit-depth composite)"* as its own line item, separate from the GPU-rendering track (Slices 1–3, all shipped). Today there is no path from "N bracketed RAW exposures of one scene" to "one cataloged image" at all — every existing multi-file operation in this codebase (`export_batch`, `print::export_pdf`) is a one-way, non-cataloged *output* (§3.1 confirms no exception exists), and every pixel buffer anywhere in the Rust core is 8-bit, non-linear, display-referred (`raw_decode.rs`'s own header: *"everything here is 8-bit, not color-managed"*).

Two scope forks were resolved with the user before design work started (2026-09-05):

1. **True radiometric merge, not Exposure Fusion.** The cheaper LDR-space alternative (Mertens et al. 2007 — blend brackets directly via contrast/saturation/well-exposedness weights, no linear buffer needed) was raised and explicitly declined in favor of a real radiance-domain merge + tone-map, matching what "HDR merge" means in MILESTONES.md and in Lightroom's own feature of the same name.
2. **Basic auto-alignment is in scope**, not deferred to a follow-up — brackets are not assumed pre-aligned/tripod-only.

Both were real forks with materially different implementation cost; this RFC is scoped to what was actually chosen, not the cheaper path.

## 2. Non-goals

- **Camera-response-curve estimation (Debevec–Malik style).** That technique exists to recover an *unknown* nonlinear response function from JPEG-like inputs where the sensor's own linear data isn't reachable. This app's bracket inputs are RAW files, and LibRaw can decode a RAW file's sensel data in genuinely linear form directly (§3.1) — response-curve estimation would be solving a problem that doesn't exist for this input class, at real implementation and numerical-stability cost (an ill-conditioned smoothness-regularized least-squares solve). **Consequence: HDR merge v1 requires every selected bracket member to be a RAW file.** A JPEG-only bracket is rejected with a clear error, not silently degraded — JPEG bracket support would need response-curve estimation as new, separate scope, not a small addition to this one.
- **Full geometric alignment (rotation, perspective, homography).** The chosen alignment method (§3.3) is Ward's Median Threshold Bitmap technique — hierarchical, whole-image **translation only**. A handheld bracket with rotation between frames (e.g. rotating the camera, not just hand-shake) will still show residual misalignment/ghosting at the edges of moving/high-parallax content. Named explicitly, not silently accepted: this is the real capability/cost line the user's own "basic... not full homography" framing drew.
- **Ghost/moving-object removal.** A person or leaves moving between exposures will blend into a ghost or soft blur wherever alignment can't fully reconcile them (translation-only alignment doesn't fix a *locally* moving subject at all, even with perfect global alignment). Real ghost removal is its own well-known, separate CV problem (per-pixel exposure-validity detection) — out of scope here.
- **Local/detail-preserving tone mapping** (bilateral/Durand-style). §3.5 uses a global Reinhard operator. A local operator produces punchier, more "HDR-look" output but is a separate, larger algorithm with its own artifact class (halos) to get right — a real, named quality ceiling for v1, not a silent gap.
- **16-bit/wide-gamut output surviving into the catalog.** §3.6 explains why the merge's own internal math is 32-bit float/scene-linear throughout, but the *cataloged* result is still an 8-bit JPEG — the same output format every existing Rust-core output path already uses. Building a 16-bit-aware `develop_engine.rs`/`DevelopCanvas.svelte`/import/thumbnail pipeline is real, substantial, separately-scoped work this slice does not also take on as a side effect.
- **A merge-preview/adjustment dialog** (choosing which frame is the alignment reference, previewing before committing, adjusting per-frame weights). v1 runs once the user confirms a selection and reports success/failure via the existing status-message convention (`handleApplyPresetToSelection`'s own bar) — no new dialog chrome.

## 3. Design

### 3.1 Linear decode: extend the RAW pipeline, not invent a response-curve solver

LibRaw's raw sensel data is physically linear in scene radiance (modulo black-level subtraction and white-balance gain, both of which LibRaw itself already applies during its normal demosaic/postprocess step) — a RAW file does not have the "unknown nonlinear response" problem a JPEG does. LibRaw exposes this directly via three `libraw_output_params_t` fields (confirmed against the vendored header, `vendor/rsraw-sys/LibRaw/libraw/libraw_types.h:889-916`): `gamm[6]` (gamma curve — `[1.0, 1.0, ...]` means no curve applied, i.e. linear output), `no_auto_bright` (disables LibRaw's own per-image auto-exposure compensation, which would otherwise defeat this feature's own controlled EV-based scaling by re-normalizing each bracket member independently), and the already-used `output_bps` (`raw.rs:212-214`, already supports `BIT_DEPTH_16`). This is the same "linear 16-bit, no auto-bright, fixed WB" recipe every RAW-based HDR tool already uses (equivalent to `dcraw -4 -g 1 1 -W`, or `rawpy`'s `postprocess(gamma=(1,1), no_auto_bright=True, output_bps=16)`), not a novel technique.

**Blocker and its fix**: `rsraw::RawImage.raw_data` (the `*mut sys::libraw_data_t` pointer these params live behind) is a **private field** — only methods defined inside the `rsraw` crate itself (like the existing `set_use_camera_wb`/`set_use_camera_matrix`, `raw.rs:199-210`) can reach it. `rsraw` itself is a plain crates.io dependency today (only its transitive `rsraw-sys` is vendored/patched, per `vendor/rsraw-sys/PATCH.md`). **This RFC extends the vendoring story**: fork `rsraw` itself into `vendor/rsraw/` (mirroring the existing `rsraw-sys` vendoring — same MIT-licensed upstream, same "copied here, not written from scratch" framing), adding one new method to `raw.rs`:

```rust
/// Configures the decode for linear-light, non-auto-brightened output —
/// the "raw scene radiance" mode HDR merge needs, as opposed to the
/// display-referred, auto-exposed 8-bit output every other decode path
/// in this app uses. Must be called before `process::<BIT_DEPTH_16>()`.
pub fn set_linear_output(&mut self) {
    unsafe {
        (*self.raw_data).params.gamm = [1.0, 1.0, 0.0, 0.0, 0.0, 0.0];
        (*self.raw_data).params.no_auto_bright = 1;
    }
}
```

`Cargo.toml`'s existing `[patch.crates-io]` entry (currently only `rsraw-sys`) gains a second entry for `rsraw` pointing at the new vendor directory. ADR-0003 gets a dated update recording this (§5).

`raw_decode.rs` gains `decode_linear(path: &Path) -> Result<DecodedLinear>`, alongside the existing `decode_preview`/`decode_develop_preview` (both untouched — every non-HDR-merge caller keeps using 8-bit display-referred decode exactly as today):

```rust
pub struct DecodedLinear {
    pub width: u32,
    pub height: u32,
    /// Interleaved RGB, one f32 per channel, normalized by 65535.0 from
    /// LibRaw's linear 16-bit output. NOT clamped to [0,1] -- a
    /// well-exposed bright frame can legitimately read above 1.0 before
    /// this value is scaled by its own frame's exposure ratio (see
    /// hdr_merge.rs); clamping here would silently discard real highlight
    /// data this whole feature exists to preserve.
    pub rgb: Vec<f32>,
}
```

Applies camera (not auto) white balance (`set_use_camera_wb(true)`, already exposed) before calling the new `set_linear_output()` and `process::<BIT_DEPTH_16>()` — a fixed WB across all bracket members keeps color consistent frame-to-frame, which matters for a correct blend (auto-WB could legitimately pick a different white point for a much-brighter or much-darker exposure of the same scene).

### 3.2 Exposure value: from already-cataloged EXIF, with a named gap

Each bracket member's absolute exposure is computed from the standard photographic EV formula, `EV = log2(aperture² / shutter_speed) - log2(ISO / 100)`, reading `images.aperture` / `images.shutter_speed` / `images.iso` — **already captured into the catalog at import time** (`metadata.rs`, no new EXIF extraction needed, confirmed via the M5 slice-2/3-era research). A pure `compute_ev(iso, aperture, shutter_speed) -> Option<f32>` helper (new, in `hdr_merge.rs`) returns `None` if any input is missing/non-positive.

**Named gap, not silently assumed away**: ADR-0003 already documents a confirmed EXIF-field drift on Windows (vcpkg-linked LibRaw 0.22.1) where `shutter`/`iso`/`aperture` can come back `None` for at least one real linear-DNG sample. `merge_hdr_bracket` rejects the whole operation with a clear "couldn't read exposure info for {path}" error if any selected frame's EV can't be computed, rather than guessing a default EV that would silently corrupt the radiometric scaling every other frame depends on being correct.

### 3.3 Alignment: hierarchical Median Threshold Bitmap (Ward 2003)

Per-frame global (x, y) pixel-offset alignment, computed on a luminance proxy (`0.2126R + 0.7152G + 0.0722B`, standard Rec.709 luma weights — this is a texture/contrast metric here, not a color-managed operation, so exact primaries don't matter) derived from each frame's **own** `DecodedLinear` buffer:

1. **Median Threshold Bitmap**: threshold each luminance image at its own median value → a 1-bit-per-pixel bitmap. Thresholding at each image's *own* median (not a shared absolute threshold) is what makes this comparable across different exposures without needing the EV-scaling from §3.2 applied first — the whole point of Ward's technique.
2. **Exclusion bitmap**: pixels within a small band of the median (e.g. ±4/255-equivalent) are excluded from the comparison — these are the pixels most likely to flip bits from sensor noise alone, which would otherwise inject false mismatches into the alignment search.
3. **Image pyramid**: build a small stack of half-resolution downscales (reusing `image::imageops::resize`, `FilterType::Triangle` — matches the resize filter already used elsewhere in this codebase, e.g. `preview_cache.rs`) of the luminance buffer, coarsest last.
4. **Hierarchical search**: at the coarsest level, exhaustively search a small offset window (e.g. ±4px) around (0,0), scoring each candidate by counting `(bitmap_a XOR bitmap_b) AND NOT (exclusion_a OR exclusion_b)` set bits (fewer mismatches = better alignment) between the reference frame and the candidate frame. The winning offset is doubled and used as the search center for the next-finer level's own small window, refining down to full resolution. This is the standard MTB pyramid search (Ward, *"Fast, Robust Image Registration for Compositing High Dynamic Range Photographs from Hand-Held Exposures,"* 2003) — a well-established published technique, not a novel algorithm being designed from scratch here.

The bracket member with EV closest to the group's median EV is chosen as the alignment reference (every other frame aligns to it) — an arbitrary-but-reasonable choice (avoids picking either extreme, which tend to have the least reliable midtone detail to align against).

Output: `Vec<(i32, i32)>`, one `(dx, dy)` per input frame (the reference frame's own offset is always `(0, 0)`).

### 3.4 Radiometric merge

For each output pixel, combine the aligned, EV-scaled linear radiance from every frame using a Debevec-style weighted average — the same weighting concept ADR-0003/0004's own deferred linear-pipeline notes have anticipated, applied here for the first time:

```
weight(z) = triangle function peaking at z = 0.5, zero at z <= 0.0 or z >= 1.0
radiance_i(x, y) = decoded_linear_i(x - dx_i, y - dy_i) * 2^(-(ev_i - ev_ref))
merged(x, y) = Σ_i [ weight(decoded_linear_i(x-dx_i, y-dy_i)) * radiance_i(x, y) ]
               ─────────────────────────────────────────────────────────────
               Σ_i [ weight(decoded_linear_i(x-dx_i, y-dy_i)) ]
```

The weight is evaluated on the **original, unscaled** decoded value (before EV-scaling) — this is what makes it a genuine "how reliable is this frame's data at this pixel" signal: a pixel near black in its own source frame carries little real signal above sensor noise floor regardless of that frame's EV, and a pixel near the sensor's own clipping ceiling carries no real information at all, in either case independent of how bright or dark that frame happens to be overall. An aligned-offset sample that falls outside a frame's bounds is excluded from that pixel's sum (frames don't need to fully overlap after alignment, just their shared region). If every frame's weight is zero at a given pixel (a genuinely all-clipped-or-all-black column across every bracket member, or an out-of-bounds edge strip after alignment where a majority of frames don't overlap), fall back to the single frame with weight closest to its own peak.

### 3.5 Tone mapping: global Reinhard

The merged linear radiance buffer has no fixed display range (unlike every other buffer in this codebase, which is already 0–255/0–1). A global Reinhard operator (`L_out = L_in / (1 + L_in)`, applied per-channel — simple, well-understood, and correctly monotonic/bounded to [0, 1) for any non-negative input, which a naive linear rescale-by-max would not guarantee is *perceptually* reasonable) compresses it back to a displayable 0–1 range, then scaled to 0–255 and written into a plain `image::RgbImage` — from this point on, a merged HDR result is indistinguishable in representation from any other cataloged photo.

### 3.6 Output & catalog integration: a genuinely new pattern, not a reused one

Confirmed during research: **no existing code path writes a derived/synthetic file and then feeds it back into the catalog as a new image** — `export.rs` and `print.rs` both write one-way, non-cataloged output files. This RFC introduces that pattern for the first time, reusing as much existing catalog machinery as it can:

- New `merges_dir` app-managed directory (mirrors the existing `thumbnails_dir`/`previews_dir` convention — same app-data-relative resolution, not a user-chosen location, since this is an internally-generated derived file, not an import).
- `hdr_merge::merge_bracket(frames: &[BracketInput]) -> Result<MergedImage>` orchestrates §3.1–3.5 and returns the final `image::RgbImage` plus the per-frame alignment/EV metadata used.
- The merged image is JPEG-encoded (matching every other output path's format — `export.rs`'s own encoder) to `merges_dir`, then blake3-hashed (matching `import.rs`'s existing content-hash convention exactly) — this new file is now, from the catalog's perspective, exactly as real as an imported photo.
- **New `hdr_merge_sources` table** (migration in `catalog.rs`, alongside the existing schema): `(result_image_id INTEGER, source_image_id INTEGER, ordinal INTEGER, ev_offset REAL, dx INTEGER, dy INTEGER, PRIMARY KEY (result_image_id, source_image_id))` — pure provenance (which originals fed a merge, in what order, with what computed alignment/EV), not consulted by any render path. This is new modeling, not a repurposing of `images.stack_id` (confirmed unused/unimplemented anywhere) or `image_versions.is_virtual_copy` (confirmed to mean "multiple edit stacks over one file," the opposite relationship from "one file derived from many").
- `Catalog::add_image_with_edit_stack` (`catalog.rs:707-757`, the exact function `import.rs` already uses for both RAW and JPEG) catalogs the new file unchanged — no new catalog-insert code path, just a new caller.
- New Tauri command `merge_hdr_bracket(image_ids: Vec<i64>) -> Result<i64, String>` (`spawn_blocking`, matching every other CPU-heavy command's convention): resolves each id's path/EXIF from the catalog, validates (≥2 images, all RAW, all with computable EV — §3.2's named gap), runs the merge, catalogs the result, records provenance, returns the new `image_id`.

### 3.7 Frontend

A new "Merge to HDR…" action in the Library titlebar, enabled at `selectedIds.size >= 2` (mirrors the existing enablement pattern for Paste-Settings-to-Selection etc.) — no new dialog: clicking it immediately calls `mergeHdrBracket([...selectedImageIds])` behind an `mergingHdr` in-flight boolean (matching `applyingPreset`'s own bar), reports the outcome via the existing `statusMessage` convention, and on success refreshes the Library grid and scrolls to/selects the new merged image. A client-side pre-check (`selectedImages.every(isRawPath)`) gives an immediate, cheap rejection before the round trip for the obvious "you selected a JPEG" case — the Rust command's own validation (§3.6) stays authoritative, this is purely a faster failure path.

## 4. Testability

- **Rust unit tests** (`hdr_merge.rs`), all against small synthetic `DecodedLinear` buffers — no real RAW file needed for the algorithm's own correctness:
  - `compute_ev`: known camera-settings combinations against hand-computed expected EVs; `None` on a zero/missing input.
  - MTB alignment: a synthetic luminance pattern (e.g. a checkerboard or a single bright square) shifted by a known (dx, dy) recovers that exact offset; a pyramid level's exclusion-bitmap logic correctly drops near-median pixels from the mismatch count.
  - Weight function: peaks at 0.5, is exactly zero at/beyond 0.0 and 1.0.
  - Merge: two synthetic solid-color frames at known, different EVs and known weights merge to the hand-computed expected radiance at a pixel with no alignment offset; an out-of-bounds sample after a nonzero offset is correctly excluded rather than reading garbage/wrapping.
  - Tone mapping: monotonic in input, output always strictly within [0, 1).
- **Real-RAW-sample end-to-end test**, `#[ignore]`-gated behind a new `EMULSION_TEST_HDR_BRACKET_DIR` env var (pointing at a small local directory of 2-3 real bracketed RAW files, not committed — same "large, third-party provenance, fetched in CI only" precedent ADR-0003 already established for its own single-RAW-sample test) — asserts the full pipeline produces a real, valid JPEG with plausible dimensions and that the catalog gained exactly one new `images` row plus the correct number of `hdr_merge_sources` rows. CI's `e2e` workflow step gains a bracket download alongside its existing single-RAW-sample download, sourced from the same CC0 `raw.pixls.us` precedent, if a suitable public bracket set can be found there; if not, this test is documented as locally-verified-only (matching this project's own precedent for gaps a CI environment genuinely can't cover, e.g. Windows-hardware-only checks) rather than silently skipped with no record.
- **No new e2e (WebdriverIO) spec** is planned for the full click-through flow given the real-RAW-sample dependency above already covers the Rust-side correctness that actually matters here (radiometric math, alignment, catalog integration); the frontend action itself (§3.7) is a thin, low-risk wrapper in the same shape as already-e2e-covered actions (Apply Preset, Batch Apply) — verified interactively against a real built `.app` instead, matching this project's own established bar for low-risk UI glue.

## 5. ADR updates required once this ships

- **ADR-0003** (raw decoding): dated update recording the new `vendor/rsraw/` fork (extending its existing `vendor/rsraw-sys/` vendoring story to the safe wrapper crate too) and the new `decode_linear`/`set_linear_output` capability.
- **ADR-0006** (edit representation): dated update stating explicitly that a merge's output is decode-equivalent — a new baked buffer with its own future edit stack — not itself expressible as an `op` inside any single stack, and that `hdr_merge_sources` is a provenance-only table, never consulted by the render/replay path ADR-0006 otherwise governs.
