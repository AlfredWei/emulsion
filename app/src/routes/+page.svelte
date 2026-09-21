<script>
  import "$lib/styles/tokens.css";
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
  import { exportFlow } from "$lib/state/exportFlow.svelte.js";
  import { createKeyboardHandlers } from "$lib/keyboard.js";
  import { createMenuHandler } from "$lib/menuActions.js";
  import { installAppEvents } from "$lib/appEvents.js";
            import { handleChoosePrintCustomProfile, handlePrint, handleExportPdf } from "$lib/actions/printActions.js";
  import {
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
    handleResetFilters,
    selectCollection,
    patchLocal,
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
    handleMergeHdrBracket,
    handleMergePanorama,
  } from "$lib/actions/importActions.js";
  import { handleRatingChange, handleFlagChange, handleColorLabelChange, handleCaptionChange, handleCopyrightChange, handleContactChange } from "$lib/actions/metadataActions.js";
  import { handleAdjustmentChange, handleToneCurveChange, handleHslBandChange, handleSplitToningZoneChange, handleSplitToningBalanceChange, handleVignetteChange, handleLensCorrectionChange, handlePerspectiveChange, handleGrainChange, handleSharpenChange, handleLumaNRChange, handleColorNRChange, handleCropChange, handleCropAspectPreset, handleCropReset, handleAutoWhiteBalance, handleWbPresetChange, handleAutoTone, handleSourceDimensions, handleHistogramUpdate, handleToggleClippingOverlay, handleHoverPixel, handlePeekHistory, handlePeekSnapshot, handleDeleteSnapshot } from "$lib/actions/developActions.js";
  import { handleGpuFallback, handleResampleColorToggle, handleColorRangeResampled, isEyedropperActive, handleEyedropperToggle, handleMaskCreated, handleCreateLuminanceRangeMask, handleMaskUpdated, handleMaskDeleted, handleEyedropperSampled } from "$lib/actions/maskActions.js";
  import { handleCreateSnapshotConfirmed, handleSaveCurrentAsPresetRequest, handleCreatePresetConfirmed, handleApplyPreset, handleExportPreset, handleImportPresetRequest, handleDeletePresetRequest, handleDeletePresetConfirmed, handleApplyPresetToSelection, handleCopySettingsRequest, handleCopySettingsConfirmed, handlePasteSettings, handlePasteSettingsToSelection, handlePeekPreset } from "$lib/actions/presetActions.js";
  import { handleChooseCustomProfile } from "$lib/actions/softProofActions.js";
  import { restoreTo, handleUndo, handleRedo, handleRestoreSnapshot, handleResetEditStack } from "$lib/actions/historyActions.js";
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
