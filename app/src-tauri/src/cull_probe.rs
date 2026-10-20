//! Measurement probe for RFC-0031 (intelligent culling): how well do a cheap sharpness score and a cheap perceptual
//! hash separate what a photographer would call blurred / duplicate from what they would not? Ignored test, run by hand:
//!   CULL_PROBE_DIR=/path/with/jpegs cargo test --release --lib cull_probe -- --ignored --nocapture
//! Not product code; the RFC quotes its output.

use image::{imageops, GrayImage, RgbImage};
use std::path::PathBuf;

const ANALYSIS_EDGE: u32 = 1024;

fn load_gray(path: &std::path::Path) -> GrayImage {
    let rgb = crate::jpeg_decode::decode(path).expect("decode");
    let img = RgbImage::from_raw(rgb.width, rgb.height, rgb.rgb).unwrap();
    to_analysis(&img)
}

fn to_analysis(img: &RgbImage) -> GrayImage {
    let (w, h) = img.dimensions();
    let scale = ANALYSIS_EDGE as f32 / w.max(h) as f32;
    let (nw, nh) = (((w as f32 * scale) as u32).max(8), ((h as f32 * scale) as u32).max(8));
    let small = imageops::resize(img, nw, nh, imageops::FilterType::Triangle);
    imageops::grayscale(&small)
}

/// Variance of the 4-neighbour Laplacian over a rectangle.
fn laplacian_var(g: &GrayImage, x0: u32, y0: u32, x1: u32, y1: u32) -> f64 {
    let (mut sum, mut sum2, mut n) = (0f64, 0f64, 0f64);
    for y in (y0.max(1))..(y1.min(g.height() - 1)) {
        for x in (x0.max(1))..(x1.min(g.width() - 1)) {
            let c = g.get_pixel(x, y)[0] as f64;
            let l = g.get_pixel(x - 1, y)[0] as f64
                + g.get_pixel(x + 1, y)[0] as f64
                + g.get_pixel(x, y - 1)[0] as f64
                + g.get_pixel(x, y + 1)[0] as f64
                - 4.0 * c;
            sum += l;
            sum2 += l * l;
            n += 1.0;
        }
    }
    if n == 0.0 {
        return 0.0;
    }
    let mean = sum / n;
    sum2 / n - mean * mean
}

/// (whole-frame score, 8x8-grid 90th-percentile score)
fn sharpness(g: &GrayImage) -> (f64, f64) {
    let whole = laplacian_var(g, 0, 0, g.width(), g.height());
    let (tw, th) = (g.width() / 8, g.height() / 8);
    let mut tiles = Vec::new();
    for ty in 0..8 {
        for tx in 0..8 {
            tiles.push(laplacian_var(g, tx * tw, ty * th, (tx + 1) * tw, (ty + 1) * th));
        }
    }
    tiles.sort_by(|a, b| a.partial_cmp(b).unwrap());
    (whole, tiles[(tiles.len() as f64 * 0.9) as usize])
}

/// 16x16 difference hash = 256 bits (17x16 samples, each bit = left brighter than right).
fn dhash(g: &GrayImage) -> Vec<bool> {
    let s = imageops::resize(g, 17, 16, imageops::FilterType::Triangle);
    let mut bits = Vec::with_capacity(256);
    for y in 0..16 {
        for x in 0..16 {
            bits.push(s.get_pixel(x, y)[0] > s.get_pixel(x + 1, y)[0]);
        }
    }
    bits
}

fn hamming(a: &[bool], b: &[bool]) -> usize {
    a.iter().zip(b).filter(|(x, y)| x != y).count()
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
    let hashes: Vec<Vec<bool>> = grays.iter().map(dhash).collect();
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
