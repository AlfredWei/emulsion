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
//   Lightroom's own per-category Amount sliders. The profile math lives in
//   `lens/profile.rs`, compiled only with the `lens-profiles` feature
//   (ADR-0011); without it `lens/profile_absent.rs` makes a profile
//   impossible, so a stored profile op renders without its profile part.
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

#[cfg(feature = "lens-profiles")]
mod profile;
#[cfg(not(feature = "lens-profiles"))]
#[path = "lens/profile_absent.rs"]
mod profile;

#[cfg(feature = "lens-profiles")]
pub(super) use profile::*;
#[cfg(not(feature = "lens-profiles"))]
pub(super) use profile::LensProfileData;

/// Manual Distortion's inverse of `Rd = Ru*(1 + k1*Ru^2)`: the point on the same
/// ray whose undistorted radius maps to this point's radius. Written from a
/// spec alone, without reading any earlier implementation (ADR-0011). Newton's
/// method in f64, started on the side of the root where it is monotone: above
/// it for k1 > 0 (the cubic is convex), at `rd` for k1 < 0 (concave up to its
/// peak). (NaN, NaN) when `rd` is past the k1 < 0 peak, so no root exists, or
/// the iteration fails; callers check for NaN and leave the pixel unmoved.
pub(super) fn lens_undist_poly3(x: f32, y: f32, k1: f32) -> (f32, f32) {
    if k1 == 0.0 || (x == 0.0 && y == 0.0) {
        return (x, y);
    }
    const NAN2: (f32, f32) = (f32::NAN, f32::NAN);
    let (xd, yd, kd) = (x as f64, y as f64, k1 as f64);
    let rd = xd.hypot(yd);
    if !rd.is_finite() || !kd.is_finite() {
        return NAN2;
    }
    let Some(ru) = undist_radius(rd, kd) else {
        return NAN2;
    };
    let s = ru / rd;
    let (ox, oy) = ((xd * s) as f32, (yd * s) as f32);
    if ox.is_finite() && oy.is_finite() {
        (ox, oy)
    } else {
        NAN2
    }
}

/// Smallest non-negative `ru` with `ru * (1 + k1 * ru^2) == rd`, or `None`.
fn undist_radius(rd: f64, k1: f64) -> Option<f64> {
    const MAX_ITERS: usize = 100;
    let f = |ru: f64| ru * (1.0 + k1 * ru * ru) - rd;
    let fp = |ru: f64| 1.0 + 3.0 * k1 * ru * ru;
    let mut ru = if k1 > 0.0 {
        rd.min((rd / k1).cbrt())
    } else {
        let ru_peak = (-1.0 / (3.0 * k1)).sqrt();
        if rd > ru_peak * (2.0 / 3.0) {
            return None;
        }
        rd
    };
    for _ in 0..MAX_ITERS {
        let fv = f(ru);
        if fv.abs() <= 4.0 * f64::EPSILON * (rd + ru) {
            return Some(ru);
        }
        let d = fp(ru);
        if !(d > 0.0) {
            return None;
        }
        let step = fv / d;
        ru -= step;
        if !ru.is_finite() || ru < 0.0 {
            return None;
        }
        if step.abs() <= 1e-15 + 1e-13 * ru {
            return Some(ru);
        }
    }
    None
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

    LensCorrection {
        profile_enabled: bool_field("profile_enabled", false),
        distortion_amount: f32_field("distortion_amount", 100.0),
        vignette_amount: f32_field("vignette_amount", 100.0),
        ca_amount: f32_field("ca_amount", 100.0),
        manual_distortion: f32_field("manual_distortion", 0.0),
        manual_ca: f32_field("manual_ca", 0.0),
        profile: op.get("profile").filter(|v| !v.is_null()).and_then(profile::parse),
    }
}

pub(super) fn lens_correction_is_identity(lc: &LensCorrection) -> bool {
    let profile_active = lc.profile_enabled
        && lc.profile.is_some()
        && (lc.distortion_amount > 0.0 || lc.vignette_amount > 0.0 || lc.ca_amount > 0.0);
    !profile_active && lc.manual_distortion == 0.0 && lc.manual_ca == 0.0
}

/// Pixel <-> normalized lens-space coordinate mapping. The interactive
/// preview and a full-resolution export are not the same resolution, so
/// this cannot be baked once at match time; only a profile's coefficients
/// can be. `for_profile` is in `lens/profile.rs`.
#[derive(Clone, Copy)]
pub(super) struct LensNorm {
    pub(super) scale: f64,
    pub(super) unscale: f64,
    pub(super) center_x: f64,
    pub(super) center_y: f64,
}

impl LensNorm {
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

#[derive(Clone, Copy, PartialEq)]
pub(super) enum LensChannel {
    Red,
    Green,
    Blue,
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
            (x, y) = profile::correct_distortion(profile, norm, x, y, lc.distortion_amount / 100.0);
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
            (x, y) = profile::correct_tca(profile, norm, channel, x, y, lc.ca_amount / 100.0);
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
            profile::devignette(image, profile, norm, lc.vignette_amount / 100.0);
        }
    }
}
