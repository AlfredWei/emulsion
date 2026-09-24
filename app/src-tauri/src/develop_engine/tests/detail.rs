use super::support::*;
use super::*;

/// amount=0 must be an EXACT passthrough regardless of the rest of the
/// algorithm -- not just numerically close to identity, structurally
/// guaranteed by `apply_edit_stack` skipping the entire dark-channel/
/// transmission computation at amount=0 (see its own doc comment), so
/// this also doubles as a check that the skip path doesn't panic/
/// misbehave on a real multi-pixel image.
#[test]
fn dehaze_amount_zero_is_an_exact_passthrough() {
    let mut image = dehaze_test_image(20, 20, [176, 171, 166], [242, 242, 242]);
    apply_edit_stack(&mut image, &stack_with(&[("dehaze", 0.0)]));
    assert_eq!(*image.get_pixel(19, 19), image::Rgb([176, 171, 166]));
    assert_eq!(*image.get_pixel(0, 0), image::Rgb([242, 242, 242]));
}

/// Hand-derived recovery at amount=100, chosen so `A=(1,1,1)` (the sky
/// patch is pure white) -- this eliminates the `I^c/A^c` division's
/// denominator entirely, keeping the arithmetic exact rather than
/// approximated. Scene = (204,153,102) = (0.8, 0.6, 0.4) exactly (each
/// is a clean multiple of 1/255).
///
/// minChannel(scene) = min(0.8,0.6,0.4)/1 = 0.4 (blue channel) --
/// dominates the whole image's dark channel per `dehaze_test_image`'s
/// own doc comment (patch too small to ever fill an entire 15x15
/// window), so darkChannel = 0.4 everywhere, EXACTLY (not approximated
/// -- every window, including ones centered on the patch itself,
/// includes scene pixels, and scene's own minChannel is the global
/// minimum).
///
/// t_raw = 1 - 0.95*0.4 = 0.62 exactly, uniform -> t_refined = 0.62
/// exactly too (box-mean of a constant field is that same constant).
///
/// Recovery (a pixel far from the patch, e.g. (19,19)):
/// r: (0.8-1.0)/0.62 + 1.0 = 1 - 0.2/0.62 = 1 - 10/31 = 0.677419 -> 172.7 -> 173
/// g: (0.6-1.0)/0.62 + 1.0 = 1 - 0.4/0.62 = 1 - 20/31 = 0.354839 -> 90.5 -> 90
/// b: (0.4-1.0)/0.62 + 1.0 = 1 - 0.6/0.62 = 1 - 30/31 = 0.032258 -> 8.2 -> 8
#[test]
fn dehaze_amount_100_matches_hand_derived_recovery() {
    let mut image = dehaze_test_image(20, 20, [204, 153, 102], [255, 255, 255]);
    apply_edit_stack(&mut image, &stack_with(&[("dehaze", 100.0)]));
    let pixel = image.get_pixel(19, 19);
    for (actual, expected) in pixel.0.iter().zip([173, 90, 8].iter()) {
        assert!(
            (*actual as i32 - expected).abs() <= 2,
            "expected ~{expected:?}, got {actual} (full pixel {:?})",
            pixel.0
        );
    }
}

/// A sky pixel that already equals atmospheric light exactly (I=A)
/// should recover UNCHANGED -- physically sensible (a clear-sky pixel
/// needs no correction) and a real property of the formula (`(A-A)/t +
/// A = A` for any t), worth asserting explicitly rather than only
/// checking the scene-pixel case above.
#[test]
fn dehaze_pixel_already_at_atmospheric_light_is_unchanged() {
    let mut image = dehaze_test_image(20, 20, [204, 153, 102], [255, 255, 255]);
    apply_edit_stack(&mut image, &stack_with(&[("dehaze", 100.0)]));
    assert_eq!(*image.get_pixel(0, 0), image::Rgb([255, 255, 255]));
}

/// `dehaze_atmospheric_light` must pick a REAL pixel's whole RGB triple
/// (argmax-by-luminance), not a synthesized independent-per-channel-max
/// color -- the real bug a design review caught before this was
/// written. `[1,0,0]` and `[0,1,0]` are each brightest in a DIFFERENT
/// single channel; a per-channel-max implementation would incorrectly
/// synthesize `[1,1,0]`, a color present in neither input pixel. The
/// correct argmax-by-luminance picks `[0,1,0]` (luma 0.7152, the
/// highest of the three candidates -- [1,0,0] is 0.2126, [0.5,0.5,0.5]
/// is 0.5) as the WHOLE winning triple.
#[test]
fn dehaze_atmospheric_light_picks_a_real_pixel_not_a_synthesized_color() {
    let pixels = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.5, 0.5, 0.5]];
    assert_eq!(dehaze_atmospheric_light(&pixels), [0.0, 1.0, 0.0]);
}

