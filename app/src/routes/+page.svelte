<script>
  import "$lib/styles/tokens.css";
  import { open, save } from "@tauri-apps/plugin-dialog";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { onMount } from "svelte";
  import LibraryFilterBar from "$lib/components/LibraryFilterBar.svelte";
  import LibraryGrid from "$lib/components/LibraryGrid.svelte";
  import DevelopCanvas from "$lib/components/DevelopCanvas.svelte";
  import DevelopPanel from "$lib/components/DevelopPanel.svelte";
  import MaskToolStrip from "$lib/components/MaskToolStrip.svelte";
  import MaskEditorPanel from "$lib/components/MaskEditorPanel.svelte";
  import ExportDialog from "$lib/components/ExportDialog.svelte";
  import ConfirmDialog from "$lib/components/ConfirmDialog.svelte";
  import TextPromptDialog from "$lib/components/TextPromptDialog.svelte";
  import SmartCollectionDialog from "$lib/components/SmartCollectionDialog.svelte";
  import MetadataPanel from "$lib/components/MetadataPanel.svelte";
  import Filmstrip from "$lib/components/Filmstrip.svelte";
  import DevelopInfoBar from "$lib/components/DevelopInfoBar.svelte";
  import HistoryPanel from "$lib/components/HistoryPanel.svelte";
  import BackupPromptDialog from "$lib/components/BackupPromptDialog.svelte";
  import SettingsDialog from "$lib/components/SettingsDialog.svelte";
  import LibraryToolbar from "$lib/components/LibraryToolbar.svelte";
  import LibraryImageViewer from "$lib/components/LibraryImageViewer.svelte";
  import LibraryCompareView from "$lib/components/LibraryCompareView.svelte";
  import LibrarySurveyView from "$lib/components/LibrarySurveyView.svelte";
  import { getStoredShortcuts } from "$lib/shortcuts.js";
  import {
    importFolder,
    importFiles,
    getSupportedExtensions,
    listImages,
    setRating,
    setFlag,
    setColorLabel,
    setCaption,
    setCopyright,
    setContact,
    removeImages,
    listAllImageKeywords,
    createCollection,
    createCollectionWithImages,
    createSmartCollection,
    updateSmartCollectionRules,
    deleteCollection,
    addImagesToCollection,
    removeImagesFromCollection,
    listCollections,
    listCollectionImageIds,
  } from "$lib/api/catalog.js";
  import {
    getEditStack,
    setEditStack,
    getHistory,
    restoreHistoryEntry,
    addSnapshot,
    getSnapshots,
    restoreSnapshot,
    deleteSnapshot,
    presetEligibleOps,
    applyPresetOps,
    createPreset,
    listPresets,
    deletePreset,
    importPresetFile,
    exportPresetFile,
    regenerateThumbnail,
    opValue,
    upsertOp,
    resetEditStack,
    listMasks,
    addMask,
    updateMask,
    removeMask,
    createLinearGradientMask,
    createRadialGradientMask,
    createBrushMask,
    createLuminanceRangeMask,
    createColorRangeMask,
    createSpotMask,
    createRedEyeMask,
    OVERLAY_CAPABLE_MASK_OPS,
    getToneCurvePoints,
    upsertToneCurve,
    IDENTITY_TONE_CURVE,
    buildToneCurveLut,
    sampleCurveLut,
    insertToneCurvePoint,
    getHslBands,
    upsertHslBand,
    IDENTITY_HSL_BANDS,
    nearestHslBand,
    getSplitToning,
    upsertSplitToningZone,
    upsertSplitToningBalance,
    IDENTITY_SPLIT_TONING,
    rgbToHsl,
    getVignette,
    upsertVignette,
    IDENTITY_VIGNETTE,
    getGrain,
    upsertGrain,
    IDENTITY_GRAIN,
    getSharpen,
    upsertSharpen,
    IDENTITY_SHARPEN,
    getLumaNr,
    upsertLumaNr,
    IDENTITY_LUMA_NR,
    getColorNr,
    upsertColorNr,
    IDENTITY_COLOR_NR,
    getCrop,
    upsertCrop,
    IDENTITY_CROP,
    getLensCorrection,
    upsertLensCorrection,
    setLensProfile,
    IDENTITY_LENS_CORRECTION,
    lookupLensProfile,
    getPerspective,
    upsertPerspective,
    IDENTITY_PERSPECTIVE,
    computeAutoWhiteBalance,
    computeEyedropperWhiteBalance,
    computeAutoTone,
    WB_PRESETS,
  } from "$lib/api/develop.js";
  import { largestCenteredCropForRatio, inscribedCropForAngle, cropRectFitsRotatedBounds } from "$lib/cropMath.js";
  import { buildKeywordIdsByImage, matchesRules } from "$lib/collectionRules.js";
  import { folderKeyForPath, buildFolderEntries } from "$lib/libraryFolders.js";
  import { getBackupSettings, updateBackupSettings, isBackupDue } from "$lib/api/backup.js";

  /** @type {import('$lib/api/catalog.js').ImageSummary[]} */
  let images = $state([]);
  // Multi-select (M2 Slice 3): `selectedIds` is the full selection,
  // `selectedId` stays as the anchor/primary -- the last plainly-clicked
  // image, which drives MetadataPanel, Shift-range endpoints, and any
  // single-image concern. Reassigned immutably on every change: Svelte 5's
  // $state doesn't deep-proxy Set, so in-place .add()/.delete() would
  // silently not react -- and reassignment matches the images-array idiom
  // used everywhere else in this file anyway.
  let selectedId = $state(/** @type {number | null} */ (null));
  let selectedIds = $state(/** @type {Set<number>} */ (new Set()));
  let confirmingRemoval = $state(false);
  let activeModule = $state("library"); // "library" | "develop"
  let libraryViewMode = $state(/** @type {"grid" | "loupe" | "compare" | "survey"} */ ("grid"));
  let libraryZoomLevel = $state(1);
  let imageViewerRef = $state(/** @type {any} */ (null));
  let shortcuts = $state(getStoredShortcuts());
  let compareCandidateId = $state(/** @type {number | null} */ (null));
  let importing = $state(false);
  let statusMessage = $state("");
  // M3 Slice 1: general Settings dialog, app-level (not module-scoped, so
  // it's not gated on activeModule like Export/Remove are).
  let settingsOpen = $state(false);

  // Collections (M2 Slice 5). `activeCollectionId === null` means "All
  // Photos" (no filter). `manualMembership` caches a manual collection's
  // image_id membership by collection id, fetched on click -- invalidated
  // by whichever collection id was actually just mutated (add/remove-from-
  // collection), never by `activeCollectionId`: those differ exactly when
  // the toolbar adds a selection to a DIFFERENT collection than the one
  // currently being viewed, and invalidating the wrong one would silently
  // leave the mutated collection's cache stale.
  let collections = $state(/** @type {import('$lib/api/catalog.js').CollectionSummary[]} */ ([]));
  let activeCollectionId = $state(/** @type {number | null} */ (null));
  let manualMembership = $state(/** @type {Map<number, Set<number>>} */ (new Map()));

  // Folders / Last Import (M4 Library slice). Three library "sources" --
  // All Photos, Last Import, and a real folder -- are mutually exclusive
  // with each other and with a Collection, so only one of
  // `activeCollectionId` / `activeFolderKey` / `showLastImportOnly` is ever
  // "on" at a time; `baseImages` below checks them in that same order.
  let activeFolderKey = $state(/** @type {string | null} */ (null));
  let showLastImportOnly = $state(false);

  let folderEntries = $derived(buildFolderEntries(images));

  /** The most recent import's batch id, or null before any tagged import
   * has happened (a pre-existing catalog whose rows predate this column).
   * `import_batch` is optional/nullable on ImageSummary, so this only
   * considers rows that actually have one. */
  let lastImportBatchId = $derived.by(() => {
    let max = /** @type {number | null} */ (null);
    for (const img of images) {
      const batch = img.import_batch;
      if (batch != null && (max === null || batch > max)) max = batch;
    }
    return max;
  });

  function selectAllPhotos() {
    activeCollectionId = null;
    activeFolderKey = null;
    showLastImportOnly = false;
  }

  function selectLastImport() {
    activeCollectionId = null;
    activeFolderKey = null;
    showLastImportOnly = true;
  }

  function selectFolder(/** @type {string} */ key) {
    activeCollectionId = null;
    activeFolderKey = key;
    showLastImportOnly = false;
  }
  let allImageKeywords = $state(/** @type {import('$lib/api/catalog.js').ImageKeywordAssignment[]} */ ([]));
  let keywordIdsByImage = $derived(buildKeywordIdsByImage(allImageKeywords));

  // Presets (M3): global, catalog-wide, same "fetch once at startup, keep
  // in sync locally" shape as `collections` above -- NOT re-fetched per
  // image the way history/snapshots are, since presets have no relation
  // to whichever photo happens to be open.
  let presets = $state(/** @type {import('$lib/api/develop.js').PresetEntry[]} */ ([]));

  async function refreshCollections() {
    collections = await listCollections();
  }

  async function refreshPresets() {
    presets = await listPresets();
  }

  async function loadManualMembership(/** @type {number} */ collectionId) {
    const memberIds = await listCollectionImageIds(collectionId);
    manualMembership = new Map(manualMembership).set(collectionId, new Set(memberIds));
  }

  // Library Filters
  let searchQuery = $state("");
  let flagFilter = $state(/** @type {"all" | "pick" | "unflagged" | "reject"} */ ("all"));
  let minRating = $state(0);
  let ratingOp = $state(/** @type {">=" | "="} */ (">="));
  let colorLabelFilter = $state("all");
  let fileTypeFilter = $state(/** @type {"all" | "raw" | "jpeg"} */ ("all"));

  function handleResetFilters() {
    searchQuery = "";
    flagFilter = "all";
    minRating = 0;
    ratingOp = ">=";
    colorLabelFilter = "all";
    fileTypeFilter = "all";
  }

  // Base image set for active folder/collection
  let baseImages = $derived.by(() => {
    if (showLastImportOnly) {
      return lastImportBatchId === null ? [] : images.filter((img) => img.import_batch === lastImportBatchId);
    }
    if (activeFolderKey !== null) {
      return images.filter((img) => folderKeyForPath(img.path) === activeFolderKey);
    }
    if (activeCollectionId === null) return images;
    const collection = collections.find((c) => c.id === activeCollectionId);
    if (!collection) return images;
    if (collection.is_smart) {
      const rules = collection.rules ?? [];
      return images.filter((img) => matchesRules(img, rules, keywordIdsByImage));
    }
    const memberIds = manualMembership.get(activeCollectionId);
    if (!memberIds) return []; // membership not fetched yet
    return images.filter((img) => memberIds.has(img.image_id));
  });

  // The image set the Library grid and filmstrip show after applying active filters
  let filteredImages = $derived.by(() => {
    let result = baseImages;

    // Search text query
    const q = searchQuery.trim().toLowerCase();
    if (q) {
      result = result.filter((img) => {
        const name = (img.path.split(/[/\\]/).pop() || "").toLowerCase();
        const path = img.path.toLowerCase();
        const make = (img.camera_make || "").toLowerCase();
        const model = (img.camera_model || "").toLowerCase();
        const lens = (img.lens_model || "").toLowerCase();
        const caption = (img.caption || "").toLowerCase();
        const copyright = (img.copyright || "").toLowerCase();
        const contact = (img.contact || "").toLowerCase();
        return (
          name.includes(q) ||
          path.includes(q) ||
          make.includes(q) ||
          model.includes(q) ||
          lens.includes(q) ||
          caption.includes(q) ||
          copyright.includes(q) ||
          contact.includes(q)
        );
      });
    }

    // Flag filter
    if (flagFilter === "pick") {
      result = result.filter((img) => img.flag === "pick");
    } else if (flagFilter === "unflagged") {
      result = result.filter((img) => img.flag === "none" || !img.flag);
    } else if (flagFilter === "reject") {
      result = result.filter((img) => img.flag === "reject");
    }

    // Star Rating filter
    if (minRating > 0) {
      if (ratingOp === ">=") {
        result = result.filter((img) => img.rating >= minRating);
      } else {
        result = result.filter((img) => img.rating === minRating);
      }
    }

    // Color label filter
    if (colorLabelFilter !== "all") {
      result = result.filter((img) => img.color_label === colorLabelFilter);
    }

    // File type filter
    if (fileTypeFilter === "raw") {
      result = result.filter(
        (img) =>
          !img.path.toLowerCase().endsWith(".jpg") && !img.path.toLowerCase().endsWith(".jpeg"),
      );
    } else if (fileTypeFilter === "jpeg") {
      result = result.filter(
        (img) =>
          img.path.toLowerCase().endsWith(".jpg") || img.path.toLowerCase().endsWith(".jpeg"),
      );
    }

    return result;
  });

  let activeCollection = $derived(collections.find((c) => c.id === activeCollectionId) ?? null);

  // The Filmstrip shows filtered images, falling back if active Develop photo is excluded
  let developFilmstripImages = $derived(
    filteredImages.some((img) => img.version_id === developVersionId) ? filteredImages : images,
  );

  async function selectCollection(/** @type {number | null} */ collectionId) {
    activeCollectionId = collectionId;
    activeFolderKey = null;
    showLastImportOnly = false;
    if (collectionId !== null && !manualMembership.has(collectionId)) {
      const collection = collections.find((c) => c.id === collectionId);
      if (collection && !collection.is_smart) await loadManualMembership(collectionId);
    }
  }

  let developVersionId = $state(/** @type {number | null} */ (null));
  let developImagePath = $state("");
  // M4 Smart Previews: derived, not a second $state var kept manually in
  // sync with developImagePath -- stays correct automatically as `images`
  // updates, and there's exactly one place (`openDevelop` below) that ever
  // needs to change which image is open anyway.
  let developImageContentHash = $derived(
    images.find((img) => img.version_id === developVersionId)?.content_hash ?? null,
  );
  /** @type {import('$lib/api/develop.js').EditStack} */
  let editStack = $state({ schema_version: 1, ops: [] });

  // History/Undo/Snapshots (M3). `history` is the current version's full
  // list (oldest first, matching Catalog::get_history's own ORDER BY id
  // ASC); `historyIndex` is a plain array index into it -- NOT a value
  // persisted anywhere -- representing "which entry does the live
  // editStack currently match." -1 means "before the first history
  // entry" (the version's untouched initial state, before any labeled
  // edit has ever been recorded). Reset to "newest" on every real edit
  // and whenever Develop (re)opens for an image; moved directly by
  // undo/redo/History-panel-click via restoreTo. See flushEditStack's own
  // doc comment for why no separate cursor concept needs to exist
  // server-side.
  let history = $state(/** @type {import('$lib/api/develop.js').HistoryEntry[]} */ ([]));
  let historyIndex = $state(-1);
  let snapshots = $state(/** @type {import('$lib/api/develop.js').SnapshotEntry[]} */ ([]));
  let exposure = $derived(opValue(editStack, "exposure", 0));
  let contrast = $derived(opValue(editStack, "contrast", 0));
  let saturation = $derived(opValue(editStack, "saturation", 0));

  // M3 Slice 5: local adjustment masks. `activeTool` drives DevelopCanvas's
  // hard-branched pointer routing (mask placement vs. pan/zoom);
  // `selectedMaskId` drives which mask (if any) MaskEditorPanel shows.
  // Both are pure view state, not persisted -- reset whenever Develop is
  // left, matching the same reasoning DevelopCanvas's own zoomMode uses.
  let masks = $derived(listMasks(editStack));
  let activeTool = $state(/** @type {string | null} */ (null));
  let selectedMaskId = $state(/** @type {string | null} */ (null));
  let selectedMask = $derived(masks.find((m) => m.id === selectedMaskId) ?? null);

  // M3 Slice 7: brush TOOL options -- unlike a mask's own exposure/
  // contrast/saturation (edited per-mask via MaskEditorPanel), these are
  // baked into each dab at the moment it's painted (real Lightroom's own
  // brush-options model: Size/Feather/Flow apply to whatever gets painted
  // NEXT), so they live here as plain view state, not per-mask, and are
  // never persisted or reset on module switch -- a user's preferred brush
  // size should survive across strokes/masks within one session.
  let brushSize = $state(0.05);
  let brushHardness = $state(70);
  let brushFlow = $state(1);
  let eraseMode = $state(false);
  // M4 Slice 2: spot removal's own brush-size TOOL option -- same "plain
  // view state, never persisted, survives across strokes/masks within one
  // session" treatment as brushSize above, kept separate (not shared with
  // brushSize) since a user's preferred adjustment-brush size and preferred
  // spot-removal size are independent preferences.
  let spotBrushSize = $state(0.02);
  // M4 Slice 2: hides every mask's overlay chrome (handles, pins, link
  // lines, brush/spot cursors) so a user can review the actual graded
  // result underneath without edit-tool UI in the way -- per explicit user
  // request ("the UI pivot points will block user's review"). Distinct
  // from `showMaskOverlay` below (that one only toggles the SELECTED
  // mask's own soft colored highlight fill); this one is a global
  // visibility switch for every mask's interactive chrome, toggleable via
  // MaskToolStrip's eye-icon button or the H hotkey (handleGlobalKeydown).
  let maskOverlaysVisible = $state(true);
  // M4 Slice 2: before/after preview -- when true, DevelopCanvas shows the
  // image as it would look with NO edits applied (skips the masks pass
  // entirely) instead of the live graded result, toggleable via the \
  // hotkey (handleGlobalKeydown) for a quick before/after comparison,
  // matching real Lightroom's own \ convention. Deliberately a toggle, not
  // a press-and-hold -- simpler and more reliable to implement correctly,
  // and matches Lightroom's own default behavior for this exact key.
  let showOriginal = $state(false);
  // M4 Slice 3: holding Space temporarily overrides whatever tool is
  // active so the user can pan a zoomed-in view without switching tools --
  // real Photoshop/Lightroom convention. Set by handleGlobalKeydown/
  // handleGlobalKeyup below (a press-and-hold, unlike showOriginal/
  // maskOverlaysVisible's own toggles, since panning only makes sense
  // while the key is physically down); also cleared on window blur so an
  // Alt-Tab away mid-hold can't leave this stuck true forever with no
  // keyup ever arriving to clear it.
  let spacePanning = $state(false);
  // Mask UI polish: soft colored overlay for the SELECTED no-geometry mask
  // (brush, luminance range), toggleable via a MaskEditorPanel checkbox or
  // the "O" hotkey. Grouped with the brush TOOL options above, not with
  // activeTool/selectedMaskId -- deliberately NEVER force-reset on
  // openDevelop/switchModule, same "a user's preferred setting should
  // survive across strokes/masks/images within one session" reasoning
  // those already document. Defaults true: these mask kinds are otherwise
  // invisible until a nonzero adjustment is set, a real discoverability
  // gap this directly fixes.
  let showMaskOverlay = $state(true);

  // Color range's "change select color" action: re-uses the SAME
  // click-to-sample canvas gesture that CREATES a color-range mask
  // (activeTool === "color_range" in DevelopCanvas.svelte), but points it
  // at an existing mask's `refColor` instead of creating a new mask.
  // `colorRangeResampleTarget` is the mask id awaiting its next canvas
  // click; kept separate from `selectedMaskId`/`activeTool` (rather than
  // overloading either) since NEITHER of those two states alone can tell
  // "the tool is active AND it's specifically in re-sample-into-an-
  // existing-mask mode, targeting THIS mask" apart from "the tool is
  // active to place a brand new mask."
  let colorRangeResampleTarget = $state(/** @type {string | null} */ (null));
  // Self-cleaning rather than patched into every place activeTool/
  // selectedMaskId can change (tool-strip toggle, panel close, mask
  // delete, module switch, selecting a different mask...): resample mode
  // is only ever valid while the color-range tool is active AND its
  // target is still the selected mask -- the instant either goes false,
  // there is no correct target left to resample into.
  $effect(() => {
    if (colorRangeResampleTarget !== null && (activeTool !== "color_range" || selectedMaskId !== colorRangeResampleTarget)) {
      colorRangeResampleTarget = null;
    }
  });
  let isResamplingColor = $derived(colorRangeResampleTarget !== null && colorRangeResampleTarget === selectedMaskId);

  /** Toggle symmetry with MaskToolStrip's own onToolToggle: clicking the
   * eyedropper again while already resampling cancels it, matching how
   * clicking an active tool button a second time turns it off. */
  function handleResampleColorToggle() {
    if (isResamplingColor) {
      activeTool = null;
      colorRangeResampleTarget = null;
      return;
    }
    if (selectedMaskId === null) return;
    activeTool = "color_range";
    colorRangeResampleTarget = selectedMaskId;
  }

  /** Commit path for a re-sample click -- patches the EXISTING mask
   * (unlike handleMaskCreated's color_range branch, which always adds a
   * new one) and, unlike the generic handleMaskUpdated slider path, also
   * exits resample mode afterward -- a re-sample is a one-shot action,
   * matching real Lightroom's own "click to pick, done" model for this
   * tool, not a mode you stay in. */
  function handleColorRangeResampled(
    /** @type {string} */ id,
    /** @type {{r: number, g: number, b: number}} */ refColor,
  ) {
    editStack = updateMask(editStack, id, { refColor });
    colorRangeResampleTarget = null;
    activeTool = null;
    scheduleFlush("Adjust Color Range");
  }

  // Eyedropper pickers (M3): Tone Curve point-insert, HSL band-identify,
  // Split Toning zone-tint all share ONE click-to-sample canvas gesture
  // (activeTool === "eyedropper" in DevelopCanvas.svelte), generalizing the
  // color-range resample pattern just above. `eyedropperTarget` names WHICH
  // of the four destinations is waiting for the next canvas click -- kept
  // separate from `activeTool` for the same reason `colorRangeResampleTarget`
  // is: `activeTool` alone can't distinguish "eyedropper active for Split
  // Toning Shadows" from "for HSL band-identify." Deliberately NOT threaded
  // into DevelopCanvas as a prop (unlike colorRangeResampleTarget): none of
  // these four destinations change how DevelopCanvas itself samples or
  // reports a click, only where +page.svelte routes the result afterward.
  let eyedropperTarget = $state(
    /** @type {"split_toning_shadows" | "split_toning_highlights" | "hsl_band" | "tone_curve_point" | "white_balance" | null} */ (
      null
    ),
  );
  // Self-cleaning, same reasoning as colorRangeResampleTarget's own effect
  // above -- including the two blanket `activeTool = null` resets on image
  // switch / module switch, which need no separate edit because this effect
  // already reacts to either of them.
  $effect(() => {
    if (eyedropperTarget !== null && activeTool !== "eyedropper") {
      eyedropperTarget = null;
    }
  });

  function isEyedropperActive(/** @type {typeof eyedropperTarget} */ target) {
    return activeTool === "eyedropper" && eyedropperTarget === target;
  }

  /** Toggle symmetry with handleResampleColorToggle above: clicking an
   * active eyedropper button again cancels it. */
  function handleEyedropperToggle(/** @type {typeof eyedropperTarget} */ target) {
    if (activeTool === "eyedropper" && eyedropperTarget === target) {
      activeTool = null;
      eyedropperTarget = null;
      return;
    }
    activeTool = "eyedropper";
    eyedropperTarget = target;
  }

  function handleMaskCreated(
    /** @type {
     *   | { kind: "linear_gradient", start: {x:number,y:number}, end: {x:number,y:number} }
     *   | { kind: "radial_gradient", center: {x:number,y:number}, radiusX: number, radiusY: number }
     *   | { kind: "brush", id: string }
     *   | { kind: "color_range", refColor: {r:number,g:number,b:number} }
     *   | { kind: "spot", id: string, initialDab: {x:number,y:number,radius:number} }
     *   | { kind: "red_eye", center: {x:number,y:number}, radiusX: number, radiusY: number }
     * } */ placement,
  ) {
    // Every kind gets its own explicit branch before the final
    // createLinearGradientMask fallback (not appended after it) -- an
    // untyped fallback assuming "unrecognized = linear" is a real bug
    // class already shipped and fixed once elsewhere in this codebase
    // (DevelopCanvas.svelte's mask-packing loop); a color-range placement
    // has no `.start`/`.end` at all, so hitting this fallback by mistake
    // would construct a broken linear mask and crash later.
    const mask =
      placement.kind === "radial_gradient"
        ? createRadialGradientMask(placement.center, placement.radiusX, placement.radiusY)
        : placement.kind === "brush"
          ? createBrushMask(placement.id)
          : placement.kind === "color_range"
            ? createColorRangeMask(placement.refColor)
            : placement.kind === "spot"
              ? createSpotMask(placement.initialDab, placement.id)
              : placement.kind === "red_eye"
                ? createRedEyeMask(placement.center, placement.radiusX, placement.radiusY)
                : createLinearGradientMask(placement.start, placement.end);
    editStack = addMask(editStack, mask);
    selectedMaskId = mask.id;
    // Real Lightroom drops back to selection after placing a gradient, but
    // a brush stroke should keep the Brush tool active (painting is
    // inherently multi-stroke -- see DevelopCanvas.svelte's brush-state
    // doc comment) rather than force a re-click of the tool for every dab.
    // Spot removal is now also a painted stroke (M4 Slice 2), so it stays
    // active the same way; only color range and the gradients are one-shot
    // placements that fall through the `!== "brush"` reset below.
    if (placement.kind !== "brush" && placement.kind !== "spot") activeTool = null;
    const label =
      placement.kind === "radial_gradient"
        ? "Add Radial Gradient"
        : placement.kind === "brush"
          ? "Add Brush Mask"
          : placement.kind === "color_range"
            ? "Add Color Range Mask"
            : placement.kind === "spot"
              ? "Add Spot Removal"
              : placement.kind === "red_eye"
                ? "Add Red Eye Correction"
                : "Add Linear Gradient";
    scheduleFlush(label);
  }

  // Luminance range has no geometry to place, so it doesn't go through
  // handleMaskCreated's placement-dispatch shape at all -- MaskToolStrip's
  // button calls this directly (real Lightroom's own behavior: this mask
  // kind is created on tool-select, no canvas interaction needed).
  function handleCreateLuminanceRangeMask() {
    const mask = createLuminanceRangeMask();
    editStack = addMask(editStack, mask);
    selectedMaskId = mask.id;
    scheduleFlush("Add Luminance Range Mask");
  }

  function handleMaskUpdated(/** @type {string} */ id, /** @type {Record<string, unknown>} */ patch) {
    editStack = updateMask(editStack, id, patch);
    scheduleFlush("Edit Mask");
  }

  function handleMaskDeleted() {
    if (selectedMaskId === null) return;
    editStack = removeMask(editStack, selectedMaskId);
    selectedMaskId = null;
    flushEditStack("Delete Mask");
  }

  // Develop panel "Reset": reverts every adjustment AND mask on the current
  // photo back to default in one shot, gated behind a confirmation (see
  // confirmingReset/the ConfirmDialog below) since it's destructive and
  // can't be undone. Same immediate-flush shape as handleMaskDeleted above
  // -- a confirmed destructive action should persist right away, not risk
  // being lost to the usual 250ms slider debounce.
  let confirmingReset = $state(false);

  function handleResetEditStack() {
    if (developVersionId === null) return;
    editStack = resetEditStack(editStack);
    selectedMaskId = null;
    activeTool = null;
    confirmingReset = false;
    flushEditStack("Reset");
  }

  // History/Snapshots (M3): naming a new snapshot uses the same generic
  // TextPromptDialog "New Collection" already uses -- no dedicated dialog
  // needed for one text field.
  let creatingSnapshot = $state(false);

  function handleCreateSnapshotConfirmed(/** @type {string} */ name) {
    creatingSnapshot = false;
    handleCreateSnapshot(name);
  }

  // Presets (M3): same TextPromptDialog/ConfirmDialog reuse as Collections/
  // Snapshots above -- no new dialog components needed.
  let creatingPreset = $state(false);
  let confirmingDeletePresetId = $state(/** @type {number | null} */ (null));
  // Guards the Library "Apply Preset to Selection" dropdown while a batch
  // apply is in flight -- narrow but real mitigation for the one residual
  // race a design review flagged: double-clicking into Develop on one of
  // the targeted images before its own invoke() in the batch has resolved.
  let applyingPreset = $state(false);

  function handleSaveCurrentAsPresetRequest() {
    creatingPreset = true;
  }

  async function handleCreatePresetConfirmed(/** @type {string} */ name) {
    creatingPreset = false;
    const preset = await createPreset(name, presetEligibleOps(editStack));
    presets = [...presets, preset];
  }

  /** Applying a preset to the currently open Develop image is an
   * immediate, discrete action (like Reset/mask-delete), not a debounced
   * slider drag -- flushes right away under its own label. */
  function handleApplyPreset(/** @type {number} */ presetId) {
    if (developVersionId === null) return;
    const preset = presets.find((p) => p.id === presetId);
    if (!preset) return;
    editStack = applyPresetOps(editStack, preset.edit_stack);
    flushEditStack(`Apply Preset: ${preset.name}`);
  }

  async function handleExportPreset(/** @type {number} */ presetId) {
    const preset = presets.find((p) => p.id === presetId);
    if (!preset) return;
    const path = await save({
      defaultPath: `${preset.name}.json`,
      filters: [{ name: "Preset", extensions: ["json"] }],
    });
    if (!path) return; // user cancelled
    try {
      await exportPresetFile(preset.name, preset.edit_stack, path);
      statusMessage = `Exported "${preset.name}"`;
    } catch (/** @type {any} */ e) {
      statusMessage = `Export preset failed: ${e}`;
    }
  }

  async function handleImportPresetRequest() {
    const path = await open({ multiple: false, filters: [{ name: "Preset", extensions: ["json"] }] });
    if (!path || Array.isArray(path)) return;
    try {
      const raw = await importPresetFile(path);
      // Defensive re-filter -- a hand-edited or foreign file could
      // contain a crop/mask op that would otherwise sail straight
      // through undetected (see importPresetFile's own doc comment).
      const filtered = presetEligibleOps({ schema_version: raw.schema_version, ops: raw.ops });
      const preset = await createPreset(raw.name, filtered);
      presets = [...presets, preset];
      statusMessage = `Imported "${raw.name}"`;
    } catch (/** @type {any} */ e) {
      statusMessage = `Import preset failed: ${e}`;
    }
  }

  function handleDeletePresetRequest(/** @type {number} */ presetId) {
    confirmingDeletePresetId = presetId;
  }

  async function handleDeletePresetConfirmed() {
    if (confirmingDeletePresetId === null) return;
    const presetId = confirmingDeletePresetId;
    confirmingDeletePresetId = null;
    await deletePreset(presetId);
    presets = presets.filter((p) => p.id !== presetId);
  }

  /** Library batch-apply -- version_id-targeted (NOT image_id: virtual
   * copies are separate versions with independent edit stacks, so
   * image_id would silently under-apply whenever one is selected
   * alongside its original). Each target is an independent getEditStack
   * -> merge -> setEditStack -> regenerateThumbnail round trip, same
   * non-atomic-across-the-batch shape rating/flag/color-label changes
   * already use -- a partial failure here is no worse than a partial
   * failure there. If the image currently open in Develop is among the
   * targets, its in-memory editStack is explicitly re-synced afterward
   * (see the comment below) so a later flush can't silently clobber the
   * just-applied preset with the stale pre-apply stack. */
  async function handleApplyPresetToSelection(/** @type {string} */ value) {
    if (!value) return;
    const preset = presets.find((p) => p.id === Number(value));
    if (!preset) return;
    const targets = [...selectedIds];
    if (targets.length === 0) return;
    applyingPreset = true;
    try {
      await Promise.all(
        targets.map(async (versionId) => {
          const current = await getEditStack(versionId);
          const merged = applyPresetOps(current, preset.edit_stack);
          await setEditStack(versionId, merged, `Apply Preset: ${preset.name}`);
          const path = await regenerateThumbnail(versionId);
          if (path) patchLocal(versionId, { thumbnail_path: path });
        }),
      );
      // Re-sync: developVersionId's in-memory editStack was NOT touched
      // by the loop above (it writes straight to the catalog), so if the
      // image currently open in Develop was also a batch target, refetch
      // it now -- otherwise a later flush (window close, switching
      // images) would still hold the stale pre-apply stack and silently
      // overwrite what this batch just wrote.
      if (developVersionId !== null && targets.includes(developVersionId)) {
        editStack = await getEditStack(developVersionId);
      }
      statusMessage = `Applied "${preset.name}" to ${targets.length} photo${targets.length === 1 ? "" : "s"}`;
    } catch (/** @type {any} */ e) {
      statusMessage = `Apply preset failed: ${e}`;
    } finally {
      applyingPreset = false;
    }
  }

  // What Export would act on right now: the open Develop image, or every
  // selected Library image (M2 Slice 3 batch export -- the frontend-only
  // follow-up M1 Slice 5's export_batch was explicitly built to accept).
  let selectedImage = $derived(images.find((img) => img.version_id === selectedId) ?? null);
  let selectedImages = $derived(images.filter((img) => selectedIds.has(img.version_id)));
  let currentExportItems = $derived.by(() => {
    if (activeModule === "develop" && developVersionId !== null) {
      return [{ path: developImagePath, version_id: developVersionId }];
    }
    if (selectedImages.length > 0) {
      return selectedImages.map((img) => ({ path: img.path, version_id: img.version_id }));
    }
    return selectedImage ? [{ path: selectedImage.path, version_id: selectedImage.version_id }] : [];
  });
  let exportItems = $state(/** @type {{ path: string, version_id: number }[] | null} */ (null));

  // Who a newly-typed keyword in MetadataPanel gets assigned to (M2 Slice
  // 4): the whole current Library selection when there is one, else just
  // the anchor image -- unconditional on "the acted-on cell is part of
  // the selection" (unlike targetVersionIds below) since there's no
  // per-cell click event here, just "apply to whatever's selected".
  let keywordTargetImageIds = $derived(
    selectedImages.length > 0
      ? selectedImages.map((img) => img.image_id)
      : selectedImage
        ? [selectedImage.image_id]
        : [],
  );

  let compareSelectImage = $derived.by(() => {
    if (selectedId !== null) {
      const match = filteredImages.find((img) => img.version_id === selectedId);
      if (match) return match;
    }
    return filteredImages[0] ?? null;
  });

  let compareCandidateImage = $derived.by(() => {
    if (compareCandidateId !== null) {
      const match = filteredImages.find((img) => img.version_id === compareCandidateId);
      if (match) return match;
    }
    if (selectedIds.size >= 2) {
      const otherId = [...selectedIds].find((id) => id !== selectedId);
      if (otherId != null) {
        const match = filteredImages.find((img) => img.version_id === otherId);
        if (match) return match;
      }
    }
    if (compareSelectImage && filteredImages.length > 1) {
      const selIdx = filteredImages.findIndex((img) => img.version_id === compareSelectImage.version_id);
      if (selIdx >= 0) {
        return filteredImages[(selIdx + 1) % filteredImages.length];
      }
    }
    return compareSelectImage;
  });

  // Persistence is debounced (not written on every slider tick) so a drag
  // doesn't flood the catalog with writes -- flushed immediately whenever
  // navigation could otherwise lose the pending change (UX-DESIGN.md §5's
  // "coalesced/debounced slider events" rule, applied to catalog writes
  // rather than the WebGPU frame loop).
  let persistTimer = /** @type {ReturnType<typeof setTimeout> | null} */ (null);
  // The label for whatever edit is currently sitting behind persistTimer's
  // debounce -- set by scheduleFlush, consumed (and cleared) by the next
  // flushEditStack call, whichever call site triggers it (the timer
  // itself, or an early explicit flush like openDevelop's). Kept as a
  // module-level variable rather than a flushEditStack parameter so every
  // existing `await flushEditStack()` call site (switchModule, openDevelop,
  // Export, window-close) keeps working unchanged: it always means "flush
  // whatever's actually pending, under whatever label it was scheduled
  // with -- or nothing, if nothing is pending."
  let pendingLabel = /** @type {string | null} */ (null);
  // Tracks an in-flight (already-fired, not-yet-resolved) save separately
  // from the debounce timer -- a flush can be triggered again (e.g. by the
  // close handler below) while a previous flush's write is still in
  // flight, and callers need to be able to wait for *that* too, not just
  // "is a timer currently pending".
  let pendingSave = /** @type {Promise<void> | null} */ (null);

  // M2 Slice 2: same shape as pendingSave above, but for the IPTC fields'
  // save-on-blur writes -- tracked separately since it's a different
  // in-flight write than the Develop edit stack's, and the close handler
  // below needs to wait on whichever (or both) are actually pending.
  let pendingIptcSave = /** @type {Promise<void> | null} */ (null);

  // M1 Slice 6: this used to NOT return setEditStack's promise, so every
  // `await flushEditStack()` call site (switchModule, openDevelop, Export)
  // resolved on the next microtask regardless of whether the write had
  // actually reached Rust/SQLite yet -- silently fire-and-forget. Fixed to
  // return the real promise; this is the fix the window-close flush below
  // actually depends on to mean anything.
  //
  // Reset-button slice: the write itself used to be gated on
  // `persistTimer !== null` -- correct for every slider-driven caller
  // (handleAdjustmentChange etc. always set persistTimer right before a
  // later flush), but a REAL bug for handleMaskDeleted and
  // handleResetEditStack below, which mutate `editStack` directly and call
  // this expecting an immediate write: since neither ever sets
  // persistTimer first, the old gate silently skipped the write entirely.
  // Verified empirically (a mask deletion's own persist was being silently
  // dropped -- the mask would incorrectly reappear after a full quit and
  // reopen, since the catalog was never actually updated). Fixed by always
  // writing whenever a develop image is open, regardless of whether a
  // timer happened to be pending -- harmless when nothing changed (an
  // idempotent re-write of the same stack), correct when something did.
  // History/Undo/Snapshots (M3): `label` is the human-readable name for
  // whatever edit is being flushed IMMEDIATELY (mask delete, Reset --
  // callers that never go through scheduleFlush's debounce at all).
  // Everything else (the setTimeout(flushEditStack, 250) debounce settle,
  // and every "flush whatever's pending before doing X" call site below)
  // omits it, falling back to `pendingLabel` -- whatever scheduleFlush
  // last recorded, or null if nothing is actually pending, in which case
  // this is the same harmless idempotent no-op-content rewrite it always
  // was. Only a truthy label ever moves `historyIndex` -- a no-op flush
  // must never disturb the undo cursor.
  function flushEditStack(/** @type {string=} */ label) {
    if (persistTimer !== null) {
      clearTimeout(persistTimer);
      persistTimer = null;
    }
    const effectiveLabel = label ?? pendingLabel;
    pendingLabel = null;
    if (developVersionId !== null) {
      const versionId = developVersionId;
      const stack = editStack;
      pendingSave = setEditStack(versionId, stack, effectiveLabel ?? undefined)
        .then((freshHistory) => {
          history = freshHistory;
          if (effectiveLabel) historyIndex = freshHistory.length - 1;
        })
        .finally(() => {
          pendingSave = null;
        });
    }
    return pendingSave ?? Promise.resolve();
  }

  /** Schedules a debounced, LABELED flush -- the replacement for every
   * former `if (persistTimer) clearTimeout(persistTimer); persistTimer =
   * setTimeout(flushEditStack, 250);` call site. Records `label` for
   * flushEditStack to pick up whenever it actually fires (the debounce
   * settling, or an earlier explicit flush elsewhere pre-empting it). */
  function scheduleFlush(/** @type {string} */ label) {
    pendingLabel = label;
    if (persistTimer) clearTimeout(persistTimer);
    persistTimer = setTimeout(flushEditStack, 250);
  }

  // Undo is disabled at historyIndex 0 (the version's oldest-ever labeled
  // edit) -- there's no history row representing "before the first edit"
  // to restore TO (edit_history only ever gains a row once a real edit
  // happens; the version's original untouched state is never itself
  // stored as one). A named, accepted scope cut, not a bug: the very
  // first edit ever made to a photo simply can't be undone via Ctrl+Z,
  // matching this session's "mid-drag undo guard" precedent of a
  // documented limitation over unrequested complexity (a synthetic
  // "Import" seed row, which real Lightroom itself uses for exactly this
  // reason -- deliberately out of scope here).
  let canUndo = $derived(historyIndex > 0);
  let canRedo = $derived(historyIndex < history.length - 1);

  /** Moves the live edit stack to `history[index]` -- undo, redo, and a
   * History-panel row click are all this same call, just with a
   * different `index`. See `history`/`historyIndex`'s own doc comment for
   * why this needs no server-side cursor concept at all. */
  async function restoreTo(/** @type {number} */ index) {
    if (developVersionId === null || index < 0 || index >= history.length) return;
    const versionId = developVersionId;
    const entryId = history[index].id;
    // A restore overwrites editStack wholesale -- cancel any debounced
    // write still pending first, or it could fire afterward under a now-
    // stale label and silently stomp the just-restored state.
    if (persistTimer !== null) {
      clearTimeout(persistTimer);
      persistTimer = null;
    }
    pendingLabel = null;
    editStack = await restoreHistoryEntry(versionId, entryId);
    historyIndex = index;
    selectedMaskId = null;
    activeTool = null;
    regenerateThumbnailFor(versionId);
  }

  function handleUndo() {
    if (canUndo) restoreTo(historyIndex - 1);
  }

  function handleRedo() {
    if (canRedo) restoreTo(historyIndex + 1);
  }

  /** Creates a named save point from whatever's CURRENTLY on screen --
   * flushes any pending debounced edit first so the snapshot never misses
   * the last slider tick. */
  async function handleCreateSnapshot(/** @type {string} */ name) {
    if (developVersionId === null) return;
    await flushEditStack();
    const versionId = developVersionId;
    const snapshot = await addSnapshot(versionId, name);
    snapshots = [...snapshots, snapshot];
  }

  /** Unlike restoreTo/restoreHistoryEntry, restoring a snapshot IS a new,
   * undoable edit of its own (see Catalog::restore_snapshot's doc
   * comment) -- the returned history list already includes its own
   * "Restore Snapshot: {name}" row, so this jumps historyIndex straight
   * to newest rather than searching for that row's position. */
  async function handleRestoreSnapshot(/** @type {number} */ snapshotId) {
    if (developVersionId === null) return;
    const versionId = developVersionId;
    if (persistTimer !== null) {
      clearTimeout(persistTimer);
      persistTimer = null;
    }
    pendingLabel = null;
    const [stack, freshHistory] = await restoreSnapshot(versionId, snapshotId);
    editStack = stack;
    history = freshHistory;
    historyIndex = freshHistory.length - 1;
    selectedMaskId = null;
    activeTool = null;
    regenerateThumbnailFor(versionId);
  }

  async function handleDeleteSnapshot(/** @type {number} */ snapshotId) {
    if (developVersionId === null) return;
    await deleteSnapshot(developVersionId, snapshotId);
    snapshots = snapshots.filter((s) => s.id !== snapshotId);
  }

  // Catalog backup (PRD §7.6): the close handler needs to actually wait for
  // the user's dialog interaction before destroying the window -- a
  // genuinely new pattern here, since every other dialog in this app is
  // fire-and-forget from its caller's perspective. `resolveBackupPrompt`
  // always eventually fires: "Skip This Time" is always available even if
  // "Back Up Now" fails, so this promise is guaranteed to settle.
  let backupPromptOpen = $state(false);
  let backupPromptSettings = $state(/** @type {import('$lib/api/backup.js').BackupSettings | null} */ (null));
  let resolveBackupPrompt = /** @type {(() => void) | null} */ (null);

  function showBackupPromptAndWait(/** @type {import('$lib/api/backup.js').BackupSettings} */ settings) {
    return new Promise((resolve) => {
      backupPromptSettings = settings;
      resolveBackupPrompt = () => resolve(undefined);
      backupPromptOpen = true;
    });
  }

  function closeBackupPrompt() {
    backupPromptOpen = false;
    resolveBackupPrompt?.();
    resolveBackupPrompt = null;
  }

  /** @param {import('$lib/api/backup.js').BackupSettings} settings */
  function handleBackupDone(settings) {
    updateBackupSettings(settings).catch(() => {});
    closeBackupPrompt();
  }

  /** @param {import('$lib/api/backup.js').BackupSettings} settings */
  function handleBackupSkip(settings) {
    // Resets the due-clock so skipping doesn't re-prompt on literally the
    // next close -- a deliberate simplification of Lightroom's own more
    // precise "postpone until the next real interval" semantics.
    updateBackupSettings({ ...settings, last_backup_at: new Date().toISOString() }).catch(() => {});
    closeBackupPrompt();
  }

  // Thumbnail refresh after a Develop edit -- entirely separate from
  // pendingSave/pendingIptcSave on purpose. Chaining this onto the same
  // promise flushEditStack's callers await would silently reintroduce the
  // exact "app hangs unable to quit" class of bug M1 Slice 6 already fixed
  // once for the edit-stack flush itself -- a slow/failed thumbnail regen
  // must never be able to delay a save or block app quit. Never awaited by
  // any caller. Only called from real "done editing this image for now"
  // transitions (leaving Develop, exporting, closing) -- not from the bare
  // 250ms debounce settle inside flushEditStack, since a cache-hit reuse
  // of the Develop preview is still a real decode+edit+resize+encode, not
  // free, and a user still actively dragging a slider would otherwise
  // trigger a regen immediately superseded by the next tick.
  function regenerateThumbnailFor(/** @type {number | null} */ versionId) {
    if (versionId === null) return;
    regenerateThumbnail(versionId)
      .then((path) => {
        if (path) patchLocal(versionId, { thumbnail_path: path });
      })
      .catch(() => {}); // best-effort; a stale grid thumbnail isn't worth surfacing an error for
  }

  async function refresh() {
    images = await listImages();
  }

  // Thumbnail generation for a freshly-imported JPEG (and the RAW backstop
  // path) runs as a fire-and-forget background pass on the Rust side (see
  // import.rs's generate_missing_thumbnails) -- it is NOT part of the
  // import command's own response, so the one-shot refresh() right after
  // import can only ever show the pre-generation state (thumbnail_path:
  // NULL). Nothing else ever tells this component to look again, so
  // without this, a just-imported photo's grid cell stays a blank
  // placeholder for the rest of the session, even once the backend has
  // long since finished. Bounded polling (not an open-ended interval) so a
  // permanently-stuck thumbnail (a real decode failure) doesn't poll
  // forever -- it just stops trying and leaves the placeholder, which is
  // the correct outcome in that case.
  let pollingThumbnails = false;
  async function pollUntilThumbnailsReady() {
    if (pollingThumbnails) return;
    pollingThumbnails = true;
    try {
      const maxAttempts = 10;
      const intervalMs = 1500;
      for (let attempt = 0; attempt < maxAttempts; attempt++) {
        if (!images.some((img) => img.thumbnail_path === null)) return;
        await new Promise((resolve) => setTimeout(resolve, intervalMs));
        await refresh();
      }
    } finally {
      pollingThumbnails = false;
    }
  }

  /** @type {string[] | null} */
  let supportedExtensions = $state(null);

  async function runImport(/** @type {() => Promise<import('$lib/api/catalog.js').ImportSummary | null>} */ doImport) {
    importing = true;
    statusMessage = "";
    try {
      const summary = await doImport();
      if (!summary) return; // user cancelled the dialog
      statusMessage = `Imported ${summary.imported}, ${summary.skipped_duplicates} already in library, ${summary.failed} failed`;
      await refresh();
      pollUntilThumbnailsReady();
    } catch (/** @type {any} */ e) {
      statusMessage = `Import failed: ${e}`;
    } finally {
      importing = false;
    }
  }

  function handleImportFolder() {
    return runImport(async () => {
      const dir = await open({ directory: true, multiple: false });
      return dir ? importFolder(/** @type {string} */ (dir)) : null;
    });
  }

  let isDraggingFiles = $state(false);

  function handleDropImport(/** @type {string[]} */ paths) {
    if (!paths || paths.length === 0) return;
    return runImport(async () => {
      return importFiles(paths);
    });
  }

  async function handleImportFiles() {
    // M2 Slice 1: a separate entry point from folder import -- Tauri's
    // dialog plugin has independent `directory`/`multiple` flags, no mode
    // that lets one native dialog pick either files or a folder.
    if (!supportedExtensions) supportedExtensions = await getSupportedExtensions();
    return runImport(async () => {
      const paths = await open({
        multiple: true,
        filters: [{ name: "Photos", extensions: /** @type {string[]} */ (supportedExtensions) }],
      });
      return paths ? importFiles(/** @type {string[]} */ (paths)) : null;
    });
  }

  // Optimistic local update (UX-DESIGN.md §5) so culling feels instant --
  // the write still goes to the real catalog, this just avoids waiting on
  // a round trip + full refetch before the UI reflects the change.
  function patchLocal(/** @type {number} */ versionId, /** @type {Partial<import('$lib/api/catalog.js').ImageSummary>} */ patch) {
    images = images.map((img) => (img.version_id === versionId ? { ...img, ...patch } : img));
  }

  // Batch rate/flag/color-label (MILESTONES.md M2 scope, deferred from
  // Slice 3's multi-select work): Lightroom-style -- acting on a cell that
  // is part of an active multi-selection applies to the whole selection,
  // not just that one cell. Acting on a cell OUTSIDE the current
  // selection (or when only one image is selected) stays single-target,
  // unaffected by an unrelated selection elsewhere. When called without an
  // explicit versionId (e.g. from toolbar / metadata panel / hotkey), applies
  // to all currently selected images (or the anchor image).
  function targetVersionIds(/** @type {number | null | undefined} */ versionId) {
    if (versionId !== undefined && versionId !== null) {
      if (selectedIds.size > 1 && selectedIds.has(versionId)) {
        return [...selectedIds];
      }
      return [versionId];
    }
    if (selectedIds.size > 0) return [...selectedIds];
    return selectedId !== null ? [selectedId] : [];
  }

  async function handleRatingChange(/** @type {number | null | undefined} */ versionId, /** @type {number} */ rating) {
    const targets = targetVersionIds(versionId);
    if (targets.length === 0) return;
    for (const id of targets) patchLocal(id, { rating });
    await Promise.all(targets.map((id) => setRating(id, rating)));
  }

  async function handleFlagChange(/** @type {number | null | undefined} */ versionId, /** @type {string} */ flag) {
    const targets = targetVersionIds(versionId);
    if (targets.length === 0) return;
    for (const id of targets) patchLocal(id, { flag });
    await Promise.all(targets.map((id) => setFlag(id, flag)));
  }

  async function handleColorLabelChange(/** @type {number | null | undefined} */ versionId, /** @type {string} */ colorLabel) {
    const targets = targetVersionIds(versionId);
    if (targets.length === 0) return;
    for (const id of targets) patchLocal(id, { color_label: colorLabel });
    await Promise.all(targets.map((id) => setColorLabel(id, colorLabel)));
  }

  // Keyboard navigation & selection helpers
  function selectNextImage(/** @type {boolean=} */ extend) {
    if (filteredImages.length === 0) return;
    if (selectedId === null) {
      const first = filteredImages[0];
      selectedId = first.version_id;
      selectedIds = new Set([first.version_id]);
      return;
    }
    const idx = filteredImages.findIndex((img) => img.version_id === selectedId);
    if (idx === -1) {
      const first = filteredImages[0];
      selectedId = first.version_id;
      selectedIds = new Set([first.version_id]);
      return;
    }
    if (idx < filteredImages.length - 1) {
      const nextImg = filteredImages[idx + 1];
      if (extend) {
        const next = new Set(selectedIds);
        next.add(nextImg.version_id);
        selectedIds = next;
        selectedId = nextImg.version_id;
      } else {
        selectedId = nextImg.version_id;
        selectedIds = new Set([nextImg.version_id]);
      }
      if (activeModule === "develop") {
        openDevelop(nextImg.version_id);
      }
    }
  }

  function selectPrevImage(/** @type {boolean=} */ extend) {
    if (filteredImages.length === 0) return;
    if (selectedId === null) {
      const last = filteredImages[filteredImages.length - 1];
      selectedId = last.version_id;
      selectedIds = new Set([last.version_id]);
      return;
    }
    const idx = filteredImages.findIndex((img) => img.version_id === selectedId);
    if (idx === -1) {
      const first = filteredImages[0];
      selectedId = first.version_id;
      selectedIds = new Set([first.version_id]);
      return;
    }
    if (idx > 0) {
      const prevImg = filteredImages[idx - 1];
      if (extend) {
        const next = new Set(selectedIds);
        next.add(prevImg.version_id);
        selectedIds = next;
        selectedId = prevImg.version_id;
      } else {
        selectedId = prevImg.version_id;
        selectedIds = new Set([prevImg.version_id]);
      }
      if (activeModule === "develop") {
        openDevelop(prevImg.version_id);
      }
    }
  }

  function selectGridStep(/** @type {number} */ step, /** @type {boolean=} */ extend) {
    if (filteredImages.length === 0) return;
    if (selectedId === null) {
      const first = filteredImages[0];
      selectedId = first.version_id;
      selectedIds = new Set([first.version_id]);
      return;
    }
    const idx = filteredImages.findIndex((img) => img.version_id === selectedId);
    if (idx === -1) return;
    const targetIdx = Math.max(0, Math.min(filteredImages.length - 1, idx + step));
    const targetImg = filteredImages[targetIdx];
    if (!targetImg) return;
    if (extend) {
      const [from, to] = idx <= targetIdx ? [idx, targetIdx] : [targetIdx, idx];
      selectedIds = new Set(filteredImages.slice(from, to + 1).map((img) => img.version_id));
      selectedId = targetImg.version_id;
    } else {
      selectedId = targetImg.version_id;
      selectedIds = new Set([targetImg.version_id]);
    }
  }

  function handleSelectAll() {
    if (filteredImages.length === 0) return;
    selectedIds = new Set(filteredImages.map((img) => img.version_id));
    if (selectedId === null || !selectedIds.has(selectedId)) {
      selectedId = filteredImages[0].version_id;
    }
  }

  function handleDeselectAll() {
    if (libraryViewMode !== "grid") {
      libraryViewMode = "grid";
      return;
    }
    selectedIds = new Set();
    selectedId = null;
  }

  function handleCompareNextCandidate() {
    if (filteredImages.length === 0) return;
    const curCandidate = compareCandidateImage;
    const curSelect = compareSelectImage;
    const cIdx = curCandidate
      ? filteredImages.findIndex((img) => img.version_id === curCandidate.version_id)
      : 0;
    const nextIdx = (cIdx + 1) % filteredImages.length;
    const nextCand = filteredImages[nextIdx];
    compareCandidateId = nextCand.version_id;
    if (curSelect) {
      selectedIds = new Set([curSelect.version_id, nextCand.version_id]);
    }
  }

  function handleComparePrevCandidate() {
    if (filteredImages.length === 0) return;
    const curCandidate = compareCandidateImage;
    const curSelect = compareSelectImage;
    const cIdx = curCandidate
      ? filteredImages.findIndex((img) => img.version_id === curCandidate.version_id)
      : 0;
    const prevIdx = (cIdx - 1 + filteredImages.length) % filteredImages.length;
    const prevCand = filteredImages[prevIdx];
    compareCandidateId = prevCand.version_id;
    if (curSelect) {
      selectedIds = new Set([curSelect.version_id, prevCand.version_id]);
    }
  }

  function handleCompareSwap() {
    const curSelect = compareSelectImage;
    const curCand = compareCandidateImage;
    if (!curSelect || !curCand) return;
    const oldSelId = curSelect.version_id;
    const oldCandId = curCand.version_id;
    selectedId = oldCandId;
    compareCandidateId = oldSelId;
    selectedIds = new Set([oldCandId, oldSelId]);
  }

  function handleCompareMakeSelect() {
    const curCand = compareCandidateImage;
    if (!curCand) return;
    selectedId = curCand.version_id;
    const newSelIdx = filteredImages.findIndex((img) => img.version_id === curCand.version_id);
    if (filteredImages.length > 1) {
      const nextCandIdx = (newSelIdx + 1) % filteredImages.length;
      compareCandidateId = filteredImages[nextCandIdx].version_id;
      selectedIds = new Set([selectedId, compareCandidateId]);
    } else {
      selectedIds = new Set([selectedId]);
    }
  }

  // Multi-select click semantics (M2 Slice 3), standard file-manager
  // behavior: plain click replaces the selection and moves the anchor;
  // Cmd/Ctrl toggles one image in/out; Shift selects the contiguous range
  // (in `filteredImages` order) from the anchor to the clicked image.
  //
  // The range is computed over `filteredImages`, NOT the full `images`
  // array (M2 Slice 5 fix): while no collection filter is active the two
  // are identical, but once a collection filters the grid, indexing into
  // the unfiltered `images` array would compute a range over catalog-wide
  // positions that don't correspond to what's on screen -- Shift-click
  // could silently pull hidden/filtered-out images into the selection,
  // which then flows into remove/batch-culling/keyword-assignment against
  // images the user never saw or selected. LibraryGrid's virtualization
  // is a separate, narrower concern (it only slices what's *rendered*
  // within the already-filtered set for scroll performance) and doesn't
  // affect this.
  function handleSelect(/** @type {number} */ versionId, /** @type {MouseEvent=} */ event) {
    if (event?.shiftKey && selectedId !== null) {
      const anchorIndex = filteredImages.findIndex((img) => img.version_id === selectedId);
      const clickedIndex = filteredImages.findIndex((img) => img.version_id === versionId);
      if (anchorIndex !== -1 && clickedIndex !== -1) {
        const [from, to] = anchorIndex <= clickedIndex ? [anchorIndex, clickedIndex] : [clickedIndex, anchorIndex];
        selectedIds = new Set(filteredImages.slice(from, to + 1).map((img) => img.version_id));
        return; // anchor stays put, Lightroom/Finder-style
      }
      // Stale anchor (e.g. it was just removed, or is outside the current
      // filter): fall through to plain select.
    }
    if (event?.metaKey || event?.ctrlKey) {
      const next = new Set(selectedIds);
      if (next.has(versionId)) {
        next.delete(versionId);
        if (selectedId === versionId) {
          selectedId = next.size > 0 ? [...next][next.size - 1] : null;
        }
      } else {
        next.add(versionId);
        selectedId = versionId;
      }
      selectedIds = next;
      return;
    }
    selectedId = versionId;
    selectedIds = new Set([versionId]);
  }

  // Non-destructive removal (M2 Slice 3): catalog rows + app-owned derived
  // files only -- the backend never touches source files. `await` the
  // command BEFORE filtering local state: the other order would let an
  // in-flight pollUntilThumbnailsReady refresh() momentarily resurrect the
  // removed rows in the UI.
  async function handleRemoveConfirmed() {
    confirmingRemoval = false;
    // Symmetry with the close handler: force any in-progress IPTC edit's
    // blur-save to fire before the rows it targets can disappear.
    /** @type {HTMLElement | null} */ (document.activeElement)?.blur();

    const imageIds = [...new Set(selectedImages.map((img) => img.image_id))];
    if (imageIds.length === 0) return;
    try {
      await removeImages(imageIds);
    } catch (/** @type {any} */ e) {
      statusMessage = `Remove failed: ${e}`;
      return;
    }
    const removedVersionIds = new Set(selectedImages.map((img) => img.version_id));
    images = images.filter((img) => !removedVersionIds.has(img.version_id));
    statusMessage = `Removed ${imageIds.length} photo${imageIds.length === 1 ? "" : "s"} from catalog`;
    selectedId = null;
    selectedIds = new Set();
    // If the image open in Develop was just removed, clear that state too --
    // otherwise the develop branch keeps rendering a deleted image, and a
    // pending debounced edit-stack save would fire a pointless IPC call
    // against the deleted version.
    if (developVersionId !== null && removedVersionIds.has(developVersionId)) {
      if (persistTimer !== null) {
        clearTimeout(persistTimer);
        persistTimer = null;
      }
      developVersionId = null;
      developImagePath = "";
    }
  }

  // Collections (M2 Slice 5). Rename/edit-existing-smart-collection-rules
  // UI is deliberately deferred (matching this codebase's precedent for
  // `add_image_with_metadata`/`add_edit_stack` -- a lower-level building
  // block kept ready without a UI trigger yet): the rail's "+" only ever
  // creates fresh collections; changing an existing one means delete and
  // recreate for now.
  let creatingCollection = $state(false);
  let creatingSmartCollection = $state(false);
  let creatingCollectionWithImages = $state(false);
  let pendingAddToCollectionImageIds = $state(/** @type {number[]} */ ([]));
  let manualCollections = $derived(collections.filter((c) => !c.is_smart));

  async function handleCreateCollection(/** @type {string} */ name) {
    creatingCollection = false;
    await createCollection(name);
    await refreshCollections();
  }

  async function handleCreateSmartCollection(
    /** @type {string} */ name,
    /** @type {import('$lib/api/catalog.js').CollectionRule[]} */ rules,
  ) {
    creatingSmartCollection = false;
    await createSmartCollection(name, rules);
    await refreshCollections();
  }

  async function handleDeleteCollection(/** @type {number} */ collectionId, /** @type {MouseEvent} */ event) {
    event.stopPropagation(); // don't also trigger selectCollection
    await deleteCollection(collectionId);
    if (activeCollectionId === collectionId) activeCollectionId = null;
    await refreshCollections();
  }

  // "Add to Collection…" toolbar picker, from a multi-selection.
  async function handleAddToCollectionSelect(/** @type {string} */ value) {
    if (!value) return;
    const imageIds = [...new Set(selectedImages.map((img) => img.image_id))];
    if (imageIds.length === 0) return;
    if (value === "__new__") {
      pendingAddToCollectionImageIds = imageIds;
      creatingCollectionWithImages = true;
      return;
    }
    const collectionId = Number(value);
    await addImagesToCollection(collectionId, imageIds);
    // Invalidate by the collection id that was actually just mutated, not
    // by activeCollectionId -- those differ when adding to a DIFFERENT
    // collection than the one currently being viewed, and invalidating
    // the wrong one would silently leave the mutated one's cache stale.
    if (manualMembership.has(collectionId)) await loadManualMembership(collectionId);
    await refreshCollections();
    statusMessage = `Added ${imageIds.length} photo${imageIds.length === 1 ? "" : "s"} to collection`;
  }

  async function handleCreateCollectionWithImages(/** @type {string} */ name) {
    creatingCollectionWithImages = false;
    const imageIds = pendingAddToCollectionImageIds;
    pendingAddToCollectionImageIds = [];
    await createCollectionWithImages(name, imageIds);
    await refreshCollections();
    statusMessage = `Added ${imageIds.length} photo${imageIds.length === 1 ? "" : "s"} to "${name}"`;
  }

  async function handleRemoveFromCollection() {
    if (activeCollectionId === null) return;
    const imageIds = [...new Set(selectedImages.map((img) => img.image_id))];
    if (imageIds.length === 0) return;
    await removeImagesFromCollection(activeCollectionId, imageIds);
    await loadManualMembership(activeCollectionId); // mutated === active here, still the right id
    await refreshCollections();
    const removedVersionIds = new Set(selectedImages.map((img) => img.version_id));
    selectedIds = new Set([...selectedIds].filter((id) => !removedVersionIds.has(id)));
    if (selectedId !== null && removedVersionIds.has(selectedId)) selectedId = null;
    statusMessage = `Removed ${imageIds.length} photo${imageIds.length === 1 ? "" : "s"} from collection`;
  }

  // M3 Slice 2: standard Lightroom Classic Library shortcuts -- 0-5 directly
  // SET star rating (0 clears, not a toggle), P/X toggle Pick/Reject on and
  // off, U always hard-clears to unflagged (a third, distinct key, not a
  // toggle of P or X), 6/7/8/9 toggle Red/Yellow/Green/Blue on and off.
  // Purple has no default key in real Lightroom, so none is bound here
  // either. Reuses handleRatingChange/handleFlagChange/handleColorLabelChange
  // with `selectedId` (the anchor) directly -- selectedId is always a
  // member of selectedIds whenever there's a real multi-selection, so their
  // existing targetVersionIds() batching applies "for free," no new
  // target-computation needed.
  const COLOR_KEYS = { 6: "red", 7: "yellow", 8: "green", 9: "blue" };

  // Comprehensive keyboard shortcut handler with custom user bindings
  function handleGlobalKeydown(/** @type {KeyboardEvent} */ e) {
    const target = e.target;
    const isTypingTarget =
      (target instanceof HTMLInputElement && target.type !== "range") ||
      target instanceof HTMLTextAreaElement ||
      target instanceof HTMLSelectElement;
    if (isTypingTarget) {
      return;
    }

    const key = e.key.toLowerCase();
    const rawKey = e.key;

    if (activeModule === "develop") {
      if (
        exportItems !== null ||
        settingsOpen ||
        backupPromptOpen ||
        creatingSnapshot ||
        creatingPreset ||
        confirmingDeletePresetId !== null
      ) {
        return;
      }
      // Undo/Redo (M3)
      if ((e.metaKey || e.ctrlKey) && !e.altKey && key === "z") {
        e.preventDefault();
        if (e.shiftKey) handleRedo();
        else handleUndo();
        return;
      }
      if (e.ctrlKey && !e.metaKey && !e.altKey && !e.shiftKey && key === "y") {
        e.preventDefault();
        handleRedo();
        return;
      }

      // Arrow navigation in Develop: navigate to previous / next photo
      if (!e.metaKey && !e.ctrlKey && !e.altKey) {
        if (rawKey === shortcuts.nextImage || rawKey === shortcuts.gridDown || rawKey === "ArrowRight") {
          e.preventDefault();
          selectNextImage(false);
          return;
        }
        if (rawKey === shortcuts.prevImage || rawKey === shortcuts.gridUp || rawKey === "ArrowLeft") {
          e.preventDefault();
          selectPrevImage(false);
          return;
        }
        if (key === shortcuts.viewGrid?.toLowerCase()) {
          e.preventDefault();
          switchModule("library");
          libraryViewMode = "grid";
          return;
        }
        if (key === shortcuts.viewLoupe?.toLowerCase()) {
          e.preventDefault();
          switchModule("library");
          libraryViewMode = "loupe";
          return;
        }
      }

      if (e.metaKey || e.ctrlKey || e.altKey) return;
      if (rawKey === " " || key === shortcuts.toggleView?.toLowerCase()) {
        e.preventDefault();
        if (!e.repeat) spacePanning = true;
        return;
      }
      if (
        key === shortcuts.toggleMaskOverlay?.toLowerCase() &&
        OVERLAY_CAPABLE_MASK_OPS.includes(selectedMask?.op ?? "")
      ) {
        e.preventDefault();
        showMaskOverlay = !showMaskOverlay;
        return;
      }
      if (key === shortcuts.toggleMaskChrome?.toLowerCase()) {
        e.preventDefault();
        maskOverlaysVisible = !maskOverlaysVisible;
        return;
      }
      if (rawKey === shortcuts.toggleOriginal || key === shortcuts.toggleOriginal?.toLowerCase()) {
        e.preventDefault();
        showOriginal = !showOriginal;
        return;
      }
      return;
    }

    if (activeModule !== "library") return;
    if (
      confirmingRemoval ||
      exportItems !== null ||
      creatingCollection ||
      creatingSmartCollection ||
      creatingCollectionWithImages ||
      settingsOpen ||
      backupPromptOpen
    ) {
      return;
    }

    // Select All: Cmd+A / Ctrl+A
    if ((e.metaKey || e.ctrlKey) && !e.altKey && key === "a") {
      e.preventDefault();
      handleSelectAll();
      return;
    }
    // Deselect All / Back to Grid: Cmd+D / Ctrl+D / Escape
    if (((e.metaKey || e.ctrlKey) && !e.altKey && key === "d") || rawKey === "Escape") {
      e.preventDefault();
      handleDeselectAll();
      return;
    }

    if (rawKey === "Delete" || rawKey === "Backspace") {
      if (selectedIds.size === 0) return;
      e.preventDefault();
      confirmingRemoval = true;
      return;
    }

    // Arrow navigation in Library
    if (rawKey === shortcuts.nextImage || rawKey === "ArrowRight") {
      e.preventDefault();
      if (libraryViewMode === "compare") {
        handleCompareNextCandidate();
      } else {
        selectNextImage(e.shiftKey);
      }
      return;
    }
    if (rawKey === shortcuts.prevImage || rawKey === "ArrowLeft") {
      e.preventDefault();
      if (libraryViewMode === "compare") {
        handleComparePrevCandidate();
      } else {
        selectPrevImage(e.shiftKey);
      }
      return;
    }
    if (rawKey === shortcuts.gridDown || rawKey === "ArrowDown") {
      e.preventDefault();
      if (libraryViewMode === "grid") {
        selectGridStep(4, e.shiftKey);
      } else {
        selectNextImage(e.shiftKey);
      }
      return;
    }
    if (rawKey === shortcuts.gridUp || rawKey === "ArrowUp") {
      e.preventDefault();
      if (libraryViewMode === "grid") {
        selectGridStep(-4, e.shiftKey);
      } else {
        selectPrevImage(e.shiftKey);
      }
      return;
    }

    if (e.metaKey || e.ctrlKey || e.altKey) return;

    // Mode hotkeys
    if (key === shortcuts.viewGrid?.toLowerCase()) {
      e.preventDefault();
      libraryViewMode = "grid";
      return;
    }
    if (key === shortcuts.viewLoupe?.toLowerCase() || rawKey === "Enter") {
      e.preventDefault();
      if (selectedId !== null) {
        libraryViewMode = "loupe";
      } else if (filteredImages.length > 0) {
        selectedId = filteredImages[0].version_id;
        selectedIds = new Set([filteredImages[0].version_id]);
        libraryViewMode = "loupe";
      }
      return;
    }
    if (key === shortcuts.viewCompare?.toLowerCase()) {
      e.preventDefault();
      libraryViewMode = "compare";
      return;
    }
    if (key === shortcuts.viewSurvey?.toLowerCase()) {
      e.preventDefault();
      libraryViewMode = "survey";
      return;
    }
    if (key === shortcuts.viewDevelop?.toLowerCase()) {
      e.preventDefault();
      if (selectedId !== null) {
        openDevelop(selectedId);
      } else if (filteredImages.length > 0) {
        openDevelop(filteredImages[0].version_id);
      }
      return;
    }
    if (rawKey === " " || key === shortcuts.toggleView?.toLowerCase()) {
      e.preventDefault();
      if (libraryViewMode === "grid") {
        if (selectedId !== null) libraryViewMode = "loupe";
      } else if (libraryViewMode === "loupe") {
        libraryViewMode = "grid";
      }
      return;
    }

    // Rating shortcuts
    if (rawKey === shortcuts.rate0) {
      e.preventDefault();
      handleRatingChange(selectedId, 0);
      return;
    }
    if (rawKey === shortcuts.rate1) {
      e.preventDefault();
      handleRatingChange(selectedId, 1);
      return;
    }
    if (rawKey === shortcuts.rate2) {
      e.preventDefault();
      handleRatingChange(selectedId, 2);
      return;
    }
    if (rawKey === shortcuts.rate3) {
      e.preventDefault();
      handleRatingChange(selectedId, 3);
      return;
    }
    if (rawKey === shortcuts.rate4) {
      e.preventDefault();
      handleRatingChange(selectedId, 4);
      return;
    }
    if (rawKey === shortcuts.rate5) {
      e.preventDefault();
      handleRatingChange(selectedId, 5);
      return;
    }

    // Flag shortcuts
    if (key === shortcuts.flagPick?.toLowerCase()) {
      e.preventDefault();
      handleFlagChange(selectedId, selectedImage?.flag === "pick" ? "none" : "pick");
      return;
    }
    if (key === shortcuts.flagReject?.toLowerCase()) {
      e.preventDefault();
      handleFlagChange(selectedId, selectedImage?.flag === "reject" ? "none" : "reject");
      return;
    }
    if (key === shortcuts.flagUnflag?.toLowerCase()) {
      e.preventDefault();
      handleFlagChange(selectedId, "none");
      return;
    }

    // Color labels
    if (key === shortcuts.colorRed?.toLowerCase()) {
      e.preventDefault();
      handleColorLabelChange(selectedId, selectedImage?.color_label === "red" ? "none" : "red");
      return;
    }
    if (key === shortcuts.colorYellow?.toLowerCase()) {
      e.preventDefault();
      handleColorLabelChange(selectedId, selectedImage?.color_label === "yellow" ? "none" : "yellow");
      return;
    }
    if (key === shortcuts.colorGreen?.toLowerCase()) {
      e.preventDefault();
      handleColorLabelChange(selectedId, selectedImage?.color_label === "green" ? "none" : "green");
      return;
    }
    if (key === shortcuts.colorBlue?.toLowerCase()) {
      e.preventDefault();
      handleColorLabelChange(selectedId, selectedImage?.color_label === "blue" ? "none" : "blue");
      return;
    }
  }

  // M4 Slice 3: releases space-pan (see spacePanning's own doc comment).
  // No input-focus/dialog guards needed here, unlike handleGlobalKeydown --
  // clearing this is always safe even if focus moved somewhere else while
  // the key was held, since spacePanning can only have been set true by
  // that same keydown handler's own guarded path in the first place.
  function handleGlobalKeyup(/** @type {KeyboardEvent} */ e) {
    if (e.key === " ") spacePanning = false;
  }

  // M2 Slice 2: IPTC fields save on blur (MetadataPanel), not debounced --
  // each is a single discrete edit rather than a slider drag, so there's no
  // flood of writes to coalesce. Still tracked via pendingIptcSave so the
  // close handler can wait for an in-flight write the same way it already
  // does for the Develop edit stack.
  function handleCaptionChange(/** @type {number} */ versionId, /** @type {string} */ caption) {
    patchLocal(versionId, { caption });
    pendingIptcSave = setCaption(versionId, caption).finally(() => {
      pendingIptcSave = null;
    });
  }

  function handleCopyrightChange(/** @type {number} */ imageId, /** @type {string} */ copyright) {
    images = images.map((img) => (img.image_id === imageId ? { ...img, copyright } : img));
    pendingIptcSave = setCopyright(imageId, copyright).finally(() => {
      pendingIptcSave = null;
    });
  }

  function handleContactChange(/** @type {number} */ imageId, /** @type {string} */ contact) {
    images = images.map((img) => (img.image_id === imageId ? { ...img, contact } : img));
    pendingIptcSave = setContact(imageId, contact).finally(() => {
      pendingIptcSave = null;
    });
  }

  async function openDevelop(/** @type {number} */ versionId) {
    // Captured before developVersionId is reassigned below -- the same
    // capture-before-reassignment shape flushEditStack itself already
    // uses, which is what keeps this race-free even if the user clicks
    // through several images in quick succession (each flush/regen closes
    // over the id it actually applies to, not whatever developVersionId
    // happens to be by the time the async work runs).
    const previousVersionId = developVersionId;
    // Awaited -- regenerate_thumbnail's own Rust command re-reads the edit
    // stack fresh from the catalog rather than trusting a client-supplied
    // one (see lib.rs's own doc comment on that command), which means it
    // could race flushEditStack's own catalog write if the two IPC calls
    // were fired back-to-back without awaiting: neither Tauri's own
    // command dispatch nor the underlying SQLite write is guaranteed to
    // land before the very next command's own read starts. A real,
    // code-verified hazard (two dependent IPC calls previously fired
    // without awaiting the first) -- not independently confirmed as a
    // reproduced user-visible symptom (an attempt to reproduce one was
    // confounded by reusing identical edit-stack values across test runs,
    // which produces an identical, correctly-unchanged content-addressed
    // thumbnail path regardless of ordering), but the same class of
    // "unawaited dependent write" bug this project already found and
    // fixed once this session (flushEditStack's own now-removed
    // persistTimer gate) -- worth closing on that precedent alone.
    await flushEditStack();
    regenerateThumbnailFor(previousVersionId);
    const image = images.find((img) => img.version_id === versionId);
    if (!image) return;
    developVersionId = versionId;
    developImagePath = image.path;
    // Cleared, not left stale, on every open -- the new image's own real
    // histogram arrives shortly via DevelopCanvas's own GPU readback, but
    // showing the PREVIOUS image's histogram in the meantime would be
    // actively misleading, not just momentarily stale.
    histogramData = null;
    hoverPixel = null;
    showClippingOverlay = false;
    // History/Snapshots (M3): re-fetched fresh on every open, not carried
    // over from whatever the previous image's panel showed -- switching
    // images via the filmstrip must never leave a stale History/Snapshots
    // list on screen for a different photo.
    const [stack, freshHistory, freshSnapshots] = await Promise.all([
      getEditStack(versionId),
      getHistory(versionId),
      getSnapshots(versionId),
    ]);
    editStack = stack;
    history = freshHistory;
    historyIndex = freshHistory.length - 1;
    snapshots = freshSnapshots;
    activeTool = null;
    selectedMaskId = null;
    activeModule = "develop";

    // Lens Corrections (M3): re-resolved fresh on every open, matching
    // History/Snapshots' own "never carry over the previous photo's data"
    // discipline above -- this photo's own EXIF, not whatever the last
    // photo's profile happened to be. A no-op (same value already baked,
    // or no match either time) skips the write entirely rather than
    // idempotently re-flushing on every single open. NOT run through
    // scheduleFlush/a history label -- this is resolved equipment data,
    // not a user-facing edit (see develop.js's own doc comment on
    // `setLensProfile`); `flushEditStack()` with no label is the same
    // silent, unlabeled persist its own doc comment already documents for
    // exactly this "idempotent no-op-content rewrite" case.
    const profile = await lookupLensProfile({
      cameraMake: image.camera_make,
      cameraModel: image.camera_model,
      lensModel: image.lens_model,
      focalLength: image.focal_length,
      aperture: image.aperture,
    });
    if (developVersionId === versionId && JSON.stringify(profile) !== JSON.stringify(lensCorrection.profile)) {
      editStack = setLensProfile(editStack, profile);
      flushEditStack();
    }
  }

  async function switchModule(/** @type {string} */ target) {
    if (activeModule === "develop" && target !== "develop") {
      // Awaited -- same unawaited-dependent-IPC-calls hazard openDevelop's
      // own flush/regen pair guards against, see that function's own doc
      // comment.
      await flushEditStack();
      regenerateThumbnailFor(developVersionId);
      activeTool = null;
      selectedMaskId = null;
    }
    activeModule = target;
  }

  // Human-readable History labels for handleAdjustmentChange's generic
  // single-scalar ops -- falls back to the raw opName (still readable
  // enough, e.g. "vibrance") for any op added later without a mapping
  // entry, rather than needing this list kept in lockstep with every op.
  const ADJUSTMENT_LABELS = /** @type {Record<string, string>} */ ({
    exposure: "Exposure",
    contrast: "Contrast",
    saturation: "Saturation",
    temperature: "Temperature",
    tint: "Tint",
    highlights: "Highlights",
    shadows: "Shadows",
    whites: "Whites",
    blacks: "Blacks",
    dehaze: "Dehaze",
    texture: "Texture",
    clarity: "Clarity",
  });

  function handleAdjustmentChange(/** @type {string} */ opName, /** @type {number} */ value) {
    editStack = upsertOp(editStack, opName, value);
    scheduleFlush(ADJUSTMENT_LABELS[opName] ?? opName);
  }

  let temperature = $derived(opValue(editStack, "temperature", 0));
  let tint = $derived(opValue(editStack, "tint", 0));
  let highlights = $derived(opValue(editStack, "highlights", 0));
  let shadows = $derived(opValue(editStack, "shadows", 0));
  let whites = $derived(opValue(editStack, "whites", 0));
  let blacks = $derived(opValue(editStack, "blacks", 0));

  // Tone Curve (M3): a global-only adjustment (applied after exposure/
  // contrast/saturation, before any mask -- see develop_engine.rs/
  // DevelopCanvas.svelte's shared ordering comment), but its payload is a
  // structured `points` array, not upsertOp's single scalar -- same
  // reason masks needed their own dedicated handler shape.
  let toneCurvePoints = $derived(getToneCurvePoints(editStack, IDENTITY_TONE_CURVE));

  function handleToneCurveChange(/** @type {readonly {x: number, y: number}[]} */ points) {
    editStack = upsertToneCurve(editStack, points);
    scheduleFlush("Tone Curve");
  }

  // HSL / Color Mixer (M3): same global-only, structured-payload shape as
  // Tone Curve above -- band-keyed, not upsertOp's single scalar.
  let hslBands = $derived(getHslBands(editStack, IDENTITY_HSL_BANDS));

  function handleHslBandChange(
    /** @type {string} */ bandName,
    /** @type {Partial<{hue: number, saturation: number, luminance: number}>} */ patch,
  ) {
    editStack = upsertHslBand(editStack, bandName, patch);
    scheduleFlush("HSL / Color Mixer");
  }

  // Split Toning (M3): same global-only shape as Tone Curve/HSL above, but
  // nested per-zone -- a per-zone UI control patches just that zone's
  // hue/saturation, leaving the other zone and balance untouched.
  let splitToning = $derived(getSplitToning(editStack, IDENTITY_SPLIT_TONING));

  function handleSplitToningZoneChange(
    /** @type {"shadows" | "highlights"} */ zone,
    /** @type {Partial<{hue: number, saturation: number}>} */ patch,
  ) {
    editStack = upsertSplitToningZone(editStack, zone, patch);
    scheduleFlush("Split Toning");
  }

  function handleSplitToningBalanceChange(/** @type {number} */ balance) {
    editStack = upsertSplitToningBalance(editStack, balance);
    scheduleFlush("Split Toning");
  }

  // Dehaze (M3): a single global scalar op (dark-channel-prior haze
  // removal), the SAME shape exposure/contrast/saturation already use --
  // reuses opValue/upsertOp/handleAdjustmentChange directly rather than a
  // dedicated getter/handler pair, since there's nothing structured about
  // its payload the generic single-scalar op model doesn't already cover.
  let dehaze = $derived(opValue(editStack, "dehaze", 0));

  // Texture & Clarity (M3): same generic single-scalar op model as Dehaze
  // above -- -100..100, no dedicated getter/handler pair needed.
  let texture = $derived(opValue(editStack, "texture", 0));
  let clarity = $derived(opValue(editStack, "clarity", 0));

  // Vignette (M3): a structured 3-field payload (amount/midpoint/feather)
  // -- same getSplitToning/upsertX shape Split Toning already established
  // for a global-only, non-single-scalar op, not the generic opValue
  // model Texture/Clarity/Dehaze use.
  let vignette = $derived(getVignette(editStack, IDENTITY_VIGNETTE));

  function handleVignetteChange(
    /** @type {Partial<{amount: number, midpoint: number, feather: number}>} */ patch,
  ) {
    editStack = upsertVignette(editStack, patch);
    scheduleFlush("Vignette");
  }

  // Lens Corrections (M3): same structured, own-getter/handler shape as
  // Vignette/Grain above, PLUS a separate profile-baking step (below,
  // called from openDevelop) -- see develop.js's own doc comment on
  // `setLensProfile` for why that's not a user-facing "change" at all,
  // and doesn't go through this handler or scheduleFlush's history label.
  let lensCorrection = $derived(getLensCorrection(editStack, IDENTITY_LENS_CORRECTION));

  function handleLensCorrectionChange(
    /** @type {Partial<{profile_enabled: boolean, distortion_amount: number, vignette_amount: number, ca_amount: number, manual_distortion: number, manual_ca: number}>} */ patch,
  ) {
    editStack = upsertLensCorrection(editStack, patch);
    scheduleFlush("Lens Corrections");
  }

  // Perspective Correction (M4): same structured, own-getter/handler shape
  // as Lens Corrections/Vignette above.
  let perspective = $derived(getPerspective(editStack, IDENTITY_PERSPECTIVE));

  function handlePerspectiveChange(
    /** @type {Partial<{vertical: number, horizontal: number, rotate: number, aspect: number, scale: number}>} */ patch,
  ) {
    editStack = upsertPerspective(editStack, patch);
    scheduleFlush("Perspective");
  }

  // Grain (M3): same structured, own-getter/handler shape as Vignette
  // above.
  let grain = $derived(getGrain(editStack, IDENTITY_GRAIN));

  function handleGrainChange(
    /** @type {Partial<{amount: number, size: number, roughness: number}>} */ patch,
  ) {
    editStack = upsertGrain(editStack, patch);
    scheduleFlush("Grain");
  }

  // Sharpening / Noise Reduction (M3): same structured, own-getter/
  // handler shape as Vignette/Grain above -- three independent ops.
  let sharpen = $derived(getSharpen(editStack, IDENTITY_SHARPEN));

  function handleSharpenChange(
    /** @type {Partial<{amount: number, radius: number, detail: number, masking: number}>} */ patch,
  ) {
    editStack = upsertSharpen(editStack, patch);
    scheduleFlush("Sharpening");
  }

  let lumaNR = $derived(getLumaNr(editStack, IDENTITY_LUMA_NR));

  function handleLumaNRChange(
    /** @type {Partial<{amount: number, detail: number, contrast: number}>} */ patch,
  ) {
    editStack = upsertLumaNr(editStack, patch);
    scheduleFlush("Luminance Noise Reduction");
  }

  let colorNR = $derived(getColorNr(editStack, IDENTITY_COLOR_NR));

  function handleColorNRChange(
    /** @type {Partial<{amount: number, detail: number}>} */ patch,
  ) {
    editStack = upsertColorNr(editStack, patch);
    scheduleFlush("Color Noise Reduction");
  }

  // Crop & Straighten (M3): same structured, own-getter/handler shape as
  // every other multi-field op above -- see develop_engine.rs's own
  // `apply_crop` doc comment for why this one has no WGSL/uniform twin.
  let crop = $derived(getCrop(editStack, IDENTITY_CROP));

  /** Ordinary field patches (drag/resize handles) pass through unchanged.
   * An ANGLE-only patch (the straighten slider, see `onCropAngleChange`
   * below) is special-cased: real Lightroom re-fits the crop rect to the
   * largest inner-fit box of the SAME aspect ratio for the new angle,
   * recentered on the image, rather than leaving the old rect in place to
   * expose the newly-rotated image's blanked-out corners (see
   * `inscribedCropForAngle`'s own doc comment for the geometry and its
   * "centered-only" scope cut -- this is that function's one caller). */
  function handleCropChange(
    /** @type {Partial<{x: number, y: number, width: number, height: number, angle: number}>} */ patch,
  ) {
    let next = patch;
    if (typeof patch.angle === "number" && patch.angle !== crop.angle && crop.width > 0 && crop.height > 0) {
      const pixelRatio = sourceWidth > 0 && sourceHeight > 0 ? (crop.width * sourceWidth) / (crop.height * sourceHeight) : null;
      const inscribed = pixelRatio ? inscribedCropForAngle(pixelRatio, sourceWidth, sourceHeight, patch.angle) : null;
      if (inscribed) next = { ...inscribed, angle: patch.angle };
    }
    // Last-resort guard against ever committing a rect that exposes the
    // rotated image's blanked-out corners (see DevelopCanvas.svelte's own
    // matching drag-time check, which is what actually stops this in the
    // interactive path -- this is the safety net for every OTHER caller of
    // handleCropChange, e.g. a future one that doesn't go through that
    // drag code at all). Falls back to the full merged rect against
    // `crop`, since `next` may be a partial patch.
    const merged = { x: crop.x, y: crop.y, width: crop.width, height: crop.height, angle: crop.angle, ...next };
    if (sourceWidth > 0 && sourceHeight > 0 && !cropRectFitsRotatedBounds(merged, sourceWidth, sourceHeight, merged.angle)) {
      return;
    }
    editStack = upsertCrop(editStack, next);
    scheduleFlush("Crop");
  }

  // Aspect-ratio lock: UI-only, NOT persisted to the edit stack (real
  // Lightroom's own crop ratio lock is a tool-state preference, not part
  // of the photo's own edit history) -- shared between DevelopCanvas.svelte
  // (corner-handle drag math) and MaskToolStrip.svelte (the preset
  // buttons' own active-state display), so it has to live here, their
  // nearest common ancestor.
  let cropAspectLock = $state(/** @type {number | null} */ (null));

  // The crop rect (crop.x/y/width/height) lives in NORMALIZED space --
  // fractions of the source image's own width/height, which are generally
  // NOT equal. A preset ratio like 1 (1:1) or 16/9 describes a PIXEL
  // aspect ratio, so it has to be corrected by the image's own native
  // aspect ratio before it's usable as a normalized width:height target --
  // otherwise "1:1" only looks square in normalized space, which is a
  // real square only when the source image itself happens to be square.
  // DevelopCanvas.svelte already tracks the decoded bitmap's pixel
  // dimensions (it needs them for the committed-crop CSS preview's own
  // `aspect-ratio` style); it reports them up here via onSourceDimensions
  // since this preset math needs them too.
  let sourceWidth = $state(0);
  let sourceHeight = $state(0);
  function handleSourceDimensions(/** @type {number} */ width, /** @type {number} */ height) {
    sourceWidth = width;
    sourceHeight = height;
  }

  // Develop histogram: fed live from DevelopCanvas's own GPU readback
  // (see that component's onHistogramUpdate/readHistogramIfIdle) --
  // deliberately NOT derived from editStack/exposure/etc. here, since the
  // actual graded pixel values (masks, curves, every spatial op) aren't
  // reproducible from JS-side state alone; DevelopCanvas is the only
  // place that ever sees the real rendered output.
  let histogramData = $state(/** @type {{r: Uint32Array, g: Uint32Array, b: Uint32Array} | null} */ (null));
  function handleHistogramUpdate(/** @type {{r: Uint32Array, g: Uint32Array, b: Uint32Array}} */ data) {
    histogramData = data;
  }

  // Histogram clipping-overlay toggle: purely a display preference (not
  // part of the edit stack), reset on openDevelop like histogramData
  // itself since it's meaningless outside a Develop session.
  let showClippingOverlay = $state(false);
  function handleToggleClippingOverlay() {
    showClippingOverlay = !showClippingOverlay;
  }

  // Histogram "value under cursor" readout, fed live from DevelopCanvas's
  // own pointer handling (see reportHoverPixel there) -- same
  // GPU-readback-can't-be-reproduced-from-JS-state reasoning as
  // histogramData above.
  let hoverPixel = $state(/** @type {{r: number, g: number, b: number} | null} */ (null));
  function handleHoverPixel(/** @type {{r: number, g: number, b: number} | null} */ rgb) {
    hoverPixel = rgb;
  }

  /** Reshapes the crop rect to the given PIXEL aspect ratio: the largest
   * rect of that ratio centered in the full image, INNER-FIT to the
   * current straighten angle (see `inscribedCropForAngle`'s doc comment --
   * at angle 0 it's identical to `largestCenteredCropForRatio`, so this
   * covers that case too without a branch). Deliberately NOT based on the
   * current rect's own size -- earlier it shrunk the current rect to fit
   * within its own previous bounding box, which (combined with the
   * uncorrected ratio) compounded into a smaller rect on every click.
   * Recomputing fresh from the full image each time is idempotent:
   * clicking the same preset twice in a row is always a no-op. `null`
   * just unlocks without reshaping anything ("Free"). */
  function handleCropAspectPreset(/** @type {number | null} */ ratio) {
    cropAspectLock = ratio;
    if (ratio === null) return;
    const next = inscribedCropForAngle(ratio, sourceWidth, sourceHeight, crop.angle);
    if (!next) return;
    handleCropChange({ ...next, angle: crop.angle });
  }

  function handleCropReset() {
    cropAspectLock = null;
    handleCropChange(IDENTITY_CROP);
  }

  // HSL band-jump eyedropper's transient navigation target -- NOT persisted
  // edit-stack state, purely a "which band should the panel scroll to and
  // highlight" signal, self-clearing after a fixed delay rather than on
  // "the next unrelated interaction" (which would mean hooking an unbounded
  // set of DOM listeners across the panel). Same fixed-timeout-reset-on-
  // retrigger idiom as persistTimer's own debounce, just for UI feedback
  // instead of persistence.
  let highlightedHslBand = $state(/** @type {string | null} */ (null));
  let hslBandHighlightTimer = /** @type {ReturnType<typeof setTimeout> | null} */ (null);

  /** Commit path for all four eyedropper destinations -- see
   * eyedropperTarget's own doc comment above for why one shared gesture
   * routes here. One-shot: resets activeTool/eyedropperTarget immediately,
   * matching handleColorRangeResampled's own "click to pick, done" model. */
  function handleEyedropperSampled(/** @type {{r: number, g: number, b: number}} */ color) {
    const target = eyedropperTarget;
    activeTool = null;
    eyedropperTarget = null;
    if (target === null) return;
    const { h, s, l } = rgbToHsl(color.r, color.g, color.b);

    if (target === "split_toning_shadows" || target === "split_toning_highlights") {
      const zone = target === "split_toning_shadows" ? "shadows" : "highlights";
      editStack = upsertSplitToningZone(editStack, zone, { hue: h, saturation: s * 100 });
      scheduleFlush("Split Toning");
      return;
    }
    if (target === "hsl_band") {
      // Navigation only -- deliberately no editStack write, no persist.
      // HSL's own sliders are relative hue/sat/lum shifts, not an absolute
      // color a sampled pixel could set; this just finds "which band".
      highlightedHslBand = nearestHslBand(h);
      if (hslBandHighlightTimer) clearTimeout(hslBandHighlightTimer);
      hslBandHighlightTimer = setTimeout(() => (highlightedHslBand = null), 1500);
      return;
    }
    if (target === "white_balance") {
      const { temperature, tint } = computeEyedropperWhiteBalance(color);
      editStack = upsertOp(editStack, "temperature", temperature);
      editStack = upsertOp(editStack, "tint", tint);
      scheduleFlush("White Balance Eyedropper");
      return;
    }
    if (target === "tone_curve_point") {
      // x comes from the sampled pixel's own lightness. Note: like every
      // eyedropper here, this samples the ORIGINAL SOURCE pixel, not the
      // graded preview (see DevelopCanvas.svelte's sampleSourcePixel doc
      // comment) -- for Tone Curve specifically this means the inserted
      // point's x itself (not just a selectivity parameter, as for the
      // other three destinations) can visibly diverge from "the tone the
      // user thinks they clicked" on a heavily-graded image. A named,
      // accepted limitation, not a bug.
      //
      // y is seeded at the curve's OWN current value at that x, so
      // insertion alone never changes the curve's visible shape until the
      // new point is dragged.
      const y = sampleCurveLut(buildToneCurveLut(toneCurvePoints), l);
      const next = insertToneCurvePoint(toneCurvePoints, l, y);
      if (next !== toneCurvePoints) {
        editStack = upsertToneCurve(editStack, next);
        scheduleFlush("Tone Curve");
      }
    }
  }

  function handleAutoWhiteBalance() {
    let avgRgb = { r: 0.5, g: 0.5, b: 0.5 };
    if (histogramData) {
      let rSum = 0,
        gSum = 0,
        bSum = 0,
        count = 0;
      for (let i = 0; i < 256; i++) {
        rSum += histogramData.r[i] * (i / 255);
        gSum += histogramData.g[i] * (i / 255);
        bSum += histogramData.b[i] * (i / 255);
        count += histogramData.r[i];
      }
      if (count > 0) {
        avgRgb = { r: rSum / count, g: gSum / count, b: bSum / count };
      }
    }
    const { temperature, tint } = computeAutoWhiteBalance(avgRgb);
    editStack = upsertOp(editStack, "temperature", temperature);
    editStack = upsertOp(editStack, "tint", tint);
    scheduleFlush("Auto White Balance");
  }

  function handleWbPresetChange(/** @type {string} */ presetKey) {
    if (presetKey === "auto") {
      handleAutoWhiteBalance();
      return;
    }
    const preset = WB_PRESETS[/** @type {keyof typeof WB_PRESETS} */ (presetKey)];
    if (!preset) return;
    editStack = upsertOp(editStack, "temperature", preset.temperature);
    editStack = upsertOp(editStack, "tint", preset.tint);
    scheduleFlush(`WB Profile: ${preset.name}`);
  }

  function handleAutoTone() {
    if (!histogramData) return;
    const tone = computeAutoTone(histogramData);
    editStack = upsertOp(editStack, "exposure", tone.exposure);
    editStack = upsertOp(editStack, "contrast", tone.contrast);
    editStack = upsertOp(editStack, "highlights", tone.highlights);
    editStack = upsertOp(editStack, "shadows", tone.shadows);
    editStack = upsertOp(editStack, "whites", tone.whites);
    editStack = upsertOp(editStack, "blacks", tone.blacks);
    scheduleFlush("Auto Tone");
  }

  async function handleExportClick() {
    // If a slider was just dragged, the debounced save may not have
    // landed yet -- flush it first so Export reads the value currently
    // on screen, not the last-persisted one. Awaited for the same
    // unawaited-dependent-IPC-calls hazard openDevelop's own flush/regen
    // pair guards against, see that function's own doc comment.
    if (activeModule === "develop") {
      await flushEditStack();
      regenerateThumbnailFor(developVersionId);
    }
    // null stays the "closed" sentinel -- never open with an empty list.
    exportItems = currentExportItems.length > 0 ? currentExportItems : null;
  }

  onMount(() => {
    // Also covers the startup catch-up pass (preview_cache::pregenerate_missing
    // / import::generate_missing_thumbnails, both run once in lib.rs's
    // .setup()): this refresh() races against that pass the same way an
    // import's own refresh() races against its own background trigger.
    refresh().then(pollUntilThumbnailsReady);
    refreshCollections();
    refreshPresets();
    listAllImageKeywords().then((assignments) => (allImageKeywords = assignments));

    // M1 Slice 6 (crash-safety): flush a pending debounced edit before the
    // window actually closes, so quitting right after a slider drag can't
    // lose it. Only intervenes when something is actually pending -- the
    // common case (nothing to flush) closes immediately, no added latency.
    // This protects a *graceful* quit only (close-button click, or another
    // OS "please close" request that routes through the same
    // closeRequested pipeline `.close()` itself uses, per Tauri's own
    // docs) -- it cannot help against SIGKILL/a hard crash, which bypasses
    // every in-process handler. Whether macOS Cmd+Q routes through this
    // same path is unverified in this environment.
    //
    // Catalog backup (PRD §7.6): settings are re-fetched fresh here, not
    // snapshotted at startup -- Tauri's own `onCloseRequested` docs show
    // `event.preventDefault()` being called *after* an `await` as the
    // canonical pattern (the wrapper awaits the whole handler before ever
    // checking `isPreventDefault()`), so there's no staleness risk to
    // engineer around by pre-fetching.
    let unlistenClose = /** @type {(() => void) | undefined} */ (undefined);
    getCurrentWindow()
      .onCloseRequested(async (event) => {
        // M3 Slice 1: force-close Settings unconditionally before any
        // backup-prompt logic below runs. SettingsDialog and
        // BackupPromptDialog share the same fixed-inset/z-index overlay
        // shell -- if both were left open at once, whichever is later in
        // DOM order would silently swallow all clicks, and the close-prompt
        // underneath could get stuck uninteractive after event.preventDefault()
        // already fired. This removes the ambiguity outright rather than
        // relying on template order as an implicit invariant.
        settingsOpen = false;

        // M2 Slice 2: an IPTC field saves on blur, so a value typed but not
        // yet blurred (e.g. the user clicks the window's close button while
        // still focused in the Caption textarea) needs to be forced to save
        // before the pending-work check below -- otherwise it's silently
        // lost, the same class of bug fixed for the Develop edit stack.
        /** @type {HTMLElement | null} */ (document.activeElement)?.blur();

        const editPending = persistTimer !== null || pendingSave !== null || pendingIptcSave !== null;
        let backupSettings = /** @type {import('$lib/api/backup.js').BackupSettings | null} */ (null);
        try {
          backupSettings = await getBackupSettings();
        } catch {
          // Treat an unreadable settings fetch as "not due" -- a backup
          // check must never block an otherwise-clean quit.
        }
        const backupDue = backupSettings !== null && isBackupDue(backupSettings);

        if (!editPending && !backupDue) return;
        event.preventDefault();

        const wasEditPending = persistTimer !== null || pendingSave !== null;
        await Promise.all([flushEditStack(), pendingIptcSave ?? Promise.resolve()]);
        // Fire-and-forget, deliberately NOT awaited: a thumbnail regen
        // abandoned by a force-quit mid-encode is a stale-until-next-flush
        // grid thumbnail, strictly lower stakes than the lost-edit bug M1
        // Slice 6 actually fixed for the edit-stack flush -- direct
        // precedent already established for generate_missing_thumbnails.
        // Blocking app quit on this would be a real regression.
        if (wasEditPending) regenerateThumbnailFor(developVersionId);

        // Always resolves -- "Skip This Time" is always available even if
        // "Back Up Now" fails, so this can never trap the user unable to quit.
        if (backupDue && backupSettings !== null) await showBackupPromptAndWait(backupSettings);

        await getCurrentWindow().destroy();
      })
      .then((fn) => {
        unlistenClose = fn;
      });

    let unlistenDragDrop = /** @type {(() => void) | undefined} */ (undefined);
    try {
      getCurrentWebview()
        .onDragDropEvent((event) => {
          if (event.payload.type === "enter" || event.payload.type === "over") {
            if (activeModule === "library") isDraggingFiles = true;
          } else if (event.payload.type === "leave") {
            isDraggingFiles = false;
          } else if (event.payload.type === "drop") {
            isDraggingFiles = false;
            if (event.payload.paths && event.payload.paths.length > 0) {
              handleDropImport(event.payload.paths);
            }
          }
        })
        .then((fn) => {
          unlistenDragDrop = fn;
        });
    } catch {
      // ignore outside Tauri
    }

    const onShortcutsUpdated = (/** @type {any} */ event) => {
      if (event.detail) shortcuts = event.detail;
    };
    window.addEventListener("shortcuts-updated", onShortcutsUpdated);

    return () => {
      unlistenClose?.();
      unlistenDragDrop?.();
      window.removeEventListener("shortcuts-updated", onShortcutsUpdated);
    };
  });
