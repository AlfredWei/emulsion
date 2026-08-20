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
    getDevelopPreview(selectImage.path)
      .then((p) => {
        if (!cancelled) selectUrl = convertFileSrc(p.path);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  });

  $effect(() => {
    let cancelled = false;
    getDevelopPreview(candidateImage.path)
      .then((p) => {
        if (!cancelled) candidateUrl = convertFileSrc(p.path);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  });

  let selectThumb = $derived(
    selectImage.thumbnail_path ? convertFileSrc(selectImage.thumbnail_path) : null,
  );
  let candidateThumb = $derived(
    candidateImage.thumbnail_path ? convertFileSrc(candidateImage.thumbnail_path) : null,
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
        <span class="pane-name truncate">{selectImage.path.split(/[/\\]/).pop()}</span>
      </div>

      <div class="image-wrapper">
        <div
          class="image-canvas"
          style="transform: translate({panX}px, {panY}px) scale({zoomScale});"
        >
          <img
            src={selectUrl || selectThumb}
            alt="Select"
            class="compare-img"
            draggable="false"
          />
        </div>
      </div>

      <!-- Culling footer -->
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
    </div>

    <div class="compare-separator"></div>

    <!-- Right: Candidate -->
    <div class="compare-pane candidate-pane">
      <div class="pane-header">
        <span class="pane-tag candidate-tag">Candidate</span>
        <span class="pane-name truncate">{candidateImage.path.split(/[/\\]/).pop()}</span>
      </div>

      <div class="image-wrapper">
        <div
          class="image-canvas"
          style="transform: translate({panX}px, {panY}px) scale({zoomScale});"
        >
          <img
            src={candidateUrl || candidateThumb}
            alt="Candidate"
            class="compare-img"
            draggable="false"
          />
        </div>
      </div>

      <!-- Culling footer for Candidate -->
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
    </div>
  </div>
</div>

<style>
  .compare-container {
    flex: 1;
    display: flex;
    flex-direction: column;
    background: var(--bg-app);
    overflow: hidden;
    user-select: none;
    cursor: grab;
    min-height: 0;
  }
  .compare-container:active {
    cursor: grabbing;
  }
  .compare-actions-bar {
    flex: none;
    height: 34px;
    background: rgba(0, 0, 0, 0.4);
    border-bottom: 1px solid var(--border-subtle);
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 14px;
    gap: 8px;
    z-index: 5;
  }
  .action-btn-group {
    display: flex;
    gap: 4px;
  }
  .action-btn {
    all: unset;
    cursor: pointer;
    font-size: 11px;
    font-family: var(--font-mono);
    padding: 3px 8px;
    background: var(--bg-panel-raised);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-s);
    color: var(--text-secondary);
    transition: all 0.1s ease;
  }
  .action-btn:hover {
    color: #fff;
    background: var(--accent);
    border-color: var(--accent);
  }
  .reset-btn {
    color: var(--text-tertiary);
  }
  .compare-viewport {
    flex: 1;
    display: flex;
    min-height: 0;
    position: relative;
  }
  .compare-pane {
    flex: 1;
    display: flex;
    flex-direction: column;
    position: relative;
    overflow: hidden;
  }
  .compare-separator {
    width: 2px;
    background: var(--border-subtle);
    box-shadow: 0 0 6px rgba(0, 0, 0, 0.5);
    z-index: 4;
  }
  .pane-header {
    position: absolute;
    top: 10px;
    left: 10px;
    right: 10px;
    display: flex;
    align-items: center;
    gap: 8px;
    z-index: 3;
    pointer-events: none;
  }
  .pane-tag {
    font-family: var(--font-mono);
    font-size: 10px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    padding: 2px 6px;
    border-radius: var(--radius-s);
    box-shadow: 0 2px 6px rgba(0, 0, 0, 0.4);
  }
  .select-tag {
    background: var(--accent);
    color: #fff;
  }
  .candidate-tag {
    background: var(--text-secondary);
    color: #000;
  }
  .pane-name {
    font-family: var(--font-mono);
    font-size: 11px;
    color: rgba(255, 255, 255, 0.85);
    background: rgba(0, 0, 0, 0.6);
    padding: 2px 6px;
    border-radius: var(--radius-s);
  }
  .image-wrapper {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    position: relative;
    overflow: hidden;
  }
  .image-canvas {
    position: absolute;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: transform 0.04s linear;
    pointer-events: none;
  }
  .compare-img {
    max-width: 90vw;
    max-height: 80vh;
    object-fit: contain;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.5);
  }
  .pane-footer {
    position: absolute;
    bottom: 10px;
    left: 50%;
    transform: translateX(-50%);
    background: rgba(0, 0, 0, 0.75);
    backdrop-filter: blur(8px);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 999px;
    padding: 3px 10px;
    display: flex;
    align-items: center;
    gap: 8px;
    z-index: 3;
  }
  .stars {
    display: flex;
    gap: 1px;
  }
  .star {
    all: unset;
    cursor: pointer;
    font-size: 14px;
    color: rgba(255, 255, 255, 0.3);
    line-height: 1;
  }
  .star.on {
    color: var(--accent-strong);
  }
  .flags {
    display: flex;
    gap: 3px;
  }
  .flag {
    all: unset;
    cursor: pointer;
    width: 18px;
    height: 18px;
    border-radius: 50%;
    background: rgba(255, 255, 255, 0.1);
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 10px;
    color: rgba(255, 255, 255, 0.6);
  }
  .flag.pick.active {
    background: var(--label-green);
    color: #fff;
  }
  .flag.reject.active {
    background: var(--label-red);
    color: #fff;
  }
</style>
