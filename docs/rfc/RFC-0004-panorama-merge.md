# RFC-0004: Panorama merge (feature-based homography stitch)

- Status: Draft — for review in PR (flips to Accepted once merged)
- Date: 2026-09-06
- Companion documents: [ADR-0006](../adr/ADR-0006-edit-representation.md), [RFC-0003](RFC-0003-hdr-merge.md), [MILESTONES](../../PRD/MILESTONES.md#m5--performance-gpu-merges-faces), [PROGRESS.md](../../PROGRESS.md)

## 1. Problem

M5's scope names *"Panorama merge (multi-shot → stitched composite), including boundary/edge correction"* as its own line item, alongside HDR merge (RFC-0003, shipped) and the GPU-rendering track (Slices 1–3, shipped). Like HDR merge before it, there is today no path from "N overlapping photos of a wider scene" to "one cataloged image" — this RFC is the second instance of the pattern RFC-0003 §3.6 introduced (write a derived file, catalog it like an import, record provenance), reusing that machinery rather than inventing a new one.

Unlike HDR merge, panorama stitching is a **geometric** alignment problem, not a radiometric one: frames don't need linear decode, EV, or RAW-only input — any decoded `RgbImage` works, matching every other non-merge code path in this codebase. The new work is entirely in estimating and applying **per-frame projective transforms** (homographies) so that overlapping content from adjacent frames lines up on one shared canvas.

## 2. Non-goals

- **Multi-row / 2D panorama grids.** v1 assumes the user selected a single left-to-right (or right-to-left) sweep — one row. A grid (multiple rows, needing 2D graph stitching and bundle adjustment across both axes) is real, separate, larger scope.
- **Cylindrical/spherical projection.** v1 warps every frame with a plain **planar homography** into one reference frame's coordinate system. This is geometrically exact for a bounded field of view but the projected scale/shear grows toward the edges of a very wide (many-frame, near-180°+) sweep — the same "named quality ceiling, not silently accepted" framing RFC-0003 §2 used for its own translation-only alignment. A 2–6 frame sweep of a typical scene (the common case) is not meaningfully affected; a 20-frame full-360° sweep would show real distortion.
- **Automatic pairwise ordering / overlap discovery.** v1 requires the user's selection order to already be the capture order (adjacent selections are assumed to overlap). Full auto-stitchers try all pairs and discover a connectivity graph; that's real, separate scope this RFC does not take on. Named explicitly because it's the most likely real-world footgun: selecting photos out of order will produce a visibly wrong or outright-rejected stitch, not a silently-corrected one.
- **Bundle adjustment / global optimization.** Pairwise homographies are estimated between adjacent frames only and then chained (§3.4) — the same "basic, not full" simplification RFC-0003 drew for alignment. Chaining accumulates drift across many frames; fine for a typical short sweep, degrades slowly beyond it.
- **Rotation/scale-invariant feature descriptors (SIFT/ORB/BRIEF).** §3.2 uses a fixed-orientation normalized-patch descriptor. This assumes adjacent frames differ mostly by translation/perspective from a handheld sweep, not large in-plane rotation or zoom between shots — a real, named limitation, not a silent one.
- **Exposure/color-gain compensation across frames.** Unlike HDR merge, this RFC does no per-frame brightness/white-balance reconciliation before blending — a visible brightness step across a seam if the camera's auto-exposure drifted between shots is a known, accepted v1 gap.
- **Seam-optimized (graph-cut) blending.** §3.5 uses simple per-frame linear (x-axis) feathering, not a content-aware seam search. A moving subject caught mid-overlap can visibly ghost/double, same underlying cause and same accepted-gap framing as RFC-0003 §2's own moving-subject non-goal.
- **A merge-preview/adjustment dialog.** Same as RFC-0003 §2: v1 runs once the user confirms a selection and reports success/failure via the existing status-message convention — no new dialog chrome.
- **Auto-crop of the output.** A stitched canvas is not rectangular in source coverage (see §3.4's canvas-fitting) — uncovered corners/edges are filled black, exactly like Perspective Correction's own already-shipped "no auto-crop, use the existing Crop tool" decision (`develop_engine.rs`'s `apply_perspective`), reusing a precedent this codebase already established rather than inventing a second convention for the same underlying situation.

## 3. Design

Everything below is hand-rolled against `image::RgbImage` and plain `f32` arithmetic — no new external dependency. A pass at using the `imageproc` crate (corner detection + projective warp) was tried and reverted: its transitive dependency tree (`nalgebra`, `glam`, SIMD backends) is a lot of weight for what turns out to be two well-known, compact, directly-testable algorithms (Harris corners, DLT homography via Gaussian elimination) that this codebase's own precedent (RFC-0003's hand-rolled MTB alignment instead of an HDR library) already favors implementing directly.

### 3.1 Feature detection: Harris corners

Standard Harris corner response on a luma buffer (Rec.709 weights, matching RFC-0003 §3.3's own luma convention):

1. Sobel gradients `Ix`, `Iy`.
2. Per-pixel structure-tensor products `Ix²`, `Iy²`, `IxIy`, each box-blurred over a small window (e.g. 3×3) to get local sums `Sxx, Syy, Sxy`.
3. Response `R = det(M) - k·trace(M)² = (Sxx·Syy - Sxy²) - k·(Sxx+Syy)²`, `k ≈ 0.04`.
4. Non-maximum suppression in a window (e.g. 7×7): keep a pixel only if it's the max `R` in its own neighborhood.
5. Keep the top-N strongest survivors (e.g. 500) — bounds matching cost (§3.3) regardless of image resolution.

### 3.2 Descriptor: normalized fixed patch

For each surviving corner (skipping any within half a patch of the image border), extract a fixed-size (e.g. 15×15) grayscale patch and normalize it to zero mean / unit variance (illumination-invariance — frames can legitimately differ slightly in auto-exposure). This is deliberately the simplest viable descriptor, not a rotation-invariant one (§2's named limitation).

### 3.3 Matching: ratio-test nearest neighbor

Brute-force SSD between every corner in frame `i+1` and every corner in frame `i` (bounded by the top-N cap above, so cost is fixed regardless of resolution). Lowe's ratio test (accept only if `best_ssd / second_best_ssd < 0.8`) filters ambiguous matches before they ever reach RANSAC.

### 3.4 Homography estimation: 4-point DLT + RANSAC, chained across frames

For each adjacent pair `(i, i+1)` in the user's selection order, estimate the homography `P[i][i+1]` mapping a point in frame `i+1`'s pixel coordinates to frame `i`'s pixel coordinates (`p_i ≈ P[i][i+1] · p_{i+1}`), from that pair's own matched correspondences:

- **Minimal solve**: 4 correspondences (checked for near-collinearity, retried on failure) give an exactly-determined 8-equation system for the 8 free parameters of `H` (with `h33` fixed to `1` — a legitimate normalization for any homography that isn't itself mapping a real point to infinity, which a realistic panorama-camera rotation never does; named as the one real numerical limitation of skipping a full SVD-based solve). Solved via plain Gaussian elimination with partial pivoting — no linear-algebra crate needed for an 8×8 system.
- **RANSAC**: fixed iteration count (e.g. 1000), each drawing 4 random correspondences, scoring by reprojection-error inlier count (e.g. within 3px) over *all* candidate matches. Keeps the best-scoring `H`.
- **Refit**: once the best inlier set is known, re-solve `H` as a least-squares fit (normal equations, same Gaussian-elimination machinery, now over an overdetermined system) using every inlier — sharper than the minimal 4-point solve alone.
- **Reject fast on a known-bad pair**: if the winning inlier count is below a fixed floor (e.g. 8, or under 40% of candidate matches), the pair is declared non-overlapping/unstitchable and the whole merge fails with a clear error — the same "reject fast on a known-bad input" precedent RFC-0003 §3.2 established for missing EXIF, rather than silently emitting a garbage stitch.

**Chaining to one reference frame**: the *middle* image of the selection (`n/2`) is chosen as the reference (spreads accumulated perspective distortion toward both ends rather than concentrating it at one, a well-known stitching convention) — not the first frame, and not chosen adaptively; a fixed, simple rule. Every other frame's own transform into reference-space is built by composing adjacent pairwise homographies (and inverting them, for frames on the reference's other side) outward from the reference. `PanoramaError::PoorOverlap` surfaces which adjacent pair failed, so a user who selected a genuinely non-overlapping run of photos gets an actionable message rather than a stack trace.

### 3.5 Canvas fitting + blending

Each frame's 4 corners, warped through its own reference-space transform, bound the output canvas (min/max across every frame, translated so the minimum lands at `(0,0)`) — capped at a fixed sanity ceiling (e.g. 4× the sum of input widths/heights) so a degenerate homography can't attempt an unbounded allocation; exceeding it is also a clear, named error rather than an OOM.

For each output pixel, pull-sample every frame whose inverse transform lands the pixel back inside that frame's own bounds (bilinear — reusing the exact `sample_bilinear` convention already established in `develop_engine.rs`'s Perspective Correction and Lens Correction code), weight each contributing frame by a simple triangular ramp over its own normalized x-position (`1 - |2x/(w-1) - 1)|`, zero at the frame's left/right edge, peak at its center) — a deliberately simple linear feather (§2's named non-goal versus a full seam search), reasonable specifically because overlaps in a left-to-right sweep occur near each frame's own left/right edges. Normalize by total weight; a pixel no frame covers is left black, exactly like Perspective Correction's own "no auto-crop, blank corners are black, use Crop" precedent.

### 3.6 Output & catalog integration: same pattern as RFC-0003 §3.6

- New `panoramas` app-managed directory (mirrors `merges`/`thumbnails`/`previews`).
- `panorama_merge::stitch(frames: &[PathBuf]) -> Result<StitchedImage>` — pure pixel pipeline, no catalog dependency, mirroring `hdr_merge::merge_bracket`'s own layering.
- Result JPEG-encoded, blake3-hashed, cataloged via the existing `Catalog::add_image_with_edit_stack` — no new catalog-insert code path.
- **New `panorama_merge_sources` table**: `(result_image_id INTEGER, source_image_id INTEGER, ordinal INTEGER, homography_json TEXT, PRIMARY KEY (result_image_id, source_image_id))` — pure provenance (which originals, in what order, with what final reference-space transform), never consulted by any render path, same framing as `hdr_merge_sources`. Stored as a JSON array of the 9 matrix values rather than 9 separate columns — nothing else in the catalog ever queries into individual matrix cells, so there's no reason to model them as anything but an opaque blob, unlike HDR's `dx`/`dy`/`ev_offset` which the frontend could plausibly want to sort/filter by individually.
- New Tauri command `merge_panorama(image_ids: Vec<i64>) -> Result<i64, String>` (`spawn_blocking`, same shape as `merge_hdr_bracket`): resolves each id's path from the catalog (a new, minimal `Catalog::get_image_path`, since panorama needs no EXIF, unlike `get_image_exposure_info`), runs the stitch, catalogs the result, records provenance.

### 3.7 Frontend

A new "Merge to Panorama…" action beside the existing "Merge to HDR…" button in the Library titlebar (same enablement rule, `selectedIds.size >= 2`, same in-flight boolean / status-message pattern as `handleMergeHdrBracket`) — no new dialog, no RAW-only client pre-check (any format works here).

## 4. Testability

- **Rust unit tests** (`panorama_merge.rs`), all against small synthetic buffers/point sets — no real photos needed for the algorithm's own correctness:
  - Harris response: a synthetic checkerboard/corner pattern produces a strong local maximum at the known corner pixel; a flat/uniform patch produces near-zero response everywhere.
  - Homography DLT: 4 known points passed through a hand-picked homography recover that exact homography (round-trip, within float tolerance); a degenerate (near-collinear) 4-point set is rejected/retried rather than solved into garbage.
  - RANSAC: a synthetic correspondence set that's mostly consistent with one homography plus a few deliberate outlier pairs recovers the inlier-consistent homography, not one skewed by the outliers.
  - Chaining: three synthetic frames with known pairwise homographies compose correctly into the middle frame's reference space (hand-computed expected composed matrix).
  - Canvas fitting + blend: two synthetic solid-half-colored frames with a known overlap and known homography blend to the hand-computed expected color in the overlap region, and to each frame's own solid color outside it.
  - `PanoramaError::PoorOverlap` surfaces for a synthetic pair with deliberately no true correspondence.
- **Real-photo end-to-end test**, `#[ignore]`-gated behind a new `EMULSION_TEST_PANORAMA_DIR` env var (a small local directory of 2–4 real overlapping photos, not committed) — mirrors RFC-0003 §4's own real-RAW-sample test precedent exactly (same "large, third-party provenance, local-only unless a suitable CC0 set is found for CI" framing).
- **No new e2e (WebdriverIO) spec** — same reasoning as RFC-0003 §4: the frontend action is a thin wrapper in the same already-e2e-covered shape as "Merge to HDR…", verified interactively against a real built `.app` instead.

## 5. ADR updates required once this ships

- **ADR-0006** (edit representation): dated update stating a panorama result is decode-equivalent to an HDR merge result — a new baked buffer with its own future edit stack, not itself expressible as an `op`, and that `panorama_merge_sources` is provenance-only, never consulted by the render/replay path, extending the exact statement RFC-0003 already added there for HDR merges.
