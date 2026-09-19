use super::support::*;
use super::*;

#[test]
fn linear_gradient_mask_before_start_gets_no_local_adjustment() {
    assert_mask_pixel(mask_stack((0.6, 0.5), (0.9, 0.5), 0.0, 1.0), [100, 100, 100]);
}

#[test]
fn linear_gradient_mask_after_end_gets_full_local_adjustment() {
    assert_mask_pixel(mask_stack((0.1, 0.5), (0.4, 0.5), 0.0, 1.0), [200, 200, 200]);
}

#[test]
fn linear_gradient_mask_midpoint_blends_halfway() {
    assert_mask_pixel(mask_stack((0.2, 0.5), (0.8, 0.5), 0.0, 1.0), [150, 150, 150]);
}

/// Feather widens the transition band around the midpoint rather than
/// only softening the corners -- at feather=50 the pixel sitting
/// exactly AT the start point (t=0) gets weight 0.25, not 0. A
/// deliberate choice matching real Lightroom's own feather model (see
/// `mask_weight`'s doc comment) -- this test pins that behavior down.
#[test]
fn linear_gradient_mask_feather_moves_the_anchor_off_zero() {
    assert_mask_pixel(mask_stack((0.5, 0.5), (0.8, 0.5), 50.0, 1.0), [125, 125, 125]);
}

/// Default (invert=false) applies OUTSIDE the ellipse -- a pixel at
/// dead center is fully "inside", so gets NO local adjustment.
#[test]
fn radial_gradient_mask_center_default_outside_gets_no_local_adjustment() {
    assert_mask_pixel(
        radial_mask_stack((0.5, 0.5), 0.3, 0.3, 0.0, false, 1.0),
        [100, 100, 100],
    );
}

/// Same geometry, invert=true (applies INSIDE) -- center gets the full
/// local adjustment.
#[test]
fn radial_gradient_mask_center_inverted_gets_full_local_adjustment() {
    assert_mask_pixel(
        radial_mask_stack((0.5, 0.5), 0.3, 0.3, 0.0, true, 1.0),
        [200, 200, 200],
    );
}

/// Well outside the ellipse, default (outside) -- gets the full local
/// adjustment (the opposite of the center case above).
#[test]
fn radial_gradient_mask_outside_default_gets_full_local_adjustment() {
    assert_mask_pixel(
        radial_mask_stack((0.1, 0.1), 0.05, 0.05, 0.0, false, 1.0),
        [200, 200, 200],
    );
}

/// Center is always fully "inside" regardless of feather -- a heavily
/// feathered, inverted mask still gives the center the full effect.
#[test]
fn radial_gradient_mask_center_stays_fully_inside_even_when_feathered() {
    assert_mask_pixel(
        radial_mask_stack((0.5, 0.5), 0.3, 0.3, 50.0, true, 1.0),
        [200, 200, 200],
    );
}

/// Well outside, feathered, inverted (inside-only effect) -- stays
/// unaffected, confirming feathering doesn't leak the inside effect
/// arbitrarily far outward.
#[test]
fn radial_gradient_mask_outside_feathered_inverted_stays_unaffected() {
    assert_mask_pixel(
        radial_mask_stack((0.1, 0.1), 0.05, 0.05, 50.0, true, 1.0),
        [100, 100, 100],
    );
}

/// Pure red, full ellipse weight, permissive pupil size, full darken --
/// the strongest correction case: desaturated to luma (0.2126 of 255 =
/// 54.2) then darkened by 60% of that (54.2 * 0.4 = 21.7) -> ~22.
#[test]
fn red_eye_mask_pure_red_full_strength_darkens_to_near_black_gray() {
    assert_mask_pixel_from(
        [255, 0, 0],
        red_eye_mask_stack((0.5, 0.5), 0.3, 0.3, 0.0, 100.0, 100.0),
        [22, 22, 22],
    );
}

/// Same pure-red, full-weight case but `darken=0` -- still fully
/// desaturates to luma (that's the tool's whole "de-redify" point, not
/// optional), just without the extra darkening step: 0.2126 * 255 =
/// 54.2 -> 54.
#[test]
fn red_eye_mask_darken_zero_still_fully_desaturates() {
    assert_mask_pixel_from(
        [255, 0, 0],
        red_eye_mask_stack((0.5, 0.5), 0.3, 0.3, 0.0, 100.0, 0.0),
        [54, 54, 54],
    );
}

