<script>
  import { ZOOM_MAX, formatZoomPercent } from "$lib/zoomMath.js";

  /**
   * Develop's zoom readout, +/-/Fit controls and navigator ("region of
   * interest" reference, as in Lightroom). Presentation only: it owns no zoom
   * state -- every button reports a `ZoomAction` (see zoomMath.js) and the
   * navigator reports a normalized focus point, and DevelopCanvas decides
   * what to do with them.
   *
   * Lives OUTSIDE the scrolling `.canvas-wrap`, as a sibling inside
   * `.canvas-stage`. Inside, an absolutely positioned child scrolls away
   * with the image once the wrap becomes `overflow: auto` -- which is how the
   * old "100%" badge ended up drawn on the photo instead of in the corner.
   *
   * @type {{
   *   scale: number,
   *   isFit: boolean,
   *   region: import('$lib/zoomMath.js').Region | null,
   *   thumbUrl: string | null,
   *   thumbImgStyle?: string,
   *   aspect: number,
   *   onZoom: (action: import('$lib/zoomMath.js').ZoomAction) => void,
   *   onNavigate: (focus: { x: number, y: number }) => void,
   * }}
   */
  let { scale, isFit, region, thumbUrl, thumbImgStyle = "", aspect, onZoom, onNavigate } = $props();

  const NAV_MAX_W = 160;
  const NAV_MAX_H = 110;

  let thumb = $derived.by(() => {
    const a = aspect > 0 ? aspect : 1.5;
    const w = a >= NAV_MAX_W / NAV_MAX_H ? NAV_MAX_W : NAV_MAX_H * a;
    return { w, h: w / a };
  });

  let canZoomIn = $derived(scale < ZOOM_MAX - 1e-3);
  let navEl = $state(/** @type {HTMLDivElement | null} */ (null));
  let dragging = false;

  /** Centre the view on the pointer's position inside the thumbnail. */
  function navigateTo(/** @type {PointerEvent} */ e) {
    if (!navEl) return;
    const r = navEl.getBoundingClientRect();
    const x = Math.min(1, Math.max(0, (e.clientX - r.left) / r.width));
    const y = Math.min(1, Math.max(0, (e.clientY - r.top) / r.height));
    onNavigate({ x, y });
  }

  function handleNavDown(/** @type {PointerEvent} */ e) {
    if (e.button !== 0) return;
    e.preventDefault();
    dragging = true;
    try {
      navEl?.setPointerCapture(e.pointerId);
    } catch {
      // Non-fatal: the drag still works while the pointer stays over the
      // thumbnail, it just isn't tracked outside it.
    }
    navigateTo(e);
  }

  function handleNavMove(/** @type {PointerEvent} */ e) {
    if (dragging) navigateTo(e);
  }

  function handleNavUp(/** @type {PointerEvent} */ e) {
    dragging = false;
    try {
      navEl?.releasePointerCapture(e.pointerId);
    } catch {
      // Releasing a capture that was never acquired throws -- non-fatal.
    }
  }
</script>

<div class="zoom-hud">
  {#if region && thumbUrl}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="navigator"
      bind:this={navEl}
      style="width:{thumb.w}px; height:{thumb.h}px;"
      aria-label="Navigator: drag to move the visible region"
      title="Drag to move the visible region"
      onpointerdown={handleNavDown}
      onpointermove={handleNavMove}
      onpointerup={handleNavUp}
      onpointercancel={handleNavUp}
    >
      <img src={thumbUrl} alt="" draggable="false" style={thumbImgStyle} />
      <div
        class="nav-region"
        style="left:{region.x * 100}%; top:{region.y * 100}%; width:{region.w * 100}%; height:{region.h * 100}%;"
      ></div>
    </div>
  {/if}
  <div class="zoom-controls" role="group" aria-label="Zoom">
    <button type="button" aria-label="Zoom out" title="Zoom out" disabled={isFit} onclick={() => onZoom({ type: "step", dir: -1 })}>−</button>
    <button type="button" class="fit" class:active={isFit} title="Fit to view" onclick={() => onZoom({ type: "fit" })}>Fit</button>
    <button type="button" class="readout" title="Zoom to 100%" onclick={() => onZoom({ type: "actual" })}>{formatZoomPercent(scale)}</button>
    <button type="button" aria-label="Zoom in" title="Zoom in" disabled={!canZoomIn} onclick={() => onZoom({ type: "step", dir: 1 })}>+</button>
  </div>
</div>

<style>
  .zoom-hud {
    position: absolute;
    right: 14px;
    bottom: 14px;
    z-index: 5;
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 6px;
    /* The stage itself must stay click-through; only the controls opt in. */
    pointer-events: none;
  }
  .navigator {
    position: relative;
    overflow: hidden;
    background: #000;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-s);
    box-shadow: 0 6px 18px -6px rgba(0, 0, 0, 0.7);
    cursor: crosshair;
    touch-action: none;
    pointer-events: auto;
  }
  .navigator img {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: fill;
    pointer-events: none;
    user-select: none;
  }
  /* The visible-region frame. The huge spread shadow dims everything outside
     it (clipped by .navigator's overflow:hidden), like Lightroom's navigator. */
  .nav-region {
    position: absolute;
    box-sizing: border-box;
    border: 1.5px solid rgba(255, 255, 255, 0.95);
    box-shadow: 0 0 0 999px rgba(0, 0, 0, 0.5);
    pointer-events: none;
  }
  .zoom-controls {
    display: flex;
    align-items: stretch;
    background: rgba(20, 18, 16, 0.78);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-s);
    pointer-events: auto;
  }
  .zoom-controls button {
    all: unset;
    box-sizing: border-box;
    min-width: 26px;
    padding: 4px 8px;
    text-align: center;
    font-family: var(--font-mono);
    font-size: 10.5px;
    letter-spacing: 0.03em;
    color: var(--text-secondary);
    cursor: pointer;
  }
  .zoom-controls button + button {
    border-left: 1px solid var(--border-subtle);
  }
  .zoom-controls button:hover:not(:disabled) {
    color: var(--text-primary);
  }
  .zoom-controls button:disabled {
    opacity: 0.35;
    cursor: default;
  }
  .zoom-controls button.active {
    color: var(--accent);
  }
  .zoom-controls button.readout {
    min-width: 46px;
  }
  .zoom-controls button:focus-visible {
    outline: 1px solid var(--accent);
    outline-offset: -1px;
  }
</style>
