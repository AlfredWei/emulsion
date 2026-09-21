<script>
  import "$lib/styles/tokens.css";
  import { onMount } from "svelte";
  import AppTitlebar from "$lib/components/AppTitlebar.svelte";
  import AppDialogs from "$lib/components/AppDialogs.svelte";
  import StatusStrip from "$lib/components/StatusStrip.svelte";
                import Filmstrip from "$lib/components/Filmstrip.svelte";
  import DevelopInfoBar from "$lib/components/DevelopInfoBar.svelte";
                  import { shell } from "$lib/state/shell.svelte.js";
    import { faces } from "$lib/state/faces.svelte.js";
  import { importFlow } from "$lib/state/importFlow.svelte.js";
  import { library } from "$lib/state/library.svelte.js";
  import { selection } from "$lib/state/selection.svelte.js";
  import { develop } from "$lib/state/develop.svelte.js";
    import { masks } from "$lib/state/masks.svelte.js";
  import { softProof } from "$lib/state/softProof.svelte.js";
  import { presets } from "$lib/state/presets.svelte.js";
  import { exportFlow } from "$lib/state/exportFlow.svelte.js";
  import { createKeyboardHandlers } from "$lib/keyboard.js";
  import { createMenuHandler } from "$lib/menuActions.js";
  import { installAppEvents } from "$lib/appEvents.js";
  import LibraryModule from "$lib/components/LibraryModule.svelte";
  import DevelopModule from "$lib/components/DevelopModule.svelte";
  import PrintModule from "$lib/components/PrintModule.svelte";
            import { handleExportPdf } from "$lib/actions/printActions.js";
  import {
    handleCancelFaceDetection,
    refreshCurrentImageFaces,
    handleDetectFacesForSelection,
    handleDetectFacesForFolder,
  } from "$lib/actions/faceActions.js";
  import { handleBackupDone, handleBackupSkip } from "$lib/actions/backupActions.js";
  import {
    handleCreateCollection,
    handleCreateSmartCollection,
    handleRemoveConfirmed,
    showMapView,
  } from "$lib/actions/libraryActions.js";
  import {
    selectGridStep,
    handleSelectAll,
    handleDeselectAll,
    handleSelect,
    handleCompareNextCandidate,
    handleComparePrevCandidate,
  } from "$lib/actions/selectionActions.js";
  import {
    handleAddToCollectionSelect,
    handleCreateCollectionWithImages,
    handleRemoveFromCollection,
  } from "$lib/actions/collectionsActions.js";
  import {
    handleImportFolder,
    handleImportFiles,
    handleMergeHdrBracket,
    handleMergePanorama,
  } from "$lib/actions/importActions.js";
  import { handleRatingChange, handleFlagChange, handleColorLabelChange } from "$lib/actions/metadataActions.js";
  import { handleToggleClippingOverlay } from "$lib/actions/developActions.js";
    import { handleCreateSnapshotConfirmed, handleCreatePresetConfirmed, handleDeletePresetConfirmed, handleApplyPresetToSelection, handleCopySettingsRequest, handleCopySettingsConfirmed, handlePasteSettings, handlePasteSettingsToSelection } from "$lib/actions/presetActions.js";
    import { handleUndo, handleRedo, handleResetEditStack } from "$lib/actions/historyActions.js";
  import { selectNextImage, selectPrevImage, openDevelop, switchModule, handleExportClick } from "$lib/actions/navigation.js";

  let imageViewerRef = $state(/** @type {any} */ (null));


  // The Filmstrip shows filtered images, falling back if active Develop photo is excluded
  let developFilmstripImages = $derived(
    library.filteredImages.some((img) => img.version_id === develop.versionId) ? library.filteredImages : library.images,
  );


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


  // Copy/Paste Settings (M4.5): an unsaved, in-memory analog of Presets --
  // "Copy Settings" snapshots a filtered subset of the CURRENTLY OPEN
  // Develop image's own edit stack (via copySettingsOps, over the exact
  // same preset-eligible op universe presetEligibleOps already defines);
  // "Paste Settings" applies that snapshot with the SAME applyPresetOps
  // upsert-by-name merge Presets use, so it carries the identical
  // whole-op-replace limitation documented there. Deliberately reuses
  // this machinery rather than inventing a second merge strategy.


  // People/Faces: reload the current photo's detected faces whenever the
  // single-image selection changes (image_id, not version_id -- faces are
  // per-image, per RFC-0005 §3.4, so switching between virtual copies of
  // the same source doesn't need a refetch). Cleared, not stale, when
  // nothing is selected.
  $effect(() => {
    void selection.selectedImage?.image_id;
    refreshCurrentImageFaces();
  });


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
      return exportFlow.items;
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
    showMapView,
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


  onMount(() => installAppEvents({ handleMenuAction }));


</script>

<svelte:window onkeydown={handleGlobalKeydown} onkeyup={handleGlobalKeyup} onblur={() => (develop.spacePanning = false)} />

<div class="app">
  <AppTitlebar
    activeModule={shell.activeModule}
    currentExportItems={exportFlow.currentItems}
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
    exportItems={exportFlow.items}
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
    onCloseExport={() => (exportFlow.items = null)}
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
    <LibraryModule />
  {:else if shell.activeModule === "develop" && develop.imagePath}
    <DevelopModule />
  {:else if shell.activeModule === "print"}
    <PrintModule />
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
