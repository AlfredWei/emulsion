// Sharpening and noise-reduction params, bindings, and their separable blur passes.
// Part of the WGSL source, split out of DevelopCanvas.svelte; see index.js for the
// concatenation order, which must not change.
export const detailFilters = `    // Sharpening / Noise Reduction (M3): a direct WGSL port of
    // develop_engine.rs's own sharpen_delta/luma_nr_delta/color_nr_delta
    // -- see that module's doc comments (Sharpen, LumaNr, ColorNr structs
    // and their delta functions) for the full parameter reasoning
    // (Detail vs Masking's genuinely different amplitude-vs-spatial
    // gates, Contrast's post-smoothing restoration, Color NR's exact
    // luma-preserving chroma-delta construction -- algebraically verified
    // in this slice's own design review). All three blurs read gradedTex
    // DIRECTLY (never rebound, unlike Texture/Clarity's lcRgbInput) --
    // they always read the SAME pre-Dehaze-recovery snapshot, see the
    // Rust twin's own doc comment on that named, accepted limitation.
    struct SharpenParams {
      amount: f32,   // 0..100
      radius: f32,   // 0..100 slider, mapped to a pixel radius below
      detail: f32,   // 0..100
      masking: f32,  // 0..100
    };
    struct LumaNrParams {
      amount: f32,    // 0..100
      detail: f32,    // 0..100
      contrast: f32,  // 0..100
      _pad0: f32,
    };
    struct ColorNrParams {
      amount: f32,  // 0..100
      detail: f32,  // 0..100
      _pad0: f32,
      _pad1: f32,
    };
    @group(0) @binding(19) var<uniform> sharpenParams: SharpenParams;
    @group(0) @binding(20) var<uniform> lumaNrParams: LumaNrParams;
    @group(0) @binding(21) var<uniform> colorNrParams: ColorNrParams;
    // Rebound per V-pass, same "generic scratch, many bind groups" pattern
    // filterInput(11)/lcBlurInput(14) already established -- binding 17 is
    // reused across Sharpen's and Luma NR's own H-output (both r32float,
    // single-channel); binding 18 is Color NR's own H-output (rgba16float,
    // a full-channel blur, needs its own dedicated slot).
    @group(0) @binding(17) var blurScratchR32: texture_2d<f32>;
    @group(0) @binding(18) var blurScratchRgba: texture_2d<f32>;
    // Final (post-V-pass) blur results -- read only by fs_final, each its
    // own dedicated binding (not reused/rebound, since fs_final needs all
    // three simultaneously in one draw call, unlike the scratch slots
    // above which are only ever bound one-at-a-time per H/V pass).
    @group(0) @binding(22) var sharpenBlurFinal: texture_2d<f32>;
    @group(0) @binding(23) var lumaNrBlurFinal: texture_2d<f32>;
    @group(0) @binding(24) var colorNrBlurFinal: texture_2d<f32>;
    // RFC-0012: Luma NR's own guided-filter intermediates, same six-binding
    // shape RFC-0010 established for Clarity (bindings 29-34) -- each its
    // own dedicated slot since more than one is read simultaneously by a
    // later pass, unlike blurScratchR32's rebinding-per-pass trick above
    // (still reused as the shared H-scratch for all four of this filter's
    // own box-filter pairs, same convention every guided-filter slice in
    // this codebase already follows).
    @group(0) @binding(43) var lumaNrMeanPFinal: texture_2d<f32>;
    @group(0) @binding(44) var lumaNrCorrPFinal: texture_2d<f32>;
    @group(0) @binding(45) var lumaNrAFinal: texture_2d<f32>;
    @group(0) @binding(46) var lumaNrBFinal: texture_2d<f32>;
    @group(0) @binding(47) var lumaNrMeanAFinal: texture_2d<f32>;
    @group(0) @binding(48) var lumaNrMeanBFinal: texture_2d<f32>;
    // RFC-0014: Color NR's own guided filter -- the GENERAL two-signal
    // case (guide = graded_luma, p = each of R/G/B), not the self-guided
    // shape above, matching Dehaze's own eight-binding pattern (RFC-0011)
    // for the guide-only statistics (49-50, single-channel, shared across
    // all three color channels) but PACKING the per-channel quantities
    // (51-56) as vec3 in one rgba16float texture each -- three independent
    // single-channel passes per quantity would triple the pass count for
    // no reason, since a box filter is per-channel-independent (the same
    // linearity the OLD fs_colorNR_h/fs_colorNR_v pair already relied on).
    @group(0) @binding(49) var colorNrMeanGuideFinal: texture_2d<f32>;
    @group(0) @binding(50) var colorNrCorrGuideFinal: texture_2d<f32>;
    @group(0) @binding(51) var colorNrMeanPFinal: texture_2d<f32>;
    @group(0) @binding(52) var colorNrCorrGuidePFinal: texture_2d<f32>;
    @group(0) @binding(53) var colorNrAFinal: texture_2d<f32>;
    @group(0) @binding(54) var colorNrBFinal: texture_2d<f32>;
    @group(0) @binding(55) var colorNrMeanAFinal: texture_2d<f32>;
    @group(0) @binding(56) var colorNrMeanBFinal: texture_2d<f32>;
    // RFC-0015: Sharpening's own guided filter -- self-guided, same shape
    // as Luma NR's six bindings above (43-48), at 57-62 instead. The real
    // difference from every prior guided-filter slice isn't the binding
    // shape, it's that every H/V pass below needs sharpenParams(19) bound
    // too, to compute its own RUNTIME radius via sharpenRadiusPx -- see
    // that function's own doc comment.
    @group(0) @binding(57) var sharpenMeanPFinal: texture_2d<f32>;
    @group(0) @binding(58) var sharpenCorrPFinal: texture_2d<f32>;
    @group(0) @binding(59) var sharpenAFinal: texture_2d<f32>;
    @group(0) @binding(60) var sharpenBFinal: texture_2d<f32>;
    @group(0) @binding(61) var sharpenMeanAFinal: texture_2d<f32>;
    @group(0) @binding(62) var sharpenMeanBFinal: texture_2d<f32>;

    const SHARPEN_MAX_RADIUS_PX: i32 = 8;
    const SHARPEN_STRENGTH: f32 = 1.6;
    const SHARPEN_DETAIL_SCALE: f32 = 0.06;
    const SHARPEN_MASK_SCALE: f32 = 0.05;
    const LUMA_NR_RADIUS: i32 = 3;
    // RFC-0012: matches the Rust twin's LUMA_NR_GUIDED_EPS exactly -- see
    // that constant's own doc comment in detail.rs for why it's smaller
    // than CLARITY_GUIDED_EPS (this op denoises, it doesn't enhance).
    const LUMA_NR_GUIDED_EPS: f32 = 0.008;
    const NR_DETAIL_SCALE: f32 = 0.6;
    const NR_CONTRAST_STRENGTH: f32 = 0.6;
    const COLOR_NR_RADIUS: i32 = 4;
    const COLOR_NR_DETAIL_SCALE: f32 = 0.5;
    // RFC-0014: matches the Rust twin's COLOR_NR_GUIDED_EPS exactly -- see
    // that constant's own doc comment in detail.rs for why it ended up
    // matching DEHAZE_GUIDED_EPS rather than LUMA_NR_GUIDED_EPS (the RFC's
    // own original guess, corrected during implementation).
    const COLOR_NR_GUIDED_EPS: f32 = 0.0001;
    // RFC-0015: matches the Rust twin's SHARPEN_GUIDED_EPS exactly -- see
    // that constant's own doc comment in detail.rs for why this is the
    // first guided filter whose radius varies at runtime, and why this
    // single eps has to behave reasonably across that whole range.
    const SHARPEN_GUIDED_EPS: f32 = 0.0009;

    // Sharpening's Radius is a genuine USER slider, not a compile-time
    // const the way every other radius in this shader is (Texture/
    // Clarity/Dehaze's own radii are all baked into the WGSL source at
    // authoring time) -- this is a deliberate, new precedent: a real
    // uniform-driven dynamic loop bound in fs_sharpen_h/fs_sharpen_v
    // below, not a template to copy for future fixed-radius ops.
    fn sharpenRadiusPx(radiusSlider: f32) -> i32 {
      let r = 1.0 + (radiusSlider / 100.0) * (f32(SHARPEN_MAX_RADIUS_PX) - 1.0);
      return max(i32(round(r)), 1);
    }

    // Sharpening's guided filter (RFC-0015): self-guided, same shape
    // RFC-0010/0012 already established for Clarity/Luma NR (mean_p,
    // corr_p, a, b, mean_a, mean_b, final) -- the one real difference is
    // that every box-filter pass here needs sharpenParams(19) bound too,
    // to compute ITS OWN runtime radius via sharpenRadiusPx, where
    // Clarity's/Luma NR's own passes each hard-code a fixed radius const.
    @fragment
    fn fs_sharpen_meanp_h(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let dims = vec2<i32>(textureDimensions(gradedTex));
      let radius = sharpenRadiusPx(sharpenParams.radius);
      var sum = 0.0;
      for (var dx = -radius; dx <= radius; dx = dx + 1) {
        let sx = clamp(coord.x + dx, 0, dims.x - 1);
        sum = sum + luma(textureLoad(gradedTex, vec2<i32>(sx, coord.y), 0).rgb);
      }
      let window = f32(2 * radius + 1);
      return vec4<f32>(sum / window, 0.0, 0.0, 1.0);
    }

    @fragment
    fn fs_sharpen_meanp_v(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let dims = vec2<i32>(textureDimensions(blurScratchR32));
      let radius = sharpenRadiusPx(sharpenParams.radius);
      var sum = 0.0;
      for (var dy = -radius; dy <= radius; dy = dy + 1) {
        let sy = clamp(coord.y + dy, 0, dims.y - 1);
        sum = sum + textureLoad(blurScratchR32, vec2<i32>(coord.x, sy), 0).r;
      }
      let window = f32(2 * radius + 1);
      return vec4<f32>(sum / window, 0.0, 0.0, 1.0);
    }

    @fragment
    fn fs_sharpen_corrp_h(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let dims = vec2<i32>(textureDimensions(gradedTex));
      let radius = sharpenRadiusPx(sharpenParams.radius);
      var sum = 0.0;
      for (var dx = -radius; dx <= radius; dx = dx + 1) {
        let sx = clamp(coord.x + dx, 0, dims.x - 1);
        let l = luma(textureLoad(gradedTex, vec2<i32>(sx, coord.y), 0).rgb);
        sum = sum + l * l;
      }
      let window = f32(2 * radius + 1);
      return vec4<f32>(sum / window, 0.0, 0.0, 1.0);
    }

    @fragment
    fn fs_sharpen_corrp_v(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let dims = vec2<i32>(textureDimensions(blurScratchR32));
      let radius = sharpenRadiusPx(sharpenParams.radius);
      var sum = 0.0;
      for (var dy = -radius; dy <= radius; dy = dy + 1) {
        let sy = clamp(coord.y + dy, 0, dims.y - 1);
        sum = sum + textureLoad(blurScratchR32, vec2<i32>(coord.x, sy), 0).r;
      }
      let window = f32(2 * radius + 1);
      return vec4<f32>(sum / window, 0.0, 0.0, 1.0);
    }

    @fragment
    fn fs_sharpen_a(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let meanP = textureLoad(sharpenMeanPFinal, coord, 0).r;
      let corrP = textureLoad(sharpenCorrPFinal, coord, 0).r;
      let varP = corrP - meanP * meanP;
      let a = varP / (varP + SHARPEN_GUIDED_EPS);
      return vec4<f32>(a, 0.0, 0.0, 1.0);
    }

    @fragment
    fn fs_sharpen_b(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let meanP = textureLoad(sharpenMeanPFinal, coord, 0).r;
      let a = textureLoad(sharpenAFinal, coord, 0).r;
      return vec4<f32>(meanP * (1.0 - a), 0.0, 0.0, 1.0);
    }

    @fragment
    fn fs_sharpen_meana_h(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let dims = vec2<i32>(textureDimensions(sharpenAFinal));
      let radius = sharpenRadiusPx(sharpenParams.radius);
      var sum = 0.0;
      for (var dx = -radius; dx <= radius; dx = dx + 1) {
        let sx = clamp(coord.x + dx, 0, dims.x - 1);
        sum = sum + textureLoad(sharpenAFinal, vec2<i32>(sx, coord.y), 0).r;
      }
      let window = f32(2 * radius + 1);
      return vec4<f32>(sum / window, 0.0, 0.0, 1.0);
    }

    @fragment
    fn fs_sharpen_meana_v(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let dims = vec2<i32>(textureDimensions(blurScratchR32));
      let radius = sharpenRadiusPx(sharpenParams.radius);
      var sum = 0.0;
      for (var dy = -radius; dy <= radius; dy = dy + 1) {
        let sy = clamp(coord.y + dy, 0, dims.y - 1);
        sum = sum + textureLoad(blurScratchR32, vec2<i32>(coord.x, sy), 0).r;
      }
      let window = f32(2 * radius + 1);
      return vec4<f32>(sum / window, 0.0, 0.0, 1.0);
    }

    @fragment
    fn fs_sharpen_meanb_h(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let dims = vec2<i32>(textureDimensions(sharpenBFinal));
      let radius = sharpenRadiusPx(sharpenParams.radius);
      var sum = 0.0;
      for (var dx = -radius; dx <= radius; dx = dx + 1) {
        let sx = clamp(coord.x + dx, 0, dims.x - 1);
        sum = sum + textureLoad(sharpenBFinal, vec2<i32>(sx, coord.y), 0).r;
      }
      let window = f32(2 * radius + 1);
      return vec4<f32>(sum / window, 0.0, 0.0, 1.0);
    }

    @fragment
    fn fs_sharpen_meanb_v(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let dims = vec2<i32>(textureDimensions(blurScratchR32));
      let radius = sharpenRadiusPx(sharpenParams.radius);
      var sum = 0.0;
      for (var dy = -radius; dy <= radius; dy = dy + 1) {
        let sy = clamp(coord.y + dy, 0, dims.y - 1);
        sum = sum + textureLoad(blurScratchR32, vec2<i32>(coord.x, sy), 0).r;
      }
      let window = f32(2 * radius + 1);
      return vec4<f32>(sum / window, 0.0, 0.0, 1.0);
    }

    // Final: q = mean_a*luma + mean_b -- the guided-filter output replacing
    // the old plain box mean, written to sharpenBlurFinal, the SAME binding
    // (22) premask.js's own sharpen_delta port already reads -- that
    // downstream consumer needed no change at all.
    @fragment
    fn fs_sharpen_final(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let l = luma(textureLoad(gradedTex, coord, 0).rgb);
      let meanA = textureLoad(sharpenMeanAFinal, coord, 0).r;
      let meanB = textureLoad(sharpenMeanBFinal, coord, 0).r;
      return vec4<f32>(meanA * l + meanB, 0.0, 0.0, 1.0);
    }

    // Luma NR (RFC-0012): self-guided image filter (He, Sun, Tang, ECCV
    // 2010/TPAMI 2013), same specialization and pass shape RFC-0010
    // established for Clarity (fs_clarity_meanp_h through fs_clarity_v),
    // ported here at LUMA_NR_RADIUS/LUMA_NR_GUIDED_EPS instead. Reads
    // gradedTex directly (never rebound), matching this op's own existing
    // "always the same pre-Dehaze-recovery snapshot" convention -- unlike
    // Clarity/Texture's lcRgbInput, there is no second self-guided op
    // sharing these bindings, so no rebinding trick is needed here beyond
    // the shared blurScratchR32 H-scratch every box-filter pair already
    // reuses.
    @fragment
    fn fs_lumaNR_meanp_h(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let dims = vec2<i32>(textureDimensions(gradedTex));
      var sum = 0.0;
      for (var dx = -LUMA_NR_RADIUS; dx <= LUMA_NR_RADIUS; dx = dx + 1) {
        let sx = clamp(coord.x + dx, 0, dims.x - 1);
        sum = sum + luma(textureLoad(gradedTex, vec2<i32>(sx, coord.y), 0).rgb);
      }
      let window = f32(2 * LUMA_NR_RADIUS + 1);
      return vec4<f32>(sum / window, 0.0, 0.0, 1.0);
    }

    @fragment
    fn fs_lumaNR_meanp_v(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let dims = vec2<i32>(textureDimensions(blurScratchR32));
      var sum = 0.0;
      for (var dy = -LUMA_NR_RADIUS; dy <= LUMA_NR_RADIUS; dy = dy + 1) {
        let sy = clamp(coord.y + dy, 0, dims.y - 1);
        sum = sum + textureLoad(blurScratchR32, vec2<i32>(coord.x, sy), 0).r;
      }
      let window = f32(2 * LUMA_NR_RADIUS + 1);
      return vec4<f32>(sum / window, 0.0, 0.0, 1.0);
    }

    @fragment
    fn fs_lumaNR_corrp_h(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let dims = vec2<i32>(textureDimensions(gradedTex));
      var sum = 0.0;
      for (var dx = -LUMA_NR_RADIUS; dx <= LUMA_NR_RADIUS; dx = dx + 1) {
        let sx = clamp(coord.x + dx, 0, dims.x - 1);
        let l = luma(textureLoad(gradedTex, vec2<i32>(sx, coord.y), 0).rgb);
        sum = sum + l * l;
      }
      let window = f32(2 * LUMA_NR_RADIUS + 1);
      return vec4<f32>(sum / window, 0.0, 0.0, 1.0);
    }

    @fragment
    fn fs_lumaNR_corrp_v(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let dims = vec2<i32>(textureDimensions(blurScratchR32));
      var sum = 0.0;
      for (var dy = -LUMA_NR_RADIUS; dy <= LUMA_NR_RADIUS; dy = dy + 1) {
        let sy = clamp(coord.y + dy, 0, dims.y - 1);
        sum = sum + textureLoad(blurScratchR32, vec2<i32>(coord.x, sy), 0).r;
      }
      let window = f32(2 * LUMA_NR_RADIUS + 1);
      return vec4<f32>(sum / window, 0.0, 0.0, 1.0);
    }

    @fragment
    fn fs_lumaNR_a(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let meanP = textureLoad(lumaNrMeanPFinal, coord, 0).r;
      let corrP = textureLoad(lumaNrCorrPFinal, coord, 0).r;
      let varP = corrP - meanP * meanP;
      let a = varP / (varP + LUMA_NR_GUIDED_EPS);
      return vec4<f32>(a, 0.0, 0.0, 1.0);
    }

    @fragment
    fn fs_lumaNR_b(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let meanP = textureLoad(lumaNrMeanPFinal, coord, 0).r;
      let a = textureLoad(lumaNrAFinal, coord, 0).r;
      return vec4<f32>(meanP * (1.0 - a), 0.0, 0.0, 1.0);
    }

    @fragment
    fn fs_lumaNR_meana_h(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let dims = vec2<i32>(textureDimensions(lumaNrAFinal));
      var sum = 0.0;
      for (var dx = -LUMA_NR_RADIUS; dx <= LUMA_NR_RADIUS; dx = dx + 1) {
        let sx = clamp(coord.x + dx, 0, dims.x - 1);
        sum = sum + textureLoad(lumaNrAFinal, vec2<i32>(sx, coord.y), 0).r;
      }
      let window = f32(2 * LUMA_NR_RADIUS + 1);
      return vec4<f32>(sum / window, 0.0, 0.0, 1.0);
    }

    @fragment
    fn fs_lumaNR_meana_v(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let dims = vec2<i32>(textureDimensions(blurScratchR32));
      var sum = 0.0;
      for (var dy = -LUMA_NR_RADIUS; dy <= LUMA_NR_RADIUS; dy = dy + 1) {
        let sy = clamp(coord.y + dy, 0, dims.y - 1);
        sum = sum + textureLoad(blurScratchR32, vec2<i32>(coord.x, sy), 0).r;
      }
      let window = f32(2 * LUMA_NR_RADIUS + 1);
      return vec4<f32>(sum / window, 0.0, 0.0, 1.0);
    }

    @fragment
    fn fs_lumaNR_meanb_h(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let dims = vec2<i32>(textureDimensions(lumaNrBFinal));
      var sum = 0.0;
      for (var dx = -LUMA_NR_RADIUS; dx <= LUMA_NR_RADIUS; dx = dx + 1) {
        let sx = clamp(coord.x + dx, 0, dims.x - 1);
        sum = sum + textureLoad(lumaNrBFinal, vec2<i32>(sx, coord.y), 0).r;
      }
      let window = f32(2 * LUMA_NR_RADIUS + 1);
      return vec4<f32>(sum / window, 0.0, 0.0, 1.0);
    }

    @fragment
    fn fs_lumaNR_meanb_v(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let dims = vec2<i32>(textureDimensions(blurScratchR32));
      var sum = 0.0;
      for (var dy = -LUMA_NR_RADIUS; dy <= LUMA_NR_RADIUS; dy = dy + 1) {
        let sy = clamp(coord.y + dy, 0, dims.y - 1);
        sum = sum + textureLoad(blurScratchR32, vec2<i32>(coord.x, sy), 0).r;
      }
      let window = f32(2 * LUMA_NR_RADIUS + 1);
      return vec4<f32>(sum / window, 0.0, 0.0, 1.0);
    }

    // Final: q = mean_a*luma + mean_b -- the guided-filter output replacing
    // the old plain box mean, written to lumaNrBlurFinal, the SAME binding
    // (23) premask.js's own luma_nr_delta port already reads -- that
    // downstream consumer needed no change at all.
    @fragment
    fn fs_lumaNR_final(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let l = luma(textureLoad(gradedTex, coord, 0).rgb);
      let meanA = textureLoad(lumaNrMeanAFinal, coord, 0).r;
      let meanB = textureLoad(lumaNrMeanBFinal, coord, 0).r;
      return vec4<f32>(meanA * l + meanB, 0.0, 0.0, 1.0);
    }

    // Color NR (RFC-0014): the general two-signal guided filter (guide =
    // graded_luma, p = each of R/G/B independently, ALL sharing one guide),
    // not the self-guided shape Clarity/Luma NR use -- see
    // COLOR_NR_GUIDED_EPS's own comment above and detail.rs's matching
    // Rust doc comment for why a channel's own statistics make a poor
    // guide for itself here. The guide-only quantities (meanGuide,
    // corrGuide) are single-channel and computed ONCE, shared across all
    // three channels; the per-channel quantities (meanP, corrGuideP, a, b,
    // meanA, meanB) are each packed as one vec3 in an rgba16float texture,
    // filtered in a single joint pass per quantity -- a box filter is
    // per-channel-independent, the same linearity the OLD fs_colorNR_h/
    // fs_colorNR_v pair (now fs_colorNR_meanp_h/_v below, unchanged logic)
    // already relied on, applied one level deeper so this doesn't cost
    // 3x a fully independent per-channel guided filter would.
    @fragment
    fn fs_colorNR_meanguide_h(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let dims = vec2<i32>(textureDimensions(gradedTex));
      var sum = 0.0;
      for (var dx = -COLOR_NR_RADIUS; dx <= COLOR_NR_RADIUS; dx = dx + 1) {
        let sx = clamp(coord.x + dx, 0, dims.x - 1);
        sum = sum + luma(textureLoad(gradedTex, vec2<i32>(sx, coord.y), 0).rgb);
      }
      let window = f32(2 * COLOR_NR_RADIUS + 1);
      return vec4<f32>(sum / window, 0.0, 0.0, 1.0);
    }

    @fragment
    fn fs_colorNR_meanguide_v(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let dims = vec2<i32>(textureDimensions(blurScratchR32));
      var sum = 0.0;
      for (var dy = -COLOR_NR_RADIUS; dy <= COLOR_NR_RADIUS; dy = dy + 1) {
        let sy = clamp(coord.y + dy, 0, dims.y - 1);
        sum = sum + textureLoad(blurScratchR32, vec2<i32>(coord.x, sy), 0).r;
      }
      let window = f32(2 * COLOR_NR_RADIUS + 1);
      return vec4<f32>(sum / window, 0.0, 0.0, 1.0);
    }

    @fragment
    fn fs_colorNR_corrguide_h(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let dims = vec2<i32>(textureDimensions(gradedTex));
      var sum = 0.0;
      for (var dx = -COLOR_NR_RADIUS; dx <= COLOR_NR_RADIUS; dx = dx + 1) {
        let sx = clamp(coord.x + dx, 0, dims.x - 1);
        let l = luma(textureLoad(gradedTex, vec2<i32>(sx, coord.y), 0).rgb);
        sum = sum + l * l;
      }
      let window = f32(2 * COLOR_NR_RADIUS + 1);
      return vec4<f32>(sum / window, 0.0, 0.0, 1.0);
    }

    @fragment
    fn fs_colorNR_corrguide_v(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let dims = vec2<i32>(textureDimensions(blurScratchR32));
      var sum = 0.0;
      for (var dy = -COLOR_NR_RADIUS; dy <= COLOR_NR_RADIUS; dy = dy + 1) {
        let sy = clamp(coord.y + dy, 0, dims.y - 1);
        sum = sum + textureLoad(blurScratchR32, vec2<i32>(coord.x, sy), 0).r;
      }
      let window = f32(2 * COLOR_NR_RADIUS + 1);
      return vec4<f32>(sum / window, 0.0, 0.0, 1.0);
    }

    // Same computation the old fs_colorNR_h/fs_colorNR_v pair already did
    // (box mean of gradedTex.rgb) -- renamed and redirected to
    // colorNrMeanPFinal instead of being the filter's own final output.
    @fragment
    fn fs_colorNR_meanp_h(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let dims = vec2<i32>(textureDimensions(gradedTex));
      var sum = vec3<f32>(0.0, 0.0, 0.0);
      for (var dx = -COLOR_NR_RADIUS; dx <= COLOR_NR_RADIUS; dx = dx + 1) {
        let sx = clamp(coord.x + dx, 0, dims.x - 1);
        sum = sum + textureLoad(gradedTex, vec2<i32>(sx, coord.y), 0).rgb;
      }
      let window = f32(2 * COLOR_NR_RADIUS + 1);
      return vec4<f32>(sum / window, 1.0);
    }

    @fragment
    fn fs_colorNR_meanp_v(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let dims = vec2<i32>(textureDimensions(blurScratchRgba));
      var sum = vec3<f32>(0.0, 0.0, 0.0);
      for (var dy = -COLOR_NR_RADIUS; dy <= COLOR_NR_RADIUS; dy = dy + 1) {
        let sy = clamp(coord.y + dy, 0, dims.y - 1);
        sum = sum + textureLoad(blurScratchRgba, vec2<i32>(coord.x, sy), 0).rgb;
      }
      let window = f32(2 * COLOR_NR_RADIUS + 1);
      return vec4<f32>(sum / window, 1.0);
    }

    @fragment
    fn fs_colorNR_corrguidep_h(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let dims = vec2<i32>(textureDimensions(gradedTex));
      var sum = vec3<f32>(0.0, 0.0, 0.0);
      for (var dx = -COLOR_NR_RADIUS; dx <= COLOR_NR_RADIUS; dx = dx + 1) {
        let sx = clamp(coord.x + dx, 0, dims.x - 1);
        let rgb = textureLoad(gradedTex, vec2<i32>(sx, coord.y), 0).rgb;
        sum = sum + luma(rgb) * rgb;
      }
      let window = f32(2 * COLOR_NR_RADIUS + 1);
      return vec4<f32>(sum / window, 1.0);
    }

    @fragment
    fn fs_colorNR_corrguidep_v(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let dims = vec2<i32>(textureDimensions(blurScratchRgba));
      var sum = vec3<f32>(0.0, 0.0, 0.0);
      for (var dy = -COLOR_NR_RADIUS; dy <= COLOR_NR_RADIUS; dy = dy + 1) {
        let sy = clamp(coord.y + dy, 0, dims.y - 1);
        sum = sum + textureLoad(blurScratchRgba, vec2<i32>(coord.x, sy), 0).rgb;
      }
      let window = f32(2 * COLOR_NR_RADIUS + 1);
      return vec4<f32>(sum / window, 1.0);
    }

    @fragment
    fn fs_colorNR_a(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let meanGuide = textureLoad(colorNrMeanGuideFinal, coord, 0).r;
      let corrGuide = textureLoad(colorNrCorrGuideFinal, coord, 0).r;
      let meanP = textureLoad(colorNrMeanPFinal, coord, 0).rgb;
      let corrGuideP = textureLoad(colorNrCorrGuidePFinal, coord, 0).rgb;
      let varGuide = corrGuide - meanGuide * meanGuide;
      let covGuideP = corrGuideP - meanGuide * meanP;
      let a = covGuideP / (varGuide + COLOR_NR_GUIDED_EPS);
      return vec4<f32>(a, 1.0);
    }

    @fragment
    fn fs_colorNR_b(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let meanGuide = textureLoad(colorNrMeanGuideFinal, coord, 0).r;
      let meanP = textureLoad(colorNrMeanPFinal, coord, 0).rgb;
      let a = textureLoad(colorNrAFinal, coord, 0).rgb;
      return vec4<f32>(meanP - a * meanGuide, 1.0);
    }

    @fragment
    fn fs_colorNR_meana_h(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let dims = vec2<i32>(textureDimensions(colorNrAFinal));
      var sum = vec3<f32>(0.0, 0.0, 0.0);
      for (var dx = -COLOR_NR_RADIUS; dx <= COLOR_NR_RADIUS; dx = dx + 1) {
        let sx = clamp(coord.x + dx, 0, dims.x - 1);
        sum = sum + textureLoad(colorNrAFinal, vec2<i32>(sx, coord.y), 0).rgb;
      }
      let window = f32(2 * COLOR_NR_RADIUS + 1);
      return vec4<f32>(sum / window, 1.0);
    }

    @fragment
    fn fs_colorNR_meana_v(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let dims = vec2<i32>(textureDimensions(blurScratchRgba));
      var sum = vec3<f32>(0.0, 0.0, 0.0);
      for (var dy = -COLOR_NR_RADIUS; dy <= COLOR_NR_RADIUS; dy = dy + 1) {
        let sy = clamp(coord.y + dy, 0, dims.y - 1);
        sum = sum + textureLoad(blurScratchRgba, vec2<i32>(coord.x, sy), 0).rgb;
      }
      let window = f32(2 * COLOR_NR_RADIUS + 1);
      return vec4<f32>(sum / window, 1.0);
    }

    @fragment
    fn fs_colorNR_meanb_h(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let dims = vec2<i32>(textureDimensions(colorNrBFinal));
      var sum = vec3<f32>(0.0, 0.0, 0.0);
      for (var dx = -COLOR_NR_RADIUS; dx <= COLOR_NR_RADIUS; dx = dx + 1) {
        let sx = clamp(coord.x + dx, 0, dims.x - 1);
        sum = sum + textureLoad(colorNrBFinal, vec2<i32>(sx, coord.y), 0).rgb;
      }
      let window = f32(2 * COLOR_NR_RADIUS + 1);
      return vec4<f32>(sum / window, 1.0);
    }

    @fragment
    fn fs_colorNR_meanb_v(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let dims = vec2<i32>(textureDimensions(blurScratchRgba));
      var sum = vec3<f32>(0.0, 0.0, 0.0);
      for (var dy = -COLOR_NR_RADIUS; dy <= COLOR_NR_RADIUS; dy = dy + 1) {
        let sy = clamp(coord.y + dy, 0, dims.y - 1);
        sum = sum + textureLoad(blurScratchRgba, vec2<i32>(coord.x, sy), 0).rgb;
      }
      let window = f32(2 * COLOR_NR_RADIUS + 1);
      return vec4<f32>(sum / window, 1.0);
    }

    // Final: q = mean_a*luma + mean_b -- the guided-filter output replacing
    // the old plain per-channel box mean, written to colorNrBlurFinal, the
    // SAME binding (24) premask.js's own color_nr_delta port already reads
    // -- that downstream consumer needed no change at all.
    @fragment
    fn fs_colorNR_final(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let l = luma(textureLoad(gradedTex, coord, 0).rgb);
      let meanA = textureLoad(colorNrMeanAFinal, coord, 0).rgb;
      let meanB = textureLoad(colorNrMeanBFinal, coord, 0).rgb;
      return vec4<f32>(meanA * l + meanB, 1.0);
    }

`;
