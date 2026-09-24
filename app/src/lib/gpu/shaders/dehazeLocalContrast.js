// Dehaze and local-contrast (texture/clarity) passes: reductions and separable min/mean/blur filters.
// Part of the WGSL source, split out of DevelopCanvas.svelte; see index.js for the
// concatenation order, which must not change.
export const dehazeLocalContrast = `    // Dehaze (M3): dark-channel-prior haze removal (He et al. 2009), a
    // direct WGSL port of develop_engine.rs's own restructured algorithm --
    // see that module's doc comment above dehaze_atmospheric_light for the
    // full algorithm, its two named deviations from He et al., and the
    // design review that caught real bugs in earlier drafts of both before
    // this was written. Fixed constants, matching the Rust twin exactly.
    const DEHAZE_PATCH_RADIUS: i32 = 7;
    const DEHAZE_OMEGA: f32 = 0.95;
    const DEHAZE_T0: f32 = 0.1;
    const DEHAZE_REFINE_RADIUS: i32 = 4;

    // All new intermediates read via textureLoad (explicit integer coords,
    // mip 0), not textureSample -- r32float/rgba16float aren't filterable
    // by default in WebGPU (needs an unrequested optional feature), and an
    // exact block/window reduction needs exact taps anyway, never a
    // filtered blend. gradedTex/atmLightFinal/transmissionTexFinal are each
    // read by a FIXED set of passes and never rebound mid-frame;
    // reduceInput/filterInput are each reused across MULTIPLE passes,
    // rebound to a different actual texture per pass (a different bind
    // group each time, same pipeline/shader code) -- see the JS side's own
    // pass-list for exactly which real texture each is bound to per draw.
    @group(0) @binding(8) var gradedTex: texture_2d<f32>;
    @group(0) @binding(9) var reduceInput: texture_2d<f32>;
    @group(0) @binding(10) var atmLightFinal: texture_2d<f32>;
    @group(0) @binding(11) var filterInput: texture_2d<f32>;
    @group(0) @binding(12) var transmissionTexFinal: texture_2d<f32>;
    // Texture & Clarity (M3): lcRgbInput is rebound per pass -- gradedTex
    // for Texture's H/V passes, textureAdjustedTex for Clarity's H/V
    // passes (see the JS side's own bind-group list) -- lcBlurInput is the
    // horizontal-blur scratch texture each op's own V pass reads to
    // complete the box-mean.
    @group(0) @binding(13) var lcRgbInput: texture_2d<f32>;
    @group(0) @binding(14) var lcBlurInput: texture_2d<f32>;

    fn luma(rgb: vec3<f32>) -> f32 {
      return dot(rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
    }

    // Texture & Clarity (M3): a direct WGSL port of develop_engine.rs's
    // own apply_local_contrast -- see that function's doc comment for the
    // additive-delta formula and why it was chosen over a luma-ratio
    // rescale (a design review caught a real hue-scrambling bug in the
    // ratio version whenever luma is near zero or negative, which
    // Contrast alone already makes reachable). Fixed radii, matching the
    // Rust twin exactly.
    const TEXTURE_RADIUS: i32 = 6;
    const CLARITY_RADIUS: i32 = 24;

    @fragment
    fn fs_texture_h(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let dims = vec2<i32>(textureDimensions(lcRgbInput));
      var sum = 0.0;
      for (var dx = -TEXTURE_RADIUS; dx <= TEXTURE_RADIUS; dx = dx + 1) {
        let sx = clamp(coord.x + dx, 0, dims.x - 1);
        sum = sum + luma(textureLoad(lcRgbInput, vec2<i32>(sx, coord.y), 0).rgb);
      }
      let window = f32(2 * TEXTURE_RADIUS + 1);
      return vec4<f32>(sum / window, 0.0, 0.0, 1.0);
    }

    // Vertical pass, completing Texture's blur, then folding in the
    // apply step directly (one fewer pass than a separate step would
    // need, matching fs_min_v's own precedent): re-reads lcRgbInput
    // (gradedTex) at this pixel for the original rgb/luma, adds the same
    // delta to all three channels, and writes the result to
    // textureAdjustedTex.
    @fragment
    fn fs_texture_v(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let dims = vec2<i32>(textureDimensions(lcBlurInput));
      var sum = 0.0;
      for (var dy = -TEXTURE_RADIUS; dy <= TEXTURE_RADIUS; dy = dy + 1) {
        let sy = clamp(coord.y + dy, 0, dims.y - 1);
        sum = sum + textureLoad(lcBlurInput, vec2<i32>(coord.x, sy), 0).r;
      }
      let window = f32(2 * TEXTURE_RADIUS + 1);
      let blurred = sum / window;

      let rgb = textureLoad(lcRgbInput, coord, 0).rgb;
      let l = luma(rgb);
      let delta = (l - blurred) * (adj.texture_amount / 100.0);
      return vec4<f32>(rgb + vec3<f32>(delta, delta, delta), 1.0);
    }

    // Clarity (RFC-0010): self-guided image filter (He, Sun, Tang, ECCV
    // 2010/TPAMI 2013) replacing the plain box mean Texture above still
    // uses -- see develop_engine.rs's own guided_filter_self/apply_clarity
    // doc comments for the full derivation and both named boundary
    // identities; this is a direct WGSL port, pass-for-pass.
    // CLARITY_GUIDED_EPS matches the Rust twin exactly. Six new persistent
    // single-channel intermediates (mean_p, corr_p, a, b, mean_a, mean_b),
    // each its own binding since more than one is read simultaneously by a
    // later pass (unlike lcRgbInput/lcBlurInput's rebinding trick, which
    // only ever needs ONE actual texture live at a time). Every box-filter
    // pass below still routes its own H-pass output through the shared
    // lcBlurInput rebinding (binding 14) for its V pass, same convention
    // as fs_texture_h/v and every Dehaze H/V pair.
    const CLARITY_GUIDED_EPS: f32 = 0.01;

    @group(0) @binding(29) var clarityMeanPFinal: texture_2d<f32>;
    @group(0) @binding(30) var clarityCorrPFinal: texture_2d<f32>;
    @group(0) @binding(31) var clarityAFinal: texture_2d<f32>;
    @group(0) @binding(32) var clarityBFinal: texture_2d<f32>;
    @group(0) @binding(33) var clarityMeanAFinal: texture_2d<f32>;
    @group(0) @binding(34) var clarityMeanBFinal: texture_2d<f32>;

    // mean_p: a plain box mean of luma at CLARITY_RADIUS -- identical
    // shape to fs_texture_h/v above (same radius, same lcRgbInput), just a
    // separate pipeline that stops at the plain mean instead of folding in
    // a delta-apply step, since the guided filter needs mean_p itself as
    // an input to later passes, not yet a finished adjustment.
    @fragment
    fn fs_clarity_meanp_h(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let dims = vec2<i32>(textureDimensions(lcRgbInput));
      var sum = 0.0;
      for (var dx = -CLARITY_RADIUS; dx <= CLARITY_RADIUS; dx = dx + 1) {
        let sx = clamp(coord.x + dx, 0, dims.x - 1);
        sum = sum + luma(textureLoad(lcRgbInput, vec2<i32>(sx, coord.y), 0).rgb);
      }
      let window = f32(2 * CLARITY_RADIUS + 1);
      return vec4<f32>(sum / window, 0.0, 0.0, 1.0);
    }

    @fragment
    fn fs_clarity_meanp_v(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let dims = vec2<i32>(textureDimensions(lcBlurInput));
      var sum = 0.0;
      for (var dy = -CLARITY_RADIUS; dy <= CLARITY_RADIUS; dy = dy + 1) {
        let sy = clamp(coord.y + dy, 0, dims.y - 1);
        sum = sum + textureLoad(lcBlurInput, vec2<i32>(coord.x, sy), 0).r;
      }
      let window = f32(2 * CLARITY_RADIUS + 1);
      return vec4<f32>(sum / window, 0.0, 0.0, 1.0);
    }

    // corr_p: box mean of luma^2 at the same radius -- same H/V shape,
    // squares each tap before accumulating instead of taking it plain.
    @fragment
    fn fs_clarity_corrp_h(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let dims = vec2<i32>(textureDimensions(lcRgbInput));
      var sum = 0.0;
      for (var dx = -CLARITY_RADIUS; dx <= CLARITY_RADIUS; dx = dx + 1) {
        let sx = clamp(coord.x + dx, 0, dims.x - 1);
        let l = luma(textureLoad(lcRgbInput, vec2<i32>(sx, coord.y), 0).rgb);
        sum = sum + l * l;
      }
      let window = f32(2 * CLARITY_RADIUS + 1);
      return vec4<f32>(sum / window, 0.0, 0.0, 1.0);
    }

    @fragment
    fn fs_clarity_corrp_v(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let dims = vec2<i32>(textureDimensions(lcBlurInput));
      var sum = 0.0;
      for (var dy = -CLARITY_RADIUS; dy <= CLARITY_RADIUS; dy = dy + 1) {
        let sy = clamp(coord.y + dy, 0, dims.y - 1);
        sum = sum + textureLoad(lcBlurInput, vec2<i32>(coord.x, sy), 0).r;
      }
      let window = f32(2 * CLARITY_RADIUS + 1);
      return vec4<f32>(sum / window, 0.0, 0.0, 1.0);
    }

    // a = var_p / (var_p + eps), var_p = corr_p - mean_p^2 -- a single
    // per-pixel pass, no window loop. Split from b below into its own
    // pass only because this module's own convention is one render target
    // per pass (every other pass here already follows it) -- the Rust
    // twin computes both in one per-pixel loop iteration, no GPU pass
    // boundary to worry about there.
    @fragment
    fn fs_clarity_a(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let meanP = textureLoad(clarityMeanPFinal, coord, 0).r;
      let corrP = textureLoad(clarityCorrPFinal, coord, 0).r;
      let varP = corrP - meanP * meanP;
      let a = varP / (varP + CLARITY_GUIDED_EPS);
      return vec4<f32>(a, 0.0, 0.0, 1.0);
    }

    // b = mean_p * (1 - a) -- reads mean_p again plus this pixel's own
    // just-written a.
    @fragment
    fn fs_clarity_b(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let meanP = textureLoad(clarityMeanPFinal, coord, 0).r;
      let a = textureLoad(clarityAFinal, coord, 0).r;
      return vec4<f32>(meanP * (1.0 - a), 0.0, 0.0, 1.0);
    }

    // mean_a / mean_b: box mean of a and b respectively -- same H/V shape
    // as mean_p/corr_p above, over these new single-channel inputs
    // instead of luma.
    @fragment
    fn fs_clarity_meana_h(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let dims = vec2<i32>(textureDimensions(clarityAFinal));
      var sum = 0.0;
      for (var dx = -CLARITY_RADIUS; dx <= CLARITY_RADIUS; dx = dx + 1) {
        let sx = clamp(coord.x + dx, 0, dims.x - 1);
        sum = sum + textureLoad(clarityAFinal, vec2<i32>(sx, coord.y), 0).r;
      }
      let window = f32(2 * CLARITY_RADIUS + 1);
      return vec4<f32>(sum / window, 0.0, 0.0, 1.0);
    }

    @fragment
    fn fs_clarity_meana_v(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let dims = vec2<i32>(textureDimensions(lcBlurInput));
      var sum = 0.0;
      for (var dy = -CLARITY_RADIUS; dy <= CLARITY_RADIUS; dy = dy + 1) {
        let sy = clamp(coord.y + dy, 0, dims.y - 1);
        sum = sum + textureLoad(lcBlurInput, vec2<i32>(coord.x, sy), 0).r;
      }
      let window = f32(2 * CLARITY_RADIUS + 1);
      return vec4<f32>(sum / window, 0.0, 0.0, 1.0);
    }

    @fragment
    fn fs_clarity_meanb_h(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let dims = vec2<i32>(textureDimensions(clarityBFinal));
      var sum = 0.0;
      for (var dx = -CLARITY_RADIUS; dx <= CLARITY_RADIUS; dx = dx + 1) {
        let sx = clamp(coord.x + dx, 0, dims.x - 1);
        sum = sum + textureLoad(clarityBFinal, vec2<i32>(sx, coord.y), 0).r;
      }
      let window = f32(2 * CLARITY_RADIUS + 1);
      return vec4<f32>(sum / window, 0.0, 0.0, 1.0);
    }

    @fragment
    fn fs_clarity_meanb_v(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let dims = vec2<i32>(textureDimensions(lcBlurInput));
      var sum = 0.0;
      for (var dy = -CLARITY_RADIUS; dy <= CLARITY_RADIUS; dy = dy + 1) {
        let sy = clamp(coord.y + dy, 0, dims.y - 1);
        sum = sum + textureLoad(lcBlurInput, vec2<i32>(coord.x, sy), 0).r;
      }
      let window = f32(2 * CLARITY_RADIUS + 1);
      return vec4<f32>(sum / window, 0.0, 0.0, 1.0);
    }

    // Final: q = mean_a*luma + mean_b (the guided-filter output replacing
    // the old plain "blurred"), delta = (luma - q)*clarity_amount, written
    // to gradedTex -- same additive-delta shape and same final render
    // target as before (Clarity is still the last of the two
    // local-contrast ops, overwriting gradedTex in place; see this
    // binding's own doc comment above for why every downstream consumer
    // already reads gradedTex strictly after this point in pass order).
    @fragment
    fn fs_clarity_v(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let rgb = textureLoad(lcRgbInput, coord, 0).rgb;
      let l = luma(rgb);
      let meanA = textureLoad(clarityMeanAFinal, coord, 0).r;
      let meanB = textureLoad(clarityMeanBFinal, coord, 0).r;
      let q = meanA * l + meanB;
      let delta = (l - q) * (adj.clarity_amount / 100.0);
      return vec4<f32>(rgb + vec3<f32>(delta, delta, delta), 1.0);
    }

    // Atmospheric-light reduction: one pass of an 8x8-block ARGMAX-BY-
    // LUMINANCE reduction (see develop_engine.rs's dehaze_atmospheric_light
    // doc comment for why this, not independent per-channel maxima -- a
    // real bug an earlier draft had) -- run repeatedly against
    // progressively smaller inputs (JS side decides how many times, sized
    // to the source image's own dimensions) until reaching 1x1. Duplicate
    // clamped edge taps are harmless for an argmax (unlike a sum), since a
    // repeated candidate can't change which one wins.
    @fragment
    fn fs_atm_reduce(in: VertexOut) -> @location(0) vec4<f32> {
      let outCoord = vec2<i32>(in.position.xy);
      let dims = vec2<i32>(textureDimensions(reduceInput));
      var best = vec3<f32>(0.0, 0.0, 0.0);
      var bestLuma = -1.0;
      for (var dy = 0; dy < 8; dy = dy + 1) {
        for (var dx = 0; dx < 8; dx = dx + 1) {
          let coord = clamp(outCoord * 8 + vec2<i32>(dx, dy), vec2<i32>(0, 0), dims - vec2<i32>(1, 1));
          let c = textureLoad(reduceInput, coord, 0).rgb;
          let l = luma(c);
          if (l > bestLuma) {
            bestLuma = l;
            best = c;
          }
        }
      }
      return vec4<f32>(best, 1.0);
    }

    // Normalized min-channel: min_c(I^c/A^c), per pixel -- the atmospheric-
    // light division happens HERE, before the windowed min below, matching
    // He et al. exactly. A real bug an earlier draft had: collapsing the
    // cross-channel min FIRST and dividing the resulting scalar by a
    // single scalar representative of A afterward is not a numerically-
    // close approximation but a structurally different, generally wrong,
    // result (see develop_engine.rs's own doc comment for the concrete
    // counterexample that caught this).
    @fragment
    fn fs_min_channel(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let c = textureLoad(gradedTex, coord, 0).rgb;
      let a = textureLoad(atmLightFinal, vec2<i32>(0, 0), 0).rgb;
      let m = min(c.r / a.r, min(c.g / a.g, c.b / a.b));
      return vec4<f32>(m, 0.0, 0.0, 1.0);
    }

    // Separable box-MIN filter (the dark-channel step): horizontal pass
    // then vertical pass -- correct (not just cheaper) because a
    // rectangular window's min is associative/commutative over its two
    // axes independently. Edge taps clamp to the input texture's own
    // bounds -- textureLoad on an out-of-range coordinate silently returns
    // zero, which would corrupt the min near image borders if not guarded.
    @fragment
    fn fs_min_h(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let dims = vec2<i32>(textureDimensions(filterInput));
      var m = 1e6;
      for (var dx = -DEHAZE_PATCH_RADIUS; dx <= DEHAZE_PATCH_RADIUS; dx = dx + 1) {
        let sx = clamp(coord.x + dx, 0, dims.x - 1);
        m = min(m, textureLoad(filterInput, vec2<i32>(sx, coord.y), 0).r);
      }
      return vec4<f32>(m, 0.0, 0.0, 1.0);
    }

    // Vertical pass, completing the dark channel -- folds in the raw-
    // transmission step (1 - omega * darkChannel) directly (one fewer pass
    // than a separate step would need), matching develop_engine.rs's own
    // combined t_raw computation.
    @fragment
    fn fs_min_v(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let dims = vec2<i32>(textureDimensions(filterInput));
      var m = 1e6;
      for (var dy = -DEHAZE_PATCH_RADIUS; dy <= DEHAZE_PATCH_RADIUS; dy = dy + 1) {
        let sy = clamp(coord.y + dy, 0, dims.y - 1);
        m = min(m, textureLoad(filterInput, vec2<i32>(coord.x, sy), 0).r);
      }
      let tRaw = 1.0 - DEHAZE_OMEGA * m;
      return vec4<f32>(tRaw, 0.0, 0.0, 1.0);
    }

    // Transmission refinement (RFC-0011): general two-signal guided filter
    // (He, Sun, Tang, ECCV 2010/TPAMI 2013 §4, haze removal) replacing the
    // plain box mean this used to be -- see develop_engine.rs's own
    // guided_filter doc comment for the full derivation and why this is a
    // DIFFERENT (larger) algorithm from Clarity's self-guided case above,
    // not a second caller of the same one. Guidance is the graded image's
    // own luma (gradedTex); the signal being filtered is t_raw (read via
    // filterInput, same rebinding this file's own header comment already
    // documents). DEHAZE_GUIDED_EPS matches the Rust twin exactly.
    //
    // 15 passes total (vs. 2 before): mean_guide, mean_p, corr_guide,
    // corr_guide_p (4 box-filter pairs -- twice Clarity's self-guided case,
    // since the general algorithm needs four distinct filtered quantities
    // where the self case only needed two), the a/b compose (2 per-pixel
    // passes), mean_a/mean_b (2 more box-filter pairs), then the final
    // compose. Every H-pass here shares transmissionHTex as its scratch,
    // completed by its own V-pass reading it back via filterInput's
    // existing rebinding trick -- same discipline RFC-0010's Clarity
    // passes established for clarityBlurScratchTex.
    const DEHAZE_GUIDED_EPS: f32 = 0.0001;

    @group(0) @binding(35) var dehazeMeanGuideFinal: texture_2d<f32>;
    @group(0) @binding(36) var dehazeMeanPFinal: texture_2d<f32>;
    @group(0) @binding(37) var dehazeCorrGuideFinal: texture_2d<f32>;
    @group(0) @binding(38) var dehazeCorrGuidePFinal: texture_2d<f32>;
    @group(0) @binding(39) var dehazeAFinal: texture_2d<f32>;
    @group(0) @binding(40) var dehazeBFinal: texture_2d<f32>;
    @group(0) @binding(41) var dehazeMeanAFinal: texture_2d<f32>;
    @group(0) @binding(42) var dehazeMeanBFinal: texture_2d<f32>;

    // mean_guide: box mean of the scene's own luma.
    @fragment
    fn fs_dehaze_meanguide_h(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let dims = vec2<i32>(textureDimensions(gradedTex));
      var sum = 0.0;
      for (var dx = -DEHAZE_REFINE_RADIUS; dx <= DEHAZE_REFINE_RADIUS; dx = dx + 1) {
        let sx = clamp(coord.x + dx, 0, dims.x - 1);
        sum = sum + luma(textureLoad(gradedTex, vec2<i32>(sx, coord.y), 0).rgb);
      }
      let window = f32(2 * DEHAZE_REFINE_RADIUS + 1);
      return vec4<f32>(sum / window, 0.0, 0.0, 1.0);
    }

    @fragment
    fn fs_dehaze_meanguide_v(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let dims = vec2<i32>(textureDimensions(filterInput));
      var sum = 0.0;
      for (var dy = -DEHAZE_REFINE_RADIUS; dy <= DEHAZE_REFINE_RADIUS; dy = dy + 1) {
        let sy = clamp(coord.y + dy, 0, dims.y - 1);
        sum = sum + textureLoad(filterInput, vec2<i32>(coord.x, sy), 0).r;
      }
      let window = f32(2 * DEHAZE_REFINE_RADIUS + 1);
      return vec4<f32>(sum / window, 0.0, 0.0, 1.0);
    }

    // mean_p: box mean of t_raw -- identical shape to the old fs_mean_h/v
    // this replaces, just no longer the FINAL output (that's the last pass
    // below now).
    @fragment
    fn fs_dehaze_meanp_h(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let dims = vec2<i32>(textureDimensions(filterInput));
      var sum = 0.0;
      for (var dx = -DEHAZE_REFINE_RADIUS; dx <= DEHAZE_REFINE_RADIUS; dx = dx + 1) {
        let sx = clamp(coord.x + dx, 0, dims.x - 1);
        sum = sum + textureLoad(filterInput, vec2<i32>(sx, coord.y), 0).r;
      }
      let window = f32(2 * DEHAZE_REFINE_RADIUS + 1);
      return vec4<f32>(sum / window, 0.0, 0.0, 1.0);
    }

    @fragment
    fn fs_dehaze_meanp_v(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let dims = vec2<i32>(textureDimensions(filterInput));
      var sum = 0.0;
      for (var dy = -DEHAZE_REFINE_RADIUS; dy <= DEHAZE_REFINE_RADIUS; dy = dy + 1) {
        let sy = clamp(coord.y + dy, 0, dims.y - 1);
        sum = sum + textureLoad(filterInput, vec2<i32>(coord.x, sy), 0).r;
      }
      let window = f32(2 * DEHAZE_REFINE_RADIUS + 1);
      return vec4<f32>(sum / window, 0.0, 0.0, 1.0);
    }

    // corr_guide: box mean of luma(gradedTex)^2.
    @fragment
    fn fs_dehaze_corrguide_h(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let dims = vec2<i32>(textureDimensions(gradedTex));
      var sum = 0.0;
      for (var dx = -DEHAZE_REFINE_RADIUS; dx <= DEHAZE_REFINE_RADIUS; dx = dx + 1) {
        let sx = clamp(coord.x + dx, 0, dims.x - 1);
        let l = luma(textureLoad(gradedTex, vec2<i32>(sx, coord.y), 0).rgb);
        sum = sum + l * l;
      }
      let window = f32(2 * DEHAZE_REFINE_RADIUS + 1);
      return vec4<f32>(sum / window, 0.0, 0.0, 1.0);
    }

    @fragment
    fn fs_dehaze_corrguide_v(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let dims = vec2<i32>(textureDimensions(filterInput));
      var sum = 0.0;
      for (var dy = -DEHAZE_REFINE_RADIUS; dy <= DEHAZE_REFINE_RADIUS; dy = dy + 1) {
        let sy = clamp(coord.y + dy, 0, dims.y - 1);
        sum = sum + textureLoad(filterInput, vec2<i32>(coord.x, sy), 0).r;
      }
      let window = f32(2 * DEHAZE_REFINE_RADIUS + 1);
      return vec4<f32>(sum / window, 0.0, 0.0, 1.0);
    }

    // corr_guide_p: box mean of luma(gradedTex) * t_raw -- the only H pass
    // here reading two different textures at once (gradedTex for the
    // guide, filterInput bound to tRawTex for p).
    @fragment
    fn fs_dehaze_corrguidep_h(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let dims = vec2<i32>(textureDimensions(gradedTex));
      var sum = 0.0;
      for (var dx = -DEHAZE_REFINE_RADIUS; dx <= DEHAZE_REFINE_RADIUS; dx = dx + 1) {
        let sx = clamp(coord.x + dx, 0, dims.x - 1);
        let l = luma(textureLoad(gradedTex, vec2<i32>(sx, coord.y), 0).rgb);
        let p = textureLoad(filterInput, vec2<i32>(sx, coord.y), 0).r;
        sum = sum + l * p;
      }
      let window = f32(2 * DEHAZE_REFINE_RADIUS + 1);
      return vec4<f32>(sum / window, 0.0, 0.0, 1.0);
    }

    @fragment
    fn fs_dehaze_corrguidep_v(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let dims = vec2<i32>(textureDimensions(filterInput));
      var sum = 0.0;
      for (var dy = -DEHAZE_REFINE_RADIUS; dy <= DEHAZE_REFINE_RADIUS; dy = dy + 1) {
        let sy = clamp(coord.y + dy, 0, dims.y - 1);
        sum = sum + textureLoad(filterInput, vec2<i32>(coord.x, sy), 0).r;
      }
      let window = f32(2 * DEHAZE_REFINE_RADIUS + 1);
      return vec4<f32>(sum / window, 0.0, 0.0, 1.0);
    }

    // a = cov_guide_p / (var_guide + eps), var_guide = corr_guide -
    // mean_guide^2, cov_guide_p = corr_guide_p - mean_guide*mean_p -- a
    // single per-pixel pass, no window loop (same "split from b below
    // because this file's own convention is one render target per pass"
    // reasoning RFC-0010's fs_clarity_a already established).
    @fragment
    fn fs_dehaze_a(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let meanGuide = textureLoad(dehazeMeanGuideFinal, coord, 0).r;
      let meanP = textureLoad(dehazeMeanPFinal, coord, 0).r;
      let corrGuide = textureLoad(dehazeCorrGuideFinal, coord, 0).r;
      let corrGuideP = textureLoad(dehazeCorrGuidePFinal, coord, 0).r;
      let varGuide = corrGuide - meanGuide * meanGuide;
      let covGuideP = corrGuideP - meanGuide * meanP;
      let a = covGuideP / (varGuide + DEHAZE_GUIDED_EPS);
      return vec4<f32>(a, 0.0, 0.0, 1.0);
    }

    // b = mean_p - a*mean_guide.
    @fragment
    fn fs_dehaze_b(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let meanGuide = textureLoad(dehazeMeanGuideFinal, coord, 0).r;
      let meanP = textureLoad(dehazeMeanPFinal, coord, 0).r;
      let a = textureLoad(dehazeAFinal, coord, 0).r;
      return vec4<f32>(meanP - a * meanGuide, 0.0, 0.0, 1.0);
    }

    // mean_a / mean_b: box mean of a and b respectively.
    @fragment
    fn fs_dehaze_meana_h(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let dims = vec2<i32>(textureDimensions(dehazeAFinal));
      var sum = 0.0;
      for (var dx = -DEHAZE_REFINE_RADIUS; dx <= DEHAZE_REFINE_RADIUS; dx = dx + 1) {
        let sx = clamp(coord.x + dx, 0, dims.x - 1);
        sum = sum + textureLoad(dehazeAFinal, vec2<i32>(sx, coord.y), 0).r;
      }
      let window = f32(2 * DEHAZE_REFINE_RADIUS + 1);
      return vec4<f32>(sum / window, 0.0, 0.0, 1.0);
    }

    @fragment
    fn fs_dehaze_meana_v(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let dims = vec2<i32>(textureDimensions(filterInput));
      var sum = 0.0;
      for (var dy = -DEHAZE_REFINE_RADIUS; dy <= DEHAZE_REFINE_RADIUS; dy = dy + 1) {
        let sy = clamp(coord.y + dy, 0, dims.y - 1);
        sum = sum + textureLoad(filterInput, vec2<i32>(coord.x, sy), 0).r;
      }
      let window = f32(2 * DEHAZE_REFINE_RADIUS + 1);
      return vec4<f32>(sum / window, 0.0, 0.0, 1.0);
    }

    @fragment
    fn fs_dehaze_meanb_h(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let dims = vec2<i32>(textureDimensions(dehazeBFinal));
      var sum = 0.0;
      for (var dx = -DEHAZE_REFINE_RADIUS; dx <= DEHAZE_REFINE_RADIUS; dx = dx + 1) {
        let sx = clamp(coord.x + dx, 0, dims.x - 1);
        sum = sum + textureLoad(dehazeBFinal, vec2<i32>(sx, coord.y), 0).r;
      }
      let window = f32(2 * DEHAZE_REFINE_RADIUS + 1);
      return vec4<f32>(sum / window, 0.0, 0.0, 1.0);
    }

    @fragment
    fn fs_dehaze_meanb_v(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let dims = vec2<i32>(textureDimensions(filterInput));
      var sum = 0.0;
      for (var dy = -DEHAZE_REFINE_RADIUS; dy <= DEHAZE_REFINE_RADIUS; dy = dy + 1) {
        let sy = clamp(coord.y + dy, 0, dims.y - 1);
        sum = sum + textureLoad(filterInput, vec2<i32>(coord.x, sy), 0).r;
      }
      let window = f32(2 * DEHAZE_REFINE_RADIUS + 1);
      return vec4<f32>(sum / window, 0.0, 0.0, 1.0);
    }

    // Final: q = mean_a*luma(gradedTex) + mean_b -- t_refined itself,
    // written to transmissionTex (the SAME final texture premask.js's own
    // recovery step already reads; this pass's only change from before is
    // what feeds it).
    @fragment
    fn fs_dehaze_refine(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let l = luma(textureLoad(gradedTex, coord, 0).rgb);
      let meanA = textureLoad(dehazeMeanAFinal, coord, 0).r;
      let meanB = textureLoad(dehazeMeanBFinal, coord, 0).r;
      return vec4<f32>(meanA * l + meanB, 0.0, 0.0, 1.0);
    }

`;
