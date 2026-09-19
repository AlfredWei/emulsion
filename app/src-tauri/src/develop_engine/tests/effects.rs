use super::support::*;
use super::*;

/// amount=0 must be an EXACT passthrough regardless of midpoint/
/// feather/uv -- `vignette_factor` returns exactly 1.0 without even
/// touching the geometry, a structural guarantee, not a numerically-
/// near-one one.
#[test]
fn vignette_amount_zero_is_an_exact_passthrough() {
    let v = Vignette { amount: 0.0, midpoint: 10.0, feather: 90.0 };
    assert_eq!(vignette_factor((0.0, 0.0), 0.6667, &v), 1.0);
    assert_eq!(vignette_factor((0.5, 0.5), 0.6667, &v), 1.0);
}

/// The dead-center pixel (normDist=0) sits below ANY positive midpoint
/// -- smoothstep(inner, outer, 0) is exactly 0 whenever inner > 0, so
/// the center is fully unaffected regardless of how strong `amount`
/// is, matching a real vignette's own "corners only" shape.
#[test]
fn vignette_center_pixel_is_unaffected_when_midpoint_is_positive() {
    let v = Vignette { amount: -100.0, midpoint: 50.0, feather: 50.0 };
    assert_eq!(vignette_factor((0.5, 0.5), 0.6667, &v), 1.0);
}

/// Hand-derived corner case: midpoint=0, feather=100 puts `inner=0`,
/// `outer=1.0` exactly, and a corner pixel's normDist is EXACTLY 1.0
/// by `corner_dist`'s own definition (the corner IS the normalizing
/// distance) -- smoothstep(0,1,1)=1.0 exactly, so the factor reduces
/// to `1 + amount/100` with no partial falloff to account for.
#[test]
fn vignette_corner_pixel_at_full_feather_matches_hand_derived_factor() {
    let darken = Vignette { amount: -100.0, midpoint: 0.0, feather: 100.0 };
    let lighten = Vignette { amount: 60.0, midpoint: 0.0, feather: 100.0 };
    let aspect = 0.6667;
    assert!((vignette_factor((0.0, 0.0), aspect, &darken) - 0.0).abs() < 1e-4);
    assert!((vignette_factor((1.0, 1.0), aspect, &darken) - 0.0).abs() < 1e-4);
    assert!((vignette_factor((0.0, 0.0), aspect, &lighten) - 1.6).abs() < 1e-4);
}

/// A negative amount must never brighten and a positive amount must
/// never darken -- monotonicity of the sign, checked at a real
/// intermediate point (not just the corner/center extremes above).
#[test]
fn vignette_sign_of_amount_matches_darken_vs_lighten() {
    let aspect = 0.6667;
    let darken = Vignette { amount: -80.0, midpoint: 20.0, feather: 60.0 };
    let lighten = Vignette { amount: 80.0, midpoint: 20.0, feather: 60.0 };
    let uv = (0.9, 0.9);
    assert!(vignette_factor(uv, aspect, &darken) < 1.0);
    assert!(vignette_factor(uv, aspect, &lighten) > 1.0);
}

/// End-to-end through `apply_edit_stack`: a real image's corner darkens
/// and its dead center stays untouched with a strong negative Amount
/// and midpoint=0 -- the same real-image contract Dehaze's own
/// end-to-end test already establishes, applied to Vignette.
#[test]
fn vignette_darkens_corners_and_leaves_center_untouched_through_edit_stack() {
    let mut image = RgbImage::from_pixel(21, 21, image::Rgb([200, 150, 100]));
    apply_edit_stack(
        &mut image,
        &EditStack {
            schema_version: 1,
            ops: vec![serde_json::json!({ "op": "vignette", "amount": -100.0, "midpoint": 0.0, "feather": 100.0 })],
        },
    );
    assert_eq!(*image.get_pixel(10, 10), image::Rgb([200, 150, 100]));
    let corner = image.get_pixel(0, 0);
    assert!(corner[0] < 50, "expected a near-black corner, got {corner:?}");
}

