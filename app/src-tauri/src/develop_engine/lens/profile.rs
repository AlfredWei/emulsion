//! The PROFILE half of Lens Corrections, compiled only with the `lens-profiles` feature (ADR-0011). Its kernels are
//! described as ports of lensfun math (RFC-0039 section 3, regions 2 and 3), so a build without lens profiles leaves
//! the whole file out and `lens.rs` uses `profile_absent.rs` instead.
//!
//! The distortion/TCA formulas below are a direct, hand-verified port of
//! `lensfun`'s own pure kernels -- the FORWARD ones (`mod_coord::{dist_poly3,
//! dist_poly5,dist_ptlens}`, `mod_subpix::tca_poly3_forward`), which are what
//! lensfun's `reverse = false` CORRECTION uses (RFC-0021: the lensfun crate's
//! own doc comment on `Modifier::new` says the opposite; its tests and
//! dispatcher agree with each other, and the earlier version of this file,
//! which ported the Newton inverses, applied distortion and TCA backwards).
//! This module never touches the `lensfun` crate itself, only the plain f32s
//! `lens_profile::match_profile` already resolved into the op's JSON.

use super::{lerp, linear_to_srgb, srgb_to_linear, LensChannel, LensNorm, RgbImage};

/// FORWARD poly3, `Rd = Ru*(1 + k1*Ru^2)` -- port of
/// `lensfun::mod_coord::dist_poly3`. This (not a Newton inverse)
/// is the CORRECTING map: an output (undistorted) pixel at radius `Ru`
/// samples the source (distorted) image at `Rd`. RFC-0021 found the
/// profile path had used the inverse kernels, i.e. the direction lensfun
/// uses with `reverse = true`, which ADDS distortion.
pub(in crate::develop_engine) fn lens_dist_poly3(x: f32, y: f32, k1: f32) -> (f32, f32) {
    let poly2 = k1 * (x * x + y * y) + 1.0;
    (x * poly2, y * poly2)
}

/// FORWARD poly5, `Rd = Ru*(1 + k1*Ru^2 + k2*Ru^4)` (`dist_poly5`).
pub(in crate::develop_engine) fn lens_dist_poly5(x: f32, y: f32, k1: f32, k2: f32) -> (f32, f32) {
    let ru2 = x * x + y * y;
    let poly4 = 1.0 + k1 * ru2 + k2 * ru2 * ru2;
    (x * poly4, y * poly4)
}

/// FORWARD PTLens, `Rd = Ru*(a*Ru^3 + b*Ru^2 + c*Ru + 1)` (`dist_ptlens`).
pub(in crate::develop_engine) fn lens_dist_ptlens(x: f32, y: f32, a: f32, b: f32, c: f32) -> (f32, f32) {
    let ru2 = x * x + y * y;
    let r = ru2.sqrt();
    let poly3 = a * ru2 * r + b * ru2 + c * r + 1.0;
    (x * poly3, y * poly3)
}

/// FORWARD per-channel TCA poly3, `Rd = Ru*(b*Ru^2 + c*Ru + v)`
/// (`mod_subpix::tca_poly3_forward`, one channel). The correcting
/// direction, same RFC-0021 reasoning as the distortion kernels above.
pub(in crate::develop_engine) fn lens_tca_poly3_forward(x: f32, y: f32, v: f32, c: f32, b: f32) -> (f32, f32) {
    let ru2 = x * x + y * y;
    let poly2 = if c == 0.0 { b * ru2 + v } else { b * ru2 + c * ru2.sqrt() + v };
    (x * poly2, y * poly2)
}

