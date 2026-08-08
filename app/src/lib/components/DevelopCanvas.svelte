<script>
  import { tick } from "svelte";
  import { convertFileSrc } from "@tauri-apps/api/core";
  import { getDevelopPreview, getDevelopFullPreview, buildToneCurveLut, buildHslUniformData, buildSplitToningUniformData, buildVignetteUniformData, buildGrainUniformData, buildSharpenUniformData, buildLumaNrUniformData, buildColorNrUniformData } from "$lib/api/develop.js";

  const MAX_MASKS = 8;

  /**
   * @type {{
   *   imagePath: string,
   *   exposure: number,
   *   contrast: number,
   *   saturation: number,
   *   masks: import('$lib/api/develop.js').Mask[],
   *   activeTool: string | null,
   *   selectedMaskId: string | null,
   *   brushSize: number,
   *   brushHardness: number,
   *   brushFlow: number,
   *   eraseMode: boolean,
   *   showMaskOverlay: boolean,
   *   onMaskCreated: (placement:
   *     | { kind: "linear_gradient", start: {x: number, y: number}, end: {x: number, y: number} }
   *     | { kind: "radial_gradient", center: {x: number, y: number}, radiusX: number, radiusY: number }
   *     | { kind: "brush", id: string }
   *     | { kind: "color_range", refColor: {r: number, g: number, b: number} }
   *   ) => void,
   *   onMaskUpdated: (id: string, patch: Partial<import('$lib/api/develop.js').Mask>) => void,
   *   onMaskSelected: (id: string) => void,
   *   colorRangeResampleId: string | null,
   *   onColorRangeResampled: (id: string, refColor: {r: number, g: number, b: number}) => void,
   *   onEyedropperSampled: (color: {r: number, g: number, b: number}) => void,
   *   toneCurvePoints: readonly {x: number, y: number}[],
   *   hslBands: Readonly<Record<string, {hue: number, saturation: number, luminance: number}>>,
   *   splitToning: {shadows: {hue: number, saturation: number}, highlights: {hue: number, saturation: number}, balance: number},
   *   dehaze: number,
   *   texture: number,
   *   clarity: number,
   *   vignette: {amount: number, midpoint: number, feather: number},
   *   grain: {amount: number, size: number, roughness: number},
   *   sharpen: {amount: number, radius: number, detail: number, masking: number},
   *   lumaNR: {amount: number, detail: number, contrast: number},
   *   colorNR: {amount: number, detail: number},
   * }}
   */
  let {
    imagePath,
    exposure,
    contrast,
    saturation,
    masks,
    activeTool,
    selectedMaskId,
    brushSize,
    brushHardness,
    brushFlow,
    eraseMode,
    showMaskOverlay,
    onMaskCreated,
    onMaskUpdated,
    onMaskSelected,
    colorRangeResampleId,
    onColorRangeResampled,
    onEyedropperSampled,
    toneCurvePoints,
    hslBands,
    splitToning,
    dehaze,
    texture,
    clarity,
    vignette,
    grain,
    sharpen,
    lumaNR,
    colorNR,
  } = $props();

  let canvasEl = $state(/** @type {HTMLCanvasElement | null} */ (null));
  let wrapEl = $state(/** @type {HTMLDivElement | null} */ (null));
  let overlayEl = $state(/** @type {HTMLDivElement | null} */ (null));
  let status = $state("loading"); // "loading" | "ready" | "error"
  let errorMessage = $state("");

  // M3 Slice 5 (fixed after an empirical regression report): the canvas
  // itself stays the sized flex item, exactly as it was before this slice
  // -- a <canvas> is a genuine replaced element with its own intrinsic
  // aspect ratio, so max-width/max-height/margin:auto alone size it
  // correctly under flexbox's default `align-items: stretch`. The FIRST
  // version of this slice instead wrapped canvas in a plain <div> sized via
  // a CSS `aspect-ratio` hint so the mask overlay would have a predictable
  // box to size against -- but a plain div has no genuine intrinsic aspect
  // ratio, and under flex stretch its height could get resolved
  // independently of the aspect-ratio hint, non-uniformly stretching the
  // canvas rendered inside it. Fixed by reverting canvas to direct sizing
  // and instead syncing the mask overlay's position/size to canvas's own
  // (correct) rendered box via ResizeObserver, which fires on every resize
  // reason (image load, zoom toggle, window resize) with no scroll
  // listener needed (offsetLeft/offsetTop are scroll-independent).
  function syncOverlayPosition() {
    if (!canvasEl || !overlayEl) return;
    overlayEl.style.left = `${canvasEl.offsetLeft}px`;
    overlayEl.style.top = `${canvasEl.offsetTop}px`;
    overlayEl.style.width = `${canvasEl.offsetWidth}px`;
    overlayEl.style.height = `${canvasEl.offsetHeight}px`;
  }

  $effect(() => {
    if (!canvasEl) return;
    const observer = new ResizeObserver(syncOverlayPosition);
    observer.observe(canvasEl);
    return () => observer.disconnect();
  });

  // M3 Slice 3: basic pan/zoom. "fit" is today's existing behavior; "100"
  // is true 1:1 canvas-backing-store pixels, scrollable via the browser's
  // own native scroll clamping rather than hand-rolled pan math.
  let zoomMode = $state("fit"); // "fit" | "100"

  // 1:1 preview tier (mirrors real Lightroom's Standard/1:1 Preview split,
  // PRD/PRD.md's own explicit phrasing): "fit" always shows the draft tier
  // (getDevelopPreview, capped to DEVELOP_PREVIEW_MAX_DIMENSION on the
  // Rust side); "100" lazily upgrades to a true native-resolution texture
  // the FIRST time an image is zoomed in, so 100% actually shows finer
  // detail rather than just CSS-magnifying the same capped preview. Once
  // upgraded, stays upgraded for the rest of this image's session (toggling
  // back to "fit" doesn't downgrade -- see the zoom-trigger $effect below)
  // -- these three vars are plain module state, not $state, matching
  // dragState/paintingMaskId's own "imperative bookkeeping, not something
  // markup reads reactively" reasoning elsewhere in this file.
  /** @type {string | null} */
  let fullTierPath = null; // path the currently-uploaded full tier belongs to, or null
  /** @type {Promise<void> | null} */
  let fullTierPromise = null; // in-flight upgrade, deduped so rapid zoom toggling can't fire it twice
  let activeTier = "draft"; // "draft" | "full"
  // Normalized (0-1) focus point for zoom-to-100% scroll centering --
  // resolution-independent (unlike a native-pixel point), so the SAME
  // fraction re-centers correctly against the draft tier's dimensions at
  // the moment of the click AND against the full tier's dimensions once
  // upgradeToFullTier swaps them in moments later. Defaults to center for
  // the zoom-badge-button entry path, which has no click point at all.
  let lastZoomFocus = { x: 0.5, y: 0.5 };
  /** @type {{ startX: number, startY: number, startScrollLeft: number, startScrollTop: number } | null} */
  let dragState = null;
  const DRAG_CLICK_THRESHOLD = 4; // px -- below this, pointerup is a click (toggle zoom), not a completed drag

  // M3 Slice 5/6: while a mask tool is active, dragging on the canvas
  // places a new mask instead of panning -- tracks the in-progress drag
  // for the live preview (line for linear, ellipse for radial), not
  // committed to the edit stack until pointerup.
  let placingMask = $state(
    /** @type {
     *   | { kind: "linear_gradient", start: {x:number,y:number}, end: {x:number,y:number} }
     *   | { kind: "radial_gradient", center: {x:number,y:number}, radiusX: number, radiusY: number }
     *   | null
     * } */ (null),
  );
  /** @type {{ maskId: string, which: "start" | "end" | "center" | "radius", center?: {x:number,y:number} } | null} */
  let handleDragState = null;

  // M3 Slice 8: color range's creation pattern is a fourth-and-different
  // one from every other kind -- linear/radial drag two points, luminance
  // range creates on tool-button click with no canvas interaction at all,
  // brush paints continuous strokes. Color range is a SINGLE click on the
  // canvas that samples a pixel and immediately commits a new mask, tuned
  // afterward via Range/Feather sliders. Tracked as a lightweight
  // click-start point (not `placingMask`, which drives a live drag
  // preview this kind has no geometry for) so `handlePointerUp` can reuse
  // the SAME click-vs-drag threshold already established below for the
  // zoom-toggle click, rather than committing unconditionally on
  // pointerdown -- a real movement past the threshold is treated as a
  // cancel, not a commit.
  /** @type {{x:number,y:number} | null} */
  let colorRangeClickStart = null;

  // Eyedropper pickers (M3): Tone Curve point-insert, HSL band-identify,
  // Split Toning zone-tint -- all four share ONE click-to-sample gesture
  // (see +page.svelte's eyedropperTarget for how the commit is routed to
  // the right destination). Same click-vs-drag-threshold shape as
  // colorRangeClickStart above, deliberately not merged with it: this
  // component only reports the raw sampled color via onEyedropperSampled,
  // it never needs to know WHICH destination is waiting for it.
  /** @type {{x:number,y:number} | null} */
  let eyedropperClickStart = null;

  // M3 Slice 7: brush painting. Deliberately transient, per-stroke state --
  // no persistent "which mask am I painting into" tracking survives past
  // pointerup. Instead, EVERY pointerdown re-derives the paint target from
  // `selectedMaskId`: if it currently points at a brush mask, this stroke
  // APPENDS to it (real Lightroom's own multi-stroke-per-mask model,
  // matching PROGRESS.md's design note); otherwise this stroke creates a
  // fresh brush mask and selects it. A "New Brush" tool-strip button
  // achieves "start fresh" simply by deselecting (selectedMaskId = null)
  // -- no separate reset signal needs to reach this component at all.
  /** @type {string | null} */
  let paintingMaskId = null;
  /** @type {import('$lib/api/develop.js').Dab[]} */
  let strokeDabs = [];
  /** @type {{x: number, y: number} | null} */
  let lastBrushPoint = null;
  // Live brush-size cursor preview (SVG ellipse in the mask-overlay, drawn
  // as a true on-screen circle via the same width/height aspect correction
  // radiusFromDrag already uses for radial masks) -- shown on hover, not
  // just while actively painting, so size is visible before committing a
  // stroke.
  let brushCursor = $state(/** @type {{x:number,y:number} | null} */ (null));

  /** CSS-pixel click position -> native canvas-backing-store pixel
   * coordinate. Reused from the zoom-to-point math (M3 Slice 3) --
   * `getBoundingClientRect()` already reflects the current scroll offset,
   * so this needs no extra bookkeeping for panned/zoomed state. */
  function screenToNativePixel(/** @type {number} */ clientX, /** @type {number} */ clientY) {
    if (!canvasEl) return { x: 0, y: 0 };
    const rect = canvasEl.getBoundingClientRect();
    const scaleX = rect.width / canvasEl.width;
    const scaleY = rect.height / canvasEl.height;
    return { x: (clientX - rect.left) / scaleX, y: (clientY - rect.top) / scaleY };
  }

  /** Native pixel coordinate -> normalized (0..1) image-space coordinate,
   * matching the shader's own `in.uv`. */
  function screenToNormalized(/** @type {number} */ clientX, /** @type {number} */ clientY) {
    if (!canvasEl) return { x: 0, y: 0 };
    const p = screenToNativePixel(clientX, clientY);
    return { x: p.x / canvasEl.width, y: p.y / canvasEl.height };
  }

  // M3 Slice 8: retained sampleable pixel data, drawn once per image load
  // (see loadImage) into a persistent 2D OffscreenCanvas -- the decoded
  // ImageBitmap itself is discarded right after its one-time
  // copyExternalImageToTexture GPU upload (see loadImage), so nothing
  // else in this component keeps pixel data around for a CPU-side read
  // like an eyedropper needs. Same "own persistent per-image resource,
  // reset in loadImage" pattern brushTextureArray/brushRasterState
  // already use.
  /** @type {OffscreenCanvas | null} */
  let sourceSampleCanvas = null;
  /** @type {OffscreenCanvasRenderingContext2D | null} */
  let sourceSampleCtx = null;

  /** Samples the ORIGINAL DECODED SOURCE pixel at a normalized (0..1)
   * coordinate -- deliberately not the currently-graded preview the user
   * sees on screen. A true WYSIWYG eyedropper matching exactly what's
   * displayed would need a live GPU texture readback (copyTextureToBuffer
   * + mapAsync against the rendered output, plus COPY_SRC usage on the
   * canvas's own texture) -- genuinely separate, heavier plumbing than
   * this slice's real scope, deferred explicitly. Source-pixel sampling
   * is still a real, useful eyedropper for moderate edits, just not exact
   * for a heavily-graded image. Returns `{r,g,b}` as 0-1 floats, matching
   * WGSL's own texture-sample convention. */
  function sampleSourcePixel(/** @type {number} */ normX, /** @type {number} */ normY) {
    if (!sourceSampleCtx || !sourceSampleCanvas) return null;
    const px = Math.min(Math.max(Math.round(normX * sourceSampleCanvas.width), 0), sourceSampleCanvas.width - 1);
    const py = Math.min(Math.max(Math.round(normY * sourceSampleCanvas.height), 0), sourceSampleCanvas.height - 1);
    const d = sourceSampleCtx.getImageData(px, py, 1, 1).data;
    return { r: d[0] / 255, g: d[1] / 255, b: d[2] / 255 };
  }

  /** M3 Slice 6: radius from a center + the current pointer, using the
   * SAME native-pixel distance on both axes so a radial mask renders as a
   * true on-screen circle by default -- a deliberate deviation from real
   * Lightroom's actual free-form bounding-box ellipse drag (documented
   * here explicitly, not left to read as an oversight); the stored data
   * model (independent radiusX/radiusY) already supports a true
   * ellipse-drag "for free" if a future slice wants it. */
  function radiusFromDrag(/** @type {{x:number,y:number}} */ center, /** @type {number} */ clientX, /** @type {number} */ clientY) {
    if (!canvasEl) return { radiusX: 0, radiusY: 0 };
    const centerNative = { x: center.x * canvasEl.width, y: center.y * canvasEl.height };
    const pointerNative = screenToNativePixel(clientX, clientY);
    const radiusPx = Math.hypot(pointerNative.x - centerNative.x, pointerNative.y - centerNative.y);
    return { radiusX: radiusPx / canvasEl.width, radiusY: radiusPx / canvasEl.height };
  }

  /** M3 Slice 7: `brushSize` is a fraction of image WIDTH only (see
   * develop.js's `Dab` typedef), which rasterizes as a true circle
   * directly (rasterizeDab draws in the offscreen canvas's own native
   * pixel space, no aspect concern there) -- but the on-screen cursor
   * preview is an SVG ellipse sized via CSS percentages relative to the
   * overlay div's width/height separately, so it needs the SAME
   * width/height aspect correction `radiusFromDrag` already applies for
   * radial masks: a height-relative ry percentage larger than the
   * width-relative rx percentage by the canvas's width/height ratio,
   * exactly compensating so both resolve to the same on-screen pixel size. */
  function brushCursorRyPercent() {
    if (!canvasEl || !canvasEl.height) return brushSize * 100;
    return brushSize * (canvasEl.width / canvasEl.height) * 100;
  }

  /** Clips the INFINITE line through `p` in direction `dir` (need not be
   * unit length) against the [0,1]x[0,1] normalized-uv box, via
   * Liang-Barsky. Returns the clipped segment's two endpoints, or `null`
   * if the line never enters the box at all -- the correct semantic for a
   * feather boundary that's genuinely off the edge of the photo (there's
   * nothing to show), not "pick a length and hope it reaches." A fixed
   * half-length was tried first and found to be structurally wrong (not
   * just under-tuned): a boundary point outside the frame along the
   * gradient's OWN axis can never be brought back in by extending a
   * PERPENDICULAR line further, since the perpendicular offset can't
   * correct a coordinate the axis direction is orthogonal to. */
  function clipLineToUnitBox(/** @type {{x:number,y:number}} */ p, /** @type {{x:number,y:number}} */ dir) {
    let t0 = -Infinity;
    let t1 = Infinity;
    const edges = [
      { pk: -dir.x, qk: p.x }, // x >= 0
      { pk: dir.x, qk: 1 - p.x }, // x <= 1
      { pk: -dir.y, qk: p.y }, // y >= 0
      { pk: dir.y, qk: 1 - p.y }, // y <= 1
    ];
    for (const { pk, qk } of edges) {
      if (pk === 0) {
        if (qk < 0) return null; // parallel to this edge and entirely outside it
        continue;
      }
      const r = qk / pk;
      if (pk < 0) {
        t0 = Math.max(t0, r);
      } else {
        t1 = Math.min(t1, r);
      }
    }
    if (t0 > t1) return null;
    return {
      x1: p.x + t0 * dir.x,
      y1: p.y + t0 * dir.y,
      x2: p.x + t1 * dir.x,
      y2: p.y + t1 * dir.y,
    };
  }

  /** The two feather-boundary guide lines for a linear mask (perpendicular
   * to the gradient axis, at the weight=0 and weight=1 points) -- derived
   * directly from the SAME `t` formula the WGSL shader/develop_engine.rs
   * use (`weight = clamp((t+softness)/(1+2*softness), 0, 1)`): weight=0 at
   * t=-softness, weight=1 at t=1+softness. Returns `null` entirely when
   * feather is 0 (nothing additional to show -- the existing single axis
   * line already IS the boundary in that case), and each individual line
   * can independently be `null` if that boundary falls off-frame. */
  function linearFeatherLines(/** @type {any} */ mask) {
    if (!mask.feather || mask.feather <= 0) return null;
    const softness = Math.min(mask.feather / 100, 0.999);
    const dx = mask.end.x - mask.start.x;
    const dy = mask.end.y - mask.start.y;
    const len = Math.hypot(dx, dy) || 1;
    const perp = { x: -dy / len, y: dx / len };
    const p0 = { x: mask.start.x - softness * dx, y: mask.start.y - softness * dy };
    const p1 = { x: mask.end.x + softness * dx, y: mask.end.y + softness * dy };
    return { zero: clipLineToUnitBox(p0, perp), one: clipLineToUnitBox(p1, perp) };
  }

  /** The two feather-boundary ellipses for a radial mask -- derived from
   * the SAME ellipse-distance formula the shader/develop_engine.rs use:
   * insideWeight is 1 at d=(1-softness), 0 at d=(1+softness), so those are
   * exactly the ellipses at radiusX/radiusY scaled by (1-softness) and
   * (1+softness). `null` when feather is 0 -- the existing single ellipse
   * at the raw radius already IS the boundary in that case, and two
   * coincident stroked shapes would alpha-composite to a visibly heavier
   * line than one (a real regression, not just redundant markup). */
  function radialFeatherRadii(/** @type {any} */ mask) {
    if (!mask.feather || mask.feather <= 0) return null;
    const softness = Math.min(mask.feather / 100, 0.999);
    return {
      inner: { rx: mask.radiusX * (1 - softness), ry: mask.radiusY * (1 - softness) },
      outer: { rx: mask.radiusX * (1 + softness), ry: mask.radiusY * (1 + softness) },
    };
  }

  // M3 Slice 5/6: a hard branch on `activeTool`, not a case bolted onto the
  // pan/zoom logic -- while a mask tool is active, dragging NEVER pans or
  // toggles zoom, even in 100% mode, and vice versa.
  /** `setPointerCapture` wrapped defensively and called AFTER the state it
   * gates is already set -- see `handleMaskHandlePointerDown`'s comment for
   * why: a real failed drag there proved a throw from this call can
   * silently abort whatever runs after it. Capture is what keeps a drag
   * working if the pointer exits the canvas mid-drag, not a strict
   * requirement, so a failure to acquire it shouldn't block the drag. */
  function tryCapturePointer(/** @type {PointerEvent} */ e) {
    try {
      canvasEl?.setPointerCapture(e.pointerId);
    } catch {
      // Non-fatal, see above.
    }
  }

  /** M3 Slice 7: one dab, baking in the CURRENT brush tool settings (size/
   * hardness/flow) and erase-mode toggle at paint time -- these never
   * change retroactively for an already-placed dab, matching real
   * Lightroom's own brush-options model (Size/Feather/Flow apply to
   * whatever gets painted NEXT). */
  function makeDab(/** @type {{x:number,y:number}} */ p) {
    return {
      x: p.x,
      y: p.y,
      radius: brushSize,
      hardness: brushHardness,
      flow: brushFlow,
      mode: /** @type {"add" | "erase"} */ (eraseMode ? "erase" : "add"),
    };
  }

  /** Spaces interpolated dabs along the path from `from` to `to` at ~25%
   * of the brush radius apart -- without this, a fast drag would produce
   * a gappy/dotted stroke, since pointermove events don't fire densely
   * enough relative to brush size at speed. Returns [] (places nothing)
   * if the move was smaller than one spacing unit, so slow/jittery
   * movement doesn't flood the dab list with near-duplicate points --
   * `lastBrushPoint` is only advanced when dabs are actually placed (see
   * the pointermove handler), so distance keeps accumulating across
   * sub-threshold moves until it clears the bar. */
  function interpolatedDabs(/** @type {{x:number,y:number}} */ from, /** @type {{x:number,y:number}} */ to) {
    const dx = to.x - from.x;
    const dy = to.y - from.y;
    const dist = Math.hypot(dx, dy);
    const spacing = Math.max(brushSize * 0.25, 0.0008);
    if (dist < spacing) return [];
    // Capped defensively -- guards against a huge single jump (e.g. a
    // pointer teleport) flooding one update with thousands of dabs.
    const steps = Math.min(Math.floor(dist / spacing), 200);
    const dabs = [];
    for (let i = 1; i <= steps; i++) {
      const t = i / steps;
      dabs.push(makeDab({ x: from.x + dx * t, y: from.y + dy * t }));
    }
    return dabs;
  }

  function handlePointerDown(/** @type {PointerEvent} */ e) {
    if (activeTool === "linear_gradient") {
      e.preventDefault();
      const p = screenToNormalized(e.clientX, e.clientY);
      placingMask = { kind: "linear_gradient", start: p, end: p };
      tryCapturePointer(e);
      return;
    }
    if (activeTool === "radial_gradient") {
      e.preventDefault();
      const center = screenToNormalized(e.clientX, e.clientY);
      placingMask = { kind: "radial_gradient", center, radiusX: 0, radiusY: 0 };
      tryCapturePointer(e);
      return;
    }
    if (activeTool === "color_range") {
      e.preventDefault();
      colorRangeClickStart = { x: e.clientX, y: e.clientY };
      tryCapturePointer(e);
      return;
    }
    if (activeTool === "eyedropper") {
      e.preventDefault();
      eyedropperClickStart = { x: e.clientX, y: e.clientY };
      tryCapturePointer(e);
      return;
    }
    if (activeTool === "brush") {
      e.preventDefault();
      const p = screenToNormalized(e.clientX, e.clientY);
      // Re-derive the paint target fresh on every stroke, from the
      // CURRENT selection -- see the brush-state doc comment above for
      // why this is deliberately transient, not tracked persistently.
      const existing = masks.find((m) => m.id === selectedMaskId && m.op === "brush_mask");
      if (existing) {
        paintingMaskId = selectedMaskId;
        strokeDabs = [.../** @type {any} */ (existing).dabs];
      } else {
        const newId = crypto.randomUUID();
        paintingMaskId = newId;
        strokeDabs = [];
        onMaskCreated({ kind: "brush", id: newId });
      }
      strokeDabs.push(makeDab(p));
      lastBrushPoint = p;
      brushCursor = p;
      onMaskUpdated(/** @type {string} */ (paintingMaskId), { dabs: [...strokeDabs] });
      tryCapturePointer(e);
      return;
    }
    if (!wrapEl) return;
    e.preventDefault();
    dragState = {
      startX: e.clientX,
      startY: e.clientY,
      startScrollLeft: wrapEl.scrollLeft,
      startScrollTop: wrapEl.scrollTop,
    };
    tryCapturePointer(e);
  }

  function handlePointerMove(/** @type {PointerEvent} */ e) {
    if (placingMask?.kind === "linear_gradient") {
      placingMask = { ...placingMask, end: screenToNormalized(e.clientX, e.clientY) };
      return;
    }
    if (placingMask?.kind === "radial_gradient") {
      placingMask = { ...placingMask, ...radiusFromDrag(placingMask.center, e.clientX, e.clientY) };
      return;
    }
    if (activeTool === "brush") {
      const p = screenToNormalized(e.clientX, e.clientY);
      brushCursor = p; // shown on hover too, not just while painting
      if (paintingMaskId && lastBrushPoint) {
        const newDabs = interpolatedDabs(lastBrushPoint, p);
        if (newDabs.length > 0) {
          strokeDabs.push(...newDabs);
          lastBrushPoint = p;
          onMaskUpdated(paintingMaskId, { dabs: [...strokeDabs] });
        }
      }
      return;
    }
    if (!dragState || !wrapEl) return;
    wrapEl.scrollLeft = dragState.startScrollLeft - (e.clientX - dragState.startX);
    wrapEl.scrollTop = dragState.startScrollTop - (e.clientY - dragState.startY);
  }

  async function handlePointerUp(/** @type {PointerEvent} */ e) {
    try {
      canvasEl?.releasePointerCapture(e.pointerId);
    } catch {
      // Releasing a capture that was never successfully acquired would
      // itself throw -- non-fatal, see tryCapturePointer's comment.
    }
    if (placingMask?.kind === "linear_gradient") {
      const { start, end } = placingMask;
      placingMask = null;
      // Ignore a near-zero-size drag (an accidental click while the tool
      // was active) -- a real gradient needs two distinct points.
      if (Math.hypot(end.x - start.x, end.y - start.y) > 0.01) onMaskCreated({ kind: "linear_gradient", start, end });
      return;
    }
    if (placingMask?.kind === "radial_gradient") {
      const { center, radiusX, radiusY } = placingMask;
      placingMask = null;
      // Minimum-radius guard: radius is a divisor in both the WGSL shader
      // and develop_engine.rs's CPU path, so a near-zero placement
      // (accidental click) must be rejected, not committed -- it would
      // corrupt the frame with Inf/NaN.
      if (radiusX > 0.01 && radiusY > 0.01) onMaskCreated({ kind: "radial_gradient", center, radiusX, radiusY });
      return;
    }
    if (colorRangeClickStart) {
      const moved = Math.max(Math.abs(e.clientX - colorRangeClickStart.x), Math.abs(e.clientY - colorRangeClickStart.y));
      colorRangeClickStart = null;
      // A movement past the threshold is a cancel (no mask created), not a
      // pan -- the hard activeTool branch above already prevents panning
      // while this tool is active, so this is purely a "did the user mean
      // to click, or did their hand slip" check, same threshold/reasoning
      // as the zoom-toggle click below.
      if (moved < DRAG_CLICK_THRESHOLD) {
        const p = screenToNormalized(e.clientX, e.clientY);
        const color = sampleSourcePixel(p.x, p.y);
        if (color) {
          // Re-sampling an EXISTING mask's reference color (triggered from
          // MaskEditorPanel's eyedropper button, see +page.svelte's
          // colorRangeResampleId wiring) reuses this exact same click
          // gesture -- only the commit target differs: patch the existing
          // mask instead of creating a new one.
          if (colorRangeResampleId) {
            onColorRangeResampled(colorRangeResampleId, color);
          } else {
            onMaskCreated({ kind: "color_range", refColor: color });
          }
        }
      }
      return;
    }
    if (eyedropperClickStart) {
      const moved = Math.max(Math.abs(e.clientX - eyedropperClickStart.x), Math.abs(e.clientY - eyedropperClickStart.y));
      eyedropperClickStart = null;
      if (moved < DRAG_CLICK_THRESHOLD) {
        const p = screenToNormalized(e.clientX, e.clientY);
        const color = sampleSourcePixel(p.x, p.y);
        if (color) onEyedropperSampled(color);
      }
      return;
    }
    if (activeTool === "brush") {
      // Stroke ends, but deliberately does NOT clear selectedMaskId in the
      // parent -- a subsequent stroke (new pointerdown, tool still active)
      // re-derives paintingMaskId from selectedMaskId and continues
      // appending to the SAME mask, giving multi-stroke-per-mask painting
      // "for free" with no persistent state here.
      paintingMaskId = null;
      lastBrushPoint = null;
      return;
    }
    if (!dragState) return;
    const moved = Math.max(Math.abs(e.clientX - dragState.startX), Math.abs(e.clientY - dragState.startY));
    const clickPoint = { x: e.clientX, y: e.clientY };
    dragState = null;
    if (moved >= DRAG_CLICK_THRESHOLD) return; // a completed drag, not a click -- leave scroll as-is

    if (zoomMode === "100") {
      zoomMode = "fit";
      return;
    }
    if (!canvasEl) return;
    const rect = canvasEl.getBoundingClientRect();
    const scaleX = rect.width / canvasEl.width;
    const scaleY = rect.height / canvasEl.height;
    const nativeX = (clickPoint.x - rect.left) / scaleX;
    const nativeY = (clickPoint.y - rect.top) / scaleY;
    // Stored normalized (0-1), not as a native-pixel point -- the point
    // itself doesn't change resolution, but the canvas's own backing-store
    // size DOES once upgradeToFullTier swaps in the 1:1 tier moments
    // later. A native-pixel value captured here would silently go stale
    // and mis-center once that resize happens; the normalized fraction
    // re-applies correctly against whichever tier's dimensions are
    // current when it's read.
    lastZoomFocus = { x: nativeX / canvasEl.width, y: nativeY / canvasEl.height };

    zoomMode = "100";
    await tick(); // required: $state-triggered DOM patches (the new canvas size) land on a microtask
    if (!wrapEl) return;
    wrapEl.scrollLeft = nativeX - wrapEl.clientWidth / 2;
    wrapEl.scrollTop = nativeY - wrapEl.clientHeight / 2;
  }

  /** Dragging an existing mask's handle -- separate from the canvas's own
   * pointer handlers above (these fire on the handle button itself, which
   * sits visually on top, so the canvas never sees them). `start`/`end`
   * (linear) and `center` (radial) are direct point patches; `radius`
   * (radial) is a resize, recomputed the same "equal native-pixel radius
   * on both axes" way as placement -- needs the mask's OWN center
   * (captured at drag-start, since it doesn't change during a radius
   * drag) to compute the new radius from. */
  function handleMaskHandlePointerDown(
    /** @type {PointerEvent} */ e,
    /** @type {string} */ maskId,
    /** @type {"start" | "end" | "center" | "radius"} */ which,
    /** @type {{x:number,y:number}=} */ center,
  ) {
    e.stopPropagation();
    e.preventDefault();
    // Set the drag state FIRST, `setPointerCapture` second, wrapped
    // defensively: empirically confirmed via a real failed drag that
    // `setPointerCapture` can throw here (button element, unlike the
    // canvas's own capture calls elsewhere in this file, which have never
    // been observed to throw) -- with the old order (capture first), a
    // throw silently aborted the rest of this function, leaving
    // `handleDragState` unset and the whole drag a no-op with no error
    // surfaced anywhere. Capture is what keeps the drag working if the
    // pointer exits the button's small hit area mid-drag -- a nice-to-have,
    // not a strict requirement, so a failure to acquire it shouldn't break
    // the drag itself.
    handleDragState = { maskId, which, center };
    onMaskSelected(maskId);
    try {
      /** @type {HTMLElement} */ (e.currentTarget).setPointerCapture(e.pointerId);
    } catch {
      // See above -- non-fatal.
    }
  }

  function handleMaskHandlePointerMove(/** @type {PointerEvent} */ e) {
    if (!handleDragState) return;
    const { maskId, which, center } = handleDragState;
    if (which === "radius" && center) {
      onMaskUpdated(maskId, radiusFromDrag(center, e.clientX, e.clientY));
      return;
    }
    onMaskUpdated(maskId, { [which]: screenToNormalized(e.clientX, e.clientY) });
  }

  function handleMaskHandlePointerUp(/** @type {PointerEvent} */ e) {
    try {
      /** @type {HTMLElement} */ (e.currentTarget).releasePointerCapture(e.pointerId);
    } catch {
      // Releasing a capture that was never successfully acquired (see
      // handleMaskHandlePointerDown) would itself throw -- non-fatal.
    }
    handleDragState = null;
  }

  // WebGPU handles -- plain vars, not $state: these drive imperative canvas
  // rendering, not Svelte's own reactivity (RFC-0001 §4 "decode once, edit
  // reactively": the texture is uploaded once per image, every subsequent
  // adjustment just rewrites a uniform buffer and re-runs the shader, no
  // re-fetch and no Svelte re-render of the DOM).
  /** @type {GPUDevice | null} */
  let device = null;
  /** @type {GPUCanvasContext | null} */
  let context = null;
  /** @type {GPURenderPipeline | null} */
  let pipeline = null;
  /** @type {GPUTexture | null} */
  let sourceTexture = null;
  /** @type {GPUBuffer | null} */
  let uniformBuffer = null;
  /** @type {GPUBuffer | null} */
  let masksBuffer = null;
  // Tone Curve (M3): device-scoped like uniformBuffer/masksBuffer above
  // (created once in initGpu, rewritten via writeBuffer whenever the curve
  // changes) -- NOT recreated per image/tier-swap the way sourceTexture/
  // brushTextureArray are, since a curve's shape has nothing to do with
  // which image is loaded.
  /** @type {GPUBuffer | null} */
  let curveLutBuffer = null;
  // HSL / Color Mixer (M3): same device-scoped treatment as curveLutBuffer
  // above -- created once, rewritten via writeBuffer on every render, not
  // tied to which image is loaded.
  /** @type {GPUBuffer | null} */
  let hslBandsBuffer = null;
  // Split Toning (M3): same device-scoped treatment as curveLutBuffer/
  // hslBandsBuffer above.
  /** @type {GPUBuffer | null} */
  let splitToningBuffer = null;
  // Vignette (M3): same device-scoped treatment -- 3 fields don't fit in
  // Adjustments' own spare padding (already claimed by Dehaze/Texture/
  // Clarity), so it gets its own small dedicated buffer, same as Split
  // Toning did for the same reason.
  /** @type {GPUBuffer | null} */
  let vignetteBuffer = null;
  // Grain (M3): same device-scoped, own-small-buffer treatment as
  // Vignette above, for the same reason (3 fields, no spare Adjustments
  // padding left).
  /** @type {GPUBuffer | null} */
  let grainBuffer = null;
  // Sharpening / Noise Reduction (M3): same device-scoped, own-small-
  // buffer treatment as Vignette/Grain above, one buffer per structured
  // op.
  /** @type {GPUBuffer | null} */
  let sharpenBuffer = null;
  /** @type {GPUBuffer | null} */
  let lumaNRBuffer = null;
  /** @type {GPUBuffer | null} */
  let colorNRBuffer = null;
  /** @type {GPUBindGroup | null} */
  let bindGroup = null;
  /** @type {GPUTextureFormat} */
  let presentationFormat = "bgra8unorm";

  // Dehaze (M3): the first op in this pipeline needing a real multi-pass
  // render graph (dark-channel-prior haze removal genuinely needs
  // neighboring-pixel/whole-image data, unlike every earlier op's single
  // straight-through fs_main) -- see the WGSL source's own doc comments on
  // fs_grade/fs_atm_reduce/fs_min_channel/fs_min_h/fs_min_v/fs_mean_h/
  // fs_mean_v/fs_final for the algorithm. `pipeline`/`bindGroup` above
  // are REPURPOSED as the final pass's own pipeline/bind group (entryPoint
  // "fs_final" now, not "fs_main") -- their bind group layout is
  // genuinely DIFFERENT from before, not a superset: fs_final no longer
  // references srcTexture(1)/curveLut(5)/hslBands(6)/splitToning(7) (those
  // moved into fs_grade below), but DOES still need srcSampler(0) -- the
  // mask loop's own pre-existing brushMasks sample uses it, unrelated to
  // Dehaze. layout:"auto" infers {0,2,3,4,8,10,12} for it -- see
  // applyBitmapToGpu's rebuilt bindGroup entries.
  /** @type {GPURenderPipeline | null} */
  let gradePipeline = null;
  /** @type {GPURenderPipeline | null} */
  let atmReducePipeline = null;
  /** @type {GPURenderPipeline | null} */
  let minChannelPipeline = null;
  /** @type {GPURenderPipeline | null} */
  let minHPipeline = null;
  /** @type {GPURenderPipeline | null} */
  let minVPipeline = null;
  /** @type {GPURenderPipeline | null} */
  let meanHPipeline = null;
  /** @type {GPURenderPipeline | null} */
  let meanVPipeline = null;
  /** @type {GPURenderPipeline | null} */
  let textureHPipeline = null;
  /** @type {GPURenderPipeline | null} */
  let textureVPipeline = null;
  /** @type {GPURenderPipeline | null} */
  let clarityHPipeline = null;
  /** @type {GPURenderPipeline | null} */
  let clarityVPipeline = null;
  /** @type {GPURenderPipeline | null} */
  let sharpenHPipeline = null;
  /** @type {GPURenderPipeline | null} */
  let sharpenVPipeline = null;
  /** @type {GPURenderPipeline | null} */
  let lumaNRHPipeline = null;
  /** @type {GPURenderPipeline | null} */
  let lumaNRVPipeline = null;
  /** @type {GPURenderPipeline | null} */
  let colorNRHPipeline = null;
  /** @type {GPURenderPipeline | null} */
  let colorNRVPipeline = null;

  // Intermediate textures -- all sized to match the CURRENT source
  // texture's own resolution (recreated in applyBitmapToGpu whenever that
  // changes, same lifecycle as sourceTexture/brushTextureArray), except
  // the atmospheric-light reduction chain, which is a SEQUENCE of
  // successively-smaller textures (8x8 block reduction per pass) computed
  // from the source resolution -- see buildAtmLightChainSizes.
  /** @type {GPUTexture | null} */
  let gradedTex = null;
  /** @type {GPUTexture | null} */
  let minChannelTex = null;
  /** @type {GPUTexture | null} */
  let darkChannelHTex = null;
  /** @type {GPUTexture | null} */
  let tRawTex = null;
  /** @type {GPUTexture | null} */
  let transmissionHTex = null;
  /** @type {GPUTexture | null} */
  let transmissionTex = null;
  /** @type {GPUTexture[]} */
  let atmLightChain = [];
  // Texture & Clarity (M3): local-contrast passes that run BEFORE Dehaze's
  // own maps, writing their final result back into gradedTex itself (see
  // fs_clarity_v's own doc comment) -- these three are the only NEW
  // textures needed. textureBlurScratchTex/clarityBlurScratchTex are each
  // dedicated to one op (not shared) even though nothing stops them from
  // being reused sequentially -- matches every other Dehaze filter stage's
  // own one-texture-per-stage convention, so a future pass reordering
  // can't silently corrupt output with no validation error to catch it.
  /** @type {GPUTexture | null} */
  let textureBlurScratchTex = null;
  /** @type {GPUTexture | null} */
  let textureAdjustedTex = null;
  /** @type {GPUTexture | null} */
  let clarityBlurScratchTex = null;
  // Sharpening / Noise Reduction (M3): same one-texture-per-stage
  // convention as Texture/Clarity above -- an H-output scratch texture
  // and a final (post-V-pass) result texture per op, all read directly
  // by fs_final (none of these overwrite gradedTex the way Clarity's own
  // V-pass does -- see fs_final's own doc comment for why these stay as
  // separate delta-source textures instead).
  /** @type {GPUTexture | null} */
  let sharpenBlurHTex = null;
  /** @type {GPUTexture | null} */
  let sharpenBlurTex = null;
  /** @type {GPUTexture | null} */
  let lumaNRBlurHTex = null;
  /** @type {GPUTexture | null} */
  let lumaNRBlurTex = null;
  /** @type {GPUTexture | null} */
  let colorNRBlurHTex = null;
  /** @type {GPUTexture | null} */
  let colorNRBlurTex = null;

  /** @type {GPUBindGroup | null} */
  let gradeBindGroup = null;
  /** @type {GPUBindGroup | null} */
  let minChannelBindGroup = null;
  /** @type {GPUBindGroup | null} */
  let minHBindGroup = null;
  /** @type {GPUBindGroup | null} */
  let minVBindGroup = null;
  /** @type {GPUBindGroup | null} */
  let meanHBindGroup = null;
  /** @type {GPUBindGroup | null} */
  let meanVBindGroup = null;
  /** @type {GPUBindGroup | null} */
  let textureHBindGroup = null;
  /** @type {GPUBindGroup | null} */
  let textureVBindGroup = null;
  /** @type {GPUBindGroup | null} */
  let clarityHBindGroup = null;
  /** @type {GPUBindGroup | null} */
  let clarityVBindGroup = null;
  /** @type {GPUBindGroup | null} */
  let sharpenHBindGroup = null;
  /** @type {GPUBindGroup | null} */
  let sharpenVBindGroup = null;
  /** @type {GPUBindGroup | null} */
  let lumaNRHBindGroup = null;
  /** @type {GPUBindGroup | null} */
  let lumaNRVBindGroup = null;
  /** @type {GPUBindGroup | null} */
  let colorNRHBindGroup = null;
  /** @type {GPUBindGroup | null} */
  let colorNRVBindGroup = null;
  /** @type {GPUBindGroup[]} */
  let atmReduceBindGroups = [];

  // Dirty-key caching: the dark-channel/atmospheric-light/transmission
  // passes above, PLUS Texture/Clarity's own local-contrast passes (which
  // write their result INTO gradedTex, unlike dehaze_amount -- see
  // writeAdjustmentsAndRender's own comment on why texture/clarity amounts
  // belong in this key but dehaze_amount doesn't), depend on {exposure,
  // contrast, saturation, toneCurvePoints, hslBands, splitToning, texture,
  // clarity} -- NOT on masks/selectedMaskId/showMaskOverlay, which only
  // affect the cheap final pass. A VALUE-based key (not reference
  // equality) is required: masks/toneCurvePoints/hslBands/splitToning are
  // all rebuilt via $derived from editStack in +page.svelte on EVERY
  // edit-stack change regardless of which op changed, so a reference check
  // would always report "changed" and silently defeat this cache. `null`
  // (not computed yet) is always treated as dirty, which is what makes the
  // very first render safe -- gradedTex/darkChannelHTex/etc are guaranteed
  // to hold real values (not uninitialized garbage) before fs_final ever
  // reads them. Named for the whole shared block it gates, not just
  // Dehaze -- the block grew two more ops without this rename, `dehaze` in
  // the name would have been a trap for the next person wiring one in.
  /** @type {string | null} */
  let spatialOpsInputsKey = null;

  // M3 Slice 7: brush masks rasterize into a shared texture ARRAY (one
  // layer per active brush mask, sized to the same combined MAX_MASKS
  // budget every mask kind shares) rather than a single texture -- a
  // single shared texture would silently break true op-order interleaving
  // and independent per-mask adjustments the moment there's more than one
  // brush mask, or a brush mask sits between two gradients in the stack.
  // Recreated per-image (see loadImage) since it must be sized to that
  // image's native resolution.
  /** @type {GPUTexture | null} */
  let brushTextureArray = null;
  /** Per-mask persistent rasterization state, keyed by mask id. Each
   * OffscreenCanvas is NEVER cleared once created -- only newly-added dabs
   * are drawn onto it (see syncBrushRasterization) -- so a long stroke's
   * per-move cost stays bound by texture resolution/upload cost, not by
   * re-rendering the whole dab list from scratch every time. Reset
   * entirely on every image change (loadImage), since a canvas sized for
   * one image's resolution is meaningless for another.
   * @type {Map<string, { canvas: OffscreenCanvas, ctx: OffscreenCanvasRenderingContext2D, layer: number, dabsDrawn: number }>} */
  let brushRasterState = new Map();
  /** @type {number[]} */
  let freeBrushLayers = [];

  // Same three global adjustments as ADR-0004/RFC-0001's Slice 3 scope,
  // plus (M3 Slice 5) a bounded array of linear-gradient local-adjustment
  // masks, applied in WGSL entirely inside the webview process -- no IPC
  // round trip per edit. This formula must be kept in hand-sync with
  // `develop_engine.rs`'s `apply_edit_stack` (app/src-tauri/src/
  // develop_engine.rs) -- the CPU-side implementation used for
  // full-resolution export and thumbnail regeneration. They can't be
  // unified into one executable implementation without native wgpu
  // (deliberately deferred to M5, see ADR-0004's dated update); until
  // then, `develop_engine.rs`'s own test table is the parity reference to
  // check this shader's math against whenever either side changes.
  const WGSL = `
    struct VertexOut {
      @builtin(position) position: vec4<f32>,
      @location(0) uv: vec2<f32>,
    };

    @vertex
    fn vs_main(@builtin(vertex_index) i: u32) -> VertexOut {
      var pos = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 3.0, -1.0),
        vec2<f32>(-1.0,  3.0),
      );
      var out: VertexOut;
      out.position = vec4<f32>(pos[i], 0.0, 1.0);
      out.uv = vec2<f32>((pos[i].x + 1.0) * 0.5, (1.0 - pos[i].y) * 0.5);
      return out;
    }

    // Padded to a full 2x vec4 (32 bytes) deliberately -- mirrors the Mask
    // struct's own "pack into vec4 multiples" discipline below, rather than
    // leaving this at an arbitrary size once it grows past 4 scalars.
    struct Adjustments {
      exposure_ev: f32,
      contrast: f32,
      saturation: f32,
      mask_count: f32,
      // Mask-overlay: -1 = no overlay, else the LOOP INDEX (not a texture
      // layer) of the currently selected mask, when overlay is enabled.
      // Unified across every no-geometry mask kind (brush, luminance
      // range) -- replaces this field's earlier brush-only overlay_layer/
      // overlay_invert pair (mask UI polish slice): the overlay now
      // reuses each mask's own already-computed, already-inverted weight
      // from the main loop below, rather than a separate
      // re-sample-and-re-invert step, so no per-kind overlay data is
      // needed here at all.
      selected_mask_index: f32,
      // Dehaze (M3): 0-100, reuses this struct's own last spare padding
      // float rather than a whole new tiny uniform buffer/binding for one
      // scalar -- unlike Split Toning (5 new fields, genuinely didn't fit
      // in 3 spare floats), Dehaze's single Amount slider does.
      dehaze_amount: f32,
      // Texture & Clarity (M3): -100..100 each, claiming this struct's own
      // last two spare padding floats the same way dehaze_amount claimed
      // its own -- see fs_texture_v/fs_clarity_v for how these are
      // consumed.
      texture_amount: f32,
      clarity_amount: f32,
    };

    // Packed entirely into vec4-multiples (48 bytes/mask) to sidestep
    // WGSL's vec2/vec3-in-array uniform alignment footguns -- array stride
    // in the uniform address space must be a multiple of 16 bytes, and an
    // all-vec4 struct is trivially aligned with no implicit padding.
    // params.w holds the brush mask's own texture-array layer index (as a
    // float, cast to i32 at sample time) -- brush is the only kind that
    // needs a fourth scalar; every other kind leaves it at 0.
    struct Mask {
      start_end: vec4<f32>,   // xy = start, zw = end (normalized image space); luminance range: x=rangeMin, y=rangeMax (both 0-100); color range: xyz=refColor (0-1), w=range (0-100)
      params: vec4<f32>,      // x = feather 0-100 (unused for brush), y = invert 0/1, z = kind (0=linear, 1=radial, 2=brush, 3=luminance range, 4=color range), w = brush texture-array layer
      adjustments: vec4<f32>, // x = exposure_ev, y = contrast, z = saturation, w unused
    };
    const MAX_MASKS = 8;

    @group(0) @binding(0) var srcSampler: sampler;
    @group(0) @binding(1) var srcTexture: texture_2d<f32>;
    @group(0) @binding(2) var<uniform> adj: Adjustments;
    @group(0) @binding(3) var<uniform> masks: array<Mask, MAX_MASKS>;
    // Brush masks rasterize CPU-side (OffscreenCanvas, luminance-as-weight)
    // rather than computing an analytic formula here -- one array layer per
    // active brush mask. Sampled via textureSampleLevel (not textureSample)
    // deliberately: this call sits inside a per-mask branch on m.params.z,
    // and textureSampleLevel has no implicit-derivative uniformity
    // restriction to worry about, unlike textureSample.
    @group(0) @binding(4) var brushMasks: texture_2d_array<f32>;
    // Tone curve LUT (M3): 256 f32 samples packed into 64 vec4s, NOT
    // array<f32,256> -- WGSL's uniform-address-space array stride must be
    // a multiple of 16 bytes (the Mask struct's own comment above
    // documents hitting this exact footgun), and this packing lets a
    // plain contiguous Float32Array(256) upload directly with no manual
    // padding on the JS side. Device-scoped, not per-image -- see
    // curveLutBuffer's own declaration in the script.
    @group(0) @binding(5) var<uniform> curveLut: array<vec4<f32>, 64>;
    // HSL / Color Mixer (M3): one vec4 per band -- x=hue shift (degrees,
    // -100..100), y=saturation delta (percent, -100..100), z=luminance
    // delta (percent, -100..100), w=unused padding, trivially vec4-
    // aligned per the same footgun documented above. Band order is fixed
    // and MUST match develop.js's HSL_BAND_NAMES/HSL_BAND_CENTERS_DEG and
    // develop_engine.rs's HSL_BAND_NAMES -- no band-name string is ever
    // uploaded, only positional order. Device-scoped, not per-image.
    @group(0) @binding(6) var<uniform> hslBands: array<vec4<f32>, 8>;
    // Split Toning (M3): a small, fixed, NAMED bag of scalars -- mirrors
    // Adjustments' own struct style rather than HSL's/curveLut's vec4-
    // array shape, since there's no natural per-element repetition in 5
    // named fields the way there is for 8 bands or 256 samples. Padded to
    // a full vec4 multiple (32 bytes) per the same footgun documented
    // above (hit three times now: Mask, curveLut, hslBands).
    struct SplitToning {
      shadow_hue: f32,           // 0..360, absolute hue-wheel position
      shadow_saturation: f32,    // 0..100
      highlight_hue: f32,        // 0..360
      highlight_saturation: f32, // 0..100
      balance: f32,              // -100..100
      _pad0: f32,
      _pad1: f32,
      _pad2: f32,
    };
    @group(0) @binding(7) var<uniform> splitToning: SplitToning;

    // Vignette (M3): a flat 3-field struct, same reasoning SplitToning's
    // own comment gives for not using curveLut/hslBands' vec4-array shape.
    // Padded to a full vec4 (16 bytes) per the same footgun documented
    // above.
    struct Vignette {
      amount: f32,   // -100..100, negative darkens, positive lightens
      midpoint: f32, // 0..100, normalized radius where falloff begins
      feather: f32,  // 0..100, width of the falloff transition
      _pad0: f32,
    };
    @group(0) @binding(15) var<uniform> vignette: Vignette;

    // Grain (M3): same flat-struct, own-buffer treatment as Vignette,
    // same reasoning.
    struct Grain {
      amount: f32,    // 0..100
      size: f32,      // 0..100, maps to lattice cell width in pixels
      roughness: f32, // 0..100, blends smooth<->blocky noise
      _pad0: f32,
    };
    @group(0) @binding(16) var<uniform> grain: Grain;

    // A direct WGSL port of develop_engine.rs's own grain_hash/
    // grain_value_noise/grain_delta -- see that module's doc comment on
    // grain_delta for the full parameter reasoning (size's pixel-cell
    // mapping, roughness's smooth/blocky blend, amount's additive-delta
    // shape). Not bit-exact with the Rust twin (different underlying sin
    // implementations), same "not byte-identical" parity bar as the rest
    // of this shader.
    const GRAIN_MAX_CELL_PX: f32 = 6.0;
    const GRAIN_STRENGTH: f32 = 0.12;

    fn grainHash(p: vec2<f32>) -> f32 {
      let v = sin(dot(p, vec2<f32>(12.9898, 78.233))) * 43758.5453123;
      return fract(v);
    }

    fn grainValueNoise(coord: vec2<f32>) -> f32 {
      let i = floor(coord);
      let f = fract(coord);
      let a = grainHash(i);
      let b = grainHash(i + vec2<f32>(1.0, 0.0));
      let c = grainHash(i + vec2<f32>(0.0, 1.0));
      let d = grainHash(i + vec2<f32>(1.0, 1.0));
      let u = f * f * (3.0 - 2.0 * f);
      return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
    }

    fn grainDelta(coord: vec2<f32>) -> f32 {
      if (grain.amount == 0.0) {
        return 0.0;
      }
      let cellPx = 1.0 + (grain.size / 100.0) * (GRAIN_MAX_CELL_PX - 1.0);
      let scaled = coord / cellPx;
      let smoothN = grainValueNoise(scaled);
      let roughN = grainHash(floor(scaled));
      let noise = mix(smoothN, roughN, clamp(grain.roughness / 100.0, 0.0, 1.0));
      return (noise * 2.0 - 1.0) * (grain.amount / 100.0) * GRAIN_STRENGTH;
    }

    // Sharpening / Noise Reduction (M3): a direct WGSL port of
    // develop_engine.rs's own sharpen_delta/luma_nr_delta/color_nr_delta
    // -- see that module's doc comments (Sharpen, LumaNr, ColorNr structs
    // and their delta functions) for the full parameter reasoning
    // (Detail vs Masking's genuinely different amplitude-vs-spatial
    // gates, Contrast's post-smoothing restoration, Color NR's exact
    // luma-preserving chroma-delta construction -- algebraically verified
    // in this slice's own design review). All three blurs read gradedTex
    // DIRECTLY (never rebound, unlike Texture/Clarity's lcRgbInput) --
    // they always read the SAME pre-Dehaze-recovery snapshot, see the
    // Rust twin's own doc comment on that named, accepted limitation.
    struct SharpenParams {
      amount: f32,   // 0..100
      radius: f32,   // 0..100 slider, mapped to a pixel radius below
      detail: f32,   // 0..100
      masking: f32,  // 0..100
    };
    struct LumaNrParams {
      amount: f32,    // 0..100
      detail: f32,    // 0..100
      contrast: f32,  // 0..100
      _pad0: f32,
    };
    struct ColorNrParams {
      amount: f32,  // 0..100
      detail: f32,  // 0..100
      _pad0: f32,
      _pad1: f32,
    };
    @group(0) @binding(19) var<uniform> sharpenParams: SharpenParams;
    @group(0) @binding(20) var<uniform> lumaNrParams: LumaNrParams;
    @group(0) @binding(21) var<uniform> colorNrParams: ColorNrParams;
    // Rebound per V-pass, same "generic scratch, many bind groups" pattern
    // filterInput(11)/lcBlurInput(14) already established -- binding 17 is
    // reused across Sharpen's and Luma NR's own H-output (both r32float,
    // single-channel); binding 18 is Color NR's own H-output (rgba16float,
    // a full-channel blur, needs its own dedicated slot).
    @group(0) @binding(17) var blurScratchR32: texture_2d<f32>;
    @group(0) @binding(18) var blurScratchRgba: texture_2d<f32>;
    // Final (post-V-pass) blur results -- read only by fs_final, each its
    // own dedicated binding (not reused/rebound, since fs_final needs all
    // three simultaneously in one draw call, unlike the scratch slots
    // above which are only ever bound one-at-a-time per H/V pass).
    @group(0) @binding(22) var sharpenBlurFinal: texture_2d<f32>;
    @group(0) @binding(23) var lumaNrBlurFinal: texture_2d<f32>;
    @group(0) @binding(24) var colorNrBlurFinal: texture_2d<f32>;

    const SHARPEN_MAX_RADIUS_PX: i32 = 8;
    const SHARPEN_STRENGTH: f32 = 1.6;
    const SHARPEN_DETAIL_SCALE: f32 = 0.06;
    const SHARPEN_MASK_SCALE: f32 = 0.05;
    const LUMA_NR_RADIUS: i32 = 3;
    const NR_DETAIL_SCALE: f32 = 0.05;
    const NR_CONTRAST_STRENGTH: f32 = 0.6;
    const COLOR_NR_RADIUS: i32 = 4;
    const COLOR_NR_DETAIL_SCALE: f32 = 0.08;

    // Sharpening's Radius is a genuine USER slider, not a compile-time
    // const the way every other radius in this shader is (Texture/
    // Clarity/Dehaze's own radii are all baked into the WGSL source at
    // authoring time) -- this is a deliberate, new precedent: a real
    // uniform-driven dynamic loop bound in fs_sharpen_h/fs_sharpen_v
    // below, not a template to copy for future fixed-radius ops.
    fn sharpenRadiusPx(radiusSlider: f32) -> i32 {
      let r = 1.0 + (radiusSlider / 100.0) * (f32(SHARPEN_MAX_RADIUS_PX) - 1.0);
      return max(i32(round(r)), 1);
    }

    @fragment
    fn fs_sharpen_h(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let dims = vec2<i32>(textureDimensions(gradedTex));
      let radius = sharpenRadiusPx(sharpenParams.radius);
      var sum = 0.0;
      for (var dx = -radius; dx <= radius; dx = dx + 1) {
        let sx = clamp(coord.x + dx, 0, dims.x - 1);
        sum = sum + luma(textureLoad(gradedTex, vec2<i32>(sx, coord.y), 0).rgb);
      }
      let window = f32(2 * radius + 1);
      return vec4<f32>(sum / window, 0.0, 0.0, 1.0);
    }

    @fragment
    fn fs_sharpen_v(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let dims = vec2<i32>(textureDimensions(blurScratchR32));
      let radius = sharpenRadiusPx(sharpenParams.radius);
      var sum = 0.0;
      for (var dy = -radius; dy <= radius; dy = dy + 1) {
        let sy = clamp(coord.y + dy, 0, dims.y - 1);
        sum = sum + textureLoad(blurScratchR32, vec2<i32>(coord.x, sy), 0).r;
      }
      let window = f32(2 * radius + 1);
      return vec4<f32>(sum / window, 0.0, 0.0, 1.0);
    }

    @fragment
    fn fs_lumaNR_h(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let dims = vec2<i32>(textureDimensions(gradedTex));
      var sum = 0.0;
      for (var dx = -LUMA_NR_RADIUS; dx <= LUMA_NR_RADIUS; dx = dx + 1) {
        let sx = clamp(coord.x + dx, 0, dims.x - 1);
        sum = sum + luma(textureLoad(gradedTex, vec2<i32>(sx, coord.y), 0).rgb);
      }
      let window = f32(2 * LUMA_NR_RADIUS + 1);
      return vec4<f32>(sum / window, 0.0, 0.0, 1.0);
    }

    @fragment
    fn fs_lumaNR_v(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let dims = vec2<i32>(textureDimensions(blurScratchR32));
      var sum = 0.0;
      for (var dy = -LUMA_NR_RADIUS; dy <= LUMA_NR_RADIUS; dy = dy + 1) {
        let sy = clamp(coord.y + dy, 0, dims.y - 1);
        sum = sum + textureLoad(blurScratchR32, vec2<i32>(coord.x, sy), 0).r;
      }
      let window = f32(2 * LUMA_NR_RADIUS + 1);
      return vec4<f32>(sum / window, 0.0, 0.0, 1.0);
    }

    @fragment
    fn fs_colorNR_h(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let dims = vec2<i32>(textureDimensions(gradedTex));
      var sum = vec3<f32>(0.0, 0.0, 0.0);
      for (var dx = -COLOR_NR_RADIUS; dx <= COLOR_NR_RADIUS; dx = dx + 1) {
        let sx = clamp(coord.x + dx, 0, dims.x - 1);
        sum = sum + textureLoad(gradedTex, vec2<i32>(sx, coord.y), 0).rgb;
      }
      let window = f32(2 * COLOR_NR_RADIUS + 1);
      return vec4<f32>(sum / window, 1.0);
    }

    @fragment
    fn fs_colorNR_v(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let dims = vec2<i32>(textureDimensions(blurScratchRgba));
      var sum = vec3<f32>(0.0, 0.0, 0.0);
      for (var dy = -COLOR_NR_RADIUS; dy <= COLOR_NR_RADIUS; dy = dy + 1) {
        let sy = clamp(coord.y + dy, 0, dims.y - 1);
        sum = sum + textureLoad(blurScratchRgba, vec2<i32>(coord.x, sy), 0).rgb;
      }
      let window = f32(2 * COLOR_NR_RADIUS + 1);
      return vec4<f32>(sum / window, 1.0);
    }

    fn apply_adjustments(rgb: vec3<f32>, exposure_ev: f32, contrast: f32, saturation: f32) -> vec3<f32> {
      var c = rgb * pow(2.0, exposure_ev);
      c = (c - 0.5) * (1.0 + contrast / 100.0) + 0.5;
      let luma = dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
      c = luma + (c - luma) * (1.0 + saturation / 100.0);
      return c;
    }

    // Same LUT-plus-linear-interpolation formula as develop.js's
    // sampleCurveLut and develop_engine.rs's sample_lut -- all three sides
    // must agree exactly, since the LUT itself (not an independent exact
    // spline evaluation) is what parity is built on here.
    fn sampleCurveLut(v: f32) -> f32 {
      let idx = clamp(v, 0.0, 1.0) * 255.0;
      let i0 = i32(floor(idx));
      let i1 = min(i0 + 1, 255);
      let frac = idx - f32(i0);
      let v0 = curveLut[i0 / 4][i0 % 4];
      let v1 = curveLut[i1 / 4][i1 % 4];
      return mix(v0, v1, frac);
    }

    const HSL_BAND_CENTERS = array<f32, 8>(0.0, 45.0, 90.0, 135.0, 180.0, 225.0, 270.0, 315.0);

    // WGSL's modulo operator is truncated remainder (sign follows the
    // dividend), NOT the always-non-negative floor-mod this HSL math needs
    // for correct hue wraparound at 0/360 -- this is the standard
    // floor-mod construction (a minus b times floor(a/b)), matching
    // Rust's f32::rem_euclid exactly, which develop_engine.rs's twin of
    // this code uses throughout.
    fn rem_euclid(a: f32, b: f32) -> f32 {
      return a - b * floor(a / b);
    }

    // Standard HSL, not this shader's own perceptual luma (used elsewhere
    // for global Saturation) -- see develop_engine.rs's rgb_to_hsl doc
    // comment for why the two must not be conflated. Returns
    // (hue_degrees 0..360, saturation 0..1, lightness 0..1).
    fn rgbToHsl(rgb: vec3<f32>) -> vec3<f32> {
      let mx = max(rgb.r, max(rgb.g, rgb.b));
      let mn = min(rgb.r, min(rgb.g, rgb.b));
      let delta = mx - mn;
      let l = (mx + mn) / 2.0;
      if (delta == 0.0) {
        return vec3<f32>(0.0, 0.0, l);
      }
      let s = delta / (1.0 - abs(2.0 * l - 1.0));
      var h: f32;
      if (mx == rgb.r) {
        h = 60.0 * rem_euclid((rgb.g - rgb.b) / delta, 6.0);
      } else if (mx == rgb.g) {
        h = 60.0 * (((rgb.b - rgb.r) / delta) + 2.0);
      } else {
        h = 60.0 * (((rgb.r - rgb.g) / delta) + 4.0);
      }
      h = rem_euclid(h, 360.0);
      return vec3<f32>(h, s, l);
    }

    fn hslToRgb(h: f32, s: f32, l: f32) -> vec3<f32> {
      let c = (1.0 - abs(2.0 * l - 1.0)) * s;
      let x = c * (1.0 - abs(rem_euclid(h / 60.0, 2.0) - 1.0));
      let m = l - c / 2.0;
      var rgb1: vec3<f32>;
      if (h < 60.0) { rgb1 = vec3<f32>(c, x, 0.0); }
      else if (h < 120.0) { rgb1 = vec3<f32>(x, c, 0.0); }
      else if (h < 180.0) { rgb1 = vec3<f32>(0.0, c, x); }
      else if (h < 240.0) { rgb1 = vec3<f32>(0.0, x, c); }
      else if (h < 300.0) { rgb1 = vec3<f32>(x, 0.0, c); }
      else { rgb1 = vec3<f32>(c, 0.0, x); }
      return rgb1 + vec3<f32>(m, m, m);
    }

    // Raised-cosine blend weight -- see develop_engine.rs's hsl_band_weight
    // doc comment for why this shape (not a triangular ramp) and why band
    // centers exactly 45 degrees apart guarantee at most 2 nonzero weights
    // summing to exactly 1, with no renormalization needed.
    fn hueBandWeight(hueDeg: f32, centerDeg: f32) -> f32 {
      let d = rem_euclid(hueDeg - centerDeg + 180.0, 360.0) - 180.0;
      let dist = abs(d);
      if (dist >= 45.0) {
        return 0.0;
      }
      return 0.5 * (cos(dist / 45.0 * 3.14159265) + 1.0);
    }

    // See develop_engine.rs's apply_hsl_bands doc comment for the full
    // reasoning (combining shift DELTAS not resultant angles, the
    // load-bearing chromaFade suppression in low-saturation regions where
    // hue is numerically unreliable) -- this is a direct WGSL port of the
    // identical formula, no LUT: the whole computation here is cheap,
    // closed-form trig/arithmetic with no interpolation-method ambiguity
    // to diverge on between this and the Rust side.
    fn applyHslBands(rgb: vec3<f32>) -> vec3<f32> {
      let hsl = rgbToHsl(rgb);
      let hPx = hsl.x;
      let sPx = hsl.y;
      let lPx = hsl.z;

      var hueAcc = 0.0;
      var satAcc = 0.0;
      var lumAcc = 0.0;
      for (var i = 0; i < 8; i = i + 1) {
        let w = hueBandWeight(hPx, HSL_BAND_CENTERS[i]);
        let band = hslBands[i];
        hueAcc = hueAcc + w * band.x;
        satAcc = satAcc + w * band.y;
        lumAcc = lumAcc + w * band.z;
      }

      let chromaFade = clamp(sPx / 0.08, 0.0, 1.0);

      let newH = rem_euclid(hPx + hueAcc * chromaFade, 360.0);
      let newS = clamp(sPx * (1.0 + satAcc / 100.0), 0.0, 1.0);

      let lumFrac = (lumAcc / 100.0) * chromaFade;
      var newL: f32;
      if (lumFrac >= 0.0) {
        newL = lPx + (1.0 - lPx) * lumFrac;
      } else {
        newL = lPx + lPx * lumFrac;
      }

      return hslToRgb(newH, newS, newL);
    }

    // Split Toning (M3): a direct WGSL port of develop_engine.rs's
    // apply_split_toning -- see that function's doc comment for the full
    // reasoning (single simultaneous 3-way blend, not two sequential
    // ones -- a real bug a design review caught before this was written;
    // sequential blending is order-dependent whenever both zone weights
    // are nonzero, which given this smoothstep transition is true for
    // nearly every pixel). Reuses rgbToHsl/hslToRgb as-is, no new
    // color-space math. Unlike applyHslBands, this never reads a pixel's
    // own hue or saturation -- only lightness -- so it needs no
    // chromaFade-style near-gray suppression (there's no ratio-of-near-
    // equal-channels term here to be numerically unstable).
    fn splitToneHighlightWeight(l: f32, balance: f32) -> f32 {
      let t = clamp(l + balance / 200.0, 0.0, 1.0);
      return t * t * (3.0 - 2.0 * t); // smoothstep(0,1,t)
    }

    fn applySplitToning(rgb: vec3<f32>) -> vec3<f32> {
      let lPx = rgbToHsl(rgb).z;
      let wHi = splitToneHighlightWeight(lPx, splitToning.balance);
      let wSh = 1.0 - wHi;
      let aSh = wSh * (splitToning.shadow_saturation / 100.0);
      let aHi = wHi * (splitToning.highlight_saturation / 100.0);
      let tintSh = hslToRgb(splitToning.shadow_hue, 1.0, lPx);
      let tintHi = hslToRgb(splitToning.highlight_hue, 1.0, lPx);
      return rgb * (1.0 - aSh - aHi) + tintSh * aSh + tintHi * aHi;
    }

    // Grade pass (M3 Dehaze): the existing global chain (exposure ->
    // contrast -> saturation -> tone curve -> HSL -> split toning),
    // redirected to write into gradedTex instead of the swapchain --
    // Dehaze (below) is the first op in this pipeline that needs
    // NEIGHBORING pixels' graded value (a windowed min-filter for the dark
    // channel, a whole-image reduction for atmospheric light), which a
    // single straight-through fragment shader can't provide -- it cannot
    // read the texture it is currently writing. Every earlier op fit in
    // one shader invocation; this is the first that genuinely needs a
    // multi-pass render graph. Mirrors develop_engine.rs's own Pass 1
    // (writing into its graded buffer instead of the source image directly).
    @fragment
    fn fs_grade(in: VertexOut) -> @location(0) vec4<f32> {
      var rgb = textureSample(srcTexture, srcSampler, in.uv).rgb;
      rgb = apply_adjustments(rgb, adj.exposure_ev, adj.contrast, adj.saturation);
      rgb = vec3<f32>(sampleCurveLut(rgb.x), sampleCurveLut(rgb.y), sampleCurveLut(rgb.z));
      rgb = applyHslBands(rgb);
      rgb = applySplitToning(rgb);
      return vec4<f32>(rgb, 1.0);
    }

    // Dehaze (M3): dark-channel-prior haze removal (He et al. 2009), a
    // direct WGSL port of develop_engine.rs's own restructured algorithm --
    // see that module's doc comment above dehaze_atmospheric_light for the
    // full algorithm, its two named deviations from He et al., and the
    // design review that caught real bugs in earlier drafts of both before
    // this was written. Fixed constants, matching the Rust twin exactly.
    const DEHAZE_PATCH_RADIUS: i32 = 7;
    const DEHAZE_OMEGA: f32 = 0.95;
    const DEHAZE_T0: f32 = 0.1;
    const DEHAZE_REFINE_RADIUS: i32 = 4;

    // All new intermediates read via textureLoad (explicit integer coords,
    // mip 0), not textureSample -- r32float/rgba16float aren't filterable
    // by default in WebGPU (needs an unrequested optional feature), and an
    // exact block/window reduction needs exact taps anyway, never a
    // filtered blend. gradedTex/atmLightFinal/transmissionTexFinal are each
    // read by a FIXED set of passes and never rebound mid-frame;
    // reduceInput/filterInput are each reused across MULTIPLE passes,
    // rebound to a different actual texture per pass (a different bind
    // group each time, same pipeline/shader code) -- see the JS side's own
    // pass-list for exactly which real texture each is bound to per draw.
    @group(0) @binding(8) var gradedTex: texture_2d<f32>;
    @group(0) @binding(9) var reduceInput: texture_2d<f32>;
    @group(0) @binding(10) var atmLightFinal: texture_2d<f32>;
    @group(0) @binding(11) var filterInput: texture_2d<f32>;
    @group(0) @binding(12) var transmissionTexFinal: texture_2d<f32>;
    // Texture & Clarity (M3): lcRgbInput is rebound per pass -- gradedTex
    // for Texture's H/V passes, textureAdjustedTex for Clarity's H/V
    // passes (see the JS side's own bind-group list) -- lcBlurInput is the
    // horizontal-blur scratch texture each op's own V pass reads to
    // complete the box-mean.
    @group(0) @binding(13) var lcRgbInput: texture_2d<f32>;
    @group(0) @binding(14) var lcBlurInput: texture_2d<f32>;

    fn luma(rgb: vec3<f32>) -> f32 {
      return dot(rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
    }

    // Texture & Clarity (M3): a direct WGSL port of develop_engine.rs's
    // own apply_local_contrast -- see that function's doc comment for the
    // additive-delta formula and why it was chosen over a luma-ratio
    // rescale (a design review caught a real hue-scrambling bug in the
    // ratio version whenever luma is near zero or negative, which
    // Contrast alone already makes reachable). Fixed radii, matching the
    // Rust twin exactly.
    const TEXTURE_RADIUS: i32 = 6;
    const CLARITY_RADIUS: i32 = 24;

    @fragment
    fn fs_texture_h(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let dims = vec2<i32>(textureDimensions(lcRgbInput));
      var sum = 0.0;
      for (var dx = -TEXTURE_RADIUS; dx <= TEXTURE_RADIUS; dx = dx + 1) {
        let sx = clamp(coord.x + dx, 0, dims.x - 1);
        sum = sum + luma(textureLoad(lcRgbInput, vec2<i32>(sx, coord.y), 0).rgb);
      }
      let window = f32(2 * TEXTURE_RADIUS + 1);
      return vec4<f32>(sum / window, 0.0, 0.0, 1.0);
    }

    // Vertical pass, completing Texture's blur, then folding in the
    // apply step directly (one fewer pass than a separate step would
    // need, matching fs_min_v's own precedent): re-reads lcRgbInput
    // (gradedTex) at this pixel for the original rgb/luma, adds the same
    // delta to all three channels, and writes the result to
    // textureAdjustedTex.
    @fragment
    fn fs_texture_v(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let dims = vec2<i32>(textureDimensions(lcBlurInput));
      var sum = 0.0;
      for (var dy = -TEXTURE_RADIUS; dy <= TEXTURE_RADIUS; dy = dy + 1) {
        let sy = clamp(coord.y + dy, 0, dims.y - 1);
        sum = sum + textureLoad(lcBlurInput, vec2<i32>(coord.x, sy), 0).r;
      }
      let window = f32(2 * TEXTURE_RADIUS + 1);
      let blurred = sum / window;

      let rgb = textureLoad(lcRgbInput, coord, 0).rgb;
      let l = luma(rgb);
      let delta = (l - blurred) * (adj.texture_amount / 100.0);
      return vec4<f32>(rgb + vec3<f32>(delta, delta, delta), 1.0);
    }

    @fragment
    fn fs_clarity_h(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let dims = vec2<i32>(textureDimensions(lcRgbInput));
      var sum = 0.0;
      for (var dx = -CLARITY_RADIUS; dx <= CLARITY_RADIUS; dx = dx + 1) {
        let sx = clamp(coord.x + dx, 0, dims.x - 1);
        sum = sum + luma(textureLoad(lcRgbInput, vec2<i32>(sx, coord.y), 0).rgb);
      }
      let window = f32(2 * CLARITY_RADIUS + 1);
      return vec4<f32>(sum / window, 0.0, 0.0, 1.0);
    }

    // Vertical pass, completing Clarity's blur + apply -- same shape as
    // fs_texture_v, but this pass's render target (set on the JS side, not
    // a WGSL binding) is gradedTex ITSELF: Clarity is the last of the two
    // local-contrast ops, so its output overwrites gradedTex in place
    // rather than needing a third "final graded" texture. Every downstream
    // consumer of gradedTex (fs_atm_reduce's first pass, fs_min_channel,
    // fs_final) already reads it AFTER this point in pass order, so they
    // transparently see Texture+Clarity already baked in -- sound because
    // WebGPU passes within one command encoder execute strictly in
    // recorded order, the same guarantee gradedTex already relies on today
    // (written once by fs_grade, read three times later in the frame).
    @fragment
    fn fs_clarity_v(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let dims = vec2<i32>(textureDimensions(lcBlurInput));
      var sum = 0.0;
      for (var dy = -CLARITY_RADIUS; dy <= CLARITY_RADIUS; dy = dy + 1) {
        let sy = clamp(coord.y + dy, 0, dims.y - 1);
        sum = sum + textureLoad(lcBlurInput, vec2<i32>(coord.x, sy), 0).r;
      }
      let window = f32(2 * CLARITY_RADIUS + 1);
      let blurred = sum / window;

      let rgb = textureLoad(lcRgbInput, coord, 0).rgb;
      let l = luma(rgb);
      let delta = (l - blurred) * (adj.clarity_amount / 100.0);
      return vec4<f32>(rgb + vec3<f32>(delta, delta, delta), 1.0);
    }

    // Atmospheric-light reduction: one pass of an 8x8-block ARGMAX-BY-
    // LUMINANCE reduction (see develop_engine.rs's dehaze_atmospheric_light
    // doc comment for why this, not independent per-channel maxima -- a
    // real bug an earlier draft had) -- run repeatedly against
    // progressively smaller inputs (JS side decides how many times, sized
    // to the source image's own dimensions) until reaching 1x1. Duplicate
    // clamped edge taps are harmless for an argmax (unlike a sum), since a
    // repeated candidate can't change which one wins.
    @fragment
    fn fs_atm_reduce(in: VertexOut) -> @location(0) vec4<f32> {
      let outCoord = vec2<i32>(in.position.xy);
      let dims = vec2<i32>(textureDimensions(reduceInput));
      var best = vec3<f32>(0.0, 0.0, 0.0);
      var bestLuma = -1.0;
      for (var dy = 0; dy < 8; dy = dy + 1) {
        for (var dx = 0; dx < 8; dx = dx + 1) {
          let coord = clamp(outCoord * 8 + vec2<i32>(dx, dy), vec2<i32>(0, 0), dims - vec2<i32>(1, 1));
          let c = textureLoad(reduceInput, coord, 0).rgb;
          let l = luma(c);
          if (l > bestLuma) {
            bestLuma = l;
            best = c;
          }
        }
      }
      return vec4<f32>(best, 1.0);
    }

    // Normalized min-channel: min_c(I^c/A^c), per pixel -- the atmospheric-
    // light division happens HERE, before the windowed min below, matching
    // He et al. exactly. A real bug an earlier draft had: collapsing the
    // cross-channel min FIRST and dividing the resulting scalar by a
    // single scalar representative of A afterward is not a numerically-
    // close approximation but a structurally different, generally wrong,
    // result (see develop_engine.rs's own doc comment for the concrete
    // counterexample that caught this).
    @fragment
    fn fs_min_channel(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let c = textureLoad(gradedTex, coord, 0).rgb;
      let a = textureLoad(atmLightFinal, vec2<i32>(0, 0), 0).rgb;
      let m = min(c.r / a.r, min(c.g / a.g, c.b / a.b));
      return vec4<f32>(m, 0.0, 0.0, 1.0);
    }

    // Separable box-MIN filter (the dark-channel step): horizontal pass
    // then vertical pass -- correct (not just cheaper) because a
    // rectangular window's min is associative/commutative over its two
    // axes independently. Edge taps clamp to the input texture's own
    // bounds -- textureLoad on an out-of-range coordinate silently returns
    // zero, which would corrupt the min near image borders if not guarded.
    @fragment
    fn fs_min_h(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let dims = vec2<i32>(textureDimensions(filterInput));
      var m = 1e6;
      for (var dx = -DEHAZE_PATCH_RADIUS; dx <= DEHAZE_PATCH_RADIUS; dx = dx + 1) {
        let sx = clamp(coord.x + dx, 0, dims.x - 1);
        m = min(m, textureLoad(filterInput, vec2<i32>(sx, coord.y), 0).r);
      }
      return vec4<f32>(m, 0.0, 0.0, 1.0);
    }

    // Vertical pass, completing the dark channel -- folds in the raw-
    // transmission step (1 - omega * darkChannel) directly (one fewer pass
    // than a separate step would need), matching develop_engine.rs's own
    // combined t_raw computation.
    @fragment
    fn fs_min_v(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let dims = vec2<i32>(textureDimensions(filterInput));
      var m = 1e6;
      for (var dy = -DEHAZE_PATCH_RADIUS; dy <= DEHAZE_PATCH_RADIUS; dy = dy + 1) {
        let sy = clamp(coord.y + dy, 0, dims.y - 1);
        m = min(m, textureLoad(filterInput, vec2<i32>(coord.x, sy), 0).r);
      }
      let tRaw = 1.0 - DEHAZE_OMEGA * m;
      return vec4<f32>(tRaw, 0.0, 0.0, 1.0);
    }

    // Separable box-MEAN filter (transmission refinement, standing in for
    // He et al.'s edge-preserving guided filter -- named limitation: mild
    // haloing near strong contrast edges a real guided filter would
    // avoid). A sum-based sliding-window accumulator would be cheaper on
    // the CPU side (see develop_engine.rs's separable_mean_filter), but a
    // naive per-tap sum here is simplest and still cheap at this radius --
    // GPU fragment shaders parallelize across pixels, not within one.
    @fragment
    fn fs_mean_h(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let dims = vec2<i32>(textureDimensions(filterInput));
      var sum = 0.0;
      for (var dx = -DEHAZE_REFINE_RADIUS; dx <= DEHAZE_REFINE_RADIUS; dx = dx + 1) {
        let sx = clamp(coord.x + dx, 0, dims.x - 1);
        sum = sum + textureLoad(filterInput, vec2<i32>(sx, coord.y), 0).r;
      }
      let window = f32(2 * DEHAZE_REFINE_RADIUS + 1);
      return vec4<f32>(sum / window, 0.0, 0.0, 1.0);
    }

    @fragment
    fn fs_mean_v(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let dims = vec2<i32>(textureDimensions(filterInput));
      var sum = 0.0;
      for (var dy = -DEHAZE_REFINE_RADIUS; dy <= DEHAZE_REFINE_RADIUS; dy = dy + 1) {
        let sy = clamp(coord.y + dy, 0, dims.y - 1);
        sum = sum + textureLoad(filterInput, vec2<i32>(coord.x, sy), 0).r;
      }
      let window = f32(2 * DEHAZE_REFINE_RADIUS + 1);
      return vec4<f32>(sum / window, 0.0, 0.0, 1.0);
    }

    // Final pass: reads the graded color back from gradedTex (not a
    // re-sample of srcTexture -- every global op through Texture and
    // Clarity is already baked in, see fs_clarity_v's own doc comment),
    // applies the Dehaze recovery + amount blend, then Vignette (a direct
    // WGSL port of develop_engine.rs's own vignette_factor -- pure
    // per-pixel, no neighboring-pixel data needed, so it folds directly
    // into this pass rather than adding a new buffer/pass the way Dehaze/
    // Texture/Clarity all needed), then runs the EXISTING mask loop
    // unchanged, writing the real swapchain output. Safe to read the
    // dehaze maps unconditionally even at amount=0 (the blend multiplies
    // their contribution by 0) as long as they're always populated with
    // REAL values before this ever runs -- see writeAdjustmentsAndRender's
    // dirty-key caching, which always treats "nothing cached yet" (the
    // very first render) as dirty.
    @fragment
    fn fs_final(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      var rgb = textureLoad(gradedTex, coord, 0).rgb;

      let a = textureLoad(atmLightFinal, vec2<i32>(0, 0), 0).rgb;
      let t = max(textureLoad(transmissionTexFinal, coord, 0).r, DEHAZE_T0);
      let recovered = (rgb - a) / t + a;
      rgb = rgb + (recovered - rgb) * (adj.dehaze_amount / 100.0);

      // Noise Reduction (luminance, then color), then Sharpening -- see
      // develop_engine.rs's own doc comments (above sharpen_delta/
      // luma_nr_delta/color_nr_delta) for the full formula reasoning and
      // the pipeline-order rationale (NR before Sharpen, to avoid
      // re-amplifying noise). origLuma/origRgb re-read gradedTex directly
      // (NOT the local rgb, which may already carry Dehaze's recovery) --
      // matching the Rust twin's own use of graded_luma/graded_rgb, the
      // same pre-Dehaze-recovery snapshot the blur passes above were
      // computed from.
      let origRgb = textureLoad(gradedTex, coord, 0).rgb;
      let origLuma = luma(origRgb);

      let lnBlurred = textureLoad(lumaNrBlurFinal, coord, 0).r;
      let lnDiff = origLuma - lnBlurred;
      let lnEdgeThreshold = max(NR_DETAIL_SCALE * (1.0 - lumaNrParams.detail / 100.0), 0.0001);
      let lnSmoothWeight = 1.0 - smoothstep(0.0, lnEdgeThreshold, abs(lnDiff));
      let lnSmoothDelta = -lnDiff * (lumaNrParams.amount / 100.0) * lnSmoothWeight;
      let lnContrastRestore = lnDiff * (lumaNrParams.contrast / 100.0) * NR_CONTRAST_STRENGTH * (lumaNrParams.amount / 100.0);
      let lnTotal = lnSmoothDelta + lnContrastRestore;
      rgb = rgb + vec3<f32>(lnTotal, lnTotal, lnTotal);

      let cnrBlurred = textureLoad(colorNrBlurFinal, coord, 0).rgb;
      let cnrD = cnrBlurred - origRgb;
      let cnrWeightedMean = dot(cnrD, vec3<f32>(0.2126, 0.7152, 0.0722));
      let cnrChromaDelta = cnrD - vec3<f32>(cnrWeightedMean, cnrWeightedMean, cnrWeightedMean);
      let cnrMag = length(cnrChromaDelta);
      let cnrThreshold = max(COLOR_NR_DETAIL_SCALE * (1.0 - colorNrParams.detail / 100.0), 0.0001);
      let cnrSmoothWeight = 1.0 - smoothstep(0.0, cnrThreshold, cnrMag);
      let cnrK = (colorNrParams.amount / 100.0) * cnrSmoothWeight;
      rgb = rgb + cnrChromaDelta * cnrK;

      let shBlurred = textureLoad(sharpenBlurFinal, coord, 0).r;
      let shDiff = origLuma - shBlurred;
      let shDetailThreshold = max(SHARPEN_DETAIL_SCALE * (1.0 - sharpenParams.detail / 100.0), 0.0001);
      let shDetailWeight = smoothstep(0.0, shDetailThreshold, abs(shDiff));
      // Local gradient magnitude (Masking): a 4-neighbor central
      // difference on gradedTex's own luma -- a genuine spatial "near an
      // edge" signal, deliberately distinct from Detail's own per-pixel
      // diff-amplitude gate above (see the Rust twin's own
      // local_gradient_magnitude doc comment for why the two needed to be
      // different, per this slice's design review).
      let dimsG = vec2<i32>(textureDimensions(gradedTex));
      let shXm = clamp(coord.x - 1, 0, dimsG.x - 1);
      let shXp = clamp(coord.x + 1, 0, dimsG.x - 1);
      let shYm = clamp(coord.y - 1, 0, dimsG.y - 1);
      let shYp = clamp(coord.y + 1, 0, dimsG.y - 1);
      let shGx = luma(textureLoad(gradedTex, vec2<i32>(shXp, coord.y), 0).rgb) - luma(textureLoad(gradedTex, vec2<i32>(shXm, coord.y), 0).rgb);
      let shGy = luma(textureLoad(gradedTex, vec2<i32>(coord.x, shYp), 0).rgb) - luma(textureLoad(gradedTex, vec2<i32>(coord.x, shYm), 0).rgb);
      let shGradMag = sqrt(shGx * shGx + shGy * shGy) * 0.5;
      let shMaskThreshold = max(SHARPEN_MASK_SCALE * (sharpenParams.masking / 100.0), 0.0001);
      let shMaskWeight = smoothstep(0.0, shMaskThreshold, shGradMag);
      let shDelta = shDiff * (sharpenParams.amount / 100.0) * shDetailWeight * shMaskWeight * SHARPEN_STRENGTH;
      rgb = rgb + vec3<f32>(shDelta, shDelta, shDelta);

      // Vignette: aspect-corrected elliptical falloff -- see
      // develop_engine.rs's vignette_factor doc comment for the full
      // shape/parameter reasoning, mirrored exactly here.
      let dims = vec2<f32>(textureDimensions(gradedTex));
      let vAspect = dims.y / dims.x;
      let centered = (in.uv - vec2<f32>(0.5, 0.5)) * 2.0;
      let vDx = centered.x;
      let vDy = centered.y * vAspect;
      let cornerDist = sqrt(1.0 + vAspect * vAspect);
      let normDist = sqrt(vDx * vDx + vDy * vDy) / cornerDist;
      let vInner = clamp(vignette.midpoint / 100.0, 0.0, 0.999);
      let vOuter = clamp(vInner + max(vignette.feather / 100.0, 0.001) * (1.0 - vInner), vInner + 0.001, 1.0);
      let vT = smoothstep(vInner, vOuter, normDist);
      let vignetteFactor = 1.0 + (vignette.amount / 100.0) * vT;
      rgb = rgb * vignetteFactor;

      // Grain: pure per-pixel procedural noise, applied right after
      // Vignette -- see grainDelta's own doc comment / develop_engine.rs's
      // grain_delta for the full reasoning.
      let gDelta = grainDelta(vec2<f32>(coord));
      rgb = rgb + vec3<f32>(gDelta, gDelta, gDelta);

      // Local adjustments layer on top of the globally-graded image,
      // matching real Lightroom's own ordering and develop_engine.rs's.
      let mask_count = i32(adj.mask_count);
      for (var i = 0; i < mask_count; i = i + 1) {
        let m = masks[i];
        let kind = m.params.z;
        var weight: f32;
        // Ascending bands, not the two-comparison overlapping-threshold
        // chain this used to be (a real bug a design review caught: that
        // older chain only correctly bucketed kinds 0/1/2 by coincidence,
        // and adding a 4th kind would have silently aliased it onto the
        // brush branch). Each else-if here bounds exactly one kind, so a
        // future 5th kind just needs one more band inserted before the
        // final else, not a re-audit of the whole chain's ordering.
        if (kind < 0.5) {
          // Linear: projection-onto-segment parametrization. 0 at start, 1
          // at end, extrapolated linearly beyond both, then clamped.
          let dir = m.start_end.zw - m.start_end.xy;
          let len2 = max(dot(dir, dir), 0.000001);
          let t = dot(in.uv - m.start_end.xy, dir) / len2;
          // Feather widens the transition band symmetrically around the
          // midpoint -- at feather=50 the pins themselves move to weight
          // 0.25/0.75 rather than staying at 0/1, matching real Lightroom's
          // own gradient-feather model (separate outer feather lines beyond
          // the pins), not a corner-only softening.
          let softness = clamp(m.params.x / 100.0, 0.0, 0.999);
          weight = clamp((t + softness) / (1.0 + 2.0 * softness), 0.0, 1.0);
          if (m.params.y > 0.5) { weight = 1.0 - weight; }
        } else if (kind < 1.5) {
          // Radial: start_end.xy = center, start_end.zw = (radiusX,
          // radiusY). d is 0 at center, 1 at the ellipse boundary. At
          // feather=0 the transition band is d in [0.999, 1.0] (width
          // 0.001, sitting just inside the boundary, not symmetric around
          // it); widens to roughly d in [0.001,1.999] as feather
          // approaches 100. insideWeight is ~1 at/near the center
          // regardless of feather.
          let dx = (in.uv.x - m.start_end.x) / m.start_end.z;
          let dy = (in.uv.y - m.start_end.y) / m.start_end.w;
          let d = sqrt(dx * dx + dy * dy);
          let softness = clamp(m.params.x / 100.0, 0.0, 0.999);
          let denom = max(2.0 * softness, 0.001);
          let insideWeight = clamp((1.0 + softness - d) / denom, 0.0, 1.0);
          // Default (invert=false) applies the effect OUTSIDE the ellipse
          // -- real Lightroom's own Radial Filter convention (its classic
          // vignette use case); invert=true applies it inside (spotlight/
          // subject use case).
          weight = select(1.0 - insideWeight, insideWeight, m.params.y > 0.5);
        } else if (kind < 2.5) {
          // Brush: rasterized CPU-side into this mask's own texture-array
          // layer, luminance-as-weight (see DevelopCanvas.svelte's
          // rasterizeDab/syncBrushRasterization and develop_engine.rs's
          // dab_falloff/brush_mask_weight for the exact accumulation
          // formula both renderers agree on).
          let layer = i32(m.params.w);
          weight = textureSampleLevel(brushMasks, srcSampler, in.uv, layer, 0.0).r;
          if (m.params.y > 0.5) { weight = 1.0 - weight; }
        } else if (kind < 3.5) {
          // Luminance range: the first kind whose weight depends on pixel
          // VALUE, not position -- reads rgb as already graded by every
          // PRECEDING mask in the stack (this is a mutating accumulator,
          // see develop_engine.rs's Mask::weight doc comment for why this
          // order-dependence is the correct WYSIWYG behavior, not a bug).
          // Trapezoidal falloff, raw-then-clamp-once (same style as the
          // linear/radial formulas above) -- start_end.x/y hold
          // rangeMin/rangeMax (0-100, same scale as feather).
          let luma = dot(rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
          let range_min = m.start_end.x / 100.0;
          let range_max = m.start_end.y / 100.0;
          let softness = clamp(m.params.x / 100.0, 0.0, 0.999);
          let feather_width = softness * 0.5;
          let denom = max(feather_width, 0.001);
          let rising = (luma - (range_min - feather_width)) / denom;
          let falling = (range_max + feather_width - luma) / denom;
          weight = clamp(min(rising, falling), 0.0, 1.0);
          if (m.params.y > 0.5) { weight = 1.0 - weight; }
        } else {
          // Color range (4), and any future kind >= 4: also reads the
          // mutating rgb accumulator, same order-dependence as luminance
          // range. One reference color and one tolerance rather than two
          // edges, so this is a SINGLE-sided falloff (closer to radial's
          // inside/outside shape, but in RGB-distance space) rather than
          // luminance's two-sided min(rising,falling) band. start_end.xyz
          // holds refColor (0-1, matching this shader's own texture-sample
          // convention), start_end.w holds range (0-100).
          //
          // The exact-match pixel must get weight=1 regardless of how
          // small feather is -- an earlier draft blended feather_width
          // into the numerator alongside threshold, which meant the same
          // denom floor meant to prevent divide-by-zero at feather=0 also
          // diluted that term, so dist=0 could evaluate to LESS than full
          // weight at low feather (see develop_engine.rs's
          // color_mask_weight doc comment for the full derivation). Fixed
          // here the same way: denom is the transition width added AFTER
          // threshold via the "+ 1.0", never blended into the numerator.
          let dist = distance(rgb, m.start_end.xyz);
          let max_dist = sqrt(3.0);
          let threshold = clamp(m.start_end.w / 100.0, 0.0, 1.0) * max_dist;
          let softness = clamp(m.params.x / 100.0, 0.0, 0.999);
          let feather_width = softness * max_dist * 0.5;
          let denom = max(feather_width, 0.001);
          weight = clamp((threshold - dist) / denom + 1.0, 0.0, 1.0);
          if (m.params.y > 0.5) { weight = 1.0 - weight; }
        }
        rgb = mix(rgb, apply_adjustments(rgb, m.adjustments.x, m.adjustments.y, m.adjustments.z), weight);

        // Selected-mask overlay (soft colored fill, toggleable): every
        // no-geometry kind (brush, luminance range, color range -- kind >=
        // 2) -- linear/radial deliberately excluded, preserving the prior
        // explicit scope decision that they keep their existing
        // dashed-outline-only feedback (PROGRESS.md,
        // mask-overlay-feather-indicators slice). Reuses the weight just
        // computed above for THIS mask -- already invert-adjusted, already
        // evaluated against the correct (pre-this-mask) rgb state -- so no
        // separate re-sample-and-re-invert step is needed regardless of
        // kind, unlike the brush-only texture-based mechanism this replaces.
        if (kind > 1.5 && i == i32(adj.selected_mask_index)) {
          rgb = mix(rgb, vec3<f32>(1.0, 0.24, 0.24), weight * 0.55);
        }
      }

      rgb = clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0));
      return vec4<f32>(rgb, 1.0);
    }
  `;

  async function initGpu(/** @type {HTMLCanvasElement} */ canvas) {
    if (!("gpu" in navigator)) {
      throw new Error("navigator.gpu is undefined in this webview");
    }
    const adapter = await navigator.gpu.requestAdapter();
    if (!adapter) throw new Error("requestAdapter() returned null");
    device = await adapter.requestDevice();
    presentationFormat = navigator.gpu.getPreferredCanvasFormat();

    // A real, pre-existing gap this component never had a way to surface:
    // most WebGPU errors (shader compile failures, bind-group-layout
    // mismatches, invalid texture usage, etc.) are reported ASYNCHRONOUSLY
    // via this event, not as a catchable JS exception at the call site --
    // createShaderModule/createRenderPipeline/beginRenderPass etc. don't
    // throw on an invalid WGSL module or a malformed pipeline; they just
    // silently produce an invalid resource, and any draw using it
    // no-ops. Without this listener, such an error would only ever be
    // visible in the webview's own devtools console, which isn't
    // reachable from outside the app -- worth catching for real now that
    // Dehaze made this shader's own pipeline count/complexity jump
    // significantly (1 pipeline -> 8).
    device.addEventListener("uncapturederror", (/** @type {any} */ event) => {
      status = "error";
      errorMessage = `WebGPU: ${event.error.message}`;
    });

    context = canvas.getContext("webgpu");
    if (!context) throw new Error("canvas.getContext('webgpu') returned null");
    context.configure({ device, format: presentationFormat, alphaMode: "opaque" });

    // One compiled module, many entry points -- each createRenderPipeline
    // call below just picks a different entryPoint out of the SAME
    // compiled WGSL, no separate compilation per pass. Each pipeline gets
    // its OWN layout:"auto"-inferred bind group layout, scoped to only the
    // bindings that specific entry point's own code actually references
    // (NOT the whole module's declarations) -- see the WGSL source's own
    // comment on gradePipeline/pipeline(final)'s deliberately DIFFERENT
    // inferred layouts for why a bind group built for one pipeline can't
    // be reused for another, even where their WGSL code looks similar.
    const module = device.createShaderModule({ code: WGSL });
    // Kept permanently (not a debugging leftover) -- shader COMPILATION
    // errors are a separate WebGPU error category from the validation
    // errors device.onuncapturederror catches above; they surface ONLY via
    // this async call, never as a device error. Without it, a future WGSL
    // typo could compile to a silently-invalid module with zero visible
    // signal beyond "the canvas is blank" -- exactly the class of bug that
    // made Dehaze's own real bind-group bug (a missing srcSampler entry,
    // unrelated to this specific check but discovered while debugging the
    // same "no error surfaces anywhere" symptom) so slow to localize.
    module.getCompilationInfo().then((info) => {
      const problems = info.messages.filter((m) => m.type !== "info");
      if (problems.length > 0) {
        status = "error";
        errorMessage = `WGSL compile: ${problems.map((m) => `line ${m.lineNum}: ${m.message}`).join(" | ")}`;
      }
    });
    pipeline = device.createRenderPipeline({
      layout: "auto",
      vertex: { module, entryPoint: "vs_main" },
      fragment: { module, entryPoint: "fs_final", targets: [{ format: presentationFormat }] },
      primitive: { topology: "triangle-list" },
    });
    gradePipeline = device.createRenderPipeline({
      layout: "auto",
      vertex: { module, entryPoint: "vs_main" },
      fragment: { module, entryPoint: "fs_grade", targets: [{ format: "rgba16float" }] },
      primitive: { topology: "triangle-list" },
    });
    atmReducePipeline = device.createRenderPipeline({
      layout: "auto",
      vertex: { module, entryPoint: "vs_main" },
      fragment: { module, entryPoint: "fs_atm_reduce", targets: [{ format: "rgba16float" }] },
      primitive: { topology: "triangle-list" },
    });
    minChannelPipeline = device.createRenderPipeline({
      layout: "auto",
      vertex: { module, entryPoint: "vs_main" },
      fragment: { module, entryPoint: "fs_min_channel", targets: [{ format: "r32float" }] },
      primitive: { topology: "triangle-list" },
    });
    minHPipeline = device.createRenderPipeline({
      layout: "auto",
      vertex: { module, entryPoint: "vs_main" },
      fragment: { module, entryPoint: "fs_min_h", targets: [{ format: "r32float" }] },
      primitive: { topology: "triangle-list" },
    });
    minVPipeline = device.createRenderPipeline({
      layout: "auto",
      vertex: { module, entryPoint: "vs_main" },
      fragment: { module, entryPoint: "fs_min_v", targets: [{ format: "r32float" }] },
      primitive: { topology: "triangle-list" },
    });
    meanHPipeline = device.createRenderPipeline({
      layout: "auto",
      vertex: { module, entryPoint: "vs_main" },
      fragment: { module, entryPoint: "fs_mean_h", targets: [{ format: "r32float" }] },
      primitive: { topology: "triangle-list" },
    });
    meanVPipeline = device.createRenderPipeline({
      layout: "auto",
      vertex: { module, entryPoint: "vs_main" },
      fragment: { module, entryPoint: "fs_mean_v", targets: [{ format: "r32float" }] },
      primitive: { topology: "triangle-list" },
    });
    textureHPipeline = device.createRenderPipeline({
      layout: "auto",
      vertex: { module, entryPoint: "vs_main" },
      fragment: { module, entryPoint: "fs_texture_h", targets: [{ format: "r32float" }] },
      primitive: { topology: "triangle-list" },
    });
    textureVPipeline = device.createRenderPipeline({
      layout: "auto",
      vertex: { module, entryPoint: "vs_main" },
      fragment: { module, entryPoint: "fs_texture_v", targets: [{ format: "rgba16float" }] },
      primitive: { topology: "triangle-list" },
    });
    clarityHPipeline = device.createRenderPipeline({
      layout: "auto",
      vertex: { module, entryPoint: "vs_main" },
      fragment: { module, entryPoint: "fs_clarity_h", targets: [{ format: "r32float" }] },
      primitive: { topology: "triangle-list" },
    });
    clarityVPipeline = device.createRenderPipeline({
      layout: "auto",
      vertex: { module, entryPoint: "vs_main" },
      fragment: { module, entryPoint: "fs_clarity_v", targets: [{ format: "rgba16float" }] },
      primitive: { topology: "triangle-list" },
    });
    sharpenHPipeline = device.createRenderPipeline({
      layout: "auto",
      vertex: { module, entryPoint: "vs_main" },
      fragment: { module, entryPoint: "fs_sharpen_h", targets: [{ format: "r32float" }] },
      primitive: { topology: "triangle-list" },
    });
    sharpenVPipeline = device.createRenderPipeline({
      layout: "auto",
      vertex: { module, entryPoint: "vs_main" },
      fragment: { module, entryPoint: "fs_sharpen_v", targets: [{ format: "r32float" }] },
      primitive: { topology: "triangle-list" },
    });
    lumaNRHPipeline = device.createRenderPipeline({
      layout: "auto",
      vertex: { module, entryPoint: "vs_main" },
      fragment: { module, entryPoint: "fs_lumaNR_h", targets: [{ format: "r32float" }] },
      primitive: { topology: "triangle-list" },
    });
    lumaNRVPipeline = device.createRenderPipeline({
      layout: "auto",
      vertex: { module, entryPoint: "vs_main" },
      fragment: { module, entryPoint: "fs_lumaNR_v", targets: [{ format: "r32float" }] },
      primitive: { topology: "triangle-list" },
    });
    colorNRHPipeline = device.createRenderPipeline({
      layout: "auto",
      vertex: { module, entryPoint: "vs_main" },
      fragment: { module, entryPoint: "fs_colorNR_h", targets: [{ format: "rgba16float" }] },
      primitive: { topology: "triangle-list" },
    });
    colorNRVPipeline = device.createRenderPipeline({
      layout: "auto",
      vertex: { module, entryPoint: "vs_main" },
      fragment: { module, entryPoint: "fs_colorNR_v", targets: [{ format: "rgba16float" }] },
      primitive: { topology: "triangle-list" },
    });

    uniformBuffer = device.createBuffer({
      size: 32, // 8 x f32 (exposure, contrast, saturation, mask_count, selected_mask_index, dehaze_amount, texture_amount, clarity_amount)
      usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
    });
    masksBuffer = device.createBuffer({
      size: MAX_MASKS * 12 * 4, // 12 f32s (3x vec4) per mask
      usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
    });
    curveLutBuffer = device.createBuffer({
      size: 64 * 16, // 64 vec4<f32> (256 f32 samples), packed to avoid WGSL's 16-byte uniform-array-stride requirement -- see the Mask struct's own comment on this exact footgun
      usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
    });
    hslBandsBuffer = device.createBuffer({
      size: 8 * 16, // 8 bands x vec4<f32> (hue, saturation, luminance, unused padding)
      usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
    });
    splitToningBuffer = device.createBuffer({
      size: 8 * 4, // 8 f32 (5 real fields + 3 padding), matches the WGSL SplitToning struct
      usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
    });
    vignetteBuffer = device.createBuffer({
      size: 4 * 4, // 4 f32 (3 real fields + 1 padding), matches the WGSL Vignette struct
      usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
    });
    grainBuffer = device.createBuffer({
      size: 4 * 4, // 4 f32 (3 real fields + 1 padding), matches the WGSL Grain struct
      usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
    });
    sharpenBuffer = device.createBuffer({
      size: 4 * 4, // 4 f32 (amount, radius, detail, masking), matches the WGSL SharpenParams struct
      usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
    });
    lumaNRBuffer = device.createBuffer({
      size: 4 * 4, // 4 f32 (3 real fields + 1 padding), matches the WGSL LumaNrParams struct
      usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
    });
    colorNRBuffer = device.createBuffer({
      size: 4 * 4, // 4 f32 (2 real fields + 2 padding), matches the WGSL ColorNrParams struct
      usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
    });
  }

  /** Every GPU-resource-side-effect of "a decoded bitmap is now the active
   * source image" -- shared between loadImage (a genuinely new image) and
   * upgradeToFullTier (the SAME image, a higher-resolution decode of it),
   * so the ~70 lines of WebGPU setup below exist in exactly one place
   * rather than two copies that could drift out of sync. */
  /** The atmospheric-light reduction chain's own successive sizes (M3
   * Dehaze) -- each pass does an 8x8-block reduction (see fs_atm_reduce's
   * own doc comment), so each step's size is ceil(previous/8), stopping
   * once BOTH dimensions reach 1. `do...while` (not `while`) guarantees at
   * least one entry even for a degenerate 1x1 source, so
   * atmLightChain[atmLightChain.length - 1] is never read from an empty
   * array. */
  function buildAtmLightChainSizes(/** @type {number} */ width, /** @type {number} */ height) {
    const sizes = [];
    let w = width;
    let h = height;
    do {
      w = Math.max(1, Math.ceil(w / 8));
      h = Math.max(1, Math.ceil(h / 8));
      sizes.push([w, h]);
    } while (w > 1 || h > 1);
    return sizes;
  }

  /** One fullscreen-triangle draw into `outputView` -- the shared shape
   * every dehaze pass and the existing final pass use, factored out to
   * avoid repeating the same beginRenderPass/setPipeline/setBindGroup/
   * draw/end boilerplate for what's now up to ~9 passes per render. */
  function runFullscreenPass(
    /** @type {GPUCommandEncoder} */ enc,
    /** @type {GPURenderPipeline} */ pl,
    /** @type {GPUBindGroup} */ bg,
    /** @type {GPUTextureView} */ outputView,
  ) {
    const p = enc.beginRenderPass({
      colorAttachments: [{ view: outputView, loadOp: "clear", storeOp: "store", clearValue: { r: 0, g: 0, b: 0, a: 1 } }],
    });
    p.setPipeline(pl);
    p.setBindGroup(0, bg);
    p.draw(3);
    p.end();
  }

  async function applyBitmapToGpu(/** @type {ImageBitmap} */ bitmap) {
    // Both callers (loadImage, upgradeToFullTier) already only reach here
    // once initGpu has run, but re-asserted here too -- both for a real
    // defensive guard against an unexpected call order, and because
    // TypeScript's null-narrowing from a caller's own guard doesn't carry
    // across a function boundary.
    if (!device || !context || !pipeline || !gradePipeline || !atmReducePipeline || !minChannelPipeline || !minHPipeline || !minVPipeline || !meanHPipeline || !meanVPipeline || !textureHPipeline || !textureVPipeline || !clarityHPipeline || !clarityVPipeline || !sharpenHPipeline || !sharpenVPipeline || !lumaNRHPipeline || !lumaNRVPipeline || !colorNRHPipeline || !colorNRVPipeline || !uniformBuffer || !masksBuffer || !curveLutBuffer || !hslBandsBuffer || !splitToningBuffer || !vignetteBuffer || !grainBuffer || !sharpenBuffer || !lumaNRBuffer || !colorNRBuffer) return;

    // GPU texture-dimension safety: a genuinely native-resolution decode
    // (the 1:1 tier, upgradeToFullTier) could in principle exceed this
    // device's actual texture-size limit on a very-high-megapixel body --
    // the draft tier is already capped to DEVELOP_PREVIEW_MAX_DIMENSION so
    // this is normally a no-op there. Downscaling defensively here (one
    // code path, both tiers) is an honest, accepted degradation on
    // whatever hardware this ends up mattering for, not a crash from an
    // opaque WebGPU validation error.
    const maxDim = device.limits.maxTextureDimension2D;
    if (bitmap.width > maxDim || bitmap.height > maxDim) {
      const scale = maxDim / Math.max(bitmap.width, bitmap.height);
      bitmap = await createImageBitmap(bitmap, {
        resizeWidth: Math.max(1, Math.round(bitmap.width * scale)),
        resizeHeight: Math.max(1, Math.round(bitmap.height * scale)),
        resizeQuality: "high",
      });
    }

    sourceTexture?.destroy();
    sourceTexture = device.createTexture({
      size: [bitmap.width, bitmap.height],
      format: "rgba8unorm",
      usage:
        GPUTextureUsage.TEXTURE_BINDING |
        GPUTextureUsage.COPY_DST |
        GPUTextureUsage.RENDER_ATTACHMENT,
    });
    device.queue.copyExternalImageToTexture(
      { source: bitmap },
      { texture: sourceTexture },
      [bitmap.width, bitmap.height],
    );

    // M3 Slice 8: retain sampleable pixel data for the color-range
    // eyedropper -- draw the SAME bitmap once into a persistent 2D
    // OffscreenCanvas before it's discarded. Neither this draw nor the GPU
    // upload above closes/consumes the bitmap, so order between them
    // doesn't matter -- bitmap.close() happens later in this function
    // (NOT here), once every remaining `bitmap.width`/`.height` read below
    // is done: per spec, close() zeroes a bitmap's width/height, so
    // closing it before those later reads would corrupt the brush texture
    // array's size and the canvas's own dimensions. Re-drawn on every call
    // (including a tier upgrade), not just the first -- leaving this stale
    // at draft resolution while the GPU texture is full-res would silently
    // make the eyedropper keep sampling coarser data at exactly the moment
    // the user zoomed in to inspect detail more closely.
    sourceSampleCanvas = new OffscreenCanvas(bitmap.width, bitmap.height);
    sourceSampleCtx = /** @type {OffscreenCanvasRenderingContext2D} */ (sourceSampleCanvas.getContext("2d"));
    sourceSampleCtx.drawImage(bitmap, 0, 0);

    // M3 Slice 7: recreated whenever the active bitmap's resolution
    // changes (new image, OR a tier upgrade) -- must be sized to match, an
    // OffscreenCanvas at the wrong resolution would rasterize dabs at the
    // wrong scale. Existing brush masks' dab lists are stored normalized
    // (0-1), so re-rasterizing from scratch into freshly-sized canvases
    // (via syncBrushRasterization, called below through this function's
    // caller's own writeAdjustmentsAndRender()) is correct with no
    // special-casing regardless of why the resolution changed.
    brushTextureArray?.destroy();
    brushTextureArray = device.createTexture({
      size: [bitmap.width, bitmap.height, MAX_MASKS],
      format: "rgba8unorm",
      usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.COPY_DST,
    });
    brushRasterState = new Map();
    freeBrushLayers = Array.from({ length: MAX_MASKS }, (_, i) => i);

    // Dehaze (M3): intermediates sized to match this bitmap's own
    // resolution -- same "recreate whenever the source resolution changes"
    // lifecycle as sourceTexture/brushTextureArray above.
    gradedTex?.destroy();
    gradedTex = device.createTexture({
      size: [bitmap.width, bitmap.height],
      // rgba16float, not rgba8unorm -- filterable+renderable by default
      // (no feature request needed) and avoids a NEW 8-bit quantization
      // step between Split Toning and Dehaze/masks that didn't exist in
      // the old single-pass fs_main.
      format: "rgba16float",
      usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.RENDER_ATTACHMENT,
    });
    minChannelTex?.destroy();
    minChannelTex = device.createTexture({
      size: [bitmap.width, bitmap.height],
      format: "r32float",
      usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.RENDER_ATTACHMENT,
    });
    darkChannelHTex?.destroy();
    darkChannelHTex = device.createTexture({
      size: [bitmap.width, bitmap.height],
      format: "r32float",
      usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.RENDER_ATTACHMENT,
    });
    tRawTex?.destroy();
    tRawTex = device.createTexture({
      size: [bitmap.width, bitmap.height],
      format: "r32float",
      usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.RENDER_ATTACHMENT,
    });
    transmissionHTex?.destroy();
    transmissionHTex = device.createTexture({
      size: [bitmap.width, bitmap.height],
      format: "r32float",
      usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.RENDER_ATTACHMENT,
    });
    transmissionTex?.destroy();
    transmissionTex = device.createTexture({
      size: [bitmap.width, bitmap.height],
      format: "r32float",
      usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.RENDER_ATTACHMENT,
    });

    // Texture & Clarity (M3): same "recreate whenever source resolution
    // changes" lifecycle as every intermediate above. textureAdjustedTex
    // is Texture's final (post-apply) output and Clarity's own input --
    // Clarity's own final output overwrites gradedTex in place (see
    // fs_clarity_v's doc comment), so it needs no texture of its own here.
    textureBlurScratchTex?.destroy();
    textureBlurScratchTex = device.createTexture({
      size: [bitmap.width, bitmap.height],
      format: "r32float",
      usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.RENDER_ATTACHMENT,
    });
    textureAdjustedTex?.destroy();
    textureAdjustedTex = device.createTexture({
      size: [bitmap.width, bitmap.height],
      format: "rgba16float",
      usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.RENDER_ATTACHMENT,
    });
    clarityBlurScratchTex?.destroy();
    clarityBlurScratchTex = device.createTexture({
      size: [bitmap.width, bitmap.height],
      format: "r32float",
      usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.RENDER_ATTACHMENT,
    });

    // Sharpening / Noise Reduction (M3): same "recreate whenever source
    // resolution changes" lifecycle as every intermediate above.
    sharpenBlurHTex?.destroy();
    sharpenBlurHTex = device.createTexture({
      size: [bitmap.width, bitmap.height],
      format: "r32float",
      usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.RENDER_ATTACHMENT,
    });
    sharpenBlurTex?.destroy();
    sharpenBlurTex = device.createTexture({
      size: [bitmap.width, bitmap.height],
      format: "r32float",
      usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.RENDER_ATTACHMENT,
    });
    lumaNRBlurHTex?.destroy();
    lumaNRBlurHTex = device.createTexture({
      size: [bitmap.width, bitmap.height],
      format: "r32float",
      usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.RENDER_ATTACHMENT,
    });
    lumaNRBlurTex?.destroy();
    lumaNRBlurTex = device.createTexture({
      size: [bitmap.width, bitmap.height],
      format: "r32float",
      usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.RENDER_ATTACHMENT,
    });
    colorNRBlurHTex?.destroy();
    colorNRBlurHTex = device.createTexture({
      size: [bitmap.width, bitmap.height],
      format: "rgba16float",
      usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.RENDER_ATTACHMENT,
    });
    colorNRBlurTex?.destroy();
    colorNRBlurTex = device.createTexture({
      size: [bitmap.width, bitmap.height],
      format: "rgba16float",
      usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.RENDER_ATTACHMENT,
    });

    // Atmospheric-light reduction chain: an ARRAY of successively-smaller
    // textures (see buildAtmLightChainSizes/fs_atm_reduce's own doc
    // comment), not one texture's mip chain -- simpler to create and bind
    // correctly by hand than juggling createView({baseMipLevel}) at every
    // step, and these are all tiny (the largest is ~1/64th of the source
    // resolution).
    atmLightChain.forEach((tex) => tex.destroy());
    const chainSizes = buildAtmLightChainSizes(bitmap.width, bitmap.height);
    // Captured as a local `const` -- TS can't narrow the outer `device`/
    // `atmReducePipeline` `let`s (reassignable elsewhere in this module)
    // across a closure boundary, even though the top-of-function guard
    // above already ensures both are non-null for this entire call.
    const gpuDevice = device;
    const reducePipeline = atmReducePipeline;
    atmLightChain = chainSizes.map(([w, h]) =>
      gpuDevice.createTexture({
        size: [w, h],
        format: "rgba16float",
        usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.RENDER_ATTACHMENT,
      }),
    );
    const atmLightFinalTex = atmLightChain[atmLightChain.length - 1];

    const sampler = device.createSampler({ magFilter: "linear", minFilter: "linear" });

    gradeBindGroup = gpuDevice.createBindGroup({
      layout: gradePipeline.getBindGroupLayout(0),
      entries: [
        { binding: 0, resource: sampler },
        { binding: 1, resource: sourceTexture.createView() },
        { binding: 2, resource: { buffer: uniformBuffer } },
        { binding: 5, resource: { buffer: curveLutBuffer } },
        { binding: 6, resource: { buffer: hslBandsBuffer } },
        { binding: 7, resource: { buffer: splitToningBuffer } },
      ],
    });

    // One bind group per reduction pass -- `reduceInput` (binding 9) is
    // rebound to a DIFFERENT actual texture each step (gradedTex for the
    // first pass, then each successively-smaller chain texture in turn),
    // the SAME atmReducePipeline object reused for every draw call.
    atmReduceBindGroups = chainSizes.map((_, i) => {
      const input = /** @type {GPUTexture} */ (i === 0 ? gradedTex : atmLightChain[i - 1]);
      return gpuDevice.createBindGroup({
        layout: reducePipeline.getBindGroupLayout(0),
        entries: [{ binding: 9, resource: input.createView() }],
      });
    });

    minChannelBindGroup = gpuDevice.createBindGroup({
      layout: minChannelPipeline.getBindGroupLayout(0),
      entries: [
        { binding: 8, resource: gradedTex.createView() },
        { binding: 10, resource: atmLightFinalTex.createView() },
      ],
    });
    minHBindGroup = gpuDevice.createBindGroup({
      layout: minHPipeline.getBindGroupLayout(0),
      entries: [{ binding: 11, resource: minChannelTex.createView() }],
    });
    minVBindGroup = gpuDevice.createBindGroup({
      layout: minVPipeline.getBindGroupLayout(0),
      entries: [{ binding: 11, resource: darkChannelHTex.createView() }],
    });
    meanHBindGroup = gpuDevice.createBindGroup({
      layout: meanHPipeline.getBindGroupLayout(0),
      entries: [{ binding: 11, resource: tRawTex.createView() }],
    });
    meanVBindGroup = gpuDevice.createBindGroup({
      layout: meanVPipeline.getBindGroupLayout(0),
      entries: [{ binding: 11, resource: transmissionHTex.createView() }],
    });

    // Texture & Clarity (M3): textureHPipeline/textureVPipeline read
    // gradedTex (binding 13, rebound per-op unlike Dehaze's filterInput
    // rebinding pattern -- here each op gets its own bind group instead,
    // since layout:"auto" infers a separate layout per entry point
    // regardless); clarityHPipeline/clarityVPipeline read
    // textureAdjustedTex instead, chaining onto Texture's own output. The
    // V passes also need binding 2 (the Adjustments uniform, for
    // texture_amount/clarity_amount) -- easy to miss since none of
    // Dehaze's own H/V bind groups need it (see the design review that
    // caught this as a real omission before it was ever written).
    textureHBindGroup = gpuDevice.createBindGroup({
      layout: textureHPipeline.getBindGroupLayout(0),
      entries: [{ binding: 13, resource: gradedTex.createView() }],
    });
    textureVBindGroup = gpuDevice.createBindGroup({
      layout: textureVPipeline.getBindGroupLayout(0),
      entries: [
        { binding: 13, resource: gradedTex.createView() },
        { binding: 14, resource: textureBlurScratchTex.createView() },
        { binding: 2, resource: { buffer: uniformBuffer } },
      ],
    });
    clarityHBindGroup = gpuDevice.createBindGroup({
      layout: clarityHPipeline.getBindGroupLayout(0),
      entries: [{ binding: 13, resource: textureAdjustedTex.createView() }],
    });
    clarityVBindGroup = gpuDevice.createBindGroup({
      layout: clarityVPipeline.getBindGroupLayout(0),
      entries: [
        { binding: 13, resource: textureAdjustedTex.createView() },
        { binding: 14, resource: clarityBlurScratchTex.createView() },
        { binding: 2, resource: { buffer: uniformBuffer } },
      ],
    });

    // Sharpening / Noise Reduction (M3): all three H-passes read
    // gradedTex(8) DIRECTLY (never rebound the way Texture/Clarity's own
    // lcRgbInput is) -- they always read the SAME pre-Dehaze-recovery
    // snapshot, so no per-pass rebinding is needed. Sharpen's own H/V
    // passes additionally need binding 19 (sharpenParams) for its
    // uniform-driven radius; Luma/Color NR's radii are fixed WGSL consts,
    // so their own H/V bind groups need no uniform at all.
    sharpenHBindGroup = gpuDevice.createBindGroup({
      layout: sharpenHPipeline.getBindGroupLayout(0),
      entries: [
        { binding: 8, resource: gradedTex.createView() },
        { binding: 19, resource: { buffer: sharpenBuffer } },
      ],
    });
    sharpenVBindGroup = gpuDevice.createBindGroup({
      layout: sharpenVPipeline.getBindGroupLayout(0),
      entries: [
        { binding: 17, resource: sharpenBlurHTex.createView() },
        { binding: 19, resource: { buffer: sharpenBuffer } },
      ],
    });
    lumaNRHBindGroup = gpuDevice.createBindGroup({
      layout: lumaNRHPipeline.getBindGroupLayout(0),
      entries: [{ binding: 8, resource: gradedTex.createView() }],
    });
    lumaNRVBindGroup = gpuDevice.createBindGroup({
      layout: lumaNRVPipeline.getBindGroupLayout(0),
      entries: [{ binding: 17, resource: lumaNRBlurHTex.createView() }],
    });
    colorNRHBindGroup = gpuDevice.createBindGroup({
      layout: colorNRHPipeline.getBindGroupLayout(0),
      entries: [{ binding: 8, resource: gradedTex.createView() }],
    });
    colorNRVBindGroup = gpuDevice.createBindGroup({
      layout: colorNRVPipeline.getBindGroupLayout(0),
      entries: [{ binding: 18, resource: colorNRBlurHTex.createView() }],
    });

    // Final pass's own bind group -- a genuinely DIFFERENT entry set than
    // before (0,2,3,4,8,10,12,15,16), NOT the old single-pass fs_main's
    // {0-7} -- fs_final no longer references srcTexture/curveLut/
    // hslBands/splitToning (those moved into fs_grade), but DOES still
    // need srcSampler(0) -- the mask loop's own pre-existing brushMasks
    // sample uses it, unrelated to Dehaze. binding 16 (grain) is the
    // newest additions here -- double-checked against fs_final's own WGSL
    // body (which reads sharpenParams/lumaNrParams/colorNrParams and
    // samples sharpenBlurFinal/lumaNrBlurFinal/colorNrBlurFinal directly)
    // before adding, given this exact bind group has already missed a
    // real binding THREE times now (srcSampler, then the Adjustments
    // uniform in fs_texture_v/fs_clarity_v, then almost a fourth time for
    // Grain) per this comment's own history -- six new bindings in one
    // slice is exactly the kind of change most likely to repeat it.
    bindGroup = gpuDevice.createBindGroup({
      layout: pipeline.getBindGroupLayout(0),
      entries: [
        // binding 0 (srcSampler) IS still needed here -- fs_final's mask
        // loop (copied unchanged from the old fs_main) samples brushMasks
        // via textureSampleLevel(brushMasks, srcSampler, ...), a real
        // dependency this bind group's entry list missed on the first
        // pass (assumed fs_final needed none of the original 0/1/5/6/7
        // bindings, but the mask loop's OWN pre-existing srcSampler use
        // doesn't go away just because Dehaze was inserted above it).
        { binding: 0, resource: sampler },
        { binding: 2, resource: { buffer: uniformBuffer } },
        { binding: 3, resource: { buffer: masksBuffer } },
        { binding: 4, resource: brushTextureArray.createView({ dimension: "2d-array" }) },
        { binding: 8, resource: gradedTex.createView() },
        { binding: 10, resource: atmLightFinalTex.createView() },
        { binding: 12, resource: transmissionTex.createView() },
        { binding: 15, resource: { buffer: vignetteBuffer } },
        { binding: 16, resource: { buffer: grainBuffer } },
        { binding: 19, resource: { buffer: sharpenBuffer } },
        { binding: 20, resource: { buffer: lumaNRBuffer } },
        { binding: 21, resource: { buffer: colorNRBuffer } },
        { binding: 22, resource: sharpenBlurTex.createView() },
        { binding: 23, resource: lumaNRBlurTex.createView() },
        { binding: 24, resource: colorNRBlurTex.createView() },
      ],
    });
    // Every intermediate above is freshly (re)created for this bitmap --
    // any previously-cached dirty key belonged to a DIFFERENT image/tier's
    // now-destroyed textures, so it must not be trusted to skip
    // recomputing the expensive passes on the next render.
    spatialOpsInputsKey = null;

    if (context.canvas instanceof HTMLCanvasElement) {
      context.canvas.width = bitmap.width;
      context.canvas.height = bitmap.height;
    }
    context.configure({ device, format: presentationFormat, alphaMode: "opaque" });
    // Every remaining bitmap.width/.height read is done -- free it now
    // that both its consumers (the GPU upload and the sample-canvas draw
    // above) are finished with it.
    bitmap.close();
  }

  async function loadImage(/** @type {string} */ path) {
    if (!device || !context || !pipeline || !gradePipeline || !atmReducePipeline || !minChannelPipeline || !minHPipeline || !minVPipeline || !meanHPipeline || !meanVPipeline || !textureHPipeline || !textureVPipeline || !clarityHPipeline || !clarityVPipeline || !sharpenHPipeline || !sharpenVPipeline || !lumaNRHPipeline || !lumaNRVPipeline || !colorNRHPipeline || !colorNRVPipeline || !uniformBuffer || !masksBuffer || !curveLutBuffer || !hslBandsBuffer || !splitToningBuffer || !vignetteBuffer || !grainBuffer || !sharpenBuffer || !lumaNRBuffer || !colorNRBuffer) return;
    status = "loading";
    errorMessage = "";
    // A genuinely new image -- any 1:1 tier state belonged to whatever was
    // loaded before and is meaningless here.
    fullTierPath = null;
    fullTierPromise = null;
    activeTier = "draft";

    const preview = await getDevelopPreview(path);
    const response = await fetch(convertFileSrc(preview.path));
    const bitmap = await createImageBitmap(await response.blob());
    await applyBitmapToGpu(bitmap);

    status = "ready";
    await tick(); // overlayEl only mounts once status flips to "ready"
    syncOverlayPosition();
    writeAdjustmentsAndRender();
  }

  /** Lazily fetches and swaps in the native-resolution 1:1 tier for the
   * CURRENTLY loaded image, the first time it's actually zoomed to 100%
   * (triggered by the $effect below, not called directly from pointer
   * handlers) -- matches real Lightroom's own lazy 1:1-preview-build
   * behavior, and this request's own trigger ("when the image zooms in").
   * Deliberately does NOT touch `status`: the draft-resolution image stays
   * visible and interactive for the whole time this decodes in the
   * background (a progressive upgrade, not a blank "Decoding..." reload) --
   * a full-native-resolution RAW decode is genuinely multi-second-capable
   * on a large sensor, unverified at interactive latency in this
   * environment, which is exactly why this must not block the UI. */
  async function upgradeToFullTier(/** @type {string} */ path) {
    if (fullTierPath === path && activeTier === "full") return;
    if (fullTierPromise) {
      await fullTierPromise;
      return;
    }
    fullTierPromise = (async () => {
      const preview = await getDevelopFullPreview(path);
      // The user may have switched images or zoomed back out while this
      // was in flight -- only apply if still relevant, otherwise this
      // would silently stomp whatever loadImage/a later upgrade already
      // put in place.
      if (imagePath !== path || zoomMode !== "100" || !device) return;
      const response = await fetch(convertFileSrc(preview.path));
      const bitmap = await createImageBitmap(await response.blob());
      if (imagePath !== path || zoomMode !== "100") return; // re-check post-decode too
      await applyBitmapToGpu(bitmap);
      fullTierPath = path;
      activeTier = "full";
      // Re-center on the same normalized focus point now that the
      // canvas's native size has just changed out from under any earlier
      // scroll position -- see lastZoomFocus's own doc comment.
      if (wrapEl && canvasEl) {
        wrapEl.scrollLeft = lastZoomFocus.x * canvasEl.width - wrapEl.clientWidth / 2;
        wrapEl.scrollTop = lastZoomFocus.y * canvasEl.height - wrapEl.clientHeight / 2;
      }
      writeAdjustmentsAndRender();
    })();
    try {
      await fullTierPromise;
    } finally {
      fullTierPromise = null;
    }
  }

  /** Draws ONE dab onto a persistent per-mask OffscreenCanvas, white-on-
   * black (luminance-as-weight), matching develop_engine.rs's
   * `dab_falloff`/`brush_mask_weight` exactly so the GPU preview and CPU
   * export agree: "add" dabs use `"lighter"` compositing (additive,
   * clamped at full white -- matches the CPU side's
   * `(weight + falloff).min(1.0)`); "erase" dabs use `"multiply"`
   * compositing with a gradient from `(1-flow)` at the dab's center to
   * `1.0` at its edge (matches the CPU side's `weight *= 1.0 - falloff`).
   * `radius`/`hardness`/`flow` are baked into the dab itself at paint time
   * (the current brush tool settings when it was placed), not read from
   * live props here. */
  function rasterizeDab(
    /** @type {OffscreenCanvasRenderingContext2D} */ ctx,
    /** @type {number} */ canvasWidth,
    /** @type {number} */ canvasHeight,
    /** @type {import('$lib/api/develop.js').Dab} */ dab,
  ) {
    const cx = dab.x * canvasWidth;
    const cy = dab.y * canvasHeight;
    // radius is a fraction of WIDTH only -- ctx.arc()'s single radius
    // parameter then produces a true circle in this canvas's own native
    // pixel space regardless of the image's aspect ratio, unlike the
    // radial mask's separate radiusX/radiusY (needed there because that
    // geometry is evaluated analytically in normalized UV space, where
    // width/height asymmetry genuinely matters).
    const r = dab.radius * canvasWidth;
    const hardStop = Math.min(Math.max(dab.hardness / 100, 0), 1);
    const flow = Math.min(Math.max(dab.flow, 0), 1);
    // At least a 0.5px gap between the inner (hardness) stop and the outer
    // edge -- avoids a degenerate r0===r1 radial gradient (unreliable
    // across Canvas2D implementations) when hardness is at/near 100.
    const outerR = Math.max(r, 0.5);
    const innerR = Math.min(hardStop * outerR, outerR - 0.5);

    ctx.beginPath();
    ctx.arc(cx, cy, outerR, 0, Math.PI * 2);
    if (dab.mode === "erase") {
      ctx.globalCompositeOperation = "multiply";
      const floor = Math.round((1 - flow) * 255);
      const gradient = ctx.createRadialGradient(cx, cy, innerR, cx, cy, outerR);
      gradient.addColorStop(0, `rgb(${floor},${floor},${floor})`);
      gradient.addColorStop(1, "rgb(255,255,255)");
      ctx.fillStyle = gradient;
    } else {
      ctx.globalCompositeOperation = "lighter";
      const peak = Math.round(flow * 255);
      const gradient = ctx.createRadialGradient(cx, cy, innerR, cx, cy, outerR);
      gradient.addColorStop(0, `rgb(${peak},${peak},${peak})`);
      gradient.addColorStop(1, "rgb(0,0,0)");
      ctx.fillStyle = gradient;
    }
    ctx.fill();
  }

  /** Ensures every brush mask in `masks` has a rasterized texture-array
   * layer, drawing only newly-added dabs onto each mask's own persistent
   * OffscreenCanvas -- never re-rasterizing dabs already drawn, which is
   * what keeps a long stroke's per-move cost O(1) (bound by texture
   * resolution/upload cost, not stroke length). Releases layers for brush
   * masks no longer present (deleted). Called at the top of
   * writeAdjustmentsAndRender, so it runs both on every mask-list change
   * and once per freshly loaded image (loadImage's initial call re-
   * rasterizes any brush masks already in that image's saved edit stack,
   * since a canvas sized for a DIFFERENT image's resolution is meaningless
   * here -- loadImage resets brushRasterState/freeBrushLayers before this
   * runs). */
  function syncBrushRasterization() {
    if (!device || !brushTextureArray) return;
    const presentIds = new Set();
    for (const mask of masks) {
      if (mask.op !== "brush_mask") continue;
      presentIds.add(mask.id);
      let entry = brushRasterState.get(mask.id);
      if (!entry) {
        const layer = freeBrushLayers.shift();
        // Combined MAX_MASKS budget exhausted -- MaskToolStrip's atCap
        // check already prevents creating a mask that would hit this, so
        // this is a defensive no-op, not an expected path.
        if (layer === undefined) continue;
        const canvas = new OffscreenCanvas(brushTextureArray.width, brushTextureArray.height);
        const ctx = /** @type {OffscreenCanvasRenderingContext2D} */ (canvas.getContext("2d"));
        // Opaque black init (NOT the canvas's default transparent) --
        // required for "multiply" erase compositing to correctly no-op
        // over never-painted areas. Against a transparent destination,
        // Porter-Duff "multiply" lets the erase gradient's own color show
        // through directly (since there's no destination alpha to
        // constrain it), which would incorrectly paint weight into
        // untouched regions. Against opaque black (alpha=1, color=0),
        // multiply always yields black regardless of the erase color, so
        // erasing over nothing stays nothing.
        ctx.fillStyle = "black";
        ctx.fillRect(0, 0, canvas.width, canvas.height);
        entry = { canvas, ctx, layer, dabsDrawn: 0 };
        brushRasterState.set(mask.id, entry);
      }
      const dabs = /** @type {any} */ (mask).dabs;
      if (dabs.length < entry.dabsDrawn) {
        // Dab list shrank -- not expected in this design (dabs only ever
        // get appended), but handled defensively rather than leaving
        // stale strokes visible.
        entry.ctx.fillStyle = "black";
        entry.ctx.fillRect(0, 0, entry.canvas.width, entry.canvas.height);
        entry.dabsDrawn = 0;
      }
      for (let i = entry.dabsDrawn; i < dabs.length; i++) {
        rasterizeDab(entry.ctx, entry.canvas.width, entry.canvas.height, dabs[i]);
      }
      if (dabs.length !== entry.dabsDrawn) {
        entry.dabsDrawn = dabs.length;
        const imageData = entry.ctx.getImageData(0, 0, entry.canvas.width, entry.canvas.height);
        device.queue.writeTexture(
          { texture: brushTextureArray, origin: { x: 0, y: 0, z: entry.layer } },
          imageData.data,
          { bytesPerRow: entry.canvas.width * 4, rowsPerImage: entry.canvas.height },
          { width: entry.canvas.width, height: entry.canvas.height },
        );
      }
    }
    for (const [id, entry] of brushRasterState) {
      if (!presentIds.has(id)) {
        freeBrushLayers.push(entry.layer);
        brushRasterState.delete(id);
      }
    }
  }

  function writeAdjustmentsAndRender() {
    if (!device || !context || !pipeline || !bindGroup || !gradePipeline || !gradeBindGroup || !atmReducePipeline || atmReduceBindGroups.length === 0 || !minChannelPipeline || !minChannelBindGroup || !minHPipeline || !minHBindGroup || !minVPipeline || !minVBindGroup || !meanHPipeline || !meanHBindGroup || !meanVPipeline || !meanVBindGroup || !textureHPipeline || !textureHBindGroup || !textureVPipeline || !textureVBindGroup || !clarityHPipeline || !clarityHBindGroup || !clarityVPipeline || !clarityVBindGroup || !sharpenHPipeline || !sharpenHBindGroup || !sharpenVPipeline || !sharpenVBindGroup || !lumaNRHPipeline || !lumaNRHBindGroup || !lumaNRVPipeline || !lumaNRVBindGroup || !colorNRHPipeline || !colorNRHBindGroup || !colorNRVPipeline || !colorNRVBindGroup || !gradedTex || !minChannelTex || !darkChannelHTex || !tRawTex || !transmissionHTex || !transmissionTex || !textureBlurScratchTex || !textureAdjustedTex || !clarityBlurScratchTex || !sharpenBlurHTex || !sharpenBlurTex || !lumaNRBlurHTex || !lumaNRBlurTex || !colorNRBlurHTex || !colorNRBlurTex || atmLightChain.length === 0 || !uniformBuffer || !masksBuffer || !curveLutBuffer || !hslBandsBuffer || !splitToningBuffer || !vignetteBuffer || !grainBuffer || !sharpenBuffer || !lumaNRBuffer || !colorNRBuffer) return;
    syncBrushRasterization();

    // Tone curve: rebuilt from the current control points and rewritten
    // every render, same "cheap enough to just always redo" treatment as
    // the uniform/mask buffers below -- no dirty-tracking needed given
    // buildToneCurveLut's own cost (a handful of points, 256 samples).
    device.queue.writeBuffer(curveLutBuffer, 0, buildToneCurveLut(toneCurvePoints));
    device.queue.writeBuffer(hslBandsBuffer, 0, buildHslUniformData(hslBands));
    device.queue.writeBuffer(splitToningBuffer, 0, buildSplitToningUniformData(splitToning));
    device.queue.writeBuffer(vignetteBuffer, 0, buildVignetteUniformData(vignette));
    device.queue.writeBuffer(grainBuffer, 0, buildGrainUniformData(grain));
    device.queue.writeBuffer(sharpenBuffer, 0, buildSharpenUniformData(sharpen));
    device.queue.writeBuffer(lumaNRBuffer, 0, buildLumaNrUniformData(lumaNR));
    device.queue.writeBuffer(colorNRBuffer, 0, buildColorNrUniformData(colorNR));

    // Mask overlay: -1 (disabled) unless the toggle is on AND the current
    // selection exists in `masks` -- `findIndex`'s own -1 miss-sentinel
    // *is* the disabled state, so no separate per-kind lookup is needed
    // here at all (the shader itself gates which kinds actually show the
    // overlay; see the kind > 1.5 check in the mask loop below).
    const selectedMaskIndex = showMaskOverlay ? masks.findIndex((m) => m.id === selectedMaskId) : -1;

    device.queue.writeBuffer(
      uniformBuffer,
      0,
      new Float32Array([exposure, contrast, saturation, masks.length, selectedMaskIndex, dehaze, texture, clarity]),
    );

    const maskData = new Float32Array(MAX_MASKS * 12);
    masks.slice(0, MAX_MASKS).forEach((/** @type {any} */ m, /** @type {number} */ i) => {
      const o = i * 12;
      if (m.op === "radial_gradient_mask") {
        maskData[o + 0] = m.center.x;
        maskData[o + 1] = m.center.y;
        maskData[o + 2] = m.radiusX;
        maskData[o + 3] = m.radiusY;
        maskData[o + 4] = m.feather;
        maskData[o + 6] = 1; // kind = radial
      } else if (m.op === "brush_mask") {
        maskData[o + 6] = 2; // kind = brush
        maskData[o + 7] = brushRasterState.get(m.id)?.layer ?? 0;
      } else if (m.op === "luminance_range_mask") {
        maskData[o + 0] = m.rangeMin;
        maskData[o + 1] = m.rangeMax;
        maskData[o + 4] = m.feather;
        maskData[o + 6] = 3; // kind = luminance range
      } else if (m.op === "color_range_mask") {
        maskData[o + 0] = m.refColor.r;
        maskData[o + 1] = m.refColor.g;
        maskData[o + 2] = m.refColor.b;
        maskData[o + 3] = m.range;
        maskData[o + 4] = m.feather;
        maskData[o + 6] = 4; // kind = color range
      } else {
        // linear_gradient_mask -- the only kind left once the four
        // explicit branches above are exhausted, given MASK_OP_NAMES
        // already gates what can appear in `masks` at all (develop.js).
        // A real bug once lived here (before luminance range existed):
        // an unconditional catch-all `else` assumed "anything that isn't
        // radial or brush is linear" -- a mask object of a kind with no
        // .start/.end would have thrown on m.start.x, aborting the render
        // for every mask in the stack the instant one existed anywhere.
        // Every new kind since (luminance range, color range) has gotten
        // its own explicit branch above this fallback for exactly that
        // reason.
        maskData[o + 0] = m.start.x;
        maskData[o + 1] = m.start.y;
        maskData[o + 2] = m.end.x;
        maskData[o + 3] = m.end.y;
        maskData[o + 4] = m.feather;
        maskData[o + 6] = 0; // kind = linear
      }
      maskData[o + 5] = m.invert ? 1 : 0;
      maskData[o + 8] = m.exposure;
      maskData[o + 9] = m.contrast;
      maskData[o + 10] = m.saturation;
    });
    device.queue.writeBuffer(masksBuffer, 0, maskData);

    const encoder = device.createCommandEncoder();

    // Texture/Clarity/Dehaze/Sharpen/NR: the local-contrast, dark-channel/
    // atmospheric-light/transmission, and sharpen/NR blur passes depend on
    // {exposure, contrast, saturation, toneCurvePoints, hslBands,
    // splitToning, texture, clarity, sharpenRadius} -- NOT masks/
    // selectedMaskId/showMaskOverlay, which only ever affect the cheap
    // final pass below, and NOT dehaze/sharpen's-own-amount/lumaNR/colorNR
    // (only fs_final's own cheap blend reads those; none of the BLUR
    // CONTENT computed in this block depends on them). texture/clarity DO
    // belong in this key, unlike dehaze -- fs_texture_v/fs_clarity_v write
    // their result INTO gradedTex itself, inside this block, so a
    // texture/clarity-only change must still invalidate the cache.
    // `sharpenRadius` belongs here for the SAME reason but a DIFFERENT
    // mechanism: unlike dehaze_amount/lumaNR/colorNR's amount-only
    // sliders, Sharpening's Radius controls the blur KERNEL SIZE itself
    // (fs_sharpen_h/fs_sharpen_v's own loop bound) -- omitting it here was
    // a real bug this slice's own design review caught before it ever
    // shipped: dragging Radius alone would silently show a stale blur
    // until some UNRELATED slider happened to invalidate the block.
    // Luminance/Color NR need nothing added -- both use FIXED radii, so
    // their blur CONTENT never changes regardless of amount/detail/
    // contrast, the same reasoning that already excludes dehaze_amount.
    // Without this whole cache, an unthrottled mask-handle drag
    // (handlePointerMove calling onMaskUpdated on every pointermove) would
    // retrigger this ~19-pass chain every single frame. A VALUE-based key,
    // not reference equality -- see spatialOpsInputsKey's own doc comment
    // for why masks/toneCurvePoints/hslBands/splitToning being freshly
    // rebuilt via $derived on every editStack change (regardless of which
    // op changed) makes a reference check always report "changed,"
    // silently defeating this cache. `spatialOpsInputsKey === null`
    // (nothing cached yet, e.g. the very first render, or right after a
    // fresh applyBitmapToGpu) is always treated as dirty.
    const spatialOpsKey = JSON.stringify({
      exposure, contrast, saturation, toneCurvePoints, hslBands, splitToning, texture, clarity,
      sharpenRadius: sharpen.radius,
    });
    if (spatialOpsKey !== spatialOpsInputsKey) {
      spatialOpsInputsKey = spatialOpsKey;
      runFullscreenPass(encoder, gradePipeline, gradeBindGroup, gradedTex.createView());
      runFullscreenPass(encoder, textureHPipeline, textureHBindGroup, textureBlurScratchTex.createView());
      runFullscreenPass(encoder, textureVPipeline, textureVBindGroup, textureAdjustedTex.createView());
      runFullscreenPass(encoder, clarityHPipeline, clarityHBindGroup, clarityBlurScratchTex.createView());
      // Overwrites gradedTex in place -- see fs_clarity_v's own doc
      // comment for why this is sound (sequential pass execution within
      // one command encoder) and why no third "final graded" texture is
      // needed.
      runFullscreenPass(encoder, clarityVPipeline, clarityVBindGroup, gradedTex.createView());
      // Sharpening / Noise Reduction: all three read gradedTex in this
      // SAME post-Texture/Clarity, pre-Dehaze-recovery state -- see
      // develop_engine.rs's own doc comment on the blur-source
      // precomputation for the named, accepted limitation this implies.
      // Order among these three (and relative to the atm-reduce chain
      // below) doesn't matter -- all read the same stable gradedTex
      // snapshot with no interdependency between them.
      runFullscreenPass(encoder, sharpenHPipeline, sharpenHBindGroup, sharpenBlurHTex.createView());
      runFullscreenPass(encoder, sharpenVPipeline, sharpenVBindGroup, sharpenBlurTex.createView());
      runFullscreenPass(encoder, lumaNRHPipeline, lumaNRHBindGroup, lumaNRBlurHTex.createView());
      runFullscreenPass(encoder, lumaNRVPipeline, lumaNRVBindGroup, lumaNRBlurTex.createView());
      runFullscreenPass(encoder, colorNRHPipeline, colorNRHBindGroup, colorNRBlurHTex.createView());
      runFullscreenPass(encoder, colorNRVPipeline, colorNRVBindGroup, colorNRBlurTex.createView());
      // Captured as a local `const` for the same reason applyBitmapToGpu's
      // own gpuDevice/reducePipeline aliases are -- TS can't narrow a
      // reassignable outer `let` across a closure boundary.
      const reducePipeline = atmReducePipeline;
      atmReduceBindGroups.forEach((bg, i) => {
        runFullscreenPass(encoder, reducePipeline, bg, atmLightChain[i].createView());
      });
      runFullscreenPass(encoder, minChannelPipeline, minChannelBindGroup, minChannelTex.createView());
      runFullscreenPass(encoder, minHPipeline, minHBindGroup, darkChannelHTex.createView());
      runFullscreenPass(encoder, minVPipeline, minVBindGroup, tRawTex.createView());
      runFullscreenPass(encoder, meanHPipeline, meanHBindGroup, transmissionHTex.createView());
      runFullscreenPass(encoder, meanVPipeline, meanVBindGroup, transmissionTex.createView());
    }

    runFullscreenPass(encoder, pipeline, bindGroup, context.getCurrentTexture().createView());
    device.queue.submit([encoder.finish()]);
  }

  $effect(() => {
    const path = imagePath;
    const canvas = canvasEl;
    if (!path || !canvas) return;
    zoomMode = "fit";

    (async () => {
      try {
        if (!device) await initGpu(canvas);
        await loadImage(path);
      } catch (/** @type {any} */ e) {
        status = "error";
        errorMessage = String(e && e.stack ? e.stack : e);
      }
    })();
  });

  $effect(() => {
    // Re-run whenever an adjustment or the mask list changes -- reads, not
    // a re-fetch. selectedMaskId/showMaskOverlay are included specifically
    // for the mask overlay: selecting a DIFFERENT mask, or toggling the
    // overlay, needs a re-render even when nothing else about the image or
    // its masks has changed.
    void exposure;
    void contrast;
    void saturation;
    void masks;
    void selectedMaskId;
    void showMaskOverlay;
    void toneCurvePoints;
    void hslBands;
    void splitToning;
    void dehaze;
    void texture;
    void clarity;
    void vignette;
    void grain;
    void sharpen;
    void lumaNR;
    void colorNR;
    if (status === "ready") writeAdjustmentsAndRender();
  });

  // 1:1 tier trigger -- fires for BOTH ways zoomMode can flip to "100"
  // (the canvas click-to-zoom in handlePointerUp, and the zoom-badge
  // button's onclick both just set zoomMode directly), so neither call
  // site needs to know about the tier upgrade at all. Guarded on
  // activeTier so it's a no-op once already upgraded for this image, and
  // on status==="ready" so it can't fire before loadImage has finished
  // its own initial setup.
  $effect(() => {
    if (status === "ready" && zoomMode === "100" && activeTier !== "full") {
      upgradeToFullTier(imagePath);
    }
  });
