use super::support::*;
use crate::develop_engine::{hsl_band_weight, HSL_BAND_CENTERS_DEG, HSL_BAND_NAMES};

/// All-zero (absent) bands must be an exact passthrough -- confirmed
/// algebraically in this module's own `apply_hsl_bands` doc comment
/// (hue_acc=sat_acc=lum_acc=0 collapses the whole formula to a
/// round-trip through rgb_to_hsl/hsl_to_rgb of the same value).
#[test]
fn hsl_bands_absent_is_a_passthrough() {
    assert_pixel_with_hsl([200, 100, 50], &[], [200, 100, 50]);
}

/// (200,50,50) has hue exactly 0 degrees (script-verified: max=r,
/// g==b so the numerator (g-b) is 0) -- landing exactly on the Red
/// band's own center, where hsl_band_weight gives Red weight 1.0 and
/// every other band (Orange is the nearest neighbor, 30 degrees away)
/// weight exactly 0. Only Red's own hue=+50/saturation=+50/
/// luminance=+30 should have any effect. Expected value computed via
/// script running the exact same formula as apply_hsl_bands (not
/// eyeballed): (246, 219, 82).
#[test]
fn hsl_single_band_at_its_own_center_isolates_that_band() {
    assert_pixel_with_hsl([200, 50, 50], &[("red", 50.0, 50.0, 30.0)], [246, 219, 82]);
}

/// RFC-0019: (200,88,50) has hue ~15.2 degrees (script-verified) --
/// almost exactly the midpoint of the Red(0)/Orange(30) interval, so
/// each band's weight is ~0.5. Red's luminance is set to +40, Orange's
/// to -40 -- opposite signs specifically so a hard nearest-band-only
/// cutoff (a real bug this test would catch) would produce a LARGE shift
/// in one direction, while the correct smooth blend nearly cancels the
/// two. Expected value computed via a script running the exact same
/// formula: (198, 87, 50) -- a tiny shift, not the large one a
/// hard-cutoff bug would produce. (Was the (179,115,77) -> (179,116,78)
/// case under the old 45-degree centers, whose Red/Orange midpoint was
/// ~22.5 degrees; that pixel is now closer to Orange, so this test moved
/// to the new midpoint rather than keeping a stale hue.)
#[test]
fn hsl_boundary_hue_blends_both_neighboring_bands() {
    assert_pixel_with_hsl(
        [200, 88, 50],
        &[("red", 0.0, 0.0, 40.0), ("orange", 0.0, 0.0, -40.0)],
        [198, 87, 50],
    );
}

/// RFC-0019, the unequal-gap case: (125,200,50) has hue exactly 90
/// degrees -- the midpoint of the Yellow(60)/Green(120) interval, a 60
/// degree gap (the old even spacing only ever had 45-degree gaps). With
/// Yellow at +40 luminance and Green at -40 the two weights (0.5 each)
/// cancel and the pixel comes back unchanged; a hard cutoff, or a weight
/// function that wrongly used a fixed 45-degree half-width, would not.
#[test]
fn hsl_unequal_gap_midpoint_blends_yellow_and_green_equally() {
    assert_pixel_with_hsl(
        [125, 200, 50],
        &[("yellow", 0.0, 0.0, 40.0), ("green", 0.0, 0.0, -40.0)],
        [125, 200, 50],
    );
}

/// RFC-0019's user-visible regression: a pure yellow (hue exactly 60,
/// (200,200,50)) must respond ONLY to the Yellow band. Under the old 90
/// degree Yellow center it responded 75% to Orange, so an Orange
/// saturation of -100 here would have visibly desaturated it. Expected
/// (238, 238, 12) is the Yellow +50 saturation result alone, computed
/// via a script running the exact same formula.
#[test]
fn hsl_pure_yellow_is_owned_by_the_yellow_band_not_orange() {
    assert_pixel_with_hsl(
        [200, 200, 50],
        &[("yellow", 0.0, 50.0, 0.0), ("orange", 0.0, -100.0, 0.0)],
        [238, 238, 12],
    );
}

/// Each band's weight is exactly 1 at its own center and 0 at every other
/// band's center -- replaces the old tests' implicit 45-degree assumption
/// with the property RFC-0019 actually guarantees.
#[test]
fn hsl_band_weight_is_one_at_own_center_and_zero_at_every_other_center() {
    for i in 0..8 {
        for j in 0..8 {
            let w = hsl_band_weight(HSL_BAND_CENTERS_DEG[j], i);
            let expected = if i == j { 1.0 } else { 0.0 };
            assert!((w - expected).abs() < 1e-5, "band {i} at center of band {j}: {w}");
        }
    }
}

/// Partition of unity across the whole hue circle, with at most two bands
/// nonzero at any hue -- the property that lets `apply_hsl_bands` combine
/// deltas without any renormalization, even with unequal gaps.
#[test]
fn hsl_band_weights_partition_unity_over_the_whole_hue_circle() {
    let mut hue = 0.0f32;
    while hue < 360.0 {
        let weights: Vec<f32> = (0..8).map(|i| hsl_band_weight(hue, i)).collect();
        let sum: f32 = weights.iter().sum();
        let nonzero = weights.iter().filter(|w| **w > 1e-6).count();
        assert!((sum - 1.0).abs() < 1e-5, "hue {hue}: sum {sum}");
        assert!(nonzero <= 2, "hue {hue}: {nonzero} nonzero bands");
        hue += 0.25;
    }
}