/// A moderately (not purely) red pixel, strict pupil size (0 ->
/// threshold 1.0): redness = 0.7059 - 0.3922 = 0.3137, red_factor =
/// 0.3137 / 1.0 = 0.3137 (not clamped) -- only a partial correction
/// blends in even at full ellipse weight and full darken.
#[test]
fn red_eye_mask_strict_pupil_size_only_partially_corrects_a_faint_red() {
    assert_mask_pixel_from(
        [180, 100, 100],
        red_eye_mask_stack((0.5, 0.5), 0.3, 0.3, 0.0, 0.0, 100.0),
        [138, 83, 83],
    );
}

/// Same faint-red pixel, permissive pupil size (100 -> threshold
/// floored at 0.05): red_factor = 0.3137 / 0.05 = 6.27, clamped to 1.0
/// -- the SAME pixel now gets the full correction, demonstrating pupil
/// size widens what counts as "red enough" rather than changing the
/// target color itself.
#[test]
fn red_eye_mask_permissive_pupil_size_fully_corrects_the_same_faint_red() {
    assert_mask_pixel_from(
        [180, 100, 100],
        red_eye_mask_stack((0.5, 0.5), 0.3, 0.3, 0.0, 100.0, 100.0),
        [47, 47, 47],
    );
}

/// Well outside the ellipse (mirroring radial's own "outside" case) --
/// a pure red pixel gets NO correction regardless of pupil size/darken,
/// since ellipse weight is 0 there.
#[test]
fn red_eye_mask_outside_ellipse_pure_red_stays_unaffected() {
    assert_mask_pixel_from(
        [255, 0, 0],
        red_eye_mask_stack((0.1, 0.1), 0.05, 0.05, 0.0, 100.0, 100.0),
        [255, 0, 0],
    );
}

/// Inside the ellipse (full spatial weight) but a NEUTRAL gray pixel --
/// zero redness means zero correction regardless of pupil size/darken,
/// the key behavior distinguishing this from a plain Radial mask (which
/// would uniformly grade every pixel in the oval, red or not).
#[test]
fn red_eye_mask_inside_ellipse_non_red_pixel_stays_unaffected() {
    assert_mask_pixel_from(
        [100, 100, 100],
        red_eye_mask_stack((0.5, 0.5), 0.3, 0.3, 0.0, 100.0, 100.0),
        [100, 100, 100],
    );
}

/// A real committed test photo (`test_image/Red-eye-flash.jpeg`, a
/// portrait with genuine flash-induced red-eye in both pupils) --
/// every other case above uses a synthetic 1x1 pixel, hand-computed
/// exactly; this one instead exercises the whole formula against real
/// photographic noise/gradients, on the real content shape (a small
/// saturated-red disc surrounded by blue iris and skin) this feature
/// was actually built for. The reddest pixel is found programmatically
/// (max `r - max(g,b)`) rather than hand-eyeballed, so this test stays
/// correct if the fixture image is ever swapped for another one.
#[test]
fn red_eye_mask_corrects_a_real_photo_pupil_without_touching_skin() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../test_image/Red-eye-flash.jpeg");
    let img = image::open(path)
        .expect("test_image/Red-eye-flash.jpeg should be present and decodable")
        .to_rgb8();
    let (w, h) = img.dimensions();

    let mut pupil = (0u32, 0u32);
    let mut pupil_redness = -1.0f32;
    for (x, y, p) in img.enumerate_pixels() {
        let r = p[0] as f32 / 255.0;
        let g = p[1] as f32 / 255.0;
        let b = p[2] as f32 / 255.0;
        let redness = r - g.max(b);
        if redness > pupil_redness {
            pupil_redness = redness;
            pupil = (x, y);
        }
    }
    assert!(
        pupil_redness > 0.3,
        "fixture should contain a clearly red pupil pixel, got max redness {pupil_redness}"
    );

    let stack = EditStack {
        schema_version: 1,
        ops: vec![serde_json::json!({
            "op": "red_eye_mask",
            "id": "real-photo-test",
            "center": { "x": (pupil.0 as f32 + 0.5) / w as f32, "y": (pupil.1 as f32 + 0.5) / h as f32 },
            "radiusX": 0.025,
            "radiusY": 0.025,
            "feather": 40.0,
            "pupilSize": 80.0,
            "darken": 80.0,
        })],
    };

    let mut edited = img.clone();
    apply_edit_stack(&mut edited, &stack);

    let after = edited.get_pixel(pupil.0, pupil.1);
    let redness_after = after[0] as f32 / 255.0 - (after[1] as f32 / 255.0).max(after[2] as f32 / 255.0);
    assert!(
        redness_after < pupil_redness * 0.3,
        "pupil redness should drop substantially after correction: before={pupil_redness}, after={redness_after}"
    );

    // Top-left corner (background, well outside either eye's small
    // correction radius) must stay unchanged -- confirms the
    // redness-selective spatial gating actually holds on a real photo,
    // not just this file's synthetic 1x1 fixtures.
    let far = img.get_pixel(w / 20, h / 20);
    let far_after = edited.get_pixel(w / 20, h / 20);
    assert_eq!(far, far_after, "a background pixel far from either eye should be untouched");
}

