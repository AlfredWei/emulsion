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

    @fragment
    fn fs_clarity_h(in: VertexOut) -> @location(0) vec4<f32> {
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

    // Vertical pass, completing Clarity's blur + apply -- same shape as
    // fs_texture_v, but this pass's render target (set on the JS side, not
    // a WGSL binding) is gradedTex ITSELF: Clarity is the last of the two
    // local-contrast ops, so its output overwrites gradedTex in place
    // rather than needing a third "final graded" texture. Every downstream
    // consumer of gradedTex (fs_atm_reduce's first pass, fs_min_channel,
    // fs_final) already reads it AFTER this point in pass order, so they
    // transparently see Texture+Clarity already baked in -- sound because
    // WebGPU passes within one command encoder execute strictly in
    // recorded order, the same guarantee gradedTex already relies on today
    // (written once by fs_grade, read three times later in the frame).
    @fragment
    fn fs_clarity_v(in: VertexOut) -> @location(0) vec4<f32> {
      let coord = vec2<i32>(in.position.xy);
      let dims = vec2<i32>(textureDimensions(lcBlurInput));
      var sum = 0.0;
      for (var dy = -CLARITY_RADIUS; dy <= CLARITY_RADIUS; dy = dy + 1) {
        let sy = clamp(coord.y + dy, 0, dims.y - 1);
        sum = sum + textureLoad(lcBlurInput, vec2<i32>(coord.x, sy), 0).r;
      }
      let window = f32(2 * CLARITY_RADIUS + 1);
      let blurred = sum / window;

      let rgb = textureLoad(lcRgbInput, coord, 0).rgb;
      let l = luma(rgb);
      let delta = (l - blurred) * (adj.clarity_amount / 100.0);
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

    // Separable box-MEAN filter (transmission refinement, standing in for
    // He et al.'s edge-preserving guided filter -- named limitation: mild
    // haloing near strong contrast edges a real guided filter would
    // avoid). A sum-based sliding-window accumulator would be cheaper on
    // the CPU side (see develop_engine.rs's separable_mean_filter), but a
    // naive per-tap sum here is simplest and still cheap at this radius --
    // GPU fragment shaders parallelize across pixels, not within one.
    @fragment
    fn fs_mean_h(in: VertexOut) -> @location(0) vec4<f32> {
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
    fn fs_mean_v(in: VertexOut) -> @location(0) vec4<f32> {
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

`;
