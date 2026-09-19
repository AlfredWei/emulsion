// The main fs_grade fragment entry point.
// Part of the WGSL source, split out of DevelopCanvas.svelte; see index.js for the
// concatenation order, which must not change.
export const grade = `    // Grade pass (M3 Dehaze): the existing global chain (exposure ->
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
      rgb = apply_global_adjustments(
        rgb,
        adj.exposure_ev,
        adj.contrast,
        adj.saturation,
        adj.temperature,
        adj.tint,
        adj.highlights,
        adj.shadows,
        adj.whites,
        adj.blacks
      );
      rgb = vec3<f32>(sampleCurveLut(rgb.x), sampleCurveLut(rgb.y), sampleCurveLut(rgb.z));
      rgb = applyHslBands(rgb);
      rgb = applySplitToning(rgb);
      return vec4<f32>(rgb, 1.0);
    }

`;