/// A hard-edged dab (hardness=100) dead center gets the full local
/// adjustment -- same shape as the radial "center" case.
#[test]
fn brush_mask_dab_at_center_gets_full_local_adjustment() {
    assert_mask_pixel(
        brush_mask_stack(vec![dab(0.5, 0.5, 0.3, 100.0, 1.0, "add")], false, 1.0),
        [200, 200, 200],
    );
}

/// Exactly at the dab's radius boundary -- normalized_d=1.0 is
/// excluded (falloff=0), matching `dab_falloff`'s `>= 1.0` cutoff.
#[test]
fn brush_mask_dab_exactly_at_radius_boundary_gets_no_local_adjustment() {
    assert_mask_pixel(
        brush_mask_stack(vec![dab(0.2, 0.5, 0.3, 100.0, 1.0, "add")], false, 1.0),
        [100, 100, 100],
    );
}

/// Well outside any dab's radius -- no local adjustment.
#[test]
fn brush_mask_pixel_outside_every_dab_gets_no_local_adjustment() {
    assert_mask_pixel(
        brush_mask_stack(vec![dab(0.1, 0.1, 0.05, 100.0, 1.0, "add")], false, 1.0),
        [100, 100, 100],
    );
}

/// Two overlapping ADD dabs at the identical position/radius/hardness
/// (hardness=0, so the fixed sample point at distance 0.15 from a
/// radius-0.3 dab center falls exactly halfway through the falloff
/// band, giving weight 0.5) must union via max, not sum -- two
/// identical partial-coverage dabs still give weight 0.5, not 1.0.
#[test]
fn brush_mask_overlapping_add_dabs_union_via_max_not_sum() {
    assert_mask_pixel(
        brush_mask_stack(
            vec![
                dab(0.35, 0.5, 0.3, 0.0, 1.0, "add"),
                dab(0.35, 0.5, 0.3, 0.0, 1.0, "add"),
            ],
            false,
            1.0,
        ),
        [150, 150, 150],
    );
}

/// An ERASE dab reduces the running weight MULTIPLICATIVELY, not by
/// subtraction -- an add dab giving weight 0.5, followed by an erase
/// dab with its own falloff 0.5 at the same spot, gives weight
/// 0.5*(1-0.5)=0.25 (multiplicative). A subtractive formula
/// (0.5-0.5=0) would produce a visibly different pixel ([100,100,100]
/// instead of [125,125,125]), so this test distinguishes the two.
#[test]
fn brush_mask_erase_dab_reduces_weight_multiplicatively() {
    assert_mask_pixel(
        brush_mask_stack(
            vec![
                dab(0.35, 0.5, 0.3, 0.0, 1.0, "add"),
                dab(0.35, 0.5, 0.3, 0.0, 1.0, "erase"),
            ],
            false,
            1.0,
        ),
        [125, 125, 125],
    );
}

/// gray=26 -> luma~0.102, well below range [0.3,0.7] with feather=0 --
/// no local adjustment.
#[test]
fn luminance_range_mask_below_range_gets_no_local_adjustment() {
    assert_luminance_pixel(26, luminance_mask_stack(30.0, 70.0, 0.0, false, 1.0), [26, 26, 26]);
}

/// gray=128 -> luma~0.502, inside [0.3,0.7] with feather=0 -- full
/// local adjustment (exposure+1.0 doubles toward white, clamped).
#[test]
fn luminance_range_mask_inside_range_gets_full_local_adjustment() {
    assert_luminance_pixel(128, luminance_mask_stack(30.0, 70.0, 0.0, false, 1.0), [255, 255, 255]);
}

