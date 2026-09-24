// Draws one frame: packs the adjustment uniforms, runs every pass in order, and reads back the
// histogram. Also brush-mask rasterization sync. Moved out of DevelopCanvas.svelte.

import { binHistogramPixels } from "$lib/histogramMath.js";
import { rasterizeSpotDab, rasterizeDab } from "$lib/gpu/brushRaster.js";
import { buildToneCurveLut, buildHslUniformData, buildSplitToningUniformData, buildLensCorrectionUniformData, buildPerspectiveUniformData, buildVignetteUniformData, buildGrainUniformData, buildSharpenUniformData, buildLumaNrUniformData, buildColorNrUniformData } from "$lib/api/develop.js";
import { spotCentroidAndRadius } from "$lib/maskGeometry.js";
import { MAX_MASKS, HISTOGRAM_SIZE } from "./gpuHandles.js";

/** One fullscreen-triangle draw into `outputView` -- the shared shape
 * every dehaze pass and the existing final pass use, factored out to
 * avoid repeating the same beginRenderPass/setPipeline/setBindGroup/
 * draw/end boilerplate for what's now up to ~9 passes per render. */
export function runFullscreenPass(
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
export async function readHistogramIfIdle(/** @type {import('./gpuHandles.js').GpuHandles} */ gpu, /** @type {import('./gpuHandles.js').RenderInputs} */ inputs) {
  if (gpu.histogramReadInFlight || !gpu.histogramReadbackBuffer) return;
  gpu.histogramReadInFlight = true;
  try {
    await gpu.histogramReadbackBuffer.mapAsync(GPUMapMode.READ);
    // Copied out via slice(0) BEFORE unmap() -- the ArrayBuffer
    // getMappedRange() returns is detached (zero-length) the instant
    // unmap() runs, so onHistogramUpdate's caller can't be handed a
    // view over it directly.
    const data = new Uint8Array(gpu.histogramReadbackBuffer.getMappedRange().slice(0));
    gpu.histogramReadbackBuffer.unmap();
    // Kept around (not just passed to onHistogramUpdate) so the pointer
    // hover-readout below can map a screen position to the SAME 256x256
    // downsampled sample the histogram itself is drawn from, without a
    // second GPU round-trip -- see reportHoverPixel.
    gpu.lastHistogramPixels = data;
    if (inputs.onHistogramUpdate) inputs.onHistogramUpdate(binHistogramPixels(data, gpu.presentationFormat.startsWith("bgra") ? "bgra" : "rgba"));
  } catch {
    // A stale/aborted map (e.g. the device was torn down mid-await, on
    // unmount) isn't user-visible -- the next render's own call simply
    // tries again.
  } finally {
    gpu.histogramReadInFlight = false;
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
export function syncMaskRasterization(/** @type {import('./gpuHandles.js').GpuHandles} */ gpu, /** @type {import('./gpuHandles.js').RenderInputs} */ inputs) {
  if (!gpu.device || !gpu.brushTextureArray) return;
  const presentIds = new Set();
  for (const mask of inputs.masks) {
    const isSpot = mask.op === "spot_mask";
    if (mask.op !== "brush_mask" && !isSpot) continue;
    presentIds.add(mask.id);
    let entry = gpu.brushRasterState.get(mask.id);
    if (!entry) {
      const layer = gpu.freeBrushLayers.shift();
      // Combined MAX_MASKS budget exhausted -- MaskToolStrip's atCap
      // check already prevents creating a mask that would hit this, so
      // this is a defensive no-op, not an expected path.
      if (layer === undefined) continue;
      const canvas = new OffscreenCanvas(gpu.brushTextureArray.width, gpu.brushTextureArray.height);
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
      gpu.brushRasterState.set(mask.id, entry);
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
      gpu.device.queue.writeTexture(
        { texture: gpu.brushTextureArray, origin: { x: 0, y: 0, z: entry.layer } },
        imageData.data,
        { bytesPerRow: entry.canvas.width * 4, rowsPerImage: entry.canvas.height },
        { width: entry.canvas.width, height: entry.canvas.height },
      );
    }
  }
  for (const [id, entry] of gpu.brushRasterState) {
    if (!presentIds.has(id)) {
      gpu.freeBrushLayers.push(entry.layer);
      gpu.brushRasterState.delete(id);
    }
  }
}

export function writeAdjustmentsAndRender(/** @type {import('./gpuHandles.js').GpuHandles} */ gpu, /** @type {import('./gpuHandles.js').RenderInputs} */ inputs) {
  if (!gpu.device || !gpu.context || !gpu.pipeline || !gpu.bindGroup || !gpu.preMaskPipeline || !gpu.preMaskBindGroup || !gpu.preMaskTex || !gpu.lensCorrectPipeline || !gpu.lensCorrectBindGroup || !gpu.lensCorrectedTex || !gpu.perspectivePipeline || !gpu.perspectiveBindGroup || !gpu.perspectiveCorrectedTex || !gpu.gradePipeline || !gpu.gradeBindGroup || !gpu.atmReducePipeline || gpu.atmReduceBindGroups.length === 0 || !gpu.minChannelPipeline || !gpu.minChannelBindGroup || !gpu.minHPipeline || !gpu.minHBindGroup || !gpu.minVPipeline || !gpu.minVBindGroup || !gpu.dehazeMeanguideHPipeline || !gpu.dehazeMeanguideHBindGroup || !gpu.dehazeMeanguideVPipeline || !gpu.dehazeMeanguideVBindGroup || !gpu.dehazeMeanpHPipeline || !gpu.dehazeMeanpHBindGroup || !gpu.dehazeMeanpVPipeline || !gpu.dehazeMeanpVBindGroup || !gpu.dehazeCorrguideHPipeline || !gpu.dehazeCorrguideHBindGroup || !gpu.dehazeCorrguideVPipeline || !gpu.dehazeCorrguideVBindGroup || !gpu.dehazeCorrguidepHPipeline || !gpu.dehazeCorrguidepHBindGroup || !gpu.dehazeCorrguidepVPipeline || !gpu.dehazeCorrguidepVBindGroup || !gpu.dehazeAPipeline || !gpu.dehazeABindGroup || !gpu.dehazeBPipeline || !gpu.dehazeBBindGroup || !gpu.dehazeMeanaHPipeline || !gpu.dehazeMeanaHBindGroup || !gpu.dehazeMeanaVPipeline || !gpu.dehazeMeanaVBindGroup || !gpu.dehazeMeanbHPipeline || !gpu.dehazeMeanbHBindGroup || !gpu.dehazeMeanbVPipeline || !gpu.dehazeMeanbVBindGroup || !gpu.dehazeRefinePipeline || !gpu.dehazeRefineBindGroup || !gpu.textureHPipeline || !gpu.textureHBindGroup || !gpu.textureVPipeline || !gpu.textureVBindGroup || !gpu.clarityMeanpHPipeline || !gpu.clarityMeanpHBindGroup || !gpu.clarityMeanpVPipeline || !gpu.clarityMeanpVBindGroup || !gpu.clarityCorrpHPipeline || !gpu.clarityCorrpHBindGroup || !gpu.clarityCorrpVPipeline || !gpu.clarityCorrpVBindGroup || !gpu.clarityAPipeline || !gpu.clarityABindGroup || !gpu.clarityBPipeline || !gpu.clarityBBindGroup || !gpu.clarityMeanaHPipeline || !gpu.clarityMeanaHBindGroup || !gpu.clarityMeanaVPipeline || !gpu.clarityMeanaVBindGroup || !gpu.clarityMeanbHPipeline || !gpu.clarityMeanbHBindGroup || !gpu.clarityMeanbVPipeline || !gpu.clarityMeanbVBindGroup || !gpu.clarityVPipeline || !gpu.clarityVBindGroup || !gpu.sharpenHPipeline || !gpu.sharpenHBindGroup || !gpu.sharpenVPipeline || !gpu.sharpenVBindGroup || !gpu.lumaNRHPipeline || !gpu.lumaNRHBindGroup || !gpu.lumaNRVPipeline || !gpu.lumaNRVBindGroup || !gpu.colorNRHPipeline || !gpu.colorNRHBindGroup || !gpu.colorNRVPipeline || !gpu.colorNRVBindGroup || !gpu.gradedTex || !gpu.minChannelTex || !gpu.darkChannelHTex || !gpu.tRawTex || !gpu.transmissionHTex || !gpu.transmissionTex || !gpu.dehazeMeanGuideTex || !gpu.dehazeMeanPTex || !gpu.dehazeCorrGuideTex || !gpu.dehazeCorrGuidePTex || !gpu.dehazeATex || !gpu.dehazeBTex || !gpu.dehazeMeanATex || !gpu.dehazeMeanBTex || !gpu.textureBlurScratchTex || !gpu.textureAdjustedTex || !gpu.clarityBlurScratchTex || !gpu.clarityMeanPTex || !gpu.clarityCorrPTex || !gpu.clarityATex || !gpu.clarityBTex || !gpu.clarityMeanATex || !gpu.clarityMeanBTex || !gpu.sharpenBlurHTex || !gpu.sharpenBlurTex || !gpu.lumaNRBlurHTex || !gpu.lumaNRBlurTex || !gpu.colorNRBlurHTex || !gpu.colorNRBlurTex || gpu.atmLightChain.length === 0 || !gpu.uniformBuffer || !gpu.masksBuffer || !gpu.curveLutBuffer || !gpu.hslBandsBuffer || !gpu.splitToningBuffer || !gpu.vignetteBuffer || !gpu.lensCorrectionBuffer || !gpu.perspectiveBuffer || !gpu.grainBuffer || !gpu.sharpenBuffer || !gpu.lumaNRBuffer || !gpu.colorNRBuffer || !gpu.clippingBuffer) return;

  // M4 Slice 2: before/after preview -- skips the ENTIRE global-grade +
  // local-mask pipeline below (not just the mask loop) and draws the raw
  // decoded source straight to the swapchain via fs_original, matching
  // real Lightroom's own \ behavior (show the photo exactly as it was
  // before any edits, not just "with local adjustments hidden"). Returns
  // before touching histogram/mask-rasterization state so toggling back
  // off simply re-renders the live graded result on the very next call,
  // no state to reconcile.
  if (inputs.showOriginal) {
    if (!gpu.originalPipeline || !gpu.originalBindGroup) return;
    const encoder = gpu.device.createCommandEncoder();
    runFullscreenPass(encoder, gpu.originalPipeline, gpu.originalBindGroup, gpu.context.getCurrentTexture().createView());
    gpu.device.queue.submit([encoder.finish()]);
    return;
  }

  syncMaskRasterization(gpu, inputs);

  // Tone curve: rebuilt from the current control points and rewritten
  // every render, same "cheap enough to just always redo" treatment as
  // the uniform/mask buffers below -- no dirty-tracking needed given
  // buildToneCurveLut's own cost (a handful of points, 256 samples).
  gpu.device.queue.writeBuffer(gpu.curveLutBuffer, 0, buildToneCurveLut(inputs.toneCurvePoints));
  gpu.device.queue.writeBuffer(gpu.hslBandsBuffer, 0, buildHslUniformData(inputs.hslBands));
  gpu.device.queue.writeBuffer(gpu.splitToningBuffer, 0, buildSplitToningUniformData(inputs.splitToning));
  gpu.device.queue.writeBuffer(gpu.lensCorrectionBuffer, 0, buildLensCorrectionUniformData(inputs.lensCorrection));
  gpu.device.queue.writeBuffer(gpu.perspectiveBuffer, 0, buildPerspectiveUniformData(inputs.perspective));
  gpu.device.queue.writeBuffer(gpu.vignetteBuffer, 0, buildVignetteUniformData(inputs.vignette));
  gpu.device.queue.writeBuffer(gpu.grainBuffer, 0, buildGrainUniformData(inputs.grain));
  gpu.device.queue.writeBuffer(gpu.sharpenBuffer, 0, buildSharpenUniformData(inputs.sharpen));
  gpu.device.queue.writeBuffer(gpu.lumaNRBuffer, 0, buildLumaNrUniformData(inputs.lumaNR));
  gpu.device.queue.writeBuffer(gpu.colorNRBuffer, 0, buildColorNrUniformData(inputs.colorNR));
  gpu.device.queue.writeBuffer(gpu.clippingBuffer, 0, new Float32Array([inputs.showClippingOverlay ? 1 : 0, 0, 0, 0]));

  // Mask overlay: -1 (disabled) unless the toggle is on AND the current
  // selection exists in `masks` -- `findIndex`'s own -1 miss-sentinel
  // *is* the disabled state, so no separate per-kind lookup is needed
  // here at all (the shader itself gates which kinds actually show the
  // overlay; see the kind > 1.5 check in the mask loop below).
  const selectedMaskIndex = inputs.showMaskOverlay ? inputs.masks.findIndex((m) => m.id === inputs.selectedMaskId) : -1;

  gpu.device.queue.writeBuffer(
    gpu.uniformBuffer,
    0,
    new Float32Array([
      inputs.exposure,
      inputs.contrast,
      inputs.saturation,
      inputs.masks.length,
      selectedMaskIndex,
      inputs.dehaze,
      inputs.texture,
      inputs.clarity,
      inputs.temperature,
      inputs.tint,
      inputs.highlights,
      inputs.shadows,
      inputs.whites,
      inputs.blacks,
      0,
      0,
    ]),
  );

  const maskData = new Float32Array(MAX_MASKS * 12);
  inputs.masks.slice(0, MAX_MASKS).forEach((/** @type {any} */ m, /** @type {number} */ i) => {
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
      maskData[o + 7] = gpu.brushRasterState.get(m.id)?.layer ?? 0;
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
      maskData[o + 7] = gpu.brushRasterState.get(m.id)?.layer ?? 0;
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
  gpu.device.queue.writeBuffer(gpu.masksBuffer, 0, maskData);

  const encoder = gpu.device.createCommandEncoder();

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
    exposure: inputs.exposure, contrast: inputs.contrast, saturation: inputs.saturation, temperature: inputs.temperature, tint: inputs.tint, highlights: inputs.highlights, shadows: inputs.shadows, whites: inputs.whites, blacks: inputs.blacks,
    toneCurvePoints: inputs.toneCurvePoints, hslBands: inputs.hslBands, splitToning: inputs.splitToning, texture: inputs.texture, clarity: inputs.clarity,
    sharpenRadius: inputs.sharpen.radius, lensCorrection: inputs.lensCorrection, perspective: inputs.perspective,
  });
  if (spatialOpsKey !== gpu.spatialOpsInputsKey) {
    gpu.spatialOpsInputsKey = spatialOpsKey;
    runFullscreenPass(encoder, gpu.lensCorrectPipeline, gpu.lensCorrectBindGroup, gpu.lensCorrectedTex.createView());
    runFullscreenPass(encoder, gpu.perspectivePipeline, gpu.perspectiveBindGroup, /** @type {GPUTexture} */ (gpu.perspectiveCorrectedTex).createView());
    runFullscreenPass(encoder, gpu.gradePipeline, gpu.gradeBindGroup, gpu.gradedTex.createView());
    runFullscreenPass(encoder, gpu.textureHPipeline, gpu.textureHBindGroup, gpu.textureBlurScratchTex.createView());
    runFullscreenPass(encoder, gpu.textureVPipeline, gpu.textureVBindGroup, gpu.textureAdjustedTex.createView());
    // Clarity's guided filter (RFC-0010): 10 passes replacing the old
    // clarityHPipeline/clarityVPipeline pair -- mean_p, corr_p, a, b,
    // mean_a, mean_b, each in the order its own inputs become available
    // (see dehazeLocalContrast.js's own doc comment for the full
    // derivation this pass list implements). clarityBlurScratchTex is
    // reused as the shared H-scratch for all four box-filter pairs here
    // (mean_p, corr_p, mean_a, mean_b), the same rebinding trick
    // Texture's own H/V pair and every Dehaze H/V pair already use --
    // each H-pass's output is fully consumed by its own V-pass before the
    // next H-pass overwrites it, so strict sequential order (guaranteed
    // within one command encoder) is all the safety this needs.
    runFullscreenPass(encoder, gpu.clarityMeanpHPipeline, gpu.clarityMeanpHBindGroup, gpu.clarityBlurScratchTex.createView());
    runFullscreenPass(encoder, gpu.clarityMeanpVPipeline, gpu.clarityMeanpVBindGroup, gpu.clarityMeanPTex.createView());
    runFullscreenPass(encoder, gpu.clarityCorrpHPipeline, gpu.clarityCorrpHBindGroup, gpu.clarityBlurScratchTex.createView());
    runFullscreenPass(encoder, gpu.clarityCorrpVPipeline, gpu.clarityCorrpVBindGroup, gpu.clarityCorrPTex.createView());
    runFullscreenPass(encoder, gpu.clarityAPipeline, gpu.clarityABindGroup, gpu.clarityATex.createView());
    runFullscreenPass(encoder, gpu.clarityBPipeline, gpu.clarityBBindGroup, gpu.clarityBTex.createView());
    runFullscreenPass(encoder, gpu.clarityMeanaHPipeline, gpu.clarityMeanaHBindGroup, gpu.clarityBlurScratchTex.createView());
    runFullscreenPass(encoder, gpu.clarityMeanaVPipeline, gpu.clarityMeanaVBindGroup, gpu.clarityMeanATex.createView());
    runFullscreenPass(encoder, gpu.clarityMeanbHPipeline, gpu.clarityMeanbHBindGroup, gpu.clarityBlurScratchTex.createView());
    runFullscreenPass(encoder, gpu.clarityMeanbVPipeline, gpu.clarityMeanbVBindGroup, gpu.clarityMeanBTex.createView());
    // Overwrites gradedTex in place -- see fs_clarity_v's own doc
    // comment for why this is sound (sequential pass execution within
    // one command encoder) and why no third "final graded" texture is
    // needed.
    runFullscreenPass(encoder, gpu.clarityVPipeline, gpu.clarityVBindGroup, gpu.gradedTex.createView());
    // Sharpening / Noise Reduction: all three read gradedTex in this
    // SAME post-Texture/Clarity, pre-Dehaze-recovery state -- see
    // develop_engine.rs's own doc comment on the blur-source
    // precomputation for the named, accepted limitation this implies.
    // Order among these three (and relative to the atm-reduce chain
    // below) doesn't matter -- all read the same stable gradedTex
    // snapshot with no interdependency between them.
    runFullscreenPass(encoder, gpu.sharpenHPipeline, gpu.sharpenHBindGroup, gpu.sharpenBlurHTex.createView());
    runFullscreenPass(encoder, gpu.sharpenVPipeline, gpu.sharpenVBindGroup, gpu.sharpenBlurTex.createView());
    runFullscreenPass(encoder, gpu.lumaNRHPipeline, gpu.lumaNRHBindGroup, gpu.lumaNRBlurHTex.createView());
    runFullscreenPass(encoder, gpu.lumaNRVPipeline, gpu.lumaNRVBindGroup, gpu.lumaNRBlurTex.createView());
    runFullscreenPass(encoder, gpu.colorNRHPipeline, gpu.colorNRHBindGroup, gpu.colorNRBlurHTex.createView());
    runFullscreenPass(encoder, gpu.colorNRVPipeline, gpu.colorNRVBindGroup, gpu.colorNRBlurTex.createView());
    // Captured as a local `const` for the same reason applyBitmapToGpu's
    // own gpuDevice/reducePipeline aliases are -- TS can't narrow a
    // reassignable outer `let` across a closure boundary.
    const reducePipeline = gpu.atmReducePipeline;
    gpu.atmReduceBindGroups.forEach((bg, i) => {
      runFullscreenPass(encoder, reducePipeline, bg, gpu.atmLightChain[i].createView());
    });
    runFullscreenPass(encoder, gpu.minChannelPipeline, gpu.minChannelBindGroup, gpu.minChannelTex.createView());
    runFullscreenPass(encoder, gpu.minHPipeline, gpu.minHBindGroup, gpu.darkChannelHTex.createView());
    runFullscreenPass(encoder, gpu.minVPipeline, gpu.minVBindGroup, gpu.tRawTex.createView());
    // Transmission refinement's guided filter (RFC-0011): 15 passes
    // replacing the old meanHPipeline/meanVPipeline pair -- mean_guide,
    // mean_p, corr_guide, corr_guide_p, a, b, mean_a, mean_b, then the
    // final compose, in that order (see dehazeLocalContrast.js's own doc
    // comment for the full derivation). transmissionHTex is reused as the
    // shared H-scratch for all four box-filter pairs here, same rebinding
    // discipline as every other box-filter pair in this file.
    runFullscreenPass(encoder, gpu.dehazeMeanguideHPipeline, gpu.dehazeMeanguideHBindGroup, gpu.transmissionHTex.createView());
    runFullscreenPass(encoder, gpu.dehazeMeanguideVPipeline, gpu.dehazeMeanguideVBindGroup, gpu.dehazeMeanGuideTex.createView());
    runFullscreenPass(encoder, gpu.dehazeMeanpHPipeline, gpu.dehazeMeanpHBindGroup, gpu.transmissionHTex.createView());
    runFullscreenPass(encoder, gpu.dehazeMeanpVPipeline, gpu.dehazeMeanpVBindGroup, gpu.dehazeMeanPTex.createView());
    runFullscreenPass(encoder, gpu.dehazeCorrguideHPipeline, gpu.dehazeCorrguideHBindGroup, gpu.transmissionHTex.createView());
    runFullscreenPass(encoder, gpu.dehazeCorrguideVPipeline, gpu.dehazeCorrguideVBindGroup, gpu.dehazeCorrGuideTex.createView());
    runFullscreenPass(encoder, gpu.dehazeCorrguidepHPipeline, gpu.dehazeCorrguidepHBindGroup, gpu.transmissionHTex.createView());
    runFullscreenPass(encoder, gpu.dehazeCorrguidepVPipeline, gpu.dehazeCorrguidepVBindGroup, gpu.dehazeCorrGuidePTex.createView());
    runFullscreenPass(encoder, gpu.dehazeAPipeline, gpu.dehazeABindGroup, gpu.dehazeATex.createView());
    runFullscreenPass(encoder, gpu.dehazeBPipeline, gpu.dehazeBBindGroup, gpu.dehazeBTex.createView());
    runFullscreenPass(encoder, gpu.dehazeMeanaHPipeline, gpu.dehazeMeanaHBindGroup, gpu.transmissionHTex.createView());
    runFullscreenPass(encoder, gpu.dehazeMeanaVPipeline, gpu.dehazeMeanaVBindGroup, gpu.dehazeMeanATex.createView());
    runFullscreenPass(encoder, gpu.dehazeMeanbHPipeline, gpu.dehazeMeanbHBindGroup, gpu.transmissionHTex.createView());
    runFullscreenPass(encoder, gpu.dehazeMeanbVPipeline, gpu.dehazeMeanbVBindGroup, gpu.dehazeMeanBTex.createView());
    runFullscreenPass(encoder, gpu.dehazeRefinePipeline, gpu.dehazeRefineBindGroup, gpu.transmissionTex.createView());
  }

  // Pre-mask pass (M4 Slice 1): unconditional every render, same as the
  // old single-pass fs_final always was -- Dehaze amount/Vignette/Grain/
  // NR are all cheap per-pixel blends read fresh from their own uniform
  // buffers every frame (unlike the expensive spatialOpsKey-gated block
  // above), so this can't be folded into that gate without breaking
  // live response to those sliders. Must run BEFORE fs_mask below --
  // preMaskTex is fs_mask's own input, see that pass's own doc comment.
  runFullscreenPass(encoder, gpu.preMaskPipeline, gpu.preMaskBindGroup, gpu.preMaskTex.createView());

  runFullscreenPass(encoder, gpu.pipeline, gpu.bindGroup, gpu.context.getCurrentTexture().createView());

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
  if (gpu.histogramTex && gpu.histogramReadbackBuffer && !gpu.histogramReadInFlight) {
    runFullscreenPass(encoder, gpu.pipeline, gpu.bindGroup, gpu.histogramTex.createView());
    encoder.copyTextureToBuffer(
      { texture: gpu.histogramTex },
      { buffer: gpu.histogramReadbackBuffer, bytesPerRow: HISTOGRAM_SIZE * 4 },
      { width: HISTOGRAM_SIZE, height: HISTOGRAM_SIZE },
    );
  }

  gpu.device.queue.submit([encoder.finish()]);
  readHistogramIfIdle(gpu, inputs);
}
