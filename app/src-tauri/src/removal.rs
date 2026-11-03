//! Object removal on the render side (M7, RFC-0032 §12): putting a photograph's stored patches into a picture of it.
//! A patch is placed in the photograph's full decode (`src_w` x `src_h`); the picture it is applied to may be a smaller
//! decode of the same photograph (a Develop preview), so position and size are scaled by the ratio. Everything outside
//! a patch's fade is left alone. Pure: no catalog, no model.

use crate::ai::inpaint_pipeline::{apply_patch, dilate, plan_window};
use crate::ai::protocol::Region;
use crate::catalog::StoredPatch;
use image::{imageops::FilterType, GrayImage, Luma, RgbImage, RgbaImage};
use serde::Deserialize;

/// One thing the user marked to be removed, in coordinates normalised to the photograph (0..1 on both axes), so it
/// means the same on a preview and on the full decode.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MaskShape {
    /// A lasso: the inside of the closed outline (even-odd).
    Polygon { points: Vec<[f32; 2]> },
    /// A brush stroke: a round brush dragged along `points`; `radius` is a fraction of the photograph's longer side.
    Stroke { points: Vec<[f32; 2]>, radius: f32 },
}

/// The separate things a mask asks to remove, as boxes. Marks that touch are one thing; and so are marks that fall inside
/// each other's window (the area the network will see around them, `plan_window`), since the network would otherwise
/// treat a neighbouring mark as picture to copy from. Marks farther apart than that are separate things, each filled
/// from a window sized for it: one window over two distant objects would be reduced and soft for both.
pub fn mask_clusters(mask: &GrayImage) -> Vec<Region> {
    let (w, h) = mask.dimensions();
    let mut seen = vec![false; (w as usize) * (h as usize)];
    let mut boxes: Vec<Region> = Vec::new();
    let mut stack: Vec<(u32, u32)> = Vec::new();
    for y0 in 0..h {
        for x0 in 0..w {
            let i0 = (y0 * w + x0) as usize;
            if seen[i0] || mask.get_pixel(x0, y0).0[0] < 128 {
                continue;
            }
            let (mut bx0, mut by0, mut bx1, mut by1) = (x0, y0, x0, y0);
            seen[i0] = true;
            stack.push((x0, y0));
            while let Some((x, y)) = stack.pop() {
                bx0 = bx0.min(x);
                bx1 = bx1.max(x);
                by0 = by0.min(y);
                by1 = by1.max(y);
                for (nx, ny) in [(x.wrapping_sub(1), y), (x + 1, y), (x, y.wrapping_sub(1)), (x, y + 1)] {
                    if nx < w && ny < h {
                        let i = (ny * w + nx) as usize;
                        if !seen[i] && mask.get_pixel(nx, ny).0[0] >= 128 {
                            seen[i] = true;
                            stack.push((nx, ny));
                        }
                    }
                }
            }
            boxes.push(Region { x: bx0, y: by0, w: bx1 - bx0 + 1, h: by1 - by0 + 1 });
        }
    }
    // Merge boxes until none lies inside another's window.
    loop {
        let mut merged = false;
        'search: for i in 0..boxes.len() {
            let window = plan_window(boxes[i], w, h);
            for j in 0..boxes.len() {
                if i != j && overlaps(&window, &boxes[j]) {
                    let (a, b) = (boxes[i], boxes[j]);
                    let (x0, y0, x1, y1) = (a.x.min(b.x), a.y.min(b.y), (a.x + a.w).max(b.x + b.w), (a.y + a.h).max(b.y + b.h));
                    boxes[i] = Region { x: x0, y: y0, w: x1 - x0, h: y1 - y0 };
                    boxes.remove(j);
                    merged = true;
                    break 'search;
                }
            }
        }
        if !merged {
            break;
        }
    }
    boxes.sort_by_key(|b| (b.y, b.x));
    boxes
}

fn overlaps(a: &Region, b: &Region) -> bool {
    a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h
}

