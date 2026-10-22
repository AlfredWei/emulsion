<script>
  import { convertFileSrc } from "@tauri-apps/api/core";

  /**
   * LibrarySurveyView: Multi-photo comparison view (Survey View 'N')
   * Displays all selected photos in a fitted multi-pane layout for quick side-by-side culling.
   * @type {{
   *   images: import('$lib/api/catalog.js').ImageSummary[],
   *   primaryId: number | null,
   *   onSetPrimary: (versionId: number) => void,
   *   onDeselect: (versionId: number) => void,
   *   onOpen: (versionId: number) => void,
   *   onRatingChange: (versionId: number, rating: number) => void,
   *   onFlagChange: (versionId: number, flag: string) => void,
   *   onColorLabelChange: (versionId: number, colorLabel: string) => void,
   *   culling?: Map<number, { best: boolean, soft: boolean, relative: number }> | null,
   *   showDeselect?: boolean,
   * }}
   *
   * `culling` (RFC-0031) is what the Similar view adds per frame (by version id): the Suggested and Soft badges and a
   * sharpness bar relative to the group's best. Without it the view is the plain Survey; `showDeselect` hides the
   * remove-from-Survey button where a frame is not the user's to remove (a group).
   */
  let {
    images,
    primaryId,
    onSetPrimary,
    onDeselect,
    onOpen,
    onRatingChange,
    onFlagChange,
    onColorLabelChange,
    culling = null,
    showDeselect = true,
  } = $props();

  let count = $derived(images.length);

  // Determine optimal grid column count to fit all images nicely in the viewport
  let gridCols = $derived.by(() => {
    if (count <= 1) return 1;
    if (count <= 2) return 2;
    if (count <= 4) return 2;
    if (count <= 6) return 3;
    if (count <= 9) return 3;
    if (count <= 16) return 4;
    return Math.ceil(Math.sqrt(count));
  });

  const COLOR_CYCLE = ["none", "red", "yellow", "green", "blue", "purple"];

  function cycleColor(/** @type {MouseEvent} */ e, /** @type {import('$lib/api/catalog.js').ImageSummary} */ img) {
    e.stopPropagation();
    const i = COLOR_CYCLE.indexOf(img.color_label);
    const next = COLOR_CYCLE[(Math.max(i, 0) + 1) % COLOR_CYCLE.length];
    onColorLabelChange(img.version_id, next);
  }
</script>

