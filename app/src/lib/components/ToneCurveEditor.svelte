<script>
  import { buildToneCurveLut, insertToneCurvePoint, MIN_TONE_CURVE_X_GAP } from "$lib/api/develop.js";

  /**
   * Master RGB point-curve editor (M3): the graph draws from the SAME
   * `buildToneCurveLut` output DevelopCanvas.svelte uploads to the GPU, so
   * the drawn curve always exactly matches what's actually applied -- not
   * a separately-evaluated approximation that could visibly disagree with
   * the real render.
   *
   * Normalized (0-1) point space, x=tonal input, y=tonal output --
   * mapped to a 0..200 SVG viewBox with y inverted (SVG y grows downward,
   * a tone curve graph's y grows upward, low-left to high-right for the
   * default identity line).
   * @type {{
   *   points: readonly {x: number, y: number}[],
   *   onChange: (points: readonly {x: number, y: number}[]) => void,
   * }}
   */
  let { points, onChange } = $props();

  const SIZE = 200;

  let svgEl = $state(/** @type {SVGSVGElement | null} */ (null));

  // Defensive sort -- upsertToneCurve/getToneCurvePoints store whatever's
  // given verbatim, they don't sort. Every mutation below emits an
  // already-sorted array (so round-tripping through persistence keeps
  // this a no-op in practice), but drawing/interaction here always reads
  // through this sorted view rather than assuming the prop already is one.
  let sorted = $derived([...points].sort((a, b) => a.x - b.x));

  let lut = $derived(buildToneCurveLut(sorted));
  let curvePath = $derived(
    Array.from(lut, (v, i) => `${(i / (lut.length - 1)) * SIZE},${(1 - v) * SIZE}`).join(" "),
  );

  /** @type {number | null} */
  let draggingIndex = null;

  function normalizedFromEvent(/** @type {PointerEvent | MouseEvent} */ e) {
    if (!svgEl) return { x: 0, y: 0 };
    const rect = svgEl.getBoundingClientRect();
    const x = (e.clientX - rect.left) / rect.width;
    const y = 1 - (e.clientY - rect.top) / rect.height;
    return { x: Math.min(Math.max(x, 0), 1), y: Math.min(Math.max(y, 0), 1) };
  }

  function addPointAt(/** @type {{x: number, y: number}} */ p) {
    const next = insertToneCurvePoint(sorted, p.x, p.y);
    if (next !== sorted) onChange(next);
  }

  /** Click on the background (not a control point -- circles sit on top
   * in DOM order, so a click directly on one hits the circle first and
   * never reaches this handler) adds a new interior point there. */
  function handleBackgroundClick(/** @type {MouseEvent} */ e) {
    addPointAt(normalizedFromEvent(e));
  }

  /** Keyboard equivalent of the click-to-add gesture: since a keyboard
   * event has no cursor position, Enter/Space adds a point at the curve's
   * own current midpoint value (x=0.5, y=the curve's own value there) --
   * a defined, real action rather than a no-op that would only exist to
   * silence a linter. */
  function handleBackgroundKeydown(/** @type {KeyboardEvent} */ e) {
    if (e.key !== "Enter" && e.key !== " ") return;
    e.preventDefault();
    addPointAt({ x: 0.5, y: lut[Math.round((lut.length - 1) / 2)] });
  }

  /** Mirrors DevelopCanvas.svelte's handleMaskHandlePointerDown ordering
   * exactly: drag state set FIRST, `setPointerCapture` second, wrapped in
   * try/catch -- a real, previously-hit bug in this exact class of drag
   * interaction proved a throw from `setPointerCapture` can silently abort
   * everything after it if called first. */
  function handlePointDown(/** @type {PointerEvent} */ e, /** @type {number} */ index) {
    e.stopPropagation();
    e.preventDefault();
    draggingIndex = index;
    try {
      /** @type {HTMLElement} */ (e.currentTarget).setPointerCapture(e.pointerId);
    } catch {
      // Non-fatal, see above.
    }
  }

  function handlePointMove(/** @type {PointerEvent} */ e) {
    if (draggingIndex === null) return;
    const p = normalizedFromEvent(e);
    const isEndpoint = draggingIndex === 0 || draggingIndex === sorted.length - 1;
    const next = sorted.map((pt, i) => {
      if (i !== draggingIndex) return pt;
      if (isEndpoint) return { x: pt.x, y: p.y };
      const minX = sorted[i - 1].x + MIN_TONE_CURVE_X_GAP;
      const maxX = sorted[i + 1].x - MIN_TONE_CURVE_X_GAP;
      return { x: Math.min(Math.max(p.x, minX), maxX), y: p.y };
    });
    onChange(next);
  }

  function handlePointUp(/** @type {PointerEvent} */ e) {
    try {
      /** @type {HTMLElement} */ (e.currentTarget).releasePointerCapture(e.pointerId);
    } catch {
      // Releasing a capture never successfully acquired would itself
      // throw -- non-fatal, see handlePointDown's comment.
    }
    draggingIndex = null;
  }

  /** Double-click removes an INTERIOR point -- endpoints (index 0 or
   * last) are permanent anchors, matching real Lightroom's own point
   * curve model. Standard point-curve-editor convention (Photoshop/
   * Lightroom/ACR all use double-click-to-delete), proposed on its own
   * merits -- this codebase otherwise always uses a visible Delete button
   * (MaskEditorPanel.svelte), so there's no existing hidden-gesture
   * precedent this is matching, just the conventional choice for this
   * specific kind of widget. */
  function handlePointDoubleClick(/** @type {number} */ index) {
    if (index === 0 || index === sorted.length - 1) return;
    onChange(sorted.filter((_, i) => i !== index));
  }

  /** Keyboard equivalent of dragging/deleting a point -- Up/Down (Shift
   * for a bigger step) nudge this point's y; Delete/Backspace removes it
   * (interior points only, same rule as the double-click gesture). */
  function handlePointKeydown(/** @type {KeyboardEvent} */ e, /** @type {number} */ index) {
    const isEndpoint = index === 0 || index === sorted.length - 1;
    if (e.key === "ArrowUp" || e.key === "ArrowDown") {
      e.preventDefault();
      const step = e.shiftKey ? 0.05 : 0.01;
      const delta = e.key === "ArrowUp" ? step : -step;
      const next = sorted.map((pt, i) =>
        i === index ? { ...pt, y: Math.min(Math.max(pt.y + delta, 0), 1) } : pt,
      );
      onChange(next);
    } else if ((e.key === "Delete" || e.key === "Backspace") && !isEndpoint) {
      e.preventDefault();
      onChange(sorted.filter((_, i) => i !== index));
    }
  }
