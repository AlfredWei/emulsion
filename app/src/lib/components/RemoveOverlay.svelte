<script>
  // The marking surface of the Remove tool (RFC-0032 slice E): a layer over the photo (the canvas's mask overlay is exactly
  // its box) where the user paints strokes or draws an outline around what should go. Marks are kept in coordinates
  // normalised to the whole photograph (api/removal.js), so they mean the same on any preview and on the full decode.
  // Holding Space lets the canvas pan underneath; Cmd/Ctrl + wheel still zooms it.
  import { removal } from "$lib/state/removal.svelte.js";
  import { handleRemoveMark, handleRemoveUnmark, handleRemoveClear, handleRemoveRun } from "$lib/actions/removalActions.js";

  /** @type {HTMLElement | undefined} */
  let layer = $state();
  let width = $state(0);
  let height = $state(0);
  let spaceHeld = $state(false);
  /** The mark being drawn: its points so far, null when none. */
  let drawing = $state(/** @type {[number, number][] | null} */ (null));
  let cursor = $state(/** @type {[number, number] | null} */ (null));

  /** Nearest a new point may be to the last one, as a fraction of the longer side: a smooth stroke without a point per pixel. */
  const MIN_STEP = 0.0015;

  const long = $derived(Math.max(width, height) || 1);
  const px = (/** @type {[number, number][]} */ pts) => pts.map(([x, y]) => `${x * width},${y * height}`).join(" ");

  /** @param {PointerEvent} e @returns {[number, number]} */
  function norm(e) {
    const r = /** @type {HTMLElement} */ (layer).getBoundingClientRect();
    const clamp = (/** @type {number} */ v) => Math.min(1, Math.max(0, v));
    return [clamp((e.clientX - r.left) / r.width), clamp((e.clientY - r.top) / r.height)];
  }

  /** @param {PointerEvent} e */
  function down(e) {
    if (e.button !== 0 || removal.phase === "running" || removal.available === false || !layer) return;
    e.preventDefault();
    e.stopPropagation();
    try {
      layer.setPointerCapture(e.pointerId);
    } catch {
      // A synthetic pointer has nothing to capture; drawing still works.
    }
    drawing = [norm(e)];
  }

  /** @param {PointerEvent} e */
  function move(e) {
    if (!layer) return;
    const p = norm(e);
    cursor = p;
    if (!drawing) return;
    const last = drawing[drawing.length - 1];
    if (Math.hypot((p[0] - last[0]) * width, (p[1] - last[1]) * height) / long >= MIN_STEP) drawing = [...drawing, p];
  }

  /** @param {PointerEvent} e */
  function up(e) {
    if (!drawing || !layer) return;
    const points = drawing;
    drawing = null;
    try {
      layer.releasePointerCapture(e.pointerId);
    } catch {
      // see down()
    }
    if (removal.mode === "brush") handleRemoveMark({ kind: "stroke", points, radius: removal.brushRadius });
    else if (points.length >= 3) handleRemoveMark({ kind: "polygon", points });
  }

  /** Cmd/Ctrl + wheel zooms the photo: the canvas listens for it, so it is handed on. @param {WheelEvent} e */
  function wheel(e) {
    if (!(e.ctrlKey || e.metaKey)) return;
    const canvas = document.querySelector(".canvas-wrap canvas");
    if (!canvas) return;
    e.preventDefault();
    canvas.dispatchEvent(new WheelEvent("wheel", { deltaX: e.deltaX, deltaY: e.deltaY, deltaMode: e.deltaMode, clientX: e.clientX, clientY: e.clientY, ctrlKey: e.ctrlKey, metaKey: e.metaKey, bubbles: true, cancelable: true }));
  }

  /** @param {KeyboardEvent} e */
  function keydown(e) {
    const t = /** @type {HTMLElement | null} */ (e.target);
    if (t && (t.tagName === "INPUT" || t.tagName === "TEXTAREA" || t.isContentEditable)) return;
    if (e.code === "Space") spaceHeld = true;
    else if (e.key === "Escape" && removal.shapes.length > 0) {
      e.preventDefault();
      handleRemoveClear();
    } else if (e.key === "Backspace" && removal.shapes.length > 0) {
      e.preventDefault();
      handleRemoveUnmark();
    } else if (e.key === "Enter" && removal.shapes.length > 0 && !e.metaKey && !e.ctrlKey) {
      e.preventDefault();
      void handleRemoveRun(removal.guarding);
    }
  }

  /** @param {KeyboardEvent} e */
  function keyup(e) {
    if (e.code === "Space") spaceHeld = false;
  }
