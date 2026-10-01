//! RFC-0021 appendix: compare the app's lens-correction coordinate mapping
//! against the upstream `lensfun` crate's own `Modifier`, for the same lens,
//! focal length and image size. NOT part of the build -- a measurement
//! harness. To run it:
//!
//!   cp docs/rfc/RFC-0021-appendix/lens_direction_check.rs \
//!      app/src-tauri/src/develop_engine/tests/zz_lens_check.rs
//!   echo "mod zz_lens_check;" >> app/src-tauri/src/develop_engine/tests/mod.rs
//!   cd app/src-tauri && cargo test --lib zz_lens_check -- --nocapture
//!   # then `git checkout app/src-tauri/src/develop_engine/tests/mod.rs` and delete the copy
//!
//! Reading the output: upstream's `Modifier::new(.., reverse)` is built with
//! BOTH values of `reverse`. The lensfun crate's own correction tests, the
//! upstream default, and darktable all use `reverse = false` to CORRECT an
//! image (the crate's doc comment on `Modifier::new` says the opposite; its
//! dispatcher and its tests agree with each other, not with that comment).
//! The app is then compared against both.

use super::*;
use lensfun::{Database, Modifier};

const MAKER: &str = "Canon";
const CAMERA: &str = "EOS 5D Mark III";
const LENS: &str = "Canon EF 24-70mm f/2.8L II USM";
const SIZE: (u32, u32) = (6000, 4000);

fn setup(focal: f32) -> (Database, crate::lens_profile::LensProfileMatch) {
    let db = Database::load_bundled().unwrap();
    let m = crate::lens_profile::match_profile(Some(MAKER), Some(CAMERA), Some(LENS), Some(focal), Some(2.8)).unwrap();
    (db, m)
}

fn app_correction(m: &crate::lens_profile::LensProfileMatch, extra: serde_json::Value) -> LensCorrection {
    let mut op = serde_json::json!({"op": "lens_correction", "profile_enabled": true});
    op["profile"] = serde_json::to_value(m).unwrap();
    for (k, v) in extra.as_object().unwrap() {
        op[k] = v.clone();
    }
    lens_correction_op(&[op])
}

/// 1. Distortion: source radius for a few output pixels. Expect the app to
///    equal `reverse=true`, not `reverse=false`.
#[test]
fn distortion_direction() {
    let (w, h) = SIZE;
    for focal in [24.0f32, 70.0] {
        let (db, m) = setup(focal);
        let cams = db.find_cameras(Some(MAKER), CAMERA);
        let lens = *db.find_lenses(cams.first().copied(), LENS).first().unwrap();
        let lc = app_correction(&m, serde_json::json!({}));
        let norm = LensNorm::for_profile(lc.profile.as_ref().unwrap(), w, h);
        let manual = LensNorm::for_manual(w, h);
        let c = (w as f32 / 2.0, h as f32 / 2.0);
        let rr = |p: (f32, f32)| (p.0 - c.0).hypot(p.1 - c.1);
        eprintln!("== {LENS} @ {focal}mm, profile.distortion = {:?}", m.distortion);
        for (px, py) in [(100.0f32, 100.0f32), (1000.0, 700.0), (3000.0, 1000.0), (5900.0, 3900.0)] {
            let app = lens_correct_coord(px, py, LensChannel::Green, &lc, Some(norm), manual);
            let mut up = Vec::new();
            for reverse in [false, true] {
                let mut md = Modifier::new(lens, focal, m.crop_factor, w, h, reverse);
                assert!(md.enable_distortion_correction(lens));
                let mut co = [0.0f32; 2];
                md.apply_geometry_distortion(px, py, 1, 1, &mut co);
                up.push((co[0], co[1]));
            }
            eprintln!(
                "out ({px},{py}) r_out={:.1} | app r_src={:.1} | upstream reverse=false {:.1} | reverse=true {:.1}",
                rr((px, py)), rr(app), rr(up[0]), rr(up[1])
            );
        }
    }
}