/// Hand-derived exact value: `sin(0.0) == 0.0` exactly, so
/// `grain_hash(0.0, 0.0)` collapses to `fract(0.0 * 43758.5453123) ==
/// 0.0` exactly -- one real closed-form point on an otherwise
/// opaque-looking hash function.
#[test]
fn grain_hash_at_origin_is_exactly_zero() {
    assert_eq!(grain_hash(0.0, 0.0), 0.0);
}

/// amount=0 must be an EXACT passthrough regardless of coord/size/
/// roughness -- `grain_delta` returns exactly 0.0 without even
/// touching the noise lattice, same structural guarantee every other
/// op's identity value already gets.
#[test]
fn grain_amount_zero_is_an_exact_passthrough() {
    let g = Grain { amount: 0.0, size: 80.0, roughness: 90.0 };
    assert_eq!(grain_delta((123.0, 456.0), &g), 0.0);
    assert_eq!(grain_delta((0.0, 0.0), &g), 0.0);
}

/// The grain pattern must be STATIC -- the same coordinate evaluated
/// twice (e.g. across two separate renders) returns the identical
/// delta, not a freshly-randomized one. `grain_delta` is a pure
/// function of its inputs, but this pins that contract down
/// explicitly rather than leaving it implicit.
#[test]
fn grain_delta_is_deterministic_for_the_same_coordinate() {
    let g = Grain { amount: 60.0, size: 40.0, roughness: 30.0 };
    let a = grain_delta((17.0, 42.0), &g);
    let b = grain_delta((17.0, 42.0), &g);
    assert_eq!(a, b);
}

/// At roughness=100 (pure lattice-cell hash, no bilinear
/// interpolation), two DIFFERENT pixel coordinates that fall in the
/// SAME lattice cell must produce the EXACT SAME delta -- this is
/// `size`'s whole visible effect (grouping pixels into same-value
/// blocks), and it's exactly derivable: size=100 -> cell = 1 +
/// 1.0*(6.0-1.0) = 6.0px, so (0,0) and (3,3) both floor-divide to
/// lattice cell (0,0), while (7,7) floor-divides to a different cell
/// (1,1).
#[test]
fn grain_size_groups_pixels_into_matching_blocky_cells_at_full_roughness() {
    let g = Grain { amount: 100.0, size: 100.0, roughness: 100.0 };
    let a = grain_delta((0.0, 0.0), &g);
    let b = grain_delta((3.0, 3.0), &g);
    let c = grain_delta((7.0, 7.0), &g);
    assert_eq!(a, b, "same 6px lattice cell must match exactly");
    assert_ne!(a, c, "a different lattice cell should (almost certainly) differ");
}

/// Roughness actually changes the result -- blending smooth
/// (bilinear) and rough (nearest-cell) noise at the two extremes must
/// generally disagree at a non-lattice-aligned point, confirming the
/// blend isn't a no-op.
#[test]
fn grain_roughness_zero_and_one_hundred_generally_differ() {
    let smooth = Grain { amount: 100.0, size: 50.0, roughness: 0.0 };
    let rough = Grain { amount: 100.0, size: 50.0, roughness: 100.0 };
    let coord = (11.3, 27.9);
    assert_ne!(grain_delta(coord, &smooth), grain_delta(coord, &rough));
}

/// End-to-end through `apply_edit_stack`: amount=0 (the default when
/// the op is absent entirely) leaves a real image byte-for-byte
/// unchanged, the same "everything off is an exact passthrough"
/// contract every other op in this stack already guarantees.
#[test]
fn grain_absent_op_is_exact_passthrough_through_edit_stack() {
    let mut image = RgbImage::from_pixel(20, 20, image::Rgb([120, 80, 40]));
    apply_edit_stack(&mut image, &stack_with(&[]));
    assert_eq!(*image.get_pixel(5, 5), image::Rgb([120, 80, 40]));
}
