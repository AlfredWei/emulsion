//! Object removal on the render side (M7, RFC-0032 §12): putting a photograph's stored patches into a picture of it.
//! A patch is placed in the photograph's full decode (`src_w` x `src_h`); the picture it is applied to may be a smaller
//! decode of the same photograph (a Develop preview), so position and size are scaled by the ratio. Everything outside
//! a patch's fade is left alone. Pure: no catalog, no model.

use crate::ai::inpaint_pipeline::apply_patch;
use crate::catalog::StoredPatch;
use image::{imageops::FilterType, RgbImage, RgbaImage};

/// How far the picture's aspect ratio may differ from the decode the patch was made on before the patch is judged to
/// belong to another picture (a RAW's half-size decode and its full decode round differently by a pixel or two).
const ASPECT_TOLERANCE: f64 = 0.01;

/// The patch's pixels, or `None` when the stored PNG is damaged.
pub fn decode_patch(patch: &StoredPatch) -> Option<RgbaImage> {
    let img = image::load_from_memory_with_format(&patch.png, image::ImageFormat::Png).ok()?.to_rgba8();
    (img.dimensions() == (patch.w, patch.h)).then_some(img)
}

/// `patch` resized by (`sx`, `sy`) with its colour premultiplied by alpha, so a fade does not pick up the colour of the
/// transparent pixels around it.
fn scaled(patch: &RgbaImage, sx: f64, sy: f64) -> RgbaImage {
    let (w, h) = (((patch.width() as f64 * sx).round() as u32).max(1), ((patch.height() as f64 * sy).round() as u32).max(1));
    let mut pre = patch.clone();
    for p in pre.pixels_mut() {
        let a = p.0[3] as u32;
        for c in 0..3 {
            p.0[c] = ((p.0[c] as u32 * a + 127) / 255) as u8;
        }
    }
    let mut out = image::imageops::resize(&pre, w, h, FilterType::Triangle);
    for p in out.pixels_mut() {
        let a = p.0[3] as u32;
        if a > 0 {
            for c in 0..3 {
                p.0[c] = ((p.0[c] as u32 * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
    out
}

/// Lays `patches`, in order, over `img`. Returns how many were applied; one whose PNG is damaged, or whose decode has
/// another aspect ratio than `img` (the photograph was replaced), is skipped, never guessed at.
pub fn apply_removals(img: &mut RgbImage, patches: &[StoredPatch]) -> usize {
    let mut applied = 0;
    for patch in patches {
        let (iw, ih) = img.dimensions();
        if patch.src_w == 0 || patch.src_h == 0 {
            continue;
        }
        let (a, b) = (patch.src_w as f64 * ih as f64, patch.src_h as f64 * iw as f64);
        if (a - b).abs() / a.max(b) > ASPECT_TOLERANCE {
            continue;
        }
        let Some(rgba) = decode_patch(patch) else { continue };
        if (iw, ih) == (patch.src_w, patch.src_h) {
            apply_patch(img, &rgba, patch.x, patch.y);
        } else {
            let (sx, sy) = (iw as f64 / patch.src_w as f64, ih as f64 / patch.src_h as f64);
            apply_patch(img, &scaled(&rgba, sx, sy), (patch.x as f64 * sx).round() as u32, (patch.y as f64 * sy).round() as u32);
        }
        applied += 1;
    }
    applied
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    fn png(img: &RgbaImage) -> Vec<u8> {
        let mut out = std::io::Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    /// An opaque `colour` square, no fade: simple and exact.
    fn square_patch(x: u32, y: u32, side: u32, src: (u32, u32), colour: [u8; 3]) -> StoredPatch {
        let img = RgbaImage::from_pixel(side, side, Rgba([colour[0], colour[1], colour[2], 255]));
        StoredPatch { id: 1, x, y, w: side, h: side, src_w: src.0, src_h: src.1, engine: "t".into(), png: png(&img) }
    }

    #[test]
    fn at_the_decode_size_the_patch_lands_exactly_where_it_was_made() {
        let mut img = RgbImage::from_pixel(400, 300, image::Rgb([10, 10, 10]));
        assert_eq!(apply_removals(&mut img, &[square_patch(100, 50, 20, (400, 300), [200, 0, 0])]), 1);
        assert_eq!(img.get_pixel(100, 50).0, [200, 0, 0]);
        assert_eq!(img.get_pixel(119, 69).0, [200, 0, 0]);
        assert_eq!(img.get_pixel(99, 50).0, [10, 10, 10]);
        assert_eq!(img.get_pixel(120, 70).0, [10, 10, 10]);
    }

    #[test]
    fn on_a_preview_a_quarter_the_size_position_and_size_scale_with_it() {
        let mut preview = RgbImage::from_pixel(100, 75, image::Rgb([10, 10, 10]));
        assert_eq!(apply_removals(&mut preview, &[square_patch(100, 40, 40, (400, 300), [0, 200, 0])]), 1);
        // 40 px at (100, 40) on a 400 x 300 decode is 10 px at (25, 10) on a 100 x 75 preview
        assert_eq!(preview.get_pixel(25, 10).0, [0, 200, 0]);
        assert_eq!(preview.get_pixel(34, 19).0, [0, 200, 0]);
        assert_eq!(preview.get_pixel(24, 10).0, [10, 10, 10]);
        assert_eq!(preview.get_pixel(35, 20).0, [10, 10, 10]);
    }

    #[test]
    fn a_faded_edge_keeps_its_colour_when_scaled_not_a_dark_halo() {
        // an opaque white core with a transparent black ring around it: premultiplied scaling must not darken the edge
        let mut p = RgbaImage::from_pixel(40, 40, Rgba([0, 0, 0, 0]));
        for y in 10..30 {
            for x in 10..30 {
                p.put_pixel(x, y, Rgba([255, 255, 255, 255]));
            }
        }
        let patch = StoredPatch { id: 1, x: 0, y: 0, w: 40, h: 40, src_w: 40, src_h: 40, engine: "t".into(), png: png(&p) };
        let mut img = RgbImage::from_pixel(20, 20, image::Rgb([255, 255, 255]));
        apply_removals(&mut img, &[patch]);
        assert!(img.pixels().all(|px| px.0 == [255, 255, 255]), "white on white must stay white at every scaled edge pixel");
    }

    #[test]
    fn a_patch_of_another_picture_or_a_damaged_one_is_skipped_and_the_rest_still_apply() {
        let mut img = RgbImage::from_pixel(400, 300, image::Rgb([10, 10, 10]));
        let wrong_aspect = square_patch(0, 0, 20, (300, 400), [1, 2, 3]);
        let damaged = StoredPatch { png: vec![1, 2, 3], ..square_patch(0, 0, 20, (400, 300), [1, 2, 3]) };
        let wrong_size_png = StoredPatch { w: 21, ..square_patch(0, 0, 20, (400, 300), [1, 2, 3]) };
        let good = square_patch(300, 200, 10, (400, 300), [9, 9, 9]);
        assert_eq!(apply_removals(&mut img, &[wrong_aspect, damaged, wrong_size_png, good]), 1);
        assert_eq!(img.get_pixel(0, 0).0, [10, 10, 10]);
        assert_eq!(img.get_pixel(300, 200).0, [9, 9, 9]);
    }

    #[test]
    fn a_patch_hanging_off_the_picture_is_cut_not_a_panic() {
        let mut img = RgbImage::from_pixel(100, 100, image::Rgb([10, 10, 10]));
        assert_eq!(apply_removals(&mut img, &[square_patch(90, 90, 20, (100, 100), [5, 5, 5])]), 1);
        assert_eq!(img.get_pixel(99, 99).0, [5, 5, 5]);
    }
}
