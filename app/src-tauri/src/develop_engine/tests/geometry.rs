use super::support::*;
use super::*;

#[test]
fn apply_crop_at_identity_is_an_exact_passthrough() {
    let mut image = RgbImage::from_pixel(20, 16, image::Rgb([90, 140, 30]));
    let before = image.clone();
    apply_crop(&mut image, &stack_with(&[]));
    assert_eq!(image, before);
}

#[test]
fn rotate_image_at_zero_degrees_is_an_exact_passthrough() {
    let mut image = RgbImage::from_pixel(12, 8, image::Rgb([10, 20, 30]));
    image.put_pixel(5, 3, image::Rgb([255, 0, 0]));
    let rotated = rotate_image(&image, 0.0);
    assert_eq!(rotated, image);
}

/// Hand-derived rotation direction check (POSITIVE = CLOCKWISE,
/// matching CSS's own `rotate(deg)`): on a 6x4 image (deliberately
/// non-square, to also catch an axis-swap bug a square test image
/// couldn't reveal), center = (3.0, 2.0). A source point one pixel
/// LEFT of center, (2,2) -- i.e. (dx,dy) = (-1,0) -- must land at
/// (3,1) after rotating 90 degrees clockwise: applying the module's
/// own documented forward matrix R_cw(90) = [0,-1;1,0] to (-1,0)
/// gives (new_dx,new_dy) = (0*-1 + -1*0, 1*-1 + 0*0) = (0,-1), i.e.
/// one pixel ABOVE center -- matching the familiar clock-hand
/// intuition that rotating the 9-o'clock position clockwise by 90
/// degrees lands it at 12 o'clock. The queried output position (3,1)
/// lands EXACTLY on an integer source pixel (2,2), so bilinear
/// sampling introduces no blending here -- an exact-value assertion
/// is legitimate, not just "close to".
#[test]
fn rotate_image_90_degrees_matches_hand_derived_direction() {
    let mut image = RgbImage::from_pixel(6, 4, image::Rgb([0, 0, 0]));
    image.put_pixel(2, 2, image::Rgb([255, 255, 255]));
    let rotated = rotate_image(&image, 90.0);
    assert_eq!(*rotated.get_pixel(3, 1), image::Rgb([255, 255, 255]));
    // Sanity: the source location itself should no longer be white
    // (confirms the test isn't trivially passing because nothing
    // moved).
    assert_eq!(*rotated.get_pixel(2, 2), image::Rgb([0, 0, 0]));
}

#[test]
fn crop_rect_px_clamps_a_rect_that_would_exceed_image_bounds() {
    let c = Crop { x: 0.8, y: 0.8, width: 0.5, height: 0.5, angle: 0.0 };
    let (px, py, pw, ph) = crop_rect_px(100, 100, &c);
    assert!(px + pw <= 100, "px={px} pw={pw}");
    assert!(py + ph <= 100, "py={py} ph={ph}");
}

#[test]
fn crop_rect_px_floors_a_degenerate_zero_size_rect() {
    let c = Crop { x: 0.5, y: 0.5, width: 0.0, height: 0.0, angle: 0.0 };
    let (_, _, pw, ph) = crop_rect_px(100, 100, &c);
    assert!(pw >= CROP_MIN_SIZE_PX, "pw={pw}");
    assert!(ph >= CROP_MIN_SIZE_PX, "ph={ph}");
}

/// End-to-end crop-only (no rotation): a 200x200 image (large enough
/// that the requested region clears CROP_MIN_SIZE_PX without the
/// floor kicking in and changing the result -- see that constant's
/// own doc comment) with a distinct 80x80 white square at
/// (60,60)-(140,140), cropped to exactly that square, must produce a
/// uniformly white 80x80 result.
#[test]
fn apply_crop_crop_only_extracts_the_expected_region() {
    let mut image = RgbImage::from_pixel(200, 200, image::Rgb([0, 0, 0]));
    for y in 60..140 {
        for x in 60..140 {
            image.put_pixel(x, y, image::Rgb([255, 255, 255]));
        }
    }
    apply_crop(
        &mut image,
        &EditStack {
            schema_version: 1,
            ops: vec![serde_json::json!({ "op": "crop", "x": 0.3, "y": 0.3, "width": 0.4, "height": 0.4, "angle": 0.0 })],
        },
    );
    assert_eq!(image.width(), 80);
    assert_eq!(image.height(), 80);
    for pixel in image.pixels() {
        assert_eq!(*pixel, image::Rgb([255, 255, 255]));
    }
}

