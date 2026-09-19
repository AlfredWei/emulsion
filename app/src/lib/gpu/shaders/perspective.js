// Perspective (keystone) params and the fs_perspective pass.
// Part of the WGSL source, split out of DevelopCanvas.svelte; see index.js for the
// concatenation order, which must not change.
export const perspective = `    // Perspective Correction (M4): a direct WGSL port of
    // develop_engine.rs's own perspective_warp_coord/PerspectiveNorm --
    // see that module's own extensive header comment for the full
    // homography derivation. Its OWN pass, chained right after
    // fs_lens_correct (reads srcTexture/srcSampler rebound to
    // lensCorrectedTex -- see perspectivePipeline's own doc comment),
    // writing into perspectiveCorrectedTex, for the same "genuine
    // resample" reason lens correction needed its own pass.
    //
    // Out-of-bounds sampling here falls back to the sampler's own
    // clamp-to-edge addressing (initGpu's own sampler, WebGPU's default),
    // NOT the Rust twin's true black corners (sample_bilinear's own
    // documented out-of-bounds contract) -- an already-accepted GPU-
    // preview-vs-CPU-export divergence, same one lens correction's own
    // geometry pass already has for the same reason (no cheap portable
    // per-tap bounds check against a linear-filtered sampler here); the
    // export/thumbnail-regen path the user actually delivers from always
    // uses the exact Rust formula.
    struct PerspectiveParams {
      vertical: f32,
      horizontal: f32,
      rotate: f32,
      aspect: f32,
      scale: f32,
      _pad0: f32,
      _pad1: f32,
      _pad2: f32,
    };
    @group(0) @binding(28) var<uniform> perspective: PerspectiveParams;

    const PERSPECTIVE_KEYSTONE_MAX: f32 = 0.7;
    const PERSPECTIVE_ASPECT_MAX: f32 = 0.5;

    // Port of perspective_warp_coord -- see that Rust function's own
    // doc comment for the stage-by-stage derivation (inverse scale,
    // inverse rotate, inverse aspect stretch, inverse keystone divide).
    fn perspectiveWarpCoord(xd: f32, yd: f32) -> vec2<f32> {
      let scaleAmt = max(perspective.scale / 100.0, 0.01);
      var x = xd / scaleAmt;
      var y = yd / scaleAmt;

      if (perspective.rotate != 0.0) {
        let rad = -radians(perspective.rotate);
        let s = sin(rad);
        let c = cos(rad);
        let rx = x * c - y * s;
        let ry = x * s + y * c;
        x = rx;
        y = ry;
      }

      if (perspective.aspect != 0.0) {
        let stretch = max(1.0 + (perspective.aspect / 100.0) * PERSPECTIVE_ASPECT_MAX, 0.01);
        x = x / stretch;
      }

      let a = (perspective.horizontal / 100.0) * PERSPECTIVE_KEYSTONE_MAX;
      let b = (perspective.vertical / 100.0) * PERSPECTIVE_KEYSTONE_MAX;
      if (a != 0.0 || b != 0.0) {
        let denom = 1.0 + a * x + b * y;
        if (abs(denom) > 0.001) {
          x = x / denom;
          y = y / denom;
        }
      }

      return vec2<f32>(x, y);
    }

    @fragment
    fn fs_perspective(in: VertexOut) -> @location(0) vec4<f32> {
      let dims = vec2<f32>(textureDimensions(srcTexture));
      let cx = dims.x / 2.0;
      let cy = dims.y / 2.0;
      let normScale = max(length(vec2<f32>(cx, cy)), 1.0);

      let px = in.uv.x * dims.x;
      let py = in.uv.y * dims.y;
      let nx = (px - cx) / normScale;
      let ny = (py - cy) / normScale;

      let s = perspectiveWarpCoord(nx, ny);
      let sx = s.x * normScale + cx;
      let sy = s.y * normScale + cy;

      let rgb = textureSample(srcTexture, srcSampler, vec2<f32>(sx, sy) / dims).rgb;
      return vec4<f32>(rgb, 1.0);
    }

`;
