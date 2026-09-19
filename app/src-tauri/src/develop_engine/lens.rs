use super::*;

// ==================== Lens Corrections (M3) ====================
//
// The `lens_correction` op combines two independent correction sources,
// matching real Lightroom's own Profile/Manual split:
//
// - PROFILE: coefficients baked at match time by `lens_profile::
//   match_profile` (see that module -- this file never touches the
//   `lensfun` crate itself, only the plain f32s it already resolved).
//   Gated on `profile_enabled` AND a `profile` actually being present;
//   `distortion_amount`/`vignette_amount`/`ca_amount` (0-100) each blend
//   between identity and the full correction independently, matching
//   Lightroom's own per-category Amount sliders.
// - MANUAL: `manual_distortion`/`manual_ca` (-100..100), a simple
//   single-coefficient correction available regardless of whether a
//   profile matched or is enabled -- real Lightroom's own Manual tab
//   works the same way. Manual VIGNETTE correction is deliberately NOT a
//   separate control here: it's the exact same radial-gain formula this
//   app's existing creative `vignette` op already exposes (a positive
//   Amount brightens corners) -- adding a second, near-identical vignette
//   slider next to the existing one would just be confusing, not a real
//   missing capability. The Lens Corrections UI points users at the
//   existing Vignette section for this instead of duplicating it.
//
// Applied BEFORE `apply_edit_stack` (grading) and well before `apply_crop`
// (geometry) in both real callers -- matching real Lightroom: the user is
// grading and cropping the CORRECTED image, not the raw lens-distorted
// one. Geometry (distortion + per-channel CA) is a genuine resample --
// each output pixel's RGB channels can each come from a DIFFERENT source
// location -- so, like `apply_crop`'s own rotation, it needs a fresh
// output buffer and `sample_bilinear`, not an in-place per-pixel formula.
// Vignetting correction is a pure per-pixel gain and runs last, in place,
// on the already geometry-corrected image.
//
// The distortion/TCA formulas below are a direct, hand-verified port of
// `lensfun`'s own pure kernels (`mod_coord::{undist_poly3,undist_poly5,
// undist_ptlens}`, `mod_subpix`'s per-channel poly3 inversion) -- not a
// reimplementation from scratch, and not called through the crate's own
// `Modifier` type (which isn't `Send`-friendly to hold across renders and
// would need re-deriving from `Lens`/`Camera` on every frame anyway,
// since it depends on the CURRENT render's width/height, not just the
// matched equipment). Keeping this module's own copy also means its
// hand-derived tests below need no `lensfun::Database` at all -- they
// exercise these formulas directly against synthetic coefficients, robust
// to the bundled lens database ever changing.

/// `Rd = Ru + k1*Ru^3` inverse (Newton, <=6 steps) -- port of
/// `lensfun::mod_coord::undist_poly3`. NaN on non-convergence/negative
/// root (matches upstream's own poly3 contract -- callers must check).
pub(super) fn lens_undist_poly3(x: f32, y: f32, k1: f32) -> (f32, f32) {
    if k1 == 0.0 {
        return (x, y);
    }
    let inv_k1 = 1.0_f64 / k1 as f64;
    let rd = ((x * x + y * y) as f64).sqrt();
    if rd == 0.0 {
        return (x, y);
    }
    let rd_div_k1 = rd * inv_k1;
    let mut ru = rd;
    let mut step = 0;
    loop {
        // Solves `k1*ru^3 + ru - rd = 0` (the forward eq `rd=ru*(1+k1*ru^2)`
        // rearranged), divided through by k1 to match upstream's own
        // `inv_k1_`-scaled form: `fru = ru^3 + ru/k1 - rd/k1`.
        let fru = ru * ru * ru + ru * inv_k1 - rd_div_k1;
        if fru.abs() < 0.00001 {
            break;
        }
        if step > 5 {
            return (f32::NAN, f32::NAN);
        }
        ru -= fru / (3.0 * ru * ru + inv_k1);
        step += 1;
    }
    if ru < 0.0 {
        return (f32::NAN, f32::NAN);
    }
    let scale = (ru / rd) as f32;
    (x * scale, y * scale)
}

