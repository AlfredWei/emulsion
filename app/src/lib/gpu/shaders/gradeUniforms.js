// Grading uniforms (tone curve LUT, HSL, split toning, vignette, grain) and the grain noise helpers.
// Part of the WGSL source, split out of DevelopCanvas.svelte; see index.js for the
// concatenation order, which must not change.
export const gradeUniforms = `    // Tone curve LUT (M3): 256 f32 samples packed into 64 vec4s, NOT
    // array<f32,256> -- WGSL's uniform-address-space array stride must be
    // a multiple of 16 bytes (the Mask struct's own comment above
    // documents hitting this exact footgun), and this packing lets a
    // plain contiguous Float32Array(256) upload directly with no manual
    // padding on the JS side. Device-scoped, not per-image -- see
    // curveLutBuffer's own declaration in the script.
    @group(0) @binding(5) var<uniform> curveLut: array<vec4<f32>, 64>;
    // HSL / Color Mixer (M3): one vec4 per band -- x=hue shift (degrees,
    // -100..100), y=saturation delta (percent, -100..100), z=luminance
    // delta (percent, -100..100), w=unused padding, trivially vec4-
    // aligned per the same footgun documented above. Band order is fixed
    // and MUST match develop.js's HSL_BAND_NAMES/HSL_BAND_CENTERS_DEG and
    // develop_engine.rs's HSL_BAND_NAMES -- no band-name string is ever
    // uploaded, only positional order. Device-scoped, not per-image.
    @group(0) @binding(6) var<uniform> hslBands: array<vec4<f32>, 8>;
    // Split Toning (M3): a small, fixed, NAMED bag of scalars -- mirrors
    // Adjustments' own struct style rather than HSL's/curveLut's vec4-
    // array shape, since there's no natural per-element repetition in 5
    // named fields the way there is for 8 bands or 256 samples. Padded to
    // a full vec4 multiple (32 bytes) per the same footgun documented
    // above (hit three times now: Mask, curveLut, hslBands).
    struct SplitToning {
      shadow_hue: f32,           // 0..360, absolute hue-wheel position
      shadow_saturation: f32,    // 0..100
      highlight_hue: f32,        // 0..360
      highlight_saturation: f32, // 0..100
      balance: f32,              // -100..100
      _pad0: f32,
      _pad1: f32,
      _pad2: f32,
    };
    @group(0) @binding(7) var<uniform> splitToning: SplitToning;

    // Vignette (M3): a flat 3-field struct, same reasoning SplitToning's
    // own comment gives for not using curveLut/hslBands' vec4-array shape.
    // Padded to a full vec4 (16 bytes) per the same footgun documented
    // above.
    struct Vignette {
      amount: f32,   // -100..100, negative darkens, positive lightens
      midpoint: f32, // 0..100, normalized radius where falloff begins
      feather: f32,  // 0..100, width of the falloff transition
      _pad0: f32,
    };
    @group(0) @binding(15) var<uniform> vignette: Vignette;

    // Grain (M3): same flat-struct, own-buffer treatment as Vignette,
    // same reasoning.
    struct Grain {
      amount: f32,    // 0..100
      size: f32,      // 0..100, maps to lattice cell width in pixels
      roughness: f32, // 0..100, blends smooth<->blocky noise
      _pad0: f32,
    };
    @group(0) @binding(16) var<uniform> grain: Grain;

    // A direct WGSL port of develop_engine.rs's own grain_hash/
    // grain_value_noise/grain_delta -- see that module's doc comment on
    // grain_delta for the full parameter reasoning (size's pixel-cell
    // mapping, roughness's smooth/blocky blend, amount's additive-delta
    // shape). Not bit-exact with the Rust twin (different underlying sin
    // implementations), same "not byte-identical" parity bar as the rest
    // of this shader.
    const GRAIN_MAX_CELL_PX: f32 = 6.0;
    const GRAIN_STRENGTH: f32 = 0.12;

    fn grainHash(p: vec2<f32>) -> f32 {
      let v = sin(dot(p, vec2<f32>(12.9898, 78.233))) * 43758.5453123;
      return fract(v);
    }

    fn grainValueNoise(coord: vec2<f32>) -> f32 {
      let i = floor(coord);
      let f = fract(coord);
      let a = grainHash(i);
      let b = grainHash(i + vec2<f32>(1.0, 0.0));
      let c = grainHash(i + vec2<f32>(0.0, 1.0));
      let d = grainHash(i + vec2<f32>(1.0, 1.0));
      let u = f * f * (3.0 - 2.0 * f);
      return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
    }

    fn grainDelta(coord: vec2<f32>) -> f32 {
      if (grain.amount == 0.0) {
        return 0.0;
      }
      let cellPx = 1.0 + (grain.size / 100.0) * (GRAIN_MAX_CELL_PX - 1.0);
      let scaled = coord / cellPx;
      let smoothN = grainValueNoise(scaled);
      let roughN = grainHash(floor(scaled));
      let noise = mix(smoothN, roughN, clamp(grain.roughness / 100.0, 0.0, 1.0));
      return (noise * 2.0 - 1.0) * (grain.amount / 100.0) * GRAIN_STRENGTH;
    }

`;
