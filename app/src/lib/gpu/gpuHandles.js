// The Develop canvas's WebGPU handle bag: every device, pipeline, texture, buffer and bind group
// it creates, on one plain (non-reactive) object. Moved out of DevelopCanvas.svelte, where these
// were ~90 separate module-level variables. The component owns one `gpu` from `createGpuHandles()`
// and hands it to the functions in pipelines.js, sourceTexture.js and renderFrame.js.

export const MAX_MASKS = 8;

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
export const HISTOGRAM_SIZE = 256;

/**
 * @typedef {Object} GpuHandles
 * @property {GPUDevice | null} device
 * @property {GPUCanvasContext | null} context
 * @property {GPURenderPipeline | null} pipeline
 * @property {GPURenderPipeline | null} originalPipeline
 * @property {GPUBindGroup | null} originalBindGroup
 * @property {GPUTexture | null} sourceTexture
 * @property {GPUBuffer | null} uniformBuffer
 * @property {GPUBuffer | null} masksBuffer
 * @property {GPUBuffer | null} curveLutBuffer
 * @property {GPUBuffer | null} hslBandsBuffer
 * @property {GPUBuffer | null} splitToningBuffer
 * @property {GPUBuffer | null} vignetteBuffer
 * @property {GPUBuffer | null} lensCorrectionBuffer
 * @property {GPUBuffer | null} perspectiveBuffer
 * @property {GPUBuffer | null} grainBuffer
 * @property {GPUBuffer | null} sharpenBuffer
 * @property {GPUBuffer | null} lumaNRBuffer
 * @property {GPUBuffer | null} colorNRBuffer
 * @property {GPUBindGroup | null} bindGroup
 * @property {GPURenderPipeline | null} preMaskPipeline
 * @property {GPUBindGroup | null} preMaskBindGroup
 * @property {GPUTexture | null} preMaskTex
 * @property {GPUTextureFormat} presentationFormat
 * @property {GPUTexture | null} histogramTex
 * @property {GPUBuffer | null} histogramReadbackBuffer
 * @property {boolean} histogramReadInFlight
 * @property {Uint8Array | null} lastHistogramPixels
 * @property {GPUBuffer | null} clippingBuffer
 * @property {GPURenderPipeline | null} lensCorrectPipeline
 * @property {GPURenderPipeline | null} perspectivePipeline
 * @property {GPURenderPipeline | null} gradePipeline
 * @property {GPURenderPipeline | null} atmReducePipeline
 * @property {GPURenderPipeline | null} minChannelPipeline
 * @property {GPURenderPipeline | null} minHPipeline
 * @property {GPURenderPipeline | null} minVPipeline
 * @property {GPURenderPipeline | null} dehazeMeanguideHPipeline
 * @property {GPURenderPipeline | null} dehazeMeanguideVPipeline
 * @property {GPURenderPipeline | null} dehazeMeanpHPipeline
 * @property {GPURenderPipeline | null} dehazeMeanpVPipeline
 * @property {GPURenderPipeline | null} dehazeCorrguideHPipeline
 * @property {GPURenderPipeline | null} dehazeCorrguideVPipeline
 * @property {GPURenderPipeline | null} dehazeCorrguidepHPipeline
 * @property {GPURenderPipeline | null} dehazeCorrguidepVPipeline
 * @property {GPURenderPipeline | null} dehazeAPipeline
 * @property {GPURenderPipeline | null} dehazeBPipeline
 * @property {GPURenderPipeline | null} dehazeMeanaHPipeline
 * @property {GPURenderPipeline | null} dehazeMeanaVPipeline
 * @property {GPURenderPipeline | null} dehazeMeanbHPipeline
 * @property {GPURenderPipeline | null} dehazeMeanbVPipeline
 * @property {GPURenderPipeline | null} dehazeRefinePipeline
 * @property {GPURenderPipeline | null} textureHPipeline
 * @property {GPURenderPipeline | null} textureVPipeline
 * @property {GPURenderPipeline | null} clarityMeanpHPipeline
 * @property {GPURenderPipeline | null} clarityMeanpVPipeline
 * @property {GPURenderPipeline | null} clarityCorrpHPipeline
 * @property {GPURenderPipeline | null} clarityCorrpVPipeline
 * @property {GPURenderPipeline | null} clarityAPipeline
 * @property {GPURenderPipeline | null} clarityBPipeline
 * @property {GPURenderPipeline | null} clarityMeanaHPipeline
 * @property {GPURenderPipeline | null} clarityMeanaVPipeline
 * @property {GPURenderPipeline | null} clarityMeanbHPipeline
 * @property {GPURenderPipeline | null} clarityMeanbVPipeline
 * @property {GPURenderPipeline | null} clarityVPipeline
 * @property {GPURenderPipeline | null} sharpenHPipeline
 * @property {GPURenderPipeline | null} sharpenVPipeline
 * @property {GPURenderPipeline | null} lumaNRHPipeline
 * @property {GPURenderPipeline | null} lumaNRVPipeline
 * @property {GPURenderPipeline | null} colorNRHPipeline
 * @property {GPURenderPipeline | null} colorNRVPipeline
 * @property {GPUTexture | null} lensCorrectedTex
 * @property {GPUTexture | null} perspectiveCorrectedTex
 * @property {GPUTexture | null} gradedTex
 * @property {GPUTexture | null} minChannelTex
 * @property {GPUTexture | null} darkChannelHTex
 * @property {GPUTexture | null} tRawTex
 * @property {GPUTexture | null} transmissionHTex
 * @property {GPUTexture | null} transmissionTex
 * @property {GPUTexture | null} dehazeMeanGuideTex
 * @property {GPUTexture | null} dehazeMeanPTex
 * @property {GPUTexture | null} dehazeCorrGuideTex
 * @property {GPUTexture | null} dehazeCorrGuidePTex
 * @property {GPUTexture | null} dehazeATex
 * @property {GPUTexture | null} dehazeBTex
 * @property {GPUTexture | null} dehazeMeanATex
 * @property {GPUTexture | null} dehazeMeanBTex
 * @property {GPUTexture[]} atmLightChain
 * @property {GPUTexture | null} textureBlurScratchTex
 * @property {GPUTexture | null} textureAdjustedTex
 * @property {GPUTexture | null} clarityBlurScratchTex
 * @property {GPUTexture | null} clarityMeanPTex
 * @property {GPUTexture | null} clarityCorrPTex
 * @property {GPUTexture | null} clarityATex
 * @property {GPUTexture | null} clarityBTex
 * @property {GPUTexture | null} clarityMeanATex
 * @property {GPUTexture | null} clarityMeanBTex
 * @property {GPUTexture | null} sharpenBlurHTex
 * @property {GPUTexture | null} sharpenBlurTex
 * @property {GPUTexture | null} lumaNRBlurHTex
 * @property {GPUTexture | null} lumaNRBlurTex
 * @property {GPUTexture | null} colorNRBlurHTex
 * @property {GPUTexture | null} colorNRBlurTex
 * @property {GPUBindGroup | null} lensCorrectBindGroup
 * @property {GPUBindGroup | null} perspectiveBindGroup
 * @property {GPUBindGroup | null} gradeBindGroup
 * @property {GPUBindGroup | null} minChannelBindGroup
 * @property {GPUBindGroup | null} minHBindGroup
 * @property {GPUBindGroup | null} minVBindGroup
 * @property {GPUBindGroup | null} dehazeMeanguideHBindGroup
 * @property {GPUBindGroup | null} dehazeMeanguideVBindGroup
 * @property {GPUBindGroup | null} dehazeMeanpHBindGroup
 * @property {GPUBindGroup | null} dehazeMeanpVBindGroup
 * @property {GPUBindGroup | null} dehazeCorrguideHBindGroup
 * @property {GPUBindGroup | null} dehazeCorrguideVBindGroup
 * @property {GPUBindGroup | null} dehazeCorrguidepHBindGroup
 * @property {GPUBindGroup | null} dehazeCorrguidepVBindGroup
 * @property {GPUBindGroup | null} dehazeABindGroup
 * @property {GPUBindGroup | null} dehazeBBindGroup
 * @property {GPUBindGroup | null} dehazeMeanaHBindGroup
 * @property {GPUBindGroup | null} dehazeMeanaVBindGroup
 * @property {GPUBindGroup | null} dehazeMeanbHBindGroup
 * @property {GPUBindGroup | null} dehazeMeanbVBindGroup
 * @property {GPUBindGroup | null} dehazeRefineBindGroup
 * @property {GPUBindGroup | null} textureHBindGroup
 * @property {GPUBindGroup | null} textureVBindGroup
 * @property {GPUBindGroup | null} clarityMeanpHBindGroup
 * @property {GPUBindGroup | null} clarityMeanpVBindGroup
 * @property {GPUBindGroup | null} clarityCorrpHBindGroup
 * @property {GPUBindGroup | null} clarityCorrpVBindGroup
 * @property {GPUBindGroup | null} clarityABindGroup
 * @property {GPUBindGroup | null} clarityBBindGroup
 * @property {GPUBindGroup | null} clarityMeanaHBindGroup
 * @property {GPUBindGroup | null} clarityMeanaVBindGroup
 * @property {GPUBindGroup | null} clarityMeanbHBindGroup
 * @property {GPUBindGroup | null} clarityMeanbVBindGroup
 * @property {GPUBindGroup | null} clarityVBindGroup
 * @property {GPUBindGroup | null} sharpenHBindGroup
 * @property {GPUBindGroup | null} sharpenVBindGroup
 * @property {GPUBindGroup | null} lumaNRHBindGroup
 * @property {GPUBindGroup | null} lumaNRVBindGroup
 * @property {GPUBindGroup | null} colorNRHBindGroup
 * @property {GPUBindGroup | null} colorNRVBindGroup
 * @property {GPUBindGroup[]} atmReduceBindGroups
 * @property {string | null} spatialOpsInputsKey
 * @property {GPUTexture | null} brushTextureArray
 * @property {Map<any, any>} brushRasterState
 * @property {number[]} freeBrushLayers
 * @property {OffscreenCanvas | null} sourceSampleCanvas
 * @property {OffscreenCanvasRenderingContext2D | null} sourceSampleCtx
 */

