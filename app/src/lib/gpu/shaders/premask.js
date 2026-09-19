// Mask pre-pass: clipping overlay, heal-ring sampling, and fs_premask.
// Part of the WGSL source, split out of DevelopCanvas.svelte; see index.js for the
// concatenation order, which must not change.
export const premask = `    // Histogram clipping-overlay toggle (own tiny padded-uniform buffer,
    // same treatment as Vignette/Grain above) -- used only by fs_final,
    // see the overlay logic near that function's own return. Declared
    // here, ahead of fs_final's own @fragment attribute, since WGSL
    // attributes bind to the very next declaration -- a struct/binding
    // pair inserted between @fragment and fs_final's own fn declaration
    // would silently detach @fragment from the function it's meant to
    // mark as the pipeline's entry point.
    struct Clipping {
      show_clipping: f32, // 0 or 1
      _pad0: f32,
      _pad1: f32,
      _pad2: f32,
    };
    @group(0) @binding(26) var<uniform> clipping: Clipping;

    // Healing/Clone brush (M4 Slice 1): a spot mask's local_color needs
    // to read some OTHER pixel's fully-graded value (its source point)
    // while computing dest's -- fusing that into fs_final's own single
    // pass would make the result depend on which of source/dest the GPU
    // happens to shade first (undefined for a fragment shader, unlike the
    // Rust CPU path's explicit raster-scan order, but the SAME underlying
    // hazard graded's own doc comment in develop_engine.rs already
    // names for Dehaze). preMaskTex is exactly develop_engine.rs's
    // pre_mask buffer: everything through Grain, written by the new
    // fs_premask pass (the old fs_final body, minus the mask loop),
    // read back here by fs_mask (the new final pass) at both the
    // current pixel AND, for spot masks, an offset pixel.
    @group(0) @binding(27) var preMaskTex: texture_2d<f32>;

    const HEAL_RING_SAMPLES: i32 = 12;
    const HEAL_RING_FACTOR: f32 = 1.15;

    // Average preMaskTex color on a ring just outside radius around
    // center (image-pixel-space center/radius, not normalized UV --
    // callers already have dims/aspect on hand). A cheaper cousin of
    // develop_engine.rs's own sample_ring_mean: 12 samples instead of
    // 24 (interactive-preview budget, not export quality) and no
    // out-of-bounds skip (textureLoad's own coordinate clamp below stands
    // in for it) -- same "not byte-identical, tolerance-tested" parity
    // bar as every other CPU/GPU pair in this file.
    fn healRingMean(centerPx: vec2<f32>, radiusPx: f32, dims: vec2<i32>) -> vec3<f32> {
      var sum = vec3<f32>(0.0, 0.0, 0.0);
      let r = radiusPx * HEAL_RING_FACTOR;
      for (var i = 0; i < HEAL_RING_SAMPLES; i = i + 1) {
        let theta = (f32(i) / f32(HEAL_RING_SAMPLES)) * 6.28318530718;
        let p = centerPx + vec2<f32>(cos(theta), sin(theta)) * r;
        let coord = clamp(vec2<i32>(p), vec2<i32>(0, 0), dims - vec2<i32>(1, 1));
        sum = sum + textureLoad(preMaskTex, coord, 0).rgb;
      }
      return sum / f32(HEAL_RING_SAMPLES);
    }

    // Pre-mask pass (M4 Slice 1: split out of what used to be one single
    // "fs_final" pass -- see preMaskTex's own doc comment for why):
    // reads the graded color back from gradedTex (not a re-sample of
    // srcTexture -- every global op through Texture and Clarity is
    // already baked in, see fs_clarity_v's own doc comment), applies the
    // Dehaze recovery + amount blend, then Vignette (a direct WGSL port
    // of develop_engine.rs's own vignette_factor -- pure per-pixel, no
    // neighboring-pixel data needed, so it folds directly into this pass
    // rather than adding a new buffer/pass the way Dehaze/Texture/Clarity
    // all needed), then Grain, writing into preMaskTex -- NOT the real
    // swapchain output; fs_mask below does that, after the mask loop.
    // Safe to read the dehaze maps unconditionally even at amount=0 (the
    // blend multiplies their contribution by 0) as long as they're always
    // populated with REAL values before this ever runs -- see
    // writeAdjustmentsAndRender's dirty-key caching, which always treats
    // "nothing cached yet" (the very first render) as dirty.
    @fragment
    fn fs_premask(in: VertexOut) -> @location(0) vec4<f32> {
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

      return vec4<f32>(rgb, 1.0);
    }

`;
