<script>
  /**
   * StatusStrip: the thin bar under the titlebar -- import/thumbnail/face progress, HDR merge
   * progress, or the latest status message (RFC-0009 P1).
   * @type {{
   *   importing: boolean,
   *   importPhase: string,
   *   thumbnailProgress: { current: number, total: number } | null,
   *   faceDetectionProgress: { current: number, total: number } | null,
   *   importProgress: { current: number, total: number } | null,
   *   mergingHdr: boolean,
   *   hdrMergeProgress: { current: number, total: number } | null,
   *   statusMessage: string,
   * }}
   */
  let {
    importing,
    importPhase,
    thumbnailProgress,
    faceDetectionProgress,
    importProgress,
    mergingHdr,
    hdrMergeProgress,
    statusMessage,
  } = $props();
</script>

{#if importing}
  {@const progress =
    importPhase === "thumbnails" ? thumbnailProgress : importPhase === "faces" ? faceDetectionProgress : importProgress}
  <div
    class="import-progress"
    role="progressbar"
    aria-valuenow={progress?.current ?? 0}
    aria-valuemin="0"
    aria-valuemax={progress?.total ?? 0}
  >
    <div
      class="import-progress-bar"
      style={`width: ${progress && progress.total > 0 ? (progress.current / progress.total) * 100 : 0}%`}
    ></div>
    <span class="import-progress-label">
      {#if importPhase === "thumbnails"}
        {progress ? `Generating thumbnails ${progress.current} / ${progress.total}…` : "Finishing up…"}
      {:else if importPhase === "faces"}
        {progress ? `Detecting faces ${progress.current} / ${progress.total}…` : "Finishing up…"}
      {:else}
        {progress ? `Importing ${progress.current} / ${progress.total}…` : "Importing…"}
      {/if}
    </span>
  </div>
{:else if mergingHdr}
  <div
    class="import-progress"
    role="progressbar"
    aria-valuenow={hdrMergeProgress?.current ?? 0}
    aria-valuemin="0"
    aria-valuemax={hdrMergeProgress?.total ?? 0}
  >
    <div
      class="import-progress-bar"
      style={`width: ${hdrMergeProgress && hdrMergeProgress.total > 0 ? (hdrMergeProgress.current / hdrMergeProgress.total) * 100 : 0}%`}
    ></div>
    <span class="import-progress-label">
      {hdrMergeProgress ? `Merging HDR bracket ${hdrMergeProgress.current} / ${hdrMergeProgress.total}…` : "Merging HDR bracket…"}
    </span>
  </div>
{:else if statusMessage}
  <div class="status">{statusMessage}</div>
{/if}

<style>
.status {
  flex: none;
  padding: 6px 14px;
  font-size: 11.5px;
  font-family: var(--font-mono);
  color: var(--text-secondary);
  background: var(--bg-panel);
  border-bottom: 1px solid var(--border-subtle);
}
.import-progress {
  flex: none;
  position: relative;
  height: 26px;
  padding: 0 14px;
  display: flex;
  align-items: center;
  font-size: 11.5px;
  font-family: var(--font-mono);
  color: var(--text-secondary);
  background: var(--bg-panel);
  border-bottom: 1px solid var(--border-subtle);
  overflow: hidden;
}
.import-progress-bar {
  position: absolute;
  inset: 0;
  width: 0%;
  background: var(--accent-soft);
  border-right: 1px solid var(--accent);
  transition: width 0.15s ease;
}
.import-progress-label {
  position: relative;
}
</style>