/// gray=230 -> luma~0.902, well above range [0.3,0.7] with feather=0 --
/// no local adjustment.
#[test]
fn luminance_range_mask_above_range_gets_no_local_adjustment() {
    assert_luminance_pixel(230, luminance_mask_stack(30.0, 70.0, 0.0, false, 1.0), [230, 230, 230]);
}

/// gray=51 -> luma=0.2 exactly, feather=40 -> feather_width=0.2, so
/// this sits exactly halfway through the rising edge below range_min
/// (0.3-0.2=0.1 at weight 0, 0.3 at weight 1, 0.2 is the midpoint) --
/// weight=0.5, half-strength local adjustment.
#[test]
fn luminance_range_mask_feathered_edge_blends_halfway() {
    assert_luminance_pixel(51, luminance_mask_stack(30.0, 70.0, 40.0, false, 1.0), [76, 76, 76]);
}

/// Order-dependency (design point 4): a linear mask with full weight
/// at the test point doubles gray=64 (luma~0.251) to ~0.502 BEFORE the
/// luminance-range mask (range [45,55], feather=0) evaluates its own
/// weight against the ALREADY-DOUBLED rgb -- 0.502 falls INSIDE
/// [0.45,0.55], so the luminance mask's own +0.5EV boost also applies,
/// landing at ~181. If mask order didn't matter (an incorrect
/// implementation evaluating luminance weight against the ORIGINAL,
/// pre-linear-mask rgb=0.251, which falls OUTSIDE the range), the
/// luminance mask would have no effect at all and the result would
/// stop at ~128 (just the linear mask's own doubling) -- a value this
/// test's ±2 tolerance cannot accidentally satisfy alongside 181,
/// making this a real, discriminating test, not just a smoke check.
#[test]
fn luminance_range_mask_selection_depends_on_preceding_masks_effect() {
    let stack = EditStack {
        schema_version: 1,
        ops: vec![
            serde_json::json!({
                "op": "linear_gradient_mask",
                "id": "order-test-linear",
                "start": { "x": 0.1, "y": 0.5 },
                "end": { "x": 0.4, "y": 0.5 },
                "feather": 0.0,
                "invert": false,
                "exposure": 1.0,
                "contrast": 0.0,
                "saturation": 0.0,
            }),
            serde_json::json!({
                "op": "luminance_range_mask",
                "id": "order-test-luminance",
                "rangeMin": 45.0,
                "rangeMax": 55.0,
                "feather": 0.0,
                "invert": false,
                "exposure": 0.5,
                "contrast": 0.0,
                "saturation": 0.0,
            }),
        ],
    };
    assert_luminance_pixel(64, stack, [181, 181, 181]);
}

/// The exact regression case for the bug the design review caught:
/// `range=0, feather=0` (the most obvious "strict color match" setting)
/// against the EXACT reference color (`ref_gray = 128/255`, matching
/// the pixel's own f32 conversion bit-for-bit, so `dist=0` exactly) --
/// must still select the clicked pixel at full weight, not zero.
#[test]
fn color_range_mask_exact_match_at_zero_feather_gets_full_local_adjustment() {
    let ref_gray = 128.0 / 255.0;
    assert_color_pixel(128, color_mask_stack(ref_gray, 0.0, 0.0, false, 1.0), [255, 255, 255]);
}

/// White (gray=255) against a black reference (`ref_gray=0`) is
/// `dist = sqrt(3)` -- the maximum possible distance -- well outside
/// `range=25, feather=20`'s `threshold + denom` (~0.606): no local
/// adjustment.
#[test]
fn color_range_mask_far_pixel_gets_no_local_adjustment() {
    assert_color_pixel(255, color_mask_stack(0.0, 25.0, 20.0, false, 1.0), [255, 255, 255]);
}

/// `ref_gray=0.3`, pixel gray=153/255=0.6 exactly -> `dist = 0.3*sqrt(3)
/// = 0.5196152`. With `range=25` (`threshold=0.4330127`) and `feather=20`
/// (`denom=0.1732051`), that's exactly `threshold + 0.5*denom` -- the
/// midpoint of the transition band -- so `weight=0.5`, a half-strength
/// local adjustment (exposure+1.0 doubles toward white at half
/// strength: `0.6 + (1.2-0.6)*0.5 = 0.9` -> byte 230).
#[test]
fn color_range_mask_feathered_edge_blends_halfway() {
    assert_color_pixel(153, color_mask_stack(0.3, 25.0, 20.0, false, 1.0), [230, 230, 230]);
}

