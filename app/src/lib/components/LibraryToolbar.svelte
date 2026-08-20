<script>
  /**
   * LibraryToolbar: Bottom toolbar providing view mode switching (Grid / Loupe / Compare / Survey),
   * quick batch culling controls (Flag, Star, Color Label), zoom slider & selection summary.
   * @type {{
   *   viewMode: "grid" | "loupe" | "compare" | "survey",
   *   selectedCount: number,
   *   totalCount: number,
   *   zoomLevel?: number,
   *   onViewModeChange: (mode: "grid" | "loupe" | "compare" | "survey") => void,
   *   onRatingChange: (rating: number) => void,
   *   onFlagChange: (flag: string) => void,
   *   onColorLabelChange: (colorLabel: string) => void,
   *   onZoomChange?: (zoom: number) => void,
   *   onZoomFit?: () => void,
   *   onZoom100?: () => void,
   * }}
   */
  let {
    viewMode,
    selectedCount,
    totalCount,
    zoomLevel = 1,
    onViewModeChange,
    onRatingChange,
    onFlagChange,
    onColorLabelChange,
    onZoomChange,
    onZoomFit,
    onZoom100,
  } = $props();

  const COLOR_LABELS = [
    { name: "red", color: "var(--label-red)" },
    { name: "yellow", color: "var(--label-yellow)" },
    { name: "green", color: "var(--label-green)" },
    { name: "blue", color: "var(--label-blue)" },
    { name: "purple", color: "var(--label-purple)" },
  ];
</script>