#[test]
fn crop_rect_px_enforces_a_real_pixel_minimum_not_just_nonzero() {
    // A tiny requested crop (2x2 out of 1000x1000, well under
    // CROP_MIN_SIZE_PX) must be floored up to a genuinely usable
    // size, not just "not literally zero".
    let c = Crop { x: 0.5, y: 0.5, width: 0.002, height: 0.002, angle: 0.0 };
    let (_, _, pw, ph) = crop_rect_px(1000, 1000, &c);
    assert!(pw >= CROP_MIN_SIZE_PX, "pw={pw}");
    assert!(ph >= CROP_MIN_SIZE_PX, "ph={ph}");
}

#[test]
fn lens_correction_absent_op_is_an_exact_passthrough() {
    let original = checkerboard(21);
    let mut image = original.clone();
    apply_lens_correction(&mut image, &EditStack { schema_version: 1, ops: vec![] });
    assert_eq!(image, original);
}

#[test]
fn lens_correction_profile_present_but_disabled_is_a_passthrough() {
    let original = checkerboard(21);
    let mut image = original.clone();
    let stack = lens_correction_op_json(serde_json::json!({
        "profile_enabled": false,
        "distortion_amount": 100, "vignette_amount": 100, "ca_amount": 100,
        "profile": {
            "distortion": { "model": "poly3", "k1": -0.3 },
            "vignetting": { "k1": -0.5, "k2": 0.0, "k3": 0.0 },
        },
    }));
    apply_lens_correction(&mut image, &stack);
    assert_eq!(image, original, "a matched-but-disabled profile must not apply");
}

#[test]
fn lens_correction_zero_manual_amounts_and_no_profile_is_a_passthrough() {
    let original = checkerboard(21);
    let mut image = original.clone();
    let stack = lens_correction_op_json(serde_json::json!({ "manual_distortion": 0, "manual_ca": 0 }));
    apply_lens_correction(&mut image, &stack);
    assert_eq!(image, original);
}

#[test]
fn lens_correction_manual_distortion_visibly_shifts_content() {
    // A single bright marker off-center on an otherwise dark image --
    // distortion is a resample, so a nonzero correction must move
    // where that marker's brightness ends up relative to a
    // passthrough render at the SAME pixel.
    let mut base = RgbImage::from_pixel(41, 41, image::Rgb([10, 10, 10]));
    base.put_pixel(32, 8, image::Rgb([255, 255, 255]));

    let mut corrected = base.clone();
    apply_lens_correction(&mut corrected, &lens_correction_op_json(serde_json::json!({ "manual_distortion": 100 })));

    assert_ne!(corrected, base, "a nonzero manual distortion amount must change the image");
}

#[test]
fn lens_correction_manual_ca_introduces_channel_divergence() {
    // A single white marker off-center on black: if R/G/B all still
    // sample the SAME source location after "correction", the pixel
    // stays pure white. Manual CA scales R/B radially relative to
    // G -- if they diverge, at least one channel samples black
    // instead, and the pixel is no longer pure white.
    let mut base = RgbImage::from_pixel(41, 41, image::Rgb([0, 0, 0]));
    base.put_pixel(35, 12, image::Rgb([255, 255, 255]));

    let mut corrected = base.clone();
    apply_lens_correction(&mut corrected, &lens_correction_op_json(serde_json::json!({ "manual_ca": 100 })));

    let marker = corrected.get_pixel(35, 12).0;
    assert!(
        marker[0] != 255 || marker[1] != 255 || marker[2] != 255,
        "expected channel divergence at the marker, got {marker:?}"
    );
}

