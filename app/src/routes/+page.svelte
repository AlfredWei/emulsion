<script>
  import "$lib/styles/tokens.css";
  import { open, save } from "@tauri-apps/plugin-dialog";
  import { convertFileSrc } from "@tauri-apps/api/core";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";
  import AppTitlebar from "$lib/components/AppTitlebar.svelte";
  import AppDialogs from "$lib/components/AppDialogs.svelte";
  import StatusStrip from "$lib/components/StatusStrip.svelte";
  import LibraryFilterBar from "$lib/components/LibraryFilterBar.svelte";
  import LibraryGrid from "$lib/components/LibraryGrid.svelte";
  import DevelopCanvas from "$lib/components/DevelopCanvas.svelte";
  import DevelopPanel from "$lib/components/DevelopPanel.svelte";
  import MaskToolStrip from "$lib/components/MaskToolStrip.svelte";
  import MaskEditorPanel from "$lib/components/MaskEditorPanel.svelte";
  import MetadataPanel from "$lib/components/MetadataPanel.svelte";
  import Filmstrip from "$lib/components/Filmstrip.svelte";
  import DevelopInfoBar from "$lib/components/DevelopInfoBar.svelte";
  import HistoryPanel from "$lib/components/HistoryPanel.svelte";
  import LibraryToolbar from "$lib/components/LibraryToolbar.svelte";
  import LibraryImageViewer from "$lib/components/LibraryImageViewer.svelte";
  import LibraryCompareView from "$lib/components/LibraryCompareView.svelte";
  import LibrarySurveyView from "$lib/components/LibrarySurveyView.svelte";
  import PrintPanel from "$lib/components/PrintPanel.svelte";
  import PrintLayoutView from "$lib/components/PrintLayoutView.svelte";
  import CatalogRail from "$lib/components/CatalogRail.svelte";
  import { shell } from "$lib/state/shell.svelte.js";
  import { print } from "$lib/state/print.svelte.js";
  import { faces } from "$lib/state/faces.svelte.js";
  import { importFlow } from "$lib/state/importFlow.svelte.js";
  import { library } from "$lib/state/library.svelte.js";
  import { selection } from "$lib/state/selection.svelte.js";
  import { createKeyboardHandlers } from "$lib/keyboard.js";
  import { createMenuHandler } from "$lib/menuActions.js";
  import {
    setRating,
    setFlag,
    setColorLabel,
    setCaption,
    setCopyright,
    setContact,
    removeImages,
    listAllImageKeywords,
    updateSmartCollectionRules,
  } from "$lib/api/catalog.js";
  import {
    getEditStack,
    setEditStack,
    getHistory,
    restoreHistoryEntry,
    previewHistoryEntry,
    addSnapshot,
    getSnapshots,
    restoreSnapshot,
    previewSnapshot,
    previewEditStack,
    deleteSnapshot,
    presetEligibleOps,
    applyPresetOps,
    copySettingsOps,
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
    getSoftProofPreview,
  } from "$lib/api/develop.js";
  import { flushThumbnailBatch } from "$lib/thumbnailBatchQueue.js";
  import { largestCenteredCropForRatio, inscribedCropForAngle, cropRectFitsRotatedBounds } from "$lib/cropMath.js";
  import { getBackupSettings, isBackupDue } from "$lib/api/backup.js";
  import { handleChoosePrintCustomProfile, handlePrint, handleExportPdf } from "$lib/actions/printActions.js";
  import {
    refreshPeople,
    handleRenamePerson,
    handleReassignFace,
    handleCreatePersonAndTagFace,
    handleSetFaceExcluded,
    handleCancelFaceDetection,
    refreshCurrentImageFaces,
    runFaceDetection,
    handleDetectFacesForSelected,
    handleDetectFacesForSelection,
    handleDetectFacesForFolder,
  } from "$lib/actions/faceActions.js";
  import { handleBackupDone, handleBackupSkip } from "$lib/actions/backupActions.js";
  import {
    loadPersonMembership,
    selectPerson,
    selectAllPhotos,
    selectLastImport,
    selectFolder,
    refreshCollections,
    handleResetFilters,
    selectCollection,
    refresh,
    patchLocal,
    handleBatchThumbnailsComplete,
    prioritizeThumbnail,
    handleCreateCollection,
    handleCreateSmartCollection,
    handleDeleteCollection,
  } from "$lib/actions/libraryActions.js";
  import {
    targetVersionIds,
    selectGridStep,
    handleSelectAll,
    handleDeselectAll,
    handleSelect,
    handleCompareNextCandidate,
    handleComparePrevCandidate,
    handleCompareSwap,
    handleCompareMakeSelect,
  } from "$lib/actions/selectionActions.js";
  import {
    handleAddToCollectionSelect,
    handleCreateCollectionWithImages,
    handleRemoveFromCollection,
  } from "$lib/actions/collectionsActions.js";
  import {
    runImport,
    handleImportFolder,
    handleImportFiles,
    handleDropImport,
    handleMergeHdrBracket,
    handleMergePanorama,
    regenerateThumbnailFor,
    pollUntilThumbnailsReadyOnStartup,
  } from "$lib/actions/importActions.js";

  let imageViewerRef = $state(/** @type {any} */ (null));

  // Presets (M3): global, catalog-wide, same "fetch once at startup, keep
  // in sync locally" shape as the library store's `collections` -- NOT re-fetched per
  // image the way history/snapshots are, since presets have no relation
  // to whichever photo happens to be open.
  let presets = $state(/** @type {import('$lib/api/develop.js').PresetEntry[]} */ ([]));


  async function refreshPresets() {
    presets = await listPresets();
  }


  // The Filmstrip shows filtered images, falling back if active Develop photo is excluded
  let developFilmstripImages = $derived(
    library.filteredImages.some((img) => img.version_id === developVersionId) ? library.filteredImages : library.images,
  );


  let developVersionId = $state(/** @type {number | null} */ (null));
  let developImagePath = $state("");
  // M4 Smart Previews: derived, not a second $state var kept manually in
  // sync with developImagePath -- stays correct automatically as `images`
  // updates, and there's exactly one place (`openDevelop` below) that ever
  // needs to change which image is open anyway.
  let developImageContentHash = $derived(
    library.images.find((img) => img.version_id === developVersionId)?.content_hash ?? null,
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

  // M4 Soft Proofing: ephemeral view state, never persisted into the edit
  // stack -- same "plain view state, not saved via handleAdjustmentChange/
  // scheduleFlush" treatment as maskOverlaysVisible/showOriginal above.
  // `softProofPreviewUrl`/`softProofLoading` are populated by the debounced
  // effect below and passed straight through to DevelopCanvas for display.
  let softProofEnabled = $state(false);
  let softProofTarget = $state(
    /** @type {"srgb" | "adobe-rgb" | "prophoto-rgb" | "custom"} */ ("srgb"),
  );
  let softProofCustomProfilePath = $state(/** @type {string | null} */ (null));
  let softProofIntent = $state(
    /** @type {"perceptual" | "relative" | "saturation" | "absolute"} */ ("relative"),
  );
  let softProofGamutWarning = $state(false);
  let softProofPreviewUrl = $state(/** @type {string | null} */ (null));
  let softProofLoading = $state(false);
  let softProofProfileLabel = $derived(
    softProofTarget === "adobe-rgb"
      ? "Adobe RGB"
      : softProofTarget === "prophoto-rgb"
        ? "ProPhoto RGB"
        : softProofTarget === "custom"
          ? (softProofCustomProfilePath?.split(/[\\/]/).pop() ?? "Custom Profile")
          : "sRGB",
  );

  // M5 Slice 1: GPU/CPU fallback. `gpuFallbackActive` is reported up by
  // DevelopCanvas's own `onGpuFallback` callback the moment its WebGPU
  // device acquisition fails (or, symmetrically, flips back false if a
  // later image's acquisition succeeds) -- this component never probes
  // `navigator.gpu` itself. `cpuFallbackPreviewUrl` is populated by the
  // debounced effect below, same "compute a static preview CPU-side and
  // hand DevelopCanvas a URL" shape as `softProofPreviewUrl` above, just
  // driven by GPU availability instead of a proofing toggle.
  let gpuFallbackActive = $state(false);
  let cpuFallbackPreviewUrl = $state(/** @type {string | null} */ (null));
  function handleGpuFallback(/** @type {boolean} */ active) {
    gpuFallbackActive = active;
    // A mask/crop tool selected before GPU became unavailable would
    // otherwise linger as "active" while its own panel/handles never
    // render (MaskToolStrip's buttons are disabled going forward, but
    // this clears whatever was already selected).
    if (active) activeTool = null;
  }

  /** Mirrors the existing single-file picker precedent (`handleImportPresetRequest`
   * below), just with an ICC/ICM extension filter instead of JSON. Selecting a
   * new custom profile also switches `softProofTarget` to "custom" -- picking
   * a file only to leave a different profile active would be confusing. */
  async function handleChooseCustomProfile() {
    const path = await open({ multiple: false, filters: [{ name: "ICC Profile", extensions: ["icc", "icm"] }] });
    if (!path || Array.isArray(path)) return;
    softProofCustomProfilePath = path;
    softProofTarget = "custom";
  }

  // Print module (M4, final scope item): ephemeral view state, same
  // "never persisted into the edit stack" treatment as Soft Proof's own
  // state above -- nothing here is part of a photo's saved edits.
  // `printItems` is a snapshot of Library's selection (or the open Develop
  // image) taken when entering Print (see switchModule), matching how
  // Develop snapshots its own open image while Library's grid is hidden.


  // Populated by handlePrint right before window.print() -- swapped into
  // PrintLayoutView's <img> src in place of the live (lower-resolution)
  // layout preview, so the actual OS print dialog sees the real
  // full-resolution, color-managed payload.


  // Debounced (same 250ms settle as scheduleFlush below, a separate timer
  // for a separate purpose -- this refetches a PREVIEW, it never writes
  // anything) fetch of the soft-proofed preview whenever proofing is on and
  // anything it depends on changes: the edit stack (so proofing reflects
  // the CURRENT graded look, not a stale one), which image is open, or the
  // proof settings themselves. Off (or no image open) just clears the
  // preview -- DevelopCanvas falls back to its own live WGSL render then.
  let softProofTimer = /** @type {ReturnType<typeof setTimeout> | null} */ (null);
  $effect(() => {
    void editStack;
    const versionId = developVersionId;
    const enabled = softProofEnabled;
    const target = softProofTarget;
    const customPath = softProofCustomProfilePath;
    const intent = softProofIntent;
    const gamutWarning = softProofGamutWarning;

    if (softProofTimer) clearTimeout(softProofTimer);

    if (!enabled || versionId === null) {
      softProofPreviewUrl = null;
      softProofLoading = false;
      return;
    }

    softProofTimer = setTimeout(() => {
      softProofLoading = true;
      getSoftProofPreview(versionId, {
        target,
        custom_profile_path: customPath,
        intent,
        gamut_warning: gamutWarning,
      })
        .then((preview) => {
          softProofPreviewUrl = convertFileSrc(preview.path);
        })
        .catch(() => {
          softProofPreviewUrl = null;
        })
        .finally(() => {
          softProofLoading = false;
        });
    }, 250);

    return () => {
      if (softProofTimer) clearTimeout(softProofTimer);
    };
  });

  // M5 Slice 1: GPU/CPU fallback preview -- same debounced-CPU-render shape
  // as the soft-proof effect just above (250ms settle, re-fires on any
  // `editStack` change), driven by `gpuFallbackActive` instead of a
  // proofing toggle. `previewEditStack` (the same CPU pipeline M4.5's
  // History/Preset hover-preview already uses) is called with the FULL
  // current `editStack`, since this is the primary canvas content in
  // fallback mode, not a peek -- unlike `schedulePreview`'s 120ms
  // hover-tuned debounce, this matches `scheduleFlush`'s own 250ms
  // "settled after a drag" window, since re-rendering CPU-side on every
  // slider tick is exactly the cost GPU was chosen to avoid.
  let gpuFallbackTimer = /** @type {ReturnType<typeof setTimeout> | null} */ (null);
  $effect(() => {
    void editStack;
    const path = developImagePath;
    const contentHash = developImageContentHash;
    const active = gpuFallbackActive;
    const stack = editStack;

    if (gpuFallbackTimer) clearTimeout(gpuFallbackTimer);

    if (!active || path === null) {
      cpuFallbackPreviewUrl = null;
      return;
    }

    gpuFallbackTimer = setTimeout(() => {
      previewEditStack(path, contentHash, stack)
        .then((preview) => {
          cpuFallbackPreviewUrl = convertFileSrc(preview.path);
        })
        .catch(() => {
          cpuFallbackPreviewUrl = null;
        });
    }, 250);

    return () => {
      if (gpuFallbackTimer) clearTimeout(gpuFallbackTimer);
    };
  });

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
  async function handleApplyPreset(/** @type {number} */ presetId) {
    clearPreview();
    if (developVersionId === null) return;
    const preset = presets.find((p) => p.id === presetId);
    if (!preset) return;
    const versionId = developVersionId;
    editStack = applyPresetOps(editStack, preset.edit_stack);
    // Same immediate-regen pattern restoreTo/handleRestoreSnapshot already
    // follow -- this is a jump-to-a-different-look commit, not a slider
    // drag, so the Library grid thumbnail shouldn't have to wait for the
    // "leaving Develop" checkpoint (switchModule/openDevelop) to catch up.
    // Awaited first, same unawaited-dependent-IPC-calls hazard openDevelop's
    // own flush/regen pair guards against (regenerateThumbnailFor re-reads
    // the edit stack fresh from the catalog, so it must not race the write
    // it's meant to reflect).
    await flushEditStack(`Apply Preset: ${preset.name}`);
    regenerateThumbnailFor(versionId);
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
      shell.notify(`Exported "${preset.name}"`);
    } catch (/** @type {any} */ e) {
      shell.notify(`Export preset failed: ${e}`);
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
      shell.notify(`Imported "${raw.name}"`);
    } catch (/** @type {any} */ e) {
      shell.notify(`Import preset failed: ${e}`);
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
    const targets = [...selection.selectedIds];
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
      shell.notify(`Applied "${preset.name}" to ${targets.length} photo${targets.length === 1 ? "" : "s"}`);
    } catch (/** @type {any} */ e) {
      shell.notify(`Apply preset failed: ${e}`);
    } finally {
      applyingPreset = false;
    }
  }

  // Copy/Paste Settings (M4.5): an unsaved, in-memory analog of Presets --
  // "Copy Settings" snapshots a filtered subset of the CURRENTLY OPEN
  // Develop image's own edit stack (via copySettingsOps, over the exact
  // same preset-eligible op universe presetEligibleOps already defines);
  // "Paste Settings" applies that snapshot with the SAME applyPresetOps
  // upsert-by-name merge Presets use, so it carries the identical
  // whole-op-replace limitation documented there. Deliberately reuses
  // this machinery rather than inventing a second merge strategy.
  let copiedSettings = $state(/** @type {import('$lib/api/develop.js').EditStack | null} */ (null));
  let copySettingsDialogOpen = $state(false);

  function handleCopySettingsRequest() {
    if (developVersionId === null) return;
    copySettingsDialogOpen = true;
  }

  function handleCopySettingsConfirmed(/** @type {string[]} */ groupIds) {
    copySettingsDialogOpen = false;
    copiedSettings = copySettingsOps(editStack, groupIds);
    shell.notify("Copied settings");
  }

  /** The Copy/Paste Settings buttons live at the bottom of DevelopPanel
   * (Develop-only), so paste there only ever targets the image currently
   * open in Develop -- an immediate, discrete action (like Apply
   * Preset), flushed right away rather than going through the slider
   * debounce. */
  async function handlePasteSettings() {
    if (!copiedSettings || developVersionId === null) return;
    const versionId = developVersionId;
    editStack = applyPresetOps(editStack, copiedSettings);
    // Same immediate-regen reasoning (and awaited-first ordering) as
    // handleApplyPreset's own comment.
    await flushEditStack("Paste Settings");
    regenerateThumbnailFor(versionId);
    shell.notify("Pasted settings");
  }

  // Guards "Paste Settings to Selection" while a batch paste is in
  // flight, same narrow race-mitigation purpose as applyingPreset above.
  let pastingSettingsToSelection = $state(false);

  /** M4.5 batch apply: applies the SAME in-memory clipboard Copy
   * Settings filled (not a Preset) across every Library-selected image
   * in one action. Mirrors handleApplyPresetToSelection's exact shape --
   * frontend-orchestrated, non-atomic-across-the-batch getEditStack ->
   * applyPresetOps merge -> setEditStack -> regenerateThumbnail per
   * target, with the same re-sync-if-the-open-Develop-image-was-a-target
   * guard -- rather than inventing a second batch pattern. */
  async function handlePasteSettingsToSelection() {
    if (!copiedSettings) return;
    const targets = [...selection.selectedIds];
    if (targets.length === 0) return;
    const settingsToApply = copiedSettings;
    pastingSettingsToSelection = true;
    try {
      await Promise.all(
        targets.map(async (versionId) => {
          const current = await getEditStack(versionId);
          const merged = applyPresetOps(current, settingsToApply);
          await setEditStack(versionId, merged, "Paste Settings");
          const path = await regenerateThumbnail(versionId);
          if (path) patchLocal(versionId, { thumbnail_path: path });
        }),
      );
      if (developVersionId !== null && targets.includes(developVersionId)) {
        editStack = await getEditStack(developVersionId);
      }
      shell.notify(`Pasted settings to ${targets.length} photo${targets.length === 1 ? "" : "s"}`);
    } catch (/** @type {any} */ e) {
      shell.notify(`Paste settings failed: ${e}`);
    } finally {
      pastingSettingsToSelection = false;
    }
  }


  // People/Faces: reload the current photo's detected faces whenever the
  // single-image selection changes (image_id, not version_id -- faces are
  // per-image, per RFC-0005 §3.4, so switching between virtual copies of
  // the same source doesn't need a refetch). Cleared, not stale, when
  // nothing is selected.
  $effect(() => {
    void selection.selectedImage?.image_id;
    refreshCurrentImageFaces();
  });

  // What Export would act on right now: the open Develop image, or every
  // selected Library image (M2 Slice 3 batch export -- the frontend-only
  // follow-up M1 Slice 5's export_batch was explicitly built to accept).
  let currentExportItems = $derived.by(() => {
    if (shell.activeModule === "develop" && developVersionId !== null) {
      return [{ path: developImagePath, version_id: developVersionId }];
    }
    if (selection.selectedImages.length > 0) {
      return selection.selectedImages.map((img) => ({ path: img.path, version_id: img.version_id }));
    }
    return selection.selectedImage ? [{ path: selection.selectedImage.path, version_id: selection.selectedImage.version_id }] : [];
  });
  let exportItems = $state(/** @type {{ path: string, version_id: number }[] | null} */ (null));



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

  // History/Snapshot/Preset hover-preview (M4.5): a small rendered
  // thumbnail in the rail's own preview box, NOT a swap of the live
  // `editStack` -- an earlier version of this feature did swap `editStack`
  // (reusing DevelopCanvas's existing reactive WebGPU render for free),
  // but that meant every hover drove the full interactive canvas pipeline,
  // the same cost as a real edit, just to preview one. This instead
  // reuses the existing CPU-rendered "graded" preview tier
  // (`preview_cache::ensure_graded_preview_for_hash`, the same one
  // Library's Loupe view already uses) at draft resolution, cached by
  // content hash + stack hash -- cheap after the first hover of any given
  // entry, and never touches the main canvas at all. `previewToken`
  // invalidates a still-in-flight render once the user has moved to a
  // different row (or left) before it resolves; debounced so quickly
  // scanning across rows doesn't fire one render per row passed over.
  let previewUrl = $state(/** @type {string | null} */ (null));
  let previewToken = 0;
  let previewDebounceTimer = /** @type {ReturnType<typeof setTimeout> | null} */ (null);

  /** mouseleave handler, AND the first thing every real commit path
   * (restoreTo, handleRestoreSnapshot, handleApplyPreset) calls -- a click
   * can land before a stray mouseleave fires, and the stale preview
   * thumbnail should disappear the instant the click actually commits,
   * not linger until the pointer happens to leave. */
  function clearPreview() {
    previewToken++;
    if (previewDebounceTimer !== null) {
      clearTimeout(previewDebounceTimer);
      previewDebounceTimer = null;
    }
    previewUrl = null;
  }

  function schedulePreview(/** @type {() => Promise<import('$lib/api/develop.js').DevelopPreviewInfo>} */ fetchPreview) {
    if (developImagePath === null) return;
    const token = ++previewToken;
    if (previewDebounceTimer !== null) clearTimeout(previewDebounceTimer);
    previewDebounceTimer = setTimeout(() => {
      fetchPreview()
        .then((info) => {
          if (token === previewToken) previewUrl = convertFileSrc(info.path);
        })
        .catch(() => {});
    }, 120);
  }

  function handlePeekHistory(/** @type {number} */ index) {
    if (developVersionId === null || developImagePath === null || index < 0 || index >= history.length) return;
    const versionId = developVersionId;
    const entryId = history[index].id;
    const path = developImagePath;
    const contentHash = developImageContentHash;
    schedulePreview(() => previewHistoryEntry(versionId, entryId, path, contentHash));
  }

  function handlePeekSnapshot(/** @type {number} */ snapshotId) {
    if (developVersionId === null || developImagePath === null) return;
    const versionId = developVersionId;
    const path = developImagePath;
    const contentHash = developImageContentHash;
    schedulePreview(() => previewSnapshot(versionId, snapshotId, path, contentHash));
  }

  function handlePeekPreset(/** @type {number} */ presetId) {
    if (developImagePath === null) return;
    const preset = presets.find((p) => p.id === presetId);
    if (!preset) return;
    const mergedStack = applyPresetOps(editStack, preset.edit_stack);
    const path = developImagePath;
    const contentHash = developImageContentHash;
    schedulePreview(() => previewEditStack(path, contentHash, mergedStack));
  }

  /** Moves the live edit stack to `history[index]` -- undo, redo, and a
   * History-panel row click are all this same call, just with a
   * different `index`. See `history`/`historyIndex`'s own doc comment for
   * why this needs no server-side cursor concept at all. */
  async function restoreTo(/** @type {number} */ index) {
    clearPreview();
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
    clearPreview();
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
    if (library.filteredImages.length === 0) return;
    if (selection.selectedId === null) {
      const first = library.filteredImages[0];
      selection.selectedId = first.version_id;
      selection.selectedIds = new Set([first.version_id]);
      return;
    }
    const idx = library.filteredImages.findIndex((img) => img.version_id === selection.selectedId);
    if (idx === -1) {
      const first = library.filteredImages[0];
      selection.selectedId = first.version_id;
      selection.selectedIds = new Set([first.version_id]);
      return;
    }
    if (idx < library.filteredImages.length - 1) {
      const nextImg = library.filteredImages[idx + 1];
      if (extend) {
        const next = new Set(selection.selectedIds);
        next.add(nextImg.version_id);
        selection.selectedIds = next;
        selection.selectedId = nextImg.version_id;
      } else {
        selection.selectedId = nextImg.version_id;
        selection.selectedIds = new Set([nextImg.version_id]);
      }
      if (shell.activeModule === "develop") {
        openDevelop(nextImg.version_id);
      }
    }
  }

  function selectPrevImage(/** @type {boolean=} */ extend) {
    if (library.filteredImages.length === 0) return;
    if (selection.selectedId === null) {
      const last = library.filteredImages[library.filteredImages.length - 1];
      selection.selectedId = last.version_id;
      selection.selectedIds = new Set([last.version_id]);
      return;
    }
    const idx = library.filteredImages.findIndex((img) => img.version_id === selection.selectedId);
    if (idx === -1) {
      const first = library.filteredImages[0];
      selection.selectedId = first.version_id;
      selection.selectedIds = new Set([first.version_id]);
      return;
    }
    if (idx > 0) {
      const prevImg = library.filteredImages[idx - 1];
      if (extend) {
        const next = new Set(selection.selectedIds);
        next.add(prevImg.version_id);
        selection.selectedIds = next;
        selection.selectedId = prevImg.version_id;
      } else {
        selection.selectedId = prevImg.version_id;
        selection.selectedIds = new Set([prevImg.version_id]);
      }
      if (shell.activeModule === "develop") {
        openDevelop(prevImg.version_id);
      }
    }
  }


  // Non-destructive removal (M2 Slice 3): catalog rows + app-owned derived
  // files only -- the backend never touches source files. `await` the
  // command BEFORE filtering local state: the other order would let an
  // in-flight pollUntilThumbnailsReady refresh() momentarily resurrect the
  // removed rows in the UI.
  async function handleRemoveConfirmed() {
    library.confirmingRemoval = false;
    // Symmetry with the close handler: force any in-progress IPTC edit's
    // blur-save to fire before the rows it targets can disappear.
    /** @type {HTMLElement | null} */ (document.activeElement)?.blur();

    const imageIds = [...new Set(selection.selectedImages.map((img) => img.image_id))];
    if (imageIds.length === 0) return;
    try {
      await removeImages(imageIds);
    } catch (/** @type {any} */ e) {
      shell.notify(`Remove failed: ${e}`);
      return;
    }
    const removedVersionIds = new Set(selection.selectedImages.map((img) => img.version_id));
    library.images = library.images.filter((img) => !removedVersionIds.has(img.version_id));
    shell.notify(`Removed ${imageIds.length} photo${imageIds.length === 1 ? "" : "s"} from catalog`);
    selection.selectedId = null;
    selection.selectedIds = new Set();
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




  // Everything the keyboard and menu handlers (lib/keyboard.js, lib/menuActions.js) read or
  // write. State goes through live getters/setters -- never copied values -- so the handlers
  // see exactly what the component sees at the moment an event fires; functions are the
  // component's own (hoisted) handlers. RFC-0009 P2: this object is the seam P4-P7 replace
  // with store fields.
  const handlerContext = {
    get activeModule() {
      return shell.activeModule;
    },
    get backupPromptOpen() {
      return importFlow.backupPromptOpen;
    },
    get confirmingDeletePresetId() {
      return confirmingDeletePresetId;
    },
    get confirmingRemoval() {
      return library.confirmingRemoval;
    },
    set confirmingRemoval(value) {
      library.confirmingRemoval = value;
    },
    get creatingCollection() {
      return library.creatingCollection;
    },
    get creatingCollectionWithImages() {
      return library.creatingCollectionWithImages;
    },
    get creatingPreset() {
      return creatingPreset;
    },
    get creatingSmartCollection() {
      return library.creatingSmartCollection;
    },
    get creatingSnapshot() {
      return creatingSnapshot;
    },
    get exportItems() {
      return exportItems;
    },
    get filteredImages() {
      return library.filteredImages;
    },
    handleColorLabelChange,
    handleCompareNextCandidate,
    handleComparePrevCandidate,
    handleCopySettingsRequest,
    handleDeselectAll,
    handleExportClick,
    handleExportPdf,
    handleFlagChange,
    handleImportFiles,
    handleImportFolder,
    handlePasteSettings,
    handleRatingChange,
    handleRedo,
    handleSelectAll,
    handleToggleClippingOverlay,
    handleUndo,
    get libraryViewMode() {
      return library.libraryViewMode;
    },
    set libraryViewMode(value) {
      library.libraryViewMode = value;
    },
    get maskOverlaysVisible() {
      return maskOverlaysVisible;
    },
    set maskOverlaysVisible(value) {
      maskOverlaysVisible = value;
    },
    openDevelop,
    get selectedId() {
      return selection.selectedId;
    },
    set selectedId(value) {
      selection.selectedId = value;
    },
    get selectedIds() {
      return selection.selectedIds;
    },
    set selectedIds(value) {
      selection.selectedIds = value;
    },
    get selectedImage() {
      return selection.selectedImage;
    },
    get selectedMask() {
      return selectedMask;
    },
    selectGridStep,
    selectNextImage,
    selectPrevImage,
    get settingsOpen() {
      return shell.settingsOpen;
    },
    set settingsOpen(value) {
      shell.settingsOpen = value;
    },
    get shortcuts() {
      return shell.shortcuts;
    },
    get showMaskOverlay() {
      return showMaskOverlay;
    },
    set showMaskOverlay(value) {
      showMaskOverlay = value;
    },
    get showOriginal() {
      return showOriginal;
    },
    set showOriginal(value) {
      showOriginal = value;
    },
    get spacePanning() {
      return spacePanning;
    },
    set spacePanning(value) {
      spacePanning = value;
    },
    switchModule,
  };
  const { handleGlobalKeydown, handleGlobalKeyup } = createKeyboardHandlers(handlerContext);
  const handleMenuAction = createMenuHandler(handlerContext);


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
    library.images = library.images.map((img) => (img.image_id === imageId ? { ...img, copyright } : img));
    pendingIptcSave = setCopyright(imageId, copyright).finally(() => {
      pendingIptcSave = null;
    });
  }

  function handleContactChange(/** @type {number} */ imageId, /** @type {string} */ contact) {
    library.images = library.images.map((img) => (img.image_id === imageId ? { ...img, contact } : img));
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
    const image = library.images.find((img) => img.version_id === versionId);
    if (!image) return;
    prioritizeThumbnail(versionId);
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
    shell.activeModule = "develop";

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
    if (shell.activeModule === "develop" && target !== "develop") {
      // Awaited -- same unawaited-dependent-IPC-calls hazard openDevelop's
      // own flush/regen pair guards against, see that function's own doc
      // comment.
      await flushEditStack();
      regenerateThumbnailFor(developVersionId);
      activeTool = null;
      selectedMaskId = null;
    }
    if (target === "print") {
      // Snapshot what Print will act on -- same source `currentExportItems`
      // already derives (open Develop image, else Library's selection) --
      // taken once on entry so the print job doesn't silently change out
      // from under the user if they alter Library's selection afterward
      // (Library's own grid is hidden while inside Print, same as Develop).
      print.items = currentExportItems;
      print.readyUrls = {};
    }
    if (target === "people") {
      refreshPeople();
    }
    shell.activeModule = target;
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
    if (shell.activeModule === "develop") {
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
    refresh().then(pollUntilThumbnailsReadyOnStartup);
    refreshCollections();
    refreshPresets();
    listAllImageKeywords().then((assignments) => (library.allImageKeywords = assignments));

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
        shell.settingsOpen = false;

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
        // Blocking app quit on this would be a real regression. Flush the
        // batch queue to ensure pending regens start immediately, but don't
        // wait for them to complete.
        if (wasEditPending) {
          regenerateThumbnailFor(developVersionId);
          flushThumbnailBatch(handleBatchThumbnailsComplete);
        }

        // Always resolves -- "Skip This Time" is always available even if
        // "Back Up Now" fails, so this can never trap the user unable to quit.
        if (backupDue && backupSettings !== null) await importFlow.showBackupPromptAndWait(backupSettings);

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
            if (shell.activeModule === "library") importFlow.isDraggingFiles = true;
          } else if (event.payload.type === "leave") {
            importFlow.isDraggingFiles = false;
          } else if (event.payload.type === "drop") {
            importFlow.isDraggingFiles = false;
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
      if (event.detail) shell.shortcuts = event.detail;
    };
    window.addEventListener("shortcuts-updated", onShortcutsUpdated);

    // Native OS menu bar (M4.5 Slice 4): lib.rs's `on_menu_event` re-emits
    // every click as a plain `"menu-action"` event, same
    // try/catch-outside-Tauri precedent as the drag-drop listener above.
    let unlistenMenu = /** @type {(() => void) | undefined} */ (undefined);
    try {
      listen("menu-action", (/** @type {{ payload: string }} */ event) => {
        handleMenuAction(event.payload);
      }).then((fn) => {
        unlistenMenu = fn;
      });
    } catch {
      // ignore outside Tauri
    }

    // Import progress bar: import_folder/import_files (lib.rs) emit this
    // once per file as they walk a folder or file list -- see import.rs's
    // `import_paths_with_progress`. Same try/catch-outside-Tauri precedent
    // as the listeners above.
    let unlistenImportProgress = /** @type {(() => void) | undefined} */ (undefined);
    try {
      listen("import-progress", (/** @type {{ payload: { current: number, total: number } }} */ event) => {
        importFlow.catalogProgress = event.payload;
      }).then((fn) => {
        unlistenImportProgress = fn;
      });
    } catch {
      // ignore outside Tauri
    }

    // Thumbnail-backfill progress bar, second phase of the same bar
    // (runImport awaits backfillMissingThumbnails right after import
    // itself resolves) -- lib.rs's backfill_missing_thumbnails emits this
    // once per image via generate_missing_thumbnails_with_progress.
    let unlistenThumbnailProgress = /** @type {(() => void) | undefined} */ (undefined);
    try {
      listen("thumbnail-progress", (/** @type {{ payload: { current: number, total: number } }} */ event) => {
        importFlow.thumbnailProgress = event.payload;
      }).then((fn) => {
        unlistenThumbnailProgress = fn;
      });
    } catch {
      // ignore outside Tauri
    }

    // HDR merge progress bar: lib.rs's merge_hdr_bracket emits this once
    // per pipeline step (see hdrMergeProgress's own doc comment above).
    let unlistenHdrMergeProgress = /** @type {(() => void) | undefined} */ (undefined);
    try {
      listen("hdr-merge-progress", (/** @type {{ payload: { current: number, total: number } }} */ event) => {
        importFlow.hdrMergeProgress = event.payload;
      }).then((fn) => {
        unlistenHdrMergeProgress = fn;
      });
    } catch {
      // ignore outside Tauri
    }

    // Face-detection progress bar, third phase of the same import bar
    // (runImport now awaits detectFacesForImportBatch right after
    // thumbnail backfill resolves, same "stay visible until genuinely
    // done" treatment as thumbnails got) -- lib.rs's
    // detect_faces_for_import_batch emits this once per image.
    let unlistenFaceDetectionProgress = /** @type {(() => void) | undefined} */ (undefined);
    try {
      listen("face-detection-progress", (/** @type {{ payload: { current: number, total: number } }} */ event) => {
        importFlow.faceDetectionProgress = event.payload;
      }).then((fn) => {
        unlistenFaceDetectionProgress = fn;
      });
    } catch {
      // ignore outside Tauri
    }

    // On-demand detection's own progress (MetadataPanel's "Face" button,
    // Library's multi-select/folder batch actions) -- a distinct event
    // from import's own "face-detection-progress" above so the two
    // progress displays can never cross-talk (see runFaceDetection's own
    // doc comment).
    let unlistenFaceScanProgress = /** @type {(() => void) | undefined} */ (undefined);
    try {
      listen("face-detection-scan-progress", (/** @type {{ payload: { current: number, total: number } }} */ event) => {
        faces.scanProgress = event.payload;
      }).then((fn) => {
        unlistenFaceScanProgress = fn;
      });
    } catch {
      // ignore outside Tauri
    }

    return () => {
      unlistenClose?.();
      unlistenDragDrop?.();
      unlistenMenu?.();
      unlistenImportProgress?.();
      unlistenThumbnailProgress?.();
      unlistenHdrMergeProgress?.();
      unlistenFaceDetectionProgress?.();
      unlistenFaceScanProgress?.();
      window.removeEventListener("shortcuts-updated", onShortcutsUpdated);
    };
  });


