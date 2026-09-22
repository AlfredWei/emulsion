<script>
  import LibraryFilterBar from "$lib/components/LibraryFilterBar.svelte";
  import { library } from "$lib/state/library.svelte.js";
  import { handleResetFilters, selectAllPhotos, selectLastImport, selectFolder, selectCollection, handleDeleteCollection, selectPerson, showMapView, handleMapClusterSelect, handleMapAssignLocation, applyLocationLocally, prioritizeThumbnail, patchLocal } from "$lib/actions/libraryActions.js";
  import { importFlow } from "$lib/state/importFlow.svelte.js";
  import CatalogRail from "$lib/components/CatalogRail.svelte";
  import { faces } from "$lib/state/faces.svelte.js";
  import { handleRenamePerson, handleDetectFacesForSelected, handleReassignFace, handleCreatePersonAndTagFace, handleSetFaceExcluded } from "$lib/actions/faceActions.js";
  import { handleImportFolder, handleImportFiles } from "$lib/actions/importActions.js";
  import LibraryGrid from "$lib/components/LibraryGrid.svelte";
  import { selection } from "$lib/state/selection.svelte.js";
  import { handleSelect, handleCompareSwap, handleCompareMakeSelect, handleCompareNextCandidate, handleComparePrevCandidate } from "$lib/actions/selectionActions.js";
  import { handleRatingChange, handleFlagChange, handleColorLabelChange, handleCaptionChange, handleCopyrightChange, handleContactChange } from "$lib/actions/metadataActions.js";
  import LibraryImageViewer from "$lib/components/LibraryImageViewer.svelte";
  import { selectPrevImage, selectNextImage, openDevelop } from "$lib/actions/navigation.js";
  import LibraryCompareView from "$lib/components/LibraryCompareView.svelte";
  import LibrarySurveyView from "$lib/components/LibrarySurveyView.svelte";
  import LibraryMapView from "$lib/components/LibraryMapView.svelte";
  import LibraryToolbar from "$lib/components/LibraryToolbar.svelte";
  import MetadataPanel from "$lib/components/MetadataPanel.svelte";
  import { shell } from "$lib/state/shell.svelte.js";

  let imageViewerRef = $state(/** @type {any} */ (null));
</script>

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
    activeMapImageIds={library.activeMapImageIds}
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
        {:else if library.activeMapImageIds !== null}
          No photos in this map selection.
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
      {:else if library.libraryViewMode === "map"}
        <LibraryMapView
          images={library.filteredImages}
          selectedImageIds={selection.keywordTargetImageIds}
          onSelectCluster={handleMapClusterSelect}
          onAssignLocation={handleMapAssignLocation}
          onNeedThumbnail={prioritizeThumbnail}
        />
      {/if}

      <!-- Library Bottom Toolbar -->
      <LibraryToolbar
        viewMode={library.libraryViewMode}
        selectedCount={selection.selectedIds.size}
        totalCount={library.filteredImages.length}
        zoomLevel={library.libraryZoomLevel}
        onViewModeChange={(m) => (m === "map" ? showMapView() : (library.libraryViewMode = m))}
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
      applyLocationLocally(imageIds, lat, lon);
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

<style>
  .body {
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
</style>