/// 2. TCA and vignetting at the same pixels. Expect: TCA app == reverse=true
///    (wrong), vignetting app == reverse=false (right).
#[test]
fn tca_and_vignette_direction() {
    let (w, h) = SIZE;
    let focal = 24.0f32;
    let (db, m) = setup(focal);
    let cams = db.find_cameras(Some(MAKER), CAMERA);
    let lens = *db.find_lenses(cams.first().copied(), LENS).first().unwrap();
    eprintln!("tca = {:?}  vignetting = {:?}", m.tca, m.vignetting);
    let lc = app_correction(&m, serde_json::json!({"distortion_amount": 0.0}));
    let norm = LensNorm::for_profile(lc.profile.as_ref().unwrap(), w, h);
    let manual = LensNorm::for_manual(w, h);
    let c = (w as f32 / 2.0, h as f32 / 2.0);
    let rr = |x: f32, y: f32| (x - c.0).hypot(y - c.1);
    for (px, py) in [(100.0f32, 100.0f32), (1000.0, 700.0)] {
        let (rx, ry) = lens_correct_coord(px, py, LensChannel::Red, &lc, Some(norm), manual);
        let (bx, by) = lens_correct_coord(px, py, LensChannel::Blue, &lc, Some(norm), manual);
        let mut up = Vec::new();
        for reverse in [false, true] {
            let mut md = Modifier::new(lens, focal, m.crop_factor, w, h, reverse);
            assert!(md.enable_tca_correction(lens));
            let mut co = [0.0f32; 6];
            md.apply_subpixel_distortion(px, py, 1, 1, &mut co);
            up.push((rr(co[0], co[1]), rr(co[4], co[5])));
        }
        eprintln!(
            "TCA ({px},{py}) r_out={:.2} | app red={:.2} blue={:.2} | upstream reverse=false red={:.2} blue={:.2} | reverse=true red={:.2} blue={:.2}",
            rr(px, py), rr(rx, ry), rr(bx, by), up[0].0, up[0].1, up[1].0, up[1].1
        );
        let (nx, ny) = norm.to_normalized(px, py);
        let r2 = nx * nx + ny * ny;
        let v = lc.profile.unwrap().vignetting.unwrap();
        let app_mult = 1.0 / (1.0 + v.k1 * r2 + v.k2 * r2 * r2 + v.k3 * r2 * r2 * r2).max(0.01);
        let mut ups = Vec::new();
        for reverse in [false, true] {
            let mut md = Modifier::new(lens, focal, m.crop_factor, w, h, reverse);
            assert!(md.enable_vignetting_correction(lens, 2.8, 1000.0));
            let mut p1 = [1.0f32; 3];
            md.apply_color_modification_f32(&mut p1, px, py, 1, 1, 3);
            ups.push(p1[0]);
        }
        eprintln!("VIG ({px},{py}) app mult={app_mult:.4} | upstream reverse=false {:.4} | reverse=true {:.4}", ups[0], ups[1]);
    }
}

/// 3. Radial source/output ratio along a ~34-degree ray, and the fraction of
///    output pixels whose source falls outside the frame (those render as
///    black on the CPU path and smeared edge pixels on the GPU path).
#[test]
fn radial_ratio_and_out_of_frame() {
    let (w, h) = SIZE;
    let c = (w as f32 / 2.0, h as f32 / 2.0);
    let half_diag = c.0.hypot(c.1);
    for focal in [24.0f32, 35.0, 50.0, 70.0] {
        let (db, m) = setup(focal);
        let cams = db.find_cameras(Some(MAKER), CAMERA);
        let lens = *db.find_lenses(cams.first().copied(), LENS).first().unwrap();
        let lc = app_correction(&m, serde_json::json!({"vignette_amount": 0.0, "ca_amount": 0.0}));
        let norm = LensNorm::for_profile(lc.profile.as_ref().unwrap(), w, h);
        let manual = LensNorm::for_manual(w, h);
        let mut up = Modifier::new(lens, focal, m.crop_factor, w, h, false);
        up.enable_distortion_correction(lens);
        let mut line = format!("{focal:>4}mm src/out radius ratio at 0.25,0.5,0.75,1.0 x half-diagonal: upstream(correct)");
        let mut app_s = String::from("  app");
        for f in [0.25f32, 0.5, 0.75, 1.0] {
            let (px, py) = (c.0 + f * half_diag * 0.832, c.1 + f * half_diag * 0.555);
            let r = (px - c.0).hypot(py - c.1);
            let mut co = [0.0f32; 2];
            up.apply_geometry_distortion(px, py, 1, 1, &mut co);
            let (ax, ay) = lens_correct_coord(px, py, LensChannel::Green, &lc, Some(norm), manual);
            line += &format!(" {:.4}", (co[0] - c.0).hypot(co[1] - c.1) / r);
            app_s += &format!(" {:.4}", (ax - c.0).hypot(ay - c.1) / r);
        }
        let (mut oob_up, mut oob_app, mut n) = (0, 0, 0);
        let out = |x: f32, y: f32| x < 0.0 || y < 0.0 || x > (w - 1) as f32 || y > (h - 1) as f32;
        for iy in 0..80 {
            for ix in 0..120 {
                let (px, py) = ((ix as f32 + 0.5) * w as f32 / 120.0, (iy as f32 + 0.5) * h as f32 / 80.0);
                let mut co = [0.0f32; 2];
                up.apply_geometry_distortion(px, py, 1, 1, &mut co);
                let (ax, ay) = lens_correct_coord(px, py, LensChannel::Green, &lc, Some(norm), manual);
                oob_up += out(co[0], co[1]) as u32;
                oob_app += out(ax, ay) as u32;
                n += 1;
            }
        }
        eprintln!(
            "{line}{app_s} | out-of-frame samples: upstream {:.2}% app {:.2}%",
            100.0 * oob_up as f32 / n as f32,
            100.0 * oob_app as f32 / n as f32
        );
    }
}
