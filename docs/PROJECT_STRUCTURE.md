# Project Structure

A map of what lives where in this repo. See [README.md](../README.md) for the project overview and quick start, and [PROGRESS.md](../PROGRESS.md) for current status — this doc is just "what's this file/folder for."

## Top level

```
lr_replace/
├── PRD/                  Product requirements — what to build, in what order
├── docs/                 Engineering docs — how it's built, and why
├── app/                  The actual Tauri + Svelte application
├── .github/workflows/    CI (GitHub Actions)
├── Makefile               make dev / make test / make build, see `make help`
├── README.md               Project overview + quick start
├── PROGRESS.md             Running status log — read this first after time away
└── .gitignore
```

## `PRD/` — product requirements

| File | What it's for |
|---|---|
| [PRD.md](../PRD/PRD.md) | The product requirements document: vision, scope, target user, functional/non-functional requirements. |
| [MILESTONES.md](../PRD/MILESTONES.md) | The 9-milestone roadmap (M0–M8), each with scope, explicitly-deferred items, and exit criteria. |
| [lightroom-reference.md](../PRD/lightroom-reference.md) | Research on Lightroom's actual v1→now feature timeline, used to sequence the roadmap above. |

## `docs/` — engineering docs

```
docs/
├── rfc/                                           Pre-implementation design docs (one per architecture/GPU-heavy slice)
│   ├── RFC-0001-architecture-and-tech-stack.md      The system architecture, ties all the ADRs together
│   ├── RFC-0002 … RFC-0007                          Per-feature designs: develop GPU/CPU fallback, HDR merge, panorama,
│   │                                                 face detection, export-plugin hook, map & geolocation
│   ├── RFC-0008-large-file-refactor.md              Plan for splitting the >1,000-line source files (living status in PROGRESS.md)
│   └── RFC-0009-page-svelte-state-design.md         State/module design for splitting `+page.svelte`
├── adr/                                           One decision per file: context, decision, consequences,
│   ├── ADR-0001-application-shell.md               alternatives considered — several have dated "M0/M1
│   ├── ADR-0002-frontend-ui-stack.md                spike finding" sections added after reality corrected
│   ├── ADR-0003-raw-decoding.md                     the original plan (that's expected, not a mistake —
│   ├── ADR-0004-rendering-and-color-management.md   see each ADR's own update log)
│   ├── ADR-0005-catalog-storage.md
│   ├── ADR-0006-edit-representation.md
│   ├── ADR-0007-face-detection-and-recognition.md
│   └── ADR-0008-plugin-extensibility-api-v0.md
├── ux/
│   ├── UX-DESIGN.md                       Design principles + module layouts for Library/Develop
│   └── mockups/library-develop-mockup.html  Static, reviewed reference mockup (not app code)
└── PROJECT_STRUCTURE.md                   This file
```

Start with RFC-0001 for the big picture, then the individual ADRs for why a specific piece is built the way it is.

> **Historical paths.** RFCs and ADRs are dated decision records and are not rewritten when code moves. Several cite `catalog.rs`, `develop_engine.rs` or `DevelopCanvas.svelte` as single files; those were split under RFC-0008 (`catalog/`, `develop_engine/`, `lib/gpu/…`) — use the maps below to find the current location.

## `app/` — the application

Two halves: `src-tauri/` is the Rust core (catalog, RAW decode, all app logic), `src/` is the Svelte frontend (UI only — no business logic lives here, it calls into `src-tauri` via Tauri commands).

### `app/src-tauri/` — Rust core