/// The exact hues named in RFC-0019 SS3.3, plus the unequal-gap midpoint
/// (90) and the 360->0 wraparound (Magenta 300 / Red 0, a 60-degree gap
/// straddling the seam): real yellow/orange/green/blue are fully owned by
/// their own band, and the cyclic-neighbor edge case blends correctly.
#[test]
fn hsl_band_weights_at_the_rfc_worked_example_hues() {
    let idx = |name: &str| HSL_BAND_NAMES.iter().position(|n| *n == name).unwrap();
    for (hue, band) in [(30.0, "orange"), (60.0, "yellow"), (120.0, "green"), (240.0, "blue")] {
        let w = hsl_band_weight(hue, idx(band));
        assert!((w - 1.0).abs() < 1e-5, "hue {hue} band {band}: {w}");
    }
    for (hue, a, b) in [(45.0, "orange", "yellow"), (90.0, "yellow", "green"), (330.0, "magenta", "red")] {
        let (wa, wb) = (hsl_band_weight(hue, idx(a)), hsl_band_weight(hue, idx(b)));
        assert!((wa - 0.5).abs() < 1e-5 && (wb - 0.5).abs() < 1e-5, "hue {hue}: {a}={wa} {b}={wb}");
    }
    // just below the 360->0 seam: still mostly Red, remainder Magenta
    let (w_red, w_mag) = (hsl_band_weight(355.0, idx("red")), hsl_band_weight(355.0, idx("magenta")));
    assert!(w_red > 0.98 && (w_red + w_mag - 1.0).abs() < 1e-5, "red={w_red} magenta={w_mag}");
}

/// (133,128,122) is a near-gray pixel (script-verified saturation
/// ~0.043, well under the chroma_fade denominator of 0.08) -- proves
/// chroma_fade meaningfully suppresses the hue/luminance shift instead
/// of applying it at full strength. Both Red and Orange set to an
/// identical (80,80,80) so the blend-vs-cutoff distinction (covered by
/// the previous test) doesn't confound this one -- this test is
/// specifically about the fade, not the blend. Expected value computed
/// via script running the exact same formula: (184, 187, 176). (Was
/// (185, 188, 177) under the old 45-degree centers: this pixel's hue
/// (~32.7 degrees) is now ~98% Orange and ~2% Yellow, and Yellow carries
/// no delta here, so the total applied weight is slightly under 1.)
#[test]
fn hsl_near_gray_pixel_shift_is_suppressed_by_chroma_fade() {
    assert_pixel_with_hsl(
        [133, 128, 122],
        &[("red", 80.0, 80.0, 80.0), ("orange", 80.0, 80.0, 80.0)],
        [184, 187, 176],
    );
}

/// Both saturations at 0 must be an exact passthrough regardless of
/// hue/balance -- confirmed algebraically in apply_split_toning's own
/// doc comment (a_sh=a_hi=0 whenever both saturations are 0,
/// independent of hue/balance, which only feed the now-zero-weighted
/// tint terms).
#[test]
fn split_toning_zero_saturation_is_a_passthrough() {
    assert_pixel_with_split_toning([100, 150, 200], 200.0, 0.0, 30.0, 0.0, 50.0, [100, 150, 200]);
}

/// Dark gray (30,30,30) has lightness 30/255=0.1176 -- comfortably in
/// the shadow-dominated region even at balance=0 (weight_shadows ~=
/// 0.962). Only the Shadow zone is set (hue=200, saturation=80);
/// Highlight is left at identity. Expected value computed via script
/// running the exact same formula: (7, 38, 53).
#[test]
fn split_toning_pure_shadow_pixel_isolates_the_shadow_zone() {
    assert_pixel_with_split_toning([30, 30, 30], 200.0, 80.0, 0.0, 0.0, 0.0, [7, 38, 53]);
}

/// Bright gray (220,220,220) has lightness 220/255=0.8627 -- comfortably
/// in the highlight-dominated region. Only the Highlight zone is set
/// (hue=30, saturation=70); Shadow is left at identity. Expected value
/// computed via script running the exact same formula: (243, 220, 197).
#[test]
fn split_toning_pure_highlight_pixel_isolates_the_highlight_zone() {
    assert_pixel_with_split_toning([220, 220, 220], 0.0, 0.0, 30.0, 70.0, 0.0, [243, 220, 197]);
}

/// Mid-gray (128,128,128) at balance=0 has weight_shadows~=0.497 and
/// weight_highlights~=0.503 -- both zones meaningfully active at once,
/// with deliberately NON-complementary hues (0=red, 90=chartreuse) and
/// unequal saturations (70 vs 40) so shadow-first-sequential and
/// highlight-first-sequential blending would each produce a
/// DIFFERENT, wrong answer from the correct single-shot combination --
/// script-verified: the correct simultaneous blend gives (172,109,58),
/// while a sequential shadow-then-highlight blend of the exact same
/// inputs gives (163,118,67), a difference well outside this test's
/// own tolerance. This is a real regression test for the exact bug a
/// design review caught before this formula was implemented, not a
/// hypothetical.
#[test]
fn split_toning_blends_both_zones_simultaneously_not_sequentially() {
    assert_pixel_with_split_toning([128, 128, 128], 0.0, 70.0, 90.0, 40.0, 0.0, [172, 109, 58]);
}