<div class="library-toolbar">
  <!-- View Mode Switcher -->
  <div class="mode-switch-group" role="tablist" aria-label="Library View Modes">
    <button
      type="button"
      class="mode-btn"
      class:active={viewMode === "grid"}
      title="Grid View (G)"
      onclick={() => onViewModeChange("grid")}
    >
      <span class="mode-icon">田</span>
      <span class="mode-text">Grid</span>
    </button>
    <button
      type="button"
      class="mode-btn"
      class:active={viewMode === "loupe"}
      title="Loupe / Single View (E / Enter)"
      onclick={() => onViewModeChange("loupe")}
    >
      <span class="mode-icon">▢</span>
      <span class="mode-text">Loupe</span>
    </button>
    <button
      type="button"
      class="mode-btn"
      class:active={viewMode === "compare"}
      title="Compare View (C)"
      onclick={() => onViewModeChange("compare")}
    >
      <span class="mode-icon">⚏</span>
      <span class="mode-text">Compare</span>
    </button>
    <button
      type="button"
      class="mode-btn"
      class:active={viewMode === "survey"}
      title="Survey View (N)"
      onclick={() => onViewModeChange("survey")}
    >
      <span class="mode-icon">⧉</span>
      <span class="mode-text">Survey</span>
    </button>
  </div>

  <div class="divider"></div>

  <!-- Batch Culling Controls -->
  <div class="culling-controls">
    <!-- Flags -->
    <div class="cull-group flags">
      <button
        type="button"
        class="cull-btn flag-pick"
        title="Pick (P) - Apply to selection"
        onclick={() => onFlagChange("pick")}
      >
        <span class="icon">✓</span>
      </button>
      <button
        type="button"
        class="cull-btn flag-reject"
        title="Reject (X) - Apply to selection"
        onclick={() => onFlagChange("reject")}
      >
        <span class="icon">✕</span>
      </button>
      <button
        type="button"
        class="cull-btn flag-unflag"
        title="Unflag (U) - Apply to selection"
        onclick={() => onFlagChange("none")}
      >
        <span class="icon">⚐</span>
      </button>
    </div>

    <div class="sub-divider"></div>

    <!-- Stars -->
    <div class="cull-group stars">
      {#each [1, 2, 3, 4, 5] as n (n)}
        <button
          type="button"
          class="cull-btn star-btn"
          title="Rate {n} star{n === 1 ? '' : 's'} ({n}) - Apply to selection"
          onclick={() => onRatingChange(n)}
        >
          ★
        </button>
      {/each}
      <button
        type="button"
        class="cull-btn star-clear"
        title="Clear star rating (0) - Apply to selection"
        onclick={() => onRatingChange(0)}
      >
        0
      </button>
    </div>

    <div class="sub-divider"></div>

    <!-- Colors -->
    <div class="cull-group colors">
      {#each COLOR_LABELS as item (item.name)}
        <button
          type="button"
          class="color-dot-btn"
          style="background: {item.color}"
          title="{item.name.charAt(0).toUpperCase() + item.name.slice(1)} ({item.name === 'red' ? '6' : item.name === 'yellow' ? '7' : item.name === 'green' ? '8' : item.name === 'blue' ? '9' : ''})"
          onclick={() => onColorLabelChange(item.name)}
        ></button>
      {/each}
      <button
        type="button"
        class="clear-color-btn"
        title="Clear color label"
        onclick={() => onColorLabelChange("none")}
      >
        ⊘
      </button>
    </div>
  </div>

  <div class="spacer"></div>

  <!-- Zoom controls (for Loupe & Compare) -->
  {#if viewMode === "loupe" || viewMode === "compare"}
    <div class="zoom-controls">
      <button type="button" class="zoom-btn" onclick={onZoomFit} title="Fit to Viewport">Fit</button>
      <button type="button" class="zoom-btn" onclick={onZoom100} title="100% 1:1 Pixel View">100%</button>
      {#if onZoomChange}
        <input
          type="range"
          min="0.1"
          max="4"
          step="0.05"
          value={zoomLevel}
          class="zoom-slider"
          title="Zoom: {Math.round(zoomLevel * 100)}%"
          oninput={(e) => onZoomChange?.(parseFloat(e.currentTarget.value))}
        />
        <span class="zoom-pct">{Math.round(zoomLevel * 100)}%</span>
      {/if}
    </div>
    <div class="divider"></div>
  {/if}

  <!-- Selection Counter -->
  <div class="selection-info">
    {#if selectedCount > 0}
      <span class="selected-text"><strong>{selectedCount}</strong> of {totalCount} selected</span>
    {:else}
      <span class="total-text">{totalCount} photos</span>
    {/if}
  </div>
</div>

<style>
  .library-toolbar {
    flex: none;
    height: 36px;
    background: var(--bg-panel);
    border-top: 1px solid var(--border-subtle);
    display: flex;
    align-items: center;
    padding: 0 12px;
    gap: 8px;
    font-size: 11px;
    user-select: none;
    z-index: 10;
  }
  .mode-switch-group {
    display: flex;
    align-items: center;
    background: var(--bg-panel-raised);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-s);
    padding: 2px;
    gap: 2px;
  }
  .mode-btn {
    all: unset;
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 3px 8px;
    border-radius: calc(var(--radius-s) - 1px);
    color: var(--text-secondary);
    font-family: var(--font-mono);
    font-size: 11px;
    font-weight: 500;
    transition: all 0.1s ease;
  }
  .mode-btn:hover {
    color: var(--text-primary);
    background: rgba(255, 255, 255, 0.05);
  }
  .mode-btn.active {
    background: var(--accent);
    color: #fff;
    font-weight: 600;
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.3);
  }
  .mode-icon {
    font-size: 12px;
    line-height: 1;
  }
  .divider {
    width: 1px;
    height: 18px;
    background: var(--border-subtle);
    margin: 0 4px;
  }
  .sub-divider {
    width: 1px;
    height: 14px;
    background: var(--border-subtle);
    opacity: 0.6;
    margin: 0 2px;
  }
  .culling-controls {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .cull-group {
    display: flex;
    align-items: center;
    gap: 2px;
  }
  .cull-btn {
    all: unset;
    cursor: pointer;
    padding: 3px 6px;
    border-radius: var(--radius-s);
    color: var(--text-secondary);
    font-family: var(--font-mono);
    font-size: 11px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    transition: all 0.1s ease;
  }
  .cull-btn:hover {
    color: var(--text-primary);
    background: var(--bg-panel-raised);
  }
  .flag-pick:hover {
    color: var(--label-green);
  }
  .flag-reject:hover {
    color: var(--label-red);
  }
  .flag-unflag:hover {
    color: var(--text-tertiary);
  }
  .star-btn {
    font-size: 13px;
    padding: 1px 2px;
    color: var(--text-tertiary);
  }
  .star-btn:hover {
    color: var(--accent-strong);
    transform: scale(1.15);
  }
  .star-clear {
    font-size: 9.5px;
    padding: 2px 4px;
    color: var(--text-tertiary);
  }
  .star-clear:hover {
    color: var(--label-red);
  }
  .color-dot-btn {
    all: unset;
    cursor: pointer;
    width: 11px;
    height: 11px;
    border-radius: 50%;
    box-shadow: 0 0 0 1px rgba(0, 0, 0, 0.4);
    transition: transform 0.1s ease;
  }
  .color-dot-btn:hover {
    transform: scale(1.25);
  }
  .clear-color-btn {
    all: unset;
    cursor: pointer;
    font-size: 11px;
    color: var(--text-tertiary);
    padding: 0 2px;
  }
  .clear-color-btn:hover {
    color: var(--label-red);
  }
  .spacer {
    flex: 1;
  }
  .zoom-controls {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .zoom-btn {
    all: unset;
    cursor: pointer;
    padding: 2px 6px;
    background: var(--bg-panel-raised);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-s);
    font-size: 10px;
    font-family: var(--font-mono);
    color: var(--text-secondary);
  }
  .zoom-btn:hover {
    color: var(--text-primary);
    border-color: var(--border-strong);
  }
  .zoom-slider {
    width: 80px;
    height: 4px;
    accent-color: var(--accent);
    cursor: pointer;
  }
  .zoom-pct {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--text-tertiary);
    min-width: 32px;
  }
  .selection-info {
    font-family: var(--font-mono);
    font-size: 10.5px;
    color: var(--text-tertiary);
  }
  .selected-text strong {
    color: var(--accent-strong);
  }
</style>