/// Order-dependency, mirroring luminance range's own such test: a
/// linear mask (full weight at the test point) boosts gray=128/255 by
/// +0.3EV to ~0.617987 BEFORE the color-range mask (ref_gray=0.6,
/// range=5 -> threshold=0.0866025, feather=0 -> near-hard edge) checks
/// its own distance -- `dist(0.617987, 0.6) = 0.031152`, well inside
/// threshold, so the color-range mask's own +0.5EV also applies, landing
/// at ~0.873962 (byte ~223). If mask order didn't matter (evaluated
/// against the ORIGINAL rgb=0.501961 instead), `dist = 0.169809` is far
/// outside threshold -- weight=0, and the result would stop at just the
/// linear mask's own boost (~0.617987, byte ~158) -- a value this test's
/// ±2 tolerance cannot accidentally satisfy alongside 223.
#[test]
fn color_range_mask_selection_depends_on_preceding_masks_effect() {
    let stack = EditStack {
        schema_version: 1,
        ops: vec![
            serde_json::json!({
                "op": "linear_gradient_mask",
                "id": "color-order-test-linear",
                "start": { "x": 0.1, "y": 0.5 },
                "end": { "x": 0.4, "y": 0.5 },
                "feather": 0.0,
                "invert": false,
                "exposure": 0.3,
                "contrast": 0.0,
                "saturation": 0.0,
            }),
            serde_json::json!({
                "op": "color_range_mask",
                "id": "color-order-test-color",
                "refColor": { "r": 0.6, "g": 0.6, "b": 0.6 },
                "range": 5.0,
                "feather": 0.0,
                "invert": false,
                "exposure": 0.5,
                "contrast": 0.0,
                "saturation": 0.0,
            }),
        ],
    };
    assert_color_pixel(128, stack, [223, 223, 223]);
}

/// `radius = 1px` (`1.0 / SPOT_TEST_SIZE`) with `feather = 0` selects
/// ONLY the exact `dest` pixel at full weight: at its immediate
/// neighbor, `normalized_d = 1.0` exactly, and `spot_mask_weight`'s
/// `(1.0 - 1.0) / 0.001` clamps to exactly 0 (see that function's own
/// doc comment on the `feather=0` near-hard-edge case) -- a hand-
/// verifiable single-pixel replacement, not an approximate blend.
#[test]
fn spot_mask_clone_copies_exact_pixel_with_hard_edge_selection() {
    let mut image = spot_test_image();
    let op = spot_mask_op((8, 20), (32, 20), 1.0 / SPOT_TEST_SIZE as f32, 0.0, "clone");
    apply_edit_stack(&mut image, &EditStack { schema_version: 1, ops: vec![op] });
    assert_eq!(
        image.get_pixel(8, 20).0,
        [SPOT_TEST_RIGHT; 3],
        "dest pixel should be fully replaced by the source pixel's content"
    );
    assert_eq!(
        image.get_pixel(7, 20).0,
        [SPOT_TEST_LEFT; 3],
        "one pixel outside the selection radius must be left untouched"
    );
}

/// Same dest/source placement as the clone test above, but `mode:
/// "heal"`: `compute_heal_shift` samples a ring around `dest` (all
/// LEFT) and around `source` (all RIGHT), so `heal_shift = LEFT -
/// RIGHT` exactly cancels the tone difference the raw clone would
/// otherwise introduce -- the healed pixel ends up matching its own
/// LEFT surroundings instead of visibly showing RIGHT content, unlike
/// the plain-clone case just above.
#[test]
fn spot_mask_heal_matches_surrounding_tone_instead_of_the_source_tone() {
    let mut image = spot_test_image();
    let op = spot_mask_op((8, 20), (32, 20), 1.0 / SPOT_TEST_SIZE as f32, 0.0, "heal");
    apply_edit_stack(&mut image, &EditStack { schema_version: 1, ops: vec![op] });
    let dest = image.get_pixel(8, 20).0;
    for (actual, expected) in dest.iter().zip([SPOT_TEST_LEFT as i32; 3].iter()) {
        assert!(
            (*actual as i32 - expected).abs() <= 2,
            "expected healed pixel ~{expected:?} (matching LEFT surroundings), got {dest:?}"
        );
    }
}

