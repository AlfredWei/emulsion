<script>
  import { onMount, tick } from "svelte";
  import { convertFileSrc } from "@tauri-apps/api/core";
  import { getDevelopPreview, getDevelopFullPreview } from "$lib/api/develop.js";

  /**
   * LibraryImageViewer: High-resolution single image viewer (Loupe View)
   * with pan & zoom, 1:1 crisp preview loading, culling HUD, and quick navigation.
   * @type {{
   *   image: import('$lib/api/catalog.js').ImageSummary,
   *   hasPrev?: boolean,
   *   hasNext?: boolean,
   *   onPrev?: () => void,
   *   onNext?: () => void,
   *   onRatingChange?: (rating: number) => void,
   *   onFlagChange?: (flag: string) => void,
   *   onColorLabelChange?: (colorLabel: string) => void,
   *   onOpenDevelop?: () => void,
   *   zoomLevel?: number,
   *   onZoomChange?: (zoom: number) => void,
   * }}
   */
  let {
    image,
    hasPrev = false,
    hasNext = false,
    onPrev,
    onNext,
    onRatingChange,
    onFlagChange,
    onColorLabelChange,
    onOpenDevelop,
    zoomLevel,
    onZoomChange,
  } = $props();

  let containerEl = $state(/** @type {HTMLDivElement | null} */ (null));
  let containerWidth = $state(0);
  let containerHeight = $state(0);

  let previewUrl = $state(/** @type {string | null} */ (null));
  let fullPreviewUrl = $state(/** @type {string | null} */ (null));
  let naturalWidth = $state(0);
  let naturalHeight = $state(0);
  let loadingPreview = $state(false);

  // Pan & Zoom state
  let isFit = $state(true);
  let scale = $state(1);
  let panX = $state(0);
  let panY = $state(0);
  let isDragging = $state(false);
  let dragStartX = 0;
  let dragStartY = 0;
  let startPanX = 0;
  let startPanY = 0;
  let spaceHeld = $state(false);

  // Fit scale calculation
  let fitScale = $derived.by(() => {
    if (!naturalWidth || !naturalHeight || !containerWidth || !containerHeight) return 1;
    const padding = 32; // breathing room
    const availW = Math.max(10, containerWidth - padding);
    const availH = Math.max(10, containerHeight - padding);
    return Math.min(availW / naturalWidth, availH / naturalHeight, 1);
  });

  // Effective scale
  let effectiveScale = $derived(isFit ? fitScale : scale);

  // Sync zoomLevel from parent if supplied
  $effect(() => {
    if (zoomLevel !== undefined && zoomLevel !== null && !isFit) {
      if (Math.abs(scale - zoomLevel) > 0.01) {
        scale = zoomLevel;
      }
    }
  });

  let thumbPlaceholder = $derived(image.thumbnail_path ? convertFileSrc(image.thumbnail_path) : null);
  let filename = $derived(image.path.split(/[/\\]/).pop() ?? image.path);

  // Load preview whenever image changes
  $effect(() => {
    const currentPath = image.path;
    let cancelled = false;
    loadingPreview = true;
    previewUrl = null;
    fullPreviewUrl = null;
    isFit = true;
    panX = 0;
    panY = 0;

    getDevelopPreview(currentPath)
      .then((preview) => {
        if (cancelled) return;
        previewUrl = convertFileSrc(preview.path);
        naturalWidth = preview.width;
        naturalHeight = preview.height;
        loadingPreview = false;
      })
      .catch(() => {
        if (cancelled) return;
        loadingPreview = false;
      });

    return () => {
      cancelled = true;
    };
  });

  // When zooming to >= 100%, lazily fetch full 1:1 preview
  $effect(() => {
    if (!isFit && effectiveScale >= 0.95 && previewUrl && !fullPreviewUrl) {
      const currentPath = image.path;
      getDevelopFullPreview(currentPath)
        .then((fullPreview) => {
          if (image.path === currentPath) {
            fullPreviewUrl = convertFileSrc(fullPreview.path);
            naturalWidth = fullPreview.width;
            naturalHeight = fullPreview.height;
          }
        })
        .catch(() => {});
    }
  });

  export function zoomToFit() {
    isFit = true;
    panX = 0;
    panY = 0;
    onZoomChange?.(fitScale);
  }

  export function zoomTo100() {
    isFit = false;
    scale = 1;
    panX = 0;
    panY = 0;
    onZoomChange?.(1);
  }

  export function setCustomZoom(/** @type {number} */ val) {
    isFit = false;
    scale = Math.max(0.1, Math.min(8.0, val));
    onZoomChange?.(scale);
  }

  function handleToggleZoom(/** @type {MouseEvent} */ e) {
    if (isFit) {
      // Zoom into 100% centered around click location
      const rect = containerEl?.getBoundingClientRect();
      if (rect) {
        const clickX = e.clientX - rect.left - rect.width / 2;
        const clickY = e.clientY - rect.top - rect.height / 2;
        scale = 1;
        panX = -clickX * (1 / fitScale - 1);
        panY = -clickY * (1 / fitScale - 1);
      } else {
        scale = 1;
        panX = 0;
        panY = 0;
      }
      isFit = false;
      onZoomChange?.(1);
    } else {
      zoomToFit();
    }
  }

  function handleWheel(/** @type {WheelEvent} */ e) {
    e.preventDefault();
    if (!containerEl) return;
    const rect = containerEl.getBoundingClientRect();
    const cursorX = e.clientX - rect.left - rect.width / 2;
    const cursorY = e.clientY - rect.top - rect.height / 2;

    const currentS = isFit ? fitScale : scale;
    const delta = e.deltaY < 0 ? 1.15 : 0.87;
    const targetScale = Math.max(0.1, Math.min(6.0, currentS * delta));

    if (Math.abs(targetScale - fitScale) < 0.05 && delta < 1) {
      zoomToFit();
      return;
    }

    isFit = false;
    panX = cursorX - (cursorX - panX) * (targetScale / currentS);
    panY = cursorY - (cursorY - panY) * (targetScale / currentS);
    scale = targetScale;
    onZoomChange?.(scale);
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

  function handleMouseUp(/** @type {MouseEvent} */ e) {
    if (!isDragging) return;
    const moved = Math.hypot(e.clientX - dragStartX, e.clientY - dragStartY);
    isDragging = false;
    // If it was just a click (not a drag), toggle zoom
    if (moved < 5) {
      handleToggleZoom(e);
    }
  }

  function handleKeyDown(/** @type {KeyboardEvent} */ e) {
    if (e.key === " " && !e.repeat) spaceHeld = true;
  }

  function handleKeyUp(/** @type {KeyboardEvent} */ e) {
    if (e.key === " ") spaceHeld = false;
  }
</script>

<svelte:window onkeydown={handleKeyDown} onkeyup={handleKeyUp} />

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div
  class="image-viewer"
  bind:this={containerEl}
  bind:clientWidth={containerWidth}
  bind:clientHeight={containerHeight}
  onwheel={handleWheel}
  onmousedown={handleMouseDown}
  onmousemove={handleMouseMove}
  onmouseup={handleMouseUp}
  class:is-panning={isDragging || spaceHeld}
  class:zoomed={!isFit}
  role="region"
  aria-label="Image Loupe View"
>
  <!-- Main Display Image -->
  <div
    class="image-canvas"
    style="transform: translate({panX}px, {panY}px) scale({effectiveScale});"
  >
    {#if fullPreviewUrl || previewUrl}
      <img
        src={fullPreviewUrl || previewUrl}
        alt={filename}
        class="main-img"
        draggable="false"
      />
    {:else if thumbPlaceholder}
      <img
        src={thumbPlaceholder}
        alt={filename}
        class="main-img placeholder"
        draggable="false"
      />
    {/if}
  </div>

  <!-- Loading indicator -->
  {#if loadingPreview}
    <div class="loading-overlay">
      <div class="spinner"></div>
    </div>
  {/if}

  <!-- Navigation Arrows -->
  {#if hasPrev}
    <button
      type="button"
      class="nav-btn prev-btn"
      title="Previous photo (Left Arrow)"
      onclick={(e) => {
        e.stopPropagation();
        onPrev?.();
      }}
    >
      ‹
    </button>
  {/if}
  {#if hasNext}
    <button
      type="button"
      class="nav-btn next-btn"
      title="Next photo (Right Arrow)"
      onclick={(e) => {
        e.stopPropagation();
        onNext?.();
      }}
    >
      ›
    </button>
  {/if}

  <!-- Top-Left Info Overlay -->
  <div class="info-hud">
    <div class="hud-filename">{filename}</div>
    <div class="hud-meta">
      {#if image.width && image.height}
        <span>{image.width} × {image.height}</span>
      {/if}
      {#if image.iso}
        <span>ISO {image.iso}</span>
      {/if}
      {#if image.aperture}
        <span>f/{image.aperture.toFixed(1)}</span>
      {/if}
      <span class="hud-zoom">{Math.round(effectiveScale * 100)}%</span>
    </div>
  </div>

  <!-- Bottom Culling Quick Overlay -->
  <div
    class="cull-hud"
    role="toolbar"
    aria-label="Culling actions"
    tabindex="-1"
    onclick={(e) => e.stopPropagation()}
    onkeydown={(e) => e.stopPropagation()}
  >
    <!-- Stars -->
    <div class="hud-stars">
      {#each [1, 2, 3, 4, 5] as n (n)}
        <button
          type="button"
          class="hud-star"
          class:on={image.rating >= n}
          title="Rate {n} star ({n})"
          onclick={() => onRatingChange?.(image.rating === n ? 0 : n)}
        >★</button>
      {/each}
    </div>

    <!-- Flags -->
    <div class="hud-flags">
      <button
        type="button"
        class="hud-flag pick"
        class:active={image.flag === "pick"}
        title="Pick (P)"
        onclick={() => onFlagChange?.(image.flag === "pick" ? "none" : "pick")}
      >✓</button>
      <button
        type="button"
        class="hud-flag reject"
        class:active={image.flag === "reject"}
        title="Reject (X)"
        onclick={() => onFlagChange?.(image.flag === "reject" ? "none" : "reject")}
      >✕</button>
    </div>

    <!-- Color Label -->
    {#if image.color_label && image.color_label !== "none"}
      <div
        class="hud-color-indicator"
        style="background: var(--label-{image.color_label})"
        title="Color: {image.color_label}"
      ></div>
    {/if}

    <!-- Develop Button -->
    {#if onOpenDevelop}
      <button
        type="button"
        class="hud-develop-btn"
        title="Edit in Develop (D)"
        onclick={onOpenDevelop}
      >
        Develop →
      </button>
    {/if}
  </div>
</div>

<style>
  .image-viewer {
    flex: 1;
    position: relative;
    background: var(--bg-app);
    overflow: hidden;
    display: flex;
    align-items: center;
    justify-content: center;
    user-select: none;
    cursor: zoom-in;
    min-height: 0;
  }
  .image-viewer.zoomed {
    cursor: grab;
  }
  .image-viewer.is-panning {
    cursor: grabbing;
  }
  .image-canvas {
    position: absolute;
    display: flex;
    align-items: center;
    justify-content: center;
    transform-origin: center center;
    transition: transform 0.05s linear;
    pointer-events: none;
  }
  .main-img {
    max-width: none;
    max-height: none;
    display: block;
    box-shadow: 0 4px 20px rgba(0, 0, 0, 0.6);
    border-radius: 2px;
  }
  .main-img.placeholder {
    filter: blur(4px);
    opacity: 0.8;
  }
  .loading-overlay {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(0, 0, 0, 0.2);
    pointer-events: none;
  }
  .spinner {
    width: 28px;
    height: 28px;
    border: 3px solid rgba(255, 255, 255, 0.2);
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  .nav-btn {
    all: unset;
    position: absolute;
    top: 50%;
    transform: translateY(-50%);
    width: 44px;
    height: 64px;
    background: rgba(0, 0, 0, 0.35);
    backdrop-filter: blur(8px);
    color: rgba(255, 255, 255, 0.8);
    font-size: 32px;
    font-weight: 300;
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    border-radius: var(--radius-s);
    opacity: 0;
    transition: all 0.15s ease;
    z-index: 5;
  }
  .prev-btn {
    left: 12px;
  }
  .next-btn {
    right: 12px;
  }
  .image-viewer:hover .nav-btn {
    opacity: 0.7;
  }
  .nav-btn:hover {
    opacity: 1 !important;
    background: rgba(0, 0, 0, 0.6);
    color: #fff;
    transform: translateY(-50%) scale(1.05);
  }
  .info-hud {
    position: absolute;
    top: 14px;
    left: 14px;
    background: rgba(0, 0, 0, 0.65);
    backdrop-filter: blur(10px);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: var(--radius-s);
    padding: 6px 10px;
    display: flex;
    flex-direction: column;
    gap: 3px;
    pointer-events: none;
    z-index: 4;
    box-shadow: 0 2px 10px rgba(0, 0, 0, 0.4);
  }
  .hud-filename {
    font-family: var(--font-mono);
    font-size: 11.5px;
    font-weight: 600;
    color: rgba(255, 255, 255, 0.95);
  }
  .hud-meta {
    display: flex;
    align-items: center;
    gap: 8px;
    font-family: var(--font-mono);
    font-size: 10.5px;
    color: rgba(255, 255, 255, 0.65);
  }
  .hud-zoom {
    color: var(--accent-strong);
    font-weight: 600;
  }
  .cull-hud {
    position: absolute;
    bottom: 14px;
    background: rgba(18, 18, 20, 0.8);
    backdrop-filter: blur(12px);
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 999px;
    padding: 4px 12px;
    display: flex;
    align-items: center;
    gap: 12px;
    z-index: 4;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.5);
  }
  .hud-stars {
    display: flex;
    gap: 2px;
  }
  .hud-star {
    all: unset;
    cursor: pointer;
    font-size: 16px;
    line-height: 1;
    color: rgba(255, 255, 255, 0.3);
    transition: transform 0.1s ease, color 0.1s ease;
  }
  .hud-star:hover,
  .hud-star.on {
    color: var(--accent-strong);
    transform: scale(1.15);
  }
  .hud-flags {
    display: flex;
    gap: 4px;
  }
  .hud-flag {
    all: unset;
    cursor: pointer;
    width: 22px;
    height: 22px;
    border-radius: 50%;
    background: rgba(255, 255, 255, 0.08);
    color: rgba(255, 255, 255, 0.6);
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 11px;
    transition: all 0.1s ease;
  }
  .hud-flag.pick.active {
    background: var(--label-green);
    color: #fff;
  }
  .hud-flag.reject.active {
    background: var(--label-red);
    color: #fff;
  }
  .hud-color-indicator {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    box-shadow: 0 0 0 1px rgba(0, 0, 0, 0.5);
  }
  .hud-develop-btn {
    all: unset;
    cursor: pointer;
    font-family: var(--font-mono);
    font-size: 10.5px;
    font-weight: 500;
    color: var(--accent-strong);
    background: var(--accent-soft);
    border: 1px solid var(--accent-border);
    padding: 3px 8px;
    border-radius: 999px;
    transition: all 0.1s ease;
  }
  .hud-develop-btn:hover {
    background: var(--accent);
    color: #fff;
  }
</style>
