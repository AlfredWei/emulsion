<script>
  import "$lib/styles/tokens.css";
  import { open, save } from "@tauri-apps/plugin-dialog";
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
  import { develop } from "$lib/state/develop.svelte.js";
  import { developView } from "$lib/state/developView.svelte.js";
  import { masks } from "$lib/state/masks.svelte.js";
  import { softProof } from "$lib/state/softProof.svelte.js";
  import { presets } from "$lib/state/presets.svelte.js";
  import { createKeyboardHandlers } from "$lib/keyboard.js";
  import { createMenuHandler } from "$lib/menuActions.js";
  import {
    listAllImageKeywords,
  } from "$lib/api/catalog.js";
  import {
    getEditStack,
    setEditStack,
    getHistory,
    restoreHistoryEntry,
    getSnapshots,
    restoreSnapshot,
    previewEditStack,
    presetEligibleOps,
    applyPresetOps,
    copySettingsOps,
    createPreset,
    listPresets,
    deletePreset,
    importPresetFile,
    exportPresetFile,
    regenerateThumbnail,
    upsertOp,
    resetEditStack,
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
    upsertToneCurve,
    buildToneCurveLut,
    sampleCurveLut,
    insertToneCurvePoint,
    nearestHslBand,
    upsertSplitToningZone,
    rgbToHsl,
    setLensProfile,
    lookupLensProfile,
    computeEyedropperWhiteBalance,
  } from "$lib/api/develop.js";
  import { flushThumbnailBatch } from "$lib/thumbnailBatchQueue.js";
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
    handleDetectFacesForSelected,
    handleDetectFacesForSelection,
    handleDetectFacesForFolder,
  } from "$lib/actions/faceActions.js";
  import { handleBackupDone, handleBackupSkip } from "$lib/actions/backupActions.js";
  import {
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
    handleRemoveConfirmed,
  } from "$lib/actions/libraryActions.js";
  import {
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
    handleImportFolder,
    handleImportFiles,
    handleDropImport,
    handleMergeHdrBracket,
    handleMergePanorama,
    regenerateThumbnailFor,
    pollUntilThumbnailsReadyOnStartup,
  } from "$lib/actions/importActions.js";
  import { handleRatingChange, handleFlagChange, handleColorLabelChange, handleCaptionChange, handleCopyrightChange, handleContactChange } from "$lib/actions/metadataActions.js";
  import { handleAdjustmentChange, handleToneCurveChange, handleHslBandChange, handleSplitToningZoneChange, handleSplitToningBalanceChange, handleVignetteChange, handleLensCorrectionChange, handlePerspectiveChange, handleGrainChange, handleSharpenChange, handleLumaNRChange, handleColorNRChange, handleCropChange, handleCropAspectPreset, handleCropReset, handleAutoWhiteBalance, handleWbPresetChange, handleAutoTone, handleSourceDimensions, handleHistogramUpdate, handleToggleClippingOverlay, handleHoverPixel, handlePeekHistory, handlePeekSnapshot, handleCreateSnapshot, handleDeleteSnapshot } from "$lib/actions/developActions.js";

  let imageViewerRef = $state(/** @type {any} */ (null));


  async function refreshPresets() {
    presets.list = await listPresets();
  }


  // The Filmstrip shows filtered images, falling back if active Develop photo is excluded
  let developFilmstripImages = $derived(
    library.filteredImages.some((img) => img.version_id === develop.versionId) ? library.filteredImages : library.images,
  );


  function handleGpuFallback(/** @type {boolean} */ active) {
    develop.gpuFallbackActive = active;
    // A mask/crop tool selected before GPU became unavailable would
    // otherwise linger as "active" while its own panel/handles never
    // render (MaskToolStrip's buttons are disabled going forward, but
    // this clears whatever was already selected).
    if (active) masks.activeTool = null;
  }

  /** Mirrors the existing single-file picker precedent (`handleImportPresetRequest`
   * below), just with an ICC/ICM extension filter instead of JSON. Selecting a
   * new custom profile also switches `softProof.target` to "custom" -- picking
   * a file only to leave a different profile active would be confusing. */
  async function handleChooseCustomProfile() {
    const path = await open({ multiple: false, filters: [{ name: "ICC Profile", extensions: ["icc", "icm"] }] });
    if (!path || Array.isArray(path)) return;
    softProof.customProfilePath = path;
    softProof.target = "custom";
  }

  // Print module (M4, final scope item): ephemeral view state, same
  // "never persisted into the edit stack" treatment as Soft Proof's own
  // state (softProof store) -- nothing here is part of a photo's saved edits.
  // `printItems` is a snapshot of Library's selection (or the open Develop
  // image) taken when entering Print (see switchModule), matching how
  // Develop snapshots its own open image while Library's grid is hidden.


  // Populated by handlePrint right before window.print() -- swapped into
  // PrintLayoutView's <img> src in place of the live (lower-resolution)
  // layout preview, so the actual OS print dialog sees the real
  // full-resolution, color-managed payload.


  // Store effects (RFC-0009 §3.5): registered here, once, at the point in component init where
  // the inline effects used to be.
  softProof.install();
  develop.installCpuFallback();
  masks.install();

  /** Toggle symmetry with MaskToolStrip's own onToolToggle: clicking the
   * eyedropper again while already resampling cancels it, matching how
   * clicking an active tool button a second time turns it off. */
  function handleResampleColorToggle() {
    if (masks.isResamplingColor) {
      masks.activeTool = null;
      masks.colorRangeResampleTarget = null;
      return;
    }
    if (masks.selectedMaskId === null) return;
    masks.activeTool = "color_range";
    masks.colorRangeResampleTarget = masks.selectedMaskId;
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
    develop.editStack = updateMask(develop.editStack, id, { refColor });
    masks.colorRangeResampleTarget = null;
    masks.activeTool = null;
    develop.scheduleFlush("Adjust Color Range");
  }

  function isEyedropperActive(/** @type {typeof masks.eyedropperTarget} */ target) {
    return masks.activeTool === "eyedropper" && masks.eyedropperTarget === target;
  }

  /** Toggle symmetry with handleResampleColorToggle above: clicking an
   * active eyedropper button again cancels it. */
  function handleEyedropperToggle(/** @type {typeof masks.eyedropperTarget} */ target) {
    if (masks.activeTool === "eyedropper" && masks.eyedropperTarget === target) {
      masks.activeTool = null;
      masks.eyedropperTarget = null;
      return;
    }
    masks.activeTool = "eyedropper";
    masks.eyedropperTarget = target;
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
    develop.editStack = addMask(develop.editStack, mask);
    masks.selectedMaskId = mask.id;
    // Real Lightroom drops back to selection after placing a gradient, but
    // a brush stroke should keep the Brush tool active (painting is
    // inherently multi-stroke -- see DevelopCanvas.svelte's brush-state
    // doc comment) rather than force a re-click of the tool for every dab.
    // Spot removal is now also a painted stroke (M4 Slice 2), so it stays
    // active the same way; only color range and the gradients are one-shot
    // placements that fall through the `!== "brush"` reset below.
    if (placement.kind !== "brush" && placement.kind !== "spot") masks.activeTool = null;
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
    develop.scheduleFlush(label);
  }

  // Luminance range has no geometry to place, so it doesn't go through
  // handleMaskCreated's placement-dispatch shape at all -- MaskToolStrip's
  // button calls this directly (real Lightroom's own behavior: this mask
  // kind is created on tool-select, no canvas interaction needed).
  function handleCreateLuminanceRangeMask() {
    const mask = createLuminanceRangeMask();
    develop.editStack = addMask(develop.editStack, mask);
    masks.selectedMaskId = mask.id;
    develop.scheduleFlush("Add Luminance Range Mask");
  }

  function handleMaskUpdated(/** @type {string} */ id, /** @type {Record<string, unknown>} */ patch) {
    develop.editStack = updateMask(develop.editStack, id, patch);
    develop.scheduleFlush("Edit Mask");
  }

  function handleMaskDeleted() {
    if (masks.selectedMaskId === null) return;
    develop.editStack = removeMask(develop.editStack, masks.selectedMaskId);
    masks.selectedMaskId = null;
    develop.flushEditStack("Delete Mask");
  }


  function handleResetEditStack() {
    if (develop.versionId === null) return;
    develop.editStack = resetEditStack(develop.editStack);
    masks.selectedMaskId = null;
    masks.activeTool = null;
    presets.confirmingReset = false;
    develop.flushEditStack("Reset");
  }


  function handleCreateSnapshotConfirmed(/** @type {string} */ name) {
    presets.creatingSnapshot = false;
    handleCreateSnapshot(name);
  }


  function handleSaveCurrentAsPresetRequest() {
    presets.creatingPreset = true;
  }

  async function handleCreatePresetConfirmed(/** @type {string} */ name) {
    presets.creatingPreset = false;
    const preset = await createPreset(name, presetEligibleOps(develop.editStack));
    presets.list = [...presets.list, preset];
  }

  /** Applying a preset to the currently open Develop image is an
   * immediate, discrete action (like Reset/mask-delete), not a debounced
   * slider drag -- flushes right away under its own label. */
  async function handleApplyPreset(/** @type {number} */ presetId) {
    develop.clearPreview();
    if (develop.versionId === null) return;
    const preset = presets.list.find((p) => p.id === presetId);
    if (!preset) return;
    const versionId = develop.versionId;
    develop.editStack = applyPresetOps(develop.editStack, preset.edit_stack);
    // Same immediate-regen pattern restoreTo/handleRestoreSnapshot already
    // follow -- this is a jump-to-a-different-look commit, not a slider
    // drag, so the Library grid thumbnail shouldn't have to wait for the
    // "leaving Develop" checkpoint (switchModule/openDevelop) to catch up.
    // Awaited first, same unawaited-dependent-IPC-calls hazard openDevelop's
    // own flush/regen pair guards against (regenerateThumbnailFor re-reads
    // the edit stack fresh from the catalog, so it must not race the write
    // it's meant to reflect).
    await develop.flushEditStack(`Apply Preset: ${preset.name}`);
    regenerateThumbnailFor(versionId);
  }

  async function handleExportPreset(/** @type {number} */ presetId) {
    const preset = presets.list.find((p) => p.id === presetId);
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
      presets.list = [...presets.list, preset];
      shell.notify(`Imported "${raw.name}"`);
    } catch (/** @type {any} */ e) {
      shell.notify(`Import preset failed: ${e}`);
    }
  }

  function handleDeletePresetRequest(/** @type {number} */ presetId) {
    presets.confirmingDeletePresetId = presetId;
  }

  async function handleDeletePresetConfirmed() {
    if (presets.confirmingDeletePresetId === null) return;
    const presetId = presets.confirmingDeletePresetId;
    presets.confirmingDeletePresetId = null;
    await deletePreset(presetId);
    presets.list = presets.list.filter((p) => p.id !== presetId);
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
    const preset = presets.list.find((p) => p.id === Number(value));
    if (!preset) return;
    const targets = [...selection.selectedIds];
    if (targets.length === 0) return;
    presets.applyingPreset = true;
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
      if (develop.versionId !== null && targets.includes(develop.versionId)) {
        develop.editStack = await getEditStack(develop.versionId);
      }
      shell.notify(`Applied "${preset.name}" to ${targets.length} photo${targets.length === 1 ? "" : "s"}`);
    } catch (/** @type {any} */ e) {
      shell.notify(`Apply preset failed: ${e}`);
    } finally {
      presets.applyingPreset = false;
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


  function handleCopySettingsRequest() {
    if (develop.versionId === null) return;
    presets.copySettingsDialogOpen = true;
  }

  function handleCopySettingsConfirmed(/** @type {string[]} */ groupIds) {
    presets.copySettingsDialogOpen = false;
    develop.copiedSettings = copySettingsOps(develop.editStack, groupIds);
    shell.notify("Copied settings");
  }

  /** The Copy/Paste Settings buttons live at the bottom of DevelopPanel
   * (Develop-only), so paste there only ever targets the image currently
   * open in Develop -- an immediate, discrete action (like Apply
   * Preset), flushed right away rather than going through the slider
   * debounce. */
  async function handlePasteSettings() {
    if (!develop.copiedSettings || develop.versionId === null) return;
    const versionId = develop.versionId;
    develop.editStack = applyPresetOps(develop.editStack, develop.copiedSettings);
    // Same immediate-regen reasoning (and awaited-first ordering) as
    // handleApplyPreset's own comment.
    await develop.flushEditStack("Paste Settings");
    regenerateThumbnailFor(versionId);
    shell.notify("Pasted settings");
  }


  /** M4.5 batch apply: applies the SAME in-memory clipboard Copy
   * Settings filled (not a Preset) across every Library-selected image
   * in one action. Mirrors handleApplyPresetToSelection's exact shape --
   * frontend-orchestrated, non-atomic-across-the-batch getEditStack ->
   * applyPresetOps merge -> setEditStack -> regenerateThumbnail per
   * target, with the same re-sync-if-the-open-Develop-image-was-a-target
   * guard -- rather than inventing a second batch pattern. */
  async function handlePasteSettingsToSelection() {
    if (!develop.copiedSettings) return;
    const targets = [...selection.selectedIds];
    if (targets.length === 0) return;
    const settingsToApply = develop.copiedSettings;
    presets.pastingSettingsToSelection = true;
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
      if (develop.versionId !== null && targets.includes(develop.versionId)) {
        develop.editStack = await getEditStack(develop.versionId);
      }
      shell.notify(`Pasted settings to ${targets.length} photo${targets.length === 1 ? "" : "s"}`);
    } catch (/** @type {any} */ e) {
      shell.notify(`Paste settings failed: ${e}`);
    } finally {
      presets.pastingSettingsToSelection = false;
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
    if (shell.activeModule === "develop" && develop.versionId !== null) {
      return [{ path: develop.imagePath, version_id: develop.versionId }];
    }
    if (selection.selectedImages.length > 0) {
      return selection.selectedImages.map((img) => ({ path: img.path, version_id: img.version_id }));
    }
    return selection.selectedImage ? [{ path: selection.selectedImage.path, version_id: selection.selectedImage.version_id }] : [];
  });
  let exportItems = $state(/** @type {{ path: string, version_id: number }[] | null} */ (null));


  function handlePeekPreset(/** @type {number} */ presetId) {
    if (develop.imagePath === null) return;
    const preset = presets.list.find((p) => p.id === presetId);
    if (!preset) return;
    const mergedStack = applyPresetOps(develop.editStack, preset.edit_stack);
    const path = develop.imagePath;
    const contentHash = develop.imageContentHash;
    develop.schedulePreview(() => previewEditStack(path, contentHash, mergedStack));
  }

  /** Moves the live edit stack to `history[index]` -- undo, redo, and a
   * History-panel row click are all this same call, just with a
   * different `index`. See `history`/`historyIndex`'s own doc comment for
   * why this needs no server-side cursor concept at all. */
  async function restoreTo(/** @type {number} */ index) {
    develop.clearPreview();
    if (develop.versionId === null || index < 0 || index >= develop.history.length) return;
    const versionId = develop.versionId;
    const entryId = develop.history[index].id;
    // A restore overwrites editStack wholesale -- cancel any debounced
    // write still pending first, or it could fire afterward under a now-
    // stale label and silently stomp the just-restored state.
    develop.cancelScheduledFlush();
    develop.discardPendingLabel();
    develop.editStack = await restoreHistoryEntry(versionId, entryId);
    develop.historyIndex = index;
    masks.selectedMaskId = null;
    masks.activeTool = null;
    regenerateThumbnailFor(versionId);
  }

  function handleUndo() {
    if (develop.canUndo) restoreTo(develop.historyIndex - 1);
  }

  function handleRedo() {
    if (develop.canRedo) restoreTo(develop.historyIndex + 1);
  }


  /** Unlike restoreTo/restoreHistoryEntry, restoring a snapshot IS a new,
   * undoable edit of its own (see Catalog::restore_snapshot's doc
   * comment) -- the returned history list already includes its own
   * "Restore Snapshot: {name}" row, so this jumps historyIndex straight
   * to newest rather than searching for that row's position. */
  async function handleRestoreSnapshot(/** @type {number} */ snapshotId) {
    develop.clearPreview();
    if (develop.versionId === null) return;
    const versionId = develop.versionId;
    develop.cancelScheduledFlush();
    develop.discardPendingLabel();
    const [stack, freshHistory] = await restoreSnapshot(versionId, snapshotId);
    develop.editStack = stack;
    develop.history = freshHistory;
    develop.historyIndex = freshHistory.length - 1;
    masks.selectedMaskId = null;
    masks.activeTool = null;
    regenerateThumbnailFor(versionId);
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
      return presets.confirmingDeletePresetId;
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
      return presets.creatingPreset;
    },
    get creatingSmartCollection() {
      return library.creatingSmartCollection;
    },
    get creatingSnapshot() {
      return presets.creatingSnapshot;
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
      return masks.maskOverlaysVisible;
    },
    set maskOverlaysVisible(value) {
      masks.maskOverlaysVisible = value;
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
      return masks.selectedMask;
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
      return masks.showMaskOverlay;
    },
    set showMaskOverlay(value) {
      masks.showMaskOverlay = value;
    },
    get showOriginal() {
      return develop.showOriginal;
    },
    set showOriginal(value) {
      develop.showOriginal = value;
    },
    get spacePanning() {
      return develop.spacePanning;
    },
    set spacePanning(value) {
      develop.spacePanning = value;
    },
    switchModule,
  };
  const { handleGlobalKeydown, handleGlobalKeyup } = createKeyboardHandlers(handlerContext);
  const handleMenuAction = createMenuHandler(handlerContext);


  async function openDevelop(/** @type {number} */ versionId) {
    // Captured before developVersionId is reassigned below -- the same
    // capture-before-reassignment shape flushEditStack itself already
    // uses, which is what keeps this race-free even if the user clicks
    // through several images in quick succession (each flush/regen closes
    // over the id it actually applies to, not whatever developVersionId
    // happens to be by the time the async work runs).
    const previousVersionId = develop.versionId;
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
    await develop.flushEditStack();
    regenerateThumbnailFor(previousVersionId);
    const image = library.images.find((img) => img.version_id === versionId);
    if (!image) return;
    prioritizeThumbnail(versionId);
    develop.versionId = versionId;
    develop.imagePath = image.path;
    // Cleared, not left stale, on every open -- the new image's own real
    // histogram arrives shortly via DevelopCanvas's own GPU readback, but
    // showing the PREVIOUS image's histogram in the meantime would be
    // actively misleading, not just momentarily stale.
    develop.histogramData = null;
    develop.hoverPixel = null;
    develop.showClippingOverlay = false;
    // History/Snapshots (M3): re-fetched fresh on every open, not carried
    // over from whatever the previous image's panel showed -- switching
    // images via the filmstrip must never leave a stale History/Snapshots
    // list on screen for a different photo.
    const [stack, freshHistory, freshSnapshots] = await Promise.all([
      getEditStack(versionId),
      getHistory(versionId),
      getSnapshots(versionId),
    ]);
    develop.editStack = stack;
    develop.history = freshHistory;
    develop.historyIndex = freshHistory.length - 1;
    develop.snapshots = freshSnapshots;
    masks.activeTool = null;
    masks.selectedMaskId = null;
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
    if (develop.versionId === versionId && JSON.stringify(profile) !== JSON.stringify(developView.lensCorrection.profile)) {
      develop.editStack = setLensProfile(develop.editStack, profile);
      develop.flushEditStack();
    }
  }

  async function switchModule(/** @type {string} */ target) {
    if (shell.activeModule === "develop" && target !== "develop") {
      // Awaited -- same unawaited-dependent-IPC-calls hazard openDevelop's
      // own flush/regen pair guards against, see that function's own doc
      // comment.
      await develop.flushEditStack();
      regenerateThumbnailFor(develop.versionId);
      masks.activeTool = null;
      masks.selectedMaskId = null;
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


  // HSL band-jump eyedropper's transient navigation target -- NOT persisted
  // edit-stack state, purely a "which band should the panel scroll to and
  // highlight" signal, self-clearing after a fixed delay rather than on
  // "the next unrelated interaction" (which would mean hooking an unbounded
  // set of DOM listeners across the panel). Same fixed-timeout-reset-on-
  // retrigger idiom as persistTimer's own debounce, just for UI feedback
  // instead of persistence.

  let hslBandHighlightTimer = /** @type {ReturnType<typeof setTimeout> | null} */ (null);

  /** Commit path for all four eyedropper destinations -- see
   * eyedropperTarget's own doc comment above for why one shared gesture
   * routes here. One-shot: resets activeTool/eyedropperTarget immediately,
   * matching handleColorRangeResampled's own "click to pick, done" model. */
  function handleEyedropperSampled(/** @type {{r: number, g: number, b: number}} */ color) {
    const target = masks.eyedropperTarget;
    masks.activeTool = null;
    masks.eyedropperTarget = null;
    if (target === null) return;
    const { h, s, l } = rgbToHsl(color.r, color.g, color.b);

    if (target === "split_toning_shadows" || target === "split_toning_highlights") {
      const zone = target === "split_toning_shadows" ? "shadows" : "highlights";
      develop.editStack = upsertSplitToningZone(develop.editStack, zone, { hue: h, saturation: s * 100 });
      develop.scheduleFlush("Split Toning");
      return;
    }
    if (target === "hsl_band") {
      // Navigation only -- deliberately no editStack write, no persist.
      // HSL's own sliders are relative hue/sat/lum shifts, not an absolute
      // color a sampled pixel could set; this just finds "which band".
      develop.highlightedHslBand = nearestHslBand(h);
      if (hslBandHighlightTimer) clearTimeout(hslBandHighlightTimer);
      hslBandHighlightTimer = setTimeout(() => (develop.highlightedHslBand = null), 1500);
      return;
    }
    if (target === "white_balance") {
      const { temperature, tint } = computeEyedropperWhiteBalance(color);
      develop.editStack = upsertOp(develop.editStack, "temperature", temperature);
      develop.editStack = upsertOp(develop.editStack, "tint", tint);
      develop.scheduleFlush("White Balance Eyedropper");
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
      const y = sampleCurveLut(buildToneCurveLut(developView.toneCurvePoints), l);
      const next = insertToneCurvePoint(developView.toneCurvePoints, l, y);
      if (next !== developView.toneCurvePoints) {
        develop.editStack = upsertToneCurve(develop.editStack, next);
        develop.scheduleFlush("Tone Curve");
      }
    }
  }


  async function handleExportClick() {
    // If a slider was just dragged, the debounced save may not have
    // landed yet -- flush it first so Export reads the value currently
    // on screen, not the last-persisted one. Awaited for the same
    // unawaited-dependent-IPC-calls hazard openDevelop's own flush/regen
    // pair guards against, see that function's own doc comment.
    if (shell.activeModule === "develop") {
      await develop.flushEditStack();
      regenerateThumbnailFor(develop.versionId);
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

        const editPending = develop.hasPendingWork;
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

        const wasEditPending = develop.hasPendingEdit;
        await develop.flushPending();
        // Fire-and-forget, deliberately NOT awaited: a thumbnail regen
        // abandoned by a force-quit mid-encode is a stale-until-next-flush
        // grid thumbnail, strictly lower stakes than the lost-edit bug M1
        // Slice 6 actually fixed for the edit-stack flush -- direct
        // precedent already established for generate_missing_thumbnails.
        // Blocking app quit on this would be a real regression. Flush the
        // batch queue to ensure pending regens start immediately, but don't
        // wait for them to complete.
        if (wasEditPending) {
          regenerateThumbnailFor(develop.versionId);
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

<svelte:window onkeydown={handleGlobalKeydown} onkeyup={handleGlobalKeyup} onblur={() => (develop.spacePanning = false)} />

<div class="app">
  <AppTitlebar
    activeModule={shell.activeModule}
    {currentExportItems}
    activeCollectionId={library.activeCollectionId}
    activeCollection={library.activeCollection}
    selectedIds={selection.selectedIds}
    manualCollections={library.manualCollections}
    applyingPreset={presets.applyingPreset}
    presets={presets.list}
    copiedSettings={develop.copiedSettings}
    pastingSettingsToSelection={presets.pastingSettingsToSelection}
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
    copySettingsDialogOpen={presets.copySettingsDialogOpen}
    confirmingFaceDetectionOnImport={faces.confirmingDetectionOnImport}
    pendingImportBatchSize={faces.pendingImportBatchSize}
    confirmingRemoval={library.confirmingRemoval}
    selectedIds={selection.selectedIds}
    confirmingReset={presets.confirmingReset}
    creatingCollection={library.creatingCollection}
    creatingCollectionWithImages={library.creatingCollectionWithImages}
    creatingSmartCollection={library.creatingSmartCollection}
    creatingSnapshot={presets.creatingSnapshot}
    creatingPreset={presets.creatingPreset}
    confirmingDeletePresetId={presets.confirmingDeletePresetId}
    backupPromptSettings={importFlow.backupPromptSettings}
    backupPromptOpen={importFlow.backupPromptOpen}
    onCloseSettings={() => (shell.settingsOpen = false)}
    onCloseExport={() => (exportItems = null)}
    onCancelCopySettings={() => (presets.copySettingsDialogOpen = false)}
    onCancelRemoval={() => (library.confirmingRemoval = false)}
    onCancelReset={() => (presets.confirmingReset = false)}
    onCancelCreateCollection={() => (library.creatingCollection = false)}
    onCancelCreateCollectionWithImages={() => {
      library.creatingCollectionWithImages = false;
      library.pendingAddToCollectionImageIds = [];
    }}
    onCancelCreateSmartCollection={() => (library.creatingSmartCollection = false)}
    onCancelCreateSnapshot={() => (presets.creatingSnapshot = false)}
    onCancelCreatePreset={() => (presets.creatingPreset = false)}
    onCancelDeletePreset={() => (presets.confirmingDeletePresetId = null)}
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
  {:else if shell.activeModule === "develop" && develop.imagePath}
    <div class="develop-body">
      <HistoryPanel
        history={develop.history}
        historyIndex={develop.historyIndex}
        snapshots={develop.snapshots}
        onJumpTo={restoreTo}
        onCreateSnapshotRequest={() => (presets.creatingSnapshot = true)}
        onRestoreSnapshot={handleRestoreSnapshot}
        onDeleteSnapshot={handleDeleteSnapshot}
        presets={presets.list}
        onApplyPreset={handleApplyPreset}
        onSaveCurrentAsPresetRequest={handleSaveCurrentAsPresetRequest}
        onExportPreset={handleExportPreset}
        onDeletePresetRequest={handleDeletePresetRequest}
        onImportPresetRequest={handleImportPresetRequest}
        onPeekHistory={handlePeekHistory}
        onPeekSnapshot={handlePeekSnapshot}
        onPeekPreset={handlePeekPreset}
        onPeekEnd={develop.clearPreview}
        previewUrl={develop.previewUrl}
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
        imagePath={develop.imagePath}
        imageContentHash={develop.imageContentHash}
        exposure={developView.exposure}
        contrast={developView.contrast}
        saturation={developView.saturation}
        temperature={developView.temperature}
        tint={developView.tint}
        highlights={developView.highlights}
        shadows={developView.shadows}
        whites={developView.whites}
        blacks={developView.blacks}
        masks={masks.list}
        activeTool={masks.activeTool}
        selectedMaskId={masks.selectedMaskId}
        brushSize={masks.brushSize}
        brushHardness={masks.brushHardness}
        brushFlow={masks.brushFlow}
        eraseMode={masks.eraseMode}
        showMaskOverlay={masks.showMaskOverlay}
        spotBrushSize={masks.spotBrushSize}
        maskOverlaysVisible={masks.maskOverlaysVisible}
        showOriginal={develop.showOriginal}
        spacePanning={develop.spacePanning}
        onSpotBrushSizeChange={(v) => (masks.spotBrushSize = v)}
        onMaskCreated={handleMaskCreated}
        onMaskUpdated={handleMaskUpdated}
        onMaskSelected={(id) => (masks.selectedMaskId = id)}
        colorRangeResampleId={masks.colorRangeResampleTarget}
        onColorRangeResampled={handleColorRangeResampled}
        onEyedropperSampled={handleEyedropperSampled}
        toneCurvePoints={developView.toneCurvePoints}
        hslBands={developView.hslBands}
        splitToning={developView.splitToning}
        dehaze={developView.dehaze}
        texture={developView.texture}
        clarity={developView.clarity}
        vignette={developView.vignette}
        lensCorrection={developView.lensCorrection}
        perspective={developView.perspective}
        grain={developView.grain}
        sharpen={developView.sharpen}
        lumaNR={developView.lumaNR}
        colorNR={developView.colorNR}
        crop={developView.crop}
        onCropChange={handleCropChange}
        cropAspectLock={develop.cropAspectLock}
        onSourceDimensions={handleSourceDimensions}
        onHistogramUpdate={handleHistogramUpdate}
        showClippingOverlay={develop.showClippingOverlay}
        onHoverPixel={handleHoverPixel}
        softProofEnabled={softProof.enabled}
        softProofPreviewUrl={softProof.previewUrl}
        softProofLoading={softProof.loading}
        softProofProfileLabel={softProof.profileLabel}
        cpuFallbackPreviewUrl={develop.cpuFallbackPreviewUrl}
        onGpuFallback={handleGpuFallback}
      />
      {#if masks.selectedMask}
        <MaskEditorPanel
          mask={masks.selectedMask}
          onChange={(patch) => handleMaskUpdated(/** @type {string} */ (masks.selectedMaskId), patch)}
          onDelete={handleMaskDeleted}
          onClose={() => (masks.selectedMaskId = null)}
          showMaskOverlay={masks.showMaskOverlay}
          onShowOverlayChange={(v) => (masks.showMaskOverlay = v)}
          isResamplingColor={masks.isResamplingColor}
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
        histogramData={develop.histogramData}
        showClippingOverlay={develop.showClippingOverlay}
        onToggleClippingOverlay={handleToggleClippingOverlay}
        hoverPixel={develop.hoverPixel}
        exposure={developView.exposure}
        contrast={developView.contrast}
        saturation={developView.saturation}
        temperature={developView.temperature}
        tint={developView.tint}
        highlights={developView.highlights}
        shadows={developView.shadows}
        whites={developView.whites}
        blacks={developView.blacks}
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
        toneCurvePoints={developView.toneCurvePoints}
        onToneCurveChange={handleToneCurveChange}
        hslBands={developView.hslBands}
        onHslBandChange={handleHslBandChange}
        splitToning={developView.splitToning}
        onSplitToningZoneChange={handleSplitToningZoneChange}
        onSplitToningBalanceChange={handleSplitToningBalanceChange}
        highlightedHslBand={develop.highlightedHslBand}
        {isEyedropperActive}
        onEyedropperToggle={handleEyedropperToggle}
        hasEdits={develop.editStack.ops.length > 0}
        onResetRequest={() => (presets.confirmingReset = true)}
        dehaze={developView.dehaze}
        onDehazeChange={(v) => handleAdjustmentChange("dehaze", v)}
        texture={developView.texture}
        onTextureChange={(v) => handleAdjustmentChange("texture", v)}
        clarity={developView.clarity}
        onClarityChange={(v) => handleAdjustmentChange("clarity", v)}
        vignette={developView.vignette}
        onVignetteChange={handleVignetteChange}
        lensCorrection={developView.lensCorrection}
        onLensCorrectionChange={handleLensCorrectionChange}
        perspective={developView.perspective}
        onPerspectiveChange={handlePerspectiveChange}
        grain={developView.grain}
        onGrainChange={handleGrainChange}
        sharpen={developView.sharpen}
        onSharpenChange={handleSharpenChange}
        lumaNR={developView.lumaNR}
        onLumaNRChange={handleLumaNRChange}
        colorNR={developView.colorNR}
        onColorNRChange={handleColorNRChange}
        softProofEnabled={softProof.enabled}
        softProofTarget={softProof.target}
        softProofCustomProfilePath={softProof.customProfilePath}
        softProofIntent={softProof.intent}
        softProofGamutWarning={softProof.gamutWarning}
        onSoftProofEnabledChange={(v) => (softProof.enabled = v)}
        onSoftProofTargetChange={(v) => (softProof.target = /** @type {typeof softProof.target} */ (v))}
        onSoftProofIntentChange={(v) => (softProof.intent = /** @type {typeof softProof.intent} */ (v))}
        onSoftProofGamutWarningChange={(v) => (softProof.gamutWarning = v)}
        onChooseCustomProfile={handleChooseCustomProfile}
        onCopySettingsRequest={handleCopySettingsRequest}
        canPasteSettings={develop.copiedSettings !== null}
        onPasteSettingsRequest={handlePasteSettings}
      />
    </div>
    <MaskToolStrip
      activeTool={masks.activeTool}
      masks={masks.list}
      selectedMaskId={masks.selectedMaskId}
      brushSize={masks.brushSize}
      brushHardness={masks.brushHardness}
      brushFlow={masks.brushFlow}
      eraseMode={masks.eraseMode}
      spotBrushSize={masks.spotBrushSize}
      maskOverlaysVisible={masks.maskOverlaysVisible}
      gpuUnavailable={develop.gpuFallbackActive}
      onToolToggle={(tool) => (masks.activeTool = masks.activeTool === tool ? null : tool)}
      onMaskSelect={(id) => (masks.selectedMaskId = id)}
      onBrushSizeChange={(v) => (masks.brushSize = v)}
      onBrushHardnessChange={(v) => (masks.brushHardness = v)}
      onBrushFlowChange={(v) => (masks.brushFlow = v)}
      onEraseToggle={() => (masks.eraseMode = !masks.eraseMode)}
      onNewBrush={() => (masks.selectedMaskId = null)}
      onSpotBrushSizeChange={(v) => (masks.spotBrushSize = v)}
      onNewSpot={() => (masks.selectedMaskId = null)}
      onToggleMaskOverlaysVisible={() => (masks.maskOverlaysVisible = !masks.maskOverlaysVisible)}
      onCreateLuminanceRange={handleCreateLuminanceRangeMask}
      crop={developView.crop}
      cropAspectLock={develop.cropAspectLock}
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

  {#if shell.activeModule === "develop" && develop.imagePath}
    <DevelopInfoBar imagePath={develop.imagePath} />
  {/if}

  {#if shell.activeModule === "library"}
    <Filmstrip images={library.filteredImages} selectedIds={selection.selectedIds} onSelect={handleSelect} onOpen={openDevelop} />
  {:else if shell.activeModule === "develop"}
    <Filmstrip
      images={developFilmstripImages}
      selectedIds={new Set(develop.versionId !== null ? [develop.versionId] : [])}
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