<div class="survey-container" role="region" aria-label="Survey View">
  {#if count === 0}
    <div class="empty-survey">
      <p>No photos selected for Survey view.</p>
      <span class="hint">Select multiple photos with Shift or Cmd/Ctrl and press N.</span>
    </div>
  {:else}
    <div
      class="survey-grid"
      style="grid-template-columns: repeat({gridCols}, minmax(0, 1fr));"
    >
      {#each images as image (image.version_id)}
        {@const thumbSrc = image.thumbnail_path ? convertFileSrc(image.thumbnail_path) : null}
        {@const filename = image.path.split(/[/\\]/).pop() ?? image.path}
        {@const isPrimary = image.version_id === primaryId}

        <div
          class="survey-cell"
          class:is-primary={isPrimary}
          class:is-rejected={culling !== null && image.flag === "reject"}
          role="button"
          tabindex="0"
          onclick={() => onSetPrimary(image.version_id)}
          ondblclick={() => onOpen(image.version_id)}
          onkeydown={(e) => e.key === "Enter" && onSetPrimary(image.version_id)}
        >
          <!-- Cell Header / Top Badges -->
          <div
            class="cell-top-bar"
            role="toolbar"
            aria-label="Photo header"
            tabindex="-1"
            onclick={(e) => e.stopPropagation()}
            onkeydown={(e) => e.stopPropagation()}
          >
            <span class="cell-filename truncate" title={image.path}>{filename}</span>
            {#if culling?.get(image.version_id)?.best}<span class="cull-badge best">Suggested</span>{/if}
            {#if culling?.get(image.version_id)?.soft}<span class="cull-badge soft" title="Under half as sharp as the sharpest frame of this group">Soft</span>{/if}
            {#if showDeselect}
              <button
                type="button"
                class="deselect-btn"
                title="Remove from Survey (deselect)"
                onclick={() => onDeselect(image.version_id)}
              >
                ✕
              </button>
            {/if}
          </div>

          <!-- Image Thumbnail Container -->
          <div class="cell-image-container">
            {#if thumbSrc}
              <img src={thumbSrc} alt={filename} class="survey-img" draggable="false" />
            {:else}
              <div class="survey-img placeholder"></div>
            {/if}
            {#if culling?.get(image.version_id)}
              {@const c = culling.get(image.version_id)}
              <div class="sharp-meter" class:low={c?.soft} title="Sharpness as a share of the group's best">
                <i style="width: {Math.round((c?.relative ?? 0) * 100)}%"></i>
                <span>{Math.round((c?.relative ?? 0) * 100)}% of best</span>
              </div>
            {/if}
          </div>

          <!-- Cell Bottom Culling Badges -->
          <div
            class="cell-bottom-bar"
            role="toolbar"
            aria-label="Culling badges"
            tabindex="-1"
            onclick={(e) => e.stopPropagation()}
            onkeydown={(e) => e.stopPropagation()}
          >
            <div class="stars">
              {#each [1, 2, 3, 4, 5] as n (n)}
                <button
                  type="button"
                  class="star"
                  class:on={image.rating >= n}
                  title="Rate {n} star{n === 1 ? '' : 's'}"
                  onclick={() => onRatingChange(image.version_id, image.rating === n ? 0 : n)}
                >★</button>
              {/each}
            </div>

            <div class="flags">
              <button
                type="button"
                class="flag pick"
                class:active={image.flag === "pick"}
                title="Pick (P)"
                aria-label="Pick flag"
                onclick={() => onFlagChange(image.version_id, image.flag === "pick" ? "none" : "pick")}
              >
                <svg viewBox="0 0 16 16" width="10" height="10" fill="currentColor" aria-hidden="true">
                  <path d="M3 2v12h1.5V9h7l-1.5-3.5L11.5 2H3z" />
                </svg>
              </button>
              <button
                type="button"
                class="flag reject"
                class:active={image.flag === "reject"}
                title="Reject (X)"
                aria-label="Reject flag"
                onclick={() => onFlagChange(image.version_id, image.flag === "reject" ? "none" : "reject")}
              >
                <svg viewBox="0 0 16 16" width="10" height="10" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" aria-hidden="true">
                  <path d="M4 4l8 8M12 4l-8 8" />
                </svg>
              </button>
            </div>

            <button
              type="button"
              class="color-dot"
              style={image.color_label && image.color_label !== "none" ? `background: var(--label-${image.color_label})` : ""}
              title="Color label ({image.color_label})"
              onclick={(e) => cycleColor(e, image)}
            ></button>
          </div>
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .survey-container {
    flex: 1;
    display: flex;
    flex-direction: column;
    background: var(--bg-app);
    padding: 12px;
    overflow: hidden;
    min-height: 0;
  }
  .empty-survey {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    color: var(--text-tertiary);
    font-size: 13px;
    gap: 6px;
  }
  .hint {
    font-size: 11px;
    opacity: 0.7;
  }
  .survey-grid {
    flex: 1;
    display: grid;
    gap: 12px;
    width: 100%;
    height: 100%;
    min-height: 0;
  }
  .survey-cell {
    all: unset;
    cursor: pointer;
    position: relative;
    display: flex;
    flex-direction: column;
    background: var(--bg-panel-raised);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-s);
    overflow: hidden;
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.4);
    transition: border-color 0.1s ease, box-shadow 0.1s ease;
    min-height: 0;
  }
  .survey-cell:hover {
    border-color: var(--border-strong);
  }
  .survey-cell.is-primary {
    border-color: var(--accent);
    box-shadow: 0 0 0 2px var(--accent), 0 4px 12px rgba(0, 0, 0, 0.5);
  }
  .cell-top-bar {
    flex: none;
    height: 28px;
    padding: 0 8px;
    background: linear-gradient(180deg, rgba(0, 0, 0, 0.8) 0%, rgba(0, 0, 0, 0.4) 100%);
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
    z-index: 2;
  }
  .cull-badge {
    flex: none;
    font-size: 9.5px;
    font-weight: 700;
    padding: 1px 6px;
    border-radius: 9px;
    letter-spacing: 0.02em;
  }
  .cull-badge.best {
    background: var(--accent);
    color: var(--accent-on, #1b1310);
  }
  .cull-badge.soft {
    background: #3b2623;
    color: #ff9c8c;
    border: 1px solid var(--label-red);
  }
  .survey-cell.is-rejected .cell-image-container {
    opacity: 0.35;
  }
  .sharp-meter {
    position: absolute;
    left: 8px;
    right: 8px;
    bottom: 6px;
    height: 14px;
    border-radius: 3px;
    background: rgba(0, 0, 0, 0.6);
    overflow: hidden;
  }
  .sharp-meter i {
    position: absolute;
    left: 0;
    top: 0;
    bottom: 0;
    background: var(--label-green);
    opacity: 0.55;
  }
  .sharp-meter.low i {
    background: var(--label-red);
  }
  .sharp-meter span {
    position: relative;
    font-size: 10px;
    padding-left: 6px;
    line-height: 14px;
    color: #fff;
  }
  .cell-filename {
    font-family: var(--font-mono);
    font-size: 11px;
    color: rgba(255, 255, 255, 0.9);
  }
  .deselect-btn {
    all: unset;
    cursor: pointer;
    width: 18px;
    height: 18px;
    border-radius: 50%;
    background: rgba(255, 255, 255, 0.15);
    color: #fff;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 10px;
    opacity: 0.7;
    transition: all 0.1s ease;
  }
  .deselect-btn:hover {
    opacity: 1;
    background: var(--label-red);
    transform: scale(1.15);
  }
  .cell-image-container {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    overflow: hidden;
    position: relative;
    padding: 4px;
    background: #000;
  }
  .survey-img {
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
  }
  .survey-img.placeholder {
    width: 100%;
    height: 100%;
    background: var(--bg-panel);
  }
  .cell-bottom-bar {
    flex: none;
    height: 32px;
    padding: 0 10px;
    background: rgba(0, 0, 0, 0.8);
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    z-index: 2;
  }
  .stars {
    display: flex;
    gap: 2px;
  }
  .star {
    all: unset;
    cursor: pointer;
    font-size: 14px;
    color: rgba(255, 255, 255, 0.25);
    line-height: 1;
  }
  .star.on {
    color: var(--accent-strong);
  }
  .flags {
    display: flex;
    gap: 4px;
  }
  .flag {
    all: unset;
    cursor: pointer;
    width: 20px;
    height: 20px;
    border-radius: 50%;
    background: rgba(255, 255, 255, 0.1);
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 10.5px;
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
  .color-dot {
    all: unset;
    cursor: pointer;
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: rgba(255, 255, 255, 0.25);
    box-shadow: 0 0 0 1px rgba(0, 0, 0, 0.5);
  }
</style>
