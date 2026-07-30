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
   *   masks: import('$lib/api/develop.js').LinearGradientMask[],
   *   activeTool: string | null,
   *   selectedMaskId: string | null,
   *   onMaskCreated: (placement: { start: {x: number, y: number}, end: {x: number, y: number} }) => void,
   *   onMaskUpdated: (id: string, patch: Partial<import('$lib/api/develop.js').LinearGradientMask>) => void,
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

  // M3 Slice 5: while the "Linear Gradient" tool is active, dragging on
  // the canvas places a new mask instead of panning -- tracks the
  // in-progress drag (normalized coords) for the live preview line, not
  // committed to the edit stack until pointerup.
  let placingMask = $state(/** @type {{ start: {x:number,y:number}, end: {x:number,y:number} } | null} */ (null));
  /** @type {{ maskId: string, which: "start" | "end" } | null} */
  let handleDragState = null;

  /** CSS-pixel click position -> normalized (0..1) image-space coordinate,
   * matching the shader's own `in.uv`. Reused from the zoom-to-point math
   * (M3 Slice 3) -- `getBoundingClientRect()` already reflects the current
   * scroll offset, so this needs no extra bookkeeping for panned/zoomed
   * state. */
  function screenToNormalized(/** @type {number} */ clientX, /** @type {number} */ clientY) {
    if (!canvasEl) return { x: 0, y: 0 };
    const rect = canvasEl.getBoundingClientRect();
    const scaleX = rect.width / canvasEl.width;
    const scaleY = rect.height / canvasEl.height;
    return {
      x: (clientX - rect.left) / scaleX / canvasEl.width,
      y: (clientY - rect.top) / scaleY / canvasEl.height,
    };
  }

  // M3 Slice 5: a hard branch on `activeTool`, not a case bolted onto the
  // pan/zoom logic -- while a mask tool is active, dragging NEVER pans or
  // toggles zoom, even in 100% mode, and vice versa.
  function handlePointerDown(/** @type {PointerEvent} */ e) {
    if (activeTool === "linear_gradient") {
      e.preventDefault();
      canvasEl?.setPointerCapture(e.pointerId);
      const p = screenToNormalized(e.clientX, e.clientY);
      placingMask = { start: p, end: p };
      return;
    }
    if (!wrapEl) return;
    e.preventDefault();
    canvasEl?.setPointerCapture(e.pointerId);
    dragState = {
      startX: e.clientX,
      startY: e.clientY,
      startScrollLeft: wrapEl.scrollLeft,
      startScrollTop: wrapEl.scrollTop,
    };
  }

  function handlePointerMove(/** @type {PointerEvent} */ e) {
    if (placingMask) {
      placingMask = { ...placingMask, end: screenToNormalized(e.clientX, e.clientY) };
      return;
    }
    if (!dragState || !wrapEl) return;
    wrapEl.scrollLeft = dragState.startScrollLeft - (e.clientX - dragState.startX);
    wrapEl.scrollTop = dragState.startScrollTop - (e.clientY - dragState.startY);
  }

  async function handlePointerUp(/** @type {PointerEvent} */ e) {
    canvasEl?.releasePointerCapture(e.pointerId);
    if (placingMask) {
      const { start, end } = placingMask;
      placingMask = null;
      // Ignore a near-zero-size drag (an accidental click while the tool
      // was active) -- a real gradient needs two distinct points.
      if (Math.hypot(end.x - start.x, end.y - start.y) > 0.01) onMaskCreated({ start, end });
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

  /** Dragging an existing mask's start/end handle -- separate from the
   * canvas's own pointer handlers above (these fire on the handle button
   * itself, which sits visually on top, so the canvas never sees them). */
  function handleMaskHandlePointerDown(
    /** @type {PointerEvent} */ e,
    /** @type {string} */ maskId,
    /** @type {"start" | "end"} */ which,
  ) {
    e.stopPropagation();
    e.preventDefault();
    /** @type {HTMLElement} */ (e.currentTarget).setPointerCapture(e.pointerId);
    handleDragState = { maskId, which };
    onMaskSelected(maskId);
  }

  function handleMaskHandlePointerMove(/** @type {PointerEvent} */ e) {
    if (!handleDragState) return;
    onMaskUpdated(handleDragState.maskId, { [handleDragState.which]: screenToNormalized(e.clientX, e.clientY) });
  }

  function handleMaskHandlePointerUp(/** @type {PointerEvent} */ e) {
    /** @type {HTMLElement} */ (e.currentTarget).releasePointerCapture(e.pointerId);
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
    struct Mask {
      start_end: vec4<f32>,   // xy = start, zw = end (normalized image space)
      params: vec4<f32>,      // x = feather 0-100, y = invert 0/1, z = kind (0=linear, reserved), w unused
      adjustments: vec4<f32>, // x = exposure_ev, y = contrast, z = saturation, w unused
    };
    const MAX_MASKS = 8;

    @group(0) @binding(0) var srcSampler: sampler;
    @group(0) @binding(1) var srcTexture: texture_2d<f32>;
    @group(0) @binding(2) var<uniform> adj: Adjustments;
    @group(0) @binding(3) var<uniform> masks: array<Mask, MAX_MASKS>;

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
        let dir = m.start_end.zw - m.start_end.xy;
        let len2 = max(dot(dir, dir), 0.000001);
        // Projection-onto-segment parametrization: 0 at start, 1 at end,
        // extrapolated linearly beyond both, then clamped.
        let t = dot(in.uv - m.start_end.xy, dir) / len2;
        // Feather widens the transition band symmetrically around the
        // midpoint -- at feather=50 the pins themselves move to weight
        // 0.25/0.75 rather than staying at 0/1, matching real Lightroom's
        // own gradient-feather model (separate outer feather lines beyond
        // the pins), not a corner-only softening.
        let softness = clamp(m.params.x / 100.0, 0.0, 0.999);
        var weight = clamp((t + softness) / (1.0 + 2.0 * softness), 0.0, 1.0);
        if (m.params.y > 0.5) { weight = 1.0 - weight; }
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

    const sampler = device.createSampler({ magFilter: "linear", minFilter: "linear" });
    bindGroup = device.createBindGroup({
      layout: pipeline.getBindGroupLayout(0),
      entries: [
        { binding: 0, resource: sampler },
        { binding: 1, resource: sourceTexture.createView() },
        { binding: 2, resource: { buffer: uniformBuffer } },
        { binding: 3, resource: { buffer: masksBuffer } },
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

  function writeAdjustmentsAndRender() {
    if (!device || !context || !pipeline || !bindGroup || !uniformBuffer || !masksBuffer) return;
    device.queue.writeBuffer(
      uniformBuffer,
      0,
      new Float32Array([exposure, contrast, saturation, masks.length]),
    );

    const maskData = new Float32Array(MAX_MASKS * 12);
    masks.slice(0, MAX_MASKS).forEach((/** @type {any} */ m, /** @type {number} */ i) => {
      const o = i * 12;
      maskData[o + 0] = m.start.x;
      maskData[o + 1] = m.start.y;
      maskData[o + 2] = m.end.x;
      maskData[o + 3] = m.end.y;
      maskData[o + 4] = m.feather;
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
    class:placing={activeTool === "linear_gradient"}
    onpointerdown={handlePointerDown}
    onpointermove={handlePointerMove}
    onpointerup={handlePointerUp}
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
      {/each}
      {#if placingMask}
        <svg class="mask-line placing">
          <line x1="{placingMask.start.x * 100}%" y1="{placingMask.start.y * 100}%" x2="{placingMask.end.x * 100}%" y2="{placingMask.end.y * 100}%" />
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
