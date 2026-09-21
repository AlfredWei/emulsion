<script>
  import HistoryPanel from "$lib/components/HistoryPanel.svelte";
  import { develop } from "$lib/state/develop.svelte.js";
  import { restoreTo, handleRestoreSnapshot } from "$lib/actions/historyActions.js";
  import { presets } from "$lib/state/presets.svelte.js";
  import { handleDeleteSnapshot, handlePeekHistory, handlePeekSnapshot, handleCropChange, handleSourceDimensions, handleHistogramUpdate, handleHoverPixel, handleToggleClippingOverlay, handleAdjustmentChange, handleAutoWhiteBalance, handleAutoTone, handleWbPresetChange, handleToneCurveChange, handleHslBandChange, handleSplitToningZoneChange, handleSplitToningBalanceChange, handleVignetteChange, handleLensCorrectionChange, handlePerspectiveChange, handleGrainChange, handleSharpenChange, handleLumaNRChange, handleColorNRChange, handleCropAspectPreset, handleCropReset } from "$lib/actions/developActions.js";
  import { handleApplyPreset, handleSaveCurrentAsPresetRequest, handleExportPreset, handleDeletePresetRequest, handleImportPresetRequest, handlePeekPreset, handleCopySettingsRequest, handlePasteSettings } from "$lib/actions/presetActions.js";
  import { shell } from "$lib/state/shell.svelte.js";
  import DevelopCanvas from "$lib/components/DevelopCanvas.svelte";
  import { developView } from "$lib/state/developView.svelte.js";
  import { masks } from "$lib/state/masks.svelte.js";
  import { handleMaskCreated, handleMaskUpdated, handleColorRangeResampled, handleEyedropperSampled, handleGpuFallback, handleMaskDeleted, handleResampleColorToggle, isEyedropperActive, handleEyedropperToggle, handleCreateLuminanceRangeMask } from "$lib/actions/maskActions.js";
  import { softProof } from "$lib/state/softProof.svelte.js";
  import MaskEditorPanel from "$lib/components/MaskEditorPanel.svelte";
  import DevelopPanel from "$lib/components/DevelopPanel.svelte";
  import { handleChooseCustomProfile } from "$lib/actions/softProofActions.js";
  import MaskToolStrip from "$lib/components/MaskToolStrip.svelte";
</script>

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

<style>
  .develop-body {
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
</style>
