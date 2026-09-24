// Uploads a decoded source image to the GPU: creates the source texture and every intermediate
// texture and bind group sized to it. Moved out of DevelopCanvas.svelte.

import { buildAtmLightChainSizes } from "$lib/gpu/atmChain.js";
import { MAX_MASKS } from "./gpuHandles.js";

export async function applyBitmapToGpu(/** @type {import('./gpuHandles.js').GpuHandles} */ gpu, /** @type {ImageBitmap} */ bitmap, /** @type {import('./gpuHandles.js').GpuHooks} */ hooks) {
  // Both callers (loadImage, upgradeToFullTier) already only reach here
  // once initGpu has run, but re-asserted here too -- both for a real
  // defensive guard against an unexpected call order, and because
  // TypeScript's null-narrowing from a caller's own guard doesn't carry
  // across a function boundary.
  if (!gpu.device || !gpu.context || !gpu.pipeline || !gpu.preMaskPipeline || !gpu.lensCorrectPipeline || !gpu.perspectivePipeline || !gpu.gradePipeline || !gpu.atmReducePipeline || !gpu.minChannelPipeline || !gpu.minHPipeline || !gpu.minVPipeline || !gpu.dehazeMeanguideHPipeline || !gpu.dehazeMeanguideVPipeline || !gpu.dehazeMeanpHPipeline || !gpu.dehazeMeanpVPipeline || !gpu.dehazeCorrguideHPipeline || !gpu.dehazeCorrguideVPipeline || !gpu.dehazeCorrguidepHPipeline || !gpu.dehazeCorrguidepVPipeline || !gpu.dehazeAPipeline || !gpu.dehazeBPipeline || !gpu.dehazeMeanaHPipeline || !gpu.dehazeMeanaVPipeline || !gpu.dehazeMeanbHPipeline || !gpu.dehazeMeanbVPipeline || !gpu.dehazeRefinePipeline || !gpu.textureHPipeline || !gpu.textureVPipeline || !gpu.clarityMeanpHPipeline || !gpu.clarityMeanpVPipeline || !gpu.clarityCorrpHPipeline || !gpu.clarityCorrpVPipeline || !gpu.clarityAPipeline || !gpu.clarityBPipeline || !gpu.clarityMeanaHPipeline || !gpu.clarityMeanaVPipeline || !gpu.clarityMeanbHPipeline || !gpu.clarityMeanbVPipeline || !gpu.clarityVPipeline || !gpu.sharpenHPipeline || !gpu.sharpenVPipeline || !gpu.lumaNRHPipeline || !gpu.lumaNRVPipeline || !gpu.colorNRHPipeline || !gpu.colorNRVPipeline || !gpu.uniformBuffer || !gpu.masksBuffer || !gpu.curveLutBuffer || !gpu.hslBandsBuffer || !gpu.splitToningBuffer || !gpu.vignetteBuffer || !gpu.lensCorrectionBuffer || !gpu.perspectiveBuffer || !gpu.grainBuffer || !gpu.sharpenBuffer || !gpu.lumaNRBuffer || !gpu.colorNRBuffer || !gpu.clippingBuffer) return;

  // GPU texture-dimension safety: a genuinely native-resolution decode
  // (the 1:1 tier, upgradeToFullTier) could in principle exceed this
  // device's actual texture-size limit on a very-high-megapixel body --
  // the draft tier is already capped to DEVELOP_PREVIEW_MAX_DIMENSION so
  // this is normally a no-op there. Downscaling defensively here (one
  // code path, both tiers) is an honest, accepted degradation on
  // whatever hardware this ends up mattering for, not a crash from an
  // opaque WebGPU validation error.
  const maxDim = gpu.device.limits.maxTextureDimension2D;
  if (bitmap.width > maxDim || bitmap.height > maxDim) {
    const scale = maxDim / Math.max(bitmap.width, bitmap.height);
    bitmap = await createImageBitmap(bitmap, {
      resizeWidth: Math.max(1, Math.round(bitmap.width * scale)),
      resizeHeight: Math.max(1, Math.round(bitmap.height * scale)),
      resizeQuality: "high",
    });
  }

  gpu.sourceTexture?.destroy();
  gpu.sourceTexture = gpu.device.createTexture({
    size: [bitmap.width, bitmap.height],
    format: "rgba8unorm",
    usage:
      GPUTextureUsage.TEXTURE_BINDING |
      GPUTextureUsage.COPY_DST |
      GPUTextureUsage.RENDER_ATTACHMENT,
  });
  gpu.device.queue.copyExternalImageToTexture(
    { source: bitmap },
    { texture: gpu.sourceTexture },
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
  gpu.sourceSampleCanvas = new OffscreenCanvas(bitmap.width, bitmap.height);
  gpu.sourceSampleCtx = /** @type {OffscreenCanvasRenderingContext2D} */ (gpu.sourceSampleCanvas.getContext("2d"));
  gpu.sourceSampleCtx.drawImage(bitmap, 0, 0);

  // M3 Slice 7: recreated whenever the active bitmap's resolution
  // changes (new image, OR a tier upgrade) -- must be sized to match, an
  // OffscreenCanvas at the wrong resolution would rasterize dabs at the
  // wrong scale. Existing brush masks' dab lists are stored normalized
  // (0-1), so re-rasterizing from scratch into freshly-sized canvases
  // (via syncBrushRasterization, called below through this function's
  // caller's own writeAdjustmentsAndRender()) is correct with no
  // special-casing regardless of why the resolution changed.
  gpu.brushTextureArray?.destroy();
  gpu.brushTextureArray = gpu.device.createTexture({
    size: [bitmap.width, bitmap.height, MAX_MASKS],
    format: "rgba8unorm",
    usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.COPY_DST,
  });
  gpu.brushRasterState = new Map();
  gpu.freeBrushLayers = Array.from({ length: MAX_MASKS }, (_, i) => i);

  // Lens Corrections (M3): same "recreate whenever the source resolution
  // changes" lifecycle as sourceTexture/brushTextureArray above --
  // fs_lens_correct's own output, read by fs_perspective in place of
  // sourceTexture (see perspectiveBindGroup's own doc comment below).
  gpu.lensCorrectedTex?.destroy();
  gpu.lensCorrectedTex = gpu.device.createTexture({
    size: [bitmap.width, bitmap.height],
    format: "rgba16float",
    usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.RENDER_ATTACHMENT,
  });

  // Perspective Correction (M4): same lifecycle as lensCorrectedTex
  // above -- fs_perspective's own output, read by fs_grade in place of
  // lensCorrectedTex (see gradeBindGroup's own doc comment below).
  gpu.perspectiveCorrectedTex?.destroy();
  gpu.perspectiveCorrectedTex = gpu.device.createTexture({
    size: [bitmap.width, bitmap.height],
    format: "rgba16float",
    usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.RENDER_ATTACHMENT,
  });

  // Dehaze (M3): intermediates sized to match this bitmap's own
  // resolution -- same "recreate whenever the source resolution changes"
  // lifecycle as sourceTexture/brushTextureArray above.
  gpu.gradedTex?.destroy();
  gpu.gradedTex = gpu.device.createTexture({
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
  gpu.preMaskTex?.destroy();
  gpu.preMaskTex = gpu.device.createTexture({
    size: [bitmap.width, bitmap.height],
    format: "rgba16float",
    usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.RENDER_ATTACHMENT,
  });

  gpu.minChannelTex?.destroy();
  gpu.minChannelTex = gpu.device.createTexture({
    size: [bitmap.width, bitmap.height],
    format: "r32float",
    usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.RENDER_ATTACHMENT,
  });
  gpu.darkChannelHTex?.destroy();
  gpu.darkChannelHTex = gpu.device.createTexture({
    size: [bitmap.width, bitmap.height],
    format: "r32float",
    usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.RENDER_ATTACHMENT,
  });
  gpu.tRawTex?.destroy();
  gpu.tRawTex = gpu.device.createTexture({
    size: [bitmap.width, bitmap.height],
    format: "r32float",
    usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.RENDER_ATTACHMENT,
  });
  gpu.transmissionHTex?.destroy();
  gpu.transmissionHTex = gpu.device.createTexture({
    size: [bitmap.width, bitmap.height],
    format: "r32float",
    usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.RENDER_ATTACHMENT,
  });
  gpu.transmissionTex?.destroy();
  gpu.transmissionTex = gpu.device.createTexture({
    size: [bitmap.width, bitmap.height],
    format: "r32float",
    usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.RENDER_ATTACHMENT,
  });
  // Transmission refinement's guided filter (RFC-0011): eight new
  // persistent single-channel intermediates -- same "own fixed binding
  // instead of a rebinding trick" reasoning as Clarity's own six (RFC-0010),
  // since more than one is read simultaneously by a later pass.
  gpu.dehazeMeanGuideTex?.destroy();
  gpu.dehazeMeanGuideTex = gpu.device.createTexture({
    size: [bitmap.width, bitmap.height],
    format: "r32float",
    usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.RENDER_ATTACHMENT,
  });
  gpu.dehazeMeanPTex?.destroy();
  gpu.dehazeMeanPTex = gpu.device.createTexture({
    size: [bitmap.width, bitmap.height],
    format: "r32float",
    usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.RENDER_ATTACHMENT,
  });
  gpu.dehazeCorrGuideTex?.destroy();
  gpu.dehazeCorrGuideTex = gpu.device.createTexture({
    size: [bitmap.width, bitmap.height],
    format: "r32float",
    usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.RENDER_ATTACHMENT,
  });
  gpu.dehazeCorrGuidePTex?.destroy();
  gpu.dehazeCorrGuidePTex = gpu.device.createTexture({
    size: [bitmap.width, bitmap.height],
    format: "r32float",
    usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.RENDER_ATTACHMENT,
  });
  gpu.dehazeATex?.destroy();
  gpu.dehazeATex = gpu.device.createTexture({
    size: [bitmap.width, bitmap.height],
    format: "r32float",
    usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.RENDER_ATTACHMENT,
  });
  gpu.dehazeBTex?.destroy();
  gpu.dehazeBTex = gpu.device.createTexture({
    size: [bitmap.width, bitmap.height],
    format: "r32float",
    usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.RENDER_ATTACHMENT,
  });
  gpu.dehazeMeanATex?.destroy();
  gpu.dehazeMeanATex = gpu.device.createTexture({
    size: [bitmap.width, bitmap.height],
    format: "r32float",
    usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.RENDER_ATTACHMENT,
  });
  gpu.dehazeMeanBTex?.destroy();
  gpu.dehazeMeanBTex = gpu.device.createTexture({
    size: [bitmap.width, bitmap.height],
    format: "r32float",
    usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.RENDER_ATTACHMENT,
  });

  // Texture & Clarity (M3): same "recreate whenever source resolution
  // changes" lifecycle as every intermediate above. textureAdjustedTex
  // is Texture's final (post-apply) output and Clarity's own input --
  // Clarity's own final output overwrites gradedTex in place (see
  // fs_clarity_v's doc comment), so it needs no texture of its own here.
  gpu.textureBlurScratchTex?.destroy();
  gpu.textureBlurScratchTex = gpu.device.createTexture({
    size: [bitmap.width, bitmap.height],
    format: "r32float",
    usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.RENDER_ATTACHMENT,
  });
  gpu.textureAdjustedTex?.destroy();
  gpu.textureAdjustedTex = gpu.device.createTexture({
    size: [bitmap.width, bitmap.height],
    format: "rgba16float",
    usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.RENDER_ATTACHMENT,
  });
  gpu.clarityBlurScratchTex?.destroy();
  gpu.clarityBlurScratchTex = gpu.device.createTexture({
    size: [bitmap.width, bitmap.height],
    format: "r32float",
    usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.RENDER_ATTACHMENT,
  });
  // Clarity's guided filter (RFC-0010): six new persistent single-channel
  // intermediates -- see dehazeLocalContrast.js's own doc comment for why
  // each needs its own texture rather than reusing clarityBlurScratchTex's
  // rebinding trick (more than one is read simultaneously by a later
  // pass). Same lifecycle as every intermediate above.
  gpu.clarityMeanPTex?.destroy();
  gpu.clarityMeanPTex = gpu.device.createTexture({
    size: [bitmap.width, bitmap.height],
    format: "r32float",
    usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.RENDER_ATTACHMENT,
  });
  gpu.clarityCorrPTex?.destroy();
  gpu.clarityCorrPTex = gpu.device.createTexture({
    size: [bitmap.width, bitmap.height],
    format: "r32float",
    usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.RENDER_ATTACHMENT,
  });
  gpu.clarityATex?.destroy();
  gpu.clarityATex = gpu.device.createTexture({
    size: [bitmap.width, bitmap.height],
    format: "r32float",
    usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.RENDER_ATTACHMENT,
  });
  gpu.clarityBTex?.destroy();
  gpu.clarityBTex = gpu.device.createTexture({
    size: [bitmap.width, bitmap.height],
    format: "r32float",
    usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.RENDER_ATTACHMENT,
  });
  gpu.clarityMeanATex?.destroy();
  gpu.clarityMeanATex = gpu.device.createTexture({
    size: [bitmap.width, bitmap.height],
    format: "r32float",
    usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.RENDER_ATTACHMENT,
  });
  gpu.clarityMeanBTex?.destroy();
  gpu.clarityMeanBTex = gpu.device.createTexture({
    size: [bitmap.width, bitmap.height],
    format: "r32float",
    usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.RENDER_ATTACHMENT,
  });

  // Sharpening / Noise Reduction (M3): same "recreate whenever source
  // resolution changes" lifecycle as every intermediate above.
  gpu.sharpenBlurHTex?.destroy();
  gpu.sharpenBlurHTex = gpu.device.createTexture({
    size: [bitmap.width, bitmap.height],
    format: "r32float",
    usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.RENDER_ATTACHMENT,
  });
  gpu.sharpenBlurTex?.destroy();
  gpu.sharpenBlurTex = gpu.device.createTexture({
    size: [bitmap.width, bitmap.height],
    format: "r32float",
    usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.RENDER_ATTACHMENT,
  });
  gpu.lumaNRBlurHTex?.destroy();
  gpu.lumaNRBlurHTex = gpu.device.createTexture({
    size: [bitmap.width, bitmap.height],
    format: "r32float",
    usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.RENDER_ATTACHMENT,
  });
  gpu.lumaNRBlurTex?.destroy();
  gpu.lumaNRBlurTex = gpu.device.createTexture({
    size: [bitmap.width, bitmap.height],
    format: "r32float",
    usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.RENDER_ATTACHMENT,
  });
  gpu.colorNRBlurHTex?.destroy();
  gpu.colorNRBlurHTex = gpu.device.createTexture({
    size: [bitmap.width, bitmap.height],
    format: "rgba16float",
    usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.RENDER_ATTACHMENT,
  });
  gpu.colorNRBlurTex?.destroy();
  gpu.colorNRBlurTex = gpu.device.createTexture({
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
  gpu.atmLightChain.forEach((tex) => tex.destroy());
  const chainSizes = buildAtmLightChainSizes(bitmap.width, bitmap.height);
  // Captured as a local `const` -- TS can't narrow the outer `device`/
  // `atmReducePipeline` `let`s (reassignable elsewhere in this module)
  // across a closure boundary, even though the top-of-function guard
  // above already ensures both are non-null for this entire call.
  const gpuDevice = gpu.device;
  const reducePipeline = gpu.atmReducePipeline;
  gpu.atmLightChain = chainSizes.map(([w, h]) =>
    gpuDevice.createTexture({
      size: [w, h],
      format: "rgba16float",
      usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.RENDER_ATTACHMENT,
    }),
  );
  const atmLightFinalTex = gpu.atmLightChain[gpu.atmLightChain.length - 1];

  const sampler = gpu.device.createSampler({ magFilter: "linear", minFilter: "linear" });

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
  gpu.lensCorrectBindGroup = gpuDevice.createBindGroup({
    layout: gpu.lensCorrectPipeline.getBindGroupLayout(0),
    entries: [
      { binding: 0, resource: sampler },
      { binding: 1, resource: gpu.sourceTexture.createView() },
      { binding: 25, resource: { buffer: gpu.lensCorrectionBuffer } },
    ],
  });
  // Perspective Correction (M4): same "same slot, different texture"
  // technique -- reads lensCorrectedTex (lens correction's own output)
  // at binding 1, not the true original sourceTexture.
  gpu.perspectiveBindGroup = gpuDevice.createBindGroup({
    layout: /** @type {GPURenderPipeline} */ (gpu.perspectivePipeline).getBindGroupLayout(0),
    entries: [
      { binding: 0, resource: sampler },
      { binding: 1, resource: gpu.lensCorrectedTex.createView() },
      { binding: 28, resource: { buffer: gpu.perspectiveBuffer } },
    ],
  });
  // M4 Slice 2: before/after preview (see fs_original's own doc comment).
  gpu.originalBindGroup = gpuDevice.createBindGroup({
    layout: /** @type {GPURenderPipeline} */ (gpu.originalPipeline).getBindGroupLayout(0),
    entries: [
      { binding: 0, resource: sampler },
      { binding: 1, resource: gpu.sourceTexture.createView() },
    ],
  });
  gpu.gradeBindGroup = gpuDevice.createBindGroup({
    layout: gpu.gradePipeline.getBindGroupLayout(0),
    entries: [
      { binding: 0, resource: sampler },
      { binding: 1, resource: /** @type {GPUTexture} */ (gpu.perspectiveCorrectedTex).createView() },
      { binding: 2, resource: { buffer: gpu.uniformBuffer } },
      { binding: 5, resource: { buffer: gpu.curveLutBuffer } },
      { binding: 6, resource: { buffer: gpu.hslBandsBuffer } },
      { binding: 7, resource: { buffer: gpu.splitToningBuffer } },
    ],
  });

  // One bind group per reduction pass -- `reduceInput` (binding 9) is
  // rebound to a DIFFERENT actual texture each step (gradedTex for the
  // first pass, then each successively-smaller chain texture in turn),
  // the SAME atmReducePipeline object reused for every draw call.
  gpu.atmReduceBindGroups = chainSizes.map((_, i) => {
    const input = /** @type {GPUTexture} */ (i === 0 ? gpu.gradedTex : gpu.atmLightChain[i - 1]);
    return gpuDevice.createBindGroup({
      layout: reducePipeline.getBindGroupLayout(0),
      entries: [{ binding: 9, resource: input.createView() }],
    });
  });

  gpu.minChannelBindGroup = gpuDevice.createBindGroup({
    layout: gpu.minChannelPipeline.getBindGroupLayout(0),
    entries: [
      { binding: 8, resource: gpu.gradedTex.createView() },
      { binding: 10, resource: atmLightFinalTex.createView() },
    ],
  });
  gpu.minHBindGroup = gpuDevice.createBindGroup({
    layout: gpu.minHPipeline.getBindGroupLayout(0),
    entries: [{ binding: 11, resource: gpu.minChannelTex.createView() }],
  });
  gpu.minVBindGroup = gpuDevice.createBindGroup({
    layout: gpu.minVPipeline.getBindGroupLayout(0),
    entries: [{ binding: 11, resource: gpu.darkChannelHTex.createView() }],
  });
  // Transmission refinement's guided filter (RFC-0011): 15 bind groups
  // replacing the old meanHBindGroup/meanVBindGroup pair. Every H-pass
  // here writes into transmissionHTex, completed by its own V-pass
  // reading it back via filterInput's existing rebinding role (binding
  // 11) -- same discipline as every other box-filter pair in this file.
  gpu.dehazeMeanguideHBindGroup = gpuDevice.createBindGroup({
    layout: gpu.dehazeMeanguideHPipeline.getBindGroupLayout(0),
    entries: [{ binding: 8, resource: gpu.gradedTex.createView() }],
  });
  gpu.dehazeMeanguideVBindGroup = gpuDevice.createBindGroup({
    layout: gpu.dehazeMeanguideVPipeline.getBindGroupLayout(0),
    entries: [{ binding: 11, resource: gpu.transmissionHTex.createView() }],
  });
  gpu.dehazeMeanpHBindGroup = gpuDevice.createBindGroup({
    layout: gpu.dehazeMeanpHPipeline.getBindGroupLayout(0),
    entries: [{ binding: 11, resource: gpu.tRawTex.createView() }],
  });
  gpu.dehazeMeanpVBindGroup = gpuDevice.createBindGroup({
    layout: gpu.dehazeMeanpVPipeline.getBindGroupLayout(0),
    entries: [{ binding: 11, resource: gpu.transmissionHTex.createView() }],
  });
  gpu.dehazeCorrguideHBindGroup = gpuDevice.createBindGroup({
    layout: gpu.dehazeCorrguideHPipeline.getBindGroupLayout(0),
    entries: [{ binding: 8, resource: gpu.gradedTex.createView() }],
  });
  gpu.dehazeCorrguideVBindGroup = gpuDevice.createBindGroup({
    layout: gpu.dehazeCorrguideVPipeline.getBindGroupLayout(0),
    entries: [{ binding: 11, resource: gpu.transmissionHTex.createView() }],
  });
  gpu.dehazeCorrguidepHBindGroup = gpuDevice.createBindGroup({
    layout: gpu.dehazeCorrguidepHPipeline.getBindGroupLayout(0),
    entries: [
      { binding: 8, resource: gpu.gradedTex.createView() },
      { binding: 11, resource: gpu.tRawTex.createView() },
    ],
  });
  gpu.dehazeCorrguidepVBindGroup = gpuDevice.createBindGroup({
    layout: gpu.dehazeCorrguidepVPipeline.getBindGroupLayout(0),
    entries: [{ binding: 11, resource: gpu.transmissionHTex.createView() }],
  });
  gpu.dehazeABindGroup = gpuDevice.createBindGroup({
    layout: gpu.dehazeAPipeline.getBindGroupLayout(0),
    entries: [
      { binding: 35, resource: gpu.dehazeMeanGuideTex.createView() },
      { binding: 36, resource: gpu.dehazeMeanPTex.createView() },
      { binding: 37, resource: gpu.dehazeCorrGuideTex.createView() },
      { binding: 38, resource: gpu.dehazeCorrGuidePTex.createView() },
    ],
  });
  gpu.dehazeBBindGroup = gpuDevice.createBindGroup({
    layout: gpu.dehazeBPipeline.getBindGroupLayout(0),
    entries: [
      { binding: 35, resource: gpu.dehazeMeanGuideTex.createView() },
      { binding: 36, resource: gpu.dehazeMeanPTex.createView() },
      { binding: 39, resource: gpu.dehazeATex.createView() },
    ],
  });
  gpu.dehazeMeanaHBindGroup = gpuDevice.createBindGroup({
    layout: gpu.dehazeMeanaHPipeline.getBindGroupLayout(0),
    entries: [{ binding: 39, resource: gpu.dehazeATex.createView() }],
  });
  gpu.dehazeMeanaVBindGroup = gpuDevice.createBindGroup({
    layout: gpu.dehazeMeanaVPipeline.getBindGroupLayout(0),
    entries: [{ binding: 11, resource: gpu.transmissionHTex.createView() }],
  });
  gpu.dehazeMeanbHBindGroup = gpuDevice.createBindGroup({
    layout: gpu.dehazeMeanbHPipeline.getBindGroupLayout(0),
    entries: [{ binding: 40, resource: gpu.dehazeBTex.createView() }],
  });
  gpu.dehazeMeanbVBindGroup = gpuDevice.createBindGroup({
    layout: gpu.dehazeMeanbVPipeline.getBindGroupLayout(0),
    entries: [{ binding: 11, resource: gpu.transmissionHTex.createView() }],
  });
  gpu.dehazeRefineBindGroup = gpuDevice.createBindGroup({
    layout: gpu.dehazeRefinePipeline.getBindGroupLayout(0),
    entries: [
      { binding: 8, resource: gpu.gradedTex.createView() },
      { binding: 41, resource: gpu.dehazeMeanATex.createView() },
      { binding: 42, resource: gpu.dehazeMeanBTex.createView() },
    ],
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
  gpu.textureHBindGroup = gpuDevice.createBindGroup({
    layout: gpu.textureHPipeline.getBindGroupLayout(0),
    entries: [{ binding: 13, resource: gpu.gradedTex.createView() }],
  });
  gpu.textureVBindGroup = gpuDevice.createBindGroup({
    layout: gpu.textureVPipeline.getBindGroupLayout(0),
    entries: [
      { binding: 13, resource: gpu.gradedTex.createView() },
      { binding: 14, resource: gpu.textureBlurScratchTex.createView() },
      { binding: 2, resource: { buffer: gpu.uniformBuffer } },
    ],
  });
  // Clarity's guided filter (RFC-0010): 10 bind groups replacing the old
  // clarityHBindGroup/clarityVBindGroup pair. Every mean_p/corr_p/a/b/
  // mean_a/mean_b pass reads textureAdjustedTex (binding 13) or the
  // shared H-scratch clarityBlurScratchTex (binding 14) exactly like
  // Texture's own bind groups above -- see dehazeLocalContrast.js's own
  // doc comment for why each of clarityMeanPTex/clarityCorrPTex/
  // clarityATex/clarityBTex/clarityMeanATex/clarityMeanBTex gets its own
  // fixed binding (29-34) instead of reusing the rebinding trick. Each
  // bind group's entries are exactly what that entry point's own WGSL
  // body references -- `layout: "auto"` infers a layout per entry point,
  // so a binding this specific pass doesn't read must not be listed here
  // (same discipline every other bind group in this file already
  // follows).
  gpu.clarityMeanpHBindGroup = gpuDevice.createBindGroup({
    layout: gpu.clarityMeanpHPipeline.getBindGroupLayout(0),
    entries: [{ binding: 13, resource: gpu.textureAdjustedTex.createView() }],
  });
  gpu.clarityMeanpVBindGroup = gpuDevice.createBindGroup({
    layout: gpu.clarityMeanpVPipeline.getBindGroupLayout(0),
    entries: [{ binding: 14, resource: gpu.clarityBlurScratchTex.createView() }],
  });
  gpu.clarityCorrpHBindGroup = gpuDevice.createBindGroup({
    layout: gpu.clarityCorrpHPipeline.getBindGroupLayout(0),
    entries: [{ binding: 13, resource: gpu.textureAdjustedTex.createView() }],
  });
  gpu.clarityCorrpVBindGroup = gpuDevice.createBindGroup({
    layout: gpu.clarityCorrpVPipeline.getBindGroupLayout(0),
    entries: [{ binding: 14, resource: gpu.clarityBlurScratchTex.createView() }],
  });
  gpu.clarityABindGroup = gpuDevice.createBindGroup({
    layout: gpu.clarityAPipeline.getBindGroupLayout(0),
    entries: [
      { binding: 29, resource: gpu.clarityMeanPTex.createView() },
      { binding: 30, resource: gpu.clarityCorrPTex.createView() },
    ],
  });
  gpu.clarityBBindGroup = gpuDevice.createBindGroup({
    layout: gpu.clarityBPipeline.getBindGroupLayout(0),
    entries: [
      { binding: 29, resource: gpu.clarityMeanPTex.createView() },
      { binding: 31, resource: gpu.clarityATex.createView() },
    ],
  });
  gpu.clarityMeanaHBindGroup = gpuDevice.createBindGroup({
    layout: gpu.clarityMeanaHPipeline.getBindGroupLayout(0),
    entries: [{ binding: 31, resource: gpu.clarityATex.createView() }],
  });
  gpu.clarityMeanaVBindGroup = gpuDevice.createBindGroup({
    layout: gpu.clarityMeanaVPipeline.getBindGroupLayout(0),
    entries: [{ binding: 14, resource: gpu.clarityBlurScratchTex.createView() }],
  });
  gpu.clarityMeanbHBindGroup = gpuDevice.createBindGroup({
    layout: gpu.clarityMeanbHPipeline.getBindGroupLayout(0),
    entries: [{ binding: 32, resource: gpu.clarityBTex.createView() }],
  });
  gpu.clarityMeanbVBindGroup = gpuDevice.createBindGroup({
    layout: gpu.clarityMeanbVPipeline.getBindGroupLayout(0),
    entries: [{ binding: 14, resource: gpu.clarityBlurScratchTex.createView() }],
  });
  gpu.clarityVBindGroup = gpuDevice.createBindGroup({
    layout: gpu.clarityVPipeline.getBindGroupLayout(0),
    entries: [
      { binding: 13, resource: gpu.textureAdjustedTex.createView() },
      { binding: 33, resource: gpu.clarityMeanATex.createView() },
      { binding: 34, resource: gpu.clarityMeanBTex.createView() },
      { binding: 2, resource: { buffer: gpu.uniformBuffer } },
    ],
  });

  // Sharpening / Noise Reduction (M3): all three H-passes read
  // gradedTex(8) DIRECTLY (never rebound the way Texture/Clarity's own
  // lcRgbInput is) -- they always read the SAME pre-Dehaze-recovery
  // snapshot, so no per-pass rebinding is needed. Sharpen's own H/V
  // passes additionally need binding 19 (sharpenParams) for its
  // uniform-driven radius; Luma/Color NR's radii are fixed WGSL consts,
  // so their own H/V bind groups need no uniform at all.
  gpu.sharpenHBindGroup = gpuDevice.createBindGroup({
    layout: gpu.sharpenHPipeline.getBindGroupLayout(0),
    entries: [
      { binding: 8, resource: gpu.gradedTex.createView() },
      { binding: 19, resource: { buffer: gpu.sharpenBuffer } },
    ],
  });
  gpu.sharpenVBindGroup = gpuDevice.createBindGroup({
    layout: gpu.sharpenVPipeline.getBindGroupLayout(0),
    entries: [
      { binding: 17, resource: gpu.sharpenBlurHTex.createView() },
      { binding: 19, resource: { buffer: gpu.sharpenBuffer } },
    ],
  });
  gpu.lumaNRHBindGroup = gpuDevice.createBindGroup({
    layout: gpu.lumaNRHPipeline.getBindGroupLayout(0),
    entries: [{ binding: 8, resource: gpu.gradedTex.createView() }],
  });
  gpu.lumaNRVBindGroup = gpuDevice.createBindGroup({
    layout: gpu.lumaNRVPipeline.getBindGroupLayout(0),
    entries: [{ binding: 17, resource: gpu.lumaNRBlurHTex.createView() }],
  });
  gpu.colorNRHBindGroup = gpuDevice.createBindGroup({
    layout: gpu.colorNRHPipeline.getBindGroupLayout(0),
    entries: [{ binding: 8, resource: gpu.gradedTex.createView() }],
  });
  gpu.colorNRVBindGroup = gpuDevice.createBindGroup({
    layout: gpu.colorNRVPipeline.getBindGroupLayout(0),
    entries: [{ binding: 18, resource: gpu.colorNRBlurHTex.createView() }],
  });

  // Pre-mask pass's own bind group (M4 Slice 1): exactly the entry set
  // the OLD single-pass fs_final's own bind group used to carry for its
  // Dehaze/NR/Sharpen/Vignette/Grain half (0,2,3,4,26 removed -- those
  // are mask-loop/clipping-only, now fs_mask's job below).
  gpu.preMaskBindGroup = gpuDevice.createBindGroup({
    layout: gpu.preMaskPipeline.getBindGroupLayout(0),
    entries: [
      { binding: 2, resource: { buffer: gpu.uniformBuffer } },
      { binding: 8, resource: gpu.gradedTex.createView() },
      { binding: 10, resource: atmLightFinalTex.createView() },
      { binding: 12, resource: gpu.transmissionTex.createView() },
      { binding: 15, resource: { buffer: gpu.vignetteBuffer } },
      { binding: 16, resource: { buffer: gpu.grainBuffer } },
      { binding: 19, resource: { buffer: gpu.sharpenBuffer } },
      { binding: 20, resource: { buffer: gpu.lumaNRBuffer } },
      { binding: 21, resource: { buffer: gpu.colorNRBuffer } },
      { binding: 22, resource: gpu.sharpenBlurTex.createView() },
      { binding: 23, resource: gpu.lumaNRBlurTex.createView() },
      { binding: 24, resource: gpu.colorNRBlurTex.createView() },
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
  gpu.bindGroup = gpuDevice.createBindGroup({
    layout: gpu.pipeline.getBindGroupLayout(0),
    entries: [
      { binding: 0, resource: sampler },
      { binding: 2, resource: { buffer: gpu.uniformBuffer } },
      { binding: 3, resource: { buffer: gpu.masksBuffer } },
      { binding: 4, resource: gpu.brushTextureArray.createView({ dimension: "2d-array" }) },
      { binding: 26, resource: { buffer: gpu.clippingBuffer } },
      { binding: 27, resource: gpu.preMaskTex.createView() },
    ],
  });
  // Every intermediate above is freshly (re)created for this bitmap --
  // any previously-cached dirty key belonged to a DIFFERENT image/tier's
  // now-destroyed textures, so it must not be trusted to skip
  // recomputing the expensive passes on the next render.
  gpu.spatialOpsInputsKey = null;

  if (gpu.context.canvas instanceof HTMLCanvasElement) {
    gpu.context.canvas.width = bitmap.width;
    gpu.context.canvas.height = bitmap.height;
  }
  hooks.onSourceDimensions(bitmap.width, bitmap.height);
  gpu.context.configure({ device: gpu.device, format: gpu.presentationFormat, alphaMode: "opaque" });
  // Every remaining bitmap.width/.height read is done -- free it now
  // that both its consumers (the GPU upload and the sample-canvas draw
  // above) are finished with it.
  bitmap.close();
}