/// `separable_min_filter` against a small hand-computed 5x1 array,
/// radius=1 (3-tap window). Height=1 makes the vertical pass a no-op
/// (each column's own single value, min'd with itself 3 times via
/// clamped taps), isolating the horizontal pass's own correctness:
/// x=0: taps (clamped) at indices 0,0,1 -> min(5,5,3)=3
/// x=1: indices 0,1,2 -> min(5,3,8)=3
/// x=2: indices 1,2,3 -> min(3,8,1)=1
/// x=3: indices 2,3,4 -> min(8,1,9)=1
/// x=4: indices 3,4,4 -> min(1,9,9)=1
#[test]
fn separable_min_filter_matches_hand_computed_values() {
    let buf = [5.0f32, 3.0, 8.0, 1.0, 9.0];
    let result = separable_min_filter(&buf, 5, 1, 1);
    assert_eq!(result, vec![3.0, 3.0, 1.0, 1.0, 1.0]);
}

/// `separable_mean_filter` against the same 5x1 array/radius, same
/// no-op-vertical-pass isolation as the min-filter test above:
/// x=0: (5+5+3)/3 = 4.333..
/// x=1: (5+3+8)/3 = 5.333..
/// x=2: (3+8+1)/3 = 4.0
/// x=3: (8+1+9)/3 = 6.0
/// x=4: (1+9+9)/3 = 6.333..
#[test]
fn separable_mean_filter_matches_hand_computed_values() {
    let buf = [5.0f32, 3.0, 8.0, 1.0, 9.0];
    let result = separable_mean_filter(&buf, 5, 1, 1);
    let expected = [13.0 / 3.0, 16.0 / 3.0, 4.0, 6.0, 19.0 / 3.0];
    for (actual, expected) in result.iter().zip(expected.iter()) {
        assert!((actual - expected).abs() < 0.001, "expected {expected}, got {actual}");
    }
}

/// amount=0 must be an EXACT passthrough (the additive-delta formula
/// makes this true even without a skip -- `delta = (luma-blurred)*0.0
/// == 0.0` always), and `apply_edit_stack` also skips the call
/// entirely at amount=0 for the same reason Dehaze does. Uses a
/// non-uniform image (so the blur itself is doing real, non-trivial
/// work) to make sure passthrough isn't just a degenerate side effect
/// of a uniform field.
#[test]
fn apply_local_contrast_amount_zero_is_exact_passthrough() {
    let mut graded = vec![[0.0, 0.0, 0.0], [0.3, 0.3, 0.3], [0.9, 0.9, 0.9]];
    let before = graded.clone();
    apply_local_contrast(&mut graded, 3, 1, 1, 0.0);
    assert_eq!(graded, before);
}

/// A perfectly uniform field's box-mean equals that same constant
/// everywhere (exact, not approximate), so `luma - blurred == 0` at
/// every pixel regardless of amount -- Texture/Clarity must leave a
/// flat-color image untouched at ANY amount, positive or negative.
#[test]
fn apply_local_contrast_uniform_field_is_unaffected_by_any_amount() {
    let mut graded = vec![[0.47, 0.31, 0.16]; 9];
    apply_local_contrast(&mut graded, 3, 3, 6, 100.0);
    apply_local_contrast(&mut graded, 3, 3, 24, -100.0);
    for rgb in &graded {
        assert!((rgb[0] - 0.47).abs() < 1e-5, "{rgb:?}");
        assert!((rgb[1] - 0.31).abs() < 1e-5, "{rgb:?}");
        assert!((rgb[2] - 0.16).abs() < 1e-5, "{rgb:?}");
    }
}

/// Hand-derived delta on a 3x1 row (radius=1, so every column's window
/// spans the whole row after edge-clamping -- clamped taps repeat the
/// boundary value, so each column's mean is a DIFFERENT weighted
/// average, not a plain 3-way split). Gray pixels (r=g=b) make
/// luma == the shared channel value exactly, sidestepping the luma
/// weight constants entirely.
///
/// col0: taps (clamped) at indices 0,0,1 -> mean(0.0,0.0,0.3) = 0.1;
///       luma=0.0; delta = (0.0-0.1)*1.0 = -0.1 -> expected -0.1
/// col1: taps at indices 0,1,2 -> mean(0.0,0.3,0.9) = 0.4;
///       luma=0.3; delta = (0.3-0.4)*1.0 = -0.1 -> expected 0.2
/// col2: taps (clamped) at indices 1,2,2 -> mean(0.3,0.9,0.9) = 0.7;
///       luma=0.9; delta = (0.9-0.7)*1.0 = 0.2 -> expected 1.1
#[test]
fn apply_local_contrast_matches_hand_derived_delta() {
    let mut graded = vec![[0.0, 0.0, 0.0], [0.3, 0.3, 0.3], [0.9, 0.9, 0.9]];
    apply_local_contrast(&mut graded, 3, 1, 1, 100.0);
    let expected = [-0.1f32, 0.2, 1.1];
    for (rgb, expected) in graded.iter().zip(expected.iter()) {
        for c in rgb {
            assert!((c - expected).abs() < 1e-4, "expected {expected}, got {rgb:?}");
        }
    }
}