</script>

<svelte:window onkeydown={handleGlobalKeydown} onkeyup={handleGlobalKeyup} onblur={() => (spacePanning = false)} />

<div class="app">
  <div class="titlebar">
    <div class="module-switch">
      <button class:active={activeModule === "library"} onclick={() => switchModule("library")}>
        Library
      </button>
      <button class:active={activeModule === "develop"} onclick={() => switchModule("develop")}>
        Develop
      </button>
    </div>
    <div class="spacer"></div>
    {#if activeModule === "library" && activeCollectionId !== null && activeCollection && !activeCollection.is_smart}
      <button class="remove-btn" onclick={handleRemoveFromCollection} disabled={selectedIds.size === 0}>
        Remove from Collection{selectedIds.size > 1 ? ` (${selectedIds.size})` : ""}
      </button>
    {/if}
    <select
      class="add-to-collection-select"
      value=""
      disabled={activeModule !== "library" || selectedIds.size === 0}
      onchange={(e) => {
        handleAddToCollectionSelect(e.currentTarget.value);
        e.currentTarget.value = "";
      }}
    >
      <option value="" disabled>Add to Collection…</option>
      {#each manualCollections as collection (collection.id)}
        <option value={collection.id}>{collection.name}</option>
      {/each}
      <option value="__new__">New Collection…</option>
    </select>
    <select
      class="add-to-collection-select"
      value=""
      disabled={activeModule !== "library" || selectedIds.size === 0 || applyingPreset}
      onchange={(e) => {
        handleApplyPresetToSelection(e.currentTarget.value);
        e.currentTarget.value = "";
      }}
    >
      <option value="" disabled>{applyingPreset ? "Applying…" : "Apply Preset…"}</option>
      {#each presets as preset (preset.id)}
        <option value={preset.id}>{preset.name}</option>
      {/each}
    </select>
    <button
      class="remove-btn"
      onclick={() => (confirmingRemoval = true)}
      disabled={activeModule !== "library" || selectedIds.size === 0}
    >
      Remove{selectedIds.size > 1 ? ` (${selectedIds.size})` : ""}
    </button>
    <button class="export-btn" onclick={handleExportClick} disabled={currentExportItems.length === 0}>
      Export{currentExportItems.length > 1 ? ` (${currentExportItems.length})` : ""}…
    </button>
    <button class="import-btn secondary" onclick={handleImportFiles} disabled={importing}>
      {importing ? "Importing…" : "Import Files…"}
    </button>
    <button class="import-btn" onclick={handleImportFolder} disabled={importing}>
      {importing ? "Importing…" : "Import Folder…"}
    </button>
    <button class="settings-btn" title="Settings" onclick={() => (settingsOpen = true)}>⚙</button>
  </div>

  <SettingsDialog open={settingsOpen} onClose={() => (settingsOpen = false)} />

  <ExportDialog items={exportItems} onClose={() => (exportItems = null)} />

  <ConfirmDialog
    open={confirmingRemoval}
    title="Remove from catalog"
    message={`Remove ${selectedIds.size} photo${selectedIds.size === 1 ? "" : "s"} from the catalog? Source files stay on disk; edits, ratings, and metadata stored in the catalog are discarded.`}
    confirmLabel="Remove"
    onConfirm={handleRemoveConfirmed}
    onCancel={() => (confirmingRemoval = false)}
  />

  <ConfirmDialog
    open={confirmingReset}
    title="Reset all edits"
    message="Revert every adjustment and mask on this photo back to default? This can't be undone."
    confirmLabel="Reset"
    onConfirm={handleResetEditStack}
    onCancel={() => (confirmingReset = false)}
  />

  <TextPromptDialog
    open={creatingCollection}
    title="New Collection"
    label="Name"
    placeholder="e.g. Portfolio"
    confirmLabel="Create"
    onConfirm={handleCreateCollection}
    onCancel={() => (creatingCollection = false)}
  />

  <TextPromptDialog
    open={creatingCollectionWithImages}
    title="New Collection"
    label="Name"
    placeholder="e.g. Portfolio"
    confirmLabel="Create"
    onConfirm={handleCreateCollectionWithImages}
    onCancel={() => {
      creatingCollectionWithImages = false;
      pendingAddToCollectionImageIds = [];
    }}
  />

  <SmartCollectionDialog
    open={creatingSmartCollection}
    title="New Smart Collection"
    confirmLabel="Create"
    onConfirm={handleCreateSmartCollection}
    onCancel={() => (creatingSmartCollection = false)}
  />

  <TextPromptDialog
    open={creatingSnapshot}
    title="New Snapshot"
    label="Name"
    placeholder="e.g. Before crop"
    confirmLabel="Create"
    onConfirm={handleCreateSnapshotConfirmed}
    onCancel={() => (creatingSnapshot = false)}
  />

  <TextPromptDialog
    open={creatingPreset}
    title="New Preset"
    label="Name"
    placeholder="e.g. Moody B&W"
    confirmLabel="Save"
    onConfirm={handleCreatePresetConfirmed}
    onCancel={() => (creatingPreset = false)}
  />

  <ConfirmDialog
    open={confirmingDeletePresetId !== null}
    title="Delete preset"
    message="Delete this preset? This can't be undone."
    confirmLabel="Delete"
    onConfirm={handleDeletePresetConfirmed}
    onCancel={() => (confirmingDeletePresetId = null)}
  />

  {#if backupPromptSettings}
    <BackupPromptDialog
      open={backupPromptOpen}
      settings={backupPromptSettings}
      onDone={handleBackupDone}
      onSkip={handleBackupSkip}
    />
  {/if}

  {#if statusMessage}
    <div class="status">{statusMessage}</div>
  {/if}

  {#if activeModule === "library"}
    <LibraryFilterBar
      searchQuery={searchQuery}
      flagFilter={flagFilter}
      minRating={minRating}
      ratingOp={ratingOp}
      colorLabelFilter={colorLabelFilter}
      fileTypeFilter={fileTypeFilter}
      totalCount={baseImages.length}
      matchedCount={filteredImages.length}
      onSearchChange={(q) => (searchQuery = q)}
      onFlagChange={(f) => (flagFilter = f)}
      onRatingChange={(r, op) => {
        minRating = r;
        ratingOp = op;
      }}
      onColorLabelChange={(c) => (colorLabelFilter = c)}
      onFileTypeChange={(t) => (fileTypeFilter = t)}
      onReset={handleResetFilters}
    />
    <div
      class="body library-body"
      role="region"
      aria-label="Library view"
      class:drag-over={isDraggingFiles}
      ondragover={(e) => {
        e.preventDefault();
        isDraggingFiles = true;
      }}
      ondragleave={() => (isDraggingFiles = false)}
      ondrop={(e) => {
        e.preventDefault();
        isDraggingFiles = false;
      }}
    >
      {#if isDraggingFiles}
        <div class="drop-overlay">
          <div class="drop-card">
            <span class="drop-icon">📥</span>
            <span class="drop-title">Drop photos or folders to import</span>
            <span class="drop-hint">Supports RAW (.CR2, .NEF, .ARW, .DNG) and JPEG</span>
          </div>
        </div>
      {/if}

      <div class="rail">
        <div class="section-label">Catalog</div>
        <button
          type="button"
          class="tree-item"
          class:active={activeCollectionId === null && activeFolderKey === null && !showLastImportOnly}
          onclick={selectAllPhotos}
        >
          All Photos
          <span class="count">{images.length}</span>
        </button>
        <button type="button" class="tree-item" class:active={showLastImportOnly} onclick={selectLastImport}>
          Last Import
          <span class="count">{lastImportBatchId === null ? 0 : images.filter((img) => img.import_batch === lastImportBatchId).length}</span>
        </button>

        {#if folderEntries.length > 0}
          <div class="section-label folders-label">Folders</div>
          {#each folderEntries as folder (folder.key)}
            <button
              type="button"
              class="tree-item"
              class:active={activeFolderKey === folder.key}
              onclick={() => selectFolder(folder.key)}
              title={folder.key}
            >
              <span class="tree-item-name">{folder.key}</span>
              <span class="count">{folder.count}</span>
            </button>
          {/each}
        {/if}

        <div class="collections-header">
          <span class="section-label">Collections</span>
          <span class="collections-actions">
            <button type="button" class="rail-action" title="New Collection" onclick={() => (creatingCollection = true)}>+</button>
            <button type="button" class="rail-action" title="New Smart Collection" onclick={() => (creatingSmartCollection = true)}>⚡+</button>
          </span>
        </div>
        {#each collections as collection (collection.id)}
          <div class="tree-item collection-item" class:active={activeCollectionId === collection.id}>
            <button type="button" class="tree-item-main" onclick={() => selectCollection(collection.id)}>
              {#if collection.is_smart}<span class="smart-icon" title="Smart Collection">⚡</span>{/if}
              <span class="tree-item-name">{collection.name}</span>
              <span class="count">
                {collection.is_smart
                  ? images.filter((img) => matchesRules(img, collection.rules ?? [], keywordIdsByImage)).length
                  : (collection.count ?? 0)}
              </span>
            </button>
            <button
              type="button"
              class="tree-item-delete"
              aria-label="Delete collection {collection.name}"
              onclick={(e) => handleDeleteCollection(collection.id, e)}
            >×</button>
          </div>
        {/each}
      </div>

      {#if images.length === 0}
        <div class="empty">
          <p>No photos yet.</p>
          <div class="empty-actions">
            <button onclick={handleImportFolder} disabled={importing}>Import a folder…</button>
            <button class="secondary" onclick={handleImportFiles} disabled={importing}>Import files…</button>
          </div>
        </div>
      {:else if filteredImages.length === 0}
        <div class="empty">
          <p>
            {#if showLastImportOnly}
              No photos in the last import.
            {:else if activeFolderKey !== null}
              No photos in this folder.
            {:else}
              No photos in this collection.
            {/if}
          </p>
        </div>
      {:else}
        <div class="library-view-container">
          {#if libraryViewMode === "grid"}
            <LibraryGrid
              images={filteredImages}
              {selectedIds}
              onSelect={handleSelect}
              onOpen={(vid) => {
                selectedId = vid;
                selectedIds = new Set([vid]);
                libraryViewMode = "loupe";
              }}
              onRatingChange={handleRatingChange}
              onFlagChange={handleFlagChange}
              onColorLabelChange={handleColorLabelChange}
            />
          {:else if libraryViewMode === "loupe" && (selectedImage || filteredImages[0])}
            {@const currentImg = selectedImage ?? filteredImages[0]}
            {@const curIdx = filteredImages.findIndex((img) => img.version_id === currentImg.version_id)}
            <LibraryImageViewer
              bind:this={imageViewerRef}
              image={currentImg}
              hasPrev={curIdx > 0}
              hasNext={curIdx < filteredImages.length - 1}
              onPrev={() => selectPrevImage(false)}
              onNext={() => selectNextImage(false)}
              onRatingChange={(r) => handleRatingChange(currentImg.version_id, r)}
              onFlagChange={(f) => handleFlagChange(currentImg.version_id, f)}
              onColorLabelChange={(c) => handleColorLabelChange(currentImg.version_id, c)}
              onOpenDevelop={() => openDevelop(currentImg.version_id)}
              zoomLevel={libraryZoomLevel}
              onZoomChange={(z) => (libraryZoomLevel = z)}
            />
          {:else if libraryViewMode === "compare" && compareSelectImage && compareCandidateImage}
            <LibraryCompareView
              selectImage={compareSelectImage}
              candidateImage={compareCandidateImage}
              onSwap={handleCompareSwap}
              onMakeSelect={handleCompareMakeSelect}
              onNextCandidate={filteredImages.length > 1 ? handleCompareNextCandidate : undefined}
              onPrevCandidate={filteredImages.length > 1 ? handleComparePrevCandidate : undefined}
              onRatingChange={handleRatingChange}
              onFlagChange={handleFlagChange}
              onColorLabelChange={handleColorLabelChange}
            />
          {:else if libraryViewMode === "survey"}
            <LibrarySurveyView
              images={selectedImages.length > 0 ? selectedImages : filteredImages.slice(0, 4)}
              primaryId={selectedId}
              onSetPrimary={(vid) => (selectedId = vid)}
              onDeselect={(vid) => {
                const next = new Set(selectedIds);
                next.delete(vid);
                selectedIds = next;
                if (selectedId === vid) selectedId = next.size > 0 ? [...next][0] : null;
              }}
              onOpen={(vid) => openDevelop(vid)}
              onRatingChange={handleRatingChange}
              onFlagChange={handleFlagChange}
              onColorLabelChange={handleColorLabelChange}
            />
          {/if}

          <!-- Library Bottom Toolbar -->
          <LibraryToolbar
            viewMode={libraryViewMode}
            selectedCount={selectedIds.size}
            totalCount={filteredImages.length}
            zoomLevel={libraryZoomLevel}
            onViewModeChange={(m) => (libraryViewMode = m)}
            onRatingChange={(r) => handleRatingChange(null, r)}
            onFlagChange={(f) => handleFlagChange(null, f)}
            onColorLabelChange={(c) => handleColorLabelChange(null, c)}
            onZoomChange={(z) => (libraryZoomLevel = z)}
            onZoomFit={() => imageViewerRef?.zoomToFit?.()}
            onZoom100={() => imageViewerRef?.zoomTo100?.()}
          />
        </div>
      {/if}

      <MetadataPanel
        image={selectedImage}
        targetImageIds={keywordTargetImageIds}
        selectedCount={selectedIds.size}
        onRatingChange={(rating) => handleRatingChange(selectedId, rating)}
        onFlagChange={(flag) => handleFlagChange(selectedId, flag)}
        onColorLabelChange={(color) => handleColorLabelChange(selectedId, color)}
        onCaptionChange={(caption) => selectedId !== null && handleCaptionChange(selectedId, caption)}
        onCopyrightChange={(copyright) =>
          selectedImage && handleCopyrightChange(selectedImage.image_id, copyright)}
        onContactChange={(contact) =>
          selectedImage && handleContactChange(selectedImage.image_id, contact)}
        onKeywordAssigned={(name, count) =>
          (statusMessage = `Added "${name}" to ${count} photo${count === 1 ? "" : "s"}`)}
        onGeoLocationChange={(lat, lon, alt) => {
          if (selectedImage) {
            patchLocal(selectedImage.version_id, { latitude: lat, longitude: lon, altitude: alt });
            statusMessage = lat != null ? "Updated GPS coordinates" : "Removed GPS coordinates";
          }
        }}
      />
    </div>
  {:else if developImagePath}
    <div class="develop-body">
      <HistoryPanel
        {history}
        {historyIndex}
        {snapshots}
        onJumpTo={restoreTo}
        onCreateSnapshotRequest={() => (creatingSnapshot = true)}
        onRestoreSnapshot={handleRestoreSnapshot}
        onDeleteSnapshot={handleDeleteSnapshot}
      />
      <DevelopCanvas
        imagePath={developImagePath}
        imageContentHash={developImageContentHash}
        {exposure}
        {contrast}
        {saturation}
        {temperature}
        {tint}
        {highlights}
        {shadows}
        {whites}
        {blacks}
        {masks}
        {activeTool}
        {selectedMaskId}
        {brushSize}
        {brushHardness}
        {brushFlow}
        {eraseMode}
        {showMaskOverlay}
        {spotBrushSize}
        {maskOverlaysVisible}
        {showOriginal}
        {spacePanning}
        onSpotBrushSizeChange={(v) => (spotBrushSize = v)}
        onMaskCreated={handleMaskCreated}
        onMaskUpdated={handleMaskUpdated}
        onMaskSelected={(id) => (selectedMaskId = id)}
        colorRangeResampleId={colorRangeResampleTarget}
        onColorRangeResampled={handleColorRangeResampled}
        onEyedropperSampled={handleEyedropperSampled}
        {toneCurvePoints}
        {hslBands}
        {splitToning}
        {dehaze}
        {texture}
        {clarity}
        {vignette}
        {lensCorrection}
        {perspective}
        {grain}
        {sharpen}
        {lumaNR}
        {colorNR}
        {crop}
        onCropChange={handleCropChange}
        {cropAspectLock}
        onSourceDimensions={handleSourceDimensions}
        onHistogramUpdate={handleHistogramUpdate}
        {showClippingOverlay}
        onHoverPixel={handleHoverPixel}
      />
      {#if selectedMask}
        <MaskEditorPanel
          mask={selectedMask}
          onChange={(patch) => handleMaskUpdated(/** @type {string} */ (selectedMaskId), patch)}
          onDelete={handleMaskDeleted}
          onClose={() => (selectedMaskId = null)}
          {showMaskOverlay}
          onShowOverlayChange={(v) => (showMaskOverlay = v)}
          {isResamplingColor}
          onResampleColor={handleResampleColorToggle}
        />
      {/if}
      <DevelopPanel
        {histogramData}
        {showClippingOverlay}
        onToggleClippingOverlay={handleToggleClippingOverlay}
        {hoverPixel}
        {exposure}
        {contrast}
        {saturation}
        {temperature}
        {tint}
        {highlights}
        {shadows}
        {whites}
        {blacks}
        onExposureChange={(v) => handleAdjustmentChange("exposure", v)}
        onContrastChange={(v) => handleAdjustmentChange("contrast", v)}
        onSaturationChange={(v) => handleAdjustmentChange("saturation", v)}
        onTemperatureChange={(v) => handleAdjustmentChange("temperature", v)}
        onTintChange={(v) => handleAdjustmentChange("tint", v)}
        onHighlightsChange={(v) => handleAdjustmentChange("highlights", v)}
        onShadowsChange={(v) => handleAdjustmentChange("shadows", v)}
        onWhitesChange={(v) => handleAdjustmentChange("whites", v)}
        onBlacksChange={(v) => handleAdjustmentChange("blacks", v)}
        onAutoWhiteBalance={handleAutoWhiteBalance}
        onAutoTone={handleAutoTone}
        onWbPresetChange={handleWbPresetChange}
        {toneCurvePoints}
        onToneCurveChange={handleToneCurveChange}
        {hslBands}
        onHslBandChange={handleHslBandChange}
        {splitToning}
        onSplitToningZoneChange={handleSplitToningZoneChange}
        onSplitToningBalanceChange={handleSplitToningBalanceChange}
        {highlightedHslBand}
        {isEyedropperActive}
        onEyedropperToggle={handleEyedropperToggle}
        hasEdits={editStack.ops.length > 0}
        onResetRequest={() => (confirmingReset = true)}
        {dehaze}
        onDehazeChange={(v) => handleAdjustmentChange("dehaze", v)}
        {texture}
        onTextureChange={(v) => handleAdjustmentChange("texture", v)}
        {clarity}
        onClarityChange={(v) => handleAdjustmentChange("clarity", v)}
        {vignette}
        onVignetteChange={handleVignetteChange}
        {lensCorrection}
        onLensCorrectionChange={handleLensCorrectionChange}
        {perspective}
        onPerspectiveChange={handlePerspectiveChange}
        {grain}
        onGrainChange={handleGrainChange}
        {sharpen}
        onSharpenChange={handleSharpenChange}
        {lumaNR}
        onLumaNRChange={handleLumaNRChange}
        {colorNR}
        onColorNRChange={handleColorNRChange}
        {presets}
        onApplyPreset={handleApplyPreset}
        onSaveCurrentAsPresetRequest={handleSaveCurrentAsPresetRequest}
        onExportPreset={handleExportPreset}
        onDeletePresetRequest={handleDeletePresetRequest}
        onImportPresetRequest={handleImportPresetRequest}
      />
    </div>
    <MaskToolStrip
      {activeTool}
      {masks}
      {selectedMaskId}
      {brushSize}
      {brushHardness}
      {brushFlow}
      {eraseMode}
      {spotBrushSize}
      {maskOverlaysVisible}
      onToolToggle={(tool) => (activeTool = activeTool === tool ? null : tool)}
      onMaskSelect={(id) => (selectedMaskId = id)}
      onBrushSizeChange={(v) => (brushSize = v)}
      onBrushHardnessChange={(v) => (brushHardness = v)}
      onBrushFlowChange={(v) => (brushFlow = v)}
      onEraseToggle={() => (eraseMode = !eraseMode)}
      onNewBrush={() => (selectedMaskId = null)}
      onSpotBrushSizeChange={(v) => (spotBrushSize = v)}
      onNewSpot={() => (selectedMaskId = null)}
      onToggleMaskOverlaysVisible={() => (maskOverlaysVisible = !maskOverlaysVisible)}
      onCreateLuminanceRange={handleCreateLuminanceRangeMask}
      {crop}
      {cropAspectLock}
      onCropAspectPreset={handleCropAspectPreset}
      onCropAngleChange={(v) => handleCropChange({ angle: v })}
      onCropReset={handleCropReset}
    />
  {:else}
    <div class="placeholder">Double-click a photo in Library to open it here.</div>
  {/if}

  {#if activeModule === "develop" && developImagePath}
    <DevelopInfoBar imagePath={developImagePath} />
  {/if}

  {#if activeModule === "library"}
    <Filmstrip images={filteredImages} {selectedIds} onSelect={handleSelect} onOpen={openDevelop} />
  {:else if activeModule === "develop"}
    <Filmstrip
      images={developFilmstripImages}
      selectedIds={new Set(developVersionId !== null ? [developVersionId] : [])}
      onSelect={openDevelop}
      onOpen={openDevelop}
    />
  {/if}
</div>

<style>
  .app {
    display: flex;
    flex-direction: column;
    height: 100vh;
  }
  .titlebar {
    display: flex;
    align-items: center;
    gap: 14px;
    height: 42px;
    flex: none;
    padding: 0 14px;
    background: var(--bg-app);
    border-bottom: 1px solid var(--border-subtle);
  }
  .module-switch {
    display: flex;
    background: var(--bg-panel);
    border: 1px solid var(--border-subtle);
    border-radius: 6px;
    padding: 2px;
    gap: 2px;
  }
  .module-switch button {
    all: unset;
    cursor: pointer;
    padding: 5px 14px;
    font-size: 11.5px;
    font-weight: 600;
    border-radius: 4px;
    color: var(--text-secondary);
  }
  .module-switch button.active {
    background: var(--bg-panel-raised);
    color: var(--text-primary);
  }
  .spacer {
    flex: 1;
  }
  .import-btn {
    all: unset;
    cursor: pointer;
    padding: 6px 14px;
    font-size: 11.5px;
    font-weight: 600;
    border-radius: 6px;
    background: var(--accent-soft);
    color: var(--accent-strong);
    border: 1px solid var(--accent);
  }
  .import-btn:disabled {
    opacity: 0.6;
    cursor: default;
  }
  .import-btn.secondary {
    background: transparent;
    color: var(--text-secondary);
    border: 1px solid var(--border-strong);
  }
  .settings-btn {
    all: unset;
    cursor: pointer;
    width: 28px;
    height: 28px;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 14px;
    border-radius: 6px;
    color: var(--text-secondary);
    border: 1px solid var(--border-strong);
  }
  .settings-btn:hover {
    color: var(--text-primary);
  }
  .export-btn,
  .remove-btn {
    all: unset;
    cursor: pointer;
    padding: 6px 14px;
    font-size: 11.5px;
    font-weight: 600;
    border-radius: 6px;
    color: var(--text-secondary);
    border: 1px solid var(--border-strong);
  }
  .remove-btn:not(:disabled):hover {
    color: var(--label-red);
    border-color: var(--label-red);
  }
  .export-btn:disabled,
  .remove-btn:disabled {
    opacity: 0.6;
    cursor: default;
  }
  .status {
    flex: none;
    padding: 6px 14px;
    font-size: 11.5px;
    font-family: var(--font-mono);
    color: var(--text-secondary);
    background: var(--bg-panel);
    border-bottom: 1px solid var(--border-subtle);
  }
  .body,
  .develop-body {
    flex: 1;
    display: flex;
    min-height: 0;
    position: relative;
  }
  .library-view-container {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
    position: relative;
  }
  .drop-overlay {
    position: absolute;
    inset: 0;
    z-index: 100;
    background: rgba(14, 14, 18, 0.85);
    backdrop-filter: blur(4px);
    display: flex;
    align-items: center;
    justify-content: center;
    border: 2px dashed var(--accent);
    pointer-events: none;
  }
  .drop-card {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    padding: 24px 36px;
    background: var(--bg-panel-raised);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-m);
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.6);
  }
  .drop-icon {
    font-size: 32px;
  }
  .drop-title {
    font-size: 14px;
    font-weight: 600;
    color: var(--text-primary);
  }
  .drop-hint {
    font-size: 11px;
    color: var(--text-tertiary);
    font-family: var(--font-mono);
  }
  .rail {
    width: 200px;
    flex: none;
    background: var(--bg-panel);
    border-right: 1px solid var(--border-subtle);
    padding: 14px 10px;
    /* M2 Slice 5: the rail was a fixed 2-line static block until now --
       a variable-length Collections list needs to scroll instead of
       spilling past the box's bottom edge. */
    overflow-y: auto;
    overflow-x: hidden;
  }
  .section-label {
    font-family: var(--font-mono);
    font-size: 10px;
    letter-spacing: 0.09em;
    text-transform: uppercase;
    color: var(--text-tertiary);
    padding: 4px;
    font-weight: 600;
  }
  /* .tree-item is a <button> now (was a plain <div>) so "All Photos" and
     collections are real click targets -- reset button chrome so it
     still reads as the same flat row it always has. */
  .tree-item {
    all: unset;
    box-sizing: border-box;
    width: 100%;
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 5px 7px;
    border-radius: var(--radius-s);
    color: var(--text-secondary);
    font-size: 12px;
    cursor: pointer;
  }
  .tree-item.active {
    background: var(--accent-soft);
    color: var(--accent-strong);
  }
  .tree-item .count {
    margin-left: auto;
    font-family: var(--font-mono);
    font-size: 10.5px;
    color: var(--text-tertiary);
  }
  .folders-label {
    margin-top: 10px;
  }
  .collections-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding-right: 2px;
    margin-top: 10px;
  }
  .collections-actions {
    display: flex;
    gap: 2px;
  }
  .rail-action {
    all: unset;
    cursor: pointer;
    padding: 2px 5px;
    font-size: 11px;
    border-radius: var(--radius-s);
    color: var(--text-tertiary);
  }
  .rail-action:hover {
    color: var(--accent-strong);
    background: var(--accent-soft);
  }
  .collection-item {
    display: flex;
    align-items: center;
    border-radius: var(--radius-s);
  }
  .collection-item.active {
    background: var(--accent-soft);
  }
  .collection-item .tree-item-main {
    flex: 1;
    min-width: 0;
  }
  .tree-item-main {
    all: unset;
    box-sizing: border-box;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 5px 7px;
    color: var(--text-secondary);
    font-size: 12px;
    cursor: pointer;
  }
  .collection-item.active .tree-item-main {
    color: var(--accent-strong);
  }
  .tree-item-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .smart-icon {
    flex: none;
    font-size: 10px;
  }
  .tree-item-delete {
    all: unset;
    cursor: pointer;
    flex: none;
    padding: 0 7px 0 2px;
    color: var(--text-tertiary);
    opacity: 0;
  }
  .collection-item:hover .tree-item-delete {
    opacity: 1;
  }
  .tree-item-delete:hover {
    color: var(--label-red);
  }
  .add-to-collection-select {
    all: unset;
    box-sizing: border-box;
    cursor: pointer;
    padding: 6px 10px;
    font-size: 11.5px;
    font-weight: 600;
    border-radius: 6px;
    color: var(--text-secondary);
    border: 1px solid var(--border-strong);
  }
  .add-to-collection-select:disabled {
    opacity: 0.6;
    cursor: default;
  }
  .empty {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 12px;
    color: var(--text-secondary);
  }
  .empty-actions {
    display: flex;
    gap: 8px;
  }
  .empty button {
    all: unset;
    cursor: pointer;
    padding: 8px 16px;
    font-size: 12px;
    font-weight: 600;
    border-radius: 6px;
    background: var(--accent-soft);
    color: var(--accent-strong);
    border: 1px solid var(--accent);
  }
  .empty button.secondary {
    background: transparent;
    color: var(--text-secondary);
    border: 1px solid var(--border-strong);
  }
  .placeholder {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-tertiary);
    font-family: var(--font-mono);
    font-size: 12px;
  }
</style>
