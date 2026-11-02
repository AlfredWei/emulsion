//! The pure half of object removal (RFC-0032 §3.3, ADR-0010): which window of the photograph the network sees, how the
//! window and the mask become LaMa's two inputs, and how the network's answer becomes a stored patch (an RGBA image
//! that is opaque inside the mask and fades out just beyond it). No model, no I/O: all of it is unit-tested without
//! weights; `bin/emulsion-ai/inpaint.rs` is the model call around it.
//!
//! LaMa (the Carve ONNX port) takes a fixed 512 x 512 window: `image` 1x3x512x512 (RGB, 0..1) and `mask` 1x1x512x512
//! (1 = fill this), and answers 1x3x512x512 on the 0..255 scale, where the pixels outside the mask come back as given.

// Shared by `#[path]` with the helper binary.
#![allow(dead_code)]

use super::protocol::Region;
use image::{imageops::FilterType, GrayImage, Luma, RgbImage, RgbaImage};

/// Side of the network's window.
pub const NET: u32 = 512;
/// The window is this many times the mask's longer side (the bake-off's rule, RFC-0032 §9.1: a 150 px hole in a 512 px
/// crop, a 500 px hole in about 1,100), but never smaller than the network's window.
pub const CONTEXT_FACTOR: f32 = 2.2;
/// A mask pixel at or above this counts as "fill".
const ON: u8 = 128;
/// How far past the mask the patch stays opaque-to-faded (native pixels), and the blur of that edge.
pub const FEATHER_PX: u32 = 4;

/// The smallest rectangle holding every filled pixel of `mask`, or `None` for an empty mask.
pub fn mask_bbox(mask: &GrayImage) -> Option<Region> {
    let (w, h) = mask.dimensions();
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0u32, 0u32);
    for (x, y, p) in mask.enumerate_pixels() {
        if p.0[0] >= ON {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
    }
    (x1 >= x0 && y1 >= y0 && x0 < w && y0 < h).then(|| Region { x: x0, y: y0, w: x1 - x0 + 1, h: y1 - y0 + 1 })
}

/// The square window the network sees: centred on `bbox`, `CONTEXT_FACTOR` times its longer side (at least `NET`),
/// kept inside the image (so it is never larger than the image's shorter side) and shifted, not shrunk, at an edge.
pub fn plan_window(bbox: Region, img_w: u32, img_h: u32) -> Region {
    let wanted = ((bbox.w.max(bbox.h) as f32 * CONTEXT_FACTOR).ceil() as u32).max(NET);
    let side = wanted.min(img_w.min(img_h)).max(1);
    let (cx, cy) = (bbox.x + bbox.w / 2, bbox.y + bbox.h / 2);
    let x = cx.saturating_sub(side / 2).min(img_w - side);
    let y = cy.saturating_sub(side / 2).min(img_h - side);
    Region { x, y, w: side, h: side }
}

/// `true` when the window is shown to the network at another size than its own, so the fill is softer than the photo
/// around it (RFC-0032 §9.3: the main quality question, worked in slice F).
pub fn is_reduced(window: Region) -> bool {
    window.w != NET
}

/// The network's two inputs for a window: `image` as CHW f32 in 0..1 and `mask` as 0 / 1, both 512 x 512.
pub fn to_net_input(crop: &RgbImage, mask: &GrayImage) -> (Vec<f32>, Vec<f32>) {
    let plane = (NET * NET) as usize;
    let (small, small_mask) = if crop.width() == NET {
        (crop.clone(), mask.clone())
    } else {
        // Lanczos to shrink or enlarge the picture; the mask is resampled smoothly and cut at one half, as in the bake-off.
        (image::imageops::resize(crop, NET, NET, FilterType::Lanczos3), image::imageops::resize(mask, NET, NET, FilterType::Triangle))
    };
    let mut image = vec![0f32; 3 * plane];
    for (i, px) in small.pixels().enumerate() {
        for c in 0..3 {
            image[c * plane + i] = px.0[c] as f32 / 255.0;
        }
    }
    let mask_data: Vec<f32> = small_mask.pixels().map(|p| if p.0[0] >= ON { 1.0 } else { 0.0 }).collect();
    (image, mask_data)
}

