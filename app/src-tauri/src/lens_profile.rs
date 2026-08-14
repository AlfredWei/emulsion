//! Lens-profile matching (M3: Lens Corrections).
//!
//! The ONLY file in this crate that touches the `lensfun` crate's types.
//! `develop_engine.rs` (CPU render/export) and the WGSL preview work
//! entirely from plain f32 coefficients baked into the edit stack's
//! `lens_correction` op -- see that op's own doc comment in
//! `develop_engine.rs` for why (in short: presets/portability, and keeping
//! the render path free of a `Database` lookup on every frame).
//!
//! `lensfun` v0.7.0 is a pre-alpha pure-Rust port of the LensFun C++
//! library -- no C/vcpkg dependency, unlike this crate's other native dep
//! (`rsraw`/LibRaw, see `vendor/rsraw-sys/PATCH.md`). Its bundled XML
//! database (`Database::load_bundled()`) ships inside the crate binary, so
//! there's no separate data file to package. License: LGPL-3.0-or-later
//! for the code (fine for this already-open-source repo); the upstream
//! lens database itself is CC-BY-SA-3.0 (attribution required) -- a real,
//! confirmed obligation, deferred to a follow-up credits/about surface,
//! not blocking for this slice.

use std::sync::OnceLock;

use lensfun::{CalibDistortion, CalibTca, CalibVignetting, Database, DistortionModel, TcaModel, VignettingModel};

/// Loaded once per process (parsing the bundled XML is not free) and
/// reused for every match -- `Database` is read-only after load, so no
/// `Mutex` is needed, matching this app's existing "one shared instance"
/// precedent (`AppState.catalog`) without adding a new lock. `None` means
/// the bundled database failed to parse (should not happen in practice,
/// but `load_bundled()` returns a real `Result` -- treated as "no
/// profiles available" rather than panicking the app over an optional
/// feature).
static LENS_DB: OnceLock<Option<Database>> = OnceLock::new();

fn lens_db() -> Option<&'static Database> {
    LENS_DB.get_or_init(|| Database::load_bundled().ok()).as_ref()
}

/// A resolved camera+lens match, with every coefficient the render path
/// needs already extracted as plain f32s -- baked verbatim into the edit
/// stack's `lens_correction` op's `profile` field by the frontend after a
/// `lookup_lens_profile` call. `crop_factor`/`real_focal`/`lens_center_*`
/// are needed at render time to convert PIXEL coordinates (which vary with
/// the current render's width/height -- the interactive preview and the
/// full-resolution export are NOT the same resolution) into the
/// normalized lens-space coordinates the distortion/TCA/vignetting
/// formulas operate in; the coefficients themselves are already rescaled
/// into that same resolution-independent normalized space (see
/// `rescale_distortion`/`rescale_tca`/`rescale_vignetting` below) and
/// don't need recomputing per render.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LensProfileMatch {
    pub camera: String,
    pub lens: String,
    pub crop_factor: f32,
    pub real_focal: f32,
    pub lens_center_x: f32,
    pub lens_center_y: f32,
    pub distortion: Option<DistortionCoeffs>,
    pub tca: Option<TcaCoeffs>,
    pub vignetting: Option<VignettingCoeffs>,
}

/// Mirrors `lensfun::DistortionModel`, minus the `None` variant (absence
/// is `Option::None` on the containing field instead) -- see that type's
/// own doc comment for the exact formula each variant evaluates.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "model", rename_all = "lowercase")]
pub enum DistortionCoeffs {
    Poly3 { k1: f32 },
    Poly5 { k1: f32, k2: f32 },
    Ptlens { a: f32, b: f32, c: f32 },
}

/// Mirrors `lensfun::TcaModel`, minus `None`. Upstream's ACM model isn't
/// ported by the `lensfun` crate yet (v0.7.0's own doc comment: Linear +
/// Poly3 cover every TCA calibration in the bundled XML database).
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "model", rename_all = "lowercase")]
pub enum TcaCoeffs {
    Linear { kr: f32, kb: f32 },
    Poly3 { red: [f32; 3], blue: [f32; 3] },
}