/// `Rd = Ru*(1 + k1*Ru^2 + k2*Ru^4)` inverse. Leaves input unchanged on
/// non-convergence (matches upstream `undist_poly5`).
pub(super) fn lens_undist_poly5(x: f32, y: f32, k1: f32, k2: f32) -> (f32, f32) {
    let rd = ((x * x + y * y) as f64).sqrt();
    if rd == 0.0 {
        return (x, y);
    }
    let (k1, k2) = (k1 as f64, k2 as f64);
    let mut ru = rd;
    let mut step = 0;
    let converged = loop {
        let ru2 = ru * ru;
        let fru = ru * (1.0 + k1 * ru2 + k2 * ru2 * ru2) - rd;
        if fru.abs() < 0.00001 {
            break true;
        }
        if step > 5 {
            break false;
        }
        ru -= fru / (1.0 + 3.0 * k1 * ru2 + 5.0 * k2 * ru2 * ru2);
        step += 1;
    };
    if !converged || ru < 0.0 {
        return (x, y);
    }
    let scale = (ru / rd) as f32;
    (x * scale, y * scale)
}

/// `Rd = Ru*(a*Ru^3 + b*Ru^2 + c*Ru + 1)` inverse. Leaves input unchanged
/// on non-convergence (matches upstream `undist_ptlens`).
pub(super) fn lens_undist_ptlens(x: f32, y: f32, a: f32, b: f32, c: f32) -> (f32, f32) {
    let rd = ((x * x + y * y) as f64).sqrt();
    if rd == 0.0 {
        return (x, y);
    }
    let (a, b, c) = (a as f64, b as f64, c as f64);
    let mut ru = rd;
    let mut step = 0;
    let converged = loop {
        let fru = ru * (a * ru * ru * ru + b * ru * ru + c * ru + 1.0) - rd;
        if fru.abs() < 0.00001 {
            break true;
        }
        if step > 5 {
            break false;
        }
        ru -= fru / (4.0 * a * ru * ru * ru + 3.0 * b * ru * ru + 2.0 * c * ru + 1.0);
        step += 1;
    };
    if !converged || ru < 0.0 {
        return (x, y);
    }
    let scale = (ru / rd) as f32;
    (x * scale, y * scale)
}

/// `Rd = Ru*(v + c*Ru + b*Ru^2)` inverse, single channel -- port of
/// `lensfun::mod_subpix`'s private `invert_one_channel`. Leaves input
/// unchanged on non-convergence or a non-positive root.
pub(super) fn lens_undist_tca_poly3(x: f32, y: f32, v: f32, c: f32, b: f32) -> (f32, f32) {
    let rd = ((x * x + y * y) as f64).sqrt();
    if rd == 0.0 {
        return (x, y);
    }
    let (v, c, b) = (v as f64, c as f64, b as f64);
    let mut ru = rd;
    let mut step = 0;
    let converged = loop {
        let ru2 = ru * ru;
        let fru = b * ru2 * ru + c * ru2 + v * ru - rd;
        if fru.abs() < 0.00001 {
            break true;
        }
        if step > 5 {
            break false;
        }
        ru -= fru / (3.0 * b * ru2 + 2.0 * c * ru + v);
        step += 1;
    };
    if !converged || ru <= 0.0 {
        return (x, y);
    }
    let scale = (ru / rd) as f32;
    (x * scale, y * scale)
}