#[test]
fn lens_correction_profile_distortion_amount_zero_is_a_passthrough_amount_100_is_not() {
    let mut base = RgbImage::from_pixel(41, 41, image::Rgb([10, 10, 10]));
    base.put_pixel(32, 8, image::Rgb([255, 255, 255]));
    let profile = serde_json::json!({
        "crop_factor": 1.0, "real_focal": 24.0,
        "distortion": { "model": "poly3", "k1": -0.3 },
    });

    let mut at_zero = base.clone();
    apply_lens_correction(
        &mut at_zero,
        &lens_correction_op_json(serde_json::json!({
            "profile_enabled": true, "distortion_amount": 0, "vignette_amount": 0, "ca_amount": 0,
            "profile": profile,
        })),
    );
    assert_eq!(at_zero, base, "distortion_amount=0 must be a passthrough even with a profile present");

    let mut at_full = base.clone();
    apply_lens_correction(
        &mut at_full,
        &lens_correction_op_json(serde_json::json!({
            "profile_enabled": true, "distortion_amount": 100, "vignette_amount": 0, "ca_amount": 0,
            "profile": profile,
        })),
    );
    assert_ne!(at_full, base, "distortion_amount=100 with a real profile must change the image");
}

#[test]
fn lens_correction_vignetting_corrects_a_uniform_image_brighter_at_the_corners() {
    // A negative k1 means the (uncorrected) lens gain is BELOW 1 at
    // the corners -- DeVignetting applies 1/gain, so a corner pixel
    // must come out brighter than the center after correction, on an
    // otherwise perfectly uniform source (isolates the vignetting
    // formula from anything content-dependent).
    let base = RgbImage::from_pixel(61, 61, image::Rgb([120, 120, 120]));
    let mut corrected = base.clone();
    apply_lens_correction(
        &mut corrected,
        &lens_correction_op_json(serde_json::json!({
            "profile_enabled": true, "distortion_amount": 0, "ca_amount": 0, "vignette_amount": 100,
            "profile": { "crop_factor": 1.0, "real_focal": 24.0, "vignetting": { "k1": -0.6, "k2": 0.0, "k3": 0.0 } },
        })),
    );
    let center = corrected.get_pixel(30, 30).0[0];
    let corner = corrected.get_pixel(0, 0).0[0];
    assert!(corner > center, "corner ({corner}) should be brighter than center ({center}) after correction");
    assert_eq!(center, 120, "the center (r~=0) should be ~unaffected by a pure radial vignetting correction");
}

#[test]
fn lens_correction_vignette_amount_zero_is_a_passthrough() {
    let base = RgbImage::from_pixel(61, 61, image::Rgb([120, 120, 120]));
    let mut image = base.clone();
    apply_lens_correction(
        &mut image,
        &lens_correction_op_json(serde_json::json!({
            "profile_enabled": true, "distortion_amount": 0, "ca_amount": 0, "vignette_amount": 0,
            "profile": { "crop_factor": 1.0, "real_focal": 24.0, "vignetting": { "k1": -0.6, "k2": 0.0, "k3": 0.0 } },
        })),
    );
    assert_eq!(image, base);
}

// -- Pure-formula correctness (round-trip against the FORWARD
// formulas, hand-transcribed from lensfun's own documented models --
// see this module's Lens Corrections header comment) --------------

#[test]
fn lens_undist_poly3_zero_k1_is_identity() {
    assert_eq!(lens_undist_poly3(0.3, -0.2, 0.0), (0.3, -0.2));
}

#[test]
fn lens_undist_poly3_solves_the_forward_equation() {
    let k1 = -0.15_f32;
    let (ru_x, ru_y) = (0.4_f32, 0.25_f32);
    // Forward: Rd = Ru*(1 + k1*Ru^2) -- lensfun::mod_coord::dist_poly3.
    let ru2 = ru_x * ru_x + ru_y * ru_y;
    let poly2 = k1 * ru2 + 1.0;
    let (rd_x, rd_y) = (ru_x * poly2, ru_y * poly2);

    let (recovered_x, recovered_y) = lens_undist_poly3(rd_x, rd_y, k1);
    assert!((recovered_x - ru_x).abs() < 1e-4, "x: {recovered_x} vs {ru_x}");
    assert!((recovered_y - ru_y).abs() < 1e-4, "y: {recovered_y} vs {ru_y}");
}

