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

    const SHARPEN_MAX_RADIUS_PX: i32 = 8;
    const SHARPEN_STRENGTH: f32 = 1.6;
    const SHARPEN_DETAIL_SCALE: f32 = 0.06;
    const SHARPEN_MASK_SCALE: f32 = 0.05;
    const LUMA_NR_RADIUS: i32 = 3;
    const NR_DETAIL_SCALE: f32 = 0.05;
    const NR_CONTRAST_STRENGTH: f32 = 0.6;
    const COLOR_NR_RADIUS: i32 = 4;
    const COLOR_NR_DETAIL_SCALE: f32 = 0.08;

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

    @fragment
    fn fs_sharpen_h(in: VertexOut) -> @location(0) vec4<f32> {
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
    fn fs_sharpen_v(in: VertexOut) -> @location(0) vec4<f32> {
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
    fn fs_lumaNR_h(in: VertexOut) -> @location(0) vec4<f32> {
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
    fn fs_lumaNR_v(in: VertexOut) -> @location(0) vec4<f32> {
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
    fn fs_colorNR_h(in: VertexOut) -> @location(0) vec4<f32> {
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
    fn fs_colorNR_v(in: VertexOut) -> @location(0) vec4<f32> {
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

`;