/// Same setup as above at amount=50 -- the delta must scale linearly
/// with amount (factor = amount/100), confirming `amount` isn't just a
/// binary on/off switch.
#[test]
fn apply_local_contrast_amount_scales_delta_linearly() {
    let mut graded = vec![[0.0, 0.0, 0.0], [0.3, 0.3, 0.3], [0.9, 0.9, 0.9]];
    apply_local_contrast(&mut graded, 3, 1, 1, 50.0);
    let expected = [-0.05f32, 0.25, 1.0];
    for (rgb, expected) in graded.iter().zip(expected.iter()) {
        for c in rgb {
            assert!((c - expected).abs() < 1e-4, "expected {expected}, got {rgb:?}");
        }
    }
}

/// End-to-end through `apply_edit_stack` (not just the direct
/// `apply_local_contrast` unit above): confirms the `op_value` wiring
/// itself, and that a nonzero Texture/Clarity plus Dehaze all
/// coexisting at amount=0 for the other two still leaves the image
/// untouched -- the same "everything off is an exact passthrough"
/// contract every other op in this stack already guarantees.
#[test]
fn texture_and_clarity_amount_zero_is_exact_passthrough_through_edit_stack() {
    let mut image = dehaze_test_image(20, 20, [176, 171, 166], [242, 242, 242]);
    apply_edit_stack(&mut image, &stack_with(&[("texture", 0.0), ("clarity", 0.0)]));
    assert_eq!(*image.get_pixel(19, 19), image::Rgb([176, 171, 166]));
    assert_eq!(*image.get_pixel(0, 0), image::Rgb([242, 242, 242]));
}

/// RFC-0010 §3's constant-input identity, hand-derived: on a flat field
/// `var_p = corr_p - mean_p^2 = 0` everywhere (a box mean of a constant is
/// that same constant, exactly), so `a = 0/(0+eps) = 0` and
/// `b = mean_p - 0*mean_p = mean_p = p`; then `mean_a = 0`, `mean_b = p`,
/// giving `q = 0*p + p = p` exactly. A flat image must come back
/// unchanged regardless of radius or `eps`.
#[test]
fn guided_filter_self_on_constant_input_is_exact_identity() {
    let buf = vec![0.42f32; 25];
    let result = guided_filter_self(&buf, 5, 5, 2, CLARITY_GUIDED_EPS);
    for v in result {
        assert!((v - 0.42).abs() < 1e-6, "{v}");
    }
}

/// RFC-0010 §3's other named limit, corrected during implementation (an
/// earlier draft of both the RFC and this test wrongly claimed `eps ->
/// infinity` reduces to `separable_mean_filter`'s own output -- it
/// doesn't: `a -> 0` and `b -> mean_p` pointwise, but `mean_b` then
/// box-filters `b` AGAIN, so the true limit is a DOUBLE box mean
/// (`boxfilter(boxfilter(p))`), not a single one; the failing version of
/// this test caught the bug). The real, useful limit is the opposite
/// direction: as `eps -> 0` (with `var_p > 0` at every pixel, i.e. no
/// perfectly flat local window anywhere -- true here since every value in
/// `buf` is distinct, so even a clamped boundary window is never all one
/// value), `a -> 1` and `b -> 0` pointwise, hence `mean_a -> 1` and
/// `mean_b -> 0`, giving `q -> 1*p + 0 = p` -- an exact identity, no
/// smoothing at all, the filter backing off completely once it has no
/// regularization telling it a region is "flat."
#[test]
fn guided_filter_self_at_a_very_small_eps_is_an_exact_identity() {
    let buf = [5.0f32, 3.0, 8.0, 1.0, 9.0, 2.0, 7.0];
    let guided = guided_filter_self(&buf, 7, 1, 2, 1e-8);
    for (g, p) in guided.iter().zip(buf.iter()) {
        assert!((g - p).abs() < 1e-3, "guided {g}, original {p}");
    }
}