/// Plain-data mirror of `lens_profile::DistortionCoeffs` -- deliberately
/// NOT the same type (this module must not depend on the `lensfun` crate
/// at all, see this section's own header comment), parsed independently
/// from the op's JSON.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum LensDistortion {
    Poly3 { k1: f32 },
    Poly5 { k1: f32, k2: f32 },
    Ptlens { a: f32, b: f32, c: f32 },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum LensTca {
    Linear { kr: f32, kb: f32 },
    Poly3 { red: [f32; 3], blue: [f32; 3] },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct LensVignetting {
    pub(super) k1: f32,
    pub(super) k2: f32,
    pub(super) k3: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(super) struct LensProfileData {
    pub(super) crop_factor: f32,
    pub(super) real_focal: f32,
    pub(super) lens_center_x: f32,
    pub(super) lens_center_y: f32,
    pub(super) distortion: Option<LensDistortion>,
    pub(super) tca: Option<LensTca>,
    pub(super) vignetting: Option<LensVignetting>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LensCorrection {
    pub(super) profile_enabled: bool,
    pub(super) distortion_amount: f32,
    pub(super) vignette_amount: f32,
    pub(super) ca_amount: f32,
    pub(super) manual_distortion: f32,
    pub(super) manual_ca: f32,
    pub(super) profile: Option<LensProfileData>,
}

impl Default for LensCorrection {
    fn default() -> Self {
        LensCorrection {
            profile_enabled: false,
            // Amounts default to full (100), NOT 0 -- a deliberate
            // deviation from every other op in this file's "0 = identity"
            // convention. Once a profile is enabled, real Lightroom's own
            // sliders start at full correction; `profile_enabled` (not
            // the amounts) is what gates whether a profile applies at
            // all, so defaulting amounts to 0 would make "just enabled a
            // profile, haven't touched a slider yet" silently do nothing.
            distortion_amount: 100.0,
            vignette_amount: 100.0,
            ca_amount: 100.0,
            manual_distortion: 0.0,
            manual_ca: 0.0,
            profile: None,
        }
    }
}

/// Manual distortion/CA are a simple, profile-independent correction --
/// unlike profile coefficients (already rescaled into a real optical
/// normalized space by `lens_profile::match_profile`), these constants
/// are hand-picked to look reasonable at slider extremes, the same
/// "calibrated by eye, confirmed empirically" practice this file already
/// uses for e.g. Grain's size/roughness mapping. Confirmed against a real
/// image during this slice's empirical verification.
pub(super) const MANUAL_DISTORTION_MAX_K1: f32 = 0.35;

pub(super) const MANUAL_CA_MAX_SHIFT: f32 = 0.02;

pub(super) fn lens_correction_op(ops: &[serde_json::Value]) -> LensCorrection {
    let op = ops.iter().find(|op| op.get("op").and_then(|v| v.as_str()) == Some("lens_correction"));
    let Some(op) = op else { return LensCorrection::default() };

    let f32_field = |key: &str, default: f32| -> f32 {
        op.get(key).and_then(|v| v.as_f64()).map(|v| v as f32).unwrap_or(default)
    };
    let bool_field = |key: &str, default: bool| -> bool { op.get(key).and_then(|v| v.as_bool()).unwrap_or(default) };

    let profile = op.get("profile").filter(|v| !v.is_null()).map(|p| {
        let f32_pf = |key: &str, default: f32| -> f32 {
            p.get(key).and_then(|v| v.as_f64()).map(|v| v as f32).unwrap_or(default)
        };
        let distortion = p.get("distortion").filter(|v| !v.is_null()).and_then(|d| {
            let dk = |key: &str| -> Option<f32> { d.get(key).and_then(|v| v.as_f64()).map(|v| v as f32) };
            match d.get("model").and_then(|v| v.as_str()) {
                Some("poly3") => dk("k1").map(|k1| LensDistortion::Poly3 { k1 }),
                Some("poly5") => match (dk("k1"), dk("k2")) {
                    (Some(k1), Some(k2)) => Some(LensDistortion::Poly5 { k1, k2 }),
                    _ => None,
                },
                Some("ptlens") => match (dk("a"), dk("b"), dk("c")) {
                    (Some(a), Some(b), Some(c)) => Some(LensDistortion::Ptlens { a, b, c }),
                    _ => None,
                },
                _ => None,
            }
        });
        let tca = p.get("tca").filter(|v| !v.is_null()).and_then(|t| {
            let tk = |key: &str| -> Option<f32> { t.get(key).and_then(|v| v.as_f64()).map(|v| v as f32) };
            let arr3 = |key: &str| -> Option<[f32; 3]> {
                let a = t.get(key)?.as_array()?;
                if a.len() != 3 {
                    return None;
                }
                Some([a[0].as_f64()? as f32, a[1].as_f64()? as f32, a[2].as_f64()? as f32])
            };
            match t.get("model").and_then(|v| v.as_str()) {
                Some("linear") => match (tk("kr"), tk("kb")) {
                    (Some(kr), Some(kb)) => Some(LensTca::Linear { kr, kb }),
                    _ => None,
                },
                Some("poly3") => match (arr3("red"), arr3("blue")) {
                    (Some(red), Some(blue)) => Some(LensTca::Poly3 { red, blue }),
                    _ => None,
                },
                _ => None,
            }
        });
        let vignetting = p.get("vignetting").filter(|v| !v.is_null()).map(|v| LensVignetting {
            k1: v.get("k1").and_then(|x| x.as_f64()).unwrap_or(0.0) as f32,
            k2: v.get("k2").and_then(|x| x.as_f64()).unwrap_or(0.0) as f32,
            k3: v.get("k3").and_then(|x| x.as_f64()).unwrap_or(0.0) as f32,
        });
        LensProfileData {
            crop_factor: f32_pf("crop_factor", 1.0),
            real_focal: f32_pf("real_focal", 50.0),
            lens_center_x: f32_pf("lens_center_x", 0.0),
            lens_center_y: f32_pf("lens_center_y", 0.0),
            distortion,
            tca,
            vignetting,
        }
    });

    LensCorrection {
        profile_enabled: bool_field("profile_enabled", false),
        distortion_amount: f32_field("distortion_amount", 100.0),
        vignette_amount: f32_field("vignette_amount", 100.0),
        ca_amount: f32_field("ca_amount", 100.0),
        manual_distortion: f32_field("manual_distortion", 0.0),
        manual_ca: f32_field("manual_ca", 0.0),
        profile,
    }
}

pub(super) fn lens_correction_is_identity(lc: &LensCorrection) -> bool {
    let profile_active = lc.profile_enabled
        && lc.profile.is_some()
        && (lc.distortion_amount > 0.0 || lc.vignette_amount > 0.0 || lc.ca_amount > 0.0);
    !profile_active && lc.manual_distortion == 0.0 && lc.manual_ca == 0.0
}

/// Pixel <-> normalized lens-space coordinate mapping. Port of
/// `Modifier::new`'s own math (`lensfun::modifier`) so the profile's
/// baked, already-rescaled coefficients land in the same space this
/// render's ACTUAL width/height implies -- the interactive preview and a
/// full-resolution export are not the same resolution, so this cannot be
/// baked once at match time; only the coefficients themselves can be.
#[derive(Clone, Copy)]
pub(super) struct LensNorm {
    pub(super) scale: f64,
    pub(super) unscale: f64,
    pub(super) center_x: f64,
    pub(super) center_y: f64,
}

impl LensNorm {
    /// Profile-space: real optical normalization (crop factor + real
    /// focal length + the lens's own optical center offset).
    pub(super) fn for_profile(p: &LensProfileData, width: u32, height: u32) -> Self {
        let w = if width >= 2 { (width - 1) as f64 } else { 1.0 };
        let h = if height >= 2 { (height - 1) as f64 } else { 1.0 };
        let scale = (36.0_f64).hypot(24.0) / p.crop_factor.max(0.01) as f64
            / (w + 1.0).hypot(h + 1.0)
            / p.real_focal.max(0.01) as f64;
        let size = w.min(h);
        let center_x = (w / 2.0 + size / 2.0 * p.lens_center_x as f64) * scale;
        let center_y = (h / 2.0 + size / 2.0 * p.lens_center_y as f64) * scale;
        LensNorm { scale, unscale: 1.0 / scale, center_x, center_y }
    }

    /// Manual-space: a simple, profile-independent normalization -- image
    /// center, radius 1.0 at the corner (half-diagonal). Deliberately its
    /// own space, not reused from `for_profile`: manual sliders are meant
    /// to work with no profile at all, so they can't depend on a real
    /// focal length/crop factor that may not exist.
    pub(super) fn for_manual(width: u32, height: u32) -> Self {
        let cx = width as f64 / 2.0;
        let cy = height as f64 / 2.0;
        let half_diag = cx.hypot(cy).max(1.0);
        let scale = 1.0 / half_diag;
        LensNorm { scale, unscale: half_diag, center_x: cx * scale, center_y: cy * scale }
    }

    pub(super) fn to_normalized(&self, px: f32, py: f32) -> (f32, f32) {
        ((px as f64 * self.scale - self.center_x) as f32, (py as f64 * self.scale - self.center_y) as f32)
    }

    pub(super) fn to_pixel(&self, nx: f32, ny: f32) -> (f32, f32) {
        (((nx as f64 + self.center_x) * self.unscale) as f32, ((ny as f64 + self.center_y) * self.unscale) as f32)
    }
}

/// Applies one distortion model, blended toward identity by `amount`
/// (0..1) -- NaN (poly3's own non-convergence contract) falls back to the
/// input unchanged, same as poly5/ptlens's built-in non-convergence
/// handling, so a pathological coefficient never produces a NaN pixel
/// coordinate downstream.
pub(super) fn apply_lens_distortion(d: LensDistortion, x: f32, y: f32, amount: f32) -> (f32, f32) {
    let (cx, cy) = match d {
        LensDistortion::Poly3 { k1 } => lens_undist_poly3(x, y, k1),
        LensDistortion::Poly5 { k1, k2 } => lens_undist_poly5(x, y, k1, k2),
        LensDistortion::Ptlens { a, b, c } => lens_undist_ptlens(x, y, a, b, c),
    };
    if cx.is_nan() || cy.is_nan() {
        return (x, y);
    }
    (lerp(x, cx, amount), lerp(y, cy, amount))
}

#[derive(Clone, Copy, PartialEq)]
pub(super) enum LensChannel {
    Red,
    Green,
    Blue,
}

/// Applies TCA for one channel (Green is always a no-op -- it's the
/// reference every other channel is corrected relative to), blended
/// toward identity by `amount`.
pub(super) fn apply_lens_tca(tca: LensTca, channel: LensChannel, x: f32, y: f32, amount: f32) -> (f32, f32) {
    if channel == LensChannel::Green {
        return (x, y);
    }
    let (cx, cy) = match tca {
        LensTca::Linear { kr, kb } => {
            let k = if channel == LensChannel::Red { kr } else { kb };
            (x * k, y * k)
        }
        LensTca::Poly3 { red, blue } => {
            let [v, c, b] = if channel == LensChannel::Red { red } else { blue };
            lens_undist_tca_poly3(x, y, v, c, b)
        }
    };
    (lerp(x, cx, amount), lerp(y, cy, amount))
}

/// Where a given OUTPUT pixel's `channel` samples from in the SOURCE
/// image: profile distortion, then manual distortion (composed
/// sequentially -- both are corrections applied on top of one another,
/// not alternatives), then profile TCA, then manual CA -- each stage
/// only for the channels/amounts actually active.
pub(super) fn lens_correct_coord(
    px: f32,
    py: f32,
    channel: LensChannel,
    lc: &LensCorrection,
    profile_norm: Option<LensNorm>,
    manual_norm: LensNorm,
) -> (f32, f32) {
    let (mut x, mut y) = (px, py);

    if let (true, Some(profile), Some(norm)) = (lc.profile_enabled, &lc.profile, profile_norm) {
        if lc.distortion_amount > 0.0 {
            if let Some(d) = profile.distortion {
                let (nx, ny) = norm.to_normalized(x, y);
                let (cx, cy) = apply_lens_distortion(d, nx, ny, lc.distortion_amount / 100.0);
                (x, y) = norm.to_pixel(cx, cy);
            }
        }
    }

    if lc.manual_distortion != 0.0 {
        let k1 = (lc.manual_distortion / 100.0) * MANUAL_DISTORTION_MAX_K1;
        let (nx, ny) = manual_norm.to_normalized(x, y);
        let (cx, cy) = lens_undist_poly3(nx, ny, k1);
        if !cx.is_nan() && !cy.is_nan() {
            (x, y) = manual_norm.to_pixel(cx, cy);
        }
    }

    if let (true, Some(profile), Some(norm)) = (lc.profile_enabled, &lc.profile, profile_norm) {
        if lc.ca_amount > 0.0 {
            if let Some(tca) = profile.tca {
                let (nx, ny) = norm.to_normalized(x, y);
                let (cx, cy) = apply_lens_tca(tca, channel, nx, ny, lc.ca_amount / 100.0);
                (x, y) = norm.to_pixel(cx, cy);
            }
        }
    }

    if lc.manual_ca != 0.0 && channel != LensChannel::Green {
        let shift = (lc.manual_ca / 100.0) * MANUAL_CA_MAX_SHIFT;
        let k = if channel == LensChannel::Red { 1.0 + shift } else { 1.0 - shift };
        let (nx, ny) = manual_norm.to_normalized(x, y);
        (x, y) = manual_norm.to_pixel(nx * k, ny * k);
    }

    (x, y)
}

/// Applies lens-correction geometry (distortion + per-channel CA) via
/// resampling, then vignetting correction as a per-pixel gain -- see this
/// section's own header comment for why geometry needs a fresh output
/// buffer (like `rotate_image`) while vignetting doesn't. Skipped
/// ENTIRELY at identity, matching every other op in this file.
pub(crate) fn apply_lens_correction(image: &mut RgbImage, stack: &EditStack) {
    let lc = lens_correction_op(&stack.ops);
    if lens_correction_is_identity(&lc) {
        return;
    }

    let (width, height) = (image.width(), image.height());
    let profile_norm = lc.profile.as_ref().map(|p| LensNorm::for_profile(p, width, height));
    let manual_norm = LensNorm::for_manual(width, height);

    let needs_geometry = (lc.profile_enabled && lc.profile.is_some() && (lc.distortion_amount > 0.0 || lc.ca_amount > 0.0))
        || lc.manual_distortion != 0.0
        || lc.manual_ca != 0.0;
    if needs_geometry {
        let source = image.clone();
        for oy in 0..height {
            for ox in 0..width {
                let (rx, ry) = lens_correct_coord(ox as f32, oy as f32, LensChannel::Red, &lc, profile_norm, manual_norm);
                let (gx, gy) =
                    lens_correct_coord(ox as f32, oy as f32, LensChannel::Green, &lc, profile_norm, manual_norm);
                let (bx, by) = lens_correct_coord(ox as f32, oy as f32, LensChannel::Blue, &lc, profile_norm, manual_norm);
                let r = sample_bilinear(&source, rx, ry).0[0];
                let g = sample_bilinear(&source, gx, gy).0[1];
                let b = sample_bilinear(&source, bx, by).0[2];
                image.put_pixel(ox, oy, image::Rgb([r, g, b]));
            }
        }
    }

    if lc.profile_enabled && lc.vignette_amount > 0.0 {
        if let (Some(profile), Some(norm)) = (&lc.profile, profile_norm) {
            if let Some(v) = profile.vignetting {
                let amount = lc.vignette_amount / 100.0;
                for oy in 0..height {
                    for ox in 0..width {
                        let (nx, ny) = norm.to_normalized(ox as f32, oy as f32);
                        let r2 = nx * nx + ny * ny;
                        let r4 = r2 * r2;
                        let r6 = r4 * r2;
                        let gain = 1.0 + v.k1 * r2 + v.k2 * r4 + v.k3 * r6;
                        // DeVignetting (correcting real darkening): apply
                        // 1/gain, matching lensfun's own reverse=false
                        // convention (mod-color.cpp:36-152 / this crate's
                        // `Modifier::apply_color_modification`).
                        let mult = lerp(1.0, 1.0 / gain.max(0.01), amount);
                        let p = image.get_pixel(ox, oy).0;
                        let scale = |c: u8| (c as f32 * mult).round().clamp(0.0, 255.0) as u8;
                        image.put_pixel(ox, oy, image::Rgb([scale(p[0]), scale(p[1]), scale(p[2])]));
                    }
                }
            }
        }
    }
}
