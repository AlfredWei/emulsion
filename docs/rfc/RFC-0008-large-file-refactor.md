# RFC-0008: Splitting the four oversized source files

- Status: Draft — plan for review; no code changes in this PR
- Date: 2026-09-19
- Companion documents: [PROJECT_STRUCTURE.md](../PROJECT_STRUCTURE.md), [RFC-0002](RFC-0002-develop-gpu-cpu-fallback.md) (CPU/GPU parity), [PROGRESS.md](../../PROGRESS.md)

## 1. Problem

Four files exceed 4,000 lines, which makes them hard to read, review, and merge without conflicts (PROGRESS.md and `+page.svelte` already collide constantly):

| File | Lines | What it is |
|---|---|---|
| `app/src/lib/components/DevelopCanvas.svelte` | 5,634 | WebGPU pipeline, ~1,600-line WGSL string, overlays, pointer handling |
| `app/src-tauri/src/develop_engine.rs` | 5,346 | CPU edit engine (~3,050) + inline tests (~2,280) |
| `app/src/routes/+page.svelte` | 4,788 | The app's god component: all modules' state, handlers, markup, styles |
| `app/src-tauri/src/catalog.rs` | 4,012 | One `Catalog` impl across every domain (~2,010) + 77 inline tests (~1,610) |

Goal: a **behavior-preserving** split so no file is over ~1,000 lines (Rust) / ~800 lines (Svelte), organized by domain. Not a rewrite: no logic changes, no renamed public APIs, no new features.

Line numbers below come from a grep-based structural survey and are approximate; each step re-derives exact ranges before moving code.

## 2. Ground rules for every step

1. **Pure moves.** A refactor PR contains moves, visibility changes (`pub(super)`), `mod`/`use` lines, and nothing else. Any bug noticed along the way gets its own PR. `git diff -M` (rename detection) should read as moves.
2. **One small PR per step**, reviewable in isolation; `main` is green after each.
3. **Public paths keep working.** Rust `mod.rs` files re-export what other modules import (`crate::catalog::{Catalog, EditStack, ExportPlugin}` etc.); Svelte callers' props/callbacks are unchanged.
4. **Tests move with their code**, and the count must not change (Rust: 380 passing, 2 ignored at time of writing).
5. **Verification per step:** Rust — `cargo test --lib` and `cargo check`, warning-free. Frontend — `npm run check` (0 errors) and `npm test` (vitest). Rendering-touching steps also run the WebdriverIO specs (§7).
6. **Freeze parallel edits to the file being split** while its steps are in flight, or expect conflicts (§6).

## 3. `catalog.rs` → `catalog/` (do first)

Rust, compiler-checked, and covered by 77 tests: the lowest-risk, highest-relief start.

Layout: `mod.rs` (~150: `Catalog` struct, `open`/`open_in_memory`, pragmas, `pub use` of the public types), `schema.rs` (`migrate` + `add_column_if_missing`), `images.rs` (import/remove/thumbnails/list — the largest, ~450 + tests), `merge_sources.rs`, `culling.rs` (rating/flag/label/caption/copyright/contact + `get_export_metadata`), `geo.rs` (coordinates, maps key, geocode provider), `faces.rs`, `keywords.rs`, `collections.rs`, `backup.rs` (backup settings, cache dir, `perform_backup`), `edit_stack.rs` (edit stack, history, snapshots), `presets.rs` (+ `DEFAULT_PRESETS`), `export_plugins.rs`. Each keeps its own `#[cfg(test)] mod tests`.

Notes: `Catalog` is defined in `mod.rs`, so child modules can read its private `conn`; multiple `impl Catalog` blocks across files are fine. `DEFAULT_PRESETS` becomes `pub(super)` (schema seeding and presets tests both use it). `migrate` keeps its single `execute_batch` — DDL order and foreign keys matter, so do not split it. Two tests reach into `catalog.conn`/`Catalog::migrate`; they stay valid as descendants of the module. Re-export only what other modules use, to avoid unused-import warnings.

PRs: **C1** convert to `catalog/mod.rs` + presets, export plugins, backup. **C2** keywords, collections, faces, edit stack. **C3** geo, culling, merge sources, images, schema (last; they cross domains).

