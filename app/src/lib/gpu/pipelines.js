// Builds the WebGPU device, canvas context, shader module and every render pipeline.
// Moved out of DevelopCanvas.svelte.

import { WGSL } from "$lib/gpu/shaders/index.js";
import { MAX_MASKS, HISTOGRAM_SIZE } from "./gpuHandles.js";

export async function initGpu(/** @type {import('./gpuHandles.js').GpuHandles} */ gpu, /** @type {HTMLCanvasElement} */ canvas, /** @type {import('./gpuHandles.js').GpuHooks} */ hooks) {
  if (!("gpu" in navigator)) {
    throw new Error("navigator.gpu is undefined in this webview");
  }
  const adapter = await navigator.gpu.requestAdapter();
  if (!adapter) throw new Error("requestAdapter() returned null");
  gpu.device = await adapter.requestDevice();
  gpu.presentationFormat = navigator.gpu.getPreferredCanvasFormat();

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
  gpu.device.addEventListener("uncapturederror", (/** @type {any} */ event) => {
    hooks.onError(`WebGPU: ${event.error.message}`);
  });

  gpu.context = canvas.getContext("webgpu");
  if (!gpu.context) throw new Error("canvas.getContext('webgpu') returned null");
  gpu.context.configure({ device: gpu.device, format: gpu.presentationFormat, alphaMode: "opaque" });

  // One compiled module, many entry points -- each createRenderPipeline
  // call below just picks a different entryPoint out of the SAME
  // compiled WGSL, no separate compilation per pass. Each pipeline gets
  // its OWN layout:"auto"-inferred bind group layout, scoped to only the
  // bindings that specific entry point's own code actually references
  // (NOT the whole module's declarations) -- see the WGSL source's own
  // comment on gradePipeline/pipeline(final)'s deliberately DIFFERENT
  // inferred layouts for why a bind group built for one pipeline can't
  // be reused for another, even where their WGSL code looks similar.
  const module = gpu.device.createShaderModule({ code: WGSL });
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
      hooks.onError(`WGSL compile: ${problems.map((m) => `line ${m.lineNum}: ${m.message}`).join(" | ")}`);
    }
  });
  gpu.pipeline = gpu.device.createRenderPipeline({
    layout: "auto",
    vertex: { module, entryPoint: "vs_main" },
    fragment: { module, entryPoint: "fs_mask", targets: [{ format: gpu.presentationFormat }] },
    primitive: { topology: "triangle-list" },
  });
  // M4 Slice 2: before/after preview (see fs_original's own doc comment).
  gpu.originalPipeline = gpu.device.createRenderPipeline({
    layout: "auto",
    vertex: { module, entryPoint: "vs_main" },
    fragment: { module, entryPoint: "fs_original", targets: [{ format: gpu.presentationFormat }] },
    primitive: { topology: "triangle-list" },
  });
  // M4 Slice 1 (Healing/Clone brush): writes preMaskTex, fs_mask's own
  // input -- see preMaskTex's WGSL-side doc comment for why this had to
  // become its own pass rather than staying fused into fs_final.
  gpu.preMaskPipeline = gpu.device.createRenderPipeline({
    layout: "auto",
    vertex: { module, entryPoint: "vs_main" },
    fragment: { module, entryPoint: "fs_premask", targets: [{ format: "rgba16float" }] },
    primitive: { topology: "triangle-list" },
  });
  gpu.lensCorrectPipeline = gpu.device.createRenderPipeline({
    layout: "auto",
    vertex: { module, entryPoint: "vs_main" },
    fragment: { module, entryPoint: "fs_lens_correct", targets: [{ format: "rgba16float" }] },
    primitive: { topology: "triangle-list" },
  });
  gpu.perspectivePipeline = gpu.device.createRenderPipeline({
    layout: "auto",
    vertex: { module, entryPoint: "vs_main" },
    fragment: { module, entryPoint: "fs_perspective", targets: [{ format: "rgba16float" }] },
    primitive: { topology: "triangle-list" },
  });
  gpu.gradePipeline = gpu.device.createRenderPipeline({
    layout: "auto",
    vertex: { module, entryPoint: "vs_main" },
    fragment: { module, entryPoint: "fs_grade", targets: [{ format: "rgba16float" }] },
    primitive: { topology: "triangle-list" },
  });
  gpu.atmReducePipeline = gpu.device.createRenderPipeline({
    layout: "auto",
    vertex: { module, entryPoint: "vs_main" },
    fragment: { module, entryPoint: "fs_atm_reduce", targets: [{ format: "rgba16float" }] },
    primitive: { topology: "triangle-list" },
  });
  gpu.minChannelPipeline = gpu.device.createRenderPipeline({
    layout: "auto",
    vertex: { module, entryPoint: "vs_main" },
    fragment: { module, entryPoint: "fs_min_channel", targets: [{ format: "r32float" }] },
    primitive: { topology: "triangle-list" },
  });
  gpu.minHPipeline = gpu.device.createRenderPipeline({
    layout: "auto",
    vertex: { module, entryPoint: "vs_main" },
    fragment: { module, entryPoint: "fs_min_h", targets: [{ format: "r32float" }] },
    primitive: { topology: "triangle-list" },
  });
  gpu.minVPipeline = gpu.device.createRenderPipeline({
    layout: "auto",
    vertex: { module, entryPoint: "vs_main" },
    fragment: { module, entryPoint: "fs_min_v", targets: [{ format: "r32float" }] },
    primitive: { topology: "triangle-list" },
  });
  gpu.meanHPipeline = gpu.device.createRenderPipeline({
    layout: "auto",
    vertex: { module, entryPoint: "vs_main" },
    fragment: { module, entryPoint: "fs_mean_h", targets: [{ format: "r32float" }] },
    primitive: { topology: "triangle-list" },
  });
  gpu.meanVPipeline = gpu.device.createRenderPipeline({
    layout: "auto",
    vertex: { module, entryPoint: "vs_main" },
    fragment: { module, entryPoint: "fs_mean_v", targets: [{ format: "r32float" }] },
    primitive: { topology: "triangle-list" },
  });
  gpu.textureHPipeline = gpu.device.createRenderPipeline({
    layout: "auto",
    vertex: { module, entryPoint: "vs_main" },
    fragment: { module, entryPoint: "fs_texture_h", targets: [{ format: "r32float" }] },
    primitive: { topology: "triangle-list" },
  });
  gpu.textureVPipeline = gpu.device.createRenderPipeline({
    layout: "auto",
    vertex: { module, entryPoint: "vs_main" },
    fragment: { module, entryPoint: "fs_texture_v", targets: [{ format: "rgba16float" }] },
    primitive: { topology: "triangle-list" },
  });
  // Clarity (RFC-0010): 10 new pipelines replacing the old fs_clarity_h/v
  // pair -- see dehazeLocalContrast.js's own doc comment for the full
  // guided-filter pass list. Every new intermediate (mean_p, corr_p, a, b,
  // mean_a, mean_b) is single-channel, same "r32float" convention as every
  // other scalar intermediate in this file; fs_clarity_v (final) is
  // unchanged in name/target -- it still writes the finished rgb+delta to
  // gradedTex.
  gpu.clarityMeanpHPipeline = gpu.device.createRenderPipeline({
    layout: "auto",
    vertex: { module, entryPoint: "vs_main" },
    fragment: { module, entryPoint: "fs_clarity_meanp_h", targets: [{ format: "r32float" }] },
    primitive: { topology: "triangle-list" },
  });
  gpu.clarityMeanpVPipeline = gpu.device.createRenderPipeline({
    layout: "auto",
    vertex: { module, entryPoint: "vs_main" },
    fragment: { module, entryPoint: "fs_clarity_meanp_v", targets: [{ format: "r32float" }] },
    primitive: { topology: "triangle-list" },
  });
  gpu.clarityCorrpHPipeline = gpu.device.createRenderPipeline({
    layout: "auto",
    vertex: { module, entryPoint: "vs_main" },
    fragment: { module, entryPoint: "fs_clarity_corrp_h", targets: [{ format: "r32float" }] },
    primitive: { topology: "triangle-list" },
  });
  gpu.clarityCorrpVPipeline = gpu.device.createRenderPipeline({
    layout: "auto",
    vertex: { module, entryPoint: "vs_main" },
    fragment: { module, entryPoint: "fs_clarity_corrp_v", targets: [{ format: "r32float" }] },
    primitive: { topology: "triangle-list" },
  });
  gpu.clarityAPipeline = gpu.device.createRenderPipeline({
    layout: "auto",
    vertex: { module, entryPoint: "vs_main" },
    fragment: { module, entryPoint: "fs_clarity_a", targets: [{ format: "r32float" }] },
    primitive: { topology: "triangle-list" },
  });
  gpu.clarityBPipeline = gpu.device.createRenderPipeline({
    layout: "auto",
    vertex: { module, entryPoint: "vs_main" },
    fragment: { module, entryPoint: "fs_clarity_b", targets: [{ format: "r32float" }] },
    primitive: { topology: "triangle-list" },
  });
  gpu.clarityMeanaHPipeline = gpu.device.createRenderPipeline({
    layout: "auto",
    vertex: { module, entryPoint: "vs_main" },
    fragment: { module, entryPoint: "fs_clarity_meana_h", targets: [{ format: "r32float" }] },
    primitive: { topology: "triangle-list" },
  });
  gpu.clarityMeanaVPipeline = gpu.device.createRenderPipeline({
    layout: "auto",
    vertex: { module, entryPoint: "vs_main" },
    fragment: { module, entryPoint: "fs_clarity_meana_v", targets: [{ format: "r32float" }] },
    primitive: { topology: "triangle-list" },
  });
  gpu.clarityMeanbHPipeline = gpu.device.createRenderPipeline({
    layout: "auto",
    vertex: { module, entryPoint: "vs_main" },
    fragment: { module, entryPoint: "fs_clarity_meanb_h", targets: [{ format: "r32float" }] },
    primitive: { topology: "triangle-list" },
  });
  gpu.clarityMeanbVPipeline = gpu.device.createRenderPipeline({
    layout: "auto",
    vertex: { module, entryPoint: "vs_main" },
    fragment: { module, entryPoint: "fs_clarity_meanb_v", targets: [{ format: "r32float" }] },
    primitive: { topology: "triangle-list" },
  });
  gpu.clarityVPipeline = gpu.device.createRenderPipeline({
    layout: "auto",
    vertex: { module, entryPoint: "vs_main" },
    fragment: { module, entryPoint: "fs_clarity_v", targets: [{ format: "rgba16float" }] },
    primitive: { topology: "triangle-list" },
  });
  gpu.sharpenHPipeline = gpu.device.createRenderPipeline({
    layout: "auto",
    vertex: { module, entryPoint: "vs_main" },
    fragment: { module, entryPoint: "fs_sharpen_h", targets: [{ format: "r32float" }] },
    primitive: { topology: "triangle-list" },
  });
  gpu.sharpenVPipeline = gpu.device.createRenderPipeline({
    layout: "auto",
    vertex: { module, entryPoint: "vs_main" },
    fragment: { module, entryPoint: "fs_sharpen_v", targets: [{ format: "r32float" }] },
    primitive: { topology: "triangle-list" },
  });
  gpu.lumaNRHPipeline = gpu.device.createRenderPipeline({
    layout: "auto",
    vertex: { module, entryPoint: "vs_main" },
    fragment: { module, entryPoint: "fs_lumaNR_h", targets: [{ format: "r32float" }] },
    primitive: { topology: "triangle-list" },
  });
  gpu.lumaNRVPipeline = gpu.device.createRenderPipeline({
    layout: "auto",
    vertex: { module, entryPoint: "vs_main" },
    fragment: { module, entryPoint: "fs_lumaNR_v", targets: [{ format: "r32float" }] },
    primitive: { topology: "triangle-list" },
  });
  gpu.colorNRHPipeline = gpu.device.createRenderPipeline({
    layout: "auto",
    vertex: { module, entryPoint: "vs_main" },
    fragment: { module, entryPoint: "fs_colorNR_h", targets: [{ format: "rgba16float" }] },
    primitive: { topology: "triangle-list" },
  });
  gpu.colorNRVPipeline = gpu.device.createRenderPipeline({
    layout: "auto",
    vertex: { module, entryPoint: "vs_main" },
    fragment: { module, entryPoint: "fs_colorNR_v", targets: [{ format: "rgba16float" }] },
    primitive: { topology: "triangle-list" },
  });

  gpu.uniformBuffer = gpu.device.createBuffer({
    size: 64, // 16 x f32 (exposure, contrast, saturation, mask_count, selected_mask_index, dehaze, texture, clarity, temp, tint, highlights, shadows, whites, blacks, pad0, pad1)
    usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
  });
  gpu.masksBuffer = gpu.device.createBuffer({
    size: MAX_MASKS * 12 * 4, // 12 f32s (3x vec4) per mask
    usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
  });
  gpu.curveLutBuffer = gpu.device.createBuffer({
    size: 64 * 16, // 64 vec4<f32> (256 f32 samples), packed to avoid WGSL's 16-byte uniform-array-stride requirement -- see the Mask struct's own comment on this exact footgun
    usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
  });
  gpu.hslBandsBuffer = gpu.device.createBuffer({
    size: 8 * 16, // 8 bands x vec4<f32> (hue, saturation, luminance, unused padding)
    usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
  });
  gpu.splitToningBuffer = gpu.device.createBuffer({
    size: 8 * 4, // 8 f32 (5 real fields + 3 padding), matches the WGSL SplitToning struct
    usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
  });
  gpu.vignetteBuffer = gpu.device.createBuffer({
    size: 4 * 4, // 4 f32 (3 real fields + 1 padding), matches the WGSL Vignette struct
    usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
  });
  gpu.lensCorrectionBuffer = gpu.device.createBuffer({
    size: 24 * 4, // 24 f32, matches the WGSL LensCorrectionParams struct exactly (no padding needed -- already a multiple of 16 bytes)
    usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
  });
  gpu.perspectiveBuffer = gpu.device.createBuffer({
    size: 8 * 4, // 8 f32 (5 real fields + 3 padding), matches the WGSL PerspectiveParams struct
    usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
  });
  gpu.grainBuffer = gpu.device.createBuffer({
    size: 4 * 4, // 4 f32 (3 real fields + 1 padding), matches the WGSL Grain struct
    usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
  });
  gpu.sharpenBuffer = gpu.device.createBuffer({
    size: 4 * 4, // 4 f32 (amount, radius, detail, masking), matches the WGSL SharpenParams struct
    usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
  });
  gpu.lumaNRBuffer = gpu.device.createBuffer({
    size: 4 * 4, // 4 f32 (3 real fields + 1 padding), matches the WGSL LumaNrParams struct
    usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
  });
  gpu.colorNRBuffer = gpu.device.createBuffer({
    size: 4 * 4, // 4 f32 (2 real fields + 2 padding), matches the WGSL ColorNrParams struct
    usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
  });

  // Histogram: RENDER_ATTACHMENT so fs_final can draw into it (see
  // writeAdjustmentsAndRender), COPY_SRC so its contents can be copied
  // out to histogramReadbackBuffer below. Same presentationFormat as the
  // canvas itself -- a render pass's color attachment format must
  // exactly match the pipeline it's used with, and `pipeline` (fs_final)
  // was already created with that target format.
  gpu.histogramTex = gpu.device.createTexture({
    size: [HISTOGRAM_SIZE, HISTOGRAM_SIZE],
    format: gpu.presentationFormat,
    usage: GPUTextureUsage.RENDER_ATTACHMENT | GPUTextureUsage.COPY_SRC,
  });
  // bytesPerRow (256 texels x 4 bytes/texel = 1024) is already a
  // multiple of 256 -- WebGPU's own copyTextureToBuffer alignment
  // requirement -- so no row padding is needed here, unlike a
  // less-conveniently-sized readback would require.
  gpu.histogramReadbackBuffer = gpu.device.createBuffer({
    size: HISTOGRAM_SIZE * 4 * HISTOGRAM_SIZE,
    usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ,
  });
  gpu.clippingBuffer = gpu.device.createBuffer({
    size: 4 * 4, // 4 f32 (1 real field + 3 padding), matches the WGSL Clipping struct
    usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
  });
}
