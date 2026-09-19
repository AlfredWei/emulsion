<script>
  /**
   * AppTitlebar: the top bar of the app shell -- module switch (Library/Develop/Print) plus the
   * batch/import/export action buttons. Stateless: every value arrives as a prop and every
   * action leaves as a callback (RFC-0009 P1).
   * @type {{
   *   activeModule: string,
   *   currentExportItems: { path: string, version_id: number }[],
   *   activeCollectionId: number | null,
   *   activeCollection: { is_smart?: boolean } | null | undefined,
   *   selectedIds: Set<number>,
   *   manualCollections: { id: number, name: string }[],
   *   applyingPreset: boolean,
   *   presets: { id: number, name: string }[],
   *   copiedSettings: unknown,
   *   pastingSettingsToSelection: boolean,
   *   mergingHdr: boolean,
   *   mergingPanorama: boolean,
   *   detectingFaces: boolean,
   *   faceScanProgress: { current: number, total: number } | null,
   *   faceDetectionCancelable: boolean,
   *   showFaceRects: boolean,
   *   importing: boolean,
   *   switchModule: (target: string) => void,
   *   handleRemoveFromCollection: () => void,
   *   handleAddToCollectionSelect: (value: string) => void,
   *   handleApplyPresetToSelection: (presetId: string) => void,
   *   handlePasteSettingsToSelection: () => void,
   *   handleMergeHdrBracket: () => void,
   *   handleMergePanorama: () => void,
   *   handleCancelFaceDetection: () => void,
   *   handleDetectFacesForSelection: () => void,
   *   handleDetectFacesForFolder: () => void,
   *   handleExportClick: () => void,
   *   handleImportFiles: () => void,
   *   handleImportFolder: () => void,
   *   onToggleFaceRects: () => void,
   *   onRequestRemoval: () => void,
   *   onOpenSettings: () => void,
   * }}
   */
  let {
    activeModule,
    currentExportItems,
    activeCollectionId,
    activeCollection,
    selectedIds,
    manualCollections,
    applyingPreset,
    presets,
    copiedSettings,
    pastingSettingsToSelection,
    mergingHdr,
    mergingPanorama,
    detectingFaces,
    faceScanProgress,
    faceDetectionCancelable,
    showFaceRects,
    importing,
    switchModule,
    handleRemoveFromCollection,
    handleAddToCollectionSelect,
    handleApplyPresetToSelection,
    handlePasteSettingsToSelection,
    handleMergeHdrBracket,
    handleMergePanorama,
    handleCancelFaceDetection,
    handleDetectFacesForSelection,
    handleDetectFacesForFolder,
    handleExportClick,
    handleImportFiles,
    handleImportFolder,
    onToggleFaceRects,
    onRequestRemoval,
    onOpenSettings,
  } = $props();
</script>

