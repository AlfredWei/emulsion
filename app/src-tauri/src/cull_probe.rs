//! Measurement probe for RFC-0031 (intelligent culling): how well do a cheap sharpness score and a cheap perceptual
//! hash separate what a photographer would call blurred / duplicate from what they would not? Ignored test, run by hand:
//!   CULL_PROBE_DIR=/path/with/jpegs cargo test --release --lib cull_probe -- --ignored --nocapture
//! Not product code; the RFC quotes its output.

use crate::cull::{analysis_gray, dhash, hamming, sharpness, ANALYSIS_EDGE};
use image::{imageops, GrayImage, RgbImage};
use std::path::PathBuf;

fn load_gray(path: &std::path::Path) -> GrayImage {
    let rgb = crate::jpeg_decode::decode(path).expect("decode");
    analysis_gray(&RgbImage::from_raw(rgb.width, rgb.height, rgb.rgb).unwrap())
}

fn files() -> Vec<PathBuf> {
    let dir = std::env::var("CULL_PROBE_DIR").expect("set CULL_PROBE_DIR");
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.extension().map(|e| e.to_string_lossy().to_lowercase()).is_some_and(|e| e == "jpg" || e == "jpeg")
        })
        .collect();
    v.sort();
    v
}

#[test]
#[ignore]
fn cull_probe_blur() {
    println!("sharpness (Laplacian variance at {ANALYSIS_EDGE}px long edge): whole-frame / 90th-pct tile");
    for p in files() {
        let g = load_gray(&p);
        let (w0, t0) = sharpness(&g);
        print!("{:50} orig {:8.1}/{:8.1}", p.file_name().unwrap().to_string_lossy(), w0, t0);
        for sigma in [0.6f32, 1.0, 1.6, 2.5] {
            let b = imageops::blur(&g, sigma);
            let (w, t) = sharpness(&b);
            print!("  s{sigma}: {:.0}%/{:.0}%", 100.0 * w / w0.max(1e-9), 100.0 * t / t0.max(1e-9));
        }
        println!();
    }
}

#[test]
#[ignore]
fn cull_probe_duplicates() {
    let paths = files();
    let grays: Vec<GrayImage> = paths.iter().map(|p| load_gray(p)).collect();
    let hashes: Vec<[u8; 32]> = grays.iter().map(dhash).collect();
    println!("--- same photo, edited (distance of 256 bits) ---");
    for (p, g) in paths.iter().zip(&grays) {
        let h = dhash(g);
        let (w, hgt) = g.dimensions();
        let half = imageops::resize(g, w / 2, hgt / 2, imageops::FilterType::Triangle);
        let bright = GrayImage::from_fn(w, hgt, |x, y| image::Luma([(g.get_pixel(x, y)[0] as f32 * 1.25).min(255.0) as u8]));
        let crop = imageops::crop_imm(g, w / 40, hgt / 40, w - w / 20, hgt - hgt / 20).to_image();
        let shift = imageops::crop_imm(g, w / 25, 0, w - w / 25, hgt).to_image(); // 4% sideways step, as a burst's pan
        let blurred = imageops::blur(g, 1.6);
        println!(
            "{:50} half {:3}  +25% exposure {:3}  crop 5% {:3}  shift 4% {:3}  blur {:3}",
            p.file_name().unwrap().to_string_lossy(),
            hamming(&h, &dhash(&half)),
            hamming(&h, &dhash(&bright)),
            hamming(&h, &dhash(&crop)),
            hamming(&h, &dhash(&shift)),
            hamming(&h, &dhash(&blurred)),
        );
    }
    println!("--- different photos (min / median over all pairs) ---");
    let mut d = Vec::new();
    for i in 0..hashes.len() {
        for j in (i + 1)..hashes.len() {
            d.push((hamming(&hashes[i], &hashes[j]), i, j));
        }
    }
    d.sort();
    for (dist, i, j) in d.iter().take(5) {
        println!("{dist:3}  {} <-> {}", paths[*i].file_name().unwrap().to_string_lossy(), paths[*j].file_name().unwrap().to_string_lossy());
    }
    println!("median {}", d[d.len() / 2].0);
}

/// What the measurement costs once the 1024 px picture exists (the decode is not counted; it already happens).
#[test]
#[ignore]
fn cull_probe_cost() {
    use crate::cull::analyse_gray;
    let paths = files();
    let rgbs: Vec<RgbImage> = paths
        .iter()
        .map(|p| {
            let d = crate::jpeg_decode::decode(p).expect("decode");
            RgbImage::from_raw(d.width, d.height, d.rgb).unwrap()
        })
        .collect();
    let t = std::time::Instant::now();
    let grays: Vec<GrayImage> = rgbs.iter().map(analysis_gray).collect();
    let reduce = t.elapsed();
    let t = std::time::Instant::now();
    for g in &grays {
        std::hint::black_box(analyse_gray(g));
    }
    let measure = t.elapsed();
    let n = rgbs.len() as f64;
    println!(
        "{} photos: reduce to 1024 gray {:.1} ms each, sharpness+hash {:.1} ms each",
        rgbs.len(),
        reduce.as_secs_f64() * 1e3 / n,
        measure.as_secs_f64() * 1e3 / n
    );
}
