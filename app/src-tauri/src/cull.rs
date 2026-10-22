//! Culling analysis (RFC-0031 slice A): for every photo, once, a sharpness score and a perceptual hash computed from
//! its 1024 px picture. Pure arithmetic, no model. The numbers are only meaningful *relative to each other* (a photo
//! against its siblings from the same moment): see RFC-0031 §2, where the same score ranged from 50 to 1,714 on sharp
//! photos. Grouping and the "Soft" rule are slice B; this module computes and stores.

use crate::catalog::Catalog;
use crate::import;
use image::{imageops, GrayImage, RgbImage};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Version of the metric. Changing the score or the hash bumps it, and rows of an older version are recomputed.
pub const ALGO_VERSION: i64 = 1;

/// The picture is reduced to this long edge before measuring, so scores are comparable between photos of any size.
pub const ANALYSIS_EDGE: u32 = 1024;
/// A stored thumbnail shorter than this (some cameras' embedded previews are tiny) is not measured; the source is.
const MIN_THUMBNAIL_EDGE: u32 = 640;

#[derive(Debug, Clone, PartialEq)]
pub struct CullMetrics {
    /// Variance of the 4-neighbour Laplacian over the whole frame.
    pub sharpness: f64,
    /// The same, 90th percentile over an 8x8 tile grid: a sharp subject on a soft background still scores.
    pub sharpness_tile: f64,
    /// 16x16 difference hash, 256 bits.
    pub dhash: [u8; 32],
}

/// The grayscale picture the metrics are taken from.
pub fn analysis_gray(img: &RgbImage) -> GrayImage {
    let (w, h) = img.dimensions();
    let long = w.max(h);
    if long <= ANALYSIS_EDGE {
        return imageops::grayscale(img);
    }
    let scale = ANALYSIS_EDGE as f32 / long as f32;
    let (nw, nh) = (((w as f32 * scale) as u32).max(8), ((h as f32 * scale) as u32).max(8));
    imageops::grayscale(&imageops::resize(img, nw, nh, imageops::FilterType::Triangle))
}

pub fn analyse_gray(g: &GrayImage) -> CullMetrics {
    let (sharpness, sharpness_tile) = sharpness(g);
    CullMetrics { sharpness, sharpness_tile, dhash: dhash(g) }
}

/// Variance of the Laplacian over the pixels strictly inside `[x0,x1) x [y0,y1)` that have all four neighbours.
fn laplacian_var(g: &GrayImage, x0: u32, y0: u32, x1: u32, y1: u32) -> f64 {
    let (w, h) = (g.width() as usize, g.height() as usize);
    let raw = g.as_raw();
    let (x0, x1) = ((x0 as usize).max(1), (x1 as usize).min(w.saturating_sub(1)));
    let (y0, y1) = ((y0 as usize).max(1), (y1 as usize).min(h.saturating_sub(1)));
    if x0 >= x1 || y0 >= y1 {
        return 0.0;
    }
    let (mut sum, mut sum2) = (0f64, 0f64);
    for y in y0..y1 {
        let row = y * w;
        for x in x0..x1 {
            let c = raw[row + x] as i32;
            let l = raw[row + x - 1] as i32 + raw[row + x + 1] as i32 + raw[row - w + x] as i32 + raw[row + w + x] as i32 - 4 * c;
            sum += l as f64;
            sum2 += (l * l) as f64;
        }
    }
    let n = ((x1 - x0) * (y1 - y0)) as f64;
    let mean = sum / n;
    sum2 / n - mean * mean
}

/// (whole-frame score, 90th-percentile tile score)
pub fn sharpness(g: &GrayImage) -> (f64, f64) {
    let whole = laplacian_var(g, 0, 0, g.width(), g.height());
    let (tw, th) = (g.width() / 8, g.height() / 8);
    let mut tiles = Vec::with_capacity(64);
    for ty in 0..8 {
        for tx in 0..8 {
            tiles.push(laplacian_var(g, tx * tw, ty * th, (tx + 1) * tw, (ty + 1) * th));
        }
    }
    tiles.sort_by(|a, b| a.total_cmp(b));
    (whole, tiles[(tiles.len() as f64 * 0.9) as usize])
}

/// Each bit: is this sample brighter than its right neighbour, over a 17x16 reduction.
pub fn dhash(g: &GrayImage) -> [u8; 32] {
    let s = imageops::resize(g, 17, 16, imageops::FilterType::Triangle);
    let mut out = [0u8; 32];
    for y in 0..16u32 {
        for x in 0..16u32 {
            if s.get_pixel(x, y)[0] > s.get_pixel(x + 1, y)[0] {
                let bit = (y * 16 + x) as usize;
                out[bit / 8] |= 1 << (bit % 8);
            }
        }
    }
    out
}

/// Number of differing bits, 0 to 256.
pub fn hamming(a: &[u8; 32], b: &[u8; 32]) -> u32 {
    a.iter().zip(b).map(|(x, y)| (x ^ y).count_ones()).sum()
}

