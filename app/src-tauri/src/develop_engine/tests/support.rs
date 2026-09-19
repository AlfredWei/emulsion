use super::*;

pub(super) fn stack_with(ops: &[(&str, f32)]) -> EditStack {
    EditStack {
        schema_version: 1,
        ops: ops
            .iter()
            .map(|(op, value)| serde_json::json!({ "op": op, "value": value }))
            .collect(),
    }
}

/// Applies `apply_edit_stack` to a single pixel and asserts against a
/// hand-derived expected value (±2/255, matching the WGSL shader's own
/// established tolerance in m1-slice3-smoke) -- the parity table this
/// module's doc comment promises.
pub(super) fn assert_pixel(input: [u8; 3], ops: &[(&str, f32)], expected: [i32; 3]) {
    let mut image = RgbImage::from_pixel(1, 1, image::Rgb(input));
    apply_edit_stack(&mut image, &stack_with(ops));
    let pixel = image.get_pixel(0, 0);
    for (actual, expected) in pixel.0.iter().zip(expected.iter()) {
        assert!(
            (*actual as i32 - expected).abs() <= 2,
            "expected ~{expected:?}, got {actual} (full pixel {:?}, ops {ops:?})",
            pixel.0
        );
    }
}

/// Builds an EditStack with the given scalar global ops (exposure/
/// contrast/saturation, same shape `stack_with` uses) plus one
/// `tone_curve` op -- `stack_with` alone can't express a curve's
/// structured `points` payload.
pub(super) fn stack_with_curve(
    scalar_ops: &[(&str, f32)],
    points: &[(f32, f32)],
) -> EditStack {
    let mut stack = stack_with(scalar_ops);
    stack.ops.push(serde_json::json!({
        "op": "tone_curve",
        "points": points.iter().map(|(x, y)| serde_json::json!({ "x": x, "y": y })).collect::<Vec<_>>(),
    }));
    stack
}

pub(super) fn assert_pixel_with_curve(input: [u8; 3], points: &[(f32, f32)], expected: [i32; 3]) {
    let mut image = RgbImage::from_pixel(1, 1, image::Rgb(input));
    apply_edit_stack(&mut image, &stack_with_curve(&[], points));
    let pixel = image.get_pixel(0, 0);
    for (actual, expected) in pixel.0.iter().zip(expected.iter()) {
        assert!(
            (*actual as i32 - expected).abs() <= 2,
            "expected ~{expected:?}, got {actual} (full pixel {:?}, points {points:?})",
            pixel.0
        );
    }
}

pub(super) fn stack_with_hsl_bands(bands: &[(&str, f32, f32, f32)]) -> EditStack {
    let bands_obj: serde_json::Map<String, serde_json::Value> = bands
        .iter()
        .map(|(name, hue, sat, lum)| {
            (
                name.to_string(),
                serde_json::json!({ "hue": hue, "saturation": sat, "luminance": lum }),
            )
        })
        .collect();
    EditStack {
        schema_version: 1,
        ops: vec![serde_json::json!({ "op": "hsl", "bands": bands_obj })],
    }
}

