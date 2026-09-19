use super::support::*;
use super::*;

#[test]
fn apply_edit_stack_is_a_passthrough_with_no_ops() {
    assert_pixel([100, 120, 140], &[], [100, 120, 140]);
}

/// The combined case from the WGSL shader's own numeric smoke test
/// (m1-slice3-smoke): (153,51,51) + exposure+0.5/contrast+10/
/// saturation+30 -> (255,56,56).
#[test]
fn apply_edit_stack_matches_the_shaders_hand_derived_combined_value() {
    assert_pixel(
        [153, 51, 51],
        &[("exposure", 0.5), ("contrast", 10.0), ("saturation", 30.0)],
        [255, 56, 56],
    );
}

#[test]
fn apply_edit_stack_pure_exposure_doubles_toward_white() {
    assert_pixel([100, 100, 100], &[("exposure", 1.0)], [200, 200, 200]);
}

#[test]
fn apply_edit_stack_pure_negative_exposure_halves_toward_black() {
    assert_pixel([200, 200, 200], &[("exposure", -1.0)], [100, 100, 100]);
}

#[test]
fn apply_edit_stack_pure_contrast_pushes_a_bright_pixel_brighter() {
    assert_pixel([200, 200, 200], &[("contrast", 50.0)], [236, 236, 236]);
}

#[test]
fn apply_edit_stack_full_desaturation_collapses_to_luma() {
    assert_pixel([200, 100, 50], &[("saturation", -100.0)], [118, 118, 118]);
}

#[test]
fn apply_edit_stack_clamps_past_white_instead_of_wrapping() {
    assert_pixel([250, 250, 250], &[("exposure", 2.0)], [255, 255, 255]);
}

/// The default 2-point identity curve `(0,0)-(1,1)` must be an exact
/// passthrough -- a Hermite cubic with tangents matching the line's own
/// slope exactly reproduces that line, so this should hold well within
/// the module's usual ±2/255 tolerance, not just approximately.
#[test]
fn tone_curve_identity_is_a_passthrough() {
    assert_pixel_with_curve([100, 120, 140], &[(0.0, 0.0), (1.0, 1.0)], [100, 120, 140]);
}

/// A 2-point curve degenerates to an exact straight line (see this
/// module's own doc comment on `compute_tangents`/Hermite evaluation),
/// so the expected values here are computed by hand exactly, not
/// approximated: curve(x) = 0.2 + 0.8*x. Applied to r=0/255=0,
/// g=128/255, b=255/255=1: curve(0)=0.2 -> 51, curve(128/255)=
/// 0.2+0.8*0.501960784=0.601568627 -> 153.4 -> 153, curve(1)=1.0 ->
/// 255. Each channel maps through the SAME function independently
/// (this is a master curve, not a luma-weighted saturation-style
/// adjustment) -- confirmed by these three genuinely different inputs
/// landing at their own independently-correct outputs, not all
/// shifted toward one shared luma value.
#[test]
fn tone_curve_applies_per_channel_independently() {
    assert_pixel_with_curve([0, 128, 255], &[(0.0, 0.2), (1.0, 1.0)], [51, 153, 255]);
}

