# RFC-0009: `+page.svelte` state design (RFC-0008 §6, steps P3 onward)

- Status: Accepted (2026-09-19, review round 1; decisions in §8.2)
- Date: 2026-09-19
- Companion documents: [RFC-0008](RFC-0008-large-file-refactor.md) (the plan this unblocks), [PROGRESS.md](../../PROGRESS.md), [PROJECT_STRUCTURE.md](../PROJECT_STRUCTURE.md)

## 1. Problem

`app/src/routes/+page.svelte` is 4,788 lines. RFC-0008 §6 says its markup shells and pure logic can be split without a design (P1–P2), but the state-holding steps (P3 onward) need agreement first: which module owns which state, how modules talk, where `$effect`s and timers live, and what `openDevelop`/`switchModule` become. This document is that agreement. It is a design for a **behavior-preserving move**, not a redesign: no feature changes, no new state, no changed call ordering.

### 1.1 What the file actually contains (measured)

A parser pass (acorn + estree-walker, scope-aware; the same tooling family used for RFC-0008's V3) over the `<script>` block found: 172 imports, 120 `$state`, 47 `$derived`, 12 plain `let`s (timers, pending promises, resolvers), 137 functions, 5 `$effect`s, one `onMount` (213 lines). For every function and effect it recorded which top-level names are read and written. Findings that drive the design:

- **Most state has one natural owner.** 27 of 137 functions write to more than one domain (table in §4); the other 110 write at most one domain and move with their state without any coordination.
- **Three variables carry most of the coupling:** `editStack` (30 writer functions, 30 readers, 2 effects), `selectedId`/`selectedIds` (13 and 15 writers), and `statusMessage` (18 writers, read only by the status strip).
- **The develop domain is a dependency root, not a leaf.** Masks, soft-proof, presets, paste-settings, and the CPU-fallback effect all read or write `editStack`. That is why RFC-0008's P3 order (masks/presets/softProof *before* develop) does not work and is amended in §6.
- **25 `$derived`s are pure projections of `editStack`** (`exposure`, `contrast`, `hslBands`, `crop`, …). They must keep their per-field granularity (§3.4).
- **The five effects are mutually independent** (none reads a value another writes), so they can move to different owners without reordering hazards. Their reads decide where each must live (§3.5).

## 2. Goals and non-goals

Goals: every file under the 1,000-line ceiling (page target ≲ 800); each state variable has exactly one owner module; the cross-domain workflows are explicit, named, and readable in one place; every step is verifiable by the existing checks (RFC-0008 §2 and §7).

Non-goals: changing reactivity semantics, introducing a third-party state library, renaming user-visible behavior or DOM structure that e2e depends on, converting to TypeScript, or refactoring the second-tier files (`lib.rs`, `api/develop.js`, `DevelopPanel`, `MetadataPanel` — their own plan, RFC-0008 §9).

## 3. Design

### 3.1 Shared state lives in `.svelte.js` store objects (module singletons)

Svelte 5 forbids exporting a reassigned `$state` primitive from a module, so each domain gets a class whose fields are `$state`/`$derived`, exported as one instance, in `app/src/lib/state/<domain>.svelte.js`:

```js
// lib/state/selection.svelte.js
import { library } from "./library.svelte.js";
class SelectionStore {
  // Field initializers run before this body, but $derived is lazy (first read),
  // so `this.library` is set by the time selectedImage is evaluated.
  constructor(library) { this.library = library; }
  selectedId = $state(/** @type {number|null} */ (null));
  selectedIds = $state(/** @type {number[]} */ ([]));
  selectedImage = $derived(this.library.images.find((i) => i.id === this.selectedId) ?? null);
  clear() { this.selectedId = null; this.selectedIds = []; }
}
export function createSelectionStore(library) { return new SelectionStore(library); }
export const selection = createSelectionStore(library);
```

- **Singletons, with a factory beside each.** This is a single-window app, so one instance per store is correct, and it lets plain `.js` action modules and `keyboard.js` import them without a component tree. The exported `create…Store()` factory exists so vitest can build fresh instances per test instead of sharing global state.
- **Reads in components look the same as today** (`selection.selectedId` instead of `selectedId`); template and effect reads still hit the same signals, so tracking is unchanged.
- *Alternatives considered:* (a) Svelte context (`setContext`) — needs a component ancestor, so plain action modules and `keyboard.js` could not use it, and there is only ever one page; (b) leave state in `+page.svelte` and pass bundles down — that is today's problem; (c) a store library — a new dependency for something runes already do.

### 3.2 Dependency rules (a strict DAG)

```
shell            (activeModule, statusMessage, settings dialog, panel layout)   ← any store may import it, for notify()
library          (images, sources, filters, collections list + create/add dialog flags, memberships, keywords)
selection        → library
develop          (version, edit stack, history, snapshots, persistence, adjustments view) → library
masks            → develop           softProof → develop        presets → develop
print            → library, selection, develop (currentExportItems)
faces            → library, selection
importFlow       → library, shell
```

1. **Stores import stores only along this DAG, never actions or components.**
2. **A function that writes more than one store is a workflow, not a store method.** It lives in `lib/actions/*.js` (plain JS, imports any stores). A store method only mutates its own state (`selection.clear()`); `handleRemoveConfirmed`, which today writes library + selection + shell + develop, becomes `actions/libraryActions.js: removeConfirmed()` and calls each store's own mutator in the same order as today.
3. **`notify(msg)`** replaces the 18 direct `statusMessage = …` writes: `shell.notify(msg)` sets the same field. No behavior change; it removes the need for every module to import the whole shell store to write one string.

### 3.3 Ownership map

| Owner (file) | State (from the measured table) | Notes |
|---|---|---|
| `shell.svelte.js` | `activeModule`, `statusMessage`, `settingsOpen`, `panelWidths`, `panelResizeState`, `shortcuts` | `activeModule` is written only by the navigation orchestrator (§3.6) and `appEvents` |
| `library.svelte.js` | `images`, `libraryViewMode`, `libraryZoomLevel`, sources (`activeFolderKey`, `activeCollectionId`, `showLastImportOnly`, `activePersonId`), `collections`, `manualMembership`, `personMembership`, `allImageKeywords`, the ten filter fields, `compareCandidateId`, `confirmingRemoval`, the four collection-dialog flags (`creatingCollection`, `creatingSmartCollection`, `creatingCollectionWithImages`, `pendingAddToCollectionImageIds`); derived `baseImages`, `filteredImages`, `folderEntries`, `lastImportBatchId`, `keywordIdsByImage`, `cameraOptions`, `lensOptions`, `activeCollection`, `manualCollections`, `developFilmstripImages`, `compare*Image` | filtering/sorting math itself moves to pure `libraryFilters.js` (P2); the store wraps it in `$derived`. **As built (P4a):** `compareSelectImage`/`compareCandidateImage` and `developFilmstripImages` are *not* here: they read selection / `developVersionId`, and the DAG is selection → library, develop → library. They stay in the page until P4b/P5 (compare ones then go to `selection`; the filmstrip one to `develop`). Source switching, IPC refreshes, thumbnail application and collection create/delete are in `actions/libraryActions.js` (16 functions) rather than store methods. |
| `selection.svelte.js` | `selectedId`, `selectedIds`; derived `selectedImage`, `selectedImages`, `keywordTargetImageIds` | **As built (P4b):** also owns `compareSelectImage`/`compareCandidateImage` (moved up from `library` in the P4a note); `createSelectionStore(library)` takes its library as an argument. Its operations are in `actions/selectionActions.js` (click/range/toggle, grid step, select/deselect all, `targetVersionIds`, the four Compare handlers) and `actions/collectionsActions.js` (add-to / create-with / remove-from collection), not `libraryActions.js` as §4 first listed. `selectNextImage`/`selectPrevImage` (call `openDevelop`) and `handleRemoveConfirmed` (clears Develop state) wait for P5. |
| `develop.svelte.js` | `developVersionId`, `developImagePath`, `editStack`, `history`, `historyIndex`, `snapshots`, `previewUrl`, `copiedSettings`, canvas readouts (`sourceWidth/Height`, `histogramData`, `hoverPixel`, `showClippingOverlay`, `showOriginal`, `spacePanning`, `cropAspectLock`, `highlightedHslBand`), `gpuFallbackActive`, `cpuFallbackPreviewUrl`; derived `canUndo`/`canRedo`, `developImageContentHash`; **private:** `persistTimer`, `pendingLabel`, `pendingSave`, `pendingIptcSave`, `previewToken`, `previewDebounceTimer`, `hslBandHighlightTimer`, `gpuFallbackTimer` | the four persistence variables move **together** (§3.6, I5). Holds state, derived fields and persistence only; history/restore/reset are `actions/historyActions.js`, mask workflows `actions/maskActions.js`, adjustment transforms `developAdjustments.js` (§8.1). **As built (P5a):** fields are named without the `develop` prefix (`develop.versionId`, `develop.imagePath`, `develop.imageContentHash`). The persistence variables `persistTimer`, `pendingLabel`, `pendingSave`, `pendingIptcSave` and the preview `previewToken`/`previewDebounceTimer` are `#private`, reached only through methods: `flushEditStack(label?)`, `scheduleFlush(label)`, `cancelScheduledFlush()` (the timer-clearing block the restore/snapshot/remove paths had inline), `discardPendingLabel()`, `trackIptcSave(promise)`, `flushPending()` and the getters `hasPendingEdit`/`hasPendingWork` (what the window-close handler reads), plus `clearPreview()`/`schedulePreview(fetch)`. `hslBandHighlightTimer` and `gpuFallbackTimer` are **not** in the store: each has a single user (the eyedropper handler, the CPU-fallback effect) that stays in the page until masks/effects move (P6), so they stay page-local next to it. |
| `developView.svelte.js` | the per-adjustment `$derived` projections of `editStack` | one class with one `$derived` field per adjustment (§3.4). **As built (P5a):** 23 fields (`exposure` … `crop`); `masks`/`selectedMask` are mask state and move with the `masks` store (P6) |
| `masks.svelte.js` | `activeTool`, `selectedMaskId`, brush settings, `spotBrushSize`, `maskOverlaysVisible`, `showMaskOverlay`, `eyedropperTarget`, `colorRangeResampleTarget`; derived `masks`, `selectedMask`, `isResamplingColor` | |
| `softProof.svelte.js` | the `softProof*` settings, `softProofPreviewUrl`, `softProofLoading`, `softProofTimer`; derived `softProofProfileLabel` | |
| `presets.svelte.js` | `presets`, `creatingPreset`, `confirmingDeletePresetId`, `applyingPreset`, `creatingSnapshot`, `confirmingReset`, `copySettingsDialogOpen`, `pastingSettingsToSelection` | |
| `print.svelte.js` | the 13 `print*` settings, `printing`, `printReadyUrls`, `exportingPdf`; derived `printColorManagementSettings` (fields drop the `print` prefix: `print.template`, `print.readyUrls`, …) | **As built (P3b):** `exportItems` and `currentExportItems` are *not* here. `currentExportItems` reads develop and selection state, which have no stores until P4/P5, and `exportItems` is the Export dialog's flag, not Print's; both move then. |
| `faces.svelte.js` | `people`, `currentImageFaces`, `showFaceRects`, `hoveredFaceId`, `detectingFaces`, `scanProgress`, `detectionCancelable`, `avatarSourceUrls`, `confirmingDetectionOnImport`, `pendingImportBatchSize`, and the import-prompt promise bridge (`promptDetectionOnImport`, `confirmDetectionPrompt`, `cancelDetectionPrompt`) | **As built (P3c):** the IPC operations that touch only this store (`refreshPeople`, rename, reassign, create-and-tag, mark-not-a-face, cancel) are in `actions/faceActions.js`. `refreshCurrentImageFaces`, `runFaceDetection` and the three `handleDetectFaces…` handlers moved into `faceActions.js` in **P4c** (they read selection/library and write `images`). The selection-change `$effect` stays in the page, not `faces.install()`: a store cannot import the action it would call, and a `$effect` cannot live in a plain `.js` module; it moves with the component split. |
| `importFlow.svelte.js` | `importing`, the three phase progress values (`catalogProgress`, `thumbnailProgress`, `faceDetectionProgress`) and `phase`, `supportedExtensions`, `isDraggingFiles`, `mergingHdr`, `hdrMergeProgress`, `mergingPanorama`, and the close-time backup prompt (`backupPromptOpen`, `backupPromptSettings`, `showBackupPromptAndWait`, `closeBackupPrompt`) | **As built (P3d):** the backup handlers (`handleBackupDone`/`Skip`) are in `actions/backupActions.js`. `runImport`, the import/merge handlers, thumbnail regeneration and the startup poll (with its now-private `pollingThumbnailsOnStartup`) moved to `actions/importActions.js` in **P4c**; `refresh` went to `libraryActions.js` in P4a. The backup prompt sits here per the original table, though it is really app-lifecycle state; a later rename is cheap if it bothers anyone. |
| stays in the component that binds it | `imageViewerRef` (`bind:this`) → `LibraryModule` | reached by `keyboard.js` through a ctx getter |

Timers and pending promises are **private fields of their owner** (JS `#private` or unexported), never exported, so nothing else can clear or race them.

### 3.4 Per-field derived granularity is preserved

`exposure`, `contrast`, … are 25 separate `$derived`s. Collapsing them into one `$derived(readAdjustments(editStack))` object would make every consumer that reads any field depend on the whole object, re-running unrelated effects on every slider drag. So `developView.svelte.js` is a class with one `$derived` **field per adjustment**, each with the exact expression the page has today. Consumers read `view.exposure`. (Same reasoning as V3's getter objects for `renderInputs`.)

### 3.5 Effects live where their reads live

A `$effect` inside a plain `.svelte.js` module only works during component initialization, so stores do not create effects at import time. Each store that needs one exports an `install…()` function that the owning component calls from its `<script>` (or an `$effect.root` in tests). Placement, from the measured read/write sets:

| Effect (current line) | Reads | Moves to |
|---|---|---|
| soft-proof preview (967) | `editStack`, `developVersionId`, five `softProof*` fields | `softProof.install()` (writes only softProof fields + its timer) |
| CPU-fallback preview (1019) | `editStack`, `developImagePath`, content hash, `gpuFallbackActive` | `develop.installCpuFallback()` |
| colour-range resample reset (1065) | `colorRangeResampleTarget`, `activeTool`, `selectedMaskId` | `masks.install()` |
| eyedropper reset (1113) | `eyedropperTarget`, `activeTool` | `masks.install()` |
| refresh faces on selection (1467) | `selectedImage` | `faces.install()` |

Because the five effects are independent, their relative creation order does not matter, but each `install` is called once at the same point in component init as the effects it replaces (before any async work), and the equal-tracking property is checked in review by diffing each effect body, which must be byte-identical apart from the `store.` prefixes. `onMount` (213 lines: Tauri listeners, drop handler, window-close flush, startup polling) becomes `appEvents.js: installAppEvents(ctx)`, called from `onMount` and returning the same cleanup function.

### 3.6 The orchestrator, the `reset()` contract, and persistence

`openDevelop` and `switchModule` cross develop, masks, print, faces and shell. They become `actions/navigation.js`, executing **exactly the current statements in the current order**; the contract each store meets is a small named mutator, not a generic `reset()` that would hide the order:

- `develop.resetCanvasReadouts()` = `histogramData = null; hoverPixel = null; showClippingOverlay = false` (today's three lines).
- `masks.deselect()` = `activeTool = null; selectedMaskId = null`.
- `develop.loadFor(versionId, image)` etc. only where a multi-line block is already a unit.

Invariants that every step must keep (each is currently encoded in comments in the page and is where a "harmless" refactor would regress):

- **I1** `previousVersionId` is captured before `developVersionId` is reassigned and before any `await`.
- **I2** `await flushEditStack()` precedes `regenerateThumbnailFor(...)` (documented unawaited-IPC race).
- **I3** After the awaited lens lookup, apply the profile only if `developVersionId === versionId` (stale-open guard).
- **I4** `flushEditStack` captures `versionId` and `stack` synchronously before its write starts.
- **I5** `persistTimer`, `pendingLabel`, `pendingSave`, `pendingIptcSave` are moved as one unit into `develop`'s persistence code, and the window-close flush in `appEvents` calls one method (`develop.flushPending()`) that awaits both edit-stack and IPTC saves — the same two promises it awaits today.
- **I6** `switchModule("print")` snapshots `currentExportItems` once on entry; `"people"` calls `refreshPeople()`; leaving develop flushes and regenerates before clearing tool/mask state.

### 3.7 Keyboard and menu: pure factories over a context of callbacks

`handleGlobalKeydown` (280 lines) and `handleMenuAction` (68) read and write across library, selection and develop. They become `lib/keyboard.js: createKeyboardHandler(ctx)` and `lib/menuActions.js: createMenuHandler(ctx)`, where `ctx` is an object of **getters and callbacks assembled in `+page.svelte`** (`ctx.activeModule`, `ctx.selectedIds`, `ctx.deselectAll()`, `ctx.viewer()`, …). The factories import no stores, so they are unit-testable with a fake `ctx` — the one place this refactor *adds* test coverage (today keyboard behavior has none below e2e). Getters (not copied values) keep reads live, exactly as V3's `renderInputs`.

## 4. Cross-domain functions and where they go

The 27 multi-domain writers found by the parser, by destination:

| Destination | Functions |
|---|---|
| `actions/navigation.js` | `openDevelop`, `switchModule` |
| `actions/libraryActions.js` | `handleDeselectAll`, `handleCompareNext/Prev/Swap/MakeSelect`, `handleRemoveConfirmed`, `handleRemoveFromCollection`, `handleCopyrightChange`, `handleContactChange` |
| `actions/collectionsActions.js` | `handleAddToCollectionSelect`, `handleCreateCollectionWithImages` |
| `actions/presetActions.js` | `handleResetEditStack`, `handleImportPresetRequest`, `handleApplyPresetToSelection`, `handleCopySettingsConfirmed`, `handlePasteSettings`, `handlePasteSettingsToSelection` |
| `actions/printActions.js` | `handlePrint`, `handleExportPdf` |
| `importFlow` module functions | `runImport`, `handleMergeHdrBracket`, `handleMergePanorama` |
| `faces` module function | `runFaceDetection` |
| factories (§3.7) | `handleGlobalKeydown`, `handleMenuAction` |

## 5. Verification (per step)

Every step is its own PR that passes: `npm run check` (0 errors), `npm test` (vitest), `vite build`, and CI's macOS + Windows e2e (parity, gpu-fallback, performance, golden-path). In addition:

1. **Line-multiset comparison** (RFC-0008 §2 method, extended): normalize `store.`/`ctx.` prefixes, then every non-blank source line before/after must match except an itemized list of intended changes, reported in the PR. Scripts for the rewrite reuse the V3 acorn rewriter, extended to rewrite reads of a moved name to `store.name` and to skip shadowed locals.
2. **Effect and derived diff:** each moved `$effect`/`$derived` body is compared with its original; only the `store.` prefix may differ.
3. **e2e DOM sensitivities** (P1/P8 only): `.develop-body > .history-rail` and `.develop-body > .panel` (`panel-resize`), `.status` (`hdr-merge`), `.dialog`, `.export-btn` (`golden-path`), `.folder-btn`, `.tab-btn` (`storage-settings`) must exist with the same parent/child structure; scoped CSS moves with the markup it styles.
4. **Manual smoke list** for behavior the e2e suite does not exercise, run in the Tauri window before requesting review on P4–P8: keyboard shortcuts (rating/flag/label/arrows/undo/redo), menu actions, drag-and-drop import, HDR and panorama merge, faces scan, print/PDF export, collection create/add/remove, rapid clicking through the filmstrip (I1–I3), and quit-during-edit persistence (I5).
5. **New tests where the design creates seams:** `libraryFilters.test.js`, `keyboard.test.js` and `menuActions.test.js` (fake `ctx`), and store tests through the factories.
6. **First-step spike (P3) — done (2026-09-19).** Vitest could **not** compile runes modules with the original config, and getting it right took three things, not one (`app/vitest.config.js`): the bare `svelte()` compiler plugin; `ssr.resolve.conditions: ["browser"]` (Vitest's Node environment resolves Svelte's *server* runtime otherwise, where `flushSync` is a no-op); and forcing `ssr: false` on the plugin's hooks, because vite-plugin-svelte compiles `.svelte.js` for the server whenever a transform is flagged `ssr`, which **strips `$effect`/`$effect.root`** — stores would still pass their tests while effects silently never ran. `lib/state/svelteRunes.test.js` guards this (it asserts an effect re-runs when a store field changes). Ablation showed `inline: [/svelte/]` and `resolve.conditions` are not needed. No jsdom dependency was added.

## 6. Step order (amends RFC-0008 §6's P3–P5)

RFC-0008 ordered P3 (`print`, `faces`, `presets`, `softProof`, `masks`) before P5 (`develop`). The measurements show presets, masks and soft-proof all read `editStack`, so `develop` must exist first. Amended order:

| Step | Content | Risk |
|---|---|---|
| **P1** | Stateless markup shells: `AppTitlebar`, `AppDialogs`, `StatusStrip` (props/callbacks only, no store) | low — e2e selectors |
| **P2** | Pure logic: `libraryFilters.js`, `keyboard.js`, `menuActions.js` + tests | low |
| **P3** | `shell` (+`notify`), then leaf stores with no develop dependency: `print`, `faces`, `importFlow`; the vitest-runes spike | medium |
| **P4** | `library` (incl. collection-dialog flags) + `selection` stores, `libraryActions.js`, `collectionsActions.js` | medium — 40+ readers of `images`/`selectedId` |
| **P5** | `develop` (edit stack, history, persistence I1–I5), `developView`, `actions/navigation.js` | **high — do alone** |
|  | *As split when it was reached:* **P5a** `develop` + `developView` stores, persistence methods and their tests (merged as one PR); **P5b** the develop-only actions (adjustment handlers, crop/white-balance/tone helpers, snapshot create/delete, canvas readout setters); `navigation.js`, `historyActions.js` (restore/undo/redo write the mask selection) and the preset/snapshot-restore workflows wait for the `masks` store, so they land **after P6**, not in P5. The dependency is real: `openDevelop`/`switchModule`/`restoreTo` write `activeTool`/`selectedMaskId`, which are page-local until `masks` exists, and an action module cannot reach a page-local `let`. | |
| **P6** | `masks`, `softProof`, `presets` (+`presetActions.js`), with their `install()` effects | medium |
| **P7** | `appEvents.js` (the `onMount` body) | medium — window-close flush |
| **P8** | `LibraryModule`, `DevelopModule`, `PrintModule` components; scoped CSS moves with markup | high — e2e DOM shape |

After P7 the script is glue (~150 lines). Markup (~820) plus styles (~320) is ~1,140 lines before P1's ~340 lines of markup/style move out, so P8 is still needed to get under the ceiling, but its scope is smaller than RFC-0008 assumed. RFC-0008 §6 gets a one-line pointer to this document.

## 7. Risks

| Risk | Mitigation |
|---|---|
| Lost reactivity when a read moves from a local `$state` to a store field | Store fields are `$state`, so tracking is identical; verified by effect/derived diff (§5.2) and the e2e render specs |
| `$effect` outside a component context throws or never runs | `install…()` called from component `<script>` (§3.5); tests use `$effect.root` |
| Behavior change from reordering statements inside orchestrators | Orchestrators are the current bodies moved verbatim; invariants I1–I6 are comment-preserved and reviewed line-by-line |
| Import cycles between stores | The DAG in §3.2; a lint-style test (`stores.dag.test.js`) parses each store file's imports and fails on an edge not in the table |
| P5 blast radius (edit stack is read by ~30 functions) | P5 done alone, preceded by the rewrite tooling proving zero unresolved references; merged only after the smoke list |
| Hidden dependence on load-time evaluation order of module singletons | Stores do nothing at import beyond (for `shell`) reading two persisted UI preferences from localStorage, whose loaders fall back to defaults; `install…()` is explicit |

## 8. Layout analysis and decisions

### 8.1 Layout analysis (added after review round 1)

Question raised in review: is `lib/state/` + `lib/actions/` (layered by kind) the right pattern, or feature folders (`lib/develop/{store,actions}`)? The measurement, assigning each of the 137 functions a home domain by which state it touches most:

| Home domain | Functions | Also touch other domains' state |
|---|---|---|
| develop | 48 | masks 7, presets 5, shell 5, selection 2, library 2 |
| library | 20 | selection 3, develop 3, others 1 each |
| selection | 13 | **library 11**, shell 5, import 2 |
| presets 9 · import 7 · shell 7 · masks 6 · faces 6 · print 5 · collectionsUI 4 · softProof 1 | | mostly ≤ 3 cross-domain touches each |
| touches no store state | 11 | — |

17 functions touch three or more domains. Findings:

1. **Coupling is hub-and-spoke around `develop` and `library`+`selection`.** Feature folders would still have to import each other's internals for these workflows, so the folder boundary would not bound anything. Layering by *kind* (state vs. workflow) matches where the coupling actually is: workflows are the glue, and they belong together. It also matches the repo's existing convention (`lib/api/`, `lib/components/`, `lib/gpu/` are all by kind).
2. **Refinement to §3.3: the seven "develop + masks" functions are mask *workflows*.** `handleMaskCreated`, `handleMaskDeleted`, `handleCreateLuminanceRangeMask`, `handleColorRangeResampled`, `handleEyedropperSampled` write both `editStack` and mask selection; `restoreTo`, `handleRestoreSnapshot`, `handleResetEditStack`, `openDevelop`, `switchModule` write `editStack` and clear the tool/mask selection. Under the §3.2 rule they are actions, not methods on `develop` or `masks`; the `masks → develop` store edge stays valid because the *stores* never call each other's mutators, only actions do. So `develop.svelte.js` holds state, derived fields and persistence only; history/restore/reset go to `actions/historyActions.js`, mask workflows to `actions/maskActions.js`.
3. **`develop` has 48 home functions, too many for one file.** The adjustment-change handlers (`handleAdjustmentChange` and its siblings) are near-pure edit-stack transforms and become `developAdjustments.js`, alongside `stepMath.js`-style helpers, with actions calling them.
4. **`library`/`selection` are bidirectionally used by functions but not by stores**: 11 selection-home functions touch library, 3 library-home functions touch selection. The store DAG (`selection → library`) holds; the crossing functions are `libraryActions.js`.

Result: the layered layout stands, with `actions/` split by workflow (`navigation`, `historyActions`, `maskActions`, `presetActions`, `libraryActions`, `collectionsActions`, `printActions`) rather than one file per store.

### 8.2 Decisions

1. **Layout:** `lib/state/*.svelte.js` + `lib/actions/*.js`, with the refinement above. Accepted after the analysis above.
2. **Store shape:** singleton with a factory alongside (as §3.1). Accepted; the rule that stores have no import-time side effects is what keeps this safe.
3. **Amended step order (§6):** accepted.
4. **`collectionsUI`:** folded into `library` — its four dialog flags become `library` fields; no separate store.

## 9. Consequences

- `+page.svelte` becomes composition and glue; the domain logic gets names, files, and (for keyboard/menu/filters) tests.
- Import paths gain one level (`$lib/state/…`); `PROJECT_STRUCTURE.md` gets a `state/` and `actions/` section as the steps land.
- No ADR yet. If P5 forces a lasting architectural choice beyond this design, it is recorded as an ADR after it ships, per the project's RFC-then-ADR practice.