/// The actual halo-reduction claim this RFC exists for, checked directly
/// at the `guided_filter_self` vs. `separable_mean_filter` level rather
/// than only through the higher-level `apply_clarity`/`apply_local_contrast`
/// (isolating the primitive itself, at Clarity's own working radius/eps):
/// a sharp step edge (a hard cut from 0.2 to 0.8, like a dark silhouette
/// against a bright sky), radius 24 so the window at a point just past the
/// edge still reaches deep into the low plateau -- exactly the setup that
/// produces Clarity's classic halo under a plain box mean. At every point
/// checked just inside the high plateau, the guided filter's blurred value
/// must land closer to the true, unblurred plateau value (0.8) than the
/// plain box mean's does.
#[test]
fn guided_filter_self_overshoots_less_than_the_box_mean_near_a_step_edge() {
    let width = 61;
    let mut buf = vec![0.2f32; width];
    for v in buf.iter_mut().skip(30) {
        *v = 0.8;
    }
    let guided = guided_filter_self(&buf, width, 1, 24, CLARITY_GUIDED_EPS);
    let boxed = separable_mean_filter(&buf, width, 1, 24);
    for x in 31..40 {
        let guided_err = (guided[x] - 0.8f32).abs();
        let boxed_err = (boxed[x] - 0.8f32).abs();
        assert!(
            guided_err < boxed_err,
            "at x={x}: guided error {guided_err} not smaller than box-mean error {boxed_err}"
        );
    }
}

/// RFC-0011's cross-check between the general two-signal `guided_filter`
/// and the self-guided specialization: when `guide == p` (the same signal
/// passed for both), the general algorithm's `corr_guide`/`corr_guide_p`
/// collapse to the identical quantity `guided_filter_self` already calls
/// `corr_p`, and `mean_guide`/`mean_p` do too -- so the two functions must
/// agree numerically on the same input, even though they're built from a
/// different number of `separable_mean_filter` calls (six vs. four) to get
/// there. A real formula mismatch in either implementation would show up
/// here as a divergence.
#[test]
fn guided_filter_matches_guided_filter_self_when_guide_equals_p() {
    let buf = [5.0f32, 3.0, 8.0, 1.0, 9.0, 2.0, 7.0];
    let general = guided_filter(&buf, &buf, 7, 1, 2, CLARITY_GUIDED_EPS);
    let specialized = guided_filter_self(&buf, 7, 1, 2, CLARITY_GUIDED_EPS);
    for (g, s) in general.iter().zip(specialized.iter()) {
        assert!((g - s).abs() < 1e-4, "general {g}, specialized {s}");
    }
}

/// RFC-0011 §6: when `p` is constant, `guided_filter` must return that same
/// constant everywhere regardless of what `guide` looks like -- hand-
/// derived: `mean_p = p0` everywhere (box mean of a constant), so
/// `corr_guide_p = boxfilter(guide * p0) = p0 * mean_guide` exactly
/// (linearity), making `cov_guide_p = corr_guide_p - mean_guide*mean_p =
/// p0*mean_guide - mean_guide*p0 = 0` EXACTLY, not just numerically small
/// -- so `a = 0` everywhere no matter what `var_guide` is, `b = mean_p =
/// p0`, `mean_a = 0`, `mean_b = p0`, giving `q = 0*guide + p0 = p0`. This
/// is Dehaze's own real scenario whenever the dark channel is uniform (the
/// existing `dehaze_amount_100_matches_hand_derived_recovery` test relies
/// on exactly this holding, via a non-uniform guide/uniform-transmission
/// fixture -- confirmed still passing after this slice, not just assumed).
#[test]
fn guided_filter_is_exact_identity_when_p_is_constant_regardless_of_guide() {
    let guide = [0.1f32, 0.9, 0.2, 0.8, 0.05, 0.95, 0.3];
    let p = vec![0.62f32; 7];
    let result = guided_filter(&guide, &p, 7, 1, 2, DEHAZE_GUIDED_EPS);
    for v in result {
        assert!((v - 0.62).abs() < 1e-5, "{v}");
    }
}

/// RFC-0011 §6: a real scene edge and a correlated transmission edge at
/// the same location (the actual Dehaze scenario a real depth
/// discontinuity produces) -- `guided_filter` must recover the true
/// transmission plateau better (closer, less overshoot) than a plain box
/// mean of `p` alone at Dehaze's own refinement radius, the same halo-
/// reduction claim RFC-0010 proved for Clarity's self-guided case, now
/// checked for the two-signal one.
#[test]
fn guided_filter_follows_a_correlated_scene_edge_better_than_the_box_mean() {
    let width = 61;
    let mut guide = vec![0.2f32; width];
    let mut p = vec![0.5f32; width];
    for i in 30..width {
        guide[i] = 0.8;
        p[i] = 0.9;
    }
    let guided = guided_filter(&guide, &p, width, 1, DEHAZE_REFINE_RADIUS, DEHAZE_GUIDED_EPS);
    let boxed = separable_mean_filter(&p, width, 1, DEHAZE_REFINE_RADIUS);
    // Only x in [edge - radius + 1, edge + radius - 1] = [27, 33] actually has
    // a box-mean window straddling the edge at all (radius=4); past that,
    // the box mean's own window sits entirely inside one plateau and is
    // already exact, leaving nothing for the guided filter to improve on --
    // checking there would test floating-point noise, not this claim.
    for x in 31..34 {
        let guided_err = (guided[x] - 0.9f32).abs();
        let boxed_err = (boxed[x] - 0.9f32).abs();
        assert!(
            guided_err < boxed_err,
            "at x={x}: guided error {guided_err} not smaller than box-mean error {boxed_err}"
        );
    }
}