/// The tone curve is the global pass's new final step, applied BEFORE
/// any mask reads `rgb` as its own accumulator base -- mirrors the
/// existing `luminance_range_mask_selection_depends_on_preceding_masks_effect`
/// pattern, but proves the curve (not another mask) is what shifts the
/// value a later luminance-range mask's own weight formula reads.
///
/// Hand-derived: gray=73 -> luma=73/255=0.286275. A 2-point curve
/// degenerates to an exact straight line (no spline approximation to
/// worry about): points (0,0.3)-(1,1) give curve(x)=0.3+0.7x, so
/// curve(0.286275)=0.3+0.7*0.286275=0.500392 -- landing comfortably
/// inside the luminance mask's [0.45,0.55] range (5 percentage points
/// of margin on either side, well clear of any rounding noise), so its
/// feather=0 weight is exactly 1.0. The mask's own +0.5EV then applies
/// at full weight: 0.500392 * 2^0.5 = 0.707745 -> round(0.707745*255)
/// = round(180.475) = 180. If the curve applied AFTER (or was skipped
/// by) the mask loop instead, the mask would read the pre-curve
/// luma=0.286275 -- clearly outside [0.45,0.55] -- weight would be 0,
/// and the pixel would stay at ~73, not jump to ~180.
#[test]
fn tone_curve_applies_after_saturation_before_masks() {
    let mut stack = stack_with_curve(&[], &[(0.0, 0.3), (1.0, 1.0)]);
    stack.ops.push(serde_json::json!({
        "op": "luminance_range_mask",
        "id": "curve-order-test",
        "rangeMin": 45.0,
        "rangeMax": 55.0,
        "feather": 0.0,
        "invert": false,
        "exposure": 0.5,
        "contrast": 0.0,
        "saturation": 0.0,
    }));
    let mut image = RgbImage::from_pixel(1, 1, image::Rgb([73, 73, 73]));
    apply_edit_stack(&mut image, &stack);
    let pixel = image.get_pixel(0, 0);
    for actual in pixel.0.iter() {
        assert!(
            (*actual as i32 - 180).abs() <= 2,
            "expected ~180 (curve lifts gray=73 into the mask's range, triggering its +0.5EV), got {:?}",
            pixel.0
        );
    }
}

#[test]
fn white_balance_temperature_positive_warms_image() {
    let rgb = [0.5, 0.5, 0.5];
    let warm = apply_white_balance(rgb, 50.0, 0.0);
    assert!(warm[0] > rgb[0], "red should increase with positive temperature");
    assert!(warm[2] < rgb[2], "blue should decrease with positive temperature");
}

#[test]
fn white_balance_tint_positive_shifts_magenta() {
    let rgb = [0.5, 0.5, 0.5];
    let magenta = apply_white_balance(rgb, 0.0, 50.0);
    assert!(magenta[1] < rgb[1], "green should decrease with positive tint (magenta shift)");
    assert!(magenta[0] > rgb[0], "red should slightly increase with positive tint");
    assert!(magenta[2] > rgb[2], "blue should slightly increase with positive tint");
}

#[test]
fn parametric_tone_highlights_boost_bright_pixels() {
    let bright = [0.8, 0.8, 0.8];
    let dark = [0.2, 0.2, 0.2];
    let boosted_bright = apply_parametric_tone(bright, 50.0, 0.0, 0.0, 0.0);
    let boosted_dark = apply_parametric_tone(dark, 50.0, 0.0, 0.0, 0.0);

    assert!(boosted_bright[0] > bright[0], "highlights adjustment should boost bright pixel");
    assert!((boosted_dark[0] - dark[0]).abs() < 1e-4, "highlights adjustment should not touch dark pixel");
}

#[test]
fn parametric_tone_shadows_lift_dark_pixels() {
    let bright = [0.8, 0.8, 0.8];
    let dark = [0.2, 0.2, 0.2];
    let lifted_dark = apply_parametric_tone(dark, 0.0, 50.0, 0.0, 0.0);
    let lifted_bright = apply_parametric_tone(bright, 0.0, 50.0, 0.0, 0.0);

    assert!(lifted_dark[0] > dark[0], "shadows adjustment should lift dark pixel");
    assert!((lifted_bright[0] - bright[0]).abs() < 1e-4, "shadows adjustment should not touch bright pixel");
}

#[test]
fn apply_edit_stack_round_trips_with_white_balance_and_tone_ops() {
    let mut image = RgbImage::from_pixel(4, 4, image::Rgb([128, 128, 128]));
    let stack = EditStack {
        schema_version: 1,
        ops: vec![
            serde_json::json!({ "op": "temperature", "value": 20.0 }),
            serde_json::json!({ "op": "tint", "value": -10.0 }),
            serde_json::json!({ "op": "highlights", "value": -15.0 }),
            serde_json::json!({ "op": "shadows", "value": 25.0 }),
            serde_json::json!({ "op": "whites", "value": 10.0 }),
            serde_json::json!({ "op": "blacks", "value": -5.0 }),
        ],
    };
    apply_edit_stack(&mut image, &stack);
    let p = image.get_pixel(0, 0).0;
    assert!(p[0] > 0 && p[1] > 0 && p[2] > 0);
}