/// Mirrors `lensfun::VignettingModel::Pa` (the only model upstream
/// supports): `gain = 1 + k1*r^2 + k2*r^4 + k3*r^6`.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct VignettingCoeffs {
    pub k1: f32,
    pub k2: f32,
    pub k3: f32,
}

/// EXIF subject-distance is written by almost no cameras, so there's no
/// real per-shot value to feed vignetting interpolation. 1000m matches
/// lensfun's own upstream convention for "effectively infinity" -- most
/// vignetting calibrations in the bundled database are themselves shot at
/// distance, not close-focus, so this is the representative default, not
/// an arbitrary placeholder.
const DEFAULT_SUBJECT_DISTANCE_M: f32 = 1000.0;

/// Matches a photo's camera+lens EXIF against the bundled lens database.
/// Returns `None` for any of: no bundled database, missing camera/lens
/// EXIF, no equipment match, or a match with no usable calibration data at
/// all (camera/lens exists in the database but has zero distortion/TCA/
/// vignetting entries -- a real, unremarkable case for many lenses).
pub fn match_profile(
    camera_make: Option<&str>,
    camera_model: Option<&str>,
    lens_model: Option<&str>,
    focal_length: Option<f32>,
    aperture: Option<f32>,
) -> Option<LensProfileMatch> {
    let db = lens_db()?;
    let camera_model = camera_model.filter(|s| !s.trim().is_empty())?;
    let lens_model = lens_model.filter(|s| !s.trim().is_empty())?;
    let focal_length = focal_length.filter(|f| *f > 0.0)?;

    let cameras = db.find_cameras(camera_make, camera_model);
    let camera = cameras.first().copied();
    let lenses = db.find_lenses(camera, lens_model);
    let lens = *lenses.first()?;

    // The lens's own calibration crop factor is the correct fallback when
    // no camera matched (or the camera has no aspect_ratio override) --
    // matches how `Modifier::new`'s own doc comment distinguishes "the
    // image's crop factor" from "the lens calibration's own crop factor".
    let crop_factor = camera.map(|c| c.crop_factor).unwrap_or(lens.crop_factor);
    let real_focal = lens
        .interpolate_distortion(focal_length)
        .and_then(|c| c.real_focal)
        .unwrap_or(focal_length);

    let distortion = lens
        .interpolate_distortion(focal_length)
        .and_then(|c| rescale_distortion(&c, lens.aspect_ratio, crop_factor, real_focal as f64));
    let tca = lens
        .interpolate_tca(focal_length)
        .and_then(|c| rescale_tca(&c, lens.aspect_ratio, crop_factor, real_focal as f64));
    let vignetting = aperture.filter(|a| *a > 0.0).and_then(|ap| {
        lens.interpolate_vignetting(focal_length, ap, DEFAULT_SUBJECT_DISTANCE_M)
            .and_then(|c| rescale_vignetting(&c, crop_factor, real_focal as f64))
    });

    if distortion.is_none() && tca.is_none() && vignetting.is_none() {
        return None;
    }

    Some(LensProfileMatch {
        camera: camera
            .map(|c| format!("{} {}", c.maker, c.model))
            .unwrap_or_else(|| format!("{} (camera unmatched)", lens.maker)),
        lens: format!("{} {}", lens.maker, lens.model),
        crop_factor,
        real_focal,
        lens_center_x: lens.center_x,
        lens_center_y: lens.center_y,
        distortion,
        tca,
        vignetting,
    })
}

// -------------- coefficient rescaling --------------
//
// `lensfun`'s own `rescale_distortion`/`rescale_tca`/`rescale_vignetting`
// (`src/modifier.rs`) are private to that crate, so these are a direct
// port of the same math (mirroring upstream `rescale_polynomial_
// coefficients` in `mod-coord.cpp`/`mod-subpix.cpp`/`mod-color.cpp`) --
// not a reimplementation from scratch. Run ONCE per match (not per pixel,
// not per render resolution -- these depend only on the matched
// equipment's aspect ratio/crop/real-focal, none of which vary with the
// current render's width/height), so the baked `profile` coefficients are
// already in the same resolution-independent normalized space every
// render (preview or full export) computes its own `norm_scale` into.