/**
 * The component props the renderer reads, exposed through getters (see DevelopCanvas.svelte's
 * `renderInputs`). Member types are copied from the component's own props type.
 * @typedef {{
   *   onHistogramUpdate?: (data: {r: Uint32Array, g: Uint32Array, b: Uint32Array}) => void,
   *   masks: import('$lib/api/develop.js').Mask[],
   *   showOriginal: boolean,
   *   toneCurvePoints: readonly {x: number, y: number}[],
   *   hslBands: Readonly<Record<string, {hue: number, saturation: number, luminance: number}>>,
   *   splitToning: {shadows: {hue: number, saturation: number}, highlights: {hue: number, saturation: number}, balance: number},
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
   *   vignette: {amount: number, midpoint: number, feather: number},
   *   grain: {amount: number, size: number, roughness: number},
   *   sharpen: {amount: number, radius: number, detail: number, masking: number},
   *   lumaNR: {amount: number, detail: number, contrast: number},
   *   colorNR: {amount: number, detail: number},
   *   showClippingOverlay: boolean,
   *   showMaskOverlay: boolean,
   *   selectedMaskId: string | null,
   *   exposure: number,
   *   contrast: number,
   *   saturation: number,
   *   dehaze: number,
   *   texture: number,
   *   clarity: number,
   *   temperature: number,
   *   tint: number,
   *   highlights: number,
   *   shadows: number,
   *   whites: number,
   *   blacks: number,
 * }} RenderInputs
 */