## 4. `develop_engine.rs` → `develop_engine/`

Layout: `mod.rs` (docs + `pub(crate) use` of the 8 items callers use: `apply_edit_stack`, `apply_crop`, `apply_lens_correction`, `apply_perspective`, `Crop`, `crop_op`, `LensCorrection`, `Perspective`), `color.rs` (`luma3`, `smoothstep`, `lerp`, HSL math, `op_value`), `tone.rs` (WB, parametric tone, tone-curve LUT), `hsl_split.rs`, `effects.rs` (vignette, grain), `detail.rs` (sharpen, noise reduction, local contrast, dehaze filters), `masks.rs` (~750, all mask kinds), `pipeline.rs` (`apply_edit_stack`), `crop.rs`, `lens.rs` (~540), `perspective.rs`, and `tests/` split by the existing clusters (with a `support.rs` for shared fixtures).

Notes: `apply_edit_stack` is one ~265-line function that runs eight passes in a specific order; **keep it whole in `pipeline.rs`** — do not split the pass order. Callers elsewhere (`export.rs`, `import.rs`, `preview_cache.rs`) need no change. Performance: there are no `#[inline]` attributes today and rustc inlines small private functions across modules within one crate, but this is the per-pixel hot path — compare export timing before/after, and add `#[inline]` to `luma3`/`smoothstep`/`lerp`/`sample_lut`/per-pixel weight functions only if it regresses. Changing any op's semantics would break CPU/GPU parity (RFC-0002), which this refactor must not touch.

PRs: **D1** `git mv` to `mod.rs`, split tests into `tests/`, extract `color.rs`. **D2** crop, lens, perspective. **D3** tone, hsl_split, effects, detail. **D4** masks, pipeline.

## 5. `DevelopCanvas.svelte`

Sections: props/state/overlay helpers (~330), crop drag (~75), coordinate math (~400), pointer handlers (~430), ~120 GPU handle declarations (~340), **`WGSL` string (~1,620)**, `initGpu` (~280), `applyBitmapToGpu` (~460), loading (~120), brush rasterization (~200), `writeAdjustmentsAndRender` (~340, the per-frame hot path), markup (~385, mask overlay ~260), styles (~445).

Plan: **V1** — move the WGSL into `lib/gpu/shaders/*.js` string modules (`common`, `grade`, `lens`, `perspective`, `dehaze`, `blur`, `premask`, `mask`, `original`) concatenated in the same order in an `index.js`. Vite has no `.wgsl?raw` set up; plain JS string modules need no config and keep the text byte-identical. Add a test asserting the concatenation equals the old literal exactly. `routes/m1-slice3-smoke/+page.svelte` holds a copy-pasted "verbatim" WGSL (probably a subset) — check before pointing it at the shared module. **V2** — pure coordinate/dab math into `lib/canvasMath.js` (next to `cropMath.js`, with vitest cases) and uniform packing into `lib/gpu/uniforms.js` (reuse the existing pack functions in `lib/api/develop.js`). **V3** — brush rasterization, histogram, pipeline and texture setup into `lib/gpu/*`, replacing the ~120 loose `let` GPU handles with one `gpu` object. **V4** — mask overlay / crop overlay markup into child components with their styles.