</script>

<svelte:window onkeydown={handleGlobalKeydown} onkeyup={handleGlobalKeyup} onblur={() => (spacePanning = false)} />

<div class="app">
  <AppTitlebar
    activeModule={shell.activeModule}
    {currentExportItems}
    activeCollectionId={library.activeCollectionId}
    activeCollection={library.activeCollection}
    selectedIds={selection.selectedIds}
    manualCollections={library.manualCollections}
    {applyingPreset}
    {presets}
    {copiedSettings}
    {pastingSettingsToSelection}
    mergingHdr={importFlow.mergingHdr}
    mergingPanorama={importFlow.mergingPanorama}
    detectingFaces={faces.detectingFaces}
    faceScanProgress={faces.scanProgress}
    faceDetectionCancelable={faces.detectionCancelable}
    showFaceRects={faces.showFaceRects}
    importing={importFlow.importing}
    {switchModule}
    {handleRemoveFromCollection}
    {handleAddToCollectionSelect}
    {handleApplyPresetToSelection}
    {handlePasteSettingsToSelection}
    {handleMergeHdrBracket}
    {handleMergePanorama}
    {handleCancelFaceDetection}
    {handleDetectFacesForSelection}
    {handleDetectFacesForFolder}
    {handleExportClick}
    {handleImportFiles}
    {handleImportFolder}
    onToggleFaceRects={() => (faces.showFaceRects = !faces.showFaceRects)}
    onRequestRemoval={() => (library.confirmingRemoval = true)}
    onOpenSettings={() => (shell.settingsOpen = true)}
  />

  <AppDialogs
    settingsOpen={shell.settingsOpen}
    {exportItems}
    {copySettingsDialogOpen}
    confirmingFaceDetectionOnImport={faces.confirmingDetectionOnImport}
    pendingImportBatchSize={faces.pendingImportBatchSize}
    confirmingRemoval={library.confirmingRemoval}
    selectedIds={selection.selectedIds}
    {confirmingReset}
    creatingCollection={library.creatingCollection}
    creatingCollectionWithImages={library.creatingCollectionWithImages}
    creatingSmartCollection={library.creatingSmartCollection}
    {creatingSnapshot}
    {creatingPreset}
    {confirmingDeletePresetId}
    backupPromptSettings={importFlow.backupPromptSettings}
    backupPromptOpen={importFlow.backupPromptOpen}
    onCloseSettings={() => (shell.settingsOpen = false)}
    onCloseExport={() => (exportItems = null)}
    onCancelCopySettings={() => (copySettingsDialogOpen = false)}
    onCancelRemoval={() => (library.confirmingRemoval = false)}
    onCancelReset={() => (confirmingReset = false)}
    onCancelCreateCollection={() => (library.creatingCollection = false)}
    onCancelCreateCollectionWithImages={() => {
      library.creatingCollectionWithImages = false;
      library.pendingAddToCollectionImageIds = [];
    }}
    onCancelCreateSmartCollection={() => (library.creatingSmartCollection = false)}
    onCancelCreateSnapshot={() => (creatingSnapshot = false)}
    onCancelCreatePreset={() => (creatingPreset = false)}
    onCancelDeletePreset={() => (confirmingDeletePresetId = null)}
    {handleCopySettingsConfirmed}
    handleFaceDetectionPromptConfirm={faces.confirmDetectionPrompt}
    handleFaceDetectionPromptCancel={faces.cancelDetectionPrompt}
    {handleRemoveConfirmed}
    {handleResetEditStack}
    {handleCreateCollection}
    {handleCreateCollectionWithImages}
    {handleCreateSmartCollection}
    {handleCreateSnapshotConfirmed}
    {handleCreatePresetConfirmed}
    {handleDeletePresetConfirmed}
    {handleBackupDone}
    {handleBackupSkip}
  />

  <StatusStrip
    importing={importFlow.importing}
    importPhase={importFlow.phase}
    thumbnailProgress={importFlow.thumbnailProgress}
    faceDetectionProgress={importFlow.faceDetectionProgress}
    importProgress={importFlow.catalogProgress}
    mergingHdr={importFlow.mergingHdr}
    hdrMergeProgress={importFlow.hdrMergeProgress}
    statusMessage={shell.statusMessage}
  />

  {#if shell.activeModule === "library"}
    <LibraryFilterBar
      searchQuery={library.searchQuery}
      flagFilter={library.flagFilter}
      minRating={library.minRating}
      ratingOp={library.ratingOp}
      colorLabelFilter={library.colorLabelFilter}
      fileTypeFilter={library.fileTypeFilter}
      cameraFilter={library.cameraFilter}
      lensFilter={library.lensFilter}
      dateFrom={library.dateFrom}
      dateTo={library.dateTo}
      cameraOptions={library.cameraOptions}
      lensOptions={library.lensOptions}
      totalCount={library.baseImages.length}
      matchedCount={library.filteredImages.length}
      onSearchChange={(q) => (library.searchQuery = q)}
      onFlagChange={(f) => (library.flagFilter = f)}
      onRatingChange={(r, op) => {
        library.minRating = r;
        library.ratingOp = op;
      }}
      onColorLabelChange={(c) => (library.colorLabelFilter = c)}
      onFileTypeChange={(t) => (library.fileTypeFilter = t)}
      onCameraChange={(c) => (library.cameraFilter = c)}
      onLensChange={(l) => (library.lensFilter = l)}
      onDateRangeChange={(from, to) => {
        library.dateFrom = from;
        library.dateTo = to;
      }}
      onReset={handleResetFilters}
    />
    <div
      class="body library-body"
      role="region"
      aria-label="Library view"
      class:drag-over={importFlow.isDraggingFiles}
      ondragover={(e) => {
        e.preventDefault();
        importFlow.isDraggingFiles = true;
      }}
      ondragleave={() => (importFlow.isDraggingFiles = false)}
      ondrop={(e) => {
        e.preventDefault();
        importFlow.isDraggingFiles = false;
      }}
    >
      {#if importFlow.isDraggingFiles}
        <div class="drop-overlay">
          <div class="drop-card">
            <span class="drop-icon">📥</span>
            <span class="drop-title">Drop photos or folders to import</span>
            <span class="drop-hint">Supports RAW (.CR2, .NEF, .ARW, .DNG) and JPEG</span>
          </div>
        </div>
      {/if}

      <CatalogRail
        images={library.images}
        activeCollectionId={library.activeCollectionId}
        activeFolderKey={library.activeFolderKey}
        showLastImportOnly={library.showLastImportOnly}
        activePersonId={library.activePersonId}
        lastImportBatchId={library.lastImportBatchId}
        folderEntries={library.folderEntries}
        collections={library.collections}
        keywordIdsByImage={library.keywordIdsByImage}
        people={faces.people}
        avatarUrls={faces.avatarSourceUrls}
        onSelectAllPhotos={selectAllPhotos}
        onSelectLastImport={selectLastImport}
        onSelectFolder={selectFolder}
        onSelectCollection={selectCollection}
        onDeleteCollection={handleDeleteCollection}
        onCreateCollection={() => (library.creatingCollection = true)}
        onCreateSmartCollection={() => (library.creatingSmartCollection = true)}
        onSelectPerson={selectPerson}
        onRenamePerson={handleRenamePerson}
      />

      {#if library.images.length === 0}
        <div class="empty">
          <p>No photos yet.</p>
          <div class="empty-actions">
            <button onclick={handleImportFolder} disabled={importFlow.importing}>Import a folder…</button>
            <button class="secondary" onclick={handleImportFiles} disabled={importFlow.importing}>Import files…</button>
          </div>
        </div>
      {:else if library.filteredImages.length === 0}
        <div class="empty">
          <p>
            {#if library.showLastImportOnly}
              No photos in the last import.
            {:else if library.activeFolderKey !== null}
              No photos in this folder.
            {:else if library.activePersonId !== null}
              No photos of this person.
            {:else}
              No photos in this collection.
            {/if}
          </p>
        </div>
      {:else}
        <div class="library-view-container">
          {#if library.libraryViewMode === "grid"}
            <LibraryGrid
              images={library.filteredImages}
              selectedIds={selection.selectedIds}
              onSelect={handleSelect}
              onOpen={(vid) => {
                selection.selectedId = vid;
                selection.selectedIds = new Set([vid]);
                library.libraryViewMode = "loupe";
                prioritizeThumbnail(vid);
              }}
              onRatingChange={handleRatingChange}
              onFlagChange={handleFlagChange}
              onColorLabelChange={handleColorLabelChange}
            />
          {:else if library.libraryViewMode === "loupe" && (selection.selectedImage || library.filteredImages[0])}
            {@const currentImg = selection.selectedImage ?? library.filteredImages[0]}
            {@const curIdx = library.filteredImages.findIndex((img) => img.version_id === currentImg.version_id)}
            <LibraryImageViewer
              bind:this={imageViewerRef}
              image={currentImg}
              hasPrev={curIdx > 0}
              hasNext={curIdx < library.filteredImages.length - 1}
              onPrev={() => selectPrevImage(false)}
              onNext={() => selectNextImage(false)}
              onRatingChange={(r) => handleRatingChange(currentImg.version_id, r)}
              onFlagChange={(f) => handleFlagChange(currentImg.version_id, f)}
              onColorLabelChange={(c) => handleColorLabelChange(currentImg.version_id, c)}
              onOpenDevelop={() => openDevelop(currentImg.version_id)}
              zoomLevel={library.libraryZoomLevel}
              onZoomChange={(z) => (library.libraryZoomLevel = z)}
              faces={currentImg.image_id === selection.selectedImage?.image_id ? faces.currentImageFaces : []}
              showFaceRects={faces.showFaceRects}
              hoveredFaceId={faces.hoveredFaceId}
            />
          {:else if library.libraryViewMode === "compare" && selection.compareSelectImage && selection.compareCandidateImage}
            <LibraryCompareView
              selectImage={selection.compareSelectImage}
              candidateImage={selection.compareCandidateImage}
              onSwap={handleCompareSwap}
              onMakeSelect={handleCompareMakeSelect}
              onNextCandidate={library.filteredImages.length > 1 ? handleCompareNextCandidate : undefined}
              onPrevCandidate={library.filteredImages.length > 1 ? handleComparePrevCandidate : undefined}
              onRatingChange={handleRatingChange}
              onFlagChange={handleFlagChange}
              onColorLabelChange={handleColorLabelChange}
            />
          {:else if library.libraryViewMode === "survey"}
            <LibrarySurveyView
              images={selection.selectedImages.length > 0 ? selection.selectedImages : library.filteredImages.slice(0, 4)}
              primaryId={selection.selectedId}
              onSetPrimary={(vid) => (selection.selectedId = vid)}
              onDeselect={(vid) => {
                const next = new Set(selection.selectedIds);
                next.delete(vid);
                selection.selectedIds = next;
                if (selection.selectedId === vid) selection.selectedId = next.size > 0 ? [...next][0] : null;
              }}
              onOpen={(vid) => openDevelop(vid)}
              onRatingChange={handleRatingChange}
              onFlagChange={handleFlagChange}
              onColorLabelChange={handleColorLabelChange}
            />
          {/if}

          <!-- Library Bottom Toolbar -->
          <LibraryToolbar
            viewMode={library.libraryViewMode}
            selectedCount={selection.selectedIds.size}
            totalCount={library.filteredImages.length}
            zoomLevel={library.libraryZoomLevel}
            onViewModeChange={(m) => (library.libraryViewMode = m)}
            onRatingChange={(r) => handleRatingChange(null, r)}
            onFlagChange={(f) => handleFlagChange(null, f)}
            onColorLabelChange={(c) => handleColorLabelChange(null, c)}
            onZoomChange={(z) => (library.libraryZoomLevel = z)}
            onZoomFit={() => imageViewerRef?.zoomToFit?.()}
            onZoom100={() => imageViewerRef?.zoomTo100?.()}
          />
        </div>
      {/if}

      <MetadataPanel
        image={selection.selectedImage}
        targetImageIds={selection.keywordTargetImageIds}
        selectedCount={selection.selectedIds.size}
        onRatingChange={(rating) => handleRatingChange(selection.selectedId, rating)}
        onFlagChange={(flag) => handleFlagChange(selection.selectedId, flag)}
        onColorLabelChange={(color) => handleColorLabelChange(selection.selectedId, color)}
        onCaptionChange={(caption) => selection.selectedId !== null && handleCaptionChange(selection.selectedId, caption)}
        onCopyrightChange={(copyright) =>
          selection.selectedImage && handleCopyrightChange(selection.selectedImage.image_id, copyright)}
        onContactChange={(contact) =>
          selection.selectedImage && handleContactChange(selection.selectedImage.image_id, contact)}
        onKeywordAssigned={(name, count) =>
          (shell.notify(`Added "${name}" to ${count} photo${count === 1 ? "" : "s"}`))}
        onGeoLocationChange={(lat, lon, alt) => {
          if (selection.selectedImage) {
            patchLocal(selection.selectedImage.version_id, { latitude: lat, longitude: lon, altitude: alt });
            shell.notify(lat != null ? "Updated GPS coordinates" : "Removed GPS coordinates");
          }
        }}
        onGeoLocationApplied={(imageIds, lat, lon) => {
          const ids = new Set(imageIds);
          library.images = library.images.map((img) => (ids.has(img.image_id) ? { ...img, latitude: lat, longitude: lon } : img));
          shell.notify(`Set location for ${imageIds.length} photo${imageIds.length === 1 ? "" : "s"}`);
        }}
        faces={faces.currentImageFaces}
        people={faces.people}
        detectingFaces={faces.detectingFaces}
        faceDetectionProgress={faces.scanProgress}
        hoveredFaceId={faces.hoveredFaceId}
        onHoverFace={(faceId) => (faces.hoveredFaceId = faceId)}
        onDetectFaces={handleDetectFacesForSelected}
        onTagFace={handleReassignFace}
        onCreateAndTagFace={handleCreatePersonAndTagFace}
        onRenamePerson={handleRenamePerson}
        onSetFaceExcluded={handleSetFaceExcluded}
      />
    </div>
  {:else if shell.activeModule === "develop" && developImagePath}
    <div class="develop-body">
      <HistoryPanel
        {history}
        {historyIndex}
        {snapshots}
        onJumpTo={restoreTo}
        onCreateSnapshotRequest={() => (creatingSnapshot = true)}
        onRestoreSnapshot={handleRestoreSnapshot}
        onDeleteSnapshot={handleDeleteSnapshot}
        {presets}
        onApplyPreset={handleApplyPreset}
        onSaveCurrentAsPresetRequest={handleSaveCurrentAsPresetRequest}
        onExportPreset={handleExportPreset}
        onDeletePresetRequest={handleDeletePresetRequest}
        onImportPresetRequest={handleImportPresetRequest}
        onPeekHistory={handlePeekHistory}
        onPeekSnapshot={handlePeekSnapshot}
        onPeekPreset={handlePeekPreset}
        onPeekEnd={clearPreview}
        {previewUrl}
        width={shell.panelWidths.history}
      />
      <div
        class="panel-resize-handle"
        role="separator"
        aria-orientation="vertical"
        aria-label="Resize History panel"
        onpointerdown={(e) => shell.handlePanelResizePointerDown(e, "history")}
        onpointermove={shell.handlePanelResizePointerMove}
        onpointerup={shell.handlePanelResizePointerUp}
      ></div>
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
        {softProofEnabled}
        {softProofPreviewUrl}
        {softProofLoading}
        {softProofProfileLabel}
        {cpuFallbackPreviewUrl}
        onGpuFallback={handleGpuFallback}
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
      <div
        class="panel-resize-handle"
        role="separator"
        aria-orientation="vertical"
        aria-label="Resize adjustments panel"
        onpointerdown={(e) => shell.handlePanelResizePointerDown(e, "develop")}
        onpointermove={shell.handlePanelResizePointerMove}
        onpointerup={shell.handlePanelResizePointerUp}
      ></div>
      <DevelopPanel
        width={shell.panelWidths.develop}
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
        {softProofEnabled}
        {softProofTarget}
        {softProofCustomProfilePath}
        {softProofIntent}
        {softProofGamutWarning}
        onSoftProofEnabledChange={(v) => (softProofEnabled = v)}
        onSoftProofTargetChange={(v) => (softProofTarget = /** @type {typeof softProofTarget} */ (v))}
        onSoftProofIntentChange={(v) => (softProofIntent = /** @type {typeof softProofIntent} */ (v))}
        onSoftProofGamutWarningChange={(v) => (softProofGamutWarning = v)}
        onChooseCustomProfile={handleChooseCustomProfile}
        onCopySettingsRequest={handleCopySettingsRequest}
        canPasteSettings={copiedSettings !== null}
        onPasteSettingsRequest={handlePasteSettings}
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
      gpuUnavailable={gpuFallbackActive}
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
  {:else if shell.activeModule === "print"}
    <div class="print-body">
      <PrintLayoutView
        items={print.items}
        template={print.template}
        fitMode={print.fitMode}
        rows={print.rows}
        cols={print.cols}
        cellSpacing={print.cellSpacing}
        paperSize={print.paperSize}
        orientation={print.orientation}
        margins={print.margins}
        colorManagement={print.colorManagementSettings}
        printReadyUrls={print.readyUrls}
      />
      <PrintPanel
        itemCount={print.items.length}
        template={print.template}
        onTemplateChange={(v) => (print.template = v)}
        fitMode={print.fitMode}
        onFitModeChange={(v) => (print.fitMode = v)}
        rows={print.rows}
        cols={print.cols}
        onRowsChange={(v) => (print.rows = v)}
        onColsChange={(v) => (print.cols = v)}
        cellSpacing={print.cellSpacing}
        onCellSpacingChange={(v) => (print.cellSpacing = v)}
        paperSize={print.paperSize}
        onPaperSizeChange={(v) => (print.paperSize = v)}
        orientation={print.orientation}
        onOrientationChange={(v) => (print.orientation = v)}
        margins={print.margins}
        onMarginChange={(side, v) => (print.margins = { ...print.margins, [side]: v })}
        colorManaged={print.colorManaged}
        onColorManagedChange={(v) => (print.colorManaged = v)}
        profileTarget={print.profileTarget}
        onProfileTargetChange={(v) => (print.profileTarget = /** @type {typeof print.profileTarget} */ (v))}
        customProfilePath={print.customProfilePath}
        onChooseCustomProfile={handleChoosePrintCustomProfile}
        intent={print.intent}
        onIntentChange={(v) => (print.intent = /** @type {typeof print.intent} */ (v))}
        printing={print.printing}
        onPrint={handlePrint}
        exportingPdf={print.exportingPdf}
        onExportPdf={handleExportPdf}
      />
    </div>
  {:else}
    <div class="placeholder">Double-click a photo in Library to open it here.</div>
  {/if}

  {#if shell.activeModule === "develop" && developImagePath}
    <DevelopInfoBar imagePath={developImagePath} />
  {/if}

  {#if shell.activeModule === "library"}
    <Filmstrip images={library.filteredImages} selectedIds={selection.selectedIds} onSelect={handleSelect} onOpen={openDevelop} />
  {:else if shell.activeModule === "develop"}
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
  .body,
  .develop-body,
  .print-body {
    flex: 1;
    display: flex;
    min-height: 0;
    position: relative;
  }
  /* M4.5: drag handle between Develop's History/adjustments rails and the
     canvas. A 1px visible line sits inside a wider invisible hit area
     (negative margin) so the actual pixel-precise drag target is easier
     to grab than a bare 1px border would be. */
  .panel-resize-handle {
    flex: none;
    width: 1px;
    margin: 0 -3px;
    padding: 0 3px;
    background-clip: content-box;
    background-color: transparent;
    cursor: ew-resize;
    position: relative;
    z-index: 5;
  }
  .panel-resize-handle:hover,
  .panel-resize-handle:active {
    background-color: var(--accent);
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
