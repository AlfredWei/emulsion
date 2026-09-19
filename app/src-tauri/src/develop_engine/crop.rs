use super::*;

/// Crop & Straighten (M3): unlike every op above, this is NOT applied
/// inside `apply_edit_stack` and has NO WGSL twin at all -- it's a pure
/// geometric post-processing step (crop rect + rotation angle), called
/// separately by `export.rs` and `import.rs`'s thumbnail regen, always
/// AFTER `apply_edit_stack` has already produced the fully graded image
/// (crop-then-resize, not resize-then-crop, so any subsequent long-edge
/// cap describes the DELIVERED image, not an intermediate pre-crop one).
/// The live GPU preview represents this purely at the CSS/display layer
/// instead (`DevelopCanvas.svelte`'s own doc comment on the committed-
/// crop preview wrapper) -- `canvasEl.width/height` never changes for
/// crop, which was a deliberate design-review-driven choice to avoid an
/// entire class of coordinate-system bugs (eyedropper sampling, 100%-zoom
/// scroll math, and mask-handle dragging under a live rotation transform
/// all silently break if the backing store's own dimensions become
/// crop-dependent) that a full GPU-pass-based draft of this feature
/// surfaced before any of it was implemented.
///
/// `x`/`y`/`width`/`height` are normalized 0-1, `angle` is degrees
/// (-45..45), POSITIVE = CLOCKWISE -- matching CSS's own `rotate(Ndeg)`
/// convention exactly, hand-verified (see this module's own test) rather
/// than assumed, since a mismatched sign would be an immediately obvious
/// "image un-rotates on commit" bug between the CSS preview and the real
/// exported pixels.
pub(crate) struct Crop {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub angle: f32,
}

impl Default for Crop {
    fn default() -> Self {
        Crop { x: 0.0, y: 0.0, width: 1.0, height: 1.0, angle: 0.0 }
    }
}

/// A floor on the crop rect's own pixel dimensions -- without this, a
/// degenerate (zero-area) crop rect would panic `image::imageops::crop`
/// and `resize` downstream, not just look wrong. Chosen as an absolute
/// pixel count (not a fraction) so it's meaningful regardless of source
/// resolution. Bumped from an earlier 4px (technically non-degenerate,
/// but a 4x4 exported crop is not a meaningfully usable photo) to match
/// `CROP_MIN_PX` in DevelopCanvas.svelte, which enforces this same real
/// floor during interactive drag -- keeping both layers at the same
/// value means the live preview and the actual export/thumbnail crop
/// agree on what "as small as this can get" means.
pub(super) const CROP_MIN_SIZE_PX: u32 = 64;

pub(crate) fn crop_op(ops: &[serde_json::Value]) -> Crop {
    let op = ops.iter().find(|op| op.get("op").and_then(|v| v.as_str()) == Some("crop"));
    let field = |key: &str, default: f32| -> f32 {
        op.and_then(|o| o.get(key)).and_then(|v| v.as_f64()).map(|v| v as f32).unwrap_or(default)
    };
    Crop {
        x: field("x", 0.0),
        y: field("y", 0.0),
        width: field("width", 1.0),
        height: field("height", 1.0),
        angle: field("angle", 0.0),
    }
}

pub(super) fn crop_is_identity(c: &Crop) -> bool {
    let eps = 1e-6;
    c.x.abs() < eps && c.y.abs() < eps && (c.width - 1.0).abs() < eps && (c.height - 1.0).abs() < eps && c.angle.abs() < eps
}

/// Bilinear-sampled affine rotation around the image's own center,
/// output the SAME dimensions as input (rotating in place -- corners
/// necessarily reveal blank space at nonzero angles; out-of-bounds
/// samples are filled black). No `imageproc` dependency available in
/// this workspace, and `image::imageops` only supports 90-degree-multiple
/// rotation, so this is hand-rolled.
///
/// Derivation (POSITIVE angle = CLOCKWISE, in this crate's own y-down
/// pixel coordinate convention): the FORWARD mapping (where a source
/// point ends up after rotating the image's content clockwise by theta)
/// is `output = center + R_cw(theta) * (source - center)` where
/// `R_cw(theta) = [cos theta, -sin theta; sin theta, cos theta]` --
/// this specific matrix, not the more common CCW-in-math-convention one,
/// because y increases DOWNWARD in image space, which flips the visual
/// handedness of the standard rotation matrix (hand-verified against
/// CSS's own `rotate(deg)` behavior in this module's own test, not
/// assumed). Since rotation matrices are orthogonal, `R_cw(theta)^-1 ==
/// R_cw(-theta)`, giving the INVERSE (sampling) mapping this function
/// actually needs -- for each OUTPUT pixel, where in the SOURCE did it
/// come from:
/// `source = center + R_cw(-theta) * (output - center)`
/// `       = center + [cos theta, sin theta; -sin theta, cos theta] * (output - center)`
pub(super) fn rotate_image(image: &RgbImage, angle_deg: f32) -> RgbImage {
    let (width, height) = (image.width(), image.height());
    let mut out = RgbImage::new(width, height);
    if angle_deg == 0.0 {
        out.copy_from_slice(image.as_raw());
        return out;
    }
    let theta = angle_deg.to_radians();
    let (sin_t, cos_t) = theta.sin_cos();
    let cx = width as f32 / 2.0;
    let cy = height as f32 / 2.0;
    for oy in 0..height {
        for ox in 0..width {
            let dx = ox as f32 - cx;
            let dy = oy as f32 - cy;
            let sx = cx + dx * cos_t + dy * sin_t;
            let sy = cy - dx * sin_t + dy * cos_t;
            out.put_pixel(ox, oy, sample_bilinear(image, sx, sy));
        }
    }
    out
}

