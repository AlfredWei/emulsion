//! RFC-0021 appendix: writes /tmp/lens_ref.json, the CPU-path reference that
//! `gpu_lens_check.js` compares the real GPU output against. Not part of the
//! build:
//!
//!   cp docs/rfc/RFC-0021-appendix/lens_cpu_reference.rs \
//!      app/src-tauri/src/develop_engine/tests/zz_ref.rs
//!   echo "mod zz_ref;" >> app/src-tauri/src/develop_engine/tests/mod.rs
//!   cd app/src-tauri && cargo test --lib zz_write_lens_reference
//!   # then restore tests/mod.rs and delete the copy
//!
//! Four cases on a 240x160 synthetic image, 49 sampled output pixels each:
//! Canon 24-70 at 24 mm with every correction, at 70 mm (corners fall outside
//! the frame), 24 mm vignetting only on uniform gray 128, and the manual
//! distortion + CA sliders with no profile.

use super::support::*;
use super::*;

fn gradient(w: u32, h: u32) -> RgbImage {
    let mut img = RgbImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            img.put_pixel(x, y, image::Rgb([((x * 255) / (w - 1)) as u8, ((y * 255) / (h - 1)) as u8, (((x + y) * 255) / (w + h - 2)) as u8]));
        }
    }
    img
}

#[test]
fn zz_write_lens_reference() {
    let (w, h) = (240u32, 160u32);
    let mut cases = Vec::new();
    for (name, focal, kind, extra) in [
        ("canon24_all", 24.0f32, "gradient", serde_json::json!({})),
        ("canon70_all_oob", 70.0, "gradient", serde_json::json!({})),
        ("canon24_vignette_only", 24.0, "gray128", serde_json::json!({"distortion_amount": 0, "ca_amount": 0})),
        ("manual", 24.0, "gradient", serde_json::json!({"profile_enabled": false, "manual_distortion": 50, "manual_ca": 30})),
    ] {
        let m = crate::lens_profile::match_profile(Some("Canon"), Some("EOS 5D Mark III"), Some("Canon EF 24-70mm f/2.8L II USM"), Some(focal), Some(2.8)).unwrap();
        let mut op = serde_json::json!({"profile_enabled": true, "distortion_amount": 100, "ca_amount": 100, "vignette_amount": 100});
        op["profile"] = serde_json::to_value(&m).unwrap();
        for (k, v) in extra.as_object().unwrap() { op[k] = v.clone(); }
        let mut img = if kind == "gradient" { gradient(w, h) } else { RgbImage::from_pixel(w, h, image::Rgb([128, 128, 128])) };
        apply_lens_correction(&mut img, &lens_correction_op_json(op.clone()));
        let mut pts = Vec::new();
        for &y in &[0u32, 5, 40, 80, 120, 154, 159] {
            for &x in &[0u32, 7, 60, 120, 180, 232, 239] {
                let p = img.get_pixel(x, y).0;
                pts.push(serde_json::json!([x, y, p[0], p[1], p[2]]));
            }
        }
        cases.push(serde_json::json!({"name": name, "kind": kind, "w": w, "h": h, "op": op, "points": pts}));
    }
    std::fs::write("/tmp/lens_ref.json", serde_json::to_string(&cases).unwrap()).unwrap();
}