```
src-tauri/
├── src/
│   ├── main.rs             Entry point, just calls into lib.rs
│   ├── lib.rs               Tauri command definitions + app setup (catalog opened here)
│   ├── catalog/             SQLite catalog, one file per domain: mod.rs (Catalog struct, open), schema.rs (migrate),
│   │                        images, merge_sources, culling, geo, faces, keywords, collections, backup, edit_stack,
│   │                        presets, export_plugins (each with its own tests; test_support.rs = shared fixtures)
│   ├── develop_engine/      CPU edit-stack interpreter (the reference the GPU shaders must match): pipeline.rs
│   │                        (`apply_edit_stack`), color, tone, hsl_split, effects, detail, masks, crop, lens,
│   │                        perspective; tests/ = per-area tests + shared support
│   ├── import.rs            Import pipeline: scan a folder, hash, dedupe, thumbnail
│   ├── raw_decode.rs         LibRaw (via `rsraw`) wrapper for decoding RAW files
│   ├── jpeg_decode.rs         JPEG decoding via the `image` crate
│   ├── source_decode.rs        Format dispatch: which decoder handles which source file
│   ├── metadata.rs             EXIF-ish metadata extraction at import
│   ├── metadata_writer.rs       EXIF/IPTC writing into exported JPEGs (per-export toggles)
│   ├── geocode.rs               Forward geocoding: OpenStreetMap (Nominatim) default, optional Google
│   ├── preview_cache.rs         Develop preview cache (graded previews by content + stack hash)
│   ├── export.rs / export_plugin.rs   Export pipeline / the v0 export-plugin hook
│   ├── hdr_merge.rs / panorama_merge.rs   HDR bracket merge / feature-based panorama stitch
│   ├── face_detect.rs, face_cluster.rs, face_models.rs, face_pipeline.rs   Face detection, clustering, model cache, orchestration
│   ├── lens_profile.rs          Lens-profile matching for Lens Corrections
│   ├── soft_proof.rs / print.rs   Soft proofing / Print module output
│   └── storage.rs               User-configurable cache location (Settings > Storage)
├── capabilities/
│   └── default.json      Which Tauri APIs the webview is allowed to call
├── icons/                 App icons for bundling
├── Cargo.toml              Rust dependencies
└── tauri.conf.json          App config: window, identifier, asset protocol, bundle targets
```

Every file in `src/` has real unit tests colocated in a `#[cfg(test)] mod tests` block at the bottom — run them with `make test` (`cargo test --lib`). Tests that need a real RAW file are gated behind an `EMULSION_TEST_RAW_SAMPLE` env var rather than a committed fixture (see ADR-0003) — they skip cleanly without it, both locally and in CI (where the CI workflow fetches a sample first).

### `app/src/` — Svelte frontend