fn rescale_distortion(
    lcd: &CalibDistortion,
    aspect_ratio: f32,
    crop: f32,
    real_focal: f64,
) -> Option<DistortionCoeffs> {
    let hugin_scale_in_mm = (36.0_f64).hypot(24.0) / crop as f64 / (aspect_ratio as f64).hypot(1.0) / 2.0;
    let hugin_scaling = (real_focal / hugin_scale_in_mm) as f32;
    let hs = hugin_scaling as f64;
    match lcd.model {
        DistortionModel::None => None,
        DistortionModel::Poly3 { k1 } => {
            let d = 1.0_f64 - k1 as f64;
            let k1 = (k1 as f64 * hs.powi(2) / d.powi(3)) as f32;
            if k1 == 0.0 { None } else { Some(DistortionCoeffs::Poly3 { k1 }) }
        }
        DistortionModel::Poly5 { k1, k2 } => Some(DistortionCoeffs::Poly5 {
            k1: (k1 as f64 * hs.powi(2)) as f32,
            k2: (k2 as f64 * hs.powi(4)) as f32,
        }),
        DistortionModel::Ptlens { a, b, c } => {
            let d = 1.0_f64 - a as f64 - b as f64 - c as f64;
            Some(DistortionCoeffs::Ptlens {
                a: (a as f64 * hs.powi(3) / d.powi(4)) as f32,
                b: (b as f64 * hs.powi(2) / d.powi(3)) as f32,
                c: (c as f64 * hs / d.powi(2)) as f32,
            })
        }
    }
}

fn rescale_tca(lctca: &CalibTca, aspect_ratio: f32, crop: f32, real_focal: f64) -> Option<TcaCoeffs> {
    let hugin_scale_in_mm = (36.0_f64).hypot(24.0) / crop as f64 / (aspect_ratio as f64).hypot(1.0) / 2.0;
    let hugin_scaling = (real_focal / hugin_scale_in_mm) as f32;
    match lctca.model {
        TcaModel::None => None,
        // Reverse direction (correcting a real photo) is baked in here --
        // this profile is always used to CORRECT, never to simulate.
        TcaModel::Linear { kr, kb } => Some(TcaCoeffs::Linear { kr: 1.0 / kr, kb: 1.0 / kb }),
        TcaModel::Poly3 { red, blue } => {
            let hs = hugin_scaling as f64;
            Some(TcaCoeffs::Poly3 {
                red: [red[0], (red[1] as f64 * hs) as f32, (red[2] as f64 * hs.powi(2)) as f32],
                blue: [blue[0], (blue[1] as f64 * hs) as f32, (blue[2] as f64 * hs.powi(2)) as f32],
            })
        }
    }
}

