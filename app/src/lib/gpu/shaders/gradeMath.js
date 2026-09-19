// Per-pixel grading math: exposure/contrast/saturation, tone curve, HSL bands, split toning.
// Part of the WGSL source, split out of DevelopCanvas.svelte; see index.js for the
// concatenation order, which must not change.
export const gradeMath = `    fn smoothstep_val(edge0: f32, edge1: f32, x: f32) -> f32 {
      let t = clamp((x - edge0) / (edge1 - edge0), 0.0, 1.0);
      return t * t * (3.0 - 2.0 * t);
    }

    fn apply_adjustments(rgb: vec3<f32>, exposure_ev: f32, contrast: f32, saturation: f32) -> vec3<f32> {
      var c = rgb * pow(2.0, exposure_ev);
      c = (c - 0.5) * (1.0 + contrast / 100.0) + 0.5;
      let luma = dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
      c = luma + (c - luma) * (1.0 + saturation / 100.0);
      return c;
    }

    fn apply_global_adjustments(
      rgb: vec3<f32>,
      exposure_ev: f32,
      contrast: f32,
      saturation: f32,
      temperature: f32,
      tint: f32,
      highlights: f32,
      shadows: f32,
      whites: f32,
      blacks: f32
    ) -> vec3<f32> {
      var c = rgb;
      let t = temperature / 100.0;
      let tint_norm = tint / 100.0;

      // White Balance
      c.x = c.x * (1.0 + 0.35 * t + 0.15 * tint_norm);
      c.y = c.y * (1.0 - 0.35 * tint_norm);
      c.z = c.z * (1.0 - 0.35 * t + 0.15 * tint_norm);

      // Exposure
      c = c * pow(2.0, exposure_ev);

      // Contrast
      c = (c - 0.5) * (1.0 + contrast / 100.0) + 0.5;

      // Parametric Tone
      let luma_val = dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
      let w_h = smoothstep_val(0.5, 1.0, luma_val);
      let delta_h = (highlights / 100.0) * w_h * (1.0 - luma_val) * 0.6;

      let w_s = smoothstep_val(0.5, 0.0, luma_val);
      let delta_s = (shadows / 100.0) * w_s * luma_val * 0.6;

      let w_w = smoothstep_val(0.7, 1.0, luma_val);
      let delta_w = (whites / 100.0) * w_w * 0.35;

      let w_b = smoothstep_val(0.3, 0.0, luma_val);
      let delta_b = (blacks / 100.0) * w_b * 0.35;

      c = c + vec3<f32>(delta_h + delta_s + delta_w + delta_b);

      // Saturation
      let luma = dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
      c = luma + (c - luma) * (1.0 + saturation / 100.0);
      return c;
    }

    // Same LUT-plus-linear-interpolation formula as develop.js's
    // sampleCurveLut and develop_engine.rs's sample_lut -- all three sides
    // must agree exactly, since the LUT itself (not an independent exact
    // spline evaluation) is what parity is built on here.
    fn sampleCurveLut(v: f32) -> f32 {
      let idx = clamp(v, 0.0, 1.0) * 255.0;
      let i0 = i32(floor(idx));
      let i1 = min(i0 + 1, 255);
      let frac = idx - f32(i0);
      let v0 = curveLut[i0 / 4][i0 % 4];
      let v1 = curveLut[i1 / 4][i1 % 4];
      return mix(v0, v1, frac);
    }

    const HSL_BAND_CENTERS = array<f32, 8>(0.0, 45.0, 90.0, 135.0, 180.0, 225.0, 270.0, 315.0);

    // WGSL's modulo operator is truncated remainder (sign follows the
    // dividend), NOT the always-non-negative floor-mod this HSL math needs
    // for correct hue wraparound at 0/360 -- this is the standard
    // floor-mod construction (a minus b times floor(a/b)), matching
    // Rust's f32::rem_euclid exactly, which develop_engine.rs's twin of
    // this code uses throughout.
    fn rem_euclid(a: f32, b: f32) -> f32 {
      return a - b * floor(a / b);
    }

    // Standard HSL, not this shader's own perceptual luma (used elsewhere
    // for global Saturation) -- see develop_engine.rs's rgb_to_hsl doc
    // comment for why the two must not be conflated. Returns
    // (hue_degrees 0..360, saturation 0..1, lightness 0..1).
    fn rgbToHsl(rgb: vec3<f32>) -> vec3<f32> {
      let mx = max(rgb.r, max(rgb.g, rgb.b));
      let mn = min(rgb.r, min(rgb.g, rgb.b));
      let delta = mx - mn;
      let l = (mx + mn) / 2.0;
      if (delta == 0.0) {
        return vec3<f32>(0.0, 0.0, l);
      }
      let s = delta / (1.0 - abs(2.0 * l - 1.0));
      var h: f32;
      if (mx == rgb.r) {
        h = 60.0 * rem_euclid((rgb.g - rgb.b) / delta, 6.0);
      } else if (mx == rgb.g) {
        h = 60.0 * (((rgb.b - rgb.r) / delta) + 2.0);
      } else {
        h = 60.0 * (((rgb.r - rgb.g) / delta) + 4.0);
      }
      h = rem_euclid(h, 360.0);
      return vec3<f32>(h, s, l);
    }

    fn hslToRgb(h: f32, s: f32, l: f32) -> vec3<f32> {
      let c = (1.0 - abs(2.0 * l - 1.0)) * s;
      let x = c * (1.0 - abs(rem_euclid(h / 60.0, 2.0) - 1.0));
      let m = l - c / 2.0;
      var rgb1: vec3<f32>;
      if (h < 60.0) { rgb1 = vec3<f32>(c, x, 0.0); }
      else if (h < 120.0) { rgb1 = vec3<f32>(x, c, 0.0); }
      else if (h < 180.0) { rgb1 = vec3<f32>(0.0, c, x); }
      else if (h < 240.0) { rgb1 = vec3<f32>(0.0, x, c); }
      else if (h < 300.0) { rgb1 = vec3<f32>(x, 0.0, c); }
      else { rgb1 = vec3<f32>(c, 0.0, x); }
      return rgb1 + vec3<f32>(m, m, m);
    }

    // Raised-cosine blend weight -- see develop_engine.rs's hsl_band_weight
    // doc comment for why this shape (not a triangular ramp) and why band
    // centers exactly 45 degrees apart guarantee at most 2 nonzero weights
    // summing to exactly 1, with no renormalization needed.
    fn hueBandWeight(hueDeg: f32, centerDeg: f32) -> f32 {
      let d = rem_euclid(hueDeg - centerDeg + 180.0, 360.0) - 180.0;
      let dist = abs(d);
      if (dist >= 45.0) {
        return 0.0;
      }
      return 0.5 * (cos(dist / 45.0 * 3.14159265) + 1.0);
    }

    // See develop_engine.rs's apply_hsl_bands doc comment for the full
    // reasoning (combining shift DELTAS not resultant angles, the
    // load-bearing chromaFade suppression in low-saturation regions where
    // hue is numerically unreliable) -- this is a direct WGSL port of the
    // identical formula, no LUT: the whole computation here is cheap,
    // closed-form trig/arithmetic with no interpolation-method ambiguity
    // to diverge on between this and the Rust side.
    fn applyHslBands(rgb: vec3<f32>) -> vec3<f32> {
      let hsl = rgbToHsl(rgb);
      let hPx = hsl.x;
      let sPx = hsl.y;
      let lPx = hsl.z;

      var hueAcc = 0.0;
      var satAcc = 0.0;
      var lumAcc = 0.0;
      for (var i = 0; i < 8; i = i + 1) {
        let w = hueBandWeight(hPx, HSL_BAND_CENTERS[i]);
        let band = hslBands[i];
        hueAcc = hueAcc + w * band.x;
        satAcc = satAcc + w * band.y;
        lumAcc = lumAcc + w * band.z;
      }

      let chromaFade = clamp(sPx / 0.08, 0.0, 1.0);

      let newH = rem_euclid(hPx + hueAcc * chromaFade, 360.0);
      let newS = clamp(sPx * (1.0 + satAcc / 100.0), 0.0, 1.0);

      let lumFrac = (lumAcc / 100.0) * chromaFade;
      var newL: f32;
      if (lumFrac >= 0.0) {
        newL = lPx + (1.0 - lPx) * lumFrac;
      } else {
        newL = lPx + lPx * lumFrac;
      }

      return hslToRgb(newH, newS, newL);
    }

    // Split Toning (M3): a direct WGSL port of develop_engine.rs's
    // apply_split_toning -- see that function's doc comment for the full
    // reasoning (single simultaneous 3-way blend, not two sequential
    // ones -- a real bug a design review caught before this was written;
    // sequential blending is order-dependent whenever both zone weights
    // are nonzero, which given this smoothstep transition is true for
    // nearly every pixel). Reuses rgbToHsl/hslToRgb as-is, no new
    // color-space math. Unlike applyHslBands, this never reads a pixel's
    // own hue or saturation -- only lightness -- so it needs no
    // chromaFade-style near-gray suppression (there's no ratio-of-near-
    // equal-channels term here to be numerically unstable).
    fn splitToneHighlightWeight(l: f32, balance: f32) -> f32 {
      let t = clamp(l + balance / 200.0, 0.0, 1.0);
      return t * t * (3.0 - 2.0 * t); // smoothstep(0,1,t)
    }

    fn applySplitToning(rgb: vec3<f32>) -> vec3<f32> {
      let lPx = rgbToHsl(rgb).z;
      let wHi = splitToneHighlightWeight(lPx, splitToning.balance);
      let wSh = 1.0 - wHi;
      let aSh = wSh * (splitToning.shadow_saturation / 100.0);
      let aHi = wHi * (splitToning.highlight_saturation / 100.0);
      let tintSh = hslToRgb(splitToning.shadow_hue, 1.0, lPx);
      let tintHi = hslToRgb(splitToning.highlight_hue, 1.0, lPx);
      return rgb * (1.0 - aSh - aHi) + tintSh * aSh + tintHi * aHi;
    }

`;