</script>

<svg
  bind:this={svgEl}
  class="curve"
  viewBox="0 0 {SIZE} {SIZE}"
  role="img"
  aria-label="Tone curve editor"
>
  <rect
    class="bg"
    x="0"
    y="0"
    width={SIZE}
    height={SIZE}
    role="button"
    tabindex="0"
    aria-label="Add a curve point (click anywhere on the graph, or press Enter to add one at the midpoint)"
    onclick={handleBackgroundClick}
    onkeydown={handleBackgroundKeydown}
  />
  <line class="grid" x1="0" y1={SIZE / 2} x2={SIZE} y2={SIZE / 2} />
  <line class="grid" x1={SIZE / 2} y1="0" x2={SIZE / 2} y2={SIZE} />
  <polyline class="curve-line" points={curvePath} />
  {#each sorted as point, i (i)}
    <circle
      class="point"
      class:endpoint={i === 0 || i === sorted.length - 1}
      cx={point.x * SIZE}
      cy={(1 - point.y) * SIZE}
      r="4"
      role="slider"
      tabindex="0"
      aria-label="Curve point at input {(point.x * 100).toFixed(0)}%"
      aria-valuemin="0"
      aria-valuemax="1"
      aria-valuenow={point.y}
      onpointerdown={(e) => handlePointDown(e, i)}
      onpointermove={handlePointMove}
      onpointerup={handlePointUp}
      ondblclick={() => handlePointDoubleClick(i)}
      onkeydown={(e) => handlePointKeydown(e, i)}
    />
  {/each}
</svg>

<style>
  .curve {
    display: block;
    width: 100%;
    aspect-ratio: 1;
    background: var(--bg-panel-raised);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-s);
    touch-action: none;
  }
  .bg {
    fill: transparent;
    cursor: crosshair;
    outline: none;
  }
  .bg:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }
  .point:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .grid {
    stroke: var(--border-subtle);
    stroke-width: 1;
    pointer-events: none;
  }
  .curve-line {
    fill: none;
    stroke: var(--accent-strong);
    stroke-width: 1.5;
    pointer-events: none;
  }
  .point {
    fill: var(--accent-strong);
    stroke: var(--bg-panel);
    stroke-width: 1.5;
    cursor: grab;
  }
  .point.endpoint {
    fill: var(--text-tertiary);
  }
</style>