/// RFC-0011 §6's reverse case: a real edge in `p` (the transmission map)
/// with NO corresponding edge in `guide` (the scene) at all -- the filter
/// has no depth signal to justify preserving it, and shouldn't do better
/// than a plain box mean at recovering it. Hand-derived: a perfectly
/// constant `guide` makes `var_guide = 0` everywhere, so (same exact-zero
/// reasoning as the constant-`p` test above, mirrored) `cov_guide_p = 0`
/// exactly too, giving `a = 0` everywhere -- but this time `b = mean_p`
/// (not constant, since `p` itself isn't), so `mean_b =
/// boxfilter(boxfilter(p))`: a DOUBLE box mean, not a single one. This is
/// RFC-0011's own restatement of the exact mistake RFC-0010's `eps ->
/// infinity` claim made (assuming a degenerate case reduces to a single
/// box-filter pass when it actually chains two) -- caught here by deriving
/// it properly up front rather than asserting the wrong thing and letting
/// a failing test catch it again. The double-filtered result is smoother,
/// hence FARTHER from the true plateau near the edge than a single box
/// mean -- i.e. strictly worse, not merely "no better."
#[test]
fn guided_filter_does_not_preserve_a_p_edge_the_guide_has_no_signal_for() {
    let width = 61;
    let guide = vec![0.5f32; width];
    let mut p = vec![0.5f32; width];
    for v in p.iter_mut().skip(30) {
        *v = 0.9;
    }
    let guided = guided_filter(&guide, &p, width, 1, DEHAZE_REFINE_RADIUS, DEHAZE_GUIDED_EPS);
    let boxed = separable_mean_filter(&p, width, 1, DEHAZE_REFINE_RADIUS);
    for x in 31..35 {
        let guided_err = (guided[x] - 0.9f32).abs();
        let boxed_err = (boxed[x] - 0.9f32).abs();
        assert!(
            guided_err > boxed_err,
            "at x={x}: guided error {guided_err} not larger than box-mean error {boxed_err} (expected the double-filtered result to be smoother, not sharper)"
        );
    }
}

/// Same passthrough contract as `apply_local_contrast_amount_zero_is_exact_passthrough`,
/// for the new Clarity-only `apply_clarity`.
#[test]
fn apply_clarity_amount_zero_is_exact_passthrough() {
    let mut graded = vec![[0.0, 0.0, 0.0], [0.3, 0.3, 0.3], [0.9, 0.9, 0.9]];
    let before = graded.clone();
    apply_clarity(&mut graded, 3, 1, 0.0);
    assert_eq!(graded, before);
}

/// Same uniform-field contract as `apply_local_contrast_uniform_field_is_unaffected_by_any_amount`
/// -- relies on `guided_filter_self`'s own constant-input identity (tested
/// directly above) rather than re-deriving it, so a 3x3 image against
/// `CLARITY_RADIUS=24` (radius far larger than the image) exercises the
/// identity under heavy edge-clamping too.
#[test]
fn apply_clarity_uniform_field_is_unaffected_by_any_amount() {
    let mut graded = vec![[0.47, 0.31, 0.16]; 9];
    apply_clarity(&mut graded, 3, 3, 100.0);
    apply_clarity(&mut graded, 3, 3, -100.0);
    for rgb in &graded {
        assert!((rgb[0] - 0.47).abs() < 1e-4, "{rgb:?}");
        assert!((rgb[1] - 0.31).abs() < 1e-4, "{rgb:?}");
        assert!((rgb[2] - 0.16).abs() < 1e-4, "{rgb:?}");
    }
}

/// End-to-end confirmation, at the `apply_clarity`/`apply_local_contrast`
/// level (not just the lower-level `guided_filter_self` test above), that
/// switching Clarity to the guided filter is a real, visible improvement:
/// same step-edge image as the primitive-level test, same expectation,
/// but exercised through the actual op functions `apply_edit_stack` calls.
#[test]
fn apply_clarity_creates_less_overshoot_than_the_old_box_mean_version_at_a_real_edge() {
    let width = 61;
    let mut low_graded = vec![[0.2f32, 0.2, 0.2]; width];
    for rgb in low_graded.iter_mut().skip(30) {
        *rgb = [0.8, 0.8, 0.8];
    }
    let mut guided = low_graded.clone();
    let mut boxed = low_graded.clone();
    apply_clarity(&mut guided, width, 1, 100.0);
    apply_local_contrast(&mut boxed, width, 1, CLARITY_RADIUS, 100.0);
    for x in 31..40 {
        let guided_err = (guided[x][0] - 0.8f32).abs();
        let boxed_err = (boxed[x][0] - 0.8f32).abs();
        assert!(
            guided_err < boxed_err,
            "at x={x}: guided error {guided_err} not smaller than box-mean error {boxed_err}"
        );
    }
}

