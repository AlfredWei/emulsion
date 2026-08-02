<script>
  import { tick } from "svelte";
  import { convertFileSrc } from "@tauri-apps/api/core";
  import { getDevelopPreview } from "$lib/api/develop.js";

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
   *   onMaskCreated: (placement:
   *     | { kind: "linear_gradient", start: {x: number, y: number}, end: {x: number, y: number} }
   *     | { kind: "radial_gradient", center: {x: number, y: number}, radiusX: number, radiusY: number }
   *     | { kind: "brush", id: string }
   *   ) => void,
   *   onMaskUpdated: (id: string, patch: Partial<import('$lib/api/develop.js').Mask>) => void,
   *   onMaskSelected: (id: string) => void,
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
    onMaskCreated,
    onMaskUpdated,
    onMaskSelected,
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
  /** @type {GPUBindGroup | null} */
  let bindGroup = null;
  /** @type {GPUTextureFormat} */
  let presentationFormat = "bgra8unorm";

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

    struct Adjustments {
      exposure_ev: f32,
      contrast: f32,
      saturation: f32,
      mask_count: f32,
    };

    // Packed entirely into vec4-multiples (48 bytes/mask) to sidestep
    // WGSL's vec2/vec3-in-array uniform alignment footguns -- array stride
    // in the uniform address space must be a multiple of 16 bytes, and an
    // all-vec4 struct is trivially aligned with no implicit padding.
    // M3 Slice 7: params.w, previously unused, now holds the brush mask's
    // own texture-array layer index (as a float, cast to i32 at sample
    // time) -- brush is the only kind that needs a fourth scalar; linear
    // and radial masks leave it at 0.
    struct Mask {
      start_end: vec4<f32>,   // xy = start, zw = end (normalized image space)
      params: vec4<f32>,      // x = feather 0-100 (unused for brush), y = invert 0/1, z = kind (0=linear, 1=radial, 2=brush), w = brush texture-array layer
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

    fn apply_adjustments(rgb: vec3<f32>, exposure_ev: f32, contrast: f32, saturation: f32) -> vec3<f32> {
      var c = rgb * pow(2.0, exposure_ev);
      c = (c - 0.5) * (1.0 + contrast / 100.0) + 0.5;
      let luma = dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
      c = luma + (c - luma) * (1.0 + saturation / 100.0);
      return c;
    }

    @fragment
    fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
      var rgb = textureSample(srcTexture, srcSampler, in.uv).rgb;
      rgb = apply_adjustments(rgb, adj.exposure_ev, adj.contrast, adj.saturation);

      // Local adjustments layer on top of the globally-graded image,
      // matching real Lightroom's own ordering and develop_engine.rs's.
      let mask_count = i32(adj.mask_count);
      for (var i = 0; i < mask_count; i = i + 1) {
        let m = masks[i];
        var weight: f32;
        if (m.params.z > 1.5) {
          // Brush (M3 Slice 7): rasterized CPU-side into this mask's own
          // texture-array layer, luminance-as-weight (see
          // DevelopCanvas.svelte's rasterizeDab/syncBrushRasterization and
          // develop_engine.rs's dab_falloff/brush_mask_weight for the
          // exact accumulation formula both renderers agree on).
          let layer = i32(m.params.w);
          weight = textureSampleLevel(brushMasks, srcSampler, in.uv, layer, 0.0).r;
          if (m.params.y > 0.5) { weight = 1.0 - weight; }
        } else if (m.params.z < 0.5) {
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
        } else {
          // Radial (M3 Slice 6): start_end.xy = center, start_end.zw =
          // (radiusX, radiusY). d is 0 at center, 1 at the ellipse
          // boundary. At feather=0 the transition band is d in
          // [0.999, 1.0] (width 0.001, sitting just inside the boundary,
          // not symmetric around it); widens to roughly d in [0.001,1.999]
          // as feather approaches 100. insideWeight is ~1 at/near the
          // center regardless of feather.
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
        }
        rgb = mix(rgb, apply_adjustments(rgb, m.adjustments.x, m.adjustments.y, m.adjustments.z), weight);
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

    context = canvas.getContext("webgpu");
    if (!context) throw new Error("canvas.getContext('webgpu') returned null");
    context.configure({ device, format: presentationFormat, alphaMode: "opaque" });

    const module = device.createShaderModule({ code: WGSL });
    pipeline = device.createRenderPipeline({
      layout: "auto",
      vertex: { module, entryPoint: "vs_main" },
      fragment: { module, entryPoint: "fs_main", targets: [{ format: presentationFormat }] },
      primitive: { topology: "triangle-list" },
    });

    uniformBuffer = device.createBuffer({
      size: 16, // 4 x f32 (exposure, contrast, saturation, mask_count)
      usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
    });
    masksBuffer = device.createBuffer({
      size: MAX_MASKS * 12 * 4, // 12 f32s (3x vec4) per mask
      usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
    });
  }

  async function loadImage(/** @type {string} */ path) {
    if (!device || !context || !pipeline || !uniformBuffer || !masksBuffer) return;
    status = "loading";
    errorMessage = "";

    const preview = await getDevelopPreview(path);
    const response = await fetch(convertFileSrc(preview.path));
    const bitmap = await createImageBitmap(await response.blob());

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

    // M3 Slice 7: recreated per image -- must be sized to THIS image's
    // native resolution, an OffscreenCanvas at the wrong resolution would
    // rasterize dabs at the wrong scale. Existing brush masks' dab lists
    // (loaded from a previously-saved edit stack) are re-rasterized from
    // scratch into fresh canvases by syncBrushRasterization, called below
    // via this function's own writeAdjustmentsAndRender() call at the end.
    brushTextureArray?.destroy();
    brushTextureArray = device.createTexture({
      size: [bitmap.width, bitmap.height, MAX_MASKS],
      format: "rgba8unorm",
      usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.COPY_DST,
    });
    brushRasterState = new Map();
    freeBrushLayers = Array.from({ length: MAX_MASKS }, (_, i) => i);

    const sampler = device.createSampler({ magFilter: "linear", minFilter: "linear" });
    bindGroup = device.createBindGroup({
      layout: pipeline.getBindGroupLayout(0),
      entries: [
        { binding: 0, resource: sampler },
        { binding: 1, resource: sourceTexture.createView() },
        { binding: 2, resource: { buffer: uniformBuffer } },
        { binding: 3, resource: { buffer: masksBuffer } },
        { binding: 4, resource: brushTextureArray.createView({ dimension: "2d-array" }) },
      ],
    });

    if (context.canvas instanceof HTMLCanvasElement) {
      context.canvas.width = bitmap.width;
      context.canvas.height = bitmap.height;
    }
    context.configure({ device, format: presentationFormat, alphaMode: "opaque" });

    status = "ready";
    await tick(); // overlayEl only mounts once status flips to "ready"
    syncOverlayPosition();
    writeAdjustmentsAndRender();
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
    if (!device || !context || !pipeline || !bindGroup || !uniformBuffer || !masksBuffer) return;
    syncBrushRasterization();
    device.queue.writeBuffer(
      uniformBuffer,
      0,
      new Float32Array([exposure, contrast, saturation, masks.length]),
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
      } else {
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
    const pass = encoder.beginRenderPass({
      colorAttachments: [
        {
          view: context.getCurrentTexture().createView(),
          clearValue: { r: 0, g: 0, b: 0, a: 1 },
          loadOp: "clear",
          storeOp: "store",
        },
      ],
    });
    pass.setPipeline(pipeline);
    pass.setBindGroup(0, bindGroup);
    pass.draw(3);
    pass.end();
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
    // a re-fetch.
    void exposure;
    void contrast;
    void saturation;
    void masks;
    if (status === "ready") writeAdjustmentsAndRender();
  });
</script>

<div class="canvas-wrap" class:zoomed={zoomMode === "100"} bind:this={wrapEl}>
  <canvas
    bind:this={canvasEl}
    class:zoomed={zoomMode === "100"}
    class:placing={activeTool === "linear_gradient" || activeTool === "radial_gradient" || activeTool === "brush"}
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
          <svg class="mask-line" class:selected={mask.id === selectedMaskId}>
            <line x1="{mask.start.x * 100}%" y1="{mask.start.y * 100}%" x2="{mask.end.x * 100}%" y2="{mask.end.y * 100}%" />
          </svg>
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
          <svg class="mask-ellipse" class:selected={mask.id === selectedMaskId}>
            <ellipse cx="{mask.center.x * 100}%" cy="{mask.center.y * 100}%" rx="{mask.radiusX * 100}%" ry="{mask.radiusY * 100}%" />
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
