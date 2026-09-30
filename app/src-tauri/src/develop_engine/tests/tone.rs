use super::support::*;
use super::*;

#[test]
fn apply_edit_stack_is_a_passthrough_with_no_ops() {
    assert_pixel([100, 120, 140], &[], [100, 120, 140]);
}

/// RFC-0017: this used to be the WGSL shader's own numeric smoke test
/// value (m1-slice3-smoke's hand-derived (255,56,56)) from BEFORE
/// Exposure/Contrast were corrected to linear-light/S-curve math -- that
/// number encoded the old, wrong formula's output, not a typo to
/// preserve. Re-derived by hand under the corrected formula: (153,51,51)
/// normalizes to r=0.6, g=b=0.2 with WB an exact identity (temperature/
/// tint both 0). `apply_exposure` at +0.5 EV (gain=2^0.5): linearizing,
/// scaling, re-encoding gives r≈0.702, g=b≈0.240. `apply_contrast` at
/// amount=0.1 (contrast=10, blending 10% toward smoothstep(0,1,v)) gives
/// r≈0.7105, g=b≈0.2304. Saturation +30% around that pixel's own luma
/// (≈0.3324) gives r≈0.8239 -> 210, g=b≈0.1998 -> 51 -- confirmed
/// against this test's own actual output, not just the symbolic
/// derivation alone.
#[test]
fn apply_edit_stack_matches_the_shaders_hand_derived_combined_value() {
    assert_pixel(
        [153, 51, 51],
        &[("exposure", 0.5), ("contrast", 10.0), ("saturation", 30.0)],
        [210, 51, 51],
    );
}

/// RFC-0017: `+1 EV` no longer doubles the gamma-encoded value directly
/// (that was the bug -- a real photographic stop is a linear-light
/// gain, not a gamma-space one). Correctly linearized, gained, and
/// re-encoded, a mid-gray 100/255 lands around 138/255, not 200/255 --
/// a real, honestly-described consequence of the fix (RFC-0017 SS3.1),
/// not a regression. The name stays accurate: this is still "pure
/// exposure pushing a pixel toward white," just by the physically
/// correct amount now.
#[test]
fn apply_edit_stack_pure_exposure_doubles_toward_white() {
    assert_pixel([100, 100, 100], &[("exposure", 1.0)], [138, 138, 138]);
}

/// RFC-0017: same correction as the positive-exposure test above, mirrored.
#[test]
fn apply_edit_stack_pure_negative_exposure_halves_toward_black() {
    assert_pixel([200, 200, 200], &[("exposure", -1.0)], [146, 146, 146]);
}