/// The measuring input for one photo: its plain `{id}.jpg` thumbnail when that is big enough (an unedited picture a
/// few milliseconds to decode; an edited one is `{id}-{hash}.jpg` and never used), else the source file.
fn load_for_analysis(image_id: i64, source: &Path, thumbnail_dir: &Path) -> Option<GrayImage> {
    let thumb = thumbnail_dir.join(format!("{image_id}.jpg"));
    if let Ok(img) = image::open(&thumb) {
        let rgb = img.to_rgb8();
        if rgb.width().max(rgb.height()) >= MIN_THUMBNAIL_EDGE {
            return Some(analysis_gray(&rgb));
        }
    }
    let decoded = crate::source_decode::decode_preview(source).ok()?;
    let rgb = RgbImage::from_raw(decoded.width, decoded.height, decoded.rgb)?;
    Some(analysis_gray(&import::downscale_to(rgb, ANALYSIS_EDGE)))
}

/// Measures every photo that has no row of the current `ALGO_VERSION`. Quiet background work: two workers, waiting
/// while an import is running (like the large-thumbnail pass), catalog writes on the calling thread. A photo that
/// cannot be read leaves no row and is tried again next time. Returns how many rows were written.
pub fn analyse_missing<F: FnMut(usize, usize)>(catalog: &Arc<Mutex<Catalog>>, thumbnail_dir: &Path, mut on_progress: F) -> usize {
    use std::sync::atomic::{AtomicUsize, Ordering};

    let todo: Vec<(i64, PathBuf)> = {
        let Ok(catalog) = catalog.lock() else { return 0 };
        catalog.images_needing_cull_analysis(ALGO_VERSION).unwrap_or_default().into_iter().map(|(id, p)| (id, PathBuf::from(p))).collect()
    };
    let total = todo.len();
    if total == 0 {
        return 0;
    }
    let workers = import::BACKGROUND_WORKERS.clamp(1, total);
    let next = AtomicUsize::new(0);
    let (tx, rx) = std::sync::mpsc::channel::<(i64, Option<CullMetrics>)>();
    let mut written = 0;
    std::thread::scope(|scope| {
        for _ in 0..workers {
            let tx = tx.clone();
            let (todo, next) = (&todo, &next);
            scope.spawn(move || loop {
                import::wait_for_foreground_work();
                let Some((id, path)) = todo.get(next.fetch_add(1, Ordering::SeqCst)) else { break };
                // A decoder that panics on a hostile file costs that one photo, not the pass.
                let metrics = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| load_for_analysis(*id, path, thumbnail_dir)))
                    .unwrap_or_else(|_| {
                        eprintln!("culling analysis panicked for {}", path.display());
                        None
                    })
                    .map(|g| analyse_gray(&g));
                if tx.send((*id, metrics)).is_err() {
                    break;
                }
            });
        }
        drop(tx);
        for (done, (id, metrics)) in rx.iter().enumerate() {
            if let Some(m) = metrics {
                if catalog.lock().ok().is_some_and(|c| c.set_cull_analysis(id, &m, ALGO_VERSION).is_ok()) {
                    written += 1;
                }
            }
            on_progress(done + 1, total);
        }
    });
    written
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Luma;

    /// Blocks of random-looking grey (deterministic per seed): edges everywhere, and the same picture at any size.
    fn blocks(seed: u32, w: u32, h: u32) -> GrayImage {
        let block = w / 32;
        GrayImage::from_fn(w, h, |x, y| {
            let (bx, by) = (x / block.max(1), y / block.max(1));
            let mut v = bx.wrapping_mul(73856093) ^ by.wrapping_mul(19349663) ^ seed.wrapping_mul(83492791);
            v ^= v >> 13;
            v = v.wrapping_mul(0x5bd1e995);
            Luma([(v >> 8) as u8])
        })
    }

    #[test]
    fn sharpness_falls_as_blur_grows() {
        let g = blocks(1, 512, 384);
        let scores: Vec<f64> = [0.0f32, 0.6, 1.0, 1.6].iter().map(|&s| sharpness(&if s == 0.0 { g.clone() } else { imageops::blur(&g, s) }).1).collect();
        assert!(scores.windows(2).all(|w| w[0] > w[1]), "scores must fall monotonically: {scores:?}");
        assert!(scores[1] < scores[0] * 0.6, "a 0.6 px blur must cost well over 40%: {scores:?}");
    }

    #[test]
    fn flat_picture_scores_zero_and_tiny_picture_does_not_panic() {
        let flat = GrayImage::from_pixel(64, 64, Luma([100]));
        assert_eq!(sharpness(&flat), (0.0, 0.0));
        let tiny = GrayImage::from_pixel(2, 2, Luma([100]));
        assert_eq!(sharpness(&tiny), (0.0, 0.0));
    }

    #[test]
    fn hash_survives_resize_exposure_and_blur_but_not_a_different_photo() {
        let a = blocks(1, 512, 384);
        let h = dhash(&a);
        let half = imageops::resize(&a, 256, 192, imageops::FilterType::Triangle);
        let bright = GrayImage::from_fn(512, 384, |x, y| Luma([(a.get_pixel(x, y)[0] as f32 * 0.8 + 30.0) as u8]));
        let soft = imageops::blur(&a, 1.5);
        assert!(hamming(&h, &dhash(&half)) <= 16, "resize");
        assert!(hamming(&h, &dhash(&bright)) <= 16, "exposure");
        assert!(hamming(&h, &dhash(&soft)) <= 32, "blur");
        assert!(hamming(&h, &dhash(&blocks(2, 512, 384))) >= 80, "a different picture");
    }

    #[test]
    fn hamming_counts_differing_bits() {
        let mut a = [0u8; 32];
        let mut b = [0u8; 32];
        assert_eq!(hamming(&a, &b), 0);
        a[0] = 0b1011;
        b[31] = 0xFF;
        assert_eq!(hamming(&a, &b), 3 + 8);
    }

    #[test]
    fn analysis_picture_is_capped_at_the_analysis_edge() {
        let big = RgbImage::from_pixel(3000, 2000, image::Rgb([9, 9, 9]));
        let g = analysis_gray(&big);
        assert_eq!(g.width().max(g.height()), ANALYSIS_EDGE);
        let small = RgbImage::from_pixel(300, 200, image::Rgb([9, 9, 9]));
        assert_eq!(analysis_gray(&small).dimensions(), (300, 200));
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("emulsion-cull-test-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn save_jpeg(path: &Path, seed: u32, w: u32, h: u32) {
        let g = blocks(seed, w, h);
        RgbImage::from_fn(w, h, |x, y| {
            let v = g.get_pixel(x, y)[0];
            image::Rgb([v, v, v])
        })
        .save(path)
        .unwrap();
    }

    #[test]
    fn pass_measures_from_the_thumbnail_when_it_is_big_enough_and_from_the_source_otherwise() {
        let dir = scratch("pass");
        let thumbs = dir.join("thumbs");
        std::fs::create_dir_all(&thumbs).unwrap();
        let catalog = Catalog::open_in_memory().unwrap();

        // 1: a big thumbnail exists and the source does not: the thumbnail must be what is measured.
        let one = catalog.add_image(&dir.join("gone.jpg").to_string_lossy()).unwrap();
        save_jpeg(&thumbs.join(format!("{one}.jpg")), 1, 800, 600);
        // 2: no thumbnail: the source is decoded.
        let src2 = dir.join("two.jpg");
        save_jpeg(&src2, 2, 700, 500);
        let two = catalog.add_image(&src2.to_string_lossy()).unwrap();
        // 3: a thumbnail that is too small: the source wins.
        let src3 = dir.join("three.jpg");
        save_jpeg(&src3, 3, 900, 600);
        let three = catalog.add_image(&src3.to_string_lossy()).unwrap();
        save_jpeg(&thumbs.join(format!("{three}.jpg")), 9, 160, 120);
        // 4: nothing readable at all: no row, and the others are unaffected.
        let four = catalog.add_image(&dir.join("missing.jpg").to_string_lossy()).unwrap();

        let catalog = Arc::new(Mutex::new(catalog));
        let progress = std::cell::Cell::new((0, 0));
        let written = analyse_missing(&catalog, &thumbs, |d, t| progress.set((d, t)));
        assert_eq!(written, 3);
        assert_eq!(progress.get(), (4, 4), "every candidate reports progress, readable or not");

        let c = catalog.lock().unwrap();
        assert!(c.get_cull_analysis(one).unwrap().is_some());
        assert!(c.get_cull_analysis(two).unwrap().is_some());
        assert!(c.get_cull_analysis(four).unwrap().is_none());
        // Photo 3 was measured from its 900 px source (seed 3), not from the 160 px thumbnail (seed 9).
        let expected = analyse_gray(&analysis_gray(&image::open(&src3).unwrap().to_rgb8()));
        let got = c.get_cull_analysis(three).unwrap().unwrap();
        assert_eq!(got.dhash, expected.dhash);
        drop(c);

        // Second run: only the unreadable one is retried; nothing is rewritten.
        let again = analyse_missing(&catalog, &thumbs, |_, _| {});
        assert_eq!(again, 0);
    }

    #[test]
    fn rows_of_an_older_algorithm_are_measured_again() {
        let dir = scratch("algo");
        let thumbs = dir.join("thumbs");
        std::fs::create_dir_all(&thumbs).unwrap();
        let catalog = Catalog::open_in_memory().unwrap();
        let id = catalog.add_image(&dir.join("x.jpg").to_string_lossy()).unwrap();
        save_jpeg(&thumbs.join(format!("{id}.jpg")), 4, 800, 600);
        let stale = CullMetrics { sharpness: 1.0, sharpness_tile: 1.0, dhash: [0; 32] };
        catalog.set_cull_analysis(id, &stale, ALGO_VERSION - 1).unwrap();

        let catalog = Arc::new(Mutex::new(catalog));
        assert_eq!(analyse_missing(&catalog, &thumbs, |_, _| {}), 1);
        let got = catalog.lock().unwrap().get_cull_analysis(id).unwrap().unwrap();
        assert_ne!(got.dhash, [0; 32]);
    }
}