/// Plain-data mirror of `lens_profile::DistortionCoeffs` -- deliberately
/// NOT the same type (this module must not depend on the `lensfun` crate
/// at all), parsed independently from the op's JSON.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(in crate::develop_engine) enum LensDistortion {
    Poly3 { k1: f32 },
    Poly5 { k1: f32, k2: f32 },
    Ptlens { a: f32, b: f32, c: f32 },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(in crate::develop_engine) enum LensTca {
    Linear { kr: f32, kb: f32 },
    Poly3 { red: [f32; 3], blue: [f32; 3] },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(in crate::develop_engine) struct LensVignetting {
    pub(in crate::develop_engine) k1: f32,
    pub(in crate::develop_engine) k2: f32,
    pub(in crate::develop_engine) k3: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(in crate::develop_engine) struct LensProfileData {
    pub(in crate::develop_engine) crop_factor: f32,
    pub(in crate::develop_engine) real_focal: f32,
    pub(in crate::develop_engine) lens_center_x: f32,
    pub(in crate::develop_engine) lens_center_y: f32,
    pub(in crate::develop_engine) distortion: Option<LensDistortion>,
    pub(in crate::develop_engine) tca: Option<LensTca>,
    pub(in crate::develop_engine) vignetting: Option<LensVignetting>,
}

/// The op's `profile` object, as `lens_profile::match_profile` baked it.
pub(super) fn parse(p: &serde_json::Value) -> Option<LensProfileData> {
    let f32_pf = |key: &str, default: f32| -> f32 { p.get(key).and_then(|v| v.as_f64()).map(|v| v as f32).unwrap_or(default) };
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
    Some(LensProfileData {
        crop_factor: f32_pf("crop_factor", 1.0),
        real_focal: f32_pf("real_focal", 50.0),
        lens_center_x: f32_pf("lens_center_x", 0.0),
        lens_center_y: f32_pf("lens_center_y", 0.0),
        distortion,
        tca,
        vignetting,
    })
}

impl LensNorm {
    /// Profile-space: real optical normalization (crop factor + real
    /// focal length + the lens's own optical center offset). Port of
    /// `Modifier::new`'s own math (`lensfun::modifier`) so the profile's
    /// baked, already-rescaled coefficients land in the same space this
    /// render's ACTUAL width/height implies.
    pub(in crate::develop_engine) fn for_profile(p: &LensProfileData, width: u32, height: u32) -> Self {
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
}

/// Applies one distortion model in the CORRECTING direction (RFC-0021: the
/// forward closed-form kernels, lensfun's `reverse = false`), blended
/// toward identity by `amount` (0..1). Closed form, so -- unlike the
/// Newton inverses this replaced -- there is no non-convergence case.
pub(in crate::develop_engine) fn apply_lens_distortion(d: LensDistortion, x: f32, y: f32, amount: f32) -> (f32, f32) {
    let (cx, cy) = match d {
        LensDistortion::Poly3 { k1 } => lens_dist_poly3(x, y, k1),
        LensDistortion::Poly5 { k1, k2 } => lens_dist_poly5(x, y, k1, k2),
        LensDistortion::Ptlens { a, b, c } => lens_dist_ptlens(x, y, a, b, c),
    };
    (lerp(x, cx, amount), lerp(y, cy, amount))
}

/// Applies TCA for one channel in the CORRECTING direction (RFC-0021;
/// Green is always a no-op -- it's the reference every other channel is
/// corrected relative to), blended toward identity by `amount`. The linear
/// model's `kr`/`kb` are used as stored (no inversion).
pub(in crate::develop_engine) fn apply_lens_tca(tca: LensTca, channel: LensChannel, x: f32, y: f32, amount: f32) -> (f32, f32) {
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
            lens_tca_poly3_forward(x, y, v, c, b)
        }
    };
    (lerp(x, cx, amount), lerp(y, cy, amount))
}

/// The profile's distortion step of `lens_correct_coord`, in pixels.
pub(super) fn correct_distortion(p: &LensProfileData, norm: LensNorm, x: f32, y: f32, amount: f32) -> (f32, f32) {
    let Some(d) = p.distortion else { return (x, y) };
    let (nx, ny) = norm.to_normalized(x, y);
    let (cx, cy) = apply_lens_distortion(d, nx, ny, amount);
    norm.to_pixel(cx, cy)
}

/// The profile's TCA step of `lens_correct_coord`, in pixels.
pub(super) fn correct_tca(p: &LensProfileData, norm: LensNorm, channel: LensChannel, x: f32, y: f32, amount: f32) -> (f32, f32) {
    let Some(tca) = p.tca else { return (x, y) };
    let (nx, ny) = norm.to_normalized(x, y);
    let (cx, cy) = apply_lens_tca(tca, channel, nx, ny, amount);
    norm.to_pixel(cx, cy)
}

/// Linear-light value back to an encoded, rounded, clamped byte
/// (RFC-0021's vignetting path; the decode half is a table in the caller).
pub(in crate::develop_engine) fn lens_vignette_encode(linear: f32) -> u8 {
    (linear_to_srgb(linear) * 255.0).round().clamp(0.0, 255.0) as u8
}

/// Vignetting correction as a per-pixel gain, in place, on the already geometry-corrected image.
pub(super) fn devignette(image: &mut RgbImage, p: &LensProfileData, norm: LensNorm, amount: f32) {
    let Some(v) = p.vignetting else { return };
    let (width, height) = (image.width(), image.height());
    // RFC-0021: the gain models optical irradiance fall-off, a
    // LINEAR-light quantity (lensfun's manual: vignetting must be
    // applied to linear data, or "the corners probably will look
    // too bright"). Decode via a 256-entry table, multiply, encode.
    let decode: Vec<f32> = (0..=255u32).map(|i| srgb_to_linear(i as f32 / 255.0)).collect();
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
            // `Modifier::apply_color_modification`). `lerp` blends
            // the LINEAR-light multiplier, so Amount 0 and a gain
            // of exactly 1 are bit-exact passthroughs.
            let mult = lerp(1.0, 1.0 / gain.max(0.01), amount);
            if mult == 1.0 {
                continue;
            }
            let px = image.get_pixel(ox, oy).0;
            let scale = |c: u8| lens_vignette_encode(decode[c as usize] * mult);
            image.put_pixel(ox, oy, image::Rgb([scale(px[0]), scale(px[1]), scale(px[2])]));
        }
    }
}
