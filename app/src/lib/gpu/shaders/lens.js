// Lens correction (distortion, chromatic aberration) params and the fs_lens_correct pass.
// Part of the WGSL source, split out of DevelopCanvas.svelte; see index.js for the
// concatenation order, which must not change.
export const lens = `    // Lens Corrections (M3): a direct WGSL port of develop_engine.rs's own
    // hand-verified formulas (Newton-iteration distortion/TCA undistort,
    // radial vignetting gain) -- see that module's own extensive header
    // comment for the Profile/Manual split, ordering, and the poly3
    // Newton-solve algebra bug its own round-trip tests caught (fixed
    // identically here). Runs as its OWN pass, before fs_grade, writing
    // into lensCorrectedTex -- a genuine resample (each output channel can
    // sample a different source location), so it can't be folded into
    // fs_grade's own straight-through per-pixel body.
    //
    // f32 throughout (WGSL has no f64) -- develop_engine.rs's own Newton
    // solves use f64 internally for precision; this GPU port accepts the
    // resulting small float error as an acceptable interactive-preview
    // tradeoff, same as every other WGSL pass in this file already does
    // relative to its Rust twin.
    //
    // Non-convergence handling deliberately does NOT mirror
    // lens_undist_poly3's own NaN-then-check-in-caller shape: WGSL has no
    // clean, portable NaN-detection builtin, and every OTHER undistort
    // function (poly5/ptlens/tca) already "just returns the unchanged
    // input" directly on non-convergence. Poly3 does the same here --
    // same OBSERVABLE behavior as the Rust twin's NaN sentinel + fallback,
    // without needing NaN plumbing at all.
    //
    // "Has a profile" is NOT a separate uniform flag: a missing profile,
    // or a missing distortion/tca/vignetting sub-block within one, is
    // uploaded as model=0 / all-zero coefficients by
    // buildLensCorrectionUniformData (develop.js) -- model 0 already means
    // "skip, return input unchanged" for distortion/TCA, and all-zero
    // vignetting coefficients already make the gain formula evaluate to
    // 1.0 (a no-op multiply). One fewer flag to keep in sync.
    struct LensCorrectionParams {
      profile_enabled: f32,   // 0/1
      distortion_amount: f32, // 0..100
      vignette_amount: f32,   // 0..100
      ca_amount: f32,         // 0..100
      manual_distortion: f32, // -100..100
      manual_ca: f32,         // -100..100
      crop_factor: f32,
      real_focal: f32,
      lens_center_x: f32,
      lens_center_y: f32,
      distortion_model: f32,  // 0=none, 1=poly3, 2=poly5, 3=ptlens
      distortion_c0: f32,     // poly3: k1 -- poly5: k1 -- ptlens: a
      distortion_c1: f32,     // poly5: k2 -- ptlens: b
      distortion_c2: f32,     // ptlens: c
      tca_model: f32,         // 0=none, 1=linear, 2=poly3
      tca_red0: f32,          // linear: kr -- poly3: v
      tca_red1: f32,          // poly3: c
      tca_red2: f32,          // poly3: b
      tca_blue0: f32,         // linear: kb -- poly3: v
      tca_blue1: f32,         // poly3: c
      tca_blue2: f32,         // poly3: b
      vignette_k1: f32,
      vignette_k2: f32,
      vignette_k3: f32,
    };
    @group(0) @binding(25) var<uniform> lensCorrection: LensCorrectionParams;

    const LENS_MANUAL_DISTORTION_MAX_K1: f32 = 0.35;
    const LENS_MANUAL_CA_MAX_SHIFT: f32 = 0.02;

    // Port of 'lens_undist_poly3' -- 'Rd = Ru + k1*Ru^3' inverse (Newton,
    // <=6 steps). Returns the unchanged input on non-convergence/negative
    // root (see this section's own header comment on why, unlike the Rust
    // twin, this isn't NaN-then-checked-by-the-caller).
    fn lensUndistPoly3(x: f32, y: f32, k1: f32) -> vec2<f32> {
      if (k1 == 0.0) { return vec2<f32>(x, y); }
      let invK1 = 1.0 / k1;
      let rd = sqrt(x * x + y * y);
      if (rd == 0.0) { return vec2<f32>(x, y); }
      let rdDivK1 = rd * invK1;
      var ru = rd;
      var i = 0;
      loop {
        let fru = ru * ru * ru + ru * invK1 - rdDivK1;
        if (abs(fru) < 0.00001) { break; }
        if (i > 5) { return vec2<f32>(x, y); }
        ru = ru - fru / (3.0 * ru * ru + invK1);
        i = i + 1;
      }
      if (ru < 0.0) { return vec2<f32>(x, y); }
      let scale = ru / rd;
      return vec2<f32>(x * scale, y * scale);
    }

    // Port of 'lens_undist_poly5' -- 'Rd = Ru*(1 + k1*Ru^2 + k2*Ru^4)' inverse.
    fn lensUndistPoly5(x: f32, y: f32, k1: f32, k2: f32) -> vec2<f32> {
      let rd = sqrt(x * x + y * y);
      if (rd == 0.0) { return vec2<f32>(x, y); }
      var ru = rd;
      var i = 0;
      var converged = false;
      loop {
        let ru2 = ru * ru;
        let fru = ru * (1.0 + k1 * ru2 + k2 * ru2 * ru2) - rd;
        if (abs(fru) < 0.00001) { converged = true; break; }
        if (i > 5) { break; }
        ru = ru - fru / (1.0 + 3.0 * k1 * ru2 + 5.0 * k2 * ru2 * ru2);
        i = i + 1;
      }
      if (!converged || ru < 0.0) { return vec2<f32>(x, y); }
      let scale = ru / rd;
      return vec2<f32>(x * scale, y * scale);
    }

    // Port of 'lens_undist_ptlens' -- 'Rd = Ru*(a*Ru^3 + b*Ru^2 + c*Ru + 1)' inverse.
    fn lensUndistPtlens(x: f32, y: f32, a: f32, b: f32, c: f32) -> vec2<f32> {
      let rd = sqrt(x * x + y * y);
      if (rd == 0.0) { return vec2<f32>(x, y); }
      var ru = rd;
      var i = 0;
      var converged = false;
      loop {
        let fru = ru * (a * ru * ru * ru + b * ru * ru + c * ru + 1.0) - rd;
        if (abs(fru) < 0.00001) { converged = true; break; }
        if (i > 5) { break; }
        ru = ru - fru / (4.0 * a * ru * ru * ru + 3.0 * b * ru * ru + 2.0 * c * ru + 1.0);
        i = i + 1;
      }
      if (!converged || ru < 0.0) { return vec2<f32>(x, y); }
      let scale = ru / rd;
      return vec2<f32>(x * scale, y * scale);
    }

    // Port of 'lens_undist_tca_poly3' -- 'Rd = Ru*(v + c*Ru + b*Ru^2)'
    // inverse, single channel.
    fn lensUndistTcaPoly3(x: f32, y: f32, v: f32, c: f32, b: f32) -> vec2<f32> {
      let rd = sqrt(x * x + y * y);
      if (rd == 0.0) { return vec2<f32>(x, y); }
      var ru = rd;
      var i = 0;
      var converged = false;
      loop {
        let ru2 = ru * ru;
        let fru = b * ru2 * ru + c * ru2 + v * ru - rd;
        if (abs(fru) < 0.00001) { converged = true; break; }
        if (i > 5) { break; }
        ru = ru - fru / (3.0 * b * ru2 + 2.0 * c * ru + v);
        i = i + 1;
      }
      if (!converged || ru <= 0.0) { return vec2<f32>(x, y); }
      let scale = ru / rd;
      return vec2<f32>(x * scale, y * scale);
    }

    // Port of 'LensNorm' -- pixel <-> normalized lens-space coordinate
    // mapping. 'norm.x'=scale, 'norm.y'=unscale, 'norm.zw'=center.
    fn lensNormForProfile(texWidth: f32, texHeight: f32) -> vec4<f32> {
      let w = select(1.0, texWidth - 1.0, texWidth >= 2.0);
      let h = select(1.0, texHeight - 1.0, texHeight >= 2.0);
      let scale = sqrt(36.0 * 36.0 + 24.0 * 24.0) / max(lensCorrection.crop_factor, 0.01)
        / sqrt((w + 1.0) * (w + 1.0) + (h + 1.0) * (h + 1.0))
        / max(lensCorrection.real_focal, 0.01);
      let size = min(w, h);
      let centerX = (w / 2.0 + size / 2.0 * lensCorrection.lens_center_x) * scale;
      let centerY = (h / 2.0 + size / 2.0 * lensCorrection.lens_center_y) * scale;
      return vec4<f32>(scale, 1.0 / scale, centerX, centerY);
    }

    fn lensNormForManual(texWidth: f32, texHeight: f32) -> vec4<f32> {
      let cx = texWidth / 2.0;
      let cy = texHeight / 2.0;
      let halfDiag = max(sqrt(cx * cx + cy * cy), 1.0);
      let scale = 1.0 / halfDiag;
      return vec4<f32>(scale, halfDiag, cx * scale, cy * scale);
    }

    fn lensToNormalized(norm: vec4<f32>, px: f32, py: f32) -> vec2<f32> {
      return vec2<f32>(px * norm.x - norm.z, py * norm.x - norm.w);
    }

    fn lensToPixel(norm: vec4<f32>, nx: f32, ny: f32) -> vec2<f32> {
      return vec2<f32>((nx + norm.z) * norm.y, (ny + norm.w) * norm.y);
    }

    // Port of 'apply_lens_distortion' -- blends the resolved model's
    // correction toward identity by 'amount' (0..1).
    fn lensApplyDistortion(nx: f32, ny: f32, amount: f32) -> vec2<f32> {
      let model = i32(lensCorrection.distortion_model);
      var c = vec2<f32>(nx, ny);
      if (model == 1) {
        c = lensUndistPoly3(nx, ny, lensCorrection.distortion_c0);
      } else if (model == 2) {
        c = lensUndistPoly5(nx, ny, lensCorrection.distortion_c0, lensCorrection.distortion_c1);
      } else if (model == 3) {
        c = lensUndistPtlens(nx, ny, lensCorrection.distortion_c0, lensCorrection.distortion_c1, lensCorrection.distortion_c2);
      } else {
        return vec2<f32>(nx, ny);
      }
      return mix(vec2<f32>(nx, ny), c, amount);
    }

    // Port of 'apply_lens_tca' -- 'channel' is 0=Red, 1=Green, 2=Blue;
    // Green is always a no-op (the reference every other channel corrects
    // relative to).
    fn lensApplyTca(nx: f32, ny: f32, channel: i32, amount: f32) -> vec2<f32> {
      if (channel == 1) { return vec2<f32>(nx, ny); }
      let model = i32(lensCorrection.tca_model);
      var c = vec2<f32>(nx, ny);
      if (model == 1) {
        let k = select(lensCorrection.tca_blue0, lensCorrection.tca_red0, channel == 0);
        c = vec2<f32>(nx * k, ny * k);
      } else if (model == 2) {
        if (channel == 0) {
          c = lensUndistTcaPoly3(nx, ny, lensCorrection.tca_red0, lensCorrection.tca_red1, lensCorrection.tca_red2);
        } else {
          c = lensUndistTcaPoly3(nx, ny, lensCorrection.tca_blue0, lensCorrection.tca_blue1, lensCorrection.tca_blue2);
        }
      } else {
        return vec2<f32>(nx, ny);
      }
      return mix(vec2<f32>(nx, ny), c, amount);
    }

    // Port of 'lens_correct_coord' -- where a given output pixel's
    // 'channel' samples from in the source image: profile distortion,
    // then manual distortion, then profile TCA, then manual CA, each only
    // for the channels/amounts actually active. 'channel' is 0=Red,
    // 1=Green, 2=Blue.
    fn lensCorrectCoord(px: f32, py: f32, channel: i32, profileNorm: vec4<f32>, manualNorm: vec4<f32>) -> vec2<f32> {
      var x = px;
      var y = py;

      if (lensCorrection.profile_enabled > 0.5 && lensCorrection.distortion_amount > 0.0) {
        let n = lensToNormalized(profileNorm, x, y);
        let c = lensApplyDistortion(n.x, n.y, lensCorrection.distortion_amount / 100.0);
        let p = lensToPixel(profileNorm, c.x, c.y);
        x = p.x;
        y = p.y;
      }

      if (lensCorrection.manual_distortion != 0.0) {
        let k1 = (lensCorrection.manual_distortion / 100.0) * LENS_MANUAL_DISTORTION_MAX_K1;
        let n = lensToNormalized(manualNorm, x, y);
        let c = lensUndistPoly3(n.x, n.y, k1);
        let p = lensToPixel(manualNorm, c.x, c.y);
        x = p.x;
        y = p.y;
      }

      if (lensCorrection.profile_enabled > 0.5 && lensCorrection.ca_amount > 0.0) {
        let n = lensToNormalized(profileNorm, x, y);
        let c = lensApplyTca(n.x, n.y, channel, lensCorrection.ca_amount / 100.0);
        let p = lensToPixel(profileNorm, c.x, c.y);
        x = p.x;
        y = p.y;
      }

      if (lensCorrection.manual_ca != 0.0 && channel != 1) {
        let shift = (lensCorrection.manual_ca / 100.0) * LENS_MANUAL_CA_MAX_SHIFT;
        let k = select(1.0 - shift, 1.0 + shift, channel == 0);
        let n = lensToNormalized(manualNorm, x, y);
        let p = lensToPixel(manualNorm, n.x * k, n.y * k);
        x = p.x;
        y = p.y;
      }

      return vec2<f32>(x, y);
    }

    // Port of 'apply_lens_correction''s geometry-resample loop, one output
    // pixel per invocation -- srcSampler's clamp-to-edge addressing
    // (initGpu's own sampler) matches 'sample_bilinear''s edge behavior
    // with no extra clamping needed here. Vignetting correction (a pure
    // per-pixel gain, not a resample) runs after, on the already
    // geometry-corrected RGB.
    @fragment
    fn fs_lens_correct(in: VertexOut) -> @location(0) vec4<f32> {
      let dims = vec2<f32>(textureDimensions(srcTexture));
      let px = in.uv.x * dims.x;
      let py = in.uv.y * dims.y;

      let profileNorm = lensNormForProfile(dims.x, dims.y);
      let manualNorm = lensNormForManual(dims.x, dims.y);

      let rCoord = lensCorrectCoord(px, py, 0, profileNorm, manualNorm);
      let gCoord = lensCorrectCoord(px, py, 1, profileNorm, manualNorm);
      let bCoord = lensCorrectCoord(px, py, 2, profileNorm, manualNorm);

      let r = textureSample(srcTexture, srcSampler, rCoord / dims).r;
      let g = textureSample(srcTexture, srcSampler, gCoord / dims).g;
      let b = textureSample(srcTexture, srcSampler, bCoord / dims).b;
      var rgb = vec3<f32>(r, g, b);

      if (lensCorrection.profile_enabled > 0.5 && lensCorrection.vignette_amount > 0.0) {
        let n = lensToNormalized(profileNorm, px, py);
        let r2 = n.x * n.x + n.y * n.y;
        let r4 = r2 * r2;
        let r6 = r4 * r2;
        let gain = 1.0 + lensCorrection.vignette_k1 * r2 + lensCorrection.vignette_k2 * r4 + lensCorrection.vignette_k3 * r6;
        let mult = mix(1.0, 1.0 / max(gain, 0.01), lensCorrection.vignette_amount / 100.0);
        rgb = rgb * mult;
      }

      return vec4<f32>(rgb, 1.0);
    }

`;
