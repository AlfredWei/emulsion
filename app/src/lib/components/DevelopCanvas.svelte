<script>
  import { linearFeatherLines, radialFeatherRadii, spotCentroidAndRadius } from "$lib/maskGeometry.js";
  import { createGpuHandles, HISTOGRAM_SIZE } from "$lib/gpu/gpuHandles.js";
  import { initGpu as initGpuImpl } from "$lib/gpu/pipelines.js";
  import { readHistogramIfIdle as readHistogramIfIdleImpl, writeAdjustmentsAndRender as writeAdjustmentsAndRenderImpl } from "$lib/gpu/renderFrame.js";
  import { applyBitmapToGpu as applyBitmapToGpuImpl } from "$lib/gpu/sourceTexture.js";
  import { buildAtmLightChainSizes } from "$lib/gpu/atmChain.js";
  import { rasterizeDab, rasterizeSpotDab } from "$lib/gpu/brushRaster.js";
  import { tick, flushSync, untrack } from "svelte";
  import { convertFileSrc } from "@tauri-apps/api/core";
  import { getDenoisedDevelopPreview } from "$lib/api/denoise.js";
  import { getDevelopPreview, getDevelopFullPreview, buildToneCurveLut, buildHslUniformData, buildSplitToningUniformData, buildVignetteUniformData, buildLensCorrectionUniformData, buildPerspectiveUniformData, buildGrainUniformData, buildSharpenUniformData, buildLumaNrUniformData, buildColorNrUniformData, isCropIdentity } from "$lib/api/develop.js";
  import { clamp01, cropMinFrac, moveCropRect, cropCornerPoints, resizeCropCorner, resizeCropEdge, cropHandlePos, trueElementBox, cropRectFitsRotatedBounds } from "$lib/cropMath.js";
  import { FIT, fitScale, effectiveScale, reduceZoom, contentOffset, visibleRegion, scrollForFocus, focusAtViewportPoint } from "$lib/zoomMath.js";
  import DevelopZoomHud from "$lib/components/DevelopZoomHud.svelte";
  import { binHistogramPixels } from "$lib/histogramMath.js";
  import { classifyGpuFailure } from "$lib/gpuFallback.js";


  /**
   * @type {{
   *   imagePath: string,
   *   imageContentHash?: string | null,
   *   aiDenoiseAmount?: number,
   *   denoiseVersion?: number,
   *   exposure: number,
   *   contrast: number,
   *   saturation: number,
   *   temperature?: number,
   *   tint?: number,
   *   highlights?: number,
   *   shadows?: number,
   *   whites?: number,
   *   blacks?: number,
   *   masks: import('$lib/api/develop.js').Mask[],
   *   activeTool: string | null,
   *   selectedMaskId: string | null,
   *   selectedShapeId?: string | null,
   *   overlayShape?: import('$lib/api/develop.js').Mask | null,
   *   shapeArmed?: boolean,
   *   brushSize: number,
   *   brushHardness: number,
   *   brushFlow: number,
   *   spotBrushSize: number,
   *   eraseMode: boolean,
   *   showMaskOverlay: boolean,
   *   maskOverlaysVisible: boolean,
   *   showOriginal: boolean,
   *   spacePanning: boolean,
   *   onSpotBrushSizeChange?: (size: number) => void,
   *   onMaskCreated: (placement:
   *     | { kind: "linear_gradient", start: {x: number, y: number}, end: {x: number, y: number} }
   *     | { kind: "radial_gradient", center: {x: number, y: number}, radiusX: number, radiusY: number }
   *     | { kind: "brush", id: string }
   *     | { kind: "color_range", refColor: {r: number, g: number, b: number} }
   *     | { kind: "spot", id: string, initialDab: import('$lib/api/develop.js').SpotDab }
   *     | { kind: "red_eye", center: {x: number, y: number}, radiusX: number, radiusY: number }
   *     | { kind: "segment", mask: import('$lib/api/develop.js').SegmentMask }
   *   ) => void,
   *   segmentPrompts?: import('$lib/api/develop.js').SegmentPrompt[],
   *   onSegmentClick?: (click: { x: number, y: number, positive: boolean }) => void,
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
   *   vignette: {amount: number, midpoint: number, feather: number, roundness: number},
   *   lensCorrection: {
   *     profile_enabled: boolean,
   *     distortion_amount: number,
   *     vignette_amount: number,
   *     ca_amount: number,
   *     manual_distortion: number,
   *     manual_ca: number,
   *     profile: import('$lib/api/develop.js').LensProfileMatch | null,
   *   },
   *   perspective: {vertical: number, horizontal: number, rotate: number, aspect: number, scale: number},
   *   grain: {amount: number, size: number, roughness: number, tone: number, chroma: number},
   *   sharpen: {amount: number, radius: number, detail: number, masking: number},
   *   lumaNR: {amount: number, detail: number, contrast: number},
   *   colorNR: {amount: number, detail: number},
   *   crop: {x: number, y: number, width: number, height: number, angle: number},
   *   onCropChange: (patch: Partial<{x: number, y: number, width: number, height: number, angle: number}>) => void,
   *   cropAspectLock: number | null,
   *   onSourceDimensions?: (width: number, height: number) => void,
   *   zoomRequest?: { action: import('$lib/zoomMath.js').ZoomAction } | null,
   *   nativeWidth?: number | null,
   *   nativeHeight?: number | null,
   *   onHistogramUpdate?: (data: {r: Uint32Array, g: Uint32Array, b: Uint32Array}) => void,
   *   showClippingOverlay?: boolean,
   *   onHoverPixel?: (rgb: {r: number, g: number, b: number} | null) => void,
   *   softProofEnabled?: boolean,
   *   softProofPreviewUrl?: string | null,
   *   softProofLoading?: boolean,
   *   softProofProfileLabel?: string,
   *   cpuFallbackPreviewUrl?: string | null,
   *   onGpuFallback?: (active: boolean) => void,
   * }}
   */
  let {
    imagePath,
    imageContentHash = null,
    aiDenoiseAmount = 0,
    denoiseVersion = 0,
    exposure,
    contrast,
    saturation,
    temperature = 0,
    tint = 0,
    highlights = 0,
    shadows = 0,
    whites = 0,
    blacks = 0,
    masks,
    activeTool,
    selectedMaskId,
    // Composable masking (RFC-0025): the shape being edited (its own id), that shape as a
    // mask-shaped object for the overlay and brush painting only (never rendered by the GPU), and
    // whether the next placement/stroke creates a SHAPE rather than a mask.
    selectedShapeId = null,
    overlayShape = null,
    shapeArmed = false,
    brushSize,
    brushHardness,
    brushFlow,
    spotBrushSize,
    eraseMode,
    showMaskOverlay,
    maskOverlaysVisible = true,
    showOriginal = false,
    spacePanning = false,
    onSpotBrushSizeChange,
    onMaskCreated,
    // Select Subject (RFC-0026): the clicks so far (drawn as dots) and where a click goes.
    segmentPrompts = [],
    onSegmentClick = () => {},
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
    lensCorrection,
    perspective,
    grain,
    sharpen,
    lumaNR,
    colorNR,
    crop,
    onCropChange,
    cropAspectLock,
    onSourceDimensions,
    zoomRequest = null,
    nativeWidth = null,
    nativeHeight = null,
    onHistogramUpdate,
    showClippingOverlay = false,
    onHoverPixel,
    softProofEnabled = false,
    softProofPreviewUrl = null,
    softProofLoading = false,
    softProofProfileLabel = "",
    cpuFallbackPreviewUrl = null,
    onGpuFallback,
  } = $props();

  let canvasEl = $state(/** @type {HTMLCanvasElement | null} */ (null));
  let wrapEl = $state(/** @type {HTMLDivElement | null} */ (null));
  let overlayEl = $state(/** @type {HTMLDivElement | null} */ (null));
  let status = $state("loading"); // "loading" | "ready" | "error" | "cpu-fallback"
  let errorMessage = $state("");
  // M5 Slice 1 (GPU/CPU fallback): set only when `status === "cpu-fallback"`,
  // i.e. `initGpu`'s own device-acquisition step threw -- see
  // `classifyGpuFailure`'s own doc comment for why only THAT failure class
  // (not a later shader-compile error, which means WebGPU itself is fine)
  // routes here. Drives the on-canvas banner's tooltip.
  let fallbackInfo = $state(/** @type {import('$lib/gpuFallback.js').GpuFallbackInfo | null} */ (null));
  // M4 Smart Previews: true when the currently-loaded preview is a
  // fallback served because the source file itself couldn't be read (see
  // getDevelopPreview's own doc comment) -- drives the on-canvas banner
  // below and disables the 1:1 tier upgrade (which would just fail the
  // same way, since it also needs to read the source).
  let isSmartPreview = $state(false);

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

  // Zoom. `zoom` is the user's intent -- Fit, or an explicit scale from 50% to
  // 200% of the photo's NATIVE size (see zoomMath.js: every input goes through
  // `reduceZoom`, so the range/clamp rules live in one tested place). The
  // derived values below turn it into pixels. Panning stays the browser's own
  // native scrolling on `.canvas-wrap` rather than hand-rolled pan math.
  let zoom = $state(/** @type {import('$lib/zoomMath.js').ZoomState} */ (FIT));
  let isZoomed = $derived(zoom.mode === "zoom");
  // Mirrors `.canvas-wrap`'s scroll position so the navigator's region
  // reacts to panning (scrollLeft/Top themselves are not reactive).
  let scrollPos = $state({ left: 0, top: 0 });

  // Crop & Straighten (M3): reactive mirror of canvasEl.width/height (set
  // once in applyBitmapToGpu, imperative and NOT itself a tracked Svelte
  // dependency) -- needed so the committed-crop preview wrapper's own
  // `aspect-ratio` CSS can recompute reactively whenever a new image
  // loads. `canvasEl.width/height` themselves NEVER change for crop (see
  // the preview wrapper's own doc comment for why that's a deliberate,
  // review-driven design choice), so these only need to track image
  // loads, not crop edits.
  let sourceWidth = $state(0);
  let sourceHeight = $state(0);

  // Crop & Straighten fix (empirically found: a committed crop rendered
  // as a literal 0x0 box -- "the image became black"): the preview
  // wrapper used to be sized via CSS `aspect-ratio` + `max-width/
  // max-height:100%` alone, a PLAIN block div with no in-flow content
  // (its only child is `position:absolute`, so it contributes nothing to
  // the div's own intrinsic content size). Confirmed via a live
  // getBoundingClientRect() check that this collapses to 0x0 under this
  // app's flex-row + WebKit combination -- the SAME class of bug this
  // file already hit once for the PLAIN (non-cropped) canvas sizing (see
  // that fix's own comment further down), except THIS wrapper genuinely
  // needs its own aspect ratio (different from the canvas's native one),
  // so it can't just be "sized directly" the way canvas is. Fixed the
  // same way syncOverlayPosition already handles the mask overlay:
  // compute the box's pixel size in JS instead of trusting CSS
  // auto-sizing, tracked via a ResizeObserver on the wrap element so it
  // stays correct across window resizes.
  let wrapWidth = $state(0);
  let wrapHeight = $state(0);
  $effect(() => {
    if (!wrapEl) return;
    const el = wrapEl;
    const observer = new ResizeObserver(() => {
      wrapWidth = el.clientWidth;
      wrapHeight = el.clientHeight;
    });
    observer.observe(el);
    return () => observer.disconnect();
  });
  const CROP_CLIP_PADDING_PX = 22; // matches .canvas-wrap's own CSS padding

  // 1:1 preview tier (mirrors real Lightroom's Standard/1:1 Preview split,
  // PRD/PRD.md's own explicit phrasing): Fit always shows the draft tier
  // (getDevelopPreview, capped to DEVELOP_PREVIEW_MAX_DIMENSION on the
  // Rust side); a zoom whose on-screen size outgrows the draft lazily
  // upgrades to a true native-resolution texture the FIRST time that
  // happens, so zoomed views show finer detail rather than just
  // CSS-magnifying the same capped preview. Once
  // upgraded, stays upgraded for the rest of this image's session (zooming
  // back out doesn't downgrade -- see the zoom-trigger $effect below)
  // -- these three vars are plain module state, not $state, matching
  // dragState/paintingMaskId's own "imperative bookkeeping, not something
  // markup reads reactively" reasoning elsewhere in this file.
  /** @type {string | null} */
  let fullTierPath = null; // path the currently-uploaded full tier belongs to, or null
  /** @type {Promise<void> | null} */
  let fullTierPromise = null; // in-flight upgrade, deduped so rapid zoom toggling can't fire it twice
  let activeTier = "draft"; // "draft" | "full"
  // AI Denoise (RFC-0027 §3.3): what the uploaded source texture currently is -- the plain preview
  // (`amount` 0) or its mix with the kept denoised copy at that amount. `version` is the store's
  // `cacheVersion` it was built under (a re-run or a removal makes an equal amount stale).
  // Imperative bookkeeping like `activeTier`: markup never reads it.
  let appliedDenoise = { path: "", tier: "draft", amount: 0, version: 0 };
  // The amount of the mix on the GPU right now (0 = the plain preview), exposed as a data attribute so a
  // test can tell which source texture the render is built from. Reactive, unlike `appliedDenoise`.
  let shownDenoise = $state(0);
  /** @param {{ path: string, tier: string, amount: number, version: number }} next */
  function setAppliedDenoise(next) {
    appliedDenoise = next;
    shownDenoise = next.amount;
  }
  let denoiseSeq = 0; // a newer request supersedes an older one still in flight
  // True once the native-resolution tier is on the GPU: the canvas backing
  // store then IS the photo's native size (natRatio below becomes 1). Reactive
  // (unlike activeTier) because the display scale is derived from it.
  let fullTierReady = $state(false);
  let uploadingFullTier = false; // set only around the full tier's own applyBitmapToGpu
  // Draft-tier preview URL, reused as the navigator's thumbnail.
  let navThumbUrl = $state(/** @type {string | null} */ (null));
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
  /** @type {{
   *   maskId: string,
   *   which: "start" | "end" | "center" | "radius" | "spot_move" | "spot_source_offset",
   *   center?: {x:number,y:number},
   *   dabs?: import('$lib/api/develop.js').SpotDab[],
   * } | null} */
  let handleDragState = null;

  // Crop & Straighten (M3): a fully separate drag system from masks above
  // -- the crop rect always EXISTS (default full-frame) rather than being
  // "placed" from empty space, so there's no `placingCrop`-style creation
  // phase, only handle-drag (resize, 8 handles) and interior-drag (move).
  // `which` doubles as both the handle identity AND the CSS class suffix
  // for positioning (see cropHandlePos below). Corner handles respect
  // `cropAspectLock`; edge handles are always free-form (a deliberate,
  // named scope cut -- edge-handle-preserves-ratio math combined with
  // bounds/min-size clamping is real, fiddly complexity real Lightroom
  // itself also doesn't apply symmetrically).
  /** @type {{ start: {x:number,y:number}, startRect: {x:number,y:number,width:number,height:number}, which: "nw"|"n"|"ne"|"e"|"se"|"s"|"sw"|"w"|"move" } | null} */
  let cropDragState = null;
  // cropAspectLock itself is a PROP (see below), not local state -- it's
  // shared with MaskToolStrip.svelte's own aspect-preset buttons, so it
  // has to live in +page.svelte, the nearest common ancestor.
  // A real PIXEL floor (not a flat normalized fraction, which this used
  // to be): a flat fraction's EFFECTIVE pixel size scales with the
  // source image's own resolution, so on a modest source it could still
  // floor at just a handful of pixels -- degenerate enough that the
  // committed-crop preview's own math (which divides by crop.width/
  // height, see cropClipSize's aspect-ratio computation) could produce a
  // useless sliver, and the exported crop would be near-meaningless too.
  // Named consistently with develop_engine.rs's own CROP_MIN_SIZE_PX
  // (same value, same reasoning, just enforced at the UI-drag layer here
  // instead of the Rust/export layer there) -- see this file's own
  // module-level `sourceWidth`/`sourceHeight` for why those are already
  // tracked reactively and safe to read directly here. The actual
  // clamping/resize math itself (clamp01, moveCropRect, cropCornerPoints,
  // resizeCropCorner, resizeCropEdge, cropHandlePos) now lives in
  // $lib/cropMath.js -- a pure, DOM-free module, imported above -- so it
  // has real unit test coverage (cropMath.test.js) instead of only ever
  // being exercised through a throwaway empirical harness.
  const CROP_MIN_PX = 64;
  function cropMinFracX() {
    return cropMinFrac(CROP_MIN_PX, sourceWidth);
  }
  function cropMinFracY() {
    return cropMinFrac(CROP_MIN_PX, sourceHeight);
  }

  function handleCropHandlePointerDown(/** @type {PointerEvent} */ e, /** @type {string} */ which) {
    e.stopPropagation();
    e.preventDefault();
    cropDragState = {
      start: screenToNormalized(e.clientX, e.clientY),
      startRect: { x: crop.x, y: crop.y, width: crop.width, height: crop.height },
      which: /** @type {any} */ (which),
    };
    try {
      /** @type {HTMLElement} */ (e.currentTarget).setPointerCapture(e.pointerId);
    } catch {
      // Non-fatal -- see handleMaskHandlePointerDown's own identical
      // try/catch for why.
    }
  }

  function handleCropHandlePointerMove(/** @type {PointerEvent} */ e) {
    if (!cropDragState) return;
    const p = screenToNormalized(e.clientX, e.clientY);
    const dx = p.x - cropDragState.start.x;
    const dy = p.y - cropDragState.start.y;
    const { which, startRect } = cropDragState;
    let next;
    if (which === "move") next = moveCropRect(startRect, dx, dy);
    else if (which === "nw" || which === "ne" || which === "sw" || which === "se") {
      next = resizeCropCorner(startRect, which, dx, dy, cropAspectLock, sourceWidth / sourceHeight, cropMinFracX(), cropMinFracY());
    } else {
      next = resizeCropEdge(startRect, which, dx, dy, cropMinFracX(), cropMinFracY());
    }
    // Reject a move/resize that would push the rect into the blanked-out
    // corners a nonzero straighten angle reveals (see
    // cropRectFitsRotatedBounds's own doc comment) -- otherwise nothing
    // stops a drag from re-exposing exactly the black wedges
    // `inscribedCropForAngle`'s own angle-change recenter exists to avoid
    // in the first place, since a plain move/resize never re-runs that
    // check. Freezing at the last valid rect (silently ignoring this one
    // pointermove) rather than clamping to the nearest valid position is a
    // real, named scope cut -- a full "slide back to the boundary" clamp
    // needs the same off-center inscribed-rect solve `inscribedCropForAngle`
    // deliberately doesn't attempt; simply not moving further in an invalid
    // direction is correct, if slightly less smooth, and never lets an
    // invalid rect reach the edit stack.
    if (!cropRectFitsRotatedBounds(next, sourceWidth, sourceHeight, crop.angle)) return;
    onCropChange(next);
  }

  function handleCropHandlePointerUp(/** @type {PointerEvent} */ e) {
    try {
      /** @type {HTMLElement} */ (e.currentTarget).releasePointerCapture(e.pointerId);
    } catch {
      // Non-fatal -- see handleMaskHandlePointerDown's own identical
      // try/catch for why.
    }
    cropDragState = null;
  }

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

  // Select Subject (RFC-0026): same single-click gesture; Alt/Option turns it into a NEGATIVE click.
  /** @type {{x:number,y:number} | null} */
  let segmentClickStart = null;

  // M3 Slice 7 (brush) / M4 Slice 2 (spot removal, brush-like per explicit
  // user request): both tools paint a stroke the SAME way, so they share
  // this one set of transient, per-stroke state variables -- safe since
  // `activeTool` can't change mid-drag. Deliberately transient: no
  // persistent "which mask am I painting into" tracking survives past
  // pointerup. Instead, EVERY pointerdown re-derives the paint target from
  // `selectedMaskId`: if it currently points at a mask of the SAME kind as
  // the active tool, this stroke APPENDS to it (real Lightroom's own
  // multi-stroke-per-mask model, matching PROGRESS.md's design note);
  // otherwise this stroke creates a fresh mask and selects it. "New Brush"/
  // "New Spot" tool-strip buttons achieve "start fresh" simply by
  // deselecting (selectedMaskId = null) -- no separate reset signal needs
  // to reach this component at all.
  // Masks plus the shape being edited (mask-shaped, own id): what the overlay draws handles for and
  // where a brush stroke can append. The GPU render reads `masks` only.
  const paintTargets = $derived(overlayShape ? [...masks, overlayShape] : masks);
  const editingId = $derived(selectedShapeId ?? selectedMaskId);
  /** @type {string | null} */
  let paintingMaskId = null;
  /** @type {(import('$lib/api/develop.js').Dab | import('$lib/api/develop.js').SpotDab)[]} */
  let strokeDabs = [];
  /** @type {{x: number, y: number} | null} */
  let lastPaintPoint = null;
  // Live brush-size cursor preview (SVG ellipse in the mask-overlay, drawn
  // as a true on-screen circle via the same width/height aspect correction
  // radiusFromDrag already uses for radial masks) -- shown on hover, not
  // just while actively painting, so size is visible before committing a
  // stroke. Separate state per tool (not shared) since a Brush cursor and a
  // Spot cursor are sized from different props (brushSize/spotBrushSize).
  let brushCursor = $state(/** @type {{x:number,y:number} | null} */ (null));
  let spotCursor = $state(/** @type {{x:number,y:number} | null} */ (null));

  /** CSS-pixel click position -> native canvas-backing-store pixel
   * coordinate. Reused from the zoom-to-point math (M3 Slice 3) --
   * `getBoundingClientRect()` already reflects the current scroll offset,
   * so this needs no extra bookkeeping for panned/zoomed state.
   *
   * Corrected via `trueElementBox` (see that function's own doc comment
   * for the full derivation) for a real, non-hypothetical case: while the
   * crop tool is active with a nonzero straighten angle, the canvas has a
   * live CSS `transform: rotate()` applied for visual feedback (see the
   * canvas element's own inline style) -- but the crop overlay/handles
   * this function's own callers interact with live in a FIXED, unrotated
   * coordinate space that never rotates with the image. `rect` (the
   * canvas's own `getBoundingClientRect()`) reflects the ROTATED visual
   * box in that state -- strictly larger than, and offset from, the true
   * unrotated box the overlay is actually calibrated against -- so using
   * its width/height/left/top directly (as this used to) silently
   * mis-scales/mis-offsets a dragged handle the moment an angle is set,
   * NOT because the click needs "un-rotating" (it's already in the right
   * space) but because `rect`'s own numbers are simply wrong for a space
   * that never rotated. `trueElementBox` derives the correct box from
   * `rect`'s reliable CENTER plus `canvasEl.offsetWidth/offsetHeight`
   * (layout measurements a CSS transform never affects). At angle 0
   * (every other tool/state) this is an exact no-op, verified in
   * cropMath.test.js. */
  function screenToNativePixel(/** @type {number} */ clientX, /** @type {number} */ clientY) {
    if (!canvasEl) return { x: 0, y: 0 };
    const rect = canvasEl.getBoundingClientRect();
    const trueBox = trueElementBox(rect, canvasEl.offsetWidth, canvasEl.offsetHeight);
    const scaleX = trueBox.width / canvasEl.width;
    const scaleY = trueBox.height / canvasEl.height;
    return { x: (clientX - trueBox.left) / scaleX, y: (clientY - trueBox.top) / scaleY };
  }

  /** Native pixel coordinate -> normalized (0..1) image-space coordinate,
   * matching the shader's own `in.uv`. */
  function screenToNormalized(/** @type {number} */ clientX, /** @type {number} */ clientY) {
    if (!canvasEl) return { x: 0, y: 0 };
    const p = screenToNativePixel(clientX, clientY);
    return { x: p.x / canvasEl.width, y: p.y / canvasEl.height };
  }

  /** Reports the graded RGB value under the cursor to `onHoverPixel`, for
   * the histogram panel's own "value under cursor" readout. Reuses
   * `lastHistogramPixels` -- the SAME 256x256 downsampled readback the
   * live histogram itself is built from (see `readHistogramIfIdle`) --
   * rather than a fresh per-pixel GPU read, so this is just an index
   * lookup, not a new readback path. This means the reported value is a
   * bilinear-downsampled sample near the cursor, not the exact
   * full-resolution pixel -- an acceptable, named approximation for a
   * "roughly what's under the cursor" readout, consistent with the
   * histogram it's paired with already being a 256-bucket sample rather
   * than exact per-pixel data. */
  function reportHoverPixel(/** @type {number} */ clientX, /** @type {number} */ clientY) {
    if (!onHoverPixel) return;
    if (!gpu.lastHistogramPixels) {
      onHoverPixel(null);
      return;
    }
    const { x, y } = screenToNormalized(clientX, clientY);
    if (x < 0 || x > 1 || y < 0 || y > 1) {
      onHoverPixel(null);
      return;
    }
    const col = Math.min(HISTOGRAM_SIZE - 1, Math.max(0, Math.floor(x * HISTOGRAM_SIZE)));
    const row = Math.min(HISTOGRAM_SIZE - 1, Math.max(0, Math.floor(y * HISTOGRAM_SIZE)));
    const i = (row * HISTOGRAM_SIZE + col) * 4;
    const bgra = gpu.presentationFormat.startsWith("bgra");
    const r = gpu.lastHistogramPixels[bgra ? i + 2 : i];
    const g = gpu.lastHistogramPixels[i + 1];
    const b = gpu.lastHistogramPixels[bgra ? i : i + 2];
    onHoverPixel({ r, g, b });
  }

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
    if (!gpu.sourceSampleCtx || !gpu.sourceSampleCanvas) return null;
    const px = Math.min(Math.max(Math.round(normX * gpu.sourceSampleCanvas.width), 0), gpu.sourceSampleCanvas.width - 1);
    const py = Math.min(Math.max(Math.round(normY * gpu.sourceSampleCanvas.height), 0), gpu.sourceSampleCanvas.height - 1);
    const d = gpu.sourceSampleCtx.getImageData(px, py, 1, 1).data;
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

  /** Same aspect correction as `brushCursorRyPercent`, generalized to an
   * arbitrary width-normalized radius -- SpotMask's own `radius` (a single
   * value, fraction of image width, matching `dab_falloff`'s convention in
   * both develop_engine.rs and the WGSL shader) needs the same height-
   * relative ry percentage to render as a true on-screen circle. */
  function spotRyPercent(/** @type {number} */ radius) {
    if (!canvasEl || !canvasEl.height) return radius * 100;
    return radius * (canvasEl.width / canvasEl.height) * 100;
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
   * `lastPaintPoint` is only advanced when dabs are actually placed (see
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

  /** M4 Slice 2: a spot dab, mirroring `makeDab` -- no `hardness`/`flow`/
   * `mode`, since a spot mask's edge softness comes from the mask's own
   * single `feather` (set via MaskEditorPanel), not per-dab like brush
   * painting. */
  function makeSpotDab(/** @type {{x:number,y:number}} */ p) {
    return { x: p.x, y: p.y, radius: spotBrushSize };
  }

  /** Same spacing/interpolation logic as `interpolatedDabs`, parametrized
   * on `spotBrushSize` instead of `brushSize` -- kept as a separate
   * function (not a shared helper both call) since the two dab shapes
   * (`Dab` vs `SpotDab`) and their paint-time-settings differ enough that
   * factoring out the shared 5 lines of spacing math would cost more
   * indirection than it saves. */
  function interpolatedSpotDabs(/** @type {{x:number,y:number}} */ from, /** @type {{x:number,y:number}} */ to) {
    const dx = to.x - from.x;
    const dy = to.y - from.y;
    const dist = Math.hypot(dx, dy);
    const spacing = Math.max(spotBrushSize * 0.25, 0.0008);
    if (dist < spacing) return [];
    const steps = Math.min(Math.floor(dist / spacing), 200);
    const dabs = [];
    for (let i = 1; i <= steps; i++) {
      const t = i / steps;
      dabs.push(makeSpotDab({ x: from.x + dx * t, y: from.y + dy * t }));
    }
    return dabs;
  }

  function handlePointerDown(/** @type {PointerEvent} */ e) {
    // Space-to-pan (checked before every activeTool branch below, including
    // "crop"): holding Space temporarily overrides WHATEVER tool is active
    // so the user can reposition a zoomed-in view without switching tools
    // and losing their place -- real Photoshop/Lightroom convention. Reuses
    // the exact same `dragState` the no-tool-active pan/zoom-click fallback
    // at the bottom of this function already implements; see
    // handlePointerMove/handlePointerUp's own matching early checks for why
    // that reuse requires `dragState` to be checked FIRST there too.
    if (spacePanning) {
      if (!wrapEl) return;
      e.preventDefault();
      dragState = {
        startX: e.clientX,
        startY: e.clientY,
        startScrollLeft: wrapEl.scrollLeft,
        startScrollTop: wrapEl.scrollTop,
      };
      tryCapturePointer(e);
      return;
    }
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
    if (activeTool === "red_eye") {
      e.preventDefault();
      const center = screenToNormalized(e.clientX, e.clientY);
      placingMask = { kind: "red_eye", center, radiusX: 0, radiusY: 0 };
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
    if (activeTool === "segment") {
      e.preventDefault();
      segmentClickStart = { x: e.clientX, y: e.clientY };
      tryCapturePointer(e);
      return;
    }
    if (activeTool === "brush") {
      e.preventDefault();
      const p = screenToNormalized(e.clientX, e.clientY);
      // Re-derive the paint target fresh on every stroke, from the
      // CURRENT selection -- see the brush-state doc comment above for
      // why this is deliberately transient, not tracked persistently.
      // Painting continues into the edited shape when one is selected, and never into an
      // existing brush MASK while a new shape is armed (that stroke creates the shape).
      const paintId = selectedShapeId ?? selectedMaskId;
      const existing = shapeArmed ? undefined : paintTargets.find((m) => m.id === paintId && m.op === "brush_mask");
      if (existing) {
        paintingMaskId = paintId;
        strokeDabs = [.../** @type {any} */ (existing).dabs];
      } else {
        const newId = crypto.randomUUID();
        paintingMaskId = newId;
        strokeDabs = [];
        onMaskCreated({ kind: "brush", id: newId });
      }
      strokeDabs.push(makeDab(p));
      lastPaintPoint = p;
      brushCursor = p;
      onMaskUpdated(/** @type {string} */ (paintingMaskId), { dabs: [...strokeDabs] });
      tryCapturePointer(e);
      return;
    }
    if (activeTool === "spot") {
      // M4 Slice 2: paints a stroke, exactly mirroring the Brush tool's own
      // pointerdown above (per explicit user request for a brush-like spot
      // removal interaction, replacing the original single click-drag
      // circle) -- see the shared paint-session state's own doc comment.
      e.preventDefault();
      const p = screenToNormalized(e.clientX, e.clientY);
      const existing = masks.find((m) => m.id === selectedMaskId && m.op === "spot_mask");
      const dab = makeSpotDab(p);
      if (existing) {
        paintingMaskId = selectedMaskId;
        strokeDabs = [.../** @type {any} */ (existing).dabs, dab];
        onMaskUpdated(/** @type {string} */ (paintingMaskId), { dabs: [...strokeDabs] });
      } else {
        const newId = crypto.randomUUID();
        paintingMaskId = newId;
        strokeDabs = [dab];
        onMaskCreated({ kind: "spot", id: newId, initialDab: dab });
      }
      lastPaintPoint = p;
      spotCursor = p;
      tryCapturePointer(e);
      return;
    }
    if (activeTool === "crop") {
      // Crop's own handles/interior rect (rendered above the canvas, each
      // with pointer-events:auto and its own stopPropagation'd handler)
      // are the only interactive surface while this tool is active -- the
      // dimmed area around them is deliberately pointer-events:none (so it
      // doesn't visually block anything), which means a click there falls
      // straight through to the canvas underneath. Without this branch
      // that click was falling into the plain click-to-zoom/drag-to-pan
      // logic below, same as this function's own doc comment already
      // requires for every OTHER placing tool -- a real, reported bug
      // ("the image breaks" when zooming/panning right after selecting
      // Crop), not a hypothetical.
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
    // Checked FIRST (ahead of every tool-specific branch below, AND ahead of
    // reportHoverPixel): `dragState` is only ever set by space-pan or the
    // no-tool-active fallback at the bottom of this function, but space-pan
    // can now start while `activeTool` is still "brush"/"spot"/etc, so this
    // can no longer wait until after those branches without being shadowed
    // by them -- see handlePointerDown's own space-pan branch. Skipping
    // reportHoverPixel during an active pan-drag avoids per-frame $state
    // churn (MetadataPanel's RGB readout) competing with scroll
    // compositing -- the readout isn't meaningful mid-pan anyway.
    if (dragState && wrapEl) {
      wrapEl.scrollLeft = dragState.startScrollLeft - (e.clientX - dragState.startX);
      wrapEl.scrollTop = dragState.startScrollTop - (e.clientY - dragState.startY);
      return;
    }
    reportHoverPixel(e.clientX, e.clientY);
    if (placingMask?.kind === "linear_gradient") {
      placingMask = { ...placingMask, end: screenToNormalized(e.clientX, e.clientY) };
      return;
    }
    if (placingMask?.kind === "radial_gradient") {
      placingMask = { ...placingMask, ...radiusFromDrag(placingMask.center, e.clientX, e.clientY) };
      return;
    }
    if (placingMask?.kind === "red_eye") {
      placingMask = { ...placingMask, ...radiusFromDrag(placingMask.center, e.clientX, e.clientY) };
      return;
    }
    if (activeTool === "brush") {
      const p = screenToNormalized(e.clientX, e.clientY);
      brushCursor = p; // shown on hover too, not just while painting
      if (paintingMaskId && lastPaintPoint) {
        const newDabs = interpolatedDabs(lastPaintPoint, p);
        if (newDabs.length > 0) {
          strokeDabs.push(...newDabs);
          lastPaintPoint = p;
          onMaskUpdated(paintingMaskId, { dabs: [...strokeDabs] });
        }
      }
      return;
    }
    if (activeTool === "spot") {
      const p = screenToNormalized(e.clientX, e.clientY);
      spotCursor = p; // shown on hover too, not just while painting
      if (paintingMaskId && lastPaintPoint) {
        const newDabs = interpolatedSpotDabs(lastPaintPoint, p);
        if (newDabs.length > 0) {
          strokeDabs.push(...newDabs);
          lastPaintPoint = p;
          onMaskUpdated(paintingMaskId, { dabs: [...strokeDabs] });
        }
      }
      return;
    }
  }

  function handlePointerUp(/** @type {PointerEvent} */ e) {
    try {
      canvasEl?.releasePointerCapture(e.pointerId);
    } catch {
      // Releasing a capture that was never successfully acquired would
      // itself throw -- non-fatal, see tryCapturePointer's comment.
    }
    // Checked FIRST (ahead of every tool-specific branch below), mirroring
    // handlePointerMove's own reordering -- `dragState` can now be set by
    // space-pan even while `activeTool` is "brush"/"spot", so the
    // `activeTool === "brush" || "spot"` branch further down (which just
    // clears paint-session state) can no longer run unconditionally before
    // this finalizes the pan/zoom-click drag.
    if (dragState) {
      const moved = Math.max(Math.abs(e.clientX - dragState.startX), Math.abs(e.clientY - dragState.startY));
      const clickPoint = { x: e.clientX, y: e.clientY };
      dragState = null;
      if (moved >= DRAG_CLICK_THRESHOLD) return; // a completed drag, not a click -- leave scroll as-is
      // Click toggles Fit <-> 100%, holding the clicked point still. A
      // click while zoomed (at any scale) goes back to Fit.
      if (!wrapEl) return;
      const r = wrapEl.getBoundingClientRect();
      applyZoom({ type: "toggle" }, { x: clickPoint.x - r.left, y: clickPoint.y - r.top });
      return;
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
    if (placingMask?.kind === "red_eye") {
      const { center, radiusX, radiusY } = placingMask;
      placingMask = null;
      // Same minimum-radius guard as radial (radius is a divisor in the
      // shader's ellipse-distance formula).
      if (radiusX > 0.01 && radiusY > 0.01) onMaskCreated({ kind: "red_eye", center, radiusX, radiusY });
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
    if (segmentClickStart) {
      const moved = Math.max(Math.abs(e.clientX - segmentClickStart.x), Math.abs(e.clientY - segmentClickStart.y));
      segmentClickStart = null;
      if (moved < DRAG_CLICK_THRESHOLD) {
        const p = screenToNormalized(e.clientX, e.clientY);
        // Outside the picture (the letterbox around a fitted image) is not a click on the subject.
        if (p.x >= 0 && p.x <= 1 && p.y >= 0 && p.y <= 1) onSegmentClick({ x: p.x, y: p.y, positive: !e.altKey });
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
    if (activeTool === "brush" || activeTool === "spot") {
      // Stroke ends, but deliberately does NOT clear selectedMaskId in the
      // parent -- a subsequent stroke (new pointerdown, tool still active)
      // re-derives paintingMaskId from selectedMaskId and continues
      // appending to the SAME mask, giving multi-stroke-per-mask painting
      // "for free" with no persistent state here.
      paintingMaskId = null;
      lastPaintPoint = null;
      return;
    }
  }

  // Mirrors MaskToolStrip.svelte's own Spot Size slider (min/max/step) --
  // scrolling over the canvas while painting spots is a much faster way to
  // resize the brush than reaching for the slider mid-stroke, matching the
  // same wheel-to-resize convention most photo editors' brush tools use.
  // Scoped to activeTool==="spot" (not brush) per explicit user request;
  // when it's any other tool, this deliberately does NOT preventDefault,
  // so the wheel event passes through untouched to whatever the browser
  // would otherwise do with it (e.g. nothing, since panning is via drag/
  // scrollbars here, not wheel).
  const SPOT_BRUSH_SIZE_MIN = 0.005;
  const SPOT_BRUSH_SIZE_MAX = 0.15;
  const SPOT_BRUSH_SIZE_STEP = 0.0025;
  function handleWheel(/** @type {WheelEvent} */ e) {
    // Ctrl/Cmd + wheel zooms the photo about the cursor (also what Chromium-
    // style webviews synthesize for a trackpad pinch). Wins over the spot
    // tool's own wheel-to-resize, which stays on the unmodified wheel.
    if ((e.ctrlKey || e.metaKey) && wrapEl) {
      e.preventDefault();
      const r = wrapEl.getBoundingClientRect();
      // deltaMode 1 = lines rather than pixels.
      const deltaY = e.deltaMode === 1 ? e.deltaY * 16 : e.deltaY;
      applyZoom({ type: "wheel", deltaY }, { x: e.clientX - r.left, y: e.clientY - r.top });
      return;
    }
    if (activeTool !== "spot" || !onSpotBrushSizeChange) return;
    e.preventDefault();
    const delta = e.deltaY > 0 ? -SPOT_BRUSH_SIZE_STEP : SPOT_BRUSH_SIZE_STEP;
    const next = Math.min(SPOT_BRUSH_SIZE_MAX, Math.max(SPOT_BRUSH_SIZE_MIN, spotBrushSize + delta));
    onSpotBrushSizeChange(next);
  }

  // A trackpad pinch over the canvas doesn't dispatch `wheel` at all -- in
  // WebKit (this app's webview on macOS) it's a separate, nonstandard
  // `gesturestart`/`gesturechange` event pair that natively zooms the whole
  // page unless prevented. Both are prevented here and `gesturechange`'s
  // cumulative `scale` drives the PHOTO's zoom instead (`pinch` action: the
  // scale when the gesture began x the gesture's scale), about the pinch
  // point. These aren't part of the DOM standard (no TS lib types, no Svelte
  // `on*` prop), so they're bound imperatively here rather than as a
  // template attribute like `onwheel`.
  $effect(() => {
    if (!canvasEl) return;
    const el = canvasEl;
    let pinchBase = 1;
    /** @param {Event} e */
    const onStart = (e) => {
      e.preventDefault();
      pinchBase = scale;
    };
    /** @param {Event} e */
    const onChange = (e) => {
      e.preventDefault();
      if (!wrapEl) return;
      const g = /** @type {any} */ (e);
      const r = wrapEl.getBoundingClientRect();
      const at = typeof g.clientX === "number" ? { x: g.clientX - r.left, y: g.clientY - r.top } : undefined;
      applyZoom({ type: "pinch", base: pinchBase, gesture: g.scale }, at);
    };
    el.addEventListener("gesturestart", onStart);
    el.addEventListener("gesturechange", onChange);
    return () => {
      el.removeEventListener("gesturestart", onStart);
      el.removeEventListener("gesturechange", onChange);
    };
  });

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
    /** @type {"start" | "end" | "center" | "radius" | "spot_move" | "spot_source_offset"} */ which,
    /** @type {{x:number,y:number}=} */ center,
    /** @type {import('$lib/api/develop.js').SpotDab[]=} */ dabs,
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
    handleDragState = { maskId, which, center, dabs };
    onMaskSelected(maskId);
    try {
      /** @type {HTMLElement} */ (e.currentTarget).setPointerCapture(e.pointerId);
    } catch {
      // See above -- non-fatal.
    }
  }

  function handleMaskHandlePointerMove(/** @type {PointerEvent} */ e) {
    if (!handleDragState) return;
    const { maskId, which, center, dabs } = handleDragState;
    if (which === "radius" && center) {
      onMaskUpdated(maskId, radiusFromDrag(center, e.clientX, e.clientY));
      return;
    }
    if (which === "spot_move" && center && dabs) {
      // M4 Slice 2: "drag to move the whole brushed spot" -- translates
      // EVERY dab by the delta from the ORIGINAL centroid (captured at
      // drag-start, in `center`) to the current pointer, applied to the
      // ORIGINAL dab snapshot (also captured at drag-start, in `dabs`),
      // not the live mask -- recomputing from the original snapshot each
      // move (rather than incrementally offsetting the already-moved
      // dabs) avoids double-applying the delta. `sourceOffset` needs no
      // change at all: it's a (dx, dy) delta, not an absolute point, so it
      // stays correct automatically once every dab moves by the same
      // amount -- the whole reason this mask shape (SpotMask's own doc
      // comment) was chosen over the original per-dest-point model.
      const p = screenToNormalized(e.clientX, e.clientY);
      const dx = p.x - center.x;
      const dy = p.y - center.y;
      onMaskUpdated(maskId, { dabs: dabs.map((d) => ({ ...d, x: d.x + dx, y: d.y + dy })) });
      return;
    }
    if (which === "spot_source_offset" && center) {
      // `center` is the stroke's own (unchanging, since only sourceOffset
      // is being edited) centroid -- the new offset is simply the current
      // pointer's delta from it.
      const p = screenToNormalized(e.clientX, e.clientY);
      onMaskUpdated(maskId, { sourceOffset: { dx: p.x - center.x, dy: p.y - center.y } });
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

  // GPU handles live on one plain (non-reactive) object; see lib/gpu/gpuHandles.js.
  const gpu = createGpuHandles();
  // A segment mask's logit field decodes asynchronously (RFC-0026); render again once it has.
  gpu.requestRender = () => {
    if (status !== "ready") return;
    // A histogram read-back still in flight makes the next render skip refreshing the histogram
    // texture (see readHistogramIfIdle), and nothing renders after this late frame to make up for
    // it -- so the histogram and the hover readout built on it would stay one frame stale. Wait
    // for the read to finish, then render.
    let tries = 0;
    const go = () => {
      if (gpu.histogramReadInFlight && tries++ < 100) {
        setTimeout(go, 30);
        return;
      }
      if (status === "ready") writeAdjustmentsAndRender();
    };
    go();
  };
  // Getters, not copies: the renderer reads props through these at the same points it used to read
  // them directly, so what the render effect tracks is unchanged.
  const renderInputs = {
    get onHistogramUpdate() {
      return onHistogramUpdate;
    },
    get masks() {
      return masks;
    },
    get showOriginal() {
      return showOriginal;
    },
    get toneCurvePoints() {
      return toneCurvePoints;
    },
    get hslBands() {
      return hslBands;
    },
    get splitToning() {
      return splitToning;
    },
    get lensCorrection() {
      return lensCorrection;
    },
    get perspective() {
      return perspective;
    },
    get vignette() {
      return vignette;
    },
    get grain() {
      return grain;
    },
    get sharpen() {
      return sharpen;
    },
    get lumaNR() {
      return lumaNR;
    },
    get colorNR() {
      return colorNR;
    },
    get showClippingOverlay() {
      return showClippingOverlay;
    },
    get showMaskOverlay() {
      return showMaskOverlay;
    },
    get selectedMaskId() {
      return selectedMaskId;
    },
    get exposure() {
      return exposure;
    },
    get contrast() {
      return contrast;
    },
    get saturation() {
      return saturation;
    },
    get dehaze() {
      return dehaze;
    },
    get texture() {
      return texture;
    },
    get clarity() {
      return clarity;
    },
    get temperature() {
      return temperature;
    },
    get tint() {
      return tint;
    },
    get highlights() {
      return highlights;
    },
    get shadows() {
      return shadows;
    },
    get whites() {
      return whites;
    },
    get blacks() {
      return blacks;
    },
  };
  const gpuHooks = {
    /** @param {string} message */
    onError(message) {
      status = "error";
      errorMessage = message;
    },
    /** @param {number} w @param {number} h */
    onSourceDimensions(w, h) {
      sourceWidth = w;
      sourceHeight = h;
      // Flip with the new dimensions in the SAME synchronous block: set a
      // microtask later, a render in between would draw the full-size
      // backing store at the stale draft-to-native ratio for a frame.
      if (uploadingFullTier) fullTierReady = true;
      onSourceDimensions?.(w, h);
    },
  };

  /** @param {HTMLCanvasElement} canvas */
  async function initGpu(canvas) {
    return initGpuImpl(gpu, canvas, gpuHooks);
  }

  function readHistogramIfIdle() {
    return readHistogramIfIdleImpl(gpu, renderInputs);
  }

  /** @param {ImageBitmap} bitmap */
  async function applyBitmapToGpu(bitmap) {
    return applyBitmapToGpuImpl(gpu, bitmap, gpuHooks);
  }

  function writeAdjustmentsAndRender() {
    return writeAdjustmentsAndRenderImpl(gpu, renderInputs);
  }

  /** The preview source with the kept denoised copy mixed in at the current amount, or null when the plain
   * preview applies (amount 0, no copy, not this picture, or the mix could not be built -- a failed mix
   * must never stop the photo loading).
   * @param {string} path @param {boolean} full */
  async function loadDenoised(path, full) {
    if (!(aiDenoiseAmount > 0) || !imageContentHash) return null;
    try {
      return await getDenoisedDevelopPreview(path, imageContentHash, aiDenoiseAmount, full);
    } catch {
      return null;
    }
  }

  /** Re-uploads the source texture when the amount (or the kept copy) changed since the last upload. Called
   * by the settle effect below, never per slider step. */
  async function applyDenoiseSource() {
    const path = imagePath;
    if (!path || !gpu.device || status !== "ready") return;
    const tier = activeTier;
    const wanted = imageContentHash ? aiDenoiseAmount : 0;
    const a = appliedDenoise;
    if (a.path === path && a.tier === tier && a.amount === wanted && (wanted === 0 || a.version === denoiseVersion)) return;
    const seq = ++denoiseSeq;
    const version = denoiseVersion;
    try {
      const mixed = wanted > 0 ? await loadDenoised(path, tier === "full") : null;
      const source = mixed ?? (tier === "full" ? await getDevelopFullPreview(path, imageContentHash) : await getDevelopPreview(path, imageContentHash));
      if (seq !== denoiseSeq || imagePath !== path || activeTier !== tier) return;
      const bitmap = await createImageBitmap(await (await fetch(convertFileSrc(source.path))).blob());
      if (seq !== denoiseSeq || imagePath !== path || activeTier !== tier) return;
      uploadingFullTier = tier === "full";
      try {
        await applyBitmapToGpu(bitmap);
      } finally {
        uploadingFullTier = false;
      }
      setAppliedDenoise({ path, tier, amount: mixed ? wanted : 0, version });
      writeAdjustmentsAndRender();
    } catch {
      // Keep the texture that is on screen; the next change tries again.
    }
  }

  async function loadImage(/** @type {string} */ path) {
    if (!gpu.device || !gpu.context || !gpu.pipeline || !gpu.preMaskPipeline || !gpu.lensCorrectPipeline || !gpu.perspectivePipeline || !gpu.gradePipeline || !gpu.atmReducePipeline || !gpu.minChannelPipeline || !gpu.minHPipeline || !gpu.minVPipeline || !gpu.dehazeMeanguideHPipeline || !gpu.dehazeMeanguideVPipeline || !gpu.dehazeMeanpHPipeline || !gpu.dehazeMeanpVPipeline || !gpu.dehazeCorrguideHPipeline || !gpu.dehazeCorrguideVPipeline || !gpu.dehazeCorrguidepHPipeline || !gpu.dehazeCorrguidepVPipeline || !gpu.dehazeAPipeline || !gpu.dehazeBPipeline || !gpu.dehazeMeanaHPipeline || !gpu.dehazeMeanaVPipeline || !gpu.dehazeMeanbHPipeline || !gpu.dehazeMeanbVPipeline || !gpu.dehazeRefinePipeline || !gpu.textureHPipeline || !gpu.textureVPipeline || !gpu.clarityMeanpHPipeline || !gpu.clarityMeanpVPipeline || !gpu.clarityCorrpHPipeline || !gpu.clarityCorrpVPipeline || !gpu.clarityAPipeline || !gpu.clarityBPipeline || !gpu.clarityMeanaHPipeline || !gpu.clarityMeanaVPipeline || !gpu.clarityMeanbHPipeline || !gpu.clarityMeanbVPipeline || !gpu.clarityVPipeline || !gpu.sharpenMeanpHPipeline || !gpu.sharpenMeanpVPipeline || !gpu.sharpenCorrpHPipeline || !gpu.sharpenCorrpVPipeline || !gpu.sharpenAPipeline || !gpu.sharpenBPipeline || !gpu.sharpenMeanaHPipeline || !gpu.sharpenMeanaVPipeline || !gpu.sharpenMeanbHPipeline || !gpu.sharpenMeanbVPipeline || !gpu.sharpenFinalPipeline || !gpu.lumaNRMeanpHPipeline || !gpu.lumaNRMeanpVPipeline || !gpu.lumaNRCorrpHPipeline || !gpu.lumaNRCorrpVPipeline || !gpu.lumaNRAPipeline || !gpu.lumaNRBPipeline || !gpu.lumaNRMeanaHPipeline || !gpu.lumaNRMeanaVPipeline || !gpu.lumaNRMeanbHPipeline || !gpu.lumaNRMeanbVPipeline || !gpu.lumaNRFinalPipeline || !gpu.colorNRMeanguideHPipeline || !gpu.colorNRMeanguideVPipeline || !gpu.colorNRCorrguideHPipeline || !gpu.colorNRCorrguideVPipeline || !gpu.colorNRMeanpHPipeline || !gpu.colorNRMeanpVPipeline || !gpu.colorNRCorrguidepHPipeline || !gpu.colorNRCorrguidepVPipeline || !gpu.colorNRAPipeline || !gpu.colorNRBPipeline || !gpu.colorNRMeanaHPipeline || !gpu.colorNRMeanaVPipeline || !gpu.colorNRMeanbHPipeline || !gpu.colorNRMeanbVPipeline || !gpu.colorNRFinalPipeline || !gpu.uniformBuffer || !gpu.masksBuffer || !gpu.curveLutBuffer || !gpu.hslBandsBuffer || !gpu.splitToningBuffer || !gpu.vignetteBuffer || !gpu.lensCorrectionBuffer || !gpu.perspectiveBuffer || !gpu.grainBuffer || !gpu.sharpenBuffer || !gpu.lumaNRBuffer || !gpu.colorNRBuffer || !gpu.clippingBuffer) return;
    status = "loading";
    errorMessage = "";
    // A genuinely new image -- any 1:1 tier state belonged to whatever was
    // loaded before and is meaningless here.
    fullTierPath = null;
    fullTierPromise = null;
    activeTier = "draft";
    fullTierReady = false;

    const preview = await getDevelopPreview(path, imageContentHash);
    isSmartPreview = preview.is_smart_preview;
    navThumbUrl = convertFileSrc(preview.path);
    // The photo may already have a kept denoised copy at a non-zero amount: start from the mix.
    const mixed = await loadDenoised(path, false);
    const response = await fetch(convertFileSrc((mixed ?? preview).path));
    const bitmap = await createImageBitmap(await response.blob());
    await applyBitmapToGpu(bitmap);
    setAppliedDenoise({ path, tier: "draft", amount: mixed ? aiDenoiseAmount : 0, version: denoiseVersion });

    status = "ready";
    await tick(); // overlayEl only mounts once status flips to "ready"
    syncOverlayPosition();
    writeAdjustmentsAndRender();
  }

  /** M5 Slice 1: the CPU-fallback counterpart of `loadImage` above -- there
   * is no GPU device to upload a bitmap to, so this only needs the source's
   * true pixel dimensions (for `onSourceDimensions`, e.g. crop-aspect-ratio
   * math elsewhere), NOT the pixel bytes themselves. `getDevelopPreview`
   * is a pure, unedited decode either way (see its own doc comment) --
   * reusing it here costs nothing new; the actual graded/cropped image the
   * fallback banner shows comes from `cpuFallbackPreviewUrl`, a prop
   * `+page.svelte` populates via `previewEditStack` (mirrors how
   * `softProofPreviewUrl` is computed there), NOT fetched by this
   * component -- this component has no access to the full `EditStack`
   * object, only decomposed per-field props. Best-effort: a failure here
   * (e.g. the source file itself is unreadable) still leaves the fallback
   * banner and `cpuFallbackPreviewUrl` working; only the aspect-ratio math
   * that depends on real source dimensions would be affected, and that's
   * moot anyway while Crop is disabled in fallback mode. */
  async function loadCpuFallbackDimensions(/** @type {string} */ path) {
    try {
      const preview = await getDevelopPreview(path, imageContentHash);
      sourceWidth = preview.width;
      sourceHeight = preview.height;
      onSourceDimensions?.(preview.width, preview.height);
    } catch {
      // best-effort only, see doc comment above
    }
  }

  /** Lazily fetches and swaps in the native-resolution 1:1 tier for the
   * CURRENTLY loaded image, the first time it's zoomed past what the draft
   * tier can show (triggered by the `needsFullTier` $effect, not called
   * directly from pointer handlers) -- matches real Lightroom's own lazy
   * 1:1-preview-build behavior, and this request's own trigger ("when the
   * image zooms in").
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
      const mixed = await loadDenoised(path, true);
      const preview = mixed ?? (await getDevelopFullPreview(path, imageContentHash));
      // The user may have switched images or zoomed back out while this
      // was in flight -- only apply if still relevant, otherwise this
      // would silently stomp whatever loadImage/a later upgrade already
      // put in place.
      if (imagePath !== path || !isZoomed || !gpu.device) return;
      const response = await fetch(convertFileSrc(preview.path));
      const bitmap = await createImageBitmap(await response.blob());
      if (imagePath !== path || !isZoomed) return; // re-check post-decode too
      uploadingFullTier = true;
      try {
        await applyBitmapToGpu(bitmap);
      } finally {
        uploadingFullTier = false;
      }
      fullTierPath = path;
      activeTier = "full";
      setAppliedDenoise({ path, tier: "full", amount: mixed ? aiDenoiseAmount : 0, version: denoiseVersion });
      // No re-centre needed: `onSourceDimensions` above flipped `fullTierReady`
      // together with the new backing-store size, so `natRatio` becomes 1
      // exactly as the backing store grows to native -- the canvas keeps its
      // on-screen size and the view stays where the user left it.
      writeAdjustmentsAndRender();
    })();
    try {
      await fullTierPromise;
    } finally {
      fullTierPromise = null;
    }
  }

  $effect(() => {
    const path = imagePath;
    const canvas = canvasEl;
    if (!path || !canvas) return;
    zoom = FIT;
    scrollPos = { left: 0, top: 0 };

    (async () => {
      // M5 Slice 1: only a failure to ACQUIRE the GPU device itself (no
      // `navigator.gpu`, no adapter, or `requestDevice()` rejecting --
      // `initGpu`'s own three throw/rejection sites) routes into CPU
      // fallback. A shader-compile failure or a later `uncapturederror`
      // does NOT throw from `initGpu` -- both mean WebGPU itself works
      // fine and this is a real app/shader bug, so they keep setting
      // `status = "error"` exactly as before this slice (see their own
      // doc comments inside `initGpu`), never masked as an environment gap.
      if (!gpu.device) {
        try {
          await initGpu(canvas);
        } catch (/** @type {any} */ e) {
          status = "cpu-fallback";
          fallbackInfo = classifyGpuFailure(e);
          onGpuFallback?.(true);
          await loadCpuFallbackDimensions(path);
          return;
        }
      }
      onGpuFallback?.(false);
      try {
        await loadImage(path);
      } catch (/** @type {any} */ e) {
        status = "error";
        errorMessage = String(e && e.stack ? e.stack : e);
      }
    })();
  });

  /** M5 Slice 3 (GPU performance validation): rolling buffer of real
   * slider-edit -> GPU-submitted-work-complete render latencies, in ms.
   * Read by `e2e/specs/develop-performance.e2e.js` via `window`, not used
   * by the UI itself -- this is instrumentation, not a feature. Timed from
   * the reactive effect's own fire (immediately after the bound
   * `$state` change a slider's `oninput` handler makes) rather than from
   * the DOM input event itself, so it excludes input-dispatch and Svelte's
   * state-to-effect-flush overhead -- both sub-millisecond in practice,
   * see the perf spec's own doc comment -- and measures the actual
   * GPU-bound cost of the render pipeline, which dominates it. */
  let renderLatencySamples = /** @type {{ ts: number, ms: number }[]} */ ([]);
  const RENDER_LATENCY_SAMPLE_CAP = 100;

  function recordRenderLatency(/** @type {number} */ startedAt) {
    const capturedDevice = gpu.device;
    if (!capturedDevice) return;
    capturedDevice.queue.onSubmittedWorkDone().then(() => {
      renderLatencySamples.push({ ts: Date.now(), ms: performance.now() - startedAt });
      if (renderLatencySamples.length > RENDER_LATENCY_SAMPLE_CAP) renderLatencySamples.shift();
      if (typeof window !== "undefined") /** @type {any} */ (window).__developRenderPerf = renderLatencySamples;
    });
  }

  $effect(() => {
    // Re-run whenever an adjustment or the mask list changes -- reads, not
    // a re-fetch. selectedMaskId/showMaskOverlay are included specifically
    // for the mask overlay: selecting a DIFFERENT mask, or toggling the
    // overlay, needs a re-render even when nothing else about the image or
    // its masks has changed.
    void exposure;
    void contrast;
    void saturation;
    void temperature;
    void tint;
    void highlights;
    void shadows;
    void whites;
    void blacks;
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
    void lensCorrection;
    void perspective;
    void grain;
    void sharpen;
    void lumaNR;
    void colorNR;
    void showClippingOverlay;
    void showOriginal;
    if (status === "ready") {
      const startedAt = performance.now();
      writeAdjustmentsAndRender();
      recordRenderLatency(startedAt);
    }
  });

  // Before/after preview: a transient "Original"/"Edited" badge, shown
  // whenever `showOriginal` actually CHANGES (via the \ hotkey) rather than
  // persistently -- per explicit user request ("when press hot-key it
  // should show original/edit label on image"), so a glance confirms which
  // state you just landed in without permanently occupying screen space.
  // `firstShowOriginalRun` skips the label on initial mount (this effect's
  // own first pass, when `showOriginal` merely takes on its starting value
  // rather than being toggled by the user) -- otherwise every freshly
  // opened photo would flash the badge once for no reason.
  let beforeAfterLabelVisible = $state(false);
  let firstShowOriginalRun = true;
  $effect(() => {
    void showOriginal;
    if (firstShowOriginalRun) {
      firstShowOriginalRun = false;
      return;
    }
    beforeAfterLabelVisible = true;
    const timer = setTimeout(() => {
      beforeAfterLabelVisible = false;
    }, 1200);
    return () => clearTimeout(timer);
  });

  // Crop & Straighten (M3): the committed-crop CSS preview (rotate +
  // clip-via-overflow, see the markup's own doc comment) shows ONLY when
  // nothing that depends on FULL-image coordinates might be actively
  // happening -- not just "no tool active": every mask's own handles are
  // ALWAYS rendered/interactive whenever the mask-overlay is shown
  // (regardless of `activeTool`, see the `{#each masks as mask}` block),
  // so `selectedMaskId` alone (set by clicking a mask CHIP in the tool
  // strip, independent of `activeTool`) must also gate this -- otherwise
  // selecting an existing mask while a crop is committed would leave its
  // handles rendered in the wrong place (full-image coordinates) under a
  // rotated, clipped canvas, with no way back to the interactive view.
  let showCommittedCropPreview = $derived(
    status === "ready" && activeTool === null && !selectedMaskId && !isCropIdentity(crop),
  );

  // ---- Zoom geometry --------------------------------------------------
  //
  // `scale` is CSS px per NATIVE source pixel. The canvas backing store is
  // not native while only the draft tier is loaded (it is capped at
  // DEVELOP_PREVIEW_MAX_DIMENSION), so `natRatio` = native px per backing px
  // converts between them. The native size comes from the catalog
  // (`nativeWidth/Height`) and only its long edge is trusted -- orientation
  // metadata could swap the two, and the aspect ratio must always come from
  // the decoded bitmap or the image would stretch. Once the full tier is
  // uploaded the backing store IS native and the ratio is exactly 1, so
  // finishing the upgrade changes sharpness, not on-screen size (no re-centre
  // needed). With no catalog size, or while showing a Smart Preview (the
  // original is unreachable, so the draft is all there is), the backing store
  // is treated as native.
  let natRatio = $derived.by(() => {
    const backingLong = Math.max(sourceWidth, sourceHeight);
    if (fullTierReady || isSmartPreview || !backingLong) return 1;
    const nativeLong = Math.max(nativeWidth ?? 0, nativeHeight ?? 0);
    return nativeLong > backingLong ? nativeLong / backingLong : 1;
  });
  /** What is shown in the viewport, in native px: the cropped region while a
   * committed crop is previewed, otherwise the whole photo. */
  let contentNative = $derived({
    w: sourceWidth * natRatio * (showCommittedCropPreview ? crop.width : 1),
    h: sourceHeight * natRatio * (showCommittedCropPreview ? crop.height : 1),
  });
  let fitS = $derived(
    fitScale(contentNative.w, contentNative.h, wrapWidth - CROP_CLIP_PADDING_PX * 2, wrapHeight - CROP_CLIP_PADDING_PX * 2),
  );
  let scale = $derived(effectiveScale(zoom, fitS));
  /** On-screen size of the content box (the canvas, or `.crop-clip`). 0 until
   * the wrap has been measured, which leaves the CSS auto-sizing in charge. */
  let contentCss = $derived(
    wrapWidth && wrapHeight && sourceWidth && sourceHeight
      ? { w: contentNative.w * scale, h: contentNative.h * scale }
      : { w: 0, h: 0 },
  );
  /** The canvas's own on-screen size. The canvas always holds the WHOLE image
   * (a committed crop only clips/offsets it inside `.crop-clip`), so this is
   * the uncropped size at the same scale. */
  let canvasCss = $derived(
    contentCss.w ? { w: sourceWidth * natRatio * scale, h: sourceHeight * natRatio * scale } : { w: 0, h: 0 },
  );
  // Inline placement of the canvas inside `.crop-clip` for a committed crop
  // (percentages of the clip box, so scale-invariant); shared with the
  // navigator thumbnail so it shows exactly the cropped view.
  let cropPlacementStyle = $derived(
    `width:${100 / crop.width}%; height:${100 / crop.height}%; left:${(-crop.x * 100) / crop.width}%; top:${(-crop.y * 100) / crop.height}%; transform: rotate(${crop.angle}deg);`,
  );
  let viewOffset = $derived(contentOffset({ w: wrapWidth, h: wrapHeight }, contentCss, CROP_CLIP_PADDING_PX));
  /** The navigator's reference rectangle -- null when the whole content is
   * visible (Fit, or zoomed out far enough to fit), where it would say
   * nothing. */
  let navRegion = $derived.by(() => {
    if (!isZoomed || !contentCss.w) return null;
    const r = visibleRegion(
      { scrollLeft: scrollPos.left, scrollTop: scrollPos.top },
      { w: wrapWidth, h: wrapHeight },
      viewOffset,
      contentCss,
    );
    return r.w >= 0.999 && r.h >= 0.999 ? null : r;
  });

  /** Run a zoom action. `at` is the viewport-relative point (px, from the
   * wrap's top-left) to hold still while the scale changes -- the cursor for
   * wheel/pinch/click, the viewport centre for buttons. Everything that
   * changes the scale funnels through here. */
  function applyZoom(
    /** @type {import('$lib/zoomMath.js').ZoomAction} */ action,
    /** @type {{x: number, y: number} | undefined} */ at,
  ) {
    if (!wrapEl) return;
    const next = reduceZoom(zoom, action, fitS);
    if (next.mode === zoom.mode && (next.mode === "fit" || (zoom.mode === "zoom" && next.scale === zoom.scale))) return;
    const anchor = at ?? { x: wrapEl.clientWidth / 2, y: wrapEl.clientHeight / 2 };
    // The image point under `anchor` BEFORE the change, in content space.
    const focus = focusAtViewportPoint(
      anchor,
      { scrollLeft: wrapEl.scrollLeft, scrollTop: wrapEl.scrollTop },
      viewOffset,
      contentCss.w ? contentCss : { w: 1, h: 1 },
    );
    zoom = next;
    flushSync(); // size/class changes must be in the DOM before scroll is set
    if (next.mode === "fit") {
      wrapEl.scrollLeft = 0;
      wrapEl.scrollTop = 0;
    } else {
      setScrollForFocus(focus, anchor);
    }
    syncScrollPos();
  }

  // Keyboard zoom: DevelopModule forwards `develop.zoomRequest`; each new
  // request object is applied once, about the viewport centre.
  $effect(() => {
    const req = zoomRequest;
    if (!req) return;
    untrack(() => {
      if (status === "ready") applyZoom(req.action);
    });
  });

  /** Scroll so image point `focus` (normalized, content space) lands at
   * viewport point `at`. Reads the post-change `viewOffset`/`contentCss`. */
  function setScrollForFocus(/** @type {{x:number,y:number}} */ focus, /** @type {{x:number,y:number}} */ at) {
    if (!wrapEl) return;
    const { scrollLeft, scrollTop } = scrollForFocus(focus, at, viewOffset, contentCss);
    wrapEl.scrollLeft = scrollLeft;
    wrapEl.scrollTop = scrollTop;
  }

  function syncScrollPos() {
    if (!wrapEl) return;
    scrollPos = { left: wrapEl.scrollLeft, top: wrapEl.scrollTop };
  }

  /** Navigator click/drag: centre the view on a normalized image point. */
  function navigateTo(/** @type {{x:number,y:number}} */ focus) {
    if (!wrapEl) return;
    setScrollForFocus(focus, { x: wrapEl.clientWidth / 2, y: wrapEl.clientHeight / 2 });
    syncScrollPos();
  }

  /** `.crop-clip`'s own box size: the crop's region at the current scale,
   * whichever mode. Fit is an object-fit:contain-style box (the largest box
   * of the crop's own aspect ratio inside the wrap's padded content area,
   * see `fitS`) and centred by the wrapper's own `margin:auto` once both
   * dimensions are explicit (a definite width/height, unlike
   * `aspect-ratio` alone, reliably centers via auto margins in every
   * engine -- see wrapWidth/wrapHeight's own doc comment for why this is
   * computed in JS at all instead of left to CSS). Zoomed, the same box
   * simply grows with the scale; the canvas's percentage-based inline
   * positioning inside it (unchanged either way -- see the markup's own
   * doc comment) is scale-invariant, so it lands on the right scale
   * automatically. `.crop-clip.active.zoomed`'s own CSS rule
   * (`max-width/max-height: none`) is what lets this box actually exceed
   * the wrap's available area so `.canvas-wrap`'s existing `overflow: auto`
   * (toggled by the same `isZoomed` condition) has something real to
   * scroll. */
  let cropClipSize = $derived(showCommittedCropPreview && sourceWidth && sourceHeight ? contentCss : { w: 0, h: 0 });

  // Full-resolution tier trigger -- fires for every way the zoom can change
  // (the actions all just set `zoom`, so no call site needs to know about
  // the tier upgrade at all). Wanted once an explicit zoom draws the photo
  // larger than the draft backing store (`scale * natRatio > 1` css px per
  // backing px); when the native size is unknown (`natRatio` stuck at 1) any
  // explicit zoom upgrades, as before. Guarded on activeTier so it's a
  // no-op once already upgraded for this image, and on status==="ready" so
  // it can't fire before loadImage has finished its own initial setup. Also
  // skipped entirely while showing a Smart Preview fallback (M4) -- the 1:1
  // tier needs the same unreachable source file the draft tier just failed
  // to read, so attempting it would just fail again; the draft tier's
  // DEVELOP_PREVIEW_MAX_DIMENSION cap is the effective zoom ceiling while
  // offline.
  let needsFullTier = $derived(
    isZoomed && !isSmartPreview && (!(nativeWidth && nativeHeight) || scale * natRatio > 1 + 1e-3),
  );
  $effect(() => {
    if (status === "ready" && needsFullTier && activeTier !== "full") {
      upgradeToFullTier(imagePath);
    }
  });
  // AI Denoise: rebuild the source texture once the amount has settled (a slider drag changes it every
  // frame; the mix is a file built by the backend), or when the kept copy was made or removed.
  $effect(() => {
    void aiDenoiseAmount;
    void denoiseVersion;
    if (status !== "ready" || !imagePath) return;
    const timer = setTimeout(applyDenoiseSource, 250);
    return () => clearTimeout(timer);
  });
</script>

<!-- `.canvas-stage` is the non-scrolling frame: `.canvas-wrap` inside it
     scrolls when zoomed, and the zoom HUD sits beside it (not in it) so it
     stays fixed in the corner instead of scrolling away with the photo. -->
<div class="canvas-stage">
<div
  class="canvas-wrap"
  class:zoomed={isZoomed}
  bind:this={wrapEl}
  onscroll={syncScrollPos}
>
  <!-- Crop & Straighten (M3): `.crop-clip` ALWAYS wraps the canvas (a
       stable DOM structure, never conditionally created/destroyed around
       the canvas element itself -- doing so would tear down and recreate
       the WebGPU context on every tool-selection change, since a fresh
       <canvas> node has none). `display:contents` when inactive removes
       the wrapper from the layout tree entirely, so the canvas behaves
       EXACTLY as before this feature existed (same offsetParent, same
       syncOverlayPosition math) whenever the committed-crop preview isn't
       showing. Only when `showCommittedCropPreview` is true does it
       become a real `overflow:hidden` box sized to the CROPPED aspect
       ratio, with the canvas absolutely positioned/sized/rotated inside
       it via inline styles (percentages relative to THIS wrapper, not
       the canvas's own size -- see the exact derivation in this file's
       own module-level comment above `showCommittedCropPreview`). That
       percentage-based canvas positioning is scale-invariant -- it works
       out to true 1:1 native-pixel scale whenever THIS wrapper itself is
       sized to the crop's own native pixel dimensions, not just when it's
       fit-scaled -- so `cropClipSize` growing with the zoom scale (see that
       variable's doc comment) is the ONLY piece that needed to change to
       support 100%-zoom scrolling in the cropped view; `class:zoomed`
       here pairs with `.crop-clip.active.zoomed`'s own CSS rule to let
       the box actually grow past the wrap's available area so there's
       something for `.canvas-wrap`'s own `overflow:auto` (same
       `isZoomed` condition) to scroll. -->
  <div
    class="crop-clip"
    class:active={showCommittedCropPreview}
    class:zoomed={isZoomed}
    style={showCommittedCropPreview ? `width:${cropClipSize.w}px; height:${cropClipSize.h}px;` : ""}
  >
    <canvas
      bind:this={canvasEl}
      data-denoise-source={shownDenoise}
      class:zoomed={isZoomed}
      class:sized={!showCommittedCropPreview && canvasCss.w > 0}
      class:cropped={showCommittedCropPreview}
      class:placing={activeTool === "linear_gradient" || activeTool === "radial_gradient" || activeTool === "brush" || activeTool === "color_range" || activeTool === "eyedropper" || activeTool === "spot" || activeTool === "red_eye" || activeTool === "segment"}
      class:space-pan={spacePanning}
      class:hidden={status === "cpu-fallback"}
      style={showCommittedCropPreview
        ? cropPlacementStyle
        : (canvasCss.w > 0 ? `width:${canvasCss.w}px; height:${canvasCss.h}px;` : "") +
          (activeTool === "crop" ? `transform: rotate(${crop.angle}deg);` : "")}
      onpointerdown={handlePointerDown}
      onpointermove={handlePointerMove}
      onpointerup={handlePointerUp}
      onwheel={handleWheel}
      onpointerleave={() => {
        brushCursor = null;
        onHoverPixel?.(null);
      }}
    ></canvas>
  </div>
  {#if status === "ready" && !showCommittedCropPreview}
    <!-- M3 Slice 5: a sibling of canvas, NOT a child of a sizing wrapper
         (see the fix note near syncOverlayPosition) -- its left/top/width/
         height are set directly in JS from canvas's own (correctly
         intrinsic-sized) rendered box via ResizeObserver, then mask
         geometry is positioned with CSS percentages relative to THIS box.
         pointer-events:none on the overlay itself so it never swallows
         pan/zoom/placement drags meant for the canvas beneath it -- only
         the individual handle buttons opt back in. -->
    <div class="mask-overlay" bind:this={overlayEl}>
      <!-- M4 Slice 2: `maskOverlaysVisible` gates every piece of mask chrome
           (outlines, handles, pins, cursors) so a user can review the
           actual graded result without edit-tool UI in the way, toggled via
           MaskToolStrip's eye button or the H hotkey. Deliberately doesn't
           gate the crop grid below (a different tool's own overlay, shown
           only while actively cropping, not a persistent mask pin). -->
      <!-- While the Crop tool straightens, the canvas gets a live CSS rotate
           (see its inline style) but this overlay's own box never does, so
           mask pins/lines/rings (Red Eye, Spot, gradients, brush shapes...)
           stayed at their unrotated screen positions while the photo
           turned under them -- a reported bug. Rotating this wrapper with
           the SAME angle about the SAME center (it fills the overlay,
           which is exactly the canvas's layout box) keeps every mask
           anchored to the photo content. Only the mask chrome rotates:
           the crop rect, dimming and handles below stay fixed, and no
           mask is edited while the Crop tool is the active tool, so no
           pointer math ever runs through the rotated space. -->
      <div class="mask-rotor" style={activeTool === "crop" && crop.angle ? `transform: rotate(${crop.angle}deg);` : ""}>
      {#if maskOverlaysVisible}
      {#each paintTargets as mask (mask.id)}
        {#if mask.op === "linear_gradient_mask"}
          {@const fl = linearFeatherLines(mask)}
          <svg class="mask-line" class:selected={mask.id === editingId}>
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
            class:selected={mask.id === editingId}
            style="left:{mask.start.x * 100}%; top:{mask.start.y * 100}%"
            aria-label="Gradient start"
            onpointerdown={(e) => handleMaskHandlePointerDown(e, mask.id, "start")}
            onpointermove={handleMaskHandlePointerMove}
            onpointerup={handleMaskHandlePointerUp}
          ></button>
          <button
            class="mask-handle"
            class:selected={mask.id === editingId}
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
          <svg class="mask-ellipse" class:selected={mask.id === editingId}>
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
            class:selected={mask.id === editingId}
            style="left:{mask.center.x * 100}%; top:{mask.center.y * 100}%"
            aria-label="Radial center"
            onpointerdown={(e) => handleMaskHandlePointerDown(e, mask.id, "center")}
            onpointermove={handleMaskHandlePointerMove}
            onpointerup={handleMaskHandlePointerUp}
          ></button>
          <button
            class="mask-handle"
            class:selected={mask.id === editingId}
            style="left:{(mask.center.x + mask.radiusX) * 100}%; top:{mask.center.y * 100}%"
            aria-label="Radial radius"
            onpointerdown={(e) => handleMaskHandlePointerDown(e, mask.id, "radius", mask.center)}
            onpointermove={handleMaskHandlePointerMove}
            onpointerup={handleMaskHandlePointerUp}
          ></button>
        {:else if mask.op === "red_eye_mask"}
          <!-- M4: same center/radius handle pair as Radial above (both
               "center" and "radius" drag kinds are already fully generic
               in handleMaskHandlePointerMove -- they patch whichever mask
               id they're given, with no op-specific branching), just no
               feather-range indicator rings (a named scope cut, not an
               oversight: red eye's own "how strict is red detection"
               control is Pupil Size, not spatial feather width, so a
               feather-boundary visualization would be misleading here). -->
          <svg class="mask-ellipse" class:selected={mask.id === editingId}>
            <ellipse cx="{mask.center.x * 100}%" cy="{mask.center.y * 100}%" rx="{mask.radiusX * 100}%" ry="{mask.radiusY * 100}%" />
          </svg>
          <button
            class="mask-handle"
            class:selected={mask.id === editingId}
            style="left:{mask.center.x * 100}%; top:{mask.center.y * 100}%"
            aria-label="Red eye center"
            onpointerdown={(e) => handleMaskHandlePointerDown(e, mask.id, "center")}
            onpointermove={handleMaskHandlePointerMove}
            onpointerup={handleMaskHandlePointerUp}
          ></button>
          <button
            class="mask-handle"
            class:selected={mask.id === editingId}
            style="left:{(mask.center.x + mask.radiusX) * 100}%; top:{mask.center.y * 100}%"
            aria-label="Red eye radius"
            onpointerdown={(e) => handleMaskHandlePointerDown(e, mask.id, "radius", mask.center)}
            onpointermove={handleMaskHandlePointerMove}
            onpointerup={handleMaskHandlePointerUp}
          ></button>
        {:else if mask.op === "spot_mask"}
          <!-- M4 Slice 2 (brush-like spot removal, replacing the original
               single dest-circle model): the painted stroke itself has no
               persistent outline overlay -- same precedent as `brush_mask`
               above, which shows no outline either once a stroke is
               committed (a multi-dab union has no simple SVG shape the way
               one circle did). Instead: a dashed circle at the stroke's
               centroid + `sourceOffset` (approximating, at the dabs'
               average radius, what content the WHOLE stroke samples from)
               plus a link line to the stroke's own centroid, a "move"
               handle at the centroid (drag to translate every dab -- see
               `handleMaskHandlePointerMove`'s own `spot_move` doc comment),
               and a source-offset handle. -->
          {@const c = spotCentroidAndRadius(mask.dabs)}
          {@const sx = c.x + mask.sourceOffset.dx}
          {@const sy = c.y + mask.sourceOffset.dy}
          <svg class="mask-ellipse spot" class:selected={mask.id === editingId}>
            <line x1="{c.x * 100}%" y1="{c.y * 100}%" x2="{sx * 100}%" y2="{sy * 100}%" class="spot-link" />
            <ellipse cx="{sx * 100}%" cy="{sy * 100}%" rx="{c.avgRadius * 100}%" ry="{spotRyPercent(c.avgRadius)}%" class="spot-source" />
          </svg>
          <button
            class="mask-handle spot-move-handle"
            class:selected={mask.id === editingId}
            style="left:{c.x * 100}%; top:{c.y * 100}%"
            aria-label="Move spot"
            onpointerdown={(e) => handleMaskHandlePointerDown(e, mask.id, "spot_move", { x: c.x, y: c.y }, mask.dabs)}
            onpointermove={handleMaskHandlePointerMove}
            onpointerup={handleMaskHandlePointerUp}
          ></button>
          <button
            class="mask-handle spot-source-handle"
            class:selected={mask.id === editingId}
            style="left:{sx * 100}%; top:{sy * 100}%"
            aria-label="Spot source"
            onpointerdown={(e) => handleMaskHandlePointerDown(e, mask.id, "spot_source_offset", { x: c.x, y: c.y })}
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
      {:else if placingMask?.kind === "red_eye"}
        <svg class="mask-ellipse placing">
          <ellipse cx="{placingMask.center.x * 100}%" cy="{placingMask.center.y * 100}%" rx="{placingMask.radiusX * 100}%" ry="{placingMask.radiusY * 100}%" />
        </svg>
      {/if}
      {#if activeTool === "segment"}
        <!-- Select Subject: one dot per click, green = part of it, red = not part of it. -->
        {#each segmentPrompts as p, i (i)}
          <span class="seg-dot" class:neg={!p.positive} style="left:{p.x * 100}%; top:{p.y * 100}%"></span>
        {/each}
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
      {#if activeTool === "spot" && spotCursor}
        <!-- M4 Slice 2: live spot-brush-size cursor, mirroring the Brush
             tool's own cursor above -- shown on hover, not just while
             painting, so size is visible before committing a stroke. -->
        <svg class="brush-cursor">
          <ellipse cx="{spotCursor.x * 100}%" cy="{spotCursor.y * 100}%" rx="{spotBrushSize * 100}%" ry="{spotRyPercent(spotBrushSize)}%" />
        </svg>
      {/if}
      {/if}
      </div>
      {#if activeTool === "crop"}
        <!-- Crop & Straighten (M3): the canvas's own LAYOUT box (offsetLeft/
             Top/Width/Height, which syncOverlayPosition and this overlay's
             own percentage-based geometry are built on) never changes while
             this tool is active -- but a straighten angle now DOES apply a
             live `transform: rotate()` to the canvas for real-time visual
             feedback (see the canvas element's own inline `style` above),
             matching real Lightroom's straighten UX: the crop boundary/grid
             stays fixed in the viewport while the photo content visibly
             rotates underneath it, and this dimmed overlay naturally shows
             what the rotation is about to push outside the frame. A CSS
             `transform` never affects layout geometry, so this overlay and
             every handle position below stay pixel-correct on-screen
             regardless of the live rotation -- and `screenToNormalized`/
             `screenToNativePixel` (handle-drag math) are correct too, via
             `trueElementBox` (see that function's own doc comment): the
             overlay/handles never rotate, so a click on them is already in
             the right space, `getBoundingClientRect()`'s rotated width/
             height/left/top were simply the wrong numbers to read for a
             space that never rotated -- not something that needed
             "un-rotating." The outside-of-crop dimming is ONE spotlight
             element (a huge box-shadow spread around the crop rect), not
             four bands tiling the canvas's layout box: the rotated photo
             sticks out past that layout box at its corners while a
             straighten angle is applied (and the bands, being layout-box
             geometry, never followed it), leaving those corners undimmed
             -- a reported bug. The spread reaches everything the wrap
             clips to, so the whole rotated photo is dimmed outside the
             crop rect. -->
        <div
          class="crop-dim"
          style="left:{crop.x * 100}%; top:{crop.y * 100}%; width:{crop.width * 100}%; height:{crop.height * 100}%"
        ></div>
        <div
          class="crop-rect"
          style="left:{crop.x * 100}%; top:{crop.y * 100}%; width:{crop.width * 100}%; height:{crop.height * 100}%"
          role="presentation"
          onpointerdown={(e) => handleCropHandlePointerDown(e, "move")}
          onpointermove={handleCropHandlePointerMove}
          onpointerup={handleCropHandlePointerUp}
        >
          <svg class="crop-grid" viewBox="0 0 3 3" preserveAspectRatio="none">
            <line x1="1" y1="0" x2="1" y2="3" />
            <line x1="2" y1="0" x2="2" y2="3" />
            <line x1="0" y1="1" x2="3" y2="1" />
            <line x1="0" y1="2" x2="3" y2="2" />
          </svg>
        </div>
        {#each ["nw", "n", "ne", "e", "se", "s", "sw", "w"] as which (which)}
          {@const pos = cropHandlePos(which, crop)}
          <button
            class="crop-handle crop-handle-{which}"
            style="left:{pos[0] * 100}%; top:{pos[1] * 100}%"
            aria-label="Crop handle ({which})"
            onpointerdown={(e) => handleCropHandlePointerDown(e, which)}
            onpointermove={handleCropHandlePointerMove}
            onpointerup={handleCropHandlePointerUp}
          ></button>
        {/each}
      {:else if !isCropIdentity(crop)}
        <!-- Static, non-interactive reference outline (design-review-
             suggested): gives spatial context for where the committed
             crop sits while placing/editing local adjustments or using
             the eyedropper, WITHOUT making the mask system itself
             crop-aware -- masks stay positioned relative to the full
             original image, exactly as before this feature existed (see
             develop_engine.rs's own `apply_crop` doc comment for why
             that's a deliberate, correct design choice, not a shortcut). -->
        <div
          class="crop-reference"
          style="left:{crop.x * 100}%; top:{crop.y * 100}%; width:{crop.width * 100}%; height:{crop.height * 100}%"
        ></div>
      {/if}
    </div>
  {/if}
  {#if status === "cpu-fallback" && cpuFallbackPreviewUrl}
    <!-- A padded flex item of the wrap (centred via margin:auto), unlike
         the overlays below, so it stays in the wrap; it never zooms. -->
    <img class="cpu-fallback-image" src={cpuFallbackPreviewUrl} alt="Preview (GPU unavailable)" draggable="false" />
  {/if}
</div>
<!-- Everything below is a sibling of the scroller, not a child: absolutely
     positioned children of `.canvas-wrap` scroll away with the photo once it
     becomes `overflow: auto` (the bug the zoom HUD was moved out for). These
     are viewport-fixed overlays, so they belong to the non-scrolling stage.
     Only the mask overlay stays inside the scroller -- it is positioned
     against the canvas's own box and must scroll with it. -->
  {#if softProofEnabled && softProofPreviewUrl}
    <!-- M4 Soft Proofing: a static, fit-to-view simulation of the CURRENT
         edit rendered against the target ICC profile (see soft_proof.rs),
         computed CPU-side and re-fetched debounced on every edit/settings
         change by +page.svelte -- not the live WGSL canvas, and
         deliberately doesn't track the canvas's own pan/zoom/crop-preview
         transforms (`object-fit: contain` over the whole `.canvas-wrap`
         instead): this is a periodic "how will this look on the target
         device" check, not a second fully-interactive view. `pointer-
         events: none` so it never blocks the tool strip/hotkeys. -->
    <img class="soft-proof-overlay" src={softProofPreviewUrl} alt="Soft proof preview" draggable="false" />
  {/if}
  {#if softProofEnabled}
    <div class="soft-proof-badge" title="Simulating: {softProofProfileLabel}">
      Soft Proof — {softProofProfileLabel}{softProofLoading ? "…" : ""}
    </div>
  {/if}
  {#if beforeAfterLabelVisible}
    <!-- M4 Slice 3: transient before/after badge -- see the effect that
         drives `beforeAfterLabelVisible` for why this is timed, not
         persistent. Rendered as a `.canvas-wrap` sibling (not inside
         `.crop-clip`) so it stays in a fixed screen position regardless of
         `showCommittedCropPreview`'s own rotate/clip transform. -->
    <div class="before-after-label">{showOriginal ? "Original" : "Edited"}</div>
  {/if}
  {#if status === "ready" && isSmartPreview}
    <div
      class="smart-preview-badge"
      title="The original photo file couldn't be found -- it may have been moved, renamed, or is on a disconnected drive. Showing a cached Smart Preview instead; your edits still apply and the full-resolution original will be used again once it's reachable."
    >
      ⚠ Smart Preview — original unavailable
    </div>
  {/if}
  {#if status === "loading"}
    <div class="overlay">Decoding…</div>
  {:else if status === "error"}
    <div class="overlay error">{errorMessage}</div>
  {:else if status === "cpu-fallback"}
    <!-- M5 Slice 1: no WebGPU device could be acquired (see
         `classifyGpuFailure`) -- Develop stays usable via a CPU-rendered
         static preview instead of erroring out entirely. `cpuFallbackPreviewUrl`
         is computed by +page.svelte (debounced on `editStack` changes,
         via `previewEditStack`, mirroring how `softProofPreviewUrl` is
         already computed there) rather than by this component, since only
         +page.svelte holds the full `EditStack` object this needs. -->
    {#if !cpuFallbackPreviewUrl}
      <div class="overlay">Rendering preview…</div>
    {/if}
    <div class="cpu-fallback-badge" title={fallbackInfo?.message ?? ""}>
      ⚠ GPU acceleration unavailable — preview updates after you stop adjusting, not live while dragging
    </div>
  {/if}
{#if status === "ready"}
  <DevelopZoomHud
    {scale}
    isFit={!isZoomed}
    region={navRegion}
    thumbUrl={navThumbUrl}
    thumbImgStyle={showCommittedCropPreview ? cropPlacementStyle : ""}
    aspect={contentNative.h > 0 ? contentNative.w / contentNative.h : 1.5}
    onZoom={(action) => applyZoom(action)}
    onNavigate={navigateTo}
  />
{/if}
</div>

<style>
  /* Non-scrolling frame around `.canvas-wrap`: positions the zoom HUD
     relative to the visible viewport rather than to the scrollable content. */
  .canvas-stage {
    flex: 1;
    display: flex;
    position: relative;
    min-width: 0;
    min-height: 0;
  }
  .canvas-wrap {
    flex: 1;
    display: flex;
    position: relative;
    padding: 22px;
    min-width: 0;
    min-height: 0;
    user-select: none;
    /* Crop & Straighten's live rotation preview (see the canvas element's
       own inline style) rotates the canvas around its own center via CSS
       transform -- a rotated rectangle's bounding box is strictly LARGER
       than the unrotated one on both axes, and this component's canvas
       routinely fills nearly all of this wrap's own available height with
       almost no existing slack (confirmed by measurement: at a modest
       12.5deg angle, the rotated bounding box extended roughly 50px past
       BOTH the top and bottom of this element). Without clipping, that
       spillover visually bleeds into whatever sits above/below Develop's
       canvas area (the titlebar, the filmstrip) -- confirmed as a real,
       reported bug ("the image overlay outside of the view"), not a
       hypothetical. `overflow: hidden` here clips it to exactly this
       wrap's own padded content box, matching real Lightroom's own
       straighten UX (the photo rotates within a fixed viewport, never
       visibly extending past it).
    */
    overflow: hidden;
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
  /* Explicitly sized from the zoom scale (inline width/height): the box is
     already exactly Fit or the zoom size, so the max-* clamp must not
     shrink only one axis. flex-shrink for the same reason. */
  canvas.sized {
    max-width: none;
    max-height: none;
    flex-shrink: 0;
  }
  canvas.placing {
    cursor: crosshair;
  }
  /* Space-to-pan: comes AFTER .placing in source order deliberately, so it
     wins the cursor tie-break and overrides the crosshair while Space is
     held and a placing tool (brush/spot/etc) is still active -- see
     handlePointerDown's own space-pan branch for why the tool stays active
     underneath rather than being switched away. */
  canvas.space-pan {
    cursor: grab;
  }
  canvas.space-pan:active {
    cursor: grabbing;
  }
  /* M5 Slice 1: no WebGPU context is ever configured onto this canvas in
     CPU-fallback mode (see `initGpu`'s own failure classification) -- left
     visible it would just show as a blank default-sized box behind
     `.cpu-fallback-image`. */
  canvas.hidden {
    display: none;
  }
  /* Crop & Straighten (M3): inactive by default (`display:contents`
     removes this wrapper from the layout tree entirely, so canvas's own
     `max-width/max-height:100%; margin:auto` sizing above works exactly
     as it did before this feature existed). `.active` makes it a real
     positioned, clipped box -- see the markup's own doc comment for the
     full derivation of the inline styles this pairs with. */
  .crop-clip {
    display: contents;
  }
  .crop-clip.active {
    display: block;
    position: relative;
    /* width/height are set inline from cropClipSize (JS-computed, see
       that variable's own doc comment for why) -- max-width/max-height
       stay as a defensive fallback only, for the brief window before the
       ResizeObserver's first callback has fired. margin:auto correctly
       centers this box in BOTH axes once width/height are explicit
       pixel values (unlike the old aspect-ratio-only approach, which
       left the cross axis with nothing definite for auto margins to
       center against). `flex-shrink: 0` -- this box is a flex item of
       `.canvas-wrap` (`display:flex`, default row axis), and a plain
       `<div>` has no INTRINSIC aspect ratio the way a replaced element
       (`<canvas>`, `<img>`) does: flexbox's default `flex-shrink: 1`
       squishes only the WIDTH (the main-axis flex-basis) down to fit the
       row when this box's content doesn't fit, leaving the explicit
       HEIGHT untouched -- a real, reported non-uniform stretch once
       `cropClipSize`'s 100%-zoom native-pixel size (see that variable's
       own doc comment) exceeds the wrap's available width. The canvas
       element itself never had this problem (a replaced element's own
       flex-shrink preserves its intrinsic aspect ratio), which is why
       this only ever showed up for the cropped view. */
    flex-shrink: 0;
    max-width: 100%;
    max-height: 100%;
    margin: auto;
    overflow: hidden;
    border-radius: 2px;
    box-shadow: 0 20px 50px -14px rgba(0, 0, 0, 0.7);
  }
  /* 100%-zoom override: `cropClipSize` sizes this box to the crop's own
     native pixel dimensions in this mode (see that variable's own doc
     comment), which routinely EXCEEDS the wrap's available area -- the
     entire point, since that's what makes something worth scrolling. The
     base rule's `max-width/max-height: 100%` above would otherwise clamp
     it right back down to fit, silently undoing the native sizing (the
     same `max-width:none`/`max-height:none` override `canvas.zoomed`
     already applies to the canvas element itself, one rule down). */
  .crop-clip.active.zoomed {
    max-width: none;
    max-height: none;
  }
  canvas.cropped {
    position: absolute;
    max-width: none;
    max-height: none;
    margin: 0;
    box-shadow: none;
    border-radius: 0;
  }
  .mask-overlay {
    position: absolute;
    pointer-events: none;
  }
  .mask-rotor {
    position: absolute;
    inset: 0;
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
  .mask-ellipse.spot .spot-source {
    stroke: rgba(255, 255, 255, 0.35);
    stroke-dasharray: 3 3;
  }
  .mask-ellipse.spot.selected .spot-source {
    stroke: var(--accent);
  }
  .mask-ellipse.spot .spot-link {
    stroke: rgba(255, 255, 255, 0.3);
    stroke-width: 1;
    stroke-dasharray: 2 3;
  }
  .mask-ellipse.spot.selected .spot-link {
    stroke: var(--accent);
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
  /* Select Subject click markers. */
  .seg-dot {
    position: absolute;
    width: 10px;
    height: 10px;
    margin-left: -5px;
    margin-top: -5px;
    border-radius: 50%;
    background: var(--label-green);
    border: 1.5px solid #fff;
    box-shadow: 0 0 0 1px rgba(0, 0, 0, 0.5);
    pointer-events: none;
  }
  .seg-dot.neg {
    background: var(--label-red);
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
  .mask-handle.spot-source-handle {
    width: 9px;
    height: 9px;
    margin-left: -4.5px;
    margin-top: -4.5px;
    background: rgba(255, 255, 255, 0.55);
  }
  .mask-handle.spot-source-handle.selected {
    background: var(--accent);
  }
  .mask-handle.spot-move-handle {
    /* Same size/look as the default `.mask-handle` -- distinguished from
       the smaller `.spot-source-handle` by cursor only, matching the
       `move` cursor convention `.crop-rect` already uses for its own
       whole-shape drag. */
    cursor: move;
  }
  .crop-dim {
    position: absolute;
    box-sizing: border-box;
    /* Spotlight: the element IS the crop rect (left undimmed); the spread
       shadow dims everything around it, including a straightened photo's
       corners outside the canvas's own layout box. `.canvas-wrap`'s
       overflow:hidden clips it to the viewport. */
    box-shadow: 0 0 0 100vmax rgba(0, 0, 0, 0.55);
    pointer-events: none;
  }
  .crop-rect {
    position: absolute;
    box-sizing: border-box;
    border: 1.5px solid rgba(255, 255, 255, 0.9);
    cursor: move;
    pointer-events: auto;
  }
  .crop-grid {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    overflow: visible;
    pointer-events: none;
  }
  .crop-grid line {
    stroke: rgba(255, 255, 255, 0.45);
    stroke-width: 0.01;
    vector-effect: non-scaling-stroke;
  }
  .crop-handle {
    all: unset;
    position: absolute;
    width: 12px;
    height: 12px;
    margin-left: -6px;
    margin-top: -6px;
    background: rgba(255, 255, 255, 0.9);
    border: 1.5px solid rgba(0, 0, 0, 0.5);
    pointer-events: auto;
  }
  .crop-handle-nw, .crop-handle-se { cursor: nwse-resize; }
  .crop-handle-ne, .crop-handle-sw { cursor: nesw-resize; }
  .crop-handle-n, .crop-handle-s { cursor: ns-resize; }
  .crop-handle-e, .crop-handle-w { cursor: ew-resize; }
  .crop-reference {
    position: absolute;
    box-sizing: border-box;
    border: 1px dashed rgba(255, 255, 255, 0.5);
    pointer-events: none;
  }
  /* M4 Soft Proofing: covers the whole canvas area (not just `.crop-clip`'s
     own box -- see the markup's own comment for why this deliberately
     doesn't track the live canvas's pan/zoom/crop-preview transforms),
     `object-fit: contain` to letterbox rather than stretch/crop, above
     every other overlay (mask chrome, before/after label) since it's meant
     to show the true simulated result unobstructed. */
  .soft-proof-overlay {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: contain;
    background: #000;
    pointer-events: none;
    z-index: 3;
  }
  /* Top-right -- `.smart-preview-badge` already owns top-left and
     `.before-after-label` owns top-center, and both can legitimately be
     visible at the same time as this one (an offline photo being soft-
     proofed, or toggling before/after while proofing is on). */
  .soft-proof-badge {
    position: absolute;
    top: 14px;
    right: 14px;
    padding: 5px 10px;
    font-family: var(--font-mono);
    font-size: 10.5px;
    font-weight: 600;
    color: var(--accent);
    background: rgba(20, 18, 16, 0.85);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-s);
    pointer-events: none;
    z-index: 4;
  }
  /* M4 Smart Previews: persistent (unlike `.before-after-label`'s transient
     fade), so top-left rather than top-center -- avoids colliding with
     that label's own top-center spot when both happen to be visible at
     once (e.g. right after toggling the before/after hotkey on an
     offline photo). */
  .smart-preview-badge {
    position: absolute;
    top: 14px;
    left: 14px;
    padding: 5px 10px;
    font-family: var(--font-mono);
    font-size: 10.5px;
    font-weight: 600;
    color: var(--label-yellow);
    background: rgba(20, 18, 16, 0.85);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-s);
    pointer-events: none;
    z-index: 2;
  }
  /* M5 Slice 1: CPU-fallback preview -- the static, debounced-re-rendered
     substitute for the live WGSL canvas, same `.canvas-wrap` box the real
     canvas normally fills (`object-fit: contain` since this is a
     pre-rendered PNG at a fixed draft resolution, not a native-sized
     backing store like the canvas). */
  .cpu-fallback-image {
    max-width: 100%;
    max-height: 100%;
    margin: auto;
    border-radius: 2px;
    box-shadow: 0 20px 50px -14px rgba(0, 0, 0, 0.7);
    object-fit: contain;
  }
  /* Shares `.smart-preview-badge`'s top-left slot's general placement
     style but sits bottom-left instead, so it can't collide with a Smart
     Preview badge (both conditions COULD theoretically be true at once --
     GPU unavailable AND the source file unreachable -- though that's an
     edge case, not one this slice specifically tests). */
  .cpu-fallback-badge {
    position: absolute;
    bottom: 14px;
    left: 14px;
    max-width: calc(100% - 28px);
    padding: 5px 10px;
    font-family: var(--font-mono);
    font-size: 10.5px;
    font-weight: 600;
    color: var(--label-yellow);
    background: rgba(20, 18, 16, 0.85);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-s);
    pointer-events: none;
    z-index: 2;
  }
  /* M4 Slice 3: before/after transient label -- top-center (distinct from
     the zoom HUD's bottom-right corner) so the two never collide, and
     `pointer-events: none` since this is purely informational, never
     interactive. */
  .before-after-label {
    position: absolute;
    top: 14px;
    left: 50%;
    transform: translateX(-50%);
    padding: 5px 14px;
    font-family: var(--font-mono);
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--text-primary);
    background: rgba(20, 18, 16, 0.75);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-s);
    pointer-events: none;
    z-index: 2;
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