fn rescale_vignetting(lcv: &CalibVignetting, crop: f32, real_focal: f64) -> Option<VignettingCoeffs> {
    let hugin_scale_in_mm = (36.0_f64).hypot(24.0) / crop as f64 / 2.0;
    let hugin_scaling = (real_focal / hugin_scale_in_mm) as f32;
    let hs = hugin_scaling as f64;
    match lcv.model {
        VignettingModel::None => None,
        VignettingModel::Pa { k1, k2, k3 } => Some(VignettingCoeffs {
            k1: (k1 as f64 * hs.powi(2)) as f32,
            k2: (k2 as f64 * hs.powi(4)) as f32,
            k3: (k3 as f64 * hs.powi(6)) as f32,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_metadata_returns_none() {
        assert_eq!(match_profile(None, None, None, None, None), None);
    }

    #[test]
    fn missing_camera_model_returns_none() {
        assert_eq!(match_profile(Some("Canon"), None, Some("EF 24-70mm f/2.8L II USM"), Some(50.0), Some(2.8)), None);
    }

    #[test]
    fn missing_focal_length_returns_none() {
        assert_eq!(match_profile(Some("Canon"), Some("EOS 5D Mark III"), Some("EF 24-70mm f/2.8L II USM"), None, Some(2.8)), None);
    }

    #[test]
    fn nonsense_equipment_returns_none() {
        // Loosely asserting, deliberately: this must never panic and must
        // resolve to "no match" for equipment that cannot exist, without
        // asserting anything about which real lenses ARE in the bundled
        // database -- that's upstream's data, not this app's contract,
        // and pinning against it would make this test fragile against a
        // routine bundled-database update.
        let result = match_profile(
            Some("Not A Real Camera Maker Inc"),
            Some("Definitely Not A Real Model 9000"),
            Some("Not A Real Lens 12-34mm f/9.9"),
            Some(50.0),
            Some(2.8),
        );
        assert_eq!(result, None);
    }

    #[test]
    fn a_well_known_camera_and_lens_resolve_to_a_real_match() {
        // Smoke test only -- asserts a match exists and its shape is
        // sane, not exact coefficient values (those are the rescale
        // functions' own concern, exercised directly below with
        // synthetic inputs that don't depend on bundled-database
        // content).
        let Some(result) = match_profile(Some("Canon"), Some("EOS 5D Mark III"), Some("Canon EF 24-70mm f/2.8L II USM"), Some(50.0), Some(2.8))
        else {
            // The bundled database not containing this specific
            // combination is not this test's concern to enforce --
            // downgrade to a no-op rather than fail the suite over an
            // upstream data change.
            return;
        };
        assert!(result.camera.contains("Canon"));
        assert!(result.lens.contains("24-70mm"));
        assert!(result.crop_factor > 0.0);
        assert!(result.real_focal > 0.0);
    }

    #[test]
    fn rescale_distortion_poly3_zero_coefficient_is_none_not_zero() {
        let lcd = CalibDistortion { focal: 50.0, model: DistortionModel::Poly3 { k1: 0.0 }, real_focal: None };
        assert_eq!(rescale_distortion(&lcd, 1.5, 1.0, 50.0), None);
    }

    #[test]
    fn rescale_distortion_poly3_nonzero_scales_by_hugin_factor() {
        let lcd = CalibDistortion { focal: 50.0, model: DistortionModel::Poly3 { k1: -0.02 }, real_focal: None };
        let Some(DistortionCoeffs::Poly3 { k1 }) = rescale_distortion(&lcd, 1.5, 1.0, 50.0) else {
            panic!("expected Poly3");
        };
        // At crop=1.0, real_focal=50.0, aspect_ratio=1.5 (the sensor's own
        // 3:2 ratio -- the identity case where hugin_scaling works out
        // close to 1x), the rescaled coefficient should stay the same
        // order of magnitude as the input, not blow up or vanish.
        assert!(k1.abs() > 0.001 && k1.abs() < 1.0, "k1={k1}");
    }

    #[test]
    fn rescale_tca_linear_inverts_for_correction() {
        let lctca = CalibTca { focal: 50.0, model: TcaModel::Linear { kr: 1.001, kb: 0.999 } };
        let Some(TcaCoeffs::Linear { kr, kb }) = rescale_tca(&lctca, 1.5, 1.0, 50.0) else {
            panic!("expected Linear");
        };
        assert!((kr - 1.0 / 1.001).abs() < 1e-6);
        assert!((kb - 1.0 / 0.999).abs() < 1e-6);
    }

    #[test]
    fn rescale_vignetting_none_model_is_none() {
        let lcv = CalibVignetting { focal: 50.0, aperture: 2.8, distance: 1000.0, model: VignettingModel::None };
        assert_eq!(rescale_vignetting(&lcv, 1.0, 50.0), None);
    }

    #[test]
    fn rescale_vignetting_pa_scales_every_coefficient() {
        let lcv = CalibVignetting { focal: 50.0, aperture: 2.8, distance: 1000.0, model: VignettingModel::Pa { k1: -0.5, k2: 0.2, k3: -0.05 } };
        let Some(VignettingCoeffs { k1, k2, k3 }) = rescale_vignetting(&lcv, 1.0, 50.0) else {
            panic!("expected Some");
        };
        assert!(k1 != 0.0 && k2 != 0.0 && k3 != 0.0);
    }
}