/// `radius = 4px`, `feather = 40` -- at the exact boundary pixel
/// (`normalized_d = 1.0`), `spot_mask_weight`'s formula reduces to
/// `softness / (2*softness) = 0.5` for ANY `feather > 0` (the same
/// feather-independent "midpoint of the transition band" fact
/// `color_range_mask_feathered_edge_blends_halfway` already
/// establishes for color-range masks) -- so this pixel's own original
/// LEFT value (40) and the fully-cloned RIGHT content (200) blend at
/// an exact 50/50 midpoint: `(40 + 200) / 2 = 120`, with no rounding
/// ambiguity.
#[test]
fn spot_mask_feathered_edge_blends_halfway() {
    let mut image = spot_test_image();
    let op = spot_mask_op((8, 20), (32, 20), 4.0 / SPOT_TEST_SIZE as f32, 40.0, "clone");
    apply_edit_stack(&mut image, &EditStack { schema_version: 1, ops: vec![op] });
    assert_eq!(image.get_pixel(12, 20).0, [120; 3], "boundary pixel should blend exactly halfway between its own value and the sampled content");
}

/// A spot mask reads `pre_mask` (the fully-graded buffer written by
/// Pass 7), not the raw source pixel -- this is the entire reason
/// `apply_edit_stack` splits its final loop into two passes (see Pass
/// 7's own doc comment). A preceding `+1.0EV` exposure op doubles the
/// source pixel's raw gray=80 to a graded 160 BEFORE the clone samples
/// it; if this mask instead sampled the raw, ungraded pixel, the dest
/// pixel would end up at 80, not 160 -- a real, distinguishable
/// regression this test would catch.
#[test]
fn spot_mask_clone_samples_the_graded_source_not_the_raw_pixel() {
    let mut image = RgbImage::from_pixel(10, 10, image::Rgb([80, 80, 80]));
    let exposure_op = serde_json::json!({ "op": "exposure", "value": 1.0 });
    let spot_op = spot_mask_op((2, 5), (7, 5), 1.0 / 10.0, 0.0, "clone");
    apply_edit_stack(&mut image, &EditStack { schema_version: 1, ops: vec![exposure_op, spot_op] });
    let dest = image.get_pixel(2, 5).0;
    for (actual, expected) in dest.iter().zip([160i32; 3].iter()) {
        assert!(
            (*actual as i32 - expected).abs() <= 2,
            "expected dest ~{expected:?} (graded source, 80 doubled to 160), got {dest:?}"
        );
    }
}

/// M4 Slice 2 (brush-like spot removal): a stroke is now a `Vec` of
/// dabs, not a single dest circle. Two 1px, hard-edged (`feather=0`)
/// dabs at (8,20) and (14,20) share ONE `sourceOffset` (derived from
/// the first dab to `source_px=(32,20)`, i.e. `+24px` in x). Both dabs'
/// own centers get full weight from THEIR OWN dab (max-across-dabs
/// accumulation, same as `brush_mask_weight`'s `Add` dabs) and each
/// samples content offset by that same `+24px`, landing on RIGHT-half
/// content in both cases -- proving the offset is shared across the
/// whole stroke, not recomputed per dab. A pixel roughly midway between
/// the two dabs, more than 1px from either center, gets weight 0 and
/// stays untouched -- proving coverage is a real per-dab union, not a
/// single blob spanning the dabs' bounding box.
#[test]
fn spot_mask_multi_dab_stroke_shares_one_source_offset_across_every_dab() {
    let mut image = spot_test_image();
    let op = spot_mask_multi_dab_op(&[(8, 20), (14, 20)], (32, 20), 1.0 / SPOT_TEST_SIZE as f32, 0.0, "clone");
    apply_edit_stack(&mut image, &EditStack { schema_version: 1, ops: vec![op] });
    assert_eq!(
        image.get_pixel(8, 20).0,
        [SPOT_TEST_RIGHT; 3],
        "first dab's own center should sample RIGHT content via the shared +24px offset"
    );
    assert_eq!(
        image.get_pixel(14, 20).0,
        [SPOT_TEST_RIGHT; 3],
        "second dab's own center should ALSO sample RIGHT content via the SAME shared offset"
    );
    assert_eq!(
        image.get_pixel(11, 20).0,
        [SPOT_TEST_LEFT; 3],
        "a pixel more than 1px from either dab center should be outside both dabs' coverage and left untouched"
    );
}
