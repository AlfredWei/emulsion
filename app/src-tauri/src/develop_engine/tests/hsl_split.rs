use super::support::*;

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
/// every other band (Orange is the nearest neighbor, 45 degrees away)
/// weight exactly 0. Only Red's own hue=+50/saturation=+50/
/// luminance=+30 should have any effect. Expected value computed via
/// script running the exact same formula as apply_hsl_bands (not
/// eyeballed): (246, 219, 82).
#[test]
fn hsl_single_band_at_its_own_center_isolates_that_band() {
    assert_pixel_with_hsl([200, 50, 50], &[("red", 50.0, 50.0, 30.0)], [246, 219, 82]);
}

/// (179,115,77)'s hue (script-verified) is ~22.35 degrees -- inside
/// the Red/Orange boundary region, roughly equidistant from both
/// centers (weight ~0.505 Red / ~0.495 Orange). Red's luminance is set
/// to +40, Orange's to -40 -- opposite signs specifically so a hard
/// nearest-band-only cutoff (a real bug this test would catch) would
/// produce a LARGE shift in one direction, while the correct smooth
/// blend nearly cancels the two (net lum_acc ~= 0.4, not +/-40).
/// Expected value computed via script running the exact same formula:
/// (179, 116, 78) -- a tiny shift, not the large one a hard-cutoff bug
/// would produce.
#[test]
fn hsl_boundary_hue_blends_both_neighboring_bands() {
    assert_pixel_with_hsl(
        [179, 115, 77],
        &[("red", 0.0, 0.0, 40.0), ("orange", 0.0, 0.0, -40.0)],
        [179, 116, 78],
    );
}

/// (133,128,122) is a near-gray pixel (script-verified saturation
/// ~0.043, well under the chroma_fade denominator of 0.08) -- proves
/// chroma_fade meaningfully suppresses the hue/luminance shift instead
/// of applying it at full strength. Both Red and Orange set to an
/// identical (80,80,80) so the blend-vs-cutoff distinction (covered by
/// the previous test) doesn't confound this one -- this test is
/// specifically about the fade, not the blend. Expected value computed
/// via script running the exact same formula: (185, 188, 177).
#[test]
fn hsl_near_gray_pixel_shift_is_suppressed_by_chroma_fade() {
    assert_pixel_with_hsl(
        [133, 128, 122],
        &[("red", 80.0, 80.0, 80.0), ("orange", 80.0, 80.0, 80.0)],
        [185, 188, 177],
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