/// Grows the filled area by `r` pixels (a separable maximum filter).
fn dilate(mask: &GrayImage, r: u32) -> GrayImage {
    if r == 0 {
        return mask.clone();
    }
    let (w, h) = mask.dimensions();
    let r = r as i64;
    let mut horizontal = GrayImage::new(w, h);
    for y in 0..h {
        for x in 0..w as i64 {
            let lo = (x - r).max(0) as u32;
            let hi = (x + r).min(w as i64 - 1) as u32;
            let m = (lo..=hi).map(|xx| mask.get_pixel(xx, y).0[0]).max().unwrap_or(0);
            horizontal.put_pixel(x as u32, y, Luma([m]));
        }
    }
    let mut out = GrayImage::new(w, h);
    for x in 0..w {
        for y in 0..h as i64 {
            let lo = (y - r).max(0) as u32;
            let hi = (y + r).min(h as i64 - 1) as u32;
            let m = (lo..=hi).map(|yy| horizontal.get_pixel(x, yy).0[0]).max().unwrap_or(0);
            out.put_pixel(x, y as u32, Luma([m]));
        }
    }
    out
}

/// The patch alpha: fully opaque on the mask, fading to nothing over about `feather` pixels beyond it.
pub fn feathered_alpha(mask: &GrayImage, feather: u32) -> GrayImage {
    let binary = GrayImage::from_fn(mask.width(), mask.height(), |x, y| Luma([if mask.get_pixel(x, y).0[0] >= ON { 255 } else { 0 }]));
    if feather == 0 {
        return binary;
    }
    let soft = image::imageops::blur(&dilate(&binary, feather), (feather as f32 / 2.0).max(0.5));
    GrayImage::from_fn(mask.width(), mask.height(), |x, y| Luma([soft.get_pixel(x, y).0[0].max(binary.get_pixel(x, y).0[0])]))
}

/// The patch: the network's answer (`net_out`, CHW on the 0..255 scale, 512 x 512) brought back to the window's size,
/// opaque on the mask and faded just beyond it, cut to the mask's box plus the fade. Returns the patch and its top-left
/// corner **in the window**; pixels outside the filled area are the photograph's own (so a fade blends like with like).
pub fn compose_patch(crop: &RgbImage, mask: &GrayImage, net_out: &[f32], feather: u32) -> (RgbaImage, u32, u32) {
    let plane = (NET * NET) as usize;
    assert_eq!(net_out.len(), 3 * plane, "the network answers 3 x 512 x 512");
    let mut net = RgbImage::new(NET, NET);
    for (i, px) in net.pixels_mut().enumerate() {
        for c in 0..3 {
            px.0[c] = net_out[c * plane + i].clamp(0.0, 255.0).round() as u8;
        }
    }
    let filled = if crop.width() == NET { net } else { image::imageops::resize(&net, crop.width(), crop.height(), FilterType::CatmullRom) };
    let alpha = feathered_alpha(mask, feather);
    let reach = dilate(mask, feather + feather);
    let bbox = mask_bbox(&reach).unwrap_or(Region { x: 0, y: 0, w: crop.width(), h: crop.height() });
    let (x0, y0) = (bbox.x, bbox.y);
    let mut patch = RgbaImage::new(bbox.w, bbox.h);
    for py in 0..bbox.h {
        for px in 0..bbox.w {
            let (x, y) = (x0 + px, y0 + py);
            let a = alpha.get_pixel(x, y).0[0];
            // Inside and just beyond the mask the pixel is the network's; farther out it does not matter (alpha is 0).
            let src = if a > 0 { filled.get_pixel(x, y) } else { crop.get_pixel(x, y) };
            patch.put_pixel(px, py, image::Rgba([src.0[0], src.0[1], src.0[2], a]));
        }
    }
    (patch, x0, y0)
}