/// RFC-0017: Contrast is now a smoothstep-blended S-curve, not a hard
/// linear stretch -- `contrast=50` (amount=0.5) blends the pixel 50% of
/// the way toward `smoothstep(0,1,v)`, which compresses rather than
/// linearly amplifies a value already this close to white, so the new
/// result (212) is a smaller displacement from 200 than the old
/// formula's 236 was. Less extreme is the point: the old hard stretch
/// had no roll-off at all.
#[test]
fn apply_edit_stack_pure_contrast_pushes_a_bright_pixel_brighter() {
    assert_pixel([200, 200, 200], &[("contrast", 50.0)], [212, 212, 212]);
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
/// at full weight, through `apply_adjustments`'s `apply_exposure` --
/// RFC-0017: no longer a direct gamma-space multiply, so this is now a
/// real linear-light stop rather than `0.500392 * 2^0.5`. Confirmed
/// (not just symbolically derived) to land around 150, not the old
/// formula's ~180 -- same class of correction as the pure-exposure
/// tests above, checked here specifically because it's the one place a
/// LOCAL mask's own exposure interacts with this ordering test. If the
/// curve applied AFTER (or was skipped by) the mask loop instead, the
/// mask would read the pre-curve luma=0.286275 -- clearly outside
/// [0.45,0.55] -- weight would be 0, and the pixel would stay at ~73,
/// not jump to ~150.
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
            (*actual as i32 - 150).abs() <= 2,
            "expected ~150 (curve lifts gray=73 into the mask's range, triggering its +0.5EV), got {:?}",
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

// --- RFC-0018: Parametric Tone's hue-preserving clamp ---

/// The concrete regression test for RFC-0018's own worked example: an
/// orange, near-white-clip pixel at `highlights=100` lands at exactly
/// `[1.0, 0.75, 0.55]`, not the old formula's `[1.0231, 0.7731, 0.5731]`
/// (which would then get asymmetrically truncated by the pipeline's own
/// single final clamp, since only R exceeds 1.0). Verified by hand in
/// the RFC (SS3.1/SS6): `luma([0.95,0.70,0.50]) = 0.73871`, giving a raw
/// `delta_h ~= 0.07308` -- comfortably past R's own headroom of `0.05`,
/// so R becomes the binding channel and the shared scale reduces the
/// applied delta to exactly R's headroom for all three channels.
#[test]
fn parametric_tone_highlights_at_near_clip_matches_the_hand_derived_shared_scale() {
    let rgb = [0.95, 0.70, 0.50];
    let result = apply_parametric_tone(rgb, 100.0, 0.0, 0.0, 0.0);
    let expected = [1.0, 0.75, 0.55];
    for (r, e) in result.iter().zip(expected.iter()) {
        assert!((r - e).abs() < 1e-4, "result={result:?} expected={expected:?}");
    }
}

/// The actual bug RFC-0018 fixes, stated as a hue claim rather than just
/// the raw numbers above: pairwise channel differences (R-G, G-B, R-B --
/// exactly what determines hue) must be preserved EXACTLY by this op's
/// own shared-scale delta, at the same near-clip pixel/highlights value
/// the previous test uses. The OLD (pre-RFC-0018) formula would NOT
/// preserve these once R's overshoot got asymmetrically truncated by the
/// pipeline's own final clamp (see the RFC's own worked example: old
/// differences 0.2269/0.20/0.4269 vs. the original 0.25/0.20/0.45).
#[test]
fn parametric_tone_preserves_pairwise_channel_differences_exactly_at_near_clip() {
    let rgb = [0.95, 0.70, 0.50];
    let before = [rgb[0] - rgb[1], rgb[1] - rgb[2], rgb[0] - rgb[2]];
    let result = apply_parametric_tone(rgb, 100.0, 0.0, 0.0, 0.0);
    let after = [result[0] - result[1], result[1] - result[2], result[0] - result[2]];
    for (b, a) in before.iter().zip(after.iter()) {
        assert!((b - a).abs() < 1e-4, "hue-determining differences should be exactly preserved: before={before:?} after={after:?}");
    }
}

/// This op's own contribution never needs the pipeline's downstream
/// final clamp to stay in range, for a spread of near-extreme starting
/// pixels (near-white, near-black, saturated near each) crossed with
/// the full range of all four sliders -- the direct proof the shared
/// scale actually bounds every channel, not just the one used in the
/// worked-example tests above.
#[test]
fn parametric_tone_never_exceeds_bounds_on_its_own_when_input_is_in_range() {
    let pixels = [
        [0.95, 0.70, 0.50], // near-white, saturated
        [0.05, 0.30, 0.50], // near-black, saturated
        [0.98, 0.97, 0.99], // near-white, nearly gray
        [0.02, 0.01, 0.03], // near-black, nearly gray
        [0.99, 0.20, 0.20], // extremely saturated red near white clip
    ];
    let slider_values = [-100.0, -50.0, 0.0, 50.0, 100.0];
    for &rgb in &pixels {
        for &highlights in &slider_values {
            for &shadows in &slider_values {
                for &whites in &slider_values {
                    for &blacks in &slider_values {
                        let result = apply_parametric_tone(rgb, highlights, shadows, whites, blacks);
                        for &c in &result {
                            assert!(
                                (-1e-4..=1.0 + 1e-4).contains(&c),
                                "channel {c} out of [0,1] for rgb={rgb:?} h={highlights} s={shadows} w={whites} b={blacks}"
                            );
                        }
                    }
                }
            }
        }
    }
}

/// The early-return path itself, not just "the math happens to round to
/// the same value": all four sliders at identity must return the exact
/// same floats, bit-for-bit.
#[test]
fn parametric_tone_all_sliders_at_zero_is_the_exact_identity() {
    let rgb = [0.37, 0.81, 0.12];
    let result = apply_parametric_tone(rgb, 0.0, 0.0, 0.0, 0.0);
    assert_eq!(result, rgb);
}

// --- RFC-0017: linear-light correctness ---

/// The basic correctness check any EOTF/OETF pair needs before anything
/// built on top of it (White Balance, Exposure) can be trusted: encoding
/// then decoding (or vice versa) round-trips to the original value,
/// across near-black, near-white, and representative midtones -- the
/// exact boundary points where the piecewise formula switches segments.
#[test]
fn srgb_linear_round_trip_is_the_identity() {
    for v in [0.0f32, 0.001, 0.0031308, 0.02, 0.04045, 0.2, 0.5, 0.7843, 0.98, 1.0] {
        let roundtrip = linear_to_srgb(srgb_to_linear(v));
        assert!((roundtrip - v).abs() < 1e-4, "srgb->linear->srgb should round-trip {v}, got {roundtrip}");
    }
}

/// The direct, provable claim RFC-0017 SS3.2 makes: White Balance's gain
/// is normalized so a neutral gray's LINEAR-light luma (physical
/// brightness) is exactly 1.0 after the adjustment, for any temperature/
/// tint -- not approximately, and not just at one convenient value.
/// Checked at the extremes, where the old (unnormalized) formula's
/// error was largest (~5% at temperature=100).
///
/// A first version of this test checked luma of the raw GAMMA-encoded
/// output bytes directly (`gray[i]*weight` summed) and failed at the
/// extremes -- a real finding, not a flaky test: once each channel is
/// individually re-encoded through the nonlinear `linear_to_srgb`, the
/// gamma-space weighted sum of already-unequal channels is no longer
/// equal to the gamma-encoding of the linear-space weighted sum
/// (Jensen's inequality -- `linear_to_srgb` is concave). The actual,
/// physically meaningful invariant this function's own normalization
/// guarantees is LINEAR luma, so this test decodes each output channel
/// back to linear before summing, matching what `apply_white_balance`
/// can actually promise.
#[test]
fn white_balance_preserves_a_neutral_grays_linear_luma_exactly() {
    for (temperature, tint) in [(-100.0, -100.0), (-100.0, 100.0), (0.0, 0.0), (100.0, -100.0), (100.0, 100.0), (37.0, -63.0)] {
        let gray = apply_white_balance([1.0, 1.0, 1.0], temperature, tint);
        let linear_luma = srgb_to_linear(gray[0]) * 0.2126 + srgb_to_linear(gray[1]) * 0.7152 + srgb_to_linear(gray[2]) * 0.0722;
        assert!((linear_luma - 1.0).abs() < 1e-4, "temperature={temperature} tint={tint}: expected linear luma 1.0, got {linear_luma}");
    }
}

/// `amount=0` (contrast=0) is an exact passthrough, and `amount=1`
/// (contrast=100) reduces exactly to `smoothstep(0,1,v)` -- both are
/// hand-derivable closed forms straight from `apply_contrast`'s own
/// definition, not just plausible-looking numbers.
#[test]
fn contrast_zero_is_identity_and_max_matches_smoothstep_exactly() {
    let v = [0.2, 0.5, 0.83];
    assert_eq!(apply_contrast(v, 0.0), v);

    let full = apply_contrast(v, 100.0);
    for (actual, input) in full.iter().zip(v.iter()) {
        let expected = smoothstep(0.0, 1.0, *input);
        assert!((actual - expected).abs() < 1e-5, "expected smoothstep({input})={expected}, got {actual}");
    }
}

/// Regression guard for RFC-0017's own central fix: a nominal `+1 EV`
/// must land MEANINGFULLY BELOW the old, wrong direct-gamma-doubling
/// value (200/255 for a 100/255 mid-gray) -- if this ever creeps back up
/// near 200, `apply_exposure` has silently regressed to gamma-space
/// math.
#[test]
fn exposure_on_a_midtone_is_gentler_than_the_old_gamma_space_formula() {
    let doubled = apply_exposure([100.0 / 255.0, 100.0 / 255.0, 100.0 / 255.0], 1.0);
    assert!(doubled[0] < 160.0 / 255.0, "expected a real linear-light stop to land well below the old formula's 200/255, got {}", doubled[0] * 255.0);
}

/// Global and local-mask Exposure/Contrast were already meant to agree
/// (same nominal formula, kept in sync by hand) but shared no code
/// before RFC-0017's refactor -- now they call the exact same
/// `apply_exposure`/`apply_contrast`, and this proves it rather than
/// assuming it from the refactor alone.
#[test]
fn local_mask_adjustments_agree_with_global_adjustments_on_exposure_and_contrast() {
    let rgb = [0.62, 0.31, 0.44];
    let via_global = apply_global_adjustments(rgb, 0.7, -20.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    let via_local = apply_adjustments(rgb, 0.7, -20.0, 0.0);
    for (g, l) in via_global.iter().zip(via_local.iter()) {
        assert!((g - l).abs() < 1e-5, "global={via_global:?} local={via_local:?} should agree with no WB/parametric-tone/saturation active");
    }
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