/// Hand-derived: a 3x3 luma buffer with a single bright spot at the
/// center-right neighbor. At (1,1): xm=0,xp=2 -> gx = luma[1,2] -
/// luma[1,0] = 1.0 - 0.0 = 1.0; ym=0,yp=2 -> gy = luma[2,1] -
/// luma[0,1] = 0.0 - 0.0 = 0.0. magnitude = sqrt(1.0^2 + 0.0^2) * 0.5
/// = 0.5 exactly.
#[test]
fn local_gradient_magnitude_matches_hand_computed_value() {
    #[rustfmt::skip]
    let buf = [
        0.0, 0.0, 0.0,
        0.0, 0.5, 1.0,
        0.0, 0.0, 0.0,
    ];
    assert!((local_gradient_magnitude(&buf, 3, 3, 1, 1) - 0.5).abs() < 1e-6);
}

/// A flat buffer has zero gradient everywhere, including at the
/// border (where clamping collapses `xm`/`xp` or `ym`/`yp` to the
/// same index) -- confirms the clamping doesn't introduce a phantom
/// gradient at the edges.
#[test]
fn local_gradient_magnitude_is_zero_on_a_flat_field_including_borders() {
    let buf = [0.4f32; 9];
    assert_eq!(local_gradient_magnitude(&buf, 3, 3, 0, 0), 0.0);
    assert_eq!(local_gradient_magnitude(&buf, 3, 3, 1, 1), 0.0);
    assert_eq!(local_gradient_magnitude(&buf, 3, 3, 2, 2), 0.0);
}

#[test]
fn sharpen_delta_amount_zero_is_an_exact_passthrough() {
    let s = Sharpen { amount: 0.0, radius: 80.0, detail: 100.0, masking: 0.0 };
    assert_eq!(sharpen_delta(0.7, 0.3, 2.0, &s), 0.0);
}

/// Hand-derived: at detail=100/masking=0, both thresholds collapse to
/// f32::EPSILON, so for any diff/grad_mag well above that floor both
/// gates saturate to ~1.0 -- the delta reduces to essentially
/// `diff * (amount/100) * SHARPEN_STRENGTH` with no meaningful
/// gating, a clean near-exact value to check against.
#[test]
fn sharpen_delta_matches_hand_derived_value_at_full_detail_and_no_masking() {
    let s = Sharpen { amount: 100.0, radius: 50.0, detail: 100.0, masking: 0.0 };
    let delta = sharpen_delta(0.6, 0.5, 1.0, &s);
    let expected = 0.1 * 1.0 * SHARPEN_STRENGTH;
    assert!((delta - expected).abs() < 1e-4, "expected ~{expected}, got {delta}");
}

/// `sharpen_radius_px` boundary mapping: slider=0 -> the minimum
/// radius (1px, since a 0px radius is meaningless), slider=100 -> the
/// fixed maximum.
#[test]
fn sharpen_radius_px_maps_slider_bounds_correctly() {
    assert_eq!(sharpen_radius_px(0.0), 1);
    assert_eq!(sharpen_radius_px(100.0), SHARPEN_MAX_RADIUS_PX);
}

#[test]
fn luma_nr_delta_amount_zero_is_an_exact_passthrough_even_at_full_contrast() {
    let n = LumaNr { amount: 0.0, detail: 50.0, contrast: 100.0 };
    assert_eq!(luma_nr_delta(0.6, 0.4, &n), 0.0);
}

/// Hand-derived: detail=0 -> edge_threshold = NR_DETAIL_SCALE exactly
/// (0.05, no epsilon floor needed since it's already positive).
/// Choosing diff = edge_threshold/2 = 0.025 gives smoothstep's own
/// input t = 0.5 exactly, and smoothstep(0.5) = 0.5*0.5*(3-1.0) = 0.5
/// exactly (the `t*t*(3-2t)` formula's own well-known value at its
/// midpoint) -- so smooth_weight = 1 - 0.5 = 0.5, and at amount=100/
/// contrast=0: smooth_delta = -0.025 * 1.0 * 0.5 = -0.0125 exactly.
#[test]
fn luma_nr_delta_matches_hand_derived_value_at_the_smoothstep_midpoint() {
    let n = LumaNr { amount: 100.0, detail: 0.0, contrast: 0.0 };
    let l = 0.525;
    let blurred = 0.5; // diff = 0.025
    let delta = luma_nr_delta(l, blurred, &n);
    assert!((delta - (-0.0125)).abs() < 1e-5, "expected ~-0.0125, got {delta}");
}

