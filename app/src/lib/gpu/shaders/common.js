// Shared vertex stage, uniform structs/bindings, and the untouched-original passthrough.
// Part of the WGSL source, split out of DevelopCanvas.svelte; see index.js for the
// concatenation order, which must not change.
export const common = `    struct VertexOut {
      @builtin(position) position: vec4<f32>,
      @location(0) uv: vec2<f32>,
    };

    @vertex
    fn vs_main(@builtin(vertex_index) i: u32) -> VertexOut {
      var pos = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 3.0, -1.0),
        vec2<f32>(-1.0,  3.0),
      );
      var out: VertexOut;
      out.position = vec4<f32>(pos[i], 0.0, 1.0);
      out.uv = vec2<f32>((pos[i].x + 1.0) * 0.5, (1.0 - pos[i].y) * 0.5);
      return out;
    }

    // Padded to a full 4x vec4 (64 bytes) -- mirrors the Mask struct's
    // "pack into vec4 multiples" discipline below.
    struct Adjustments {
      exposure_ev: f32,
      contrast: f32,
      saturation: f32,
      mask_count: f32,
      selected_mask_index: f32,
      dehaze_amount: f32,
      texture_amount: f32,
      clarity_amount: f32,
      temperature: f32,
      tint: f32,
      highlights: f32,
      shadows: f32,
      whites: f32,
      blacks: f32,
      pad0: f32,
      pad1: f32,
    };

    // Packed entirely into vec4-multiples (48 bytes/mask) to sidestep
    // WGSL's vec2/vec3-in-array uniform alignment footguns -- array stride
    // in the uniform address space must be a multiple of 16 bytes, and an
    // all-vec4 struct is trivially aligned with no implicit padding.
    // params.w holds a texture-array layer index for BOTH kind=2 (brush)
    // and kind=5 (spot, M4 Slice 2: spot dabs rasterize into the SAME
    // shared texture array brush masks already use, since both are now
    // dab-stroke masks -- see syncMaskRasterization's own doc comment).
    // adjustments is entirely unused for spot (no exposure/contrast/
    // saturation channel -- it copies pixel CONTENT, see Mask::local_color
    // in develop_engine.rs) except adjustments.x, repurposed to carry
    // mode (0=clone, 1=heal). kind=6 (red eye, M4) reuses radial's own
    // start_end/params.x geometry layout verbatim (same click-drag ellipse)
    // but repurposes the two slots radial uses for invert/padding instead:
    // params.y becomes pupilSize (0-100, red eye has no invert concept)
    // and adjustments.w becomes darken (0-100, otherwise unused padding on
    // every other kind) -- see fs_mask's own red-eye branch for the
    // formula both slots feed into.
    struct Mask {
      start_end: vec4<f32>,   // xy = start, zw = end (normalized image space); luminance range: x=rangeMin, y=rangeMax (both 0-100); color range: xyz=refColor (0-1), w=range (0-100); spot: xy=sourceOffset(dx,dy), zw=dabs centroid (for heal-ring sampling only); red eye: xy=center, zw=(radiusX,radiusY), same as radial
      params: vec4<f32>,      // x = feather 0-100 (unused for brush), y = invert 0/1 (unused for brush/spot; spot repurposes this for dabs' average radius, also for heal-ring sampling; red eye repurposes this for pupilSize 0-100), z = kind (0=linear, 1=radial, 2=brush, 3=luminance range, 4=color range, 5=spot, 6=red eye), w = texture-array layer (brush AND spot)
      adjustments: vec4<f32>, // x = exposure_ev (spot: mode, 0=clone/1=heal), y = contrast (unused for spot/red eye), z = saturation (unused for spot/red eye), w unused (red eye repurposes this for darken 0-100)
    };
    const MAX_MASKS = 8;

    @group(0) @binding(0) var srcSampler: sampler;
    @group(0) @binding(1) var srcTexture: texture_2d<f32>;

    // M4 Slice 2: before/after preview -- a dedicated pass, entirely
    // separate from fs_grade/fs_premask/fs_mask's whole global-grade +
    // local-mask pipeline, that just samples the RAW decoded source
    // straight to the swapchain with zero edits applied. Deliberately its
    // own pass rather than a branch inside fs_mask: fs_mask's own inferred
    // bind-group layout only includes preMaskTex/masks/etc (whatever IT
    // references), not srcTexture, so a "skip everything" branch there
    // would need its own separate binding anyway -- a whole separate
    // pipeline+bind-group (see originalPipeline/originalBindGroup) is no
    // more code and keeps fs_mask's own already-complex body untouched.
    @fragment
    fn fs_original(in: VertexOut) -> @location(0) vec4<f32> {
      return vec4<f32>(textureSampleLevel(srcTexture, srcSampler, in.uv, 0.0).rgb, 1.0);
    }

    @group(0) @binding(2) var<uniform> adj: Adjustments;
    @group(0) @binding(3) var<uniform> masks: array<Mask, MAX_MASKS>;
    // Brush masks rasterize CPU-side (OffscreenCanvas, luminance-as-weight)
    // rather than computing an analytic formula here -- one array layer per
    // active brush mask. Sampled via textureSampleLevel (not textureSample)
    // deliberately: this call sits inside a per-mask branch on m.params.z,
    // and textureSampleLevel has no implicit-derivative uniformity
    // restriction to worry about, unlike textureSample.
    @group(0) @binding(4) var brushMasks: texture_2d_array<f32>;
`;