/// Bilinear sample at a fractional pixel position -- out-of-bounds
/// (including partially, at the four-tap boundary) falls back to black,
/// matching `rotate_image`'s own documented "blank corners" behavior
/// rather than clamping to the nearest edge pixel (which would smear
/// edge content into the revealed corners instead of leaving them
/// honestly blank).
pub(super) fn sample_bilinear(image: &RgbImage, x: f32, y: f32) -> image::Rgb<u8> {
    let (width, height) = (image.width() as i32, image.height() as i32);
    if x < 0.0 || y < 0.0 || x > (width - 1) as f32 || y > (height - 1) as f32 {
        return image::Rgb([0, 0, 0]);
    }
    let x0 = x.floor() as i32;
    let y0 = y.floor() as i32;
    let x1 = (x0 + 1).min(width - 1);
    let y1 = (y0 + 1).min(height - 1);
    let fx = x - x0 as f32;
    let fy = y - y0 as f32;
    let p00 = image.get_pixel(x0 as u32, y0 as u32).0;
    let p10 = image.get_pixel(x1 as u32, y0 as u32).0;
    let p01 = image.get_pixel(x0 as u32, y1 as u32).0;
    let p11 = image.get_pixel(x1 as u32, y1 as u32).0;
    let mut out = [0u8; 3];
    for c in 0..3 {
        let top = p00[c] as f32 * (1.0 - fx) + p10[c] as f32 * fx;
        let bottom = p01[c] as f32 * (1.0 - fx) + p11[c] as f32 * fx;
        out[c] = (top * (1.0 - fy) + bottom * fy).round() as u8;
    }
    image::Rgb(out)
}

/// Converts the normalized crop rect into a pixel rect within `(width,
/// height)`, clamped so it never exceeds the image bounds and never
/// collapses below `CROP_MIN_SIZE_PX` in either dimension. Size is
/// resolved BEFORE position, not the other way around: with a real
/// (non-trivial) `CROP_MIN_SIZE_PX`, a requested position near the far
/// edge (e.g. x=0.8 on a modest-width image) combined with flooring the
/// requested size up to the minimum can genuinely conflict with "stay
/// inside the image" -- computing position first and then trying to fit
/// the floored size into whatever space was left could push `px + pw`
/// past `width`. Resolving size first (floored, then capped at the
/// image's own dimension so an over-large minimum on a tiny image still
/// degrades gracefully) and THEN clamping position into the range that
/// keeps the now-fixed size entirely on-image guarantees both invariants
/// hold simultaneously, by construction.
pub(super) fn crop_rect_px(width: u32, height: u32, c: &Crop) -> (u32, u32, u32, u32) {
    let min_w = CROP_MIN_SIZE_PX.min(width);
    let min_h = CROP_MIN_SIZE_PX.min(height);
    let pw = (c.width * width as f32).round().max(min_w as f32).min(width as f32) as u32;
    let ph = (c.height * height as f32).round().max(min_h as f32).min(height as f32) as u32;
    let px = (c.x * width as f32).round().clamp(0.0, (width - pw) as f32) as u32;
    let py = (c.y * height as f32).round().clamp(0.0, (height - ph) as f32) as u32;
    (px, py, pw, ph)
}

/// Applies Crop & Straighten to `image` in place (rotate, then crop --
/// matching this module's own doc comment on why crop-then-resize, not
/// resize-then-crop, in the two real callers). Skipped ENTIRELY at
/// identity, the same exact-passthrough-and-cheap discipline every other
/// op in this crate already follows.
pub(crate) fn apply_crop(image: &mut RgbImage, stack: &EditStack) {
    let crop = crop_op(&stack.ops);
    if crop_is_identity(&crop) {
        return;
    }
    let rotated = if crop.angle == 0.0 { image.clone() } else { rotate_image(image, crop.angle) };
    let (width, height) = (rotated.width(), rotated.height());
    let (px, py, pw, ph) = crop_rect_px(width, height, &crop);
    *image = image::imageops::crop_imm(&rotated, px, py, pw, ph).to_image();
}