/// Contrast restoration is scaled by `amount` too -- confirms it
/// can't fire when amount=0 even with contrast=100 (already covered
/// above), and separately confirms it DOES contribute a real,
/// independent term when amount>0: at contrast=100 the restoration
/// term exactly cancels part of the smoothing term (both proportional
/// to the same `diff`), which is itself a meaningful, checkable
/// property -- the combined delta must have a SMALLER magnitude than
/// the smoothing-only delta (contrast=0) at the same amount.
#[test]
fn luma_nr_contrast_restoration_reduces_the_net_smoothing_effect() {
    let no_restore = LumaNr { amount: 100.0, detail: 0.0, contrast: 0.0 };
    let with_restore = LumaNr { amount: 100.0, detail: 0.0, contrast: 100.0 };
    let l = 0.525;
    let blurred = 0.5;
    let d1 = luma_nr_delta(l, blurred, &no_restore).abs();
    let d2 = luma_nr_delta(l, blurred, &with_restore).abs();
    assert!(d2 < d1, "expected contrast restoration to shrink the net delta: {d2} vs {d1}");
}

/// RFC-0012: Luma NR's blur source is now `guided_filter_self` at
/// `LUMA_NR_RADIUS`/`LUMA_NR_GUIDED_EPS`, not `separable_mean_filter`. In a
/// genuinely flat-but-noisy region (small per-pixel variance, no real
/// edge), the guided filter should behave close to the old box-mean
/// baseline -- `LUMA_NR_GUIDED_EPS` was specifically chosen so `a` stays
/// near 0 at noise-scale variance, matching this op's own "denoise, don't
/// enhance" purpose (unlike Clarity's much larger eps).
#[test]
fn luma_nr_guided_blur_matches_the_box_mean_closely_in_a_flat_noisy_region() {
    let width = 20;
    let noisy: Vec<f32> = (0..width * 3)
        .map(|i| 0.5 + if i % 2 == 0 { 0.01 } else { -0.01 })
        .collect();
    let guided = guided_filter_self(&noisy, width, 3, LUMA_NR_RADIUS, LUMA_NR_GUIDED_EPS);
    let boxed = separable_mean_filter(&noisy, width, 3, LUMA_NR_RADIUS);
    for i in 0..noisy.len() {
        // Not IDENTICAL to the box mean -- that only happens in the true
        // eps -> infinity limit (RFC-0010's own corrected identity), which
        // isn't this op's working point. 0.005 is comfortably above the
        // actual observed divergence at LUMA_NR_GUIDED_EPS for this
        // +-0.01 noise amplitude (~0.0028) while still being a real,
        // checkable "close to" bound, not a rubber-stamp tolerance.
        assert!(
            (guided[i] - boxed[i]).abs() < 0.005,
            "at {i}: guided={} box={} diverge more than expected in a flat noisy region",
            guided[i],
            boxed[i]
        );
    }
}

/// Same step-edge shape as `guided_filter_self_overshoots_less_than_the_box_mean_near_a_step_edge`
/// and `apply_clarity_creates_less_overshoot_than_the_old_box_mean_version_at_a_real_edge`,
/// but at Luma NR's own (much smaller) radius/eps -- confirms the
/// edge-awareness win isn't specific to Clarity's large radius/eps working
/// point. At radius=3, only x in [4,6) actually straddles the edge at x=5
/// for an 8-radius window... re-derived directly: window [x-3,x+3], edge
/// at x=5, so x in [3,7] all have a window overlapping both sides; check
/// the ones closest to the edge, where a box mean is most wrong.
#[test]
fn luma_nr_guided_blur_overshoots_less_than_the_box_mean_near_a_step_edge() {
    let width = 20;
    let mut buf = vec![0.2f32; width];
    for v in buf.iter_mut().skip(5) {
        *v = 0.8;
    }
    let guided = guided_filter_self(&buf, width, 1, LUMA_NR_RADIUS, LUMA_NR_GUIDED_EPS);
    let boxed = separable_mean_filter(&buf, width, 1, LUMA_NR_RADIUS);
    for x in 4..7 {
        let guided_err = (guided[x] - buf[x]).abs();
        let boxed_err = (boxed[x] - buf[x]).abs();
        assert!(
            guided_err <= boxed_err,
            "at x={x}: guided error {guided_err} not <= box-mean error {boxed_err}"
        );
    }
}

/// End-to-end sanity through `apply_edit_stack` -- Luma NR at a real
/// amount still smooths a flat, noisy region (the op's whole purpose is
/// unaffected by the blur-source swap) and amount=0 is still an exact
/// passthrough (already covered generally by
/// `sharpen_and_nr_absent_ops_are_exact_passthrough_through_edit_stack`,
/// re-checked here specifically for a present-but-zero-amount luma_nr op).
#[test]
fn luma_nr_end_to_end_through_edit_stack_still_smooths_a_noisy_flat_region() {
    let width = 20;
    let height = 3;
    let mut image = image::ImageBuffer::from_fn(width, height, |x, _y| {
        let v = if x % 2 == 0 { 130u8 } else { 126u8 };
        image::Rgb([v, v, v])
    });
    let before = *image.get_pixel(10, 1);
    apply_edit_stack(
        &mut image,
        &EditStack {
            schema_version: 1,
            ops: vec![serde_json::json!({ "op": "luma_nr", "amount": 100.0, "detail": 50.0, "contrast": 0.0 })],
        },
    );
    let after = *image.get_pixel(10, 1);
    assert_ne!(after, before, "a real Luma NR amount should visibly smooth alternating noise");
}