#[test]
fn lens_undist_poly5_solves_the_forward_equation() {
    let (k1, k2) = (-0.08_f32, 0.01_f32);
    let (ru_x, ru_y) = (0.35_f32, -0.2_f32);
    let ru2 = ru_x * ru_x + ru_y * ru_y;
    let poly4 = 1.0 + k1 * ru2 + k2 * ru2 * ru2;
    let (rd_x, rd_y) = (ru_x * poly4, ru_y * poly4);

    let (recovered_x, recovered_y) = lens_undist_poly5(rd_x, rd_y, k1, k2);
    assert!((recovered_x - ru_x).abs() < 1e-4, "x: {recovered_x} vs {ru_x}");
    assert!((recovered_y - ru_y).abs() < 1e-4, "y: {recovered_y} vs {ru_y}");
}

#[test]
fn lens_undist_ptlens_solves_the_forward_equation() {
    let (a, b, c) = (0.01_f32, -0.02_f32, 0.03_f32);
    let (ru_x, ru_y) = (0.3_f32, 0.1_f32);
    let ru2 = ru_x * ru_x + ru_y * ru_y;
    let r = ru2.sqrt();
    let poly3 = a * ru2 * r + b * ru2 + c * r + 1.0;
    let (rd_x, rd_y) = (ru_x * poly3, ru_y * poly3);

    let (recovered_x, recovered_y) = lens_undist_ptlens(rd_x, rd_y, a, b, c);
    assert!((recovered_x - ru_x).abs() < 1e-4, "x: {recovered_x} vs {ru_x}");
    assert!((recovered_y - ru_y).abs() < 1e-4, "y: {recovered_y} vs {ru_y}");
}

#[test]
fn lens_undist_tca_poly3_solves_the_forward_equation() {
    let (v, c, b) = (1.001_f32, 0.0002_f32, -0.0005_f32);
    let (ru_x, ru_y) = (0.4_f32, 0.15_f32);
    let ru2 = ru_x * ru_x + ru_y * ru_y;
    let poly2 = b * ru2 + c * ru2.sqrt() + v;
    let (rd_x, rd_y) = (ru_x * poly2, ru_y * poly2);

    let (recovered_x, recovered_y) = lens_undist_tca_poly3(rd_x, rd_y, v, c, b);
    assert!((recovered_x - ru_x).abs() < 1e-4, "x: {recovered_x} vs {ru_x}");
    assert!((recovered_y - ru_y).abs() < 1e-4, "y: {recovered_y} vs {ru_y}");
}

#[test]
fn lens_undist_at_origin_is_always_identity() {
    // rd == 0 short-circuits in every formula -- there's no radius to
    // solve for, and (0,0) is a fixed point of every one of these
    // radial models by construction.
    assert_eq!(lens_undist_poly3(0.0, 0.0, -0.2), (0.0, 0.0));
    assert_eq!(lens_undist_poly5(0.0, 0.0, -0.2, 0.05), (0.0, 0.0));
    assert_eq!(lens_undist_ptlens(0.0, 0.0, 0.01, -0.02, 0.03), (0.0, 0.0));
    assert_eq!(lens_undist_tca_poly3(0.0, 0.0, 1.0, 0.0, 0.0), (0.0, 0.0));
}

#[test]
fn apply_lens_distortion_falls_back_to_input_on_non_convergence() {
    // A large-enough k1 pushes poly3's Newton solve past its 6-step
    // budget for some inputs (matches upstream's own NaN contract --
    // see lens_undist_poly3's doc comment); apply_lens_distortion
    // must absorb that into "leave the coordinate unchanged", never
    // let a NaN escape into a pixel coordinate.
    let (x, y) = apply_lens_distortion(LensDistortion::Poly3 { k1: -50.0 }, 2.0, 2.0, 1.0);
    assert!(x.is_finite() && y.is_finite(), "got ({x}, {y})");
}

#[test]
fn lens_correction_is_identity_matches_apply_lens_correction_no_op_cases() {
    assert!(lens_correction_is_identity(&LensCorrection::default()));
    assert!(lens_correction_is_identity(&LensCorrection {
        profile_enabled: true,
        profile: Some(LensProfileData::default()),
        distortion_amount: 0.0,
        vignette_amount: 0.0,
        ca_amount: 0.0,
        ..LensCorrection::default()
    }));
    assert!(!lens_correction_is_identity(&LensCorrection { manual_distortion: 5.0, ..LensCorrection::default() }));
}