</script>

<div class="canvas-wrap" class:zoomed={zoomMode === "100"} bind:this={wrapEl}>
  <canvas
    bind:this={canvasEl}
    class:zoomed={zoomMode === "100"}
    class:placing={activeTool === "linear_gradient" || activeTool === "radial_gradient" || activeTool === "brush" || activeTool === "color_range" || activeTool === "eyedropper"}
    onpointerdown={handlePointerDown}
    onpointermove={handlePointerMove}
    onpointerup={handlePointerUp}
    onpointerleave={() => (brushCursor = null)}
  ></canvas>
  {#if status === "ready"}
    <!-- M3 Slice 5: a sibling of canvas, NOT a child of a sizing wrapper
         (see the fix note near syncOverlayPosition) -- its left/top/width/
         height are set directly in JS from canvas's own (correctly
         intrinsic-sized) rendered box via ResizeObserver, then mask
         geometry is positioned with CSS percentages relative to THIS box.
         pointer-events:none on the overlay itself so it never swallows
         pan/zoom/placement drags meant for the canvas beneath it -- only
         the individual handle buttons opt back in. -->
    <div class="mask-overlay" bind:this={overlayEl}>
      {#each masks as mask (mask.id)}
        {#if mask.op === "linear_gradient_mask"}
          {@const fl = linearFeatherLines(mask)}
          <svg class="mask-line" class:selected={mask.id === selectedMaskId}>
            <line x1="{mask.start.x * 100}%" y1="{mask.start.y * 100}%" x2="{mask.end.x * 100}%" y2="{mask.end.y * 100}%" />
          </svg>
          {#if fl}
            <!-- Feather range indicators: two lines perpendicular to the
                 gradient axis at the weight=0/weight=1 boundaries, only
                 when feather > 0 -- purely additive/informational, does
                 NOT change the existing draggable axis line/handles below.
                 Each independently omitted (clipLineToUnitBox returns
                 null) if that particular boundary falls off-frame. -->
            <svg class="mask-feather-line">
              {#if fl.zero}
                <line x1="{fl.zero.x1 * 100}%" y1="{fl.zero.y1 * 100}%" x2="{fl.zero.x2 * 100}%" y2="{fl.zero.y2 * 100}%" />
              {/if}
              {#if fl.one}
                <line x1="{fl.one.x1 * 100}%" y1="{fl.one.y1 * 100}%" x2="{fl.one.x2 * 100}%" y2="{fl.one.y2 * 100}%" />
              {/if}
            </svg>
          {/if}
          <button
            class="mask-handle"
            class:selected={mask.id === selectedMaskId}
            style="left:{mask.start.x * 100}%; top:{mask.start.y * 100}%"
            aria-label="Gradient start"
            onpointerdown={(e) => handleMaskHandlePointerDown(e, mask.id, "start")}
            onpointermove={handleMaskHandlePointerMove}
            onpointerup={handleMaskHandlePointerUp}
          ></button>
          <button
            class="mask-handle"
            class:selected={mask.id === selectedMaskId}
            style="left:{mask.end.x * 100}%; top:{mask.end.y * 100}%"
            aria-label="Gradient end"
            onpointerdown={(e) => handleMaskHandlePointerDown(e, mask.id, "end")}
            onpointermove={handleMaskHandlePointerMove}
            onpointerup={handleMaskHandlePointerUp}
          ></button>
        {:else if mask.op === "radial_gradient_mask"}
          <!-- M3 Slice 6: cx/rx resolve against the SVG viewport's width,
               cy/ry against height -- an <ellipse>-specific percentage
               behavior (unlike <circle>'s r, which resolves against the
               diagonal), confirmed by design review before implementation
               to correctly match this component's normalized (x=width-
               fraction, y=height-fraction) coordinate convention. -->
          {@const fr = radialFeatherRadii(mask)}
          <svg class="mask-ellipse" class:selected={mask.id === selectedMaskId}>
            {#if fr}
              <!-- Feather range indicators: inner (fully-inside boundary)
                   + outer (fully-outside boundary) ellipses, only when
                   feather > 0. The raw radiusX/radiusY (still what the
                   radius handle below edits) now sits exactly halfway
                   between them. feather=0 deliberately keeps the single-
                   ellipse rendering rather than drawing two coincident
                   shapes, which would alpha-composite to a visibly
                   heavier line than one. -->
              <ellipse cx="{mask.center.x * 100}%" cy="{mask.center.y * 100}%" rx="{fr.inner.rx * 100}%" ry="{fr.inner.ry * 100}%" />
              <ellipse cx="{mask.center.x * 100}%" cy="{mask.center.y * 100}%" rx="{fr.outer.rx * 100}%" ry="{fr.outer.ry * 100}%" />
            {:else}
              <ellipse cx="{mask.center.x * 100}%" cy="{mask.center.y * 100}%" rx="{mask.radiusX * 100}%" ry="{mask.radiusY * 100}%" />
            {/if}
          </svg>
          <button
            class="mask-handle"
            class:selected={mask.id === selectedMaskId}
            style="left:{mask.center.x * 100}%; top:{mask.center.y * 100}%"
            aria-label="Radial center"
            onpointerdown={(e) => handleMaskHandlePointerDown(e, mask.id, "center")}
            onpointermove={handleMaskHandlePointerMove}
            onpointerup={handleMaskHandlePointerUp}
          ></button>
          <button
            class="mask-handle"
            class:selected={mask.id === selectedMaskId}
            style="left:{(mask.center.x + mask.radiusX) * 100}%; top:{mask.center.y * 100}%"
            aria-label="Radial radius"
            onpointerdown={(e) => handleMaskHandlePointerDown(e, mask.id, "radius", mask.center)}
            onpointermove={handleMaskHandlePointerMove}
            onpointerup={handleMaskHandlePointerUp}
          ></button>
        {/if}
      {/each}
      {#if placingMask?.kind === "linear_gradient"}
        <svg class="mask-line placing">
          <line x1="{placingMask.start.x * 100}%" y1="{placingMask.start.y * 100}%" x2="{placingMask.end.x * 100}%" y2="{placingMask.end.y * 100}%" />
        </svg>
      {:else if placingMask?.kind === "radial_gradient"}
        <svg class="mask-ellipse placing">
          <ellipse cx="{placingMask.center.x * 100}%" cy="{placingMask.center.y * 100}%" rx="{placingMask.radiusX * 100}%" ry="{placingMask.radiusY * 100}%" />
        </svg>
      {/if}
      {#if activeTool === "brush" && brushCursor}
        <!-- M3 Slice 7: live brush-size cursor, shown on hover (not just
             while painting) so size is visible before committing a
             stroke. Red when erasing, matching this app's existing
             label-red convention for destructive/removal affordances. -->
        <svg class="brush-cursor" class:erasing={eraseMode}>
          <ellipse cx="{brushCursor.x * 100}%" cy="{brushCursor.y * 100}%" rx="{brushSize * 100}%" ry="{brushCursorRyPercent()}%" />
        </svg>
      {/if}
    </div>
  {/if}
  {#if status === "ready"}
    <button
      class="zoom-badge"
      type="button"
      title={zoomMode === "fit" ? "Click image for 100%" : "Click image to fit"}
      onclick={() => (zoomMode = zoomMode === "fit" ? "100" : "fit")}
    >{zoomMode === "fit" ? "Fit" : "100%"}</button>
  {/if}
  {#if status === "loading"}
    <div class="overlay">Decoding…</div>
  {:else if status === "error"}
    <div class="overlay error">{errorMessage}</div>
  {/if}
</div>

<style>
  .canvas-wrap {
    flex: 1;
    display: flex;
    position: relative;
    padding: 22px;
    min-width: 0;
    min-height: 0;
    user-select: none;
  }
  /* M3 Slice 3: overflow:auto only in "100" mode -- centering the frame
     via `margin: auto` (not align-items/justify-content on this flex
     container) is what avoids a real "scroll trap": centering an
     OVERFLOWING flex item via align-items/justify-content computes a
     negative starting scroll offset that clamps to 0, permanently hiding
     the "before center" portion of the image. Auto margins on the flex
     item itself absorb free space when it fits and resolve to 0 when it
     overflows -- one rule handles both modes correctly. */
  .canvas-wrap.zoomed {
    overflow: auto;
  }
  canvas {
    max-width: 100%;
    max-height: 100%;
    margin: auto;
    border-radius: 2px;
    box-shadow: 0 20px 50px -14px rgba(0, 0, 0, 0.7);
    cursor: zoom-in;
    touch-action: none;
  }
  canvas.zoomed {
    max-width: none;
    max-height: none;
    cursor: grab;
  }
  canvas.placing {
    cursor: crosshair;
  }
  .mask-overlay {
    position: absolute;
    pointer-events: none;
  }
  .mask-line {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    overflow: visible;
  }
  .mask-line line {
    stroke: rgba(255, 255, 255, 0.55);
    stroke-width: 1.5;
    stroke-dasharray: 5 4;
  }
  .mask-line.selected line {
    stroke: var(--accent-strong);
    stroke-width: 2;
  }
  .mask-line.placing line {
    stroke: var(--accent-strong);
  }
  .mask-feather-line {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    overflow: visible;
  }
  .mask-feather-line line {
    stroke: rgba(255, 255, 255, 0.3);
    stroke-width: 1;
    stroke-dasharray: 2 4;
  }
  .mask-ellipse {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    overflow: visible;
  }
  .mask-ellipse ellipse {
    fill: none;
    stroke: rgba(255, 255, 255, 0.55);
    stroke-width: 1.5;
    stroke-dasharray: 5 4;
  }
  .mask-ellipse.selected ellipse {
    stroke: var(--accent-strong);
    stroke-width: 2;
  }
  .mask-ellipse.placing ellipse {
    stroke: var(--accent-strong);
  }
  .brush-cursor {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    overflow: visible;
  }
  .brush-cursor ellipse {
    fill: none;
    stroke: rgba(255, 255, 255, 0.75);
    stroke-width: 1.5;
  }
  .brush-cursor.erasing ellipse {
    stroke: var(--label-red);
  }
  .mask-handle {
    all: unset;
    position: absolute;
    width: 12px;
    height: 12px;
    margin-left: -6px;
    margin-top: -6px;
    border-radius: 50%;
    background: rgba(255, 255, 255, 0.85);
    border: 1.5px solid rgba(0, 0, 0, 0.5);
    cursor: grab;
    pointer-events: auto;
  }
  .mask-handle.selected {
    background: var(--accent-strong);
    border-color: var(--bg-panel);
  }
  .zoom-badge {
    all: unset;
    position: absolute;
    right: 30px;
    bottom: 30px;
    padding: 4px 9px;
    font-family: var(--font-mono);
    font-size: 10.5px;
    letter-spacing: 0.03em;
    color: var(--text-secondary);
    background: rgba(20, 18, 16, 0.7);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-s);
    cursor: pointer;
    z-index: 1;
  }
  .zoom-badge:hover {
    color: var(--text-primary);
  }
  .overlay {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-tertiary);
    font-family: var(--font-mono);
    font-size: 12px;
    text-align: center;
    padding: 24px;
    background: rgba(20, 18, 16, 0.6);
  }
  .overlay.error {
    color: var(--label-red);
    white-space: pre-wrap;
  }
</style>