pub(super) fn assert_pixel_with_hsl(input: [u8; 3], bands: &[(&str, f32, f32, f32)], expected: [i32; 3]) {
    let mut image = RgbImage::from_pixel(1, 1, image::Rgb(input));
    apply_edit_stack(&mut image, &stack_with_hsl_bands(bands));
    let pixel = image.get_pixel(0, 0);
    for (actual, expected) in pixel.0.iter().zip(expected.iter()) {
        assert!(
            (*actual as i32 - expected).abs() <= 2,
            "expected ~{expected:?}, got {actual} (full pixel {:?}, bands {bands:?})",
            pixel.0
        );
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn stack_with_split_toning(
    shadow_hue: f32,
    shadow_saturation: f32,
    highlight_hue: f32,
    highlight_saturation: f32,
    balance: f32,
) -> EditStack {
    EditStack {
        schema_version: 1,
        ops: vec![serde_json::json!({
            "op": "split_toning",
            "shadows": { "hue": shadow_hue, "saturation": shadow_saturation },
            "highlights": { "hue": highlight_hue, "saturation": highlight_saturation },
            "balance": balance,
        })],
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn assert_pixel_with_split_toning(
    input: [u8; 3],
    shadow_hue: f32,
    shadow_saturation: f32,
    highlight_hue: f32,
    highlight_saturation: f32,
    balance: f32,
    expected: [i32; 3],
) {
    let mut image = RgbImage::from_pixel(1, 1, image::Rgb(input));
    apply_edit_stack(
        &mut image,
        &stack_with_split_toning(shadow_hue, shadow_saturation, highlight_hue, highlight_saturation, balance),
    );
    let pixel = image.get_pixel(0, 0);
    for (actual, expected) in pixel.0.iter().zip(expected.iter()) {
        assert!(
            (*actual as i32 - expected).abs() <= 2,
            "expected ~{expected:?}, got {actual} (full pixel {:?})",
            pixel.0
        );
    }
}

/// A 1x1 test image always samples at uv=(0.5,0.5) (pixel-center
/// sampling) -- so these cases vary the MASK's start/end to place that
/// fixed point at the desired relative position (before/after/at the
/// gradient), rather than varying the image. Each expected value
/// hand-computed precisely via script (not eyeballed) before writing
/// the assertion.
pub(super) fn mask_stack(
    start: (f32, f32),
    end: (f32, f32),
    feather: f32,
    exposure: f32,
) -> EditStack {
    EditStack {
        schema_version: 1,
        ops: vec![serde_json::json!({
            "op": "linear_gradient_mask",
            "id": "test-mask",
            "start": { "x": start.0, "y": start.1 },
            "end": { "x": end.0, "y": end.1 },
            "feather": feather,
            "invert": false,
            "exposure": exposure,
            "contrast": 0.0,
            "saturation": 0.0,
        })],
    }
}

pub(super) fn assert_mask_pixel(stack: EditStack, expected: [i32; 3]) {
    let mut image = RgbImage::from_pixel(1, 1, image::Rgb([100, 100, 100]));
    apply_edit_stack(&mut image, &stack);
    let pixel = image.get_pixel(0, 0);
    for (actual, expected) in pixel.0.iter().zip(expected.iter()) {
        assert!(
            (*actual as i32 - expected).abs() <= 2,
            "expected ~{expected:?}, got {actual} (full pixel {:?})",
            pixel.0
        );
    }
}

/// Radial gradient masks (M3 Slice 6). A 1x1 test image always samples
/// at uv=(0.5,0.5) -- these cases vary the mask's center/radius so that
/// fixed point lands at the desired position relative to the ellipse
/// (center, well outside, etc). Each expected value hand-computed
/// precisely via script (not eyeballed), same pattern as the linear
/// cases above.
pub(super) fn radial_mask_stack(
    center: (f32, f32),
    radius_x: f32,
    radius_y: f32,
    feather: f32,
    invert: bool,
    exposure: f32,
) -> EditStack {
    EditStack {
        schema_version: 1,
        ops: vec![serde_json::json!({
            "op": "radial_gradient_mask",
            "id": "test-radial-mask",
            "center": { "x": center.0, "y": center.1 },
            "radiusX": radius_x,
            "radiusY": radius_y,
            "feather": feather,
            "invert": invert,
            "exposure": exposure,
            "contrast": 0.0,
            "saturation": 0.0,
        })],
    }
}

/// Same shape as `assert_mask_pixel`, but starting from a caller-chosen
/// color instead of a fixed neutral gray -- required for Red Eye's own
/// tests below, since a gray starting pixel has zero "redness" by
/// definition and could never exercise the redness-selective formula.
pub(super) fn assert_mask_pixel_from(start: [u8; 3], stack: EditStack, expected: [i32; 3]) {
    let mut image = RgbImage::from_pixel(1, 1, image::Rgb(start));
    apply_edit_stack(&mut image, &stack);
    let pixel = image.get_pixel(0, 0);
    for (actual, expected) in pixel.0.iter().zip(expected.iter()) {
        assert!(
            (*actual as i32 - expected).abs() <= 2,
            "expected ~{expected:?}, got {actual} (full pixel {:?})",
            pixel.0
        );
    }
}

/// Red Eye masks (M4). Same 1x1-image-sampled-at-uv=(0.5,0.5)
/// convention as radial's own tests above, reusing radial's exact
/// ellipse geometry -- these cases instead vary `pupilSize`/`darken`
/// and the STARTING pixel color (via `assert_mask_pixel_from`), since
/// this mask's whole point is that its effect depends on color, not
/// just position. Every expected value hand-computed precisely (not
/// eyeballed) from `red_eye_redness_factor`/`red_eye_local_color`.
pub(super) fn red_eye_mask_stack(
    center: (f32, f32),
    radius_x: f32,
    radius_y: f32,
    feather: f32,
    pupil_size: f32,
    darken: f32,
) -> EditStack {
    EditStack {
        schema_version: 1,
        ops: vec![serde_json::json!({
            "op": "red_eye_mask",
            "id": "test-red-eye-mask",
            "center": { "x": center.0, "y": center.1 },
            "radiusX": radius_x,
            "radiusY": radius_y,
            "feather": feather,
            "pupilSize": pupil_size,
            "darken": darken,
        })],
    }
}

/// Brush masks (M3 Slice 7). A 1x1 test image always samples at
/// uv=(0.5,0.5) -- these cases place dabs at hand-computed positions so
/// that fixed point lands at the desired distance/falloff, same
/// pattern as the linear/radial cases above. `aspect` is 1.0 for a 1x1
/// image, so dab.radius (width-only) behaves as a plain circle here.
#[allow(clippy::too_many_arguments)]
pub(super) fn dab(x: f32, y: f32, radius: f32, hardness: f32, flow: f32, mode: &str) -> serde_json::Value {
    serde_json::json!({ "x": x, "y": y, "radius": radius, "hardness": hardness, "flow": flow, "mode": mode })
}

pub(super) fn brush_mask_stack(dabs: Vec<serde_json::Value>, invert: bool, exposure: f32) -> EditStack {
    EditStack {
        schema_version: 1,
        ops: vec![serde_json::json!({
            "op": "brush_mask",
            "id": "test-brush-mask",
            "dabs": dabs,
            "invert": invert,
            "exposure": exposure,
            "contrast": 0.0,
            "saturation": 0.0,
        })],
    }
}

/// Luminance range masks -- the first kind whose weight depends on
/// pixel VALUE, not just uv position. Unlike the geometric masks
/// above, the sample point (uv=(0.5,0.5) via `assert_mask_pixel`'s
/// fixed 1x1 image) never moves -- these cases instead vary the
/// STARTING pixel's own gray value to land its luma at the desired
/// position relative to the range. Each expected value hand-computed
/// precisely (not eyeballed), same discipline as every mask kind above.
pub(super) fn luminance_mask_stack(
    range_min: f32,
    range_max: f32,
    feather: f32,
    invert: bool,
    exposure: f32,
) -> EditStack {
    EditStack {
        schema_version: 1,
        ops: vec![serde_json::json!({
            "op": "luminance_range_mask",
            "id": "test-luminance-mask",
            "rangeMin": range_min,
            "rangeMax": range_max,
            "feather": feather,
            "invert": invert,
            "exposure": exposure,
            "contrast": 0.0,
            "saturation": 0.0,
        })],
    }
}

pub(super) fn assert_luminance_pixel(gray: u8, stack: EditStack, expected: [i32; 3]) {
    let mut image = RgbImage::from_pixel(1, 1, image::Rgb([gray, gray, gray]));
    apply_edit_stack(&mut image, &stack);
    let pixel = image.get_pixel(0, 0);
    for (actual, expected) in pixel.0.iter().zip(expected.iter()) {
        assert!(
            (*actual as i32 - expected).abs() <= 2,
            "expected ~{expected:?}, got {actual} (full pixel {:?})",
            pixel.0
        );
    }
}

/// Color range masks -- single reference color + tolerance, single-sided
/// falloff (unlike luminance's two-sided band). These cases use a GRAY
/// reference color and a gray starting pixel so `dist = |gray - ref| *
/// sqrt(3)` (all three channels differ from the reference by the same
/// amount), keeping the distance arithmetic simple to hand-verify.
pub(super) fn color_mask_stack(ref_gray: f32, range: f32, feather: f32, invert: bool, exposure: f32) -> EditStack {
    EditStack {
        schema_version: 1,
        ops: vec![serde_json::json!({
            "op": "color_range_mask",
            "id": "test-color-mask",
            "refColor": { "r": ref_gray, "g": ref_gray, "b": ref_gray },
            "range": range,
            "feather": feather,
            "invert": invert,
            "exposure": exposure,
            "contrast": 0.0,
            "saturation": 0.0,
        })],
    }
}

pub(super) fn assert_color_pixel(gray: u8, stack: EditStack, expected: [i32; 3]) {
    let mut image = RgbImage::from_pixel(1, 1, image::Rgb([gray, gray, gray]));
    apply_edit_stack(&mut image, &stack);
    let pixel = image.get_pixel(0, 0);
    for (actual, expected) in pixel.0.iter().zip(expected.iter()) {
        assert!(
            (*actual as i32 - expected).abs() <= 2,
            "expected ~{expected:?}, got {actual} (full pixel {:?})",
            pixel.0
        );
    }
}

// Dehaze -- the first op in this module needing a real multi-pixel
// image (every earlier op's test used a 1x1 image, since every earlier
// op is purely per-pixel; dark-channel-prior genuinely needs spatial
// extent to exercise its windowed min-filter).

/// A small "sky" patch (near-white, brighter than everything else) in
/// one corner plus a uniform "scene" fill everywhere else -- crafted so
/// `dehaze_atmospheric_light` picks the sky patch's own exact color
/// (highest luminance) and the DARK-CHANNEL WINDOW (15x15, radius 7) is
/// always much larger than the 3x3 patch, so every pixel's own
/// dark-channel value is dominated by the scene's (lower) minChannel
/// value -- confirmed by hand: even a window centered at the very
/// corner (0,0), clamped to an 8x8 region, still reaches scene pixels
/// at x/y >= 3, so no window is ever entirely contained within the
/// patch. This makes the dark channel/transmission maps uniform across
/// the whole image, which is what makes this test's expected values
/// hand-computable exactly rather than needing a real reference
/// implementation to check against.
pub(super) fn dehaze_test_image(width: u32, height: u32, scene: [u8; 3], sky: [u8; 3]) -> RgbImage {
    image::ImageBuffer::from_fn(width, height, |x, y| {
        if x < 3 && y < 3 {
            image::Rgb(sky)
        } else {
            image::Rgb(scene)
        }
    })
}

// -- M3 Lens Corrections ------------------------------------------

pub(super) fn lens_correction_op_json(fields: serde_json::Value) -> EditStack {
    let mut op = serde_json::json!({ "op": "lens_correction" });
    op.as_object_mut().unwrap().extend(fields.as_object().unwrap().clone());
    EditStack { schema_version: 1, ops: vec![op] }
}

pub(super) fn checkerboard(size: u32) -> RgbImage {
    RgbImage::from_fn(size, size, |x, y| {
        if (x + y) % 4 < 2 { image::Rgb([220, 220, 220]) } else { image::Rgb([30, 30, 30]) }
    })
}

// M4 Slice 1 (Healing/Clone brush). Every case below builds a flat
// two-tone image (x < SPOT_TEST_SIZE/2 is LEFT gray, the rest is RIGHT
// gray) and places `dest`/`source` deep enough inside their own half
// that even the heal ring (`radius * HEAL_RING_FACTOR`) never crosses
// the LEFT/RIGHT boundary -- so `sample_ring_mean` always reads a
// single flat color on each side, keeping every expected value exactly
// hand-derivable rather than approximate.
pub(super) const SPOT_TEST_SIZE: u32 = 40;

pub(super) const SPOT_TEST_LEFT: u8 = 40;

pub(super) const SPOT_TEST_RIGHT: u8 = 200;

pub(super) fn spot_test_image() -> RgbImage {
    RgbImage::from_fn(SPOT_TEST_SIZE, SPOT_TEST_SIZE, |x, _y| {
        if x < SPOT_TEST_SIZE / 2 {
            image::Rgb([SPOT_TEST_LEFT; 3])
        } else {
            image::Rgb([SPOT_TEST_RIGHT; 3])
        }
    })
}

/// `dest_px`/`source_px` are given as pixel INDICES, converted to
/// pixel-center UV (`(px + 0.5) / SPOT_TEST_SIZE`) the same way
/// `apply_edit_stack`'s own Pass 8 derives `uv` from `(x, y)` -- so a
/// radius expressed as a whole number of pixels (e.g. `1.0 /
/// SPOT_TEST_SIZE as f32`) lands exactly on a pixel boundary, not
/// somewhere between two pixel centers. Single-dab convenience wrapper
/// around `spot_mask_multi_dab_op` (M4 Slice 2: a stroke is a `Vec` of
/// dabs now, not one dest point) -- `source_offset` is derived as
/// `source_px - dest_px` in UV space, exactly reproducing this test
/// helper's original one-dest-one-source behavior.
pub(super) fn spot_mask_op(dest_px: (u32, u32), source_px: (u32, u32), radius: f32, feather: f32, mode: &str) -> serde_json::Value {
    spot_mask_multi_dab_op(&[dest_px], source_px, radius, feather, mode)
}

/// General form: an arbitrary list of dab center pixel indices, all
/// sharing `radius`, plus ONE `source_px` that (combined with the
/// FIRST dab) determines `sourceOffset` -- matching the real op shape's
/// own "single offset applied to every dab" model (see `SpotMask`'s
/// own doc comment).
pub(super) fn spot_mask_multi_dab_op(dab_px: &[(u32, u32)], source_px: (u32, u32), radius: f32, feather: f32, mode: &str) -> serde_json::Value {
    let to_uv = |p: (u32, u32)| ((p.0 as f32 + 0.5) / SPOT_TEST_SIZE as f32, (p.1 as f32 + 0.5) / SPOT_TEST_SIZE as f32);
    let first = to_uv(dab_px[0]);
    let s = to_uv(source_px);
    let dabs: Vec<_> = dab_px
        .iter()
        .map(|&p| {
            let (x, y) = to_uv(p);
            serde_json::json!({ "x": x, "y": y, "radius": radius })
        })
        .collect();
    serde_json::json!({
        "op": "spot_mask",
        "id": "test-spot-mask",
        "dabs": dabs,
        "sourceOffset": { "dx": s.0 - first.0, "dy": s.1 - first.1 },
        "feather": feather,
        "mode": mode,
    })
}