<div class="titlebar">
  <div class="module-switch">
    <button class:active={activeModule === "library"} onclick={() => switchModule("library")}>
      Library
    </button>
    <button class:active={activeModule === "develop"} onclick={() => switchModule("develop")}>
      Develop
    </button>
    <button
      class:active={activeModule === "print"}
      onclick={() => switchModule("print")}
      disabled={activeModule !== "print" && currentExportItems.length === 0}
    >
      Print
    </button>
  </div>
  <div class="spacer"></div>
  {#if activeModule === "library" && activeCollectionId !== null && activeCollection && !activeCollection.is_smart}
    <button
      class="icon-btn remove-from-collection-btn"
      onclick={handleRemoveFromCollection}
      disabled={selectedIds.size === 0}
      title={`Remove from Collection${selectedIds.size > 1 ? ` (${selectedIds.size})` : ""}`}
    >
      <span class="icon" aria-hidden="true">OUT</span>
      <span class="label">Remove from Collection{selectedIds.size > 1 ? ` (${selectedIds.size})` : ""}</span>
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
    class="icon-btn paste-settings-btn"
    onclick={handlePasteSettingsToSelection}
    disabled={activeModule !== "library" || selectedIds.size === 0 || !copiedSettings || pastingSettingsToSelection}
    title={pastingSettingsToSelection ? "Pasting…" : copiedSettings ? "Paste Settings to Selection — paste the copied Develop settings onto every selected photo" : "Copy Settings in Develop first"}
  >
    <span class="glyph" aria-hidden="true">⧉</span>
    <span class="code" aria-hidden="true">PST</span>
    <span class="label">{pastingSettingsToSelection ? "Pasting…" : "Paste Settings to Selection"}</span>
  </button>
  <button
    class="icon-btn merge-hdr-btn"
    onclick={handleMergeHdrBracket}
    disabled={activeModule !== "library" || selectedIds.size < 2 || mergingHdr}
    title={mergingHdr ? "Merging…" : `Merge to HDR — combine 2+ RAW exposures of the same scene into one HDR image${selectedIds.size >= 2 ? ` (${selectedIds.size} selected)` : ""}`}
  >
    <span class="glyph" aria-hidden="true">◐</span>
    <span class="code" aria-hidden="true">HDR</span>
    <span class="label">{mergingHdr ? "Merging…" : `Merge to HDR…${selectedIds.size >= 2 ? ` (${selectedIds.size})` : ""}`}</span>
  </button>
  <button
    class="icon-btn merge-panorama-btn"
    onclick={handleMergePanorama}
    disabled={activeModule !== "library" || selectedIds.size < 2 || mergingPanorama}
    title={mergingPanorama ? "Stitching…" : `Merge to Panorama — stitch 2+ overlapping photos, selected in capture order, into one wide composite${selectedIds.size >= 2 ? ` (${selectedIds.size} selected)` : ""}`}
  >
    <span class="glyph" aria-hidden="true">▭</span>
    <span class="code" aria-hidden="true">PAN</span>
    <span class="label">{mergingPanorama ? "Stitching…" : `Merge to Panorama…${selectedIds.size >= 2 ? ` (${selectedIds.size})` : ""}`}</span>
  </button>
  {#if detectingFaces}
    <div class="face-detect-toolbar-progress">
      <progress value={faceScanProgress?.current ?? 0} max={faceScanProgress?.total ?? 1}></progress>
      <span class="face-detect-toolbar-label">
        Detecting faces {faceScanProgress?.current ?? 0} / {faceScanProgress?.total ?? 0}
      </span>
      {#if faceDetectionCancelable}
        <button class="face-detect-toolbar-cancel" type="button" onclick={handleCancelFaceDetection}>Cancel</button>
      {/if}
    </div>
  {:else}
    <button
      class="icon-btn detect-faces-btn"
      onclick={handleDetectFacesForSelection}
      disabled={activeModule !== "library" || selectedIds.size < 2}
      title={`Detect Faces — run face detection on the ${selectedIds.size} selected photos not yet scanned`}
    >
      <span class="glyph" aria-hidden="true">☺</span>
      <span class="code" aria-hidden="true">FACE</span>
      <span class="label">Detect Faces{selectedIds.size >= 2 ? ` (${selectedIds.size})` : ""}</span>
    </button>
    <button
      class="icon-btn detect-faces-folder-btn"
      onclick={handleDetectFacesForFolder}
      disabled={activeModule !== "library"}
      title="Detect Faces in Folder — run face detection on every unscanned photo in the current view"
    >
      <span class="glyph" aria-hidden="true">☺</span>
      <span class="code" aria-hidden="true">ALL</span>
      <span class="label">Detect Faces in Folder</span>
    </button>
  {/if}
  <button
    class="icon-btn show-face-rects-btn"
    class:accent={showFaceRects}
    onclick={onToggleFaceRects}
    disabled={activeModule !== "library"}
    title={showFaceRects ? "Hide face rectangles over the Loupe view" : "Show face rectangles over the Loupe view"}
  >
    <span class="glyph" aria-hidden="true">▢</span>
    <span class="code" aria-hidden="true">RECT</span>
    <span class="label">{showFaceRects ? "Hide Faces" : "Show Faces"}</span>
  </button>
  <button
    class="icon-btn danger remove-btn"
    onclick={onRequestRemoval}
    disabled={activeModule !== "library" || selectedIds.size === 0}
    title={`Remove${selectedIds.size > 1 ? ` (${selectedIds.size})` : ""} — remove from the catalog (source files stay on disk)`}
  >
    <span class="glyph" aria-hidden="true">×</span>
    <span class="code" aria-hidden="true">DEL</span>
    <span class="label">Remove{selectedIds.size > 1 ? ` (${selectedIds.size})` : ""}</span>
  </button>
  <button
    class="icon-btn export-btn"
    onclick={handleExportClick}
    disabled={currentExportItems.length === 0}
    title={`Export${currentExportItems.length > 1 ? ` (${currentExportItems.length})` : ""}…`}
  >
    <span class="glyph" aria-hidden="true">↗</span>
    <span class="code" aria-hidden="true">EXP</span>
    <span class="label">Export{currentExportItems.length > 1 ? ` (${currentExportItems.length})` : ""}…</span>
  </button>
  <button class="icon-btn import-files-btn" onclick={handleImportFiles} disabled={importing} title={importing ? "Importing…" : "Import Files…"}>
    <span class="glyph" aria-hidden="true">▤</span>
    <span class="code" aria-hidden="true">FILE</span>
    <span class="label">{importing ? "Importing…" : "Import Files…"}</span>
  </button>
  <button class="icon-btn accent import-folder-btn" onclick={handleImportFolder} disabled={importing} title={importing ? "Importing…" : "Import Folder…"}>
    <span class="glyph" aria-hidden="true">▦</span>
    <span class="code" aria-hidden="true">DIR</span>
    <span class="label">{importing ? "Importing…" : "Import Folder…"}</span>
  </button>
  <button class="settings-btn" title="Settings" onclick={onOpenSettings}>⚙</button>
</div>

<style>
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
.module-switch button:disabled {
  cursor: default;
  opacity: 0.5;
}
.spacer {
  flex: 1;
}
/* Toolbar icon buttons: a small pictograph glyph plus a short bold
 * monogram code (OUT/PST/HDR/DEL/EXP/FILE/DIR), both always visible --
 * no hover-expand transition, since a prior version of that design was
 * dropped when almost no one triggered it in practice. Both are plain
 * BMP symbols, not emoji-range pictographs -- deliberately, confirmed
 * the hard way in this environment's own browser preview: emoji-range
 * glyphs (📋🗑📤📄📁) rendered as blank/missing-glyph boxes, while plain
 * BMP symbols and text always render everywhere, in any font, on any
 * platform. Matches this file's own existing plain-glyph precedent (⚙,
 * ★, ×) rather than gambling on a color-emoji font being present. The
 * full descriptive text is always present in the DOM (so it's still
 * announced to screen readers and still searchable in e2e specs via
 * textContent) but visually hidden; `title` carries the same text as an
 * immediate native tooltip. */
.icon-btn {
  all: unset;
  cursor: pointer;
  flex: none;
  display: flex;
  align-items: center;
  gap: 5px;
  height: 30px;
  padding: 0 9px;
  border-radius: 6px;
  color: var(--text-secondary);
  border: 1px solid var(--border-strong);
  overflow: hidden;
  white-space: nowrap;
}
.icon-btn .glyph {
  flex: none;
  font-size: 12px;
  line-height: 1;
}
.icon-btn .code {
  flex: none;
  font-size: 10.5px;
  font-weight: 800;
  letter-spacing: 0.03em;
  line-height: 1;
}
.icon-btn .label {
  position: absolute;
  width: 1px;
  height: 1px;
  padding: 0;
  margin: -1px;
  overflow: hidden;
  clip: rect(0, 0, 0, 0);
  white-space: nowrap;
  border: 0;
}
.icon-btn:hover {
  color: var(--text-primary);
  border-color: var(--text-secondary);
}
.icon-btn:disabled {
  opacity: 0.5;
  cursor: default;
}
.icon-btn:disabled:hover {
  color: var(--text-secondary);
  border-color: var(--border-strong);
}
.icon-btn.accent {
  background: var(--accent-soft);
  color: var(--accent-strong);
  border-color: var(--accent);
}
.icon-btn.danger:not(:disabled):hover {
  color: var(--label-red);
  border-color: var(--label-red);
}
.face-detect-toolbar-progress {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 11px;
  color: var(--text-secondary);
  flex: none;
}
.face-detect-toolbar-progress progress {
  width: 110px;
  height: 5px;
  accent-color: var(--accent);
}
.face-detect-toolbar-label {
  white-space: nowrap;
}
.face-detect-toolbar-cancel {
  all: unset;
  cursor: pointer;
  font-size: 11px;
  font-weight: 600;
  color: var(--text-secondary);
  padding: 3px 8px;
  border: 1px solid var(--border-strong);
  border-radius: 6px;
}
.face-detect-toolbar-cancel:hover {
  color: var(--label-red);
  border-color: var(--label-red);
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
</style>