#[test]
fn color_nr_delta_amount_zero_is_an_exact_passthrough() {
    let n = ColorNr { amount: 0.0, detail: 0.0 };
    let delta = color_nr_delta([0.5, 0.3, 0.7], [0.4, 0.4, 0.6], &n);
    assert_eq!(delta, [0.0, 0.0, 0.0]);
}

/// The property this slice's own design review verified algebraically
/// (chroma_delta's weighted sum, using luma3's own Rec.709 weights,
/// telescopes to exactly zero by construction): reconstructing
/// `orig + color_nr_delta(orig, blurred, n)` must leave `luma3`
/// UNCHANGED, regardless of the actual (nonzero) scalar `k` the
/// gating produces. Uses a near-gray pixel with a small perturbation
/// so `color_smooth_weight` isn't gated all the way to zero (a
/// trivial, uninformative pass if k happened to be exactly 0).
#[test]
fn color_nr_delta_preserves_luminance_exactly() {
    let n = ColorNr { amount: 100.0, detail: 0.0 };
    let orig = [0.5f32, 0.5, 0.5];
    let blurred = [0.51f32, 0.49, 0.505];
    let delta = color_nr_delta(orig, blurred, &n);
    // Sanity: k must actually be nonzero for this to be a meaningful
    // check, not a trivial all-zero-delta pass.
    assert!(delta.iter().any(|d| d.abs() > 1e-6), "delta was trivially zero: {delta:?}");
    let new_rgb = [orig[0] + delta[0], orig[1] + delta[1], orig[2] + delta[2]];
    let orig_luma = luma3(orig);
    let new_luma = luma3(new_rgb);
    assert!(
        (new_luma - orig_luma).abs() < 1e-5,
        "expected luma to stay at {orig_luma}, got {new_luma} (delta {delta:?})"
    );
}

/// Same invariant, checked again at a DIFFERENT amount/detail (a
/// different, nonzero `k`) and a different orig/blurred pair, so the
/// exact-cancellation property isn't only verified at one coincidental
/// parameter combination.
#[test]
fn color_nr_delta_preserves_luminance_exactly_at_a_different_k() {
    let n = ColorNr { amount: 60.0, detail: 20.0 };
    let orig = [0.2f32, 0.6, 0.35];
    let blurred = [0.22f32, 0.58, 0.34];
    let delta = color_nr_delta(orig, blurred, &n);
    let new_rgb = [orig[0] + delta[0], orig[1] + delta[1], orig[2] + delta[2]];
    assert!((luma3(new_rgb) - luma3(orig)).abs() < 1e-5);
}

/// End-to-end through `apply_edit_stack`, all three ops at amount=0
/// (the default when absent): a real image is left byte-for-byte
/// unchanged, the same contract every op in this stack already
/// guarantees.
#[test]
fn sharpen_and_nr_absent_ops_are_exact_passthrough_through_edit_stack() {
    let mut image = RgbImage::from_pixel(24, 24, image::Rgb([140, 90, 60]));
    apply_edit_stack(&mut image, &stack_with(&[]));
    assert_eq!(*image.get_pixel(12, 12), image::Rgb([140, 90, 60]));
}

/// End-to-end: strong Sharpening on a real image with an actual edge
/// (a small bright patch in a uniform scene, same `dehaze_test_image`
/// helper used for Dehaze's own end-to-end tests) visibly boosts
/// contrast at the boundary -- the pixel immediately outside the
/// patch (x=3, adjacent to the patch's own x=2 edge column, so BOTH
/// the local gradient Masking reads and the wider blur Detail reads
/// see a real edge there) should get darker (overshoot below the
/// scene value) since unsharp masking always produces a halo at a
/// real edge.
#[test]
fn sharpen_creates_a_visible_halo_at_a_real_edge_through_edit_stack() {
    let mut image = dehaze_test_image(30, 30, [150, 150, 150], [220, 220, 220]);
    let before = *image.get_pixel(3, 1); // immediately outside the 3x3 bright patch, still scene-colored
    apply_edit_stack(
        &mut image,
        &EditStack {
            schema_version: 1,
            ops: vec![serde_json::json!({ "op": "sharpen", "amount": 100.0, "radius": 20.0, "detail": 100.0, "masking": 0.0 })],
        },
    );
    let after = image.get_pixel(3, 1);
    assert!(
        after[0] < before[0],
        "expected a darker halo pixel near the edge, before={before:?} after={after:?}"
    );
}