/**
 * The component-state writes the GPU code makes.
 * @typedef {{
 *   onError: (message: string) => void,
 *   onSourceDimensions: (width: number, height: number) => void,
 * }} GpuHooks
 */

/** @returns {GpuHandles} */
export function createGpuHandles() {
  return {
    // WebGPU handles -- plain vars, not $state: these drive imperative canvas
    // rendering, not Svelte's own reactivity (RFC-0001 §4 "decode once, edit
    // reactively": the texture is uploaded once per image, every subsequent
    // adjustment just rewrites a uniform buffer and re-runs the shader, no
    // re-fetch and no Svelte re-render of the DOM).
    device: null,
    context: null,
    pipeline: null,
    /** M4 Slice 2: before/after preview's own dedicated pass (fs_original) --
     * see that WGSL function's own doc comment for why this is a separate
     * pipeline rather than a branch inside `pipeline` (fs_mask). */
    originalPipeline: null,
    originalBindGroup: null,
    sourceTexture: null,
    uniformBuffer: null,
    masksBuffer: null,
    // Tone Curve (M3): device-scoped like uniformBuffer/masksBuffer above
    // (created once in initGpu, rewritten via writeBuffer whenever the curve
    // changes) -- NOT recreated per image/tier-swap the way sourceTexture/
    // brushTextureArray are, since a curve's shape has nothing to do with
    // which image is loaded.
    curveLutBuffer: null,
    // HSL / Color Mixer (M3): same device-scoped treatment as curveLutBuffer
    // above -- created once, rewritten via writeBuffer on every render, not
    // tied to which image is loaded.
    hslBandsBuffer: null,
    // Split Toning (M3): same device-scoped treatment as curveLutBuffer/
    // hslBandsBuffer above.
    splitToningBuffer: null,
    // Vignette (M3): same device-scoped treatment -- 3 fields don't fit in
    // Adjustments' own spare padding (already claimed by Dehaze/Texture/
    // Clarity), so it gets its own small dedicated buffer, same as Split
    // Toning did for the same reason.
    vignetteBuffer: null,
    // Lens Corrections (M3): a larger flat struct (24 f32s, see the WGSL
    // `LensCorrectionParams` doc comment) than Vignette/Grain's own -- still
    // device-scoped and rewritten every render, same as those.
    lensCorrectionBuffer: null,
    // Perspective Correction (M4): same device-scoped, own-small-buffer
    // treatment as Vignette/Grain above.
    perspectiveBuffer: null,
    // Grain (M3): same device-scoped, own-small-buffer treatment as
    // Vignette above, for the same reason (3 fields, no spare Adjustments
    // padding left).
    grainBuffer: null,
    // Sharpening / Noise Reduction (M3): same device-scoped, own-small-
    // buffer treatment as Vignette/Grain above, one buffer per structured
    // op.
    sharpenBuffer: null,
    lumaNRBuffer: null,
    colorNRBuffer: null,
    bindGroup: null,
    // M4 Slice 1 (Healing/Clone brush): the fs_premask pass's own
    // pipeline/bind group, plus preMaskTex itself -- see preMaskTex's WGSL-
    // side doc comment for the full split reasoning. preMaskTex is
    // per-image (recreated alongside gradedTex, same size), the pipeline/
    // bind-group-SHAPE is device-scoped (created once in initGpu, like
    // `pipeline` itself), but preMaskBindGroup still needs recreating per
    // image since it references preMaskTex's own view target indirectly
    // via the textures it reads (gradedTex etc, same lifecycle as
    // `bindGroup` above).
    preMaskPipeline: null,
    preMaskBindGroup: null,
    preMaskTex: null,
    presentationFormat: "bgra8unorm",
    histogramTex: null,
    histogramReadbackBuffer: null,
    histogramReadInFlight: false,
    // The most recent histogramTex readback's raw bytes -- kept around
    // (not just its binned form) so hover-RGB lookups (see
    // reportHoverPixel) can index directly into it without a second GPU
    // round-trip. An approximate (256x256, not full-resolution) but
    // genuinely GRADED sample -- unlike sampleSourcePixel's own SOURCE-only
    // sampling, see that function's doc comment for why a true graded
    // readback was previously deferred; this reuses the exact same texture
    // the histogram itself already reads back every render, so no
    // additional GPU work is needed for this feature at all.
    lastHistogramPixels: null,
    // Clipping-overlay toggle: device-scoped, same tiny-padded-uniform
    // treatment as Vignette/Grain/etc.'s own small buffers.
    clippingBuffer: null,
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
    lensCorrectPipeline: null,
    // Perspective Correction (M4): another new pass, chained right after
    // lens correction and before fs_grade -- reads lensCorrectedTex (same
    // "same slot, different physical texture" technique lens correction's
    // own doc comment above describes) and writes perspectiveCorrectedTex,
    // which gradeBindGroup's binding 1 is then rebound to instead.
    perspectivePipeline: null,
    gradePipeline: null,
    atmReducePipeline: null,
    minChannelPipeline: null,
    minHPipeline: null,
    minVPipeline: null,
    dehazeMeanguideHPipeline: null,
    dehazeMeanguideVPipeline: null,
    dehazeMeanpHPipeline: null,
    dehazeMeanpVPipeline: null,
    dehazeCorrguideHPipeline: null,
    dehazeCorrguideVPipeline: null,
    dehazeCorrguidepHPipeline: null,
    dehazeCorrguidepVPipeline: null,
    dehazeAPipeline: null,
    dehazeBPipeline: null,
    dehazeMeanaHPipeline: null,
    dehazeMeanaVPipeline: null,
    dehazeMeanbHPipeline: null,
    dehazeMeanbVPipeline: null,
    dehazeRefinePipeline: null,
    textureHPipeline: null,
    textureVPipeline: null,
    clarityMeanpHPipeline: null,
    clarityMeanpVPipeline: null,
    clarityCorrpHPipeline: null,
    clarityCorrpVPipeline: null,
    clarityAPipeline: null,
    clarityBPipeline: null,
    clarityMeanaHPipeline: null,
    clarityMeanaVPipeline: null,
    clarityMeanbHPipeline: null,
    clarityMeanbVPipeline: null,
    clarityVPipeline: null,
    sharpenHPipeline: null,
    sharpenVPipeline: null,
    lumaNRHPipeline: null,
    lumaNRVPipeline: null,
    colorNRHPipeline: null,
    colorNRVPipeline: null,
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
    lensCorrectedTex: null,
    // Perspective Correction (M4): fs_perspective's own output -- fs_grade
    // reads this instead of lensCorrectedTex directly (see
    // perspectivePipeline's own doc comment above).
    perspectiveCorrectedTex: null,
    gradedTex: null,
    minChannelTex: null,
    darkChannelHTex: null,
    tRawTex: null,
    transmissionHTex: null,
    transmissionTex: null,
    dehazeMeanGuideTex: null,
    dehazeMeanPTex: null,
    dehazeCorrGuideTex: null,
    dehazeCorrGuidePTex: null,
    dehazeATex: null,
    dehazeBTex: null,
    dehazeMeanATex: null,
    dehazeMeanBTex: null,
    atmLightChain: [],
    // Texture & Clarity (M3): local-contrast passes that run BEFORE Dehaze's
    // own maps, writing their final result back into gradedTex itself (see
    // fs_clarity_v's own doc comment) -- these three are the only NEW
    // textures needed. textureBlurScratchTex/clarityBlurScratchTex are each
    // dedicated to one op (not shared) even though nothing stops them from
    // being reused sequentially -- matches every other Dehaze filter stage's
    // own one-texture-per-stage convention, so a future pass reordering
    // can't silently corrupt output with no validation error to catch it.
    textureBlurScratchTex: null,
    textureAdjustedTex: null,
    clarityBlurScratchTex: null,
    clarityMeanPTex: null,
    clarityCorrPTex: null,
    clarityATex: null,
    clarityBTex: null,
    clarityMeanATex: null,
    clarityMeanBTex: null,
    // Sharpening / Noise Reduction (M3): same one-texture-per-stage
    // convention as Texture/Clarity above -- an H-output scratch texture
    // and a final (post-V-pass) result texture per op, all read directly
    // by fs_final (none of these overwrite gradedTex the way Clarity's own
    // V-pass does -- see fs_final's own doc comment for why these stay as
    // separate delta-source textures instead).
    sharpenBlurHTex: null,
    sharpenBlurTex: null,
    lumaNRBlurHTex: null,
    lumaNRBlurTex: null,
    colorNRBlurHTex: null,
    colorNRBlurTex: null,
    lensCorrectBindGroup: null,
    perspectiveBindGroup: null,
    gradeBindGroup: null,
    minChannelBindGroup: null,
    minHBindGroup: null,
    minVBindGroup: null,
    dehazeMeanguideHBindGroup: null,
    dehazeMeanguideVBindGroup: null,
    dehazeMeanpHBindGroup: null,
    dehazeMeanpVBindGroup: null,
    dehazeCorrguideHBindGroup: null,
    dehazeCorrguideVBindGroup: null,
    dehazeCorrguidepHBindGroup: null,
    dehazeCorrguidepVBindGroup: null,
    dehazeABindGroup: null,
    dehazeBBindGroup: null,
    dehazeMeanaHBindGroup: null,
    dehazeMeanaVBindGroup: null,
    dehazeMeanbHBindGroup: null,
    dehazeMeanbVBindGroup: null,
    dehazeRefineBindGroup: null,
    textureHBindGroup: null,
    textureVBindGroup: null,
    clarityMeanpHBindGroup: null,
    clarityMeanpVBindGroup: null,
    clarityCorrpHBindGroup: null,
    clarityCorrpVBindGroup: null,
    clarityABindGroup: null,
    clarityBBindGroup: null,
    clarityMeanaHBindGroup: null,
    clarityMeanaVBindGroup: null,
    clarityMeanbHBindGroup: null,
    clarityMeanbVBindGroup: null,
    clarityVBindGroup: null,
    sharpenHBindGroup: null,
    sharpenVBindGroup: null,
    lumaNRHBindGroup: null,
    lumaNRVBindGroup: null,
    colorNRHBindGroup: null,
    colorNRVBindGroup: null,
    atmReduceBindGroups: [],
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
    spatialOpsInputsKey: null,
    // M3 Slice 7: brush masks rasterize into a shared texture ARRAY (one
    // layer per active brush mask, sized to the same combined MAX_MASKS
    // budget every mask kind shares) rather than a single texture -- a
    // single shared texture would silently break true op-order interleaving
    // and independent per-mask adjustments the moment there's more than one
    // brush mask, or a brush mask sits between two gradients in the stack.
    // Recreated per-image (see loadImage) since it must be sized to that
    // image's native resolution.
    brushTextureArray: null,
    /** Per-mask persistent rasterization state, keyed by mask id. Each
     * OffscreenCanvas is NEVER cleared once created -- only newly-added dabs
     * are drawn onto it (see syncMaskRasterization) -- so a long stroke's
     * per-move cost stays bound by texture resolution/upload cost, not by
     * re-rendering the whole dab list from scratch every time. Reset
     * entirely on every image change (loadImage), since a canvas sized for
     * one image's resolution is meaningless for another.
     * @type {Map<string, { canvas: OffscreenCanvas, ctx: OffscreenCanvasRenderingContext2D, layer: number, dabsDrawn: number, featherDrawn: number, firstDabX: number, firstDabY: number }>} */
    brushRasterState: new Map(),
    freeBrushLayers: [],
    // M3 Slice 8: retained sampleable pixel data, drawn once per image load
    // (see loadImage) into a persistent 2D OffscreenCanvas -- the decoded
    // ImageBitmap itself is discarded right after its one-time
    // copyExternalImageToTexture GPU upload (see loadImage), so nothing
    // else in this component keeps pixel data around for a CPU-side read
    // like an eyedropper needs. Same "own persistent per-image resource,
    // reset in loadImage" pattern brushTextureArray/brushRasterState
    // already use.
    sourceSampleCanvas: null,
    sourceSampleCtx: null,
  };
}
