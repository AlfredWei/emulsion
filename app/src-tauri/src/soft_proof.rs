//! Soft proofing (M4) — simulates how the current edit-graded image would
//! render on a target output profile, via lcms2's real ICC proofing
//! transform (`Transform::new_proofing`). This is the first real use of
//! the `lcms2` dependency, which has sat unused since M0 (see
//! `raw_decode.rs`'s own header comment noting the still-unwired color
//! management step).
//!
//! Runs entirely CPU-side against a cached PNG (see
//! `preview_cache::ensure_soft_proof_preview_for_hash`) — the same scope
//! cut this codebase already made for Export (ADR-0004: no <100ms latency
//! requirement, no need to port an ICC transform into the interactive
//! WGSL shader).
//!
//! **Named simplification, not a silently-papered-over gap**: this app has
//! no real working-space/display-profile management anywhere in its
//! pipeline (every other consumer — the WGSL shader's display math,
//! Export's JPEG encode, `preview_cache::ensure_graded_preview_for_hash`'s
//! own graded PNG — already treats the rendered buffer as plain sRGB, with
//! no embedded-profile handling). Soft proofing here therefore simulates
//! "sRGB source → target profile", not "true working space → target
//! profile" — consistent with the rest of this codebase's current color
//! model, not a new gap introduced by this slice. Real embedded-source-
//! profile handling is a separate, future gap (M2's own scope already
//! named "read and apply each file's embedded color profile" as unbuilt).

use lcms2::{
    CIExyY, CIExyYTRIPLE, Flags, GlobalContext, Intent, PixelFormat, Profile, ToneCurve, Transform,
};

pub const TARGET_SRGB: &str = "srgb";
pub const TARGET_ADOBE_RGB: &str = "adobe-rgb";
pub const TARGET_PROPHOTO_RGB: &str = "prophoto-rgb";
pub const TARGET_CUSTOM: &str = "custom";

