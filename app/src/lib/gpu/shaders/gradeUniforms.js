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

    // Vignette (M3): a flat 4-field struct, same reasoning SplitToning's
    // own comment gives for not using curveLut/hslBands' vec4-array shape.
    // Padded to a full vec4 (16 bytes) per the same footgun documented
    // above -- roundness (RFC-0016) reuses what was _pad0, so this is
    // still exactly one vec4, no layout change.
    struct Vignette {
      amount: f32,    // -100..100, negative darkens, positive lightens
      midpoint: f32,  // 0..100, normalized radius where falloff begins
      feather: f32,   // 0..100, width of the falloff transition
      roundness: f32, // -100..100, 0=today's ellipse; see RFC-0016
    };
    @group(0) @binding(15) var<uniform> vignette: Vignette;

    // RFC-0016: the exponent Vignette's "more rectangular" branch (see
    // fs_premask below) blends toward as roundness approaches -100.
    // Mirrors VIGNETTE_ROUNDNESS_MAX_P in develop_engine/effects.rs --
    // keep both in sync. No reference signal to tune this against (it's
    // a shape parameter, not a filter accuracy constant), so this is a
    // reasoned choice, not a verified match to Adobe's own algorithm.
    const VIGNETTE_ROUNDNESS_MAX_P: f32 = 5.0;

    // Grain (M3): same flat-struct, own-buffer treatment as Vignette,
    // same reasoning.
    struct Grain {
      amount: f32,    // 0..100
      size: f32,      // 0..100, frame-relative cell width (RFC-0022 §3.4)
      roughness: f32, // 0..100, even<->clumpy particles (RFC-0020)
      _pad0: f32,
    };
    @group(0) @binding(16) var<uniform> grain: Grain;

    // A direct WGSL port of develop_engine/effects.rs's lowbias32/
    // particle_hash/grain_particle_noise/grain_delta (RFC-0020) -- see that
    // module's doc comments for the full parameter reasoning. The hash is
    // pure u32 xor/shift/wrapping-multiply, which WGSL and Rust define
    // bit-for-bit identically (the old sin-based hash was neither
    // bit-exact nor even random on the GPU: sin's error x 43758 before
    // fract). The noise itself uses only f32 + - * / plus one
    // normalization (one sqrt), so it agrees with the Rust twin to ~1e-6, and a
    // one-ulp floor() disagreement cannot change the pattern (compact
    // kernel support inside the 3x3 block).
    // RFC-0022 §3.4: frame-relative Size (micrometres of a 36 mm frame ->
    // pixels via the image's long edge) and the pixel-footprint amplitude
    // compensation r(c, rho). Constants mirror effects.rs.
    const GRAIN_FRAME_UM: f32 = 36000.0;
    const GRAIN_CELL_UM_MIN: f32 = 6.0;
    const GRAIN_CELL_UM_SPAN: f32 = 30.0;
    const GRAIN_FOOTPRINT_K2_BASE: f32 = 0.6;
    const GRAIN_FOOTPRINT_K2_PER_ROUGHNESS: f32 = 0.3;
    const GRAIN_SIGMA: f32 = 0.0548;
    const GRAIN_PARTICLES_PER_CELL: u32 = 2u;
    const GRAIN_PI: f32 = 3.14159265358979;

    fn lowbias32(xin: u32) -> u32 {
      var x = xin;
      x ^= x >> 16u;
      x *= 0x7feb352du;
      x ^= x >> 15u;
      x *= 0x846ca68bu;
      x ^= x >> 16u;
      return x;
    }

    fn particleHash(ix: i32, iy: i32, s: u32) -> u32 {
      return lowbias32(bitcast<u32>(ix) ^ lowbias32(bitcast<u32>(iy) ^ 0x9E3779B9u ^ s));
    }

    fn grainParticleNorm(rho: f32) -> f32 {
      let ew2 = 1.0 - rho + rho * rho / 3.0;
      let a = 0.5 * rho;
      let er2 = 1.0 - a + a * a / 3.0;
      return 1.0 / sqrt(f32(GRAIN_PARTICLES_PER_CELL) * ew2 * 0.2 * GRAIN_PI * er2);
    }

    // p in CELL units, rho in [0,1]; unit variance, zero mean.
    fn grainParticleNoise(p: vec2<f32>, rhoIn: f32, seed: u32) -> f32 {
      let rho = clamp(rhoIn, 0.0, 1.0);
      let cx = i32(floor(p.x));
      let cy = i32(floor(p.y));
      var sum = 0.0;
      for (var a = -1; a <= 1; a++) {
        for (var b = -1; b <= 1; b++) {
          for (var k = 0u; k < GRAIN_PARTICLES_PER_CELL; k++) {
            let h = particleHash(cx + a, cy + b, seed + k * 7919u);
            let jx = f32(h & 0xFFu) / 256.0;
            let jy = f32((h >> 8u) & 0xFFu) / 256.0;
            let wbits = (h >> 16u) & 0xFFu;
            let sgn = select(-1.0, 1.0, (wbits & 0x80u) != 0u);
            let um = f32(wbits & 0x7Fu) / 128.0;
            let ur = f32((h >> 24u) & 0xFFu) / 256.0;
            let w = sgn * (1.0 - rho * um);
            let r = 1.0 - 0.5 * rho * ur;
            let dx = p.x - (f32(cx + a) + jx);
            let dy = p.y - (f32(cy + b) + jy);
            let d2 = (dx * dx + dy * dy) / (r * r);
            if (d2 < 1.0) {
              let t = 1.0 - d2;
              sum += w * t * t;
            }
          }
        }
      }
      return sum * grainParticleNorm(rho);
    }

    // longEdge: the whole uncropped frame's long edge in pixels (the grain
    // pass runs on the uncropped source texture), per RFC-0022 §3.4.
    fn grainDelta(coord: vec2<f32>, longEdge: f32) -> f32 {
      if (grain.amount == 0.0) {
        return 0.0;
      }
      let rho = grain.roughness / 100.0;
      let cellPx = (GRAIN_CELL_UM_MIN + (grain.size / 100.0) * GRAIN_CELL_UM_SPAN) * longEdge / GRAIN_FRAME_UM;
      let footprint = cellPx / sqrt(cellPx * cellPx + GRAIN_FOOTPRINT_K2_BASE + GRAIN_FOOTPRINT_K2_PER_ROUGHNESS * rho);
      let noise = grainParticleNoise(coord / cellPx, rho, 0u);
      return noise * (grain.amount / 100.0) * GRAIN_SIGMA * footprint;
    }

`;