// -- M4 Perspective Correction --------------------------------------

#[test]
fn perspective_is_identity_matches_apply_perspective_no_op_cases() {
    assert!(perspective_is_identity(&Perspective::default()));
    assert!(!perspective_is_identity(&Perspective { vertical: 1.0, ..Perspective::default() }));
    assert!(!perspective_is_identity(&Perspective { horizontal: 1.0, ..Perspective::default() }));
    assert!(!perspective_is_identity(&Perspective { rotate: 1.0, ..Perspective::default() }));
    assert!(!perspective_is_identity(&Perspective { aspect: 1.0, ..Perspective::default() }));
    assert!(!perspective_is_identity(&Perspective { scale: 101.0, ..Perspective::default() }));
}

#[test]
fn perspective_warp_coord_at_default_params_is_the_exact_identity() {
    // a=b=0 (keystone), rotate=0, aspect=0, scale=100 -- every stage's
    // own divisor/multiplier collapses to exactly 1.0, not just
    // "close to". Checked at several points, not just the origin,
    // since a bug in any one stage could still leave (0,0) fixed.
    for (x, y) in [(0.0, 0.0), (0.5, -0.3), (-0.8, 0.9), (0.1, 0.1)] {
        let (sx, sy) = perspective_warp_coord(x, y, &Perspective::default());
        assert_eq!((sx, sy), (x, y), "input ({x}, {y})");
    }
}

#[test]
fn apply_perspective_at_identity_is_an_exact_passthrough() {
    let mut image = RgbImage::from_pixel(20, 16, image::Rgb([90, 140, 30]));
    let before = image.clone();
    apply_perspective(&mut image, &stack_with(&[]));
    assert_eq!(image, before);
}

/// Hand-derived: on the horizontal centerline (`y = 0`), `vertical`
/// (the `b` coefficient) drops out of the divisor entirely (`1 + a*x
/// + b*0 = 1 + a*x`), so with `horizontal = 0` too (`a = 0`) the
/// divisor is exactly 1.0 and `x` is unchanged, regardless of how
/// large `vertical` is -- a real behavioral guarantee (Vertical must
/// not warp the horizontal centerline at all), not just an
/// implementation detail.
#[test]
fn perspective_warp_coord_vertical_only_leaves_the_horizontal_centerline_unchanged() {
    let p = Perspective { vertical: 80.0, ..Perspective::default() };
    for x in [-0.9, -0.2, 0.0, 0.4, 0.95] {
        let (sx, sy) = perspective_warp_coord(x, 0.0, &p);
        assert_eq!(sx, x, "x should be exactly unchanged on the centerline, got sx={sx} for x={x}");
        assert_eq!(sy, 0.0, "y should stay exactly 0 on the centerline");
    }
}

/// Mirror of the above on the vertical centerline (`x = 0`):
/// `horizontal` alone must leave it exactly unchanged.
#[test]
fn perspective_warp_coord_horizontal_only_leaves_the_vertical_centerline_unchanged() {
    let p = Perspective { horizontal: 80.0, ..Perspective::default() };
    for y in [-0.9, -0.2, 0.0, 0.4, 0.95] {
        let (sx, sy) = perspective_warp_coord(0.0, y, &p);
        assert_eq!(sx, 0.0, "x should stay exactly 0 on the centerline");
        assert_eq!(sy, y, "y should be exactly unchanged on the centerline, got sy={sy} for y={y}");
    }
}

/// Hand-derived exact value: at `y = 0.5`, `vertical = 100` gives
/// `b = 1.0 * PERSPECTIVE_KEYSTONE_MAX = 0.7`, so the divisor is
/// `1 + 0.7*0.5 = 1.35` and `y` maps to `0.5 / 1.35`.
#[test]
fn perspective_warp_coord_vertical_matches_hand_derived_divisor() {
    let p = Perspective { vertical: 100.0, ..Perspective::default() };
    let (sx, sy) = perspective_warp_coord(0.0, 0.5, &p);
    assert_eq!(sx, 0.0);
    let expected = 0.5 / (1.0 + PERSPECTIVE_KEYSTONE_MAX * 0.5);
    assert!((sy - expected).abs() < 1e-6, "sy={sy} expected={expected}");
}