#[derive(Debug, Clone, serde::Deserialize)]
pub struct SoftProofSettings {
    pub target: String,
    pub custom_profile_path: Option<String>,
    /// Rendering intent used for the proofing step itself ("perceptual",
    /// "relative", "saturation", or "absolute") — the intent a user
    /// actually chooses in real Lightroom's own soft-proof panel. The
    /// source→display leg (sRGB→sRGB, see this module's header comment)
    /// is always Relative Colorimetric, since source and display are the
    /// same profile there — there's nothing meaningful to choose.
    pub intent: String,
    pub gamut_warning: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum SoftProofError {
    #[error("unknown proof target: {0}")]
    UnknownTarget(String),
    #[error("unknown rendering intent: {0}")]
    UnknownIntent(String),
    #[error("no custom ICC profile file was chosen")]
    MissingCustomProfile,
    #[error("could not read ICC profile file: {0}")]
    Io(#[from] std::io::Error),
    #[error("could not load ICC profile: {0}")]
    Profile(String),
}

fn parse_intent(s: &str) -> Result<Intent, SoftProofError> {
    match s {
        "perceptual" => Ok(Intent::Perceptual),
        "relative" => Ok(Intent::RelativeColorimetric),
        "saturation" => Ok(Intent::Saturation),
        "absolute" => Ok(Intent::AbsoluteColorimetric),
        other => Err(SoftProofError::UnknownIntent(other.to_string())),
    }
}

/// Adobe RGB (1998) compatible primaries/white point/gamma — built via
/// `Profile::new_rgb` rather than bundling a real ICC asset, since lcms2
/// can synthesize an equivalent matrix-shaper profile from these well-known,
/// publicly documented constants.
fn adobe_rgb_profile() -> Result<Profile<GlobalContext>, SoftProofError> {
    let white = CIExyY { x: 0.3127, y: 0.3290, Y: 1.0 };
    let primaries = CIExyYTRIPLE {
        Red: CIExyY { x: 0.6400, y: 0.3300, Y: 1.0 },
        Green: CIExyY { x: 0.2100, y: 0.7100, Y: 1.0 },
        Blue: CIExyY { x: 0.1500, y: 0.0600, Y: 1.0 },
    };
    let curve = ToneCurve::new(2.19921875);
    Profile::new_rgb(&white, &primaries, &[&curve, &curve, &curve])
        .map_err(|e| SoftProofError::Profile(e.to_string()))
}

/// ProPhoto RGB compatible primaries/white point, approximated with a
/// simple gamma-1.8 curve (real ProPhoto RGB has a small linear toe below
/// a threshold; this matrix-shaper approximation is standard practice for
/// a synthesized profile and close enough for a soft-proof simulation —
/// not claimed to be byte-identical to the official ICC profile).
fn prophoto_rgb_profile() -> Result<Profile<GlobalContext>, SoftProofError> {
    let white = CIExyY { x: 0.3457, y: 0.3585, Y: 1.0 };
    let primaries = CIExyYTRIPLE {
        Red: CIExyY { x: 0.7347, y: 0.2653, Y: 1.0 },
        Green: CIExyY { x: 0.1596, y: 0.8404, Y: 1.0 },
        Blue: CIExyY { x: 0.0366, y: 0.0001, Y: 1.0 },
    };
    let curve = ToneCurve::new(1.8);
    Profile::new_rgb(&white, &primaries, &[&curve, &curve, &curve])
        .map_err(|e| SoftProofError::Profile(e.to_string()))
}

/// Resolves the settings' target profile, plus the raw bytes behind
/// `TARGET_CUSTOM` (`None` for every built-in target). The bytes are
/// returned so `settings_cache_key` can content-hash them — a user-chosen
/// ICC file can change on disk at the same path, and this codebase already
/// established (`preview_cache.rs`) that hashing content, not path, is the
/// only cache-key discipline that doesn't have a latent staleness bug.
fn resolve_target(
    settings: &SoftProofSettings,
) -> Result<(Profile<GlobalContext>, Option<Vec<u8>>), SoftProofError> {
    match settings.target.as_str() {
        TARGET_SRGB => Ok((Profile::new_srgb(), None)),
        TARGET_ADOBE_RGB => Ok((adobe_rgb_profile()?, None)),
        TARGET_PROPHOTO_RGB => Ok((prophoto_rgb_profile()?, None)),
        TARGET_CUSTOM => {
            let path = settings
                .custom_profile_path
                .as_deref()
                .ok_or(SoftProofError::MissingCustomProfile)?;
            let bytes = std::fs::read(path)?;
            let profile = Profile::new_icc(&bytes).map_err(|e| SoftProofError::Profile(e.to_string()))?;
            Ok((profile, Some(bytes)))
        }
        other => Err(SoftProofError::UnknownTarget(other.to_string())),
    }
}

/// A short, stable string identifying this exact proofing configuration,
/// for `preview_cache.rs`'s cache-key folding — mirrors
/// `ensure_graded_preview_for_hash`'s own edit-stack-hash precedent
/// (content-addressed, so a settings change naturally produces a new
/// filename with no explicit invalidation step to get wrong). Deliberately
/// cheap to call before deciding whether a cache hit exists — the more
/// expensive `apply_soft_proof` (which also resolves the target profile)
/// only runs on an actual cache miss.
pub fn settings_cache_key(settings: &SoftProofSettings) -> Result<String, SoftProofError> {
    let (_, custom_bytes) = resolve_target(settings)?;
    let mut key = format!("{}_{}_{}", settings.target, settings.intent, settings.gamut_warning);
    if let Some(bytes) = custom_bytes {
        key.push('_');
        key.push_str(&blake3::hash(&bytes).to_hex().to_string()[..8]);
    }
    Ok(blake3::hash(key.as_bytes()).to_hex().to_string()[..8].to_string())
}

/// Applies the ICC soft-proof (and, if requested, gamut-check) transform to
/// `image` in place.
pub fn apply_soft_proof(image: &mut image::RgbImage, settings: &SoftProofSettings) -> Result<(), SoftProofError> {
    let (proof, _) = resolve_target(settings)?;
    let intent = parse_intent(&settings.intent)?;
    apply_soft_proof_with_profile(image, &proof, intent, settings.gamut_warning)
}

/// The actual transform, split out from `apply_soft_proof` so tests can
/// exercise it against a hand-built profile without going through the
/// public `SoftProofSettings` target enum (none of `TARGET_*`'s built-ins
/// are narrow enough relative to sRGB to produce an interesting, testable
/// gamut-check result — see the tests below).
///
/// `Transform::set_global_alarm_codes` is a genuinely global (not
/// per-transform) lcms2 setting — acceptable here since every real call
/// path runs sequentially on its own `spawn_blocking` task, never
/// concurrently with another proofing call in practice (same class of
/// accepted, named non-issue as this codebase's other single-threaded-in-
/// practice assumptions).
fn apply_soft_proof_with_profile(
    image: &mut image::RgbImage,
    proof: &Profile<GlobalContext>,
    proofing_intent: Intent,
    gamut_warning: bool,
) -> Result<(), SoftProofError> {
    let source = Profile::new_srgb();

    let mut flags = Flags::SOFT_PROOFING;
    if gamut_warning {
        flags = flags | Flags::GAMUT_CHECK;
        // Real Lightroom's own default gamut-warning color: a flat mid-gray.
        // 16 is lcms2's own `MAXCHANNELS` constant (not publicly re-exported
        // by the `lcms2` crate, only its `-sys` crate) -- fixed by the C
        // library itself, not something this app chooses.
        //
        // `set_global_alarm_codes` is deprecated in favor of
        // `ThreadContext::set_alarm_codes` -- not adopted here since that
        // would mean threading a `ThreadContext` through every profile/
        // transform in this module for one attribute setting, and every
        // real call path already runs sequentially (see this function's
        // own doc comment), so the "global" scope this deprecated function
        // implies is a non-issue in practice.
        #[allow(deprecated)]
        Transform::<[u8; 3], [u8; 3]>::set_global_alarm_codes([0x8080; 16]);
    }

    let transform: Transform<[u8; 3], [u8; 3]> = Transform::new_proofing(
        &source,
        PixelFormat::RGB_8,
        &source,
        PixelFormat::RGB_8,
        proof,
        Intent::RelativeColorimetric,
        proofing_intent,
        flags,
    )
    .map_err(|e| SoftProofError::Profile(e.to_string()))?;

    let pixels: &mut [[u8; 3]] = bytemuck::cast_slice_mut(image.as_mut());
    transform.transform_in_place(pixels);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn narrow_test_profile() -> Profile<GlobalContext> {
        // A deliberately small-gamut RGB profile (primaries clustered near
        // the D65 white point) — much narrower than sRGB, so a saturated
        // sRGB color is guaranteed to fall outside it. Real bundled targets
        // (Adobe RGB, ProPhoto RGB) are all WIDER than sRGB and so can
        // never produce an out-of-gamut result worth testing here.
        let white = CIExyY { x: 0.3127, y: 0.3290, Y: 1.0 };
        let primaries = CIExyYTRIPLE {
            Red: CIExyY { x: 0.40, y: 0.30, Y: 1.0 },
            Green: CIExyY { x: 0.30, y: 0.40, Y: 1.0 },
            Blue: CIExyY { x: 0.28, y: 0.25, Y: 1.0 },
        };
        let curve = ToneCurve::new(2.2);
        Profile::new_rgb(&white, &primaries, &[&curve, &curve, &curve]).unwrap()
    }

    #[test]
    fn srgb_target_at_relative_intent_is_effectively_identity() {
        let proof = Profile::new_srgb();
        let mut image = image::RgbImage::from_raw(2, 1, vec![255, 0, 0, 30, 140, 200]).unwrap();
        let before = image.clone();

        apply_soft_proof_with_profile(&mut image, &proof, Intent::RelativeColorimetric, false).unwrap();

        for (a, b) in before.pixels().zip(image.pixels()) {
            for c in 0..3 {
                let diff = (a[c] as i16 - b[c] as i16).abs();
                assert!(diff <= 2, "expected near-identity, got {a:?} -> {b:?}");
            }
        }
    }

    #[test]
    fn gamut_warning_flags_an_out_of_gamut_pixel_but_not_a_near_neutral_one() {
        let proof = narrow_test_profile();
        // Pixel 0: fully saturated sRGB red -- well outside the narrow
        // profile's tiny gamut. Pixel 1: a near-neutral mid-gray -- inside
        // any reasonable RGB gamut, including the narrow one.
        let mut image = image::RgbImage::from_raw(2, 1, vec![255, 0, 0, 128, 128, 128]).unwrap();

        apply_soft_proof_with_profile(&mut image, &proof, Intent::RelativeColorimetric, true).unwrap();

        let flagged = image.get_pixel(0, 0);
        assert_eq!(flagged.0, [0x80, 0x80, 0x80], "out-of-gamut pixel should be flagged with the alarm color");

        let neutral = image.get_pixel(1, 0);
        let diff: i16 = (neutral[0] as i16 - 128).abs() + (neutral[1] as i16 - 128).abs() + (neutral[2] as i16 - 128).abs();
        assert!(diff <= 6, "in-gamut neutral gray should be left close to unchanged, got {neutral:?}");
    }

    #[test]
    fn soft_proofing_without_gamut_warning_still_clips_toward_gamut_instead_of_flagging() {
        let proof = narrow_test_profile();
        let mut image = image::RgbImage::from_raw(1, 1, vec![255, 0, 0]).unwrap();

        apply_soft_proof_with_profile(&mut image, &proof, Intent::RelativeColorimetric, false).unwrap();

        let simulated = image.get_pixel(0, 0);
        assert_ne!(simulated.0, [0x80, 0x80, 0x80], "no gamut_warning flag requested -- must not be the alarm color");
        assert_ne!(simulated.0, [255, 0, 0], "an out-of-gamut color must actually be remapped by proofing, not passed through");
    }

    #[test]
    fn unknown_target_fails_cleanly() {
        let settings = SoftProofSettings {
            target: "not-a-real-target".to_string(),
            custom_profile_path: None,
            intent: "relative".to_string(),
            gamut_warning: false,
        };
        let mut image = image::RgbImage::from_raw(1, 1, vec![10, 20, 30]).unwrap();
        let err = apply_soft_proof(&mut image, &settings).unwrap_err();
        assert!(matches!(err, SoftProofError::UnknownTarget(_)));
    }

    #[test]
    fn unknown_intent_fails_cleanly() {
        let settings = SoftProofSettings {
            target: TARGET_SRGB.to_string(),
            custom_profile_path: None,
            intent: "not-a-real-intent".to_string(),
            gamut_warning: false,
        };
        let mut image = image::RgbImage::from_raw(1, 1, vec![10, 20, 30]).unwrap();
        let err = apply_soft_proof(&mut image, &settings).unwrap_err();
        assert!(matches!(err, SoftProofError::UnknownIntent(_)));
    }

    #[test]
    fn custom_target_with_no_path_fails_cleanly() {
        let settings = SoftProofSettings {
            target: TARGET_CUSTOM.to_string(),
            custom_profile_path: None,
            intent: "relative".to_string(),
            gamut_warning: false,
        };
        let mut image = image::RgbImage::from_raw(1, 1, vec![10, 20, 30]).unwrap();
        let err = apply_soft_proof(&mut image, &settings).unwrap_err();
        assert!(matches!(err, SoftProofError::MissingCustomProfile));
    }

    #[test]
    fn custom_target_with_an_unreadable_path_fails_cleanly() {
        let settings = SoftProofSettings {
            target: TARGET_CUSTOM.to_string(),
            custom_profile_path: Some("/nonexistent/path/does-not-exist.icc".to_string()),
            intent: "relative".to_string(),
            gamut_warning: false,
        };
        let mut image = image::RgbImage::from_raw(1, 1, vec![10, 20, 30]).unwrap();
        let err = apply_soft_proof(&mut image, &settings).unwrap_err();
        assert!(matches!(err, SoftProofError::Io(_)));
    }

    #[test]
    fn cache_key_changes_when_any_setting_changes() {
        let base = SoftProofSettings {
            target: TARGET_SRGB.to_string(),
            custom_profile_path: None,
            intent: "relative".to_string(),
            gamut_warning: false,
        };
        let base_key = settings_cache_key(&base).unwrap();

        let different_target = SoftProofSettings { target: TARGET_ADOBE_RGB.to_string(), ..base.clone() };
        let different_intent = SoftProofSettings { intent: "perceptual".to_string(), ..base.clone() };
        let different_gamut = SoftProofSettings { gamut_warning: true, ..base.clone() };

        assert_ne!(base_key, settings_cache_key(&different_target).unwrap());
        assert_ne!(base_key, settings_cache_key(&different_intent).unwrap());
        assert_ne!(base_key, settings_cache_key(&different_gamut).unwrap());
    }
}
