<script>
  import { WGSL } from "$lib/gpu/shaders/index.js";
  import { linearFeatherLines, radialFeatherRadii, spotCentroidAndRadius } from "$lib/maskGeometry.js";
  import { buildAtmLightChainSizes } from "$lib/gpu/atmChain.js";
  import { rasterizeDab, rasterizeSpotDab } from "$lib/gpu/brushRaster.js";
  import { tick } from "svelte";
  import { convertFileSrc } from "@tauri-apps/api/core";
  import { getDevelopPreview, getDevelopFullPreview, buildToneCurveLut, buildHslUniformData, buildSplitToningUniformData, buildVignetteUniformData, buildLensCorrectionUniformData, buildPerspectiveUniformData, buildGrainUniformData, buildSharpenUniformData, buildLumaNrUniformData, buildColorNrUniformData, isCropIdentity } from "$lib/api/develop.js";
  import { clamp01, cropMinFrac, moveCropRect, cropCornerPoints, resizeCropCorner, resizeCropEdge, cropHandlePos, trueElementBox, nativeCropClipSize, scrollTargetForNativeFocus, cropRectFitsRotatedBounds } from "$lib/cropMath.js";
  import { binHistogramPixels } from "$lib/histogramMath.js";
  import { classifyGpuFailure } from "$lib/gpuFallback.js";

  const MAX_MASKS = 8;

  /**
   * @type {{
   *   imagePath: string,
   *   imageContentHash?: string | null,
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
   *   grain: {amount: number, size: number, roughness: number},
   *   sharpen: {amount: number, radius: number, detail: number, masking: number},
   *   lumaNR: {amount: number, detail: number, contrast: number},
   *   colorNR: {amount: number, detail: number},
   *   crop: {x: number, y: number, width: number, height: number, angle: number},
   *   onCropChange: (patch: Partial<{x: number, y: number, width: number, height: number, angle: number}>) => void,
   *   cropAspectLock: number | null,
   *   onSourceDimensions?: (width: number, height: number) => void,
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

  // M3 Slice 3: basic pan/zoom. "fit" is today's existing behavior; "100"
  // is true 1:1 canvas-backing-store pixels, scrollable via the browser's
  // own native scroll clamping rather than hand-rolled pan math.
  let zoomMode = $state("fit"); // "fit" | "100"

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
  // Guards upgradeToFullTier's own re-center (see that function's doc
  // comment) against clobbering a pan the user made WHILE the full-res
  // decode was still in flight -- reset to false every time lastZoomFocus
  // is freshly captured (a new zoom-in click), set true by any actual
  // pan-drag scroll write in the meantime. Without this, a slow full-tier
  // decode (large/portrait source images that exceed the draft tier's
  // DEVELOP_PREVIEW_MAX_DIMENSION cap on their long axis are the real-world
  // trigger, since only those force a genuinely different, non-instant
  // decode+GPU-upload) lets the user start dragging, then has the view
  // silently snap back to the original click point the moment the decode
  // resolves -- visible as jitter/instability, not a single one-time jump.
  let zoomFocusPanned = false;
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
    if (!lastHistogramPixels) {
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
    const bgra = presentationFormat.startsWith("bgra");
    const r = lastHistogramPixels[bgra ? i + 2 : i];
    const g = lastHistogramPixels[i + 1];
    const b = lastHistogramPixels[bgra ? i : i + 2];
    onHoverPixel({ r, g, b });
  }

  /** Scrolls `wrapEl` so a focus point given in FULL-IMAGE native-pixel
   * coordinates (e.g. from `screenToNativePixel`, or `lastZoomFocus`
   * re-applied against the current tier's dimensions) is centered --
   * correctly whether or not a crop is currently committed, via
   * `scrollTargetForNativeFocus` (see that function's own doc comment for
   * why the committed-crop case needs its own coordinate offset: the
   * scrollable box in that state is `.crop-clip`, whose own origin sits
   * away from the canvas's, not the canvas element itself). The one
   * shared call site both this component's own zoom-in-on-click
   * (`handlePointerUp`) and its 1:1-tier re-center
   * (`upgradeToFullTier`) now go through, so the two can't drift apart
   * the way two independent inline copies of this math eventually would. */
  function scrollToNativeFocus(/** @type {number} */ nativeX, /** @type {number} */ nativeY) {
    if (!wrapEl || !canvasEl) return;
    const { scrollLeft, scrollTop } = scrollTargetForNativeFocus(
      nativeX,
      nativeY,
      showCommittedCropPreview,
      crop,
      canvasEl.width,
      canvasEl.height,
      wrapEl.clientWidth,
      wrapEl.clientHeight,
    );
    wrapEl.scrollLeft = scrollLeft;
    wrapEl.scrollTop = scrollTop;
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
      zoomFocusPanned = true; // see this flag's own doc comment
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

  async function handlePointerUp(/** @type {PointerEvent} */ e) {
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
      if (zoomMode === "100") {
        zoomMode = "fit";
        return;
      }
      if (!canvasEl) return;
      // Reuses the same helper crop-handle dragging uses (instead of
      // duplicating the same math inline, as this used to) so both paths
      // stay correct together.
      const { x: nativeX, y: nativeY } = screenToNativePixel(clickPoint.x, clickPoint.y);
      // Stored normalized (0-1), not as a native-pixel point -- the point
      // itself doesn't change resolution, but the canvas's own backing-store
      // size DOES once upgradeToFullTier swaps in the 1:1 tier moments
      // later. A native-pixel value captured here would silently go stale
      // and mis-center once that resize happens; the normalized fraction
      // re-applies correctly against whichever tier's dimensions are
      // current when it's read.
      lastZoomFocus = { x: nativeX / canvasEl.width, y: nativeY / canvasEl.height };
      zoomFocusPanned = false; // fresh focus point -- see this flag's own doc comment
      zoomMode = "100";
      await tick(); // required: $state-triggered DOM patches (the new canvas size) land on a microtask
      scrollToNativeFocus(nativeX, nativeY);
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
    if (activeTool !== "spot" || !onSpotBrushSizeChange) return;
    e.preventDefault();
    const delta = e.deltaY > 0 ? -SPOT_BRUSH_SIZE_STEP : SPOT_BRUSH_SIZE_STEP;
    const next = Math.min(SPOT_BRUSH_SIZE_MAX, Math.max(SPOT_BRUSH_SIZE_MIN, spotBrushSize + delta));
    onSpotBrushSizeChange(next);
  }

  // A trackpad pinch over the canvas doesn't dispatch `wheel` at all -- in
  // WebKit (this app's webview on macOS) it's a separate, nonstandard
  // `gesturestart`/`gesturechange` event pair that natively zooms the whole
  // page unless prevented, which is what a user doing a pinch-style gesture
  // while trying to resize the spot brush would hit: `handleWheel` above
  // never even fires, so its own `preventDefault()` can't help. These
  // aren't part of the DOM standard (no TS lib types, no Svelte `on*`
  // prop), so they're bound imperatively here rather than as a template
  // attribute like `onwheel`.
  $effect(() => {
    if (!canvasEl) return;
    const el = canvasEl;
    /** @param {Event} e */
    const preventIfSpot = (e) => {
      if (activeTool === "spot") e.preventDefault();
    };
    el.addEventListener("gesturestart", preventIfSpot);
    el.addEventListener("gesturechange", preventIfSpot);
    return () => {
      el.removeEventListener("gesturestart", preventIfSpot);
      el.removeEventListener("gesturechange", preventIfSpot);
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
  /** M4 Slice 2: before/after preview's own dedicated pass (fs_original) --
   * see that WGSL function's own doc comment for why this is a separate
   * pipeline rather than a branch inside `pipeline` (fs_mask). */
  /** @type {GPURenderPipeline | null} */
  let originalPipeline = null;
  /** @type {GPUBindGroup | null} */
  let originalBindGroup = null;
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
  // Lens Corrections (M3): a larger flat struct (24 f32s, see the WGSL
  // `LensCorrectionParams` doc comment) than Vignette/Grain's own -- still
  // device-scoped and rewritten every render, same as those.
  /** @type {GPUBuffer | null} */
  let lensCorrectionBuffer = null;
  // Perspective Correction (M4): same device-scoped, own-small-buffer
  // treatment as Vignette/Grain above.
  /** @type {GPUBuffer | null} */
  let perspectiveBuffer = null;
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
  // M4 Slice 1 (Healing/Clone brush): the fs_premask pass's own
  // pipeline/bind group, plus preMaskTex itself -- see preMaskTex's WGSL-
  // side doc comment for the full split reasoning. preMaskTex is
  // per-image (recreated alongside gradedTex, same size), the pipeline/
  // bind-group-SHAPE is device-scoped (created once in initGpu, like
  // `pipeline` itself), but preMaskBindGroup still needs recreating per
  // image since it references preMaskTex's own view target indirectly
  // via the textures it reads (gradedTex etc, same lifecycle as
  // `bindGroup` above).
  /** @type {GPURenderPipeline | null} */
  let preMaskPipeline = null;
  /** @type {GPUBindGroup | null} */
  let preMaskBindGroup = null;
  /** @type {GPUTexture | null} */
  let preMaskTex = null;
  /** @type {GPUTextureFormat} */
  let presentationFormat = "bgra8unorm";

  // Histogram: a fixed 256x256 target, device-scoped (created once in
  // initGpu, unlike every per-image texture above) since a histogram is a
  // statistical sample of the graded output, not something that needs
  // full source resolution -- see readHistogramIfIdle's own doc comment
  // for the full reasoning on why 256x256 specifically. Rendered into
  // using fs_final's OWN existing `pipeline`/`bindGroup` a second time
  // (see writeAdjustmentsAndRender), so no new WGSL entry point, pipeline,
  // or bind group is needed at all -- fs_mask's own `in.uv`-based coord
  // (see that pass's doc comment) proportionally maps this small target
  // across the full preMaskTex, giving a nearest-neighbor 256x256 grid
  // sample of the same graded pixels the main canvas shows, just at a
  // smaller output resolution.
  const HISTOGRAM_SIZE = 256;
  /** @type {GPUTexture | null} */
  let histogramTex = null;
  /** @type {GPUBuffer | null} */
  let histogramReadbackBuffer = null;
  let histogramReadInFlight = false;
  // The most recent histogramTex readback's raw bytes -- kept around
  // (not just its binned form) so hover-RGB lookups (see
  // reportHoverPixel) can index directly into it without a second GPU
  // round-trip. An approximate (256x256, not full-resolution) but
  // genuinely GRADED sample -- unlike sampleSourcePixel's own SOURCE-only
  // sampling, see that function's doc comment for why a true graded
  // readback was previously deferred; this reuses the exact same texture
  // the histogram itself already reads back every render, so no
  // additional GPU work is needed for this feature at all.
  /** @type {Uint8Array | null} */
  let lastHistogramPixels = null;

  // Clipping-overlay toggle: device-scoped, same tiny-padded-uniform
  // treatment as Vignette/Grain/etc.'s own small buffers.
  /** @type {GPUBuffer | null} */
  let clippingBuffer = null;

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
  // Lens Corrections (M3): a NEW pass that runs BEFORE fs_grade, writing
  // into lensCorrectedTex -- gradeBindGroup's own binding 1 is rebound to
  // read lensCorrectedTex instead of sourceTexture (see applyBitmapToGpu),
  // the same "same slot number, different physical texture per bind
  // group" technique already established for lcRgbInput/lcBlurInput, so
  // fs_grade's own WGSL body and inferred layout need no change at all.
  /** @type {GPURenderPipeline | null} */
  let lensCorrectPipeline = null;
  // Perspective Correction (M4): another new pass, chained right after
  // lens correction and before fs_grade -- reads lensCorrectedTex (same
  // "same slot, different physical texture" technique lens correction's
  // own doc comment above describes) and writes perspectiveCorrectedTex,
  // which gradeBindGroup's binding 1 is then rebound to instead.
  /** @type {GPURenderPipeline | null} */
  let perspectivePipeline = null;
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
  // Lens Corrections (M3): fs_lens_correct's own output -- fs_grade reads
  // this instead of sourceTexture directly (see lensCorrectPipeline's own
  // doc comment above). rgba16float for the same reason gradedTex is: no
  // new 8-bit quantization step before grading.
  /** @type {GPUTexture | null} */
  let lensCorrectedTex = null;
  // Perspective Correction (M4): fs_perspective's own output -- fs_grade
  // reads this instead of lensCorrectedTex directly (see
  // perspectivePipeline's own doc comment above).
  /** @type {GPUTexture | null} */
  let perspectiveCorrectedTex = null;
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
  let lensCorrectBindGroup = null;
  /** @type {GPUBindGroup | null} */
  let perspectiveBindGroup = null;
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
   * are drawn onto it (see syncMaskRasterization) -- so a long stroke's
   * per-move cost stays bound by texture resolution/upload cost, not by
   * re-rendering the whole dab list from scratch every time. Reset
   * entirely on every image change (loadImage), since a canvas sized for
   * one image's resolution is meaningless for another.
   * @type {Map<string, { canvas: OffscreenCanvas, ctx: OffscreenCanvasRenderingContext2D, layer: number, dabsDrawn: number, featherDrawn: number, firstDabX: number, firstDabY: number }>} */
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
      fragment: { module, entryPoint: "fs_mask", targets: [{ format: presentationFormat }] },
      primitive: { topology: "triangle-list" },
    });
    // M4 Slice 2: before/after preview (see fs_original's own doc comment).
    originalPipeline = device.createRenderPipeline({
      layout: "auto",
      vertex: { module, entryPoint: "vs_main" },
      fragment: { module, entryPoint: "fs_original", targets: [{ format: presentationFormat }] },
      primitive: { topology: "triangle-list" },
    });
    // M4 Slice 1 (Healing/Clone brush): writes preMaskTex, fs_mask's own
    // input -- see preMaskTex's WGSL-side doc comment for why this had to
    // become its own pass rather than staying fused into fs_final.
    preMaskPipeline = device.createRenderPipeline({
      layout: "auto",
      vertex: { module, entryPoint: "vs_main" },
      fragment: { module, entryPoint: "fs_premask", targets: [{ format: "rgba16float" }] },
      primitive: { topology: "triangle-list" },
    });
    lensCorrectPipeline = device.createRenderPipeline({
      layout: "auto",
      vertex: { module, entryPoint: "vs_main" },
      fragment: { module, entryPoint: "fs_lens_correct", targets: [{ format: "rgba16float" }] },
      primitive: { topology: "triangle-list" },
    });
    perspectivePipeline = device.createRenderPipeline({
      layout: "auto",
      vertex: { module, entryPoint: "vs_main" },
      fragment: { module, entryPoint: "fs_perspective", targets: [{ format: "rgba16float" }] },
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
      size: 64, // 16 x f32 (exposure, contrast, saturation, mask_count, selected_mask_index, dehaze, texture, clarity, temp, tint, highlights, shadows, whites, blacks, pad0, pad1)
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
    lensCorrectionBuffer = device.createBuffer({
      size: 24 * 4, // 24 f32, matches the WGSL LensCorrectionParams struct exactly (no padding needed -- already a multiple of 16 bytes)
      usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
    });
    perspectiveBuffer = device.createBuffer({
      size: 8 * 4, // 8 f32 (5 real fields + 3 padding), matches the WGSL PerspectiveParams struct
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

    // Histogram: RENDER_ATTACHMENT so fs_final can draw into it (see
    // writeAdjustmentsAndRender), COPY_SRC so its contents can be copied
    // out to histogramReadbackBuffer below. Same presentationFormat as the
    // canvas itself -- a render pass's color attachment format must
    // exactly match the pipeline it's used with, and `pipeline` (fs_final)
    // was already created with that target format.
    histogramTex = device.createTexture({
      size: [HISTOGRAM_SIZE, HISTOGRAM_SIZE],
      format: presentationFormat,
      usage: GPUTextureUsage.RENDER_ATTACHMENT | GPUTextureUsage.COPY_SRC,
    });
    // bytesPerRow (256 texels x 4 bytes/texel = 1024) is already a
    // multiple of 256 -- WebGPU's own copyTextureToBuffer alignment
    // requirement -- so no row padding is needed here, unlike a
    // less-conveniently-sized readback would require.
    histogramReadbackBuffer = device.createBuffer({
      size: HISTOGRAM_SIZE * 4 * HISTOGRAM_SIZE,
      usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ,
    });
    clippingBuffer = device.createBuffer({
      size: 4 * 4, // 4 f32 (1 real field + 3 padding), matches the WGSL Clipping struct
      usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
    });
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

  /** Reads back `histogramTex` (rendered as part of writeAdjustmentsAndRender,
   * see that function's own doc comment) and reports 256-bin R/G/B counts
   * via `onHistogramUpdate`. `GPUBuffer.mapAsync` can't be awaited inline
   * inside the render loop without stalling it, so this runs detached,
   * guarded by `histogramReadInFlight` -- both because a buffer that's
   * currently mapped can't be written to again (the render function skips
   * re-copying into it while a read is in flight, see there) and because
   * overlapping reads should simply be DROPPED, not queued: a live
   * histogram only needs to reflect a RECENT frame, not every single one,
   * and queuing would only fall further behind under sustained rapid
   * input (e.g. dragging a slider faster than one readback completes). */
  async function readHistogramIfIdle() {
    if (histogramReadInFlight || !histogramReadbackBuffer) return;
    histogramReadInFlight = true;
    try {
      await histogramReadbackBuffer.mapAsync(GPUMapMode.READ);
      // Copied out via slice(0) BEFORE unmap() -- the ArrayBuffer
      // getMappedRange() returns is detached (zero-length) the instant
      // unmap() runs, so onHistogramUpdate's caller can't be handed a
      // view over it directly.
      const data = new Uint8Array(histogramReadbackBuffer.getMappedRange().slice(0));
      histogramReadbackBuffer.unmap();
      // Kept around (not just passed to onHistogramUpdate) so the pointer
      // hover-readout below can map a screen position to the SAME 256x256
      // downsampled sample the histogram itself is drawn from, without a
      // second GPU round-trip -- see reportHoverPixel.
      lastHistogramPixels = data;
      if (onHistogramUpdate) onHistogramUpdate(binHistogramPixels(data, presentationFormat.startsWith("bgra") ? "bgra" : "rgba"));
    } catch {
      // A stale/aborted map (e.g. the device was torn down mid-await, on
      // unmount) isn't user-visible -- the next render's own call simply
      // tries again.
    } finally {
      histogramReadInFlight = false;
    }
  }

  async function applyBitmapToGpu(/** @type {ImageBitmap} */ bitmap) {
    // Both callers (loadImage, upgradeToFullTier) already only reach here
    // once initGpu has run, but re-asserted here too -- both for a real
    // defensive guard against an unexpected call order, and because
    // TypeScript's null-narrowing from a caller's own guard doesn't carry
    // across a function boundary.
    if (!device || !context || !pipeline || !preMaskPipeline || !lensCorrectPipeline || !perspectivePipeline || !gradePipeline || !atmReducePipeline || !minChannelPipeline || !minHPipeline || !minVPipeline || !meanHPipeline || !meanVPipeline || !textureHPipeline || !textureVPipeline || !clarityHPipeline || !clarityVPipeline || !sharpenHPipeline || !sharpenVPipeline || !lumaNRHPipeline || !lumaNRVPipeline || !colorNRHPipeline || !colorNRVPipeline || !uniformBuffer || !masksBuffer || !curveLutBuffer || !hslBandsBuffer || !splitToningBuffer || !vignetteBuffer || !lensCorrectionBuffer || !perspectiveBuffer || !grainBuffer || !sharpenBuffer || !lumaNRBuffer || !colorNRBuffer || !clippingBuffer) return;

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

    // Lens Corrections (M3): same "recreate whenever the source resolution
    // changes" lifecycle as sourceTexture/brushTextureArray above --
    // fs_lens_correct's own output, read by fs_perspective in place of
    // sourceTexture (see perspectiveBindGroup's own doc comment below).
    lensCorrectedTex?.destroy();
    lensCorrectedTex = device.createTexture({
      size: [bitmap.width, bitmap.height],
      format: "rgba16float",
      usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.RENDER_ATTACHMENT,
    });

    // Perspective Correction (M4): same lifecycle as lensCorrectedTex
    // above -- fs_perspective's own output, read by fs_grade in place of
    // lensCorrectedTex (see gradeBindGroup's own doc comment below).
    perspectiveCorrectedTex?.destroy();
    perspectiveCorrectedTex = device.createTexture({
      size: [bitmap.width, bitmap.height],
      format: "rgba16float",
      usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.RENDER_ATTACHMENT,
    });

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

    // M4 Slice 1 (Healing/Clone brush): same lifecycle/format/size as
    // gradedTex above -- see preMaskTex's own WGSL-side doc comment for
    // why this exists as a real texture rather than staying fused into
    // fs_final's own single pass.
    preMaskTex?.destroy();
    preMaskTex = device.createTexture({
      size: [bitmap.width, bitmap.height],
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

    // Lens Corrections (M3): fs_lens_correct's own bind group reads the
    // TRUE original sourceTexture at binding 1. perspectiveBindGroup below
    // binds a DIFFERENT physical texture (lensCorrectedTex) to that SAME
    // slot number for fs_perspective's own separately-inferred layout --
    // the same "same binding index, different texture per bind group"
    // technique already established for lcRgbInput/lcBlurInput (see that
    // binding's own doc comment), so fs_perspective's WGSL body needs no
    // change at all. gradeBindGroup, in turn, does the same trick again
    // one stage later, binding perspectiveCorrectedTex to binding 1 for
    // fs_grade's own separately-inferred layout.
    lensCorrectBindGroup = gpuDevice.createBindGroup({
      layout: lensCorrectPipeline.getBindGroupLayout(0),
      entries: [
        { binding: 0, resource: sampler },
        { binding: 1, resource: sourceTexture.createView() },
        { binding: 25, resource: { buffer: lensCorrectionBuffer } },
      ],
    });
    // Perspective Correction (M4): same "same slot, different texture"
    // technique -- reads lensCorrectedTex (lens correction's own output)
    // at binding 1, not the true original sourceTexture.
    perspectiveBindGroup = gpuDevice.createBindGroup({
      layout: /** @type {GPURenderPipeline} */ (perspectivePipeline).getBindGroupLayout(0),
      entries: [
        { binding: 0, resource: sampler },
        { binding: 1, resource: lensCorrectedTex.createView() },
        { binding: 28, resource: { buffer: perspectiveBuffer } },
      ],
    });
    // M4 Slice 2: before/after preview (see fs_original's own doc comment).
    originalBindGroup = gpuDevice.createBindGroup({
      layout: /** @type {GPURenderPipeline} */ (originalPipeline).getBindGroupLayout(0),
      entries: [
        { binding: 0, resource: sampler },
        { binding: 1, resource: sourceTexture.createView() },
      ],
    });
    gradeBindGroup = gpuDevice.createBindGroup({
      layout: gradePipeline.getBindGroupLayout(0),
      entries: [
        { binding: 0, resource: sampler },
        { binding: 1, resource: /** @type {GPUTexture} */ (perspectiveCorrectedTex).createView() },
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

    // Pre-mask pass's own bind group (M4 Slice 1): exactly the entry set
    // the OLD single-pass fs_final's own bind group used to carry for its
    // Dehaze/NR/Sharpen/Vignette/Grain half (0,2,3,4,26 removed -- those
    // are mask-loop/clipping-only, now fs_mask's job below).
    preMaskBindGroup = gpuDevice.createBindGroup({
      layout: preMaskPipeline.getBindGroupLayout(0),
      entries: [
        { binding: 2, resource: { buffer: uniformBuffer } },
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

    // Final pass's own bind group -- pruned down (M4 Slice 1) to just
    // what fs_mask itself references now that Dehaze/NR/Sharpen/Vignette/
    // Grain moved into fs_premask above: srcSampler(0, still needed --
    // the mask loop's own brushMasks sample uses it, unrelated to
    // Dehaze), the Adjustments uniform(2, for mask_count/
    // selected_mask_index), masks(3), brushMasks(4), clipping(26), and
    // the NEW preMaskTex(27) -- fs_mask's own input, replacing the direct
    // gradedTex/atmLightFinal/etc. reads this bind group used to carry.
    bindGroup = gpuDevice.createBindGroup({
      layout: pipeline.getBindGroupLayout(0),
      entries: [
        { binding: 0, resource: sampler },
        { binding: 2, resource: { buffer: uniformBuffer } },
        { binding: 3, resource: { buffer: masksBuffer } },
        { binding: 4, resource: brushTextureArray.createView({ dimension: "2d-array" }) },
        { binding: 26, resource: { buffer: clippingBuffer } },
        { binding: 27, resource: preMaskTex.createView() },
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
    sourceWidth = bitmap.width;
    sourceHeight = bitmap.height;
    onSourceDimensions?.(sourceWidth, sourceHeight);
    context.configure({ device, format: presentationFormat, alphaMode: "opaque" });
    // Every remaining bitmap.width/.height read is done -- free it now
    // that both its consumers (the GPU upload and the sample-canvas draw
    // above) are finished with it.
    bitmap.close();
  }

  async function loadImage(/** @type {string} */ path) {
    if (!device || !context || !pipeline || !preMaskPipeline || !lensCorrectPipeline || !perspectivePipeline || !gradePipeline || !atmReducePipeline || !minChannelPipeline || !minHPipeline || !minVPipeline || !meanHPipeline || !meanVPipeline || !textureHPipeline || !textureVPipeline || !clarityHPipeline || !clarityVPipeline || !sharpenHPipeline || !sharpenVPipeline || !lumaNRHPipeline || !lumaNRVPipeline || !colorNRHPipeline || !colorNRVPipeline || !uniformBuffer || !masksBuffer || !curveLutBuffer || !hslBandsBuffer || !splitToningBuffer || !vignetteBuffer || !lensCorrectionBuffer || !perspectiveBuffer || !grainBuffer || !sharpenBuffer || !lumaNRBuffer || !colorNRBuffer || !clippingBuffer) return;
    status = "loading";
    errorMessage = "";
    // A genuinely new image -- any 1:1 tier state belonged to whatever was
    // loaded before and is meaningless here.
    fullTierPath = null;
    fullTierPromise = null;
    activeTier = "draft";

    const preview = await getDevelopPreview(path, imageContentHash);
    isSmartPreview = preview.is_smart_preview;
    const response = await fetch(convertFileSrc(preview.path));
    const bitmap = await createImageBitmap(await response.blob());
    await applyBitmapToGpu(bitmap);

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
      // scroll position -- see lastZoomFocus's own doc comment. In the
      // committed-crop case `.crop-clip`'s own size is a reactive
      // `$derived` style binding (`cropClipSize`), not a direct DOM
      // mutation like the canvas's own backing store -- `tick()` first so
      // the wrapper has actually resized before `scrollToNativeFocus`
      // reads/sets scroll position against it (otherwise the browser
      // clamps the new scrollLeft/scrollTop against the STALE, still
      // fit-sized box).
      //
      // Skipped entirely if the user has already panned since the zoom-in
      // click (zoomFocusPanned) -- see that flag's own doc comment. This
      // decode is async and, for a source whose long axis exceeds the
      // draft tier's own resolution cap, genuinely slow -- long enough for
      // a user to start dragging before it resolves. Recentering
      // unconditionally here would silently snap their pan back to the
      // original click point the instant the decode finishes.
      await tick();
      if (canvasEl && !zoomFocusPanned) {
        scrollToNativeFocus(lastZoomFocus.x * canvasEl.width, lastZoomFocus.y * canvasEl.height);
      }
      writeAdjustmentsAndRender();
    })();
    try {
      await fullTierPromise;
    } finally {
      fullTierPromise = null;
    }
  }

  /** Ensures every brush/spot mask in `masks` has a rasterized texture-
   * array layer, drawing only newly-added dabs onto each mask's own
   * persistent OffscreenCanvas -- never re-rasterizing dabs already drawn,
   * which is what keeps a long stroke's per-move cost O(1) (bound by
   * texture resolution/upload cost, not stroke length). Shared between the
   * two mask kinds (M4 Slice 2 generalized this from brush-only) since
   * both are dab-stroke masks that need the exact same texture-array
   * plumbing, drawing into the SAME shared texture array/layer pool -- the
   * combined MAX_MASKS budget already caps total masks at 8 regardless of
   * kind, so there's always enough room for every dab-stroke mask
   * (brush or spot) to get its own layer.
   *
   * Spot masks have one wrinkle brush masks don't: a spot mask's edge
   * softness is a single mask-level `feather` (not baked per-dab at paint
   * time the way brush's hardness/flow are), so changing it on an
   * EXISTING mask (via MaskEditorPanel's Feather slider) must
   * re-rasterize every already-drawn dab, not just newly-appended ones --
   * `featherDrawn` tracks the feather value last baked into each spot
   * entry's canvas, reusing the same "dabs shrank -> full clear and
   * redraw" path a genuine dab-list shrink (not expected, but handled
   * defensively) already needed.
   *
   * Releases layers for masks no longer present (deleted). Called at the
   * top of writeAdjustmentsAndRender, so it runs both on every mask-list
   * change and once per freshly loaded image (loadImage's initial call
   * re-rasterizes any brush/spot masks already in that image's saved edit
   * stack, since a canvas sized for a DIFFERENT image's resolution is
   * meaningless here -- loadImage resets brushRasterState/freeBrushLayers
   * before this runs). */
  function syncMaskRasterization() {
    if (!device || !brushTextureArray) return;
    const presentIds = new Set();
    for (const mask of masks) {
      const isSpot = mask.op === "spot_mask";
      if (mask.op !== "brush_mask" && !isSpot) continue;
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
        // required for brush's "multiply" erase compositing to correctly
        // no-op over never-painted areas (spot never uses "multiply", but
        // shares this same init for one consistent starting state).
        // Against a transparent destination, Porter-Duff "multiply" lets
        // the erase gradient's own color show through directly (since
        // there's no destination alpha to constrain it), which would
        // incorrectly paint weight into untouched regions. Against opaque
        // black (alpha=1, color=0), multiply always yields black
        // regardless of the erase color, so erasing over nothing stays
        // nothing.
        ctx.fillStyle = "black";
        ctx.fillRect(0, 0, canvas.width, canvas.height);
        entry = { canvas, ctx, layer, dabsDrawn: 0, featherDrawn: 0, firstDabX: 0, firstDabY: 0 };
        brushRasterState.set(mask.id, entry);
      }
      const dabs = /** @type {any} */ (mask).dabs;
      const featherChanged = isSpot && /** @type {any} */ (mask).feather !== entry.featherDrawn;
      // Dragging a spot mask's "move" handle translates every dab in place
      // (see handleMaskHandlePointerMove's own spot_move case) -- the dab
      // COUNT never changes, so the append-only dirty check below (which
      // this whole function otherwise relies on to keep a long stroke's
      // per-move cost O(1)) is blind to it. Comparing dabs[0]'s own
      // position against what was last baked into the texture is a cheap,
      // sufficient proxy: a move translates the WHOLE stroke by one
      // uniform delta, so if the first dab moved, they all did.
      const posChanged = isSpot && dabs.length > 0 && (dabs[0].x !== entry.firstDabX || dabs[0].y !== entry.firstDabY);
      if (dabs.length < entry.dabsDrawn || featherChanged || posChanged) {
        // Dab list shrank (not expected in this design, dabs only ever get
        // appended, but handled defensively rather than leaving stale
        // strokes visible) OR a spot mask's shared feather changed (every
        // dab needs the new softness baked in, not just new ones) OR the
        // whole stroke was dragged to a new position (every dab needs
        // re-rasterizing at its new coordinates, not just newly-painted
        // ones).
        entry.ctx.fillStyle = "black";
        entry.ctx.fillRect(0, 0, entry.canvas.width, entry.canvas.height);
        entry.dabsDrawn = 0;
      }
      for (let i = entry.dabsDrawn; i < dabs.length; i++) {
        if (isSpot) {
          rasterizeSpotDab(entry.ctx, entry.canvas.width, entry.canvas.height, dabs[i], /** @type {any} */ (mask).feather);
        } else {
          rasterizeDab(entry.ctx, entry.canvas.width, entry.canvas.height, dabs[i]);
        }
      }
      if (isSpot) {
        entry.featherDrawn = /** @type {any} */ (mask).feather;
        entry.firstDabX = dabs.length > 0 ? dabs[0].x : 0;
        entry.firstDabY = dabs.length > 0 ? dabs[0].y : 0;
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
    if (!device || !context || !pipeline || !bindGroup || !preMaskPipeline || !preMaskBindGroup || !preMaskTex || !lensCorrectPipeline || !lensCorrectBindGroup || !lensCorrectedTex || !perspectivePipeline || !perspectiveBindGroup || !perspectiveCorrectedTex || !gradePipeline || !gradeBindGroup || !atmReducePipeline || atmReduceBindGroups.length === 0 || !minChannelPipeline || !minChannelBindGroup || !minHPipeline || !minHBindGroup || !minVPipeline || !minVBindGroup || !meanHPipeline || !meanHBindGroup || !meanVPipeline || !meanVBindGroup || !textureHPipeline || !textureHBindGroup || !textureVPipeline || !textureVBindGroup || !clarityHPipeline || !clarityHBindGroup || !clarityVPipeline || !clarityVBindGroup || !sharpenHPipeline || !sharpenHBindGroup || !sharpenVPipeline || !sharpenVBindGroup || !lumaNRHPipeline || !lumaNRHBindGroup || !lumaNRVPipeline || !lumaNRVBindGroup || !colorNRHPipeline || !colorNRHBindGroup || !colorNRVPipeline || !colorNRVBindGroup || !gradedTex || !minChannelTex || !darkChannelHTex || !tRawTex || !transmissionHTex || !transmissionTex || !textureBlurScratchTex || !textureAdjustedTex || !clarityBlurScratchTex || !sharpenBlurHTex || !sharpenBlurTex || !lumaNRBlurHTex || !lumaNRBlurTex || !colorNRBlurHTex || !colorNRBlurTex || atmLightChain.length === 0 || !uniformBuffer || !masksBuffer || !curveLutBuffer || !hslBandsBuffer || !splitToningBuffer || !vignetteBuffer || !lensCorrectionBuffer || !perspectiveBuffer || !grainBuffer || !sharpenBuffer || !lumaNRBuffer || !colorNRBuffer || !clippingBuffer) return;

    // M4 Slice 2: before/after preview -- skips the ENTIRE global-grade +
    // local-mask pipeline below (not just the mask loop) and draws the raw
    // decoded source straight to the swapchain via fs_original, matching
    // real Lightroom's own \ behavior (show the photo exactly as it was
    // before any edits, not just "with local adjustments hidden"). Returns
    // before touching histogram/mask-rasterization state so toggling back
    // off simply re-renders the live graded result on the very next call,
    // no state to reconcile.
    if (showOriginal) {
      if (!originalPipeline || !originalBindGroup) return;
      const encoder = device.createCommandEncoder();
      runFullscreenPass(encoder, originalPipeline, originalBindGroup, context.getCurrentTexture().createView());
      device.queue.submit([encoder.finish()]);
      return;
    }

    syncMaskRasterization();

    // Tone curve: rebuilt from the current control points and rewritten
    // every render, same "cheap enough to just always redo" treatment as
    // the uniform/mask buffers below -- no dirty-tracking needed given
    // buildToneCurveLut's own cost (a handful of points, 256 samples).
    device.queue.writeBuffer(curveLutBuffer, 0, buildToneCurveLut(toneCurvePoints));
    device.queue.writeBuffer(hslBandsBuffer, 0, buildHslUniformData(hslBands));
    device.queue.writeBuffer(splitToningBuffer, 0, buildSplitToningUniformData(splitToning));
    device.queue.writeBuffer(lensCorrectionBuffer, 0, buildLensCorrectionUniformData(lensCorrection));
    device.queue.writeBuffer(perspectiveBuffer, 0, buildPerspectiveUniformData(perspective));
    device.queue.writeBuffer(vignetteBuffer, 0, buildVignetteUniformData(vignette));
    device.queue.writeBuffer(grainBuffer, 0, buildGrainUniformData(grain));
    device.queue.writeBuffer(sharpenBuffer, 0, buildSharpenUniformData(sharpen));
    device.queue.writeBuffer(lumaNRBuffer, 0, buildLumaNrUniformData(lumaNR));
    device.queue.writeBuffer(colorNRBuffer, 0, buildColorNrUniformData(colorNR));
    device.queue.writeBuffer(clippingBuffer, 0, new Float32Array([showClippingOverlay ? 1 : 0, 0, 0, 0]));

    // Mask overlay: -1 (disabled) unless the toggle is on AND the current
    // selection exists in `masks` -- `findIndex`'s own -1 miss-sentinel
    // *is* the disabled state, so no separate per-kind lookup is needed
    // here at all (the shader itself gates which kinds actually show the
    // overlay; see the kind > 1.5 check in the mask loop below).
    const selectedMaskIndex = showMaskOverlay ? masks.findIndex((m) => m.id === selectedMaskId) : -1;

    device.queue.writeBuffer(
      uniformBuffer,
      0,
      new Float32Array([
        exposure,
        contrast,
        saturation,
        masks.length,
        selectedMaskIndex,
        dehaze,
        texture,
        clarity,
        temperature,
        tint,
        highlights,
        shadows,
        whites,
        blacks,
        0,
        0,
      ]),
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
      } else if (m.op === "spot_mask") {
        // M4 Slice 1/2 (Healing/Clone brush): structurally unlike every
        // kind above -- no exposure/contrast/saturation (see SpotMask's
        // own doc comment in develop.js), so this branch is the ONLY one
        // that must ALSO set offset 8 (adjustments.x, repurposed as
        // mode) itself, and skip the trailing common exposure/contrast/
        // saturation writes below (guarded by the `m.op !== "spot_mask"`
        // check right after this if/else chain) -- those would otherwise
        // read `m.exposure` etc as `undefined`, which Float32Array
        // silently coerces to NaN, corrupting the mode field they'd
        // overwrite. M4 Slice 2: same texture-array-layer packing as
        // brush_mask above (o+7), plus the dabs' own centroid/average
        // radius (o+2/o+3, o+5) for heal-ring sampling -- see the WGSL
        // Mask struct's own doc comment for the full field-repurposing map.
        const c = spotCentroidAndRadius(m.dabs);
        maskData[o + 0] = m.sourceOffset.dx;
        maskData[o + 1] = m.sourceOffset.dy;
        maskData[o + 2] = c.x;
        maskData[o + 3] = c.y;
        maskData[o + 4] = m.feather;
        maskData[o + 5] = c.avgRadius;
        maskData[o + 6] = 5; // kind = spot
        maskData[o + 7] = brushRasterState.get(m.id)?.layer ?? 0;
        maskData[o + 8] = m.mode === "heal" ? 1 : 0;
      } else if (m.op === "red_eye_mask") {
        // M4: same center/radiusX/radiusY/feather geometry as radial above,
        // but o+5 (params.y, radial's own invert slot) is repurposed as
        // pupilSize and o+11 (adjustments.w, unused padding on every other
        // kind) as darken -- see the WGSL Mask struct's own doc comment.
        // Like spot, must set its own o+5/o+8-10 here and be excluded from
        // the common invert/exposure/contrast/saturation write below,
        // since `m.invert`/`m.exposure`/etc are all undefined on this
        // mask kind (see RedEyeMask's own JSDoc typedef in develop.js).
        maskData[o + 0] = m.center.x;
        maskData[o + 1] = m.center.y;
        maskData[o + 2] = m.radiusX;
        maskData[o + 3] = m.radiusY;
        maskData[o + 4] = m.feather;
        maskData[o + 5] = m.pupilSize;
        maskData[o + 6] = 6; // kind = red eye
        maskData[o + 11] = m.darken;
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
      if (m.op !== "spot_mask" && m.op !== "red_eye_mask") {
        maskData[o + 5] = m.invert ? 1 : 0;
        maskData[o + 8] = m.exposure;
        maskData[o + 9] = m.contrast;
        maskData[o + 10] = m.saturation;
      }
    });
    device.queue.writeBuffer(masksBuffer, 0, maskData);

    const encoder = device.createCommandEncoder();

    // Texture/Clarity/Dehaze/Sharpen/NR: the local-contrast, dark-channel/
    // atmospheric-light/transmission, and sharpen/NR blur passes depend on
    // {exposure, contrast, saturation, temperature, tint, highlights,
    // shadows, whites, blacks, toneCurvePoints, hslBands, splitToning,
    // texture, clarity, sharpenRadius} -- NOT masks/
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
    // Lens Corrections' own inputs join this key for the same reason
    // texture/clarity's own amounts do (see this key's own doc comment
    // above): fs_lens_correct writes into lensCorrectedTex, which
    // gradeBindGroup reads INSIDE this same dirty-gated block -- a
    // lens-correction-only change (e.g. dragging Manual Distortion) that
    // isn't in this key would silently show a stale, uncorrected preview
    // until some unrelated slider happened to invalidate the block.
    // Perspective Correction's own inputs join for the identical reason:
    // fs_perspective writes into perspectiveCorrectedTex, which
    // gradeBindGroup ALSO reads inside this same block.
    // White Balance (temperature/tint) and Basic Tone (highlights/shadows/
    // whites/blacks) belong here for the SAME reason exposure/contrast/
    // saturation do, not a new one -- fs_grade applies all of them together
    // in one apply_global_adjustments call (see that WGSL function's own
    // parameter list) and writes the result into gradedTex, INSIDE this
    // gated block. Omitting them was a real bug: the interactive Uniform
    // buffer write always carries their current value (writeBuffer below is
    // unconditional), but without fs_grade actually re-running, gradedTex
    // stayed stale -- Temp/Tint/Highlights/Shadows/Whites/Blacks silently
    // had zero visible effect on their own, until some unrelated slider in
    // this key happened to invalidate the block and "catch up."
    const spatialOpsKey = JSON.stringify({
      exposure, contrast, saturation, temperature, tint, highlights, shadows, whites, blacks,
      toneCurvePoints, hslBands, splitToning, texture, clarity,
      sharpenRadius: sharpen.radius, lensCorrection, perspective,
    });
    if (spatialOpsKey !== spatialOpsInputsKey) {
      spatialOpsInputsKey = spatialOpsKey;
      runFullscreenPass(encoder, lensCorrectPipeline, lensCorrectBindGroup, lensCorrectedTex.createView());
      runFullscreenPass(encoder, perspectivePipeline, perspectiveBindGroup, /** @type {GPUTexture} */ (perspectiveCorrectedTex).createView());
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

    // Pre-mask pass (M4 Slice 1): unconditional every render, same as the
    // old single-pass fs_final always was -- Dehaze amount/Vignette/Grain/
    // NR are all cheap per-pixel blends read fresh from their own uniform
    // buffers every frame (unlike the expensive spatialOpsKey-gated block
    // above), so this can't be folded into that gate without breaking
    // live response to those sliders. Must run BEFORE fs_mask below --
    // preMaskTex is fs_mask's own input, see that pass's own doc comment.
    runFullscreenPass(encoder, preMaskPipeline, preMaskBindGroup, preMaskTex.createView());

    runFullscreenPass(encoder, pipeline, bindGroup, context.getCurrentTexture().createView());

    // Histogram: fs_final's OWN pipeline/bindGroup, unchanged, drawn a
    // SECOND time into the small fixed-size histogramTex -- fs_mask's own
    // `in.uv`-based coord proportionally re-maps across the same graded
    // pixels the canvas above just got, so this needs no separate WGSL
    // entry point or bind group (see histogramTex's own doc comment).
    // Skipped while a previous readback is still in flight -- a buffer
    // that's currently mapped (see readHistogramIfIdle) can't be copied
    // into again without a validation error, and the next render (there
    // will be one shortly, since this fires on every relevant UI change)
    // will naturally catch up once that map resolves.
    if (histogramTex && histogramReadbackBuffer && !histogramReadInFlight) {
      runFullscreenPass(encoder, pipeline, bindGroup, histogramTex.createView());
      encoder.copyTextureToBuffer(
        { texture: histogramTex },
        { buffer: histogramReadbackBuffer, bytesPerRow: HISTOGRAM_SIZE * 4 },
        { width: HISTOGRAM_SIZE, height: HISTOGRAM_SIZE },
      );
    }

    device.queue.submit([encoder.finish()]);
    readHistogramIfIdle();
  }

  $effect(() => {
    const path = imagePath;
    const canvas = canvasEl;
    if (!path || !canvas) return;
    zoomMode = "fit";

    (async () => {
      // M5 Slice 1: only a failure to ACQUIRE the GPU device itself (no
      // `navigator.gpu`, no adapter, or `requestDevice()` rejecting --
      // `initGpu`'s own three throw/rejection sites) routes into CPU
      // fallback. A shader-compile failure or a later `uncapturederror`
      // does NOT throw from `initGpu` -- both mean WebGPU itself works
      // fine and this is a real app/shader bug, so they keep setting
      // `status = "error"` exactly as before this slice (see their own
      // doc comments inside `initGpu`), never masked as an environment gap.
      if (!device) {
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
    const capturedDevice = device;
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

  /** `.crop-clip`'s own box size. Two modes, switched on `zoomMode`:
   * "fit" is an object-fit:contain-style box, the largest box of the
   * crop's own aspect ratio that fits within the wrap's padded content
   * area, centered via the wrapper's own `margin:auto` once both
   * dimensions are explicit (a definite width/height, unlike
   * `aspect-ratio` alone, reliably centers via auto margins in every
   * engine -- see wrapWidth/wrapHeight's own doc comment for why this is
   * computed in JS at all instead of left to CSS). "100" uses
   * `nativeCropClipSize` instead -- see that function's own doc comment
   * for why sizing the wrapper to the crop's true pixel dimensions is
   * exactly what makes the canvas's existing percentage-based inline
   * positioning (unchanged either way -- see the markup's own doc
   * comment) land on a true 1:1 scale. `.crop-clip.active.zoomed`'s own
   * CSS rule (`max-width/max-height: none`) is what lets this box actually
   * exceed the wrap's available area so `.canvas-wrap`'s existing
   * `overflow: auto` (already toggled by the SAME `zoomMode === "100"`
   * condition, see that class binding) has something real to scroll. */
  let cropClipSize = $derived.by(() => {
    if (!showCommittedCropPreview || !sourceWidth || !sourceHeight) {
      return { w: 0, h: 0 };
    }
    if (zoomMode === "100") {
      return nativeCropClipSize(crop, sourceWidth, sourceHeight);
    }
    if (!wrapWidth || !wrapHeight) return { w: 0, h: 0 };
    const availW = Math.max(wrapWidth - CROP_CLIP_PADDING_PX * 2, 1);
    const availH = Math.max(wrapHeight - CROP_CLIP_PADDING_PX * 2, 1);
    const aspect = (crop.width * sourceWidth) / (crop.height * sourceHeight);
    let w = availW;
    let h = w / aspect;
    if (h > availH) {
      h = availH;
      w = h * aspect;
    }
    return { w, h };
  });

  // 1:1 tier trigger -- fires for BOTH ways zoomMode can flip to "100"
  // (the canvas click-to-zoom in handlePointerUp, and the zoom-badge
  // button's onclick both just set zoomMode directly), so neither call
  // site needs to know about the tier upgrade at all. Guarded on
  // activeTier so it's a no-op once already upgraded for this image, and
  // on status==="ready" so it can't fire before loadImage has finished
  // its own initial setup. Also skipped entirely while showing a Smart
  // Preview fallback (M4) -- the 1:1 tier needs the same unreachable
  // source file the draft tier just failed to read, so attempting it
  // would just fail again; the draft tier's DEVELOP_PREVIEW_MAX_DIMENSION
  // cap is the effective zoom ceiling while offline.
  $effect(() => {
    if (status === "ready" && zoomMode === "100" && activeTier !== "full" && !isSmartPreview) {
      upgradeToFullTier(imagePath);
    }
  });
</script>

<div class="canvas-wrap" class:zoomed={zoomMode === "100"} bind:this={wrapEl}>
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
       fit-scaled -- so `cropClipSize`'s own `zoomMode` branch (see that
       variable's doc comment) is the ONLY piece that needed to change to
       support 100%-zoom scrolling in the cropped view; `class:zoomed`
       here pairs with `.crop-clip.active.zoomed`'s own CSS rule to let
       the box actually grow past the wrap's available area so there's
       something for `.canvas-wrap`'s own `overflow:auto` (same
       `zoomMode` condition) to scroll. -->
  <div
    class="crop-clip"
    class:active={showCommittedCropPreview}
    class:zoomed={zoomMode === "100"}
    style={showCommittedCropPreview ? `width:${cropClipSize.w}px; height:${cropClipSize.h}px;` : ""}
  >
    <canvas
      bind:this={canvasEl}
      class:zoomed={zoomMode === "100"}
      class:cropped={showCommittedCropPreview}
      class:placing={activeTool === "linear_gradient" || activeTool === "radial_gradient" || activeTool === "brush" || activeTool === "color_range" || activeTool === "eyedropper" || activeTool === "spot" || activeTool === "red_eye"}
      class:space-pan={spacePanning}
      class:hidden={status === "cpu-fallback"}
      style={showCommittedCropPreview
        ? `width:${100 / crop.width}%; height:${100 / crop.height}%; left:${(-crop.x * 100) / crop.width}%; top:${(-crop.y * 100) / crop.height}%; transform: rotate(${crop.angle}deg);`
        : activeTool === "crop"
          ? `transform: rotate(${crop.angle}deg);`
          : ""}
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
      {#if maskOverlaysVisible}
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
        {:else if mask.op === "red_eye_mask"}
          <!-- M4: same center/radius handle pair as Radial above (both
               "center" and "radius" drag kinds are already fully generic
               in handleMaskHandlePointerMove -- they patch whichever mask
               id they're given, with no op-specific branching), just no
               feather-range indicator rings (a named scope cut, not an
               oversight: red eye's own "how strict is red detection"
               control is Pupil Size, not spatial feather width, so a
               feather-boundary visualization would be misleading here). -->
          <svg class="mask-ellipse" class:selected={mask.id === selectedMaskId}>
            <ellipse cx="{mask.center.x * 100}%" cy="{mask.center.y * 100}%" rx="{mask.radiusX * 100}%" ry="{mask.radiusY * 100}%" />
          </svg>
          <button
            class="mask-handle"
            class:selected={mask.id === selectedMaskId}
            style="left:{mask.center.x * 100}%; top:{mask.center.y * 100}%"
            aria-label="Red eye center"
            onpointerdown={(e) => handleMaskHandlePointerDown(e, mask.id, "center")}
            onpointermove={handleMaskHandlePointerMove}
            onpointerup={handleMaskHandlePointerUp}
          ></button>
          <button
            class="mask-handle"
            class:selected={mask.id === selectedMaskId}
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
          <svg class="mask-ellipse spot" class:selected={mask.id === selectedMaskId}>
            <line x1="{c.x * 100}%" y1="{c.y * 100}%" x2="{sx * 100}%" y2="{sy * 100}%" class="spot-link" />
            <ellipse cx="{sx * 100}%" cy="{sy * 100}%" rx="{c.avgRadius * 100}%" ry="{spotRyPercent(c.avgRadius)}%" class="spot-source" />
          </svg>
          <button
            class="mask-handle spot-move-handle"
            class:selected={mask.id === selectedMaskId}
            style="left:{c.x * 100}%; top:{c.y * 100}%"
            aria-label="Move spot"
            onpointerdown={(e) => handleMaskHandlePointerDown(e, mask.id, "spot_move", { x: c.x, y: c.y }, mask.dabs)}
            onpointermove={handleMaskHandlePointerMove}
            onpointerup={handleMaskHandlePointerUp}
          ></button>
          <button
            class="mask-handle spot-source-handle"
            class:selected={mask.id === selectedMaskId}
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
             "un-rotating." Four darkened bands (not a single clip-path/mask shape)
             spotlight the crop rect -- simplest way to dim the
             outside-of-crop area without extra CSS feature requirements. -->
        <div class="crop-dim" style="left:0; top:0; width:100%; height:{crop.y * 100}%"></div>
        <div class="crop-dim" style="left:0; top:{(crop.y + crop.height) * 100}%; width:100%; height:{(1 - crop.y - crop.height) * 100}%"></div>
        <div class="crop-dim" style="left:0; top:{crop.y * 100}%; width:{crop.x * 100}%; height:{crop.height * 100}%"></div>
        <div class="crop-dim" style="left:{(crop.x + crop.width) * 100}%; top:{crop.y * 100}%; width:{(1 - crop.x - crop.width) * 100}%; height:{crop.height * 100}%"></div>
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
  {#if status === "ready"}
    <button
      class="zoom-badge"
      type="button"
      title={zoomMode === "fit" ? "Click image for 100%" : "Click image to fit"}
      onclick={() => (zoomMode = zoomMode === "fit" ? "100" : "fit")}
    >{zoomMode === "fit" ? "Fit" : "100%"}</button>
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
    {#if cpuFallbackPreviewUrl}
      <img class="cpu-fallback-image" src={cpuFallbackPreviewUrl} alt="Preview (GPU unavailable)" draggable="false" />
    {:else}
      <div class="overlay">Rendering preview…</div>
    {/if}
    <div class="cpu-fallback-badge" title={fallbackInfo?.message ?? ""}>
      ⚠ GPU acceleration unavailable — preview updates after you stop adjusting, not live while dragging
    </div>
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
    background: rgba(0, 0, 0, 0.55);
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
     `.zoom-badge`'s bottom-right corner) so the two never collide, and
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