```
src/
├── routes/
│   ├── +layout.js          SPA-mode config (Tauri has no server, so no SSR)
│   ├── +page.svelte         THE REAL APP — all four modules' orchestration (being split per RFC-0009;
│   │                         state/actions modules will land in lib/state/ and lib/actions/)
│   └── m*-spike / m*-smoke/   Throwaway diagnostic routes (see note below), not part of the app
├── lib/
│   ├── api/                 Thin wrappers around invoke(): the only place that knows Tauri command names/shapes
│   │                        (catalog, develop, export, faces, map, print, storage, backup, system)
│   ├── components/          Svelte components. App shell: AppTitlebar, AppDialogs, StatusStrip; module
│   │                        bodies picked by +page.svelte: LibraryModule, DevelopModule, PrintModule (prop-less, import the stores/actions they use; RFC-0009 P8). Library:
│   │                        CatalogRail, LibraryGrid/GridCell, LibraryToolbar, LibraryFilterBar, LibraryImageViewer,
│   │                        LibraryCompareView, LibrarySurveyView, LibraryHistogram, MetadataPanel. Develop:
│   │                        DevelopCanvas (WebGPU), DevelopPanel, DevelopInfoBar, Filmstrip, Histogram,
│   │                        HistoryPanel, ToneCurveEditor, MaskToolStrip, MaskEditorPanel. Print: PrintPanel,
│   │                        PrintLayoutView. Dialogs: SettingsDialog, ExportDialog, ConfirmDialog,
│   │                        TextPromptDialog, SmartCollectionDialog, CopySettingsDialog, BackupPromptDialog
│   ├── gpu/                 Develop WebGPU code, split out of DevelopCanvas: gpuHandles.js (handle object +
│   │                        types), pipelines.js (init), sourceTexture.js (upload), renderFrame.js (per-frame
│   │                        render/histogram), atmChain.js + brushRaster.js (pure helpers), shaders/*.js (the
│   │                        WGSL sections, assembled by shaders/index.js)
│   ├── maskGeometry.js, cropMath.js, stepMath.js, histogramMath.js   Pure geometry/math helpers (with tests)
│   ├── state/               Shared app state as `.svelte.js` store classes (RFC-0009): shell.svelte.js (active
│   │                        module, status/notify, settings dialog, Develop rail widths, shortcuts);
│   │                        print.svelte.js (Print module settings/state), faces.svelte.js (People/Faces state +
│   │                        import-time detection prompt), importFlow.svelte.js (import/merge progress + the
│   │                        close-time backup prompt), library.svelte.js (image list, sources,
│   │                        filters, collections + their derived sets), selection.svelte.js (selected ids/anchor, selected images,
│   │                        Compare pair), develop.svelte.js (open image, live edit stack, history/snapshots, preview,
│   │                        canvas readouts, edit-stack persistence), developView.svelte.js (per-adjustment derived
│   │                        views of the edit stack), masks.svelte.js (mask tool/selection state, brush options, resample/eyedropper
│   │                        targets; `install()` self-cleaning effects), softProof.svelte.js (proof settings + debounced
│   │                        preview effect), presets.svelte.js (preset list + Develop dialog flags), exportFlow.svelte.js (Export dialog items + what Export/Print
│   │                        would act on); more land per RFC-0009 P7+.
│   │                        Tested via lib/state/*.test.js
│   ├── actions/             Workflows that write more than one store (RFC-0009 §3.2): printActions.js
│   │                        (print, export PDF, choose print profile), faceActions.js (people/face
│   │                        operations), backupActions.js (backup prompt answers), libraryActions.js (source switching, image/
│   │                        collection refresh, thumbnail patches, collection create/delete), selectionActions.js (click/range/
│   │                        step/select-all, Compare navigation), collectionsActions.js (add to / remove from collection),
│   │                        importActions.js (import/merge runners, thumbnail regeneration + startup poll; faceActions.js
│   │                        also holds the face-detection runners), developActions.js (adjustment/crop/WB/tone handlers,
│   │                        readout setters, peeks, snapshots), metadataActions.js (rating/flag/label, IPTC saves);
│   │                        libraryActions.js also holds handleRemoveConfirmed; maskActions.js (mask create/update/delete, resample,
│   │                        eyedropper, GPU fallback), presetActions.js (preset list, apply/import/export, copy/paste settings),
│   │                        historyActions.js (restore, undo/redo, snapshot restore, reset), softProofActions.js (custom
│   │                        profile), navigation.js (openDevelop, switchModule, next/prev image, export click); more land per RFC-0009
│   ├── libraryFilters.js    Pure Library scope + filter-bar logic (base image set, filters, camera/lens options)
│   ├── keyboard.js, menuActions.js   Global keyboard shortcuts / native-menu actions as `create…Handlers(ctx)`
│   │                        factories over a context object (unit-tested with a fake ctx; RFC-0009 P2)
│   ├── appEvents.js         `installAppEvents({ handleMenuAction })`: startup refreshes, window-close flush + backup prompt,
│   │                        Tauri listeners (drag-drop, menu, progress streams); called from the page's onMount (RFC-0009 P7)
│   ├── collectionRules.js, libraryFolders.js, railSections.js, panelLayout.js, shortcuts.js,
│   │   thumbnailBatchQueue.js, gpuFallback.js   Pure UI logic extracted from the page/components
│   └── styles/tokens.css     Dark-theme design tokens, ported from the reviewed mockup
├── app.html                 SvelteKit's HTML shell
└── static/                  Static assets (icons, etc.)
e2e/                         WebdriverIO specs (parity, gpu-fallback, performance, golden-path, …) run in CI on macOS + Windows
```

**The `m*-spike`/`m*-smoke` routes are not part of the app** — they're throwaway diagnostic pages used to verify something empirically (in-webview WebGPU, a real RAW decode, the asset protocol) when there's no tool available in this environment to screenshot or drive the native window directly. Each one is self-contained, reports its result by invoking a `report_spike_result` Tauri command that prints to the Rust process's stdout, and is left in the repo as a re-runnable check rather than deleted — but none of them are linked from the real UI. If one of these routes is ever loaded by the actual app (via `tauri.conf.json`'s window `url`), that's a leftover from manual testing that should have been reverted — check `git diff app/src-tauri/tauri.conf.json` if `make dev` ever opens one of these instead of the real Library view.

## `.github/workflows/ci.yml`

Two jobs on every push/PR to `main`: a `macos-latest`/`windows-latest` matrix that builds and tests the Rust core (fetching a real sample RAW file so the gated tests actually run, not just skip), and a `frontend-check` job (`npm run check`). Both platforms pass as of 2026-07-25 — RAW decoding on Windows links a vcpkg-installed prebuilt LibRaw instead of building from source under MSVC; see [ADR-0003](adr/ADR-0003-raw-decoding.md) and `app/src-tauri/vendor/rsraw-sys/PATCH.md` for why.
