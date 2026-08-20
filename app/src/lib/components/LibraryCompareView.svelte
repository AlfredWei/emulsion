<script>
  import { convertFileSrc } from "@tauri-apps/api/core";
  import { getDevelopPreview } from "$lib/api/develop.js";

  /**
   * LibraryCompareView: Side-by-side comparison of 2 photos (Select vs Candidate)
   * with synchronized pan & zoom and individual culling controls.
   * @type {{
   *   selectImage: import('$lib/api/catalog.js').ImageSummary,
   *   candidateImage: import('$lib/api/catalog.js').ImageSummary,
   *   onSwap: () => void,
   *   onMakeSelect: () => void,
   *   onNextCandidate?: () => void,
   *   onPrevCandidate?: () => void,
   *   onRatingChange: (versionId: number, rating: number) => void,
   *   onFlagChange: (versionId: number, flag: string) => void,
   *   onColorLabelChange: (versionId: number, colorLabel: string) => void,
   * }}
   */
  let {
    selectImage,
    candidateImage,
    onSwap,
    onMakeSelect,
    onNextCandidate,
    onPrevCandidate,
    onRatingChange,
    onFlagChange,
    onColorLabelChange,
  } = $props();

  let selectUrl = $state(/** @type {string | null} */ (null));
  let candidateUrl = $state(/** @type {string | null} */ (null));

  let zoomScale = $state(1);
  let panX = $state(0);
  let panY = $state(0);
  let isDragging = $state(false);
  let dragStartX = 0;
  let dragStartY = 0;
  let startPanX = 0;
  let startPanY = 0;

  $effect(() => {
    let cancelled = false;
    const path = selectImage?.path;
    if (!path) {
      selectUrl = null;
      return;
    }
    getDevelopPreview(path)
      .then((p) => {
        if (!cancelled && p?.path) selectUrl = convertFileSrc(p.path);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  });

  $effect(() => {
    let cancelled = false;
    const path = candidateImage?.path;
    if (!path) {
      candidateUrl = null;
      return;
    }
    getDevelopPreview(path)
      .then((p) => {
        if (!cancelled && p?.path) candidateUrl = convertFileSrc(p.path);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  });

  let selectThumb = $derived(
    selectImage?.thumbnail_path ? convertFileSrc(selectImage.thumbnail_path) : null,
  );
  let candidateThumb = $derived(
    candidateImage?.thumbnail_path ? convertFileSrc(candidateImage.thumbnail_path) : null,
  );

  let selectFilename = $derived(
    selectImage?.path ? (selectImage.path.split(/[/\\]/).pop() ?? selectImage.path) : "Select",
  );
  let candidateFilename = $derived(
    candidateImage?.path ? (candidateImage.path.split(/[/\\]/).pop() ?? candidateImage.path) : "Candidate",
  );

  let isSameImage = $derived(
    Boolean(selectImage && candidateImage && selectImage.version_id === candidateImage.version_id),
  );

  function handleWheel(/** @type {WheelEvent} */ e) {
    e.preventDefault();
    const delta = e.deltaY < 0 ? 1.15 : 0.87;
    zoomScale = Math.max(0.5, Math.min(5.0, zoomScale * delta));
  }

  function handleMouseDown(/** @type {MouseEvent} */ e) {
    if (e.button !== 0) return;
    isDragging = true;
    dragStartX = e.clientX;
    dragStartY = e.clientY;
    startPanX = panX;
    startPanY = panY;
  }

  function handleMouseMove(/** @type {MouseEvent} */ e) {
    if (!isDragging) return;
    panX = startPanX + (e.clientX - dragStartX);
    panY = startPanY + (e.clientY - dragStartY);
  }

  function handleMouseUp() {
    isDragging = false;
  }

  function handleResetView() {
    zoomScale = 1;
    panX = 0;
    panY = 0;
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div
  class="compare-container"
  onwheel={handleWheel}
  onmousedown={handleMouseDown}
  onmousemove={handleMouseMove}
  onmouseup={handleMouseUp}
  role="region"
  aria-label="Compare View"
>
  <!-- Top Compare Toolbar / Actions -->
  <div class="compare-actions-bar">
    <div class="action-btn-group">
      <button type="button" class="action-btn" onclick={onSwap} title="Swap Select and Candidate (S)">
        ⇄ Swap
      </button>
      <button type="button" class="action-btn" onclick={onMakeSelect} title="Make Candidate the new Select">
        ★ Make Select
      </button>
    </div>

    <div class="action-btn-group">
      {#if onPrevCandidate}
        <button type="button" class="action-btn" onclick={onPrevCandidate} title="Previous Candidate">
          ‹ Prev
        </button>
      {/if}
      {#if onNextCandidate}
        <button type="button" class="action-btn" onclick={onNextCandidate} title="Next Candidate">
          Next ›
        </button>
      {/if}
    </div>

    <button type="button" class="action-btn reset-btn" onclick={handleResetView} title="Reset Zoom">
      Reset View ({Math.round(zoomScale * 100)}%)
    </button>
  </div>

  <!-- Side-by-Side Viewport -->
  <div class="compare-viewport">
    <!-- Left: Select -->
    <div class="compare-pane select-pane">
      <div class="pane-header">
        <span class="pane-tag select-tag">Select</span>
        <span class="pane-name truncate" title={selectImage?.path ?? ""}>{selectFilename}</span>
      </div>

      <div class="image-wrapper">
        <div
          class="image-canvas"
          style="transform: translate({panX}px, {panY}px) scale({zoomScale});"
        >
          {#if selectUrl || selectThumb}
            <img
              src={selectUrl || selectThumb}
              alt="Select"
              class="compare-img"
              draggable="false"
            />
          {:else}
            <div class="compare-img placeholder"></div>
          {/if}
        </div>
      </div>

      <!-- Culling footer -->
      {#if selectImage}
        <div
          class="pane-footer"
          role="toolbar"
          aria-label="Culling actions"
          tabindex="-1"
          onclick={(e) => e.stopPropagation()}
          onkeydown={(e) => e.stopPropagation()}
        >
          <div class="stars">
            {#each [1, 2, 3, 4, 5] as n (n)}
              <button
                type="button"
                class="star"
                class:on={selectImage.rating >= n}
                onclick={() => onRatingChange(selectImage.version_id, selectImage.rating === n ? 0 : n)}
              >★</button>
            {/each}
          </div>
          <div class="flags">
            <button
              type="button"
              class="flag pick"
              class:active={selectImage.flag === "pick"}
              title="Pick (P)"
              aria-label="Pick flag"
              onclick={() => onFlagChange(selectImage.version_id, selectImage.flag === "pick" ? "none" : "pick")}
            >
              <svg viewBox="0 0 16 16" width="11" height="11" fill="currentColor" aria-hidden="true">
                <path d="M3 2v12h1.5V9h7l-1.5-3.5L11.5 2H3z" />
              </svg>
            </button>
            <button
              type="button"
              class="flag reject"
              class:active={selectImage.flag === "reject"}
              title="Reject (X)"
              aria-label="Reject flag"
              onclick={() => onFlagChange(selectImage.version_id, selectImage.flag === "reject" ? "none" : "reject")}
            >
              <svg viewBox="0 0 16 16" width="11" height="11" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" aria-hidden="true">
                <path d="M4 4l8 8M12 4l-8 8" />
              </svg>
            </button>
          </div>
        </div>
      {/if}
    </div>

    <div class="compare-separator"></div>

    <!-- Right: Candidate -->
    <div class="compare-pane candidate-pane">
      <div class="pane-header">
        <span class="pane-tag candidate-tag">Candidate</span>
        <span class="pane-name truncate" title={candidateImage?.path ?? ""}>
          {candidateFilename}
          {#if isSameImage}
            <span class="same-tag">(Same Photo)</span>
          {/if}
        </span>
      </div>

      <div class="image-wrapper">
        <div
          class="image-canvas"
          style="transform: translate({panX}px, {panY}px) scale({zoomScale});"
        >
          {#if candidateUrl || candidateThumb}
            <img
              src={candidateUrl || candidateThumb}
              alt="Candidate"
              class="compare-img"
              draggable="false"
            />
          {:else}
            <div class="compare-img placeholder"></div>
          {/if}
        </div>
      </div>

      <!-- Culling footer for Candidate -->
      {#if candidateImage}
        <div
          class="pane-footer"
          role="toolbar"
          aria-label="Candidate culling actions"
          tabindex="-1"
          onclick={(e) => e.stopPropagation()}
          onkeydown={(e) => e.stopPropagation()}
        >
          <div class="stars">
            {#each [1, 2, 3, 4, 5] as n (n)}
              <button
                type="button"
                class="star"
                class:on={candidateImage.rating >= n}
                onclick={() => onRatingChange(candidateImage.version_id, candidateImage.rating === n ? 0 : n)}
              >★</button>
            {/each}
          </div>
          <div class="flags">
            <button
              type="button"
              class="flag pick"
              class:active={candidateImage.flag === "pick"}
              title="Pick (P)"
              aria-label="Pick flag"
              onclick={() => onFlagChange(candidateImage.version_id, candidateImage.flag === "pick" ? "none" : "pick")}
            >
              <svg viewBox="0 0 16 16" width="11" height="11" fill="currentColor" aria-hidden="true">
                <path d="M3 2v12h1.5V9h7l-1.5-3.5L11.5 2H3z" />
              </svg>
            </button>
            <button
              type="button"
              class="flag reject"
              class:active={candidateImage.flag === "reject"}
              title="Reject (X)"
              aria-label="Reject flag"
              onclick={() => onFlagChange(candidateImage.version_id, candidateImage.flag === "reject" ? "none" : "reject")}
            >
              <svg viewBox="0 0 16 16" width="11" height="11" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" aria-hidden="true">
                <path d="M4 4l8 8M12 4l-8 8" />
              </svg>
            </button>
          </div>
        </div>
      {/if}
    </div>
  </div>
</div>

<style>
  .compare-container {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
    background: #0d0d11;
    overflow: hidden;
    position: relative;
    user-select: none;
  }
  .compare-actions-bar {
    height: 36px;
    background: var(--bg-panel);
    border-bottom: 1px solid var(--border-subtle);
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 12px;
    z-index: 10;
  }
  .action-btn-group {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .action-btn {
    all: unset;
    cursor: pointer;
    font-family: var(--font-mono);
    font-size: 11px;
    font-weight: 500;
    padding: 4px 10px;
    background: var(--bg-panel-raised);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-s);
    color: var(--text-secondary);
    transition: all 0.12s ease;
  }
  .action-btn:hover {
    color: var(--text-primary);
    border-color: var(--border-strong);
    background: var(--bg-panel);
  }
  .reset-btn {
    color: var(--text-tertiary);
  }
  .compare-viewport {
    flex: 1;
    display: flex;
    min-height: 0;
    position: relative;
    overflow: hidden;
  }
  .compare-pane {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
    position: relative;
    background: #08080a;
  }
  .compare-separator {
    width: 2px;
    background: var(--border-subtle);
    flex: none;
    z-index: 5;
  }
  .pane-header {
    height: 28px;
    background: rgba(18, 18, 24, 0.75);
    backdrop-filter: blur(4px);
    border-bottom: 1px solid var(--border-subtle);
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 10px;
    z-index: 5;
  }
  .pane-tag {
    font-family: var(--font-mono);
    font-size: 9.5px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    padding: 2px 6px;
    border-radius: 3px;
  }
  .select-tag {
    background: rgba(99, 102, 241, 0.2);
    color: #818cf8;
    border: 1px solid rgba(99, 102, 241, 0.4);
  }
  .candidate-tag {
    background: rgba(234, 179, 8, 0.2);
    color: #facc15;
    border: 1px solid rgba(234, 179, 8, 0.4);
  }
  .pane-name {
    font-family: var(--font-mono);
    font-size: 10.5px;
    color: var(--text-secondary);
  }
  .same-tag {
    font-size: 9.5px;
    color: var(--text-tertiary);
    margin-left: 4px;
    font-style: italic;
  }
  .image-wrapper {
    flex: 1;
    position: relative;
    overflow: hidden;
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: grab;
  }
  .compare-container:active .image-wrapper {
    cursor: grabbing;
  }
  .image-canvas {
    width: 100%;
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    transform-origin: center center;
    transition: transform 0.05s ease-out;
  }
  .compare-img {
    max-width: 95%;
    max-height: 95%;
    object-fit: contain;
    box-shadow: 0 4px 20px rgba(0, 0, 0, 0.7);
    pointer-events: none;
  }
  .compare-img.placeholder {
    width: 300px;
    height: 200px;
    background: #141418;
    border: 1px dashed var(--border-subtle);
    border-radius: var(--radius-s);
  }
  .pane-footer {
    height: 32px;
    background: rgba(18, 18, 24, 0.75);
    backdrop-filter: blur(4px);
    border-top: 1px solid var(--border-subtle);
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 10px;
    z-index: 5;
  }
  .stars {
    display: flex;
    align-items: center;
    gap: 2px;
  }
  .star {
    all: unset;
    cursor: pointer;
    font-size: 13px;
    color: var(--text-tertiary);
    line-height: 1;
    transition: color 0.1s ease;
  }
  .star.on {
    color: var(--accent-strong);
  }
  .flags {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .flag {
    all: unset;
    cursor: pointer;
    width: 20px;
    height: 20px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border-radius: 3px;
    color: var(--text-tertiary);
    background: rgba(255, 255, 255, 0.04);
    border: 1px solid transparent;
    transition: all 0.1s ease;
  }
  .flag.pick.active {
    color: var(--label-green);
    background: rgba(34, 197, 94, 0.2);
    border-color: rgba(34, 197, 94, 0.4);
  }
  .flag.reject.active {
    color: var(--label-red);
    background: rgba(239, 68, 68, 0.2);
    border-color: rgba(239, 68, 68, 0.4);
  }
</style>