/// Lays a patch over `dst` at (`x`, `y`): `out = dst + (patch - dst) * alpha`. What the render path does with a stored patch.
pub fn apply_patch(dst: &mut RgbImage, patch: &RgbaImage, x: u32, y: u32) {
    for (px, py, p) in patch.enumerate_pixels() {
        let (dx, dy) = (x + px, y + py);
        if dx >= dst.width() || dy >= dst.height() {
            continue;
        }
        let a = p.0[3] as f32 / 255.0;
        let d = dst.get_pixel_mut(dx, dy);
        for c in 0..3 {
            d.0[c] = (d.0[c] as f32 + (p.0[c] as f32 - d.0[c] as f32) * a).round().clamp(0.0, 255.0) as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn disc_mask(w: u32, h: u32, cx: i32, cy: i32, r: i32) -> GrayImage {
        GrayImage::from_fn(w, h, |x, y| Luma([if (x as i32 - cx).pow(2) + (y as i32 - cy).pow(2) <= r * r { 255 } else { 0 }]))
    }

    #[test]
    fn the_box_of_a_mask_is_tight_and_an_empty_mask_has_none() {
        let m = disc_mask(100, 80, 40, 30, 10);
        assert_eq!(mask_bbox(&m), Some(Region { x: 30, y: 20, w: 21, h: 21 }));
        assert_eq!(mask_bbox(&GrayImage::new(10, 10)), None);
        // a faint stroke below the threshold is not a mask
        assert_eq!(mask_bbox(&GrayImage::from_pixel(10, 10, Luma([100]))), None);
    }

    #[test]
    fn a_small_hole_gets_the_networks_own_window_at_native_size() {
        let w = plan_window(Region { x: 1000, y: 800, w: 150, h: 120 }, 4000, 3000);
        assert_eq!((w.w, w.h), (NET, NET));
        assert!(!is_reduced(w));
        // centred on the hole
        assert!(w.x <= 1000 + 75 && w.x + w.w >= 1000 + 75 && w.y <= 800 + 60 && w.y + w.h >= 800 + 60);
    }

    #[test]
    fn a_big_hole_gets_two_point_two_times_its_longer_side_and_is_reduced() {
        let w = plan_window(Region { x: 1000, y: 800, w: 500, h: 300 }, 4000, 3000);
        assert_eq!((w.w, w.h), (1100, 1100));
        assert!(is_reduced(w));
    }

    #[test]
    fn the_window_is_shifted_not_shrunk_at_an_edge_and_never_leaves_the_image() {
        let corner = plan_window(Region { x: 0, y: 0, w: 40, h: 40 }, 4000, 3000);
        assert_eq!((corner.x, corner.y, corner.w), (0, 0, NET));
        let far = plan_window(Region { x: 3980, y: 2980, w: 20, h: 20 }, 4000, 3000);
        assert_eq!((far.x + far.w, far.y + far.h), (4000, 3000));
        // a hole bigger than the image allows is capped at the image's shorter side
        let huge = plan_window(Region { x: 100, y: 100, w: 2500, h: 2000 }, 4000, 3000);
        assert_eq!((huge.x, huge.y, huge.w, huge.h), (0, 0, 3000, 3000));
        assert!(huge.x <= 100 && huge.x + huge.w >= 100 + 2500, "the hole is inside the window");
        // an image smaller than the network's window gives a smaller window, shown to the network enlarged
        let small = plan_window(Region { x: 10, y: 10, w: 20, h: 20 }, 400, 300);
        assert_eq!((small.w, small.h), (300, 300));
        assert!(is_reduced(small));
    }

    #[test]
    fn net_input_is_chw_in_zero_one_with_a_binary_mask_whatever_the_window_size() {
        for side in [512u32, 1100, 300] {
            let crop = RgbImage::from_pixel(side, side, image::Rgb([255, 0, 51]));
            let mask = disc_mask(side, side, (side / 2) as i32, (side / 2) as i32, (side / 8) as i32);
            let (img, m) = to_net_input(&crop, &mask);
            let plane = (NET * NET) as usize;
            assert_eq!((img.len(), m.len()), (3 * plane, plane));
            assert!((img[0] - 1.0).abs() < 1e-3 && img[plane].abs() < 1e-3 && (img[2 * plane] - 0.2).abs() < 1e-3, "side {side}");
            assert!(m.iter().all(|v| *v == 0.0 || *v == 1.0));
            let on = m.iter().filter(|v| **v == 1.0).count() as f32 / plane as f32;
            let want = std::f32::consts::PI / 64.0;
            assert!((on - want).abs() < 0.01, "side {side}: {on} vs {want}");
        }
    }

    #[test]
    fn the_alpha_is_opaque_on_the_mask_fades_beyond_it_and_is_zero_far_away() {
        let mask = disc_mask(200, 200, 100, 100, 20);
        let a = feathered_alpha(&mask, 4);
        assert_eq!(a.get_pixel(100, 100).0[0], 255);
        assert_eq!(a.get_pixel(120, 100).0[0], 255, "the mask's own edge is opaque");
        let beyond = a.get_pixel(123, 100).0[0];
        assert!(beyond > 0 && beyond < 255, "fading: {beyond}");
        assert_eq!(a.get_pixel(140, 100).0[0], 0);
        assert!(a.get_pixel(122, 100).0[0] >= a.get_pixel(124, 100).0[0], "never rises moving away");
        // no feather: the mask itself
        assert_eq!(feathered_alpha(&mask, 0).get_pixel(123, 100).0[0], 0);
    }

    /// A network answer that is `colour` everywhere, on the 0..255 scale.
    fn flat_answer(colour: [f32; 3]) -> Vec<f32> {
        let plane = (NET * NET) as usize;
        (0..3 * plane).map(|i| colour[i / plane]).collect()
    }

    #[test]
    fn a_patch_replaces_the_masked_pixels_and_leaves_every_other_one_alone() {
        let crop = RgbImage::from_pixel(512, 512, image::Rgb([10, 20, 30]));
        let mask = disc_mask(512, 512, 256, 256, 30);
        let (patch, px, py) = compose_patch(&crop, &mask, &flat_answer([200.0, 100.0, 50.0]), FEATHER_PX);
        let mut out = crop.clone();
        apply_patch(&mut out, &patch, px, py);
        for (x, y, p) in mask.enumerate_pixels() {
            if p.0[0] >= ON {
                assert_eq!(out.get_pixel(x, y).0, [200, 100, 50], "({x},{y}) is the network's");
            }
        }
        assert_eq!(out.get_pixel(5, 5).0, [10, 20, 30]);
        assert_eq!(out.get_pixel(256, 256 + 30 + 3 * FEATHER_PX).0, [10, 20, 30], "well beyond the fade it is untouched");
        // the patch is only the mask's box plus the fade, not the whole window
        assert!(patch.width() < 100 && patch.height() < 100, "{}x{}", patch.width(), patch.height());
    }

    #[test]
    fn the_fade_blends_between_the_fill_and_the_photograph() {
        let crop = RgbImage::from_pixel(512, 512, image::Rgb([0, 0, 0]));
        let mask = disc_mask(512, 512, 256, 256, 30);
        let (patch, px, py) = compose_patch(&crop, &mask, &flat_answer([255.0, 255.0, 255.0]), FEATHER_PX);
        let mut out = crop.clone();
        apply_patch(&mut out, &patch, px, py);
        let edge = out.get_pixel(256 + 33, 256).0[0];
        assert!(edge > 0 && edge < 255, "a fading edge, got {edge}");
    }

    #[test]
    fn a_reduced_window_is_brought_back_to_its_own_size_and_still_only_touches_the_mask() {
        let crop = RgbImage::from_pixel(1100, 1100, image::Rgb([10, 20, 30]));
        let mask = disc_mask(1100, 1100, 550, 550, 120);
        let (patch, px, py) = compose_patch(&crop, &mask, &flat_answer([200.0, 100.0, 50.0]), FEATHER_PX);
        let mut out = crop.clone();
        apply_patch(&mut out, &patch, px, py);
        assert_eq!(out.get_pixel(550, 550).0, [200, 100, 50]);
        assert_eq!(out.get_pixel(20, 20).0, [10, 20, 30]);
        assert!(patch.width() <= 2 * 120 + 4 * FEATHER_PX + 2);
    }
}