Constraints: `$state` reads inside `writeAdjustmentsAndRender` drive the re-render effect, so those reads stay in the component or a `.svelte.js` file; `bind:this` refs (`canvasEl`, `wrapEl`, `overlayEl`) are passed as arguments; the only consumer is `+page.svelte` (~55 props, 8+ callbacks) and that surface does not change; the `.crop-clip` element must not become conditionally rendered (re-creating it destroys the WebGPU context — the file's own comment says so); no new per-frame allocations.

Unchecked in the survey and to verify in V1/V3: where `MAX_MASKS` is defined, and where `window.__developRenderPerf` (read by the performance e2e) is set.

## 6. `+page.svelte` (do last; needs its own design first)

Sections: script ~3,640 lines (≈360 top-level declarations, 5 `$effect`s, one `onMount`), markup ~820 (titlebar, dialogs, status strip, Library ~225, Develop ~210, Print ~50), styles ~320. Nearly every region already has a component (`CatalogRail`, `LibraryGrid`, `DevelopPanel`, …); what remains in the page is glue, handlers, and derived state.

Hard part: heavily shared state — `images`, `selectedId`, `selectedIds`, `filteredImages`, `statusMessage`, `activeModule`, `editStack` are each read/written from many domains. Svelte 5 cannot export a reassigned `$state` primitive from a module, so shared state lives in `.svelte.js` modules exposing an object/class with `$state` fields (module singletons are fine in a single-window app). `statusMessage` becomes a `notify(msg)` function. Several timers (`persistTimer`, `previewDebounceTimer`, `softProofTimer`, `gpuFallbackTimer`) must move with their owners or flush ordering changes. `openDevelop`/`switchModule` reset state across domains and become an orchestrator calling each module's `reset()`. The keyboard and menu handlers reach nearly every domain and take a context object of callbacks.

Plan: **P1** stateless markup shells → `AppTitlebar`, `AppDialogs`, `StatusStrip`. **P2** pure logic → `libraryFilters.js` (filter/sort), `menuActions.js`, `keyboard.js` factory, with vitest cases. **P3** domain runes modules: `print`, `faces`, `presets`, `softProof`, `masks`. **P4** `importFlow`, `collectionsActions`, `selection`. **P5** `develop.svelte.js` (edit stack, history, flush, undo/redo) + `developAdjustments` + `appEvents` (the `onMount` listeners). **P6** `LibraryModule`, `DevelopModule`, `PrintModule` components — riskiest, because scoped CSS moves with the markup and e2e selectors depend on DOM structure. **P3 onward needs a short state-store design agreed before coding** (which modules own which state, the `reset()` contract); P1–P2 do not.

## 7. Verification beyond unit tests

Frontend logic has thinner automated coverage than the Rust core, and the e2e specs need a debug Tauri build (`npm run test:e2e`). Specs most sensitive to this work:

- Rendering (V-steps, and D-steps at the end): `develop-cpu-gpu-parity`, `develop-gpu-fallback`, `develop-performance`, `golden-path`.
- DOM structure (P6): `panel-resize` depends on `.develop-body > .history-rail` and `.develop-body > .panel` staying direct children.
- Import/status (P4): `import-progress`, `thumbnail-backfill`, `hdr-merge` (`.status`). Settings dialog (P1): `storage-settings`.

CI runs these on macOS and Windows on every PR, so each PR gets them for free; run the relevant ones locally only when iterating on a risky step.

## 8. Sequencing and order

Recommended: **catalog → develop_engine → DevelopCanvas (V1–V4) → `+page.svelte`**, ~15 small PRs. Rust first because the compiler and 380 tests catch mistakes; `+page.svelte` last because it is riskiest and benefits from the other three being done.

Interaction with feature work: M5.5's next slices (map view, pin-drop, reverse geocode) will touch `+page.svelte`, `MetadataPanel`, and `catalog.rs`. Suggested: land PR #135, do the two Rust splits (C1–C3, D1–D4) — quick and low-risk — before the map view slice, and then interleave the Svelte splits, freezing feature edits to a file only while its own steps are in flight.

## 9. Decisions for review

1. **Size thresholds** (~1,000 Rust / ~800 Svelte): acceptable?
2. **Order and interleaving** with the map-view work as in §8, or pause features until the refactor is done?
3. **Second tier**: `lib.rs` (2,203), `lib/api/develop.js` (1,726), `DevelopPanel.svelte` (1,607), `MetadataPanel.svelte` (1,544) are also large but under 4k. Out of scope here; want a follow-up plan after the four, or leave them?

## 10. Non-goals and risks

- No behavior, API, schema, or shader changes; no dependency changes.
- Risk: a mechanical move silently changing behavior (Svelte reactivity is the main way — reads moving out of an effect's tracking scope). Mitigation: pure-move rule, small PRs, exact-string check for the shaders, e2e in CI.
- Risk: performance regression on the CPU hot path or GPU frame loop. Mitigation: before/after timing on the affected steps; `#[inline]` only if measured.