/// Hand-derived: `rotate = 90` degrees rotates the INVERSE map by
/// `-90`, so a destination point at `(1, 0)` samples from source
/// `(cos(-90), sin(-90)) = (0, -1)` -- i.e. the point that was to the
/// RIGHT of center in the output now reads from ABOVE center in the
/// source, matching a clockwise-appearing rotation of the displayed
/// image (consistent with this file's other rotation direction
/// convention -- see `rotate_image_90_degrees_matches_hand_derived_direction`).
#[test]
fn perspective_warp_coord_rotate_90_matches_hand_derived_direction() {
    let p = Perspective { rotate: 90.0, ..Perspective::default() };
    let (sx, sy) = perspective_warp_coord(1.0, 0.0, &p);
    assert!((sx - 0.0).abs() < 1e-5, "sx={sx}");
    assert!((sy - -1.0).abs() < 1e-5, "sy={sy}");
}

/// Hand-derived: `aspect = 100` gives `stretch = 1 +
/// PERSPECTIVE_ASPECT_MAX = 1.5`, so `x` maps to `x / 1.5`; `y` (no
/// aspect term at all) is untouched.
#[test]
fn perspective_warp_coord_aspect_matches_hand_derived_stretch() {
    let p = Perspective { aspect: 100.0, ..Perspective::default() };
    let (sx, sy) = perspective_warp_coord(0.6, 0.3, &p);
    let expected_x = 0.6 / (1.0 + PERSPECTIVE_ASPECT_MAX);
    assert!((sx - expected_x).abs() < 1e-6, "sx={sx} expected={expected_x}");
    assert_eq!(sy, 0.3);
}

/// Hand-derived: `scale = 200` (2x) divides both axes by 2 before any
/// other stage runs, sampling a source region HALF the size -- i.e.
/// the displayed image appears zoomed in 2x.
#[test]
fn perspective_warp_coord_scale_matches_hand_derived_zoom() {
    let p = Perspective { scale: 200.0, ..Perspective::default() };
    let (sx, sy) = perspective_warp_coord(0.4, -0.6, &p);
    assert!((sx - 0.2).abs() < 1e-6, "sx={sx}");
    assert!((sy - -0.3).abs() < 1e-6, "sy={sy}");
}

/// End-to-end: a nonzero Vertical correction is a genuine resample, so
/// it must visibly move where an off-center marker ends up relative
/// to a passthrough render at the SAME pixel -- same style of check
/// `lens_correction_manual_distortion_visibly_shifts_content` uses.
#[test]
fn apply_perspective_vertical_visibly_shifts_content() {
    let mut base = RgbImage::from_pixel(61, 61, image::Rgb([10, 10, 10]));
    base.put_pixel(30, 10, image::Rgb([255, 255, 255]));

    let mut corrected = base.clone();
    apply_perspective(
        &mut corrected,
        &EditStack { schema_version: 1, ops: vec![serde_json::json!({ "op": "perspective", "vertical": 80.0 })] },
    );

    assert_ne!(corrected, base, "a nonzero Vertical amount must change the image");
}

/// End-to-end: increasing Scale (zooming in) must NOT introduce any
/// blank (pure-black, `sample_bilinear`'s documented out-of-bounds
/// fallback -- see that function's own doc comment) pixels on an
/// image with no blank content to begin with, since zooming in only
/// ever samples a SMALLER, fully-in-bounds region of the source.
#[test]
fn apply_perspective_scale_up_alone_never_reveals_blank_corners() {
    let mut image = checkerboard(41);
    apply_perspective(
        &mut image,
        &EditStack { schema_version: 1, ops: vec![serde_json::json!({ "op": "perspective", "scale": 150.0 })] },
    );
    for pixel in image.pixels() {
        assert_ne!(*pixel, image::Rgb([0, 0, 0]), "scale-up-only must never sample out of bounds");
    }
}