</script>

<svelte:window onkeydown={keydown} onkeyup={keyup} onblur={() => (spaceHeld = false)} />

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="remove-layer"
  class:pass={spaceHeld}
  class:busy={removal.phase === "running"}
  class:off={removal.available === false}
  data-testid="remove-layer"
  bind:this={layer}
  bind:clientWidth={width}
  bind:clientHeight={height}
  onpointerdown={down}
  onpointermove={move}
  onpointerup={up}
  onpointercancel={up}
  onpointerleave={() => (cursor = null)}
  onwheel={wheel}
>
  <svg {width} {height} aria-hidden="true">
    {#each removal.shapes as shape, i (i)}
      {#if shape.kind === "stroke"}
        {#if shape.points.length === 1}
          <circle class="mark" cx={shape.points[0][0] * width} cy={shape.points[0][1] * height} r={shape.radius * long} />
        {:else}
          <polyline class="mark-stroke" points={px(shape.points)} stroke-width={shape.radius * long * 2} />
        {/if}
      {:else}
        <polygon class="mark" points={px(shape.points)} />
      {/if}
    {/each}
    {#if drawing}
      {#if removal.mode === "brush"}
        {#if drawing.length === 1}
          <circle class="mark" cx={drawing[0][0] * width} cy={drawing[0][1] * height} r={removal.brushRadius * long} />
        {:else}
          <polyline class="mark-stroke" points={px(drawing)} stroke-width={removal.brushRadius * long * 2} />
        {/if}
      {:else}
        <polyline class="outline" points={px(drawing)} />
      {/if}
    {/if}
    {#if cursor && removal.mode === "brush" && !spaceHeld && removal.phase !== "running"}
      <circle class="cursor" cx={cursor[0] * width} cy={cursor[1] * height} r={removal.brushRadius * long} />
    {/if}
  </svg>
</div>

<style>
  .remove-layer {
    position: absolute;
    inset: 0;
    pointer-events: auto;
    cursor: crosshair;
    touch-action: none;
  }
  .remove-layer.pass {
    pointer-events: none;
  }
  .remove-layer.busy {
    cursor: progress;
  }
  .remove-layer.off {
    cursor: not-allowed;
  }
  svg {
    position: absolute;
    inset: 0;
    pointer-events: none;
    overflow: hidden;
  }
  .mark {
    fill: rgba(255, 70, 70, 0.4);
    stroke: rgba(255, 70, 70, 0.9);
    stroke-width: 1;
  }
  .mark-stroke {
    fill: none;
    stroke: rgba(255, 70, 70, 0.4);
    stroke-linecap: round;
    stroke-linejoin: round;
  }
  .outline {
    fill: rgba(255, 70, 70, 0.15);
    stroke: rgba(255, 255, 255, 0.95);
    stroke-width: 1.5;
    stroke-dasharray: 5 4;
  }
  .cursor {
    fill: none;
    stroke: rgba(255, 255, 255, 0.9);
    stroke-width: 1;
  }
  .busy .mark,
  .busy .mark-stroke {
    animation: pulse 1.2s ease-in-out infinite;
  }
  @keyframes pulse {
    50% {
      opacity: 0.55;
    }
  }
</style>