/// The mask of `shapes` on a `w` x `h` picture (255 = remove), grown by `grow_px` pixels: an inpainter needs the halo
/// and shadow of an object covered, which a hand-drawn outline rarely does.
pub fn rasterise_mask(w: u32, h: u32, shapes: &[MaskShape], grow_px: u32) -> GrayImage {
    let mut mask = GrayImage::new(w, h);
    let (fw, fh) = (w as f32, h as f32);
    for shape in shapes {
        match shape {
            MaskShape::Polygon { points } if points.len() >= 3 => {
                let pts: Vec<(f32, f32)> = points.iter().map(|p| (p[0] * fw, p[1] * fh)).collect();
                for y in 0..h {
                    let yc = y as f32 + 0.5;
                    let mut xs: Vec<f32> = Vec::new();
                    for i in 0..pts.len() {
                        let ((x0, y0), (x1, y1)) = (pts[i], pts[(i + 1) % pts.len()]);
                        if (y0 <= yc) != (y1 <= yc) {
                            xs.push(x0 + (yc - y0) / (y1 - y0) * (x1 - x0));
                        }
                    }
                    xs.sort_by(|a, b| a.total_cmp(b));
                    for pair in xs.chunks_exact(2) {
                        let (from, to) = ((pair[0] - 0.5).ceil().max(0.0) as u32, (pair[1] - 0.5).floor().min(fw - 1.0));
                        if to >= 0.0 {
                            for x in from..=(to as u32) {
                                mask.put_pixel(x, y, Luma([255]));
                            }
                        }
                    }
                }
            }
            MaskShape::Stroke { points, radius } if !points.is_empty() => {
                let r = (radius * fw.max(fh)).max(0.5);
                let pts: Vec<(f32, f32)> = points.iter().map(|p| (p[0] * fw, p[1] * fh)).collect();
                for i in 0..pts.len() {
                    let (a, b) = (pts[i], pts[(i + 1).min(pts.len() - 1)]);
                    let (x0, x1) = ((a.0.min(b.0) - r).floor().max(0.0) as u32, ((a.0.max(b.0) + r).ceil() as u32).min(w - 1));
                    let (y0, y1) = ((a.1.min(b.1) - r).floor().max(0.0) as u32, ((a.1.max(b.1) + r).ceil() as u32).min(h - 1));
                    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
                    let len2 = dx * dx + dy * dy;
                    for y in y0..=y1 {
                        for x in x0..=x1 {
                            let (px, py) = (x as f32 + 0.5 - a.0, y as f32 + 0.5 - a.1);
                            let t = if len2 > 0.0 { ((px * dx + py * dy) / len2).clamp(0.0, 1.0) } else { 0.0 };
                            let (ex, ey) = (px - t * dx, py - t * dy);
                            if ex * ex + ey * ey <= r * r {
                                mask.put_pixel(x, y, Luma([255]));
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
    dilate(&mask, grow_px)
}

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

    fn on(m: &GrayImage) -> u32 {
        m.pixels().filter(|p| p.0[0] == 255).count() as u32
    }

    fn dab(x: f32, y: f32) -> MaskShape {
        MaskShape::Stroke { points: vec![[x, y]], radius: 0.01 }
    }

    #[test]
    fn a_polygon_fills_its_inside_and_a_degenerate_one_fills_nothing() {
        let square = MaskShape::Polygon { points: vec![[0.25, 0.25], [0.75, 0.25], [0.75, 0.75], [0.25, 0.75]] };
        let m = rasterise_mask(200, 100, &[square], 0);
        assert_eq!(m.get_pixel(100, 50).0[0], 255);
        assert_eq!(m.get_pixel(10, 50).0[0], 0);
        assert_eq!(m.get_pixel(100, 10).0[0], 0);
        assert_eq!(on(&m), 100 * 50, "a 100 x 50 px rectangle");
        assert_eq!(on(&rasterise_mask(50, 50, &[MaskShape::Polygon { points: vec![[0.1, 0.1], [0.9, 0.9]] }], 0)), 0);
    }

    #[test]
    fn a_concave_polygon_leaves_its_notch_empty() {
        // a "U": the middle of the top edge is cut in
        let u = MaskShape::Polygon { points: vec![[0.1, 0.1], [0.4, 0.1], [0.4, 0.6], [0.6, 0.6], [0.6, 0.1], [0.9, 0.1], [0.9, 0.9], [0.1, 0.9]] };
        let m = rasterise_mask(100, 100, &[u], 0);
        assert_eq!(m.get_pixel(50, 30).0[0], 0, "inside the notch");
        assert_eq!(m.get_pixel(50, 80).0[0], 255);
        assert_eq!(m.get_pixel(20, 30).0[0], 255);
    }

    #[test]
    fn a_stroke_is_a_round_brush_along_its_path_and_a_single_point_is_a_dab() {
        let line = MaskShape::Stroke { points: vec![[0.2, 0.5], [0.8, 0.5]], radius: 0.05 };
        let m = rasterise_mask(200, 100, &[line], 0);
        assert_eq!(m.get_pixel(100, 50).0[0], 255);
        assert_eq!(m.get_pixel(100, 58).0[0], 255, "within the 10 px radius");
        assert_eq!(m.get_pixel(100, 70).0[0], 0);
        assert_eq!(m.get_pixel(20, 50).0[0], 0, "before the start of the path");
        let disc = rasterise_mask(100, 100, &[MaskShape::Stroke { points: vec![[0.5, 0.5]], radius: 0.1 }], 0);
        assert!((on(&disc) as f32 - std::f32::consts::PI * 100.0).abs() < 25.0, "a disc of radius 10: {}", on(&disc));
    }

    #[test]
    fn growing_covers_the_halo_and_shapes_off_the_picture_are_clipped_not_a_panic() {
        let s = MaskShape::Stroke { points: vec![[0.5, 0.5]], radius: 0.05 };
        assert!(on(&rasterise_mask(100, 100, std::slice::from_ref(&s), 4)) > on(&rasterise_mask(100, 100, &[s], 0)));
        let off = MaskShape::Polygon { points: vec![[-0.5, -0.5], [1.5, -0.5], [1.5, 1.5], [-0.5, 1.5]] };
        assert_eq!(on(&rasterise_mask(20, 20, &[off], 0)), 400);
        let edge = MaskShape::Stroke { points: vec![[1.2, 1.2]], radius: 0.3 };
        let _ = rasterise_mask(20, 20, &[edge], 0);
    }

    #[test]
    fn marks_far_apart_are_separate_things_and_marks_that_share_a_window_are_one() {
        // 4000 x 3000: a dab of radius 40 px; the network's window is 512 px, so marks 3200 px apart are separate
        let far = rasterise_mask(4000, 3000, &[dab(0.1, 0.5), dab(0.9, 0.5)], 0);
        let boxes = mask_clusters(&far);
        assert_eq!(boxes.len(), 2);
        assert!(boxes[0].x < boxes[1].x && boxes.iter().all(|b| b.w < 100 && b.h < 100), "{boxes:?}");
        // 280 px apart: inside each other's 512 px window, so one thing even though they do not touch
        let near = rasterise_mask(4000, 3000, &[dab(0.5, 0.5), dab(0.57, 0.5)], 0);
        let one = mask_clusters(&near);
        assert_eq!(one.len(), 1);
        assert!(one[0].w > 300, "{one:?}");
        assert!(mask_clusters(&GrayImage::new(50, 50)).is_empty());
    }

    #[test]
    fn a_chain_of_marks_each_near_the_next_becomes_one_thing() {
        let chain: Vec<MaskShape> = (0..6).map(|i| dab(0.2 + 0.06 * i as f32, 0.5)).collect();
        assert_eq!(mask_clusters(&rasterise_mask(4000, 3000, &chain, 0)).len(), 1);
    }

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
