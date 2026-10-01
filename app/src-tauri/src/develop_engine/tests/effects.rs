use super::support::*;
use super::*;

/// amount=0 must be an EXACT passthrough regardless of midpoint/
/// feather/uv -- `vignette_factor` returns exactly 1.0 without even
/// touching the geometry, a structural guarantee, not a numerically-
/// near-one one.
#[test]
fn vignette_amount_zero_is_an_exact_passthrough() {
    let v = Vignette { amount: 0.0, midpoint: 10.0, feather: 90.0, roundness: 0.0 };
    assert_eq!(vignette_factor((0.0, 0.0), 0.6667, &v), 1.0);
    assert_eq!(vignette_factor((0.5, 0.5), 0.6667, &v), 1.0);
}

/// The dead-center pixel (normDist=0) sits below ANY positive midpoint
/// -- smoothstep(inner, outer, 0) is exactly 0 whenever inner > 0, so
/// the center is fully unaffected regardless of how strong `amount`
/// is, matching a real vignette's own "corners only" shape.
#[test]
fn vignette_center_pixel_is_unaffected_when_midpoint_is_positive() {
    let v = Vignette { amount: -100.0, midpoint: 50.0, feather: 50.0, roundness: 0.0 };
    assert_eq!(vignette_factor((0.5, 0.5), 0.6667, &v), 1.0);
}

/// Hand-derived corner case: midpoint=0, feather=100 puts `inner=0`,
/// `outer=1.0` exactly, and a corner pixel's normDist is EXACTLY 1.0
/// by `corner_dist`'s own definition (the corner IS the normalizing
/// distance) -- smoothstep(0,1,1)=1.0 exactly, so the factor reduces
/// to `1 + amount/100` with no partial falloff to account for.
#[test]
fn vignette_corner_pixel_at_full_feather_matches_hand_derived_factor() {
    let darken = Vignette { amount: -100.0, midpoint: 0.0, feather: 100.0, roundness: 0.0 };
    let lighten = Vignette { amount: 60.0, midpoint: 0.0, feather: 100.0, roundness: 0.0 };
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
    let darken = Vignette { amount: -80.0, midpoint: 20.0, feather: 60.0, roundness: 0.0 };
    let lighten = Vignette { amount: 80.0, midpoint: 20.0, feather: 60.0, roundness: 0.0 };
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

// --- RFC-0016: roundness ---

/// `amount=0` must stay an EXACT passthrough regardless of `roundness` --
/// the short-circuit at the very top of `vignette_factor` runs before
/// any of the three roundness branches, so a nonzero/extreme roundness
/// must never be able to re-enable geometry that `amount=0` disabled.
#[test]
fn vignette_amount_zero_is_an_exact_passthrough_regardless_of_roundness() {
    for roundness in [-100.0, -37.0, 0.0, 42.0, 100.0] {
        let v = Vignette { amount: 0.0, midpoint: 10.0, feather: 90.0, roundness };
        assert_eq!(vignette_factor((0.1, 0.9), 0.6667, &v), 1.0, "roundness={roundness}");
    }
}

/// The corner (uv=(1,1), i.e. normDist's own normalizing point) reaches
/// the full, un-fed-back Amount effect exactly, for EVERY roundness --
/// all three `norm_dist` branches divide by a `corner_dist` defined as
/// exactly the same expression evaluated at (dx=1, dy=<that branch's own
/// aspect>), so uv=(1,1) yields normDist=1.0 exactly by construction in
/// every branch, not just the unmodified roundness=0 one. midpoint=0,
/// feather=100 puts smoothstep's own edges at (0,1), so a normDist of
/// exactly 1.0 collapses the factor to `1 + amount/100` with zero
/// partial falloff -- same hand-derived trick the pre-RFC-0016 corner
/// test already used, now checked across the whole roundness range.
#[test]
fn vignette_corner_always_reaches_full_effect_regardless_of_roundness() {
    let aspect = 0.6667;
    for roundness in [-100.0, -50.0, 0.0, 50.0, 100.0] {
        let v = Vignette { amount: -80.0, midpoint: 0.0, feather: 100.0, roundness };
        let factor = vignette_factor((1.0, 1.0), aspect, &v);
        assert!((factor - 0.2).abs() < 1e-3, "roundness={roundness}: got {factor}");
    }
}

/// `roundness=+100` collapses `eff_aspect` to exactly 1.0 (`aspect + (1 -
/// aspect) * 1.0 == 1.0` for any `aspect`), so the shape stops being
/// aspect-corrected at all and becomes a true circle: two points offset
/// equally from center along the x-axis and the (unscaled) y-axis must
/// land on the exact same falloff value, even though the image's own
/// aspect ratio (0.5, i.e. not square) would make those two offsets
/// land at different normDist values at roundness=0.
#[test]
fn vignette_roundness_positive_100_produces_a_true_circle_ignoring_aspect() {
    let aspect = 0.5;
    let v = Vignette { amount: 80.0, midpoint: 20.0, feather: 60.0, roundness: 100.0 };
    // uv_x: dx=0.6, dy=0. uv_y: dy=(0.8-0.5)*2*eff_aspect=0.6 (eff_aspect=1
    // at roundness=100), dx=0 -- both reduce to the same (0.6, 0.0)-shaped
    // offset once eff_aspect replaces aspect, so normDist must match.
    let uv_x = (0.8, 0.5);
    let uv_y = (0.5, 0.8);
    let factor_x = vignette_factor(uv_x, aspect, &v);
    let factor_y = vignette_factor(uv_y, aspect, &v);
    assert!((factor_x - factor_y).abs() < 1e-4, "circle asymmetry: x={factor_x} y={factor_y}");
}

/// `roundness=-100` blends the exponent toward `VIGNETTE_ROUNDNESS_MAX_P`,
/// bulging the shape toward the frame's own rectangle everywhere except
/// exactly on the axes/diagonal (where the Lp norm is invariant to `p`).
/// At a point nearer an axis than the diagonal, `corner_dist` (measured
/// exactly at the diagonal-analog corner point) shrinks faster under a
/// higher exponent than the point's own raw distance does, so `normDist`
/// -- and therefore how much of Amount's effect has already kicked in --
/// is STRICTLY LARGER at roundness=-100 than at roundness=0 at that same
/// point. This is a real, derived directional claim (verified against
/// general Lp-norm monotonicity-in-p theory before being encoded here),
/// not an assumption -- see RFC-0016 SS6.
#[test]
fn vignette_roundness_negative_100_reaches_a_near_axis_point_sooner_than_default() {
    let aspect = 0.6667;
    let uv = (0.75, 0.6); // near-axis: dx=0.5 dominates, dy=0.1333 is small
    let default = Vignette { amount: 80.0, midpoint: 20.0, feather: 60.0, roundness: 0.0 };
    let squarer = Vignette { amount: 80.0, midpoint: 20.0, feather: 60.0, roundness: -100.0 };
    let factor_default = vignette_factor(uv, aspect, &default);
    let factor_squarer = vignette_factor(uv, aspect, &squarer);
    assert!(
        factor_squarer > factor_default,
        "expected roundness=-100 to reach more effect at a near-axis point: default={factor_default} squarer={factor_squarer}"
    );
}

/// End-to-end through `apply_edit_stack`: `roundness` parses out of the
/// JSON op payload (`vignette_op`'s new field) and actually reaches
/// `vignette_factor` -- a strongly negative Amount at roundness=-100
/// still darkens the corner and leaves dead-center untouched, the same
/// real-image contract the pre-RFC-0016 end-to-end test already checks,
/// now with the new field threaded all the way through.
#[test]
fn vignette_roundness_field_threads_through_edit_stack_end_to_end() {
    let mut image = RgbImage::from_pixel(21, 21, image::Rgb([200, 150, 100]));
    apply_edit_stack(
        &mut image,
        &EditStack {
            schema_version: 1,
            ops: vec![
                serde_json::json!({ "op": "vignette", "amount": -100.0, "midpoint": 0.0, "feather": 100.0, "roundness": -100.0 }),
            ],
        },
    );
    assert_eq!(*image.get_pixel(10, 10), image::Rgb([200, 150, 100]));
    let corner = image.get_pixel(0, 0);
    assert!(corner[0] < 50, "expected a near-black corner, got {corner:?}");
}

/// RFC-0020 §3.1 golden vectors, computed by an independent numpy
/// implementation (docs/rfc/RFC-0020-appendix/grainlab.py, `hash`), not by
/// this code. The integer hash must match EXACTLY -- the same values are
/// reproduced on a real GPU against the WGSL `particleHash`, which is the
/// proof of the "bit-identical CPU/GPU by construction" claim.
/// (Replaces `grain_hash_at_origin_is_exactly_zero`, which asserted a
/// property of the old sin hash -- `0 * 43758 == 0` -- that the new hash
/// deliberately does not have.)
#[test]
fn grain_particle_hash_matches_golden_vectors_exactly() {
    let golden: [((i32, i32), u32); 7] = [
        ((0, 0), 0xae6f80f1),
        ((1, 0), 0xa07c7a97),
        ((0, 1), 0x8e374fe0),
        ((1, 1), 0xa290702b),
        ((17, 42), 0xf50c661e),
        ((4095, 3071), 0x30ed22ee),
        ((123456, 654321), 0x4cdf1783),
    ];
    for ((ix, iy), want) in golden {
        assert_eq!(particle_hash(ix, iy, 0), want, "particle_hash({ix},{iy},0)");
    }
    // >> 8 / 2^24 is v1's `grain_hash` value: exactly representable in f32.
    assert_eq!((particle_hash(0, 0, 0) >> 8) as f32 / 16_777_216.0, 0.6813888549804688);
}

/// RFC-0020 §5 golden full-function vectors (seed 0), from the numpy
/// reference. Not bit-exact with numpy (different summation/rounding), so a
/// tolerance of 1e-4 on a unit-variance value -- ~5e-6 after the * 0.05
/// scale, far below one 8-bit step.
#[test]
fn grain_particle_noise_matches_golden_vectors() {
    let pts = [(0.0f32, 0.0f32), (3.25, 7.75), (100.5, 200.125), (1234.5, 2345.5)];
    let golden = [
        (0.0f32, [-0.154646f32, 0.932119, -0.581099, 1.521419]),
        (0.5, [-0.257437, 0.772472, -1.018404, 1.568755]),
        (1.0, [-0.167784, 0.286400, -1.398528, 1.504852]),
    ];
    for (rho, want) in golden {
        for (p, w) in pts.iter().zip(want) {
            let got = grain_particle_noise(*p, rho, 0);
            assert!((got - w).abs() < 1e-4, "rho={rho} p={p:?}: got {got}, want {w}");
        }
    }
}

/// Field over a `n` x `n` pixel patch at a given cell size, for the
/// statistical tests below.
fn particle_field(n: usize, cell: f32, rho: f32, seed: u32) -> Vec<f32> {
    let mut v = Vec::with_capacity(n * n);
    for y in 0..n {
        for x in 0..n {
            v.push(grain_particle_noise((x as f32 / cell, y as f32 / cell), rho, seed));
        }
    }
    v
}

fn mean_std(v: &[f32]) -> (f64, f64) {
    let n = v.len() as f64;
    let m = v.iter().map(|&x| x as f64).sum::<f64>() / n;
    let var = v.iter().map(|&x| (x as f64 - m).powi(2)).sum::<f64>() / n;
    (m, var.sqrt())
}

/// RFC-0020's fix for the v1 "Roughness also changes amplitude" item: the
/// noise is unit-variance zero-mean at every roughness and cell size, so
/// Roughness changes character only, never strength.
#[test]
fn grain_particle_noise_is_unit_variance_at_every_roughness_and_size() {
    for &cell in &[1.0f32, 3.5, 6.0] {
        for &rho in &[0.0f32, 0.5, 1.0] {
            let mut all = Vec::new();
            for seed in 0..4u32 {
                all.extend(particle_field(96, cell, rho, seed * 104_729 + 1));
            }
            let (m, sd) = mean_std(&all);
            assert!(m.abs() < 0.05, "cell={cell} rho={rho}: mean {m}");
            assert!((sd - 1.0).abs() < 0.05, "cell={cell} rho={rho}: std {sd}");
        }
    }
}

/// A 24 MP frame's long edge: where RFC-0022's frame-relative Size equals the
/// pre-RFC-0022 fixed pixel scale (Size 25 = 2.25 px), so every pre-existing
/// grain test keeps its meaning.
const GRAIN_TEST_LONG_EDGE: f32 = 6000.0;

/// RFC-0020 §3.3 calibration: the delta's std at Amount 100, default
/// Size/Roughness stays within 5% of the pre-RFC field's measured 0.0511,
/// so a given Amount does not silently get stronger or weaker.
#[test]
fn grain_delta_std_at_defaults_matches_the_pre_rfc_calibration_target() {
    let g = Grain { amount: 100.0, size: 25.0, roughness: 50.0 };
    let mut v = Vec::new();
    for y in 0..256 {
        for x in 0..256 {
            v.push(grain_delta((x as f32, y as f32), &g, GRAIN_TEST_LONG_EDGE));
        }
    }
    let (_, sd) = mean_std(&v);
    assert!((sd - 0.0511).abs() / 0.0511 < 0.05, "delta std {sd}");
}

/// RFC-0020 §1.3 -- the lattice-signature fix, the reason value noise was
/// replaced. Per-pixel variance ACROSS independent seeds must be the same
/// everywhere (a stationary field). Measured with the same 3x3-smoothed
/// variance map as the numpy reference: value noise gives relative std
/// 0.221, a stationary field of this correlation length ~0.045, this
/// generator 0.042-0.051. The threshold sits between them, so a value-noise
/// implementation would fail this loudly. Needs the test-only `seed`
/// parameter.
#[test]
fn grain_particle_noise_is_stationary_across_the_lattice() {
    const N: usize = 32;
    const SEEDS: u32 = 800;
    let cell = 6.0;
    let mut sum = vec![0.0f64; N * N];
    let mut sum2 = vec![0.0f64; N * N];
    for s in 0..SEEDS {
        let f = particle_field(N, cell, 0.5, s * 104_729 + 1);
        for (i, &x) in f.iter().enumerate() {
            sum[i] += x as f64;
            sum2[i] += (x as f64) * (x as f64);
        }
    }
    let n = SEEDS as f64;
    let var: Vec<f64> = (0..N * N).map(|i| sum2[i] / n - (sum[i] / n).powi(2)).collect();
    // 3x3 smoothing, interior pixels only.
    let mut sm = Vec::new();
    for y in 1..N - 1 {
        for x in 1..N - 1 {
            let mut acc = 0.0;
            for dy in 0..3 {
                for dx in 0..3 {
                    acc += var[(y + dy - 1) * N + (x + dx - 1)];
                }
            }
            sm.push(acc / 9.0);
        }
    }
    let m = sm.iter().sum::<f64>() / sm.len() as f64;
    let sd = (sm.iter().map(|v| (v - m).powi(2)).sum::<f64>() / sm.len() as f64).sqrt();
    let rel = sd / m;
    assert!(rel < 0.09, "variance-map relative std {rel} (value noise ~0.22, stationary ~0.05)");
}

/// Isotropy: correlation between two points at the SAME Euclidean
/// distance must not depend on direction -- (5,0) vs (3,4), both 5 px.
/// A square lattice would favour the axes.
#[test]
fn grain_particle_noise_is_isotropic() {
    let n = 160;
    let corr = |f: &[f32], dx: usize, dy: usize| -> f64 {
        let (m, sd) = mean_std(f);
        let mut acc = 0.0;
        let mut cnt = 0.0;
        for y in 0..n - dy {
            for x in 0..n - dx {
                acc += (f[y * n + x] as f64 - m) * (f[(y + dy) * n + x + dx] as f64 - m);
                cnt += 1.0;
            }
        }
        acc / cnt / (sd * sd)
    };
    let mut axis = 0.0;
    let mut diag = 0.0;
    for seed in 0..6u32 {
        let f = particle_field(n, 6.0, 0.5, seed * 104_729 + 1);
        axis += corr(&f, 5, 0);
        diag += corr(&f, 3, 4);
    }
    axis /= 6.0;
    diag /= 6.0;
    assert!((axis - diag).abs() < 0.05, "axis {axis} vs diagonal {diag}");
}

/// The kernel has compact support inside the 3x3 block, so evaluating at
/// `x` just below and just above a cell boundary (one-ulp `floor`
/// disagreement between engines) must give nearly the same value: an ulp of
/// input disagreement stays an ulp-order output difference.
#[test]
fn grain_particle_noise_is_continuous_across_cell_boundaries() {
    for &rho in &[0.0f32, 0.5, 1.0] {
        for &(bx, by) in &[(3.0f32, 5.5f32), (12.0, 7.0), (100.0, 100.0)] {
            let below = grain_particle_noise((f32::from_bits(bx.to_bits() - 1), by), rho, 0);
            let above = grain_particle_noise((bx, by), rho, 0);
            assert!((below - above).abs() < 1e-4, "rho={rho} at ({bx},{by}): {below} vs {above}");
        }
    }
}

/// Different seeds give independent fields (needed by the follow-on
/// per-channel film-grain layers), but the same seed is reproducible.
#[test]
fn grain_particle_noise_seed_selects_an_independent_field() {
    let a = particle_field(48, 3.0, 0.5, 1);
    let b = particle_field(48, 3.0, 0.5, 2);
    assert_eq!(a, particle_field(48, 3.0, 0.5, 1));
    let (ma, sa) = mean_std(&a);
    let (mb, sb) = mean_std(&b);
    let cov = a.iter().zip(&b).map(|(&x, &y)| (x as f64 - ma) * (y as f64 - mb)).sum::<f64>() / a.len() as f64;
    assert!((cov / (sa * sb)).abs() < 0.1, "seeds 1 and 2 correlate: {}", cov / (sa * sb));
}

/// amount=0 must be an EXACT passthrough regardless of coord/size/
/// roughness -- `grain_delta` returns exactly 0.0 without even
/// touching the noise lattice, same structural guarantee every other
/// op's identity value already gets.
#[test]
fn grain_amount_zero_is_an_exact_passthrough() {
    let g = Grain { amount: 0.0, size: 80.0, roughness: 90.0 };
    assert_eq!(grain_delta((123.0, 456.0), &g, GRAIN_TEST_LONG_EDGE), 0.0);
    assert_eq!(grain_delta((0.0, 0.0), &g, GRAIN_TEST_LONG_EDGE), 0.0);
}

/// The grain pattern must be STATIC -- the same coordinate evaluated
/// twice (e.g. across two separate renders) returns the identical
/// delta, not a freshly-randomized one. `grain_delta` is a pure
/// function of its inputs, but this pins that contract down
/// explicitly rather than leaving it implicit.
#[test]
fn grain_delta_is_deterministic_for_the_same_coordinate() {
    let g = Grain { amount: 60.0, size: 40.0, roughness: 30.0 };
    let a = grain_delta((17.0, 42.0), &g, GRAIN_TEST_LONG_EDGE);
    let b = grain_delta((17.0, 42.0), &g, GRAIN_TEST_LONG_EDGE);
    assert_eq!(a, b);
}

/// `size` still sets the feature size: adjacent-pixel correlation grows
/// with the cell width (1 px at Size 0 ~ uncorrelated; 6 px at Size 100 ~
/// strongly correlated). Replaces the old "same blocky lattice cell gives
/// the same delta" test, which described the pre-RFC-0020 raw-cell mode
/// that no longer exists (RFC-0020 §3.3).
#[test]
fn grain_size_sets_the_feature_size() {
    let adjacent_corr = |size: f32| -> f64 {
        let g = Grain { amount: 100.0, size, roughness: 50.0 };
        let n = 128;
        let mut f = Vec::new();
        for y in 0..n {
            for x in 0..n {
                f.push(grain_delta((x as f32, y as f32), &g, GRAIN_TEST_LONG_EDGE));
            }
        }
        let (m, sd) = mean_std(&f);
        let mut acc = 0.0;
        for y in 0..n {
            for x in 0..n - 1 {
                acc += (f[y * n + x] as f64 - m) * (f[y * n + x + 1] as f64 - m);
            }
        }
        acc / (n * (n - 1)) as f64 / (sd * sd)
    };
    let fine = adjacent_corr(0.0);
    let coarse = adjacent_corr(100.0);
    assert!(fine < 0.5, "size 0 adjacent correlation {fine}");
    assert!(coarse > 0.8, "size 100 adjacent correlation {coarse}");
}

/// Roughness actually changes the result -- even (0) and clumpy (100)
/// particles must generally disagree at a non-lattice-aligned point,
/// confirming the knob isn't a no-op.
#[test]
fn grain_roughness_zero_and_one_hundred_generally_differ() {
    let smooth = Grain { amount: 100.0, size: 50.0, roughness: 0.0 };
    let rough = Grain { amount: 100.0, size: 50.0, roughness: 100.0 };
    let coord = (11.3, 27.9);
    assert_ne!(grain_delta(coord, &smooth, GRAIN_TEST_LONG_EDGE), grain_delta(coord, &rough, GRAIN_TEST_LONG_EDGE));
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

/// RFC-0020 §4.3 performance check, kept as an opt-in report rather than an
/// assertion (wall-clock thresholds flake on shared CI runners):
/// `cargo test --release --lib grain_cpu_cost_report -- --ignored --nocapture`.
#[test]
#[ignore]
fn grain_cpu_cost_report() {
    let g = Grain { amount: 100.0, size: 25.0, roughness: 50.0 };
    let (w, h) = (2048usize, 1536usize); // 3.1 MP, ~ the interactive preview cap
    let t = std::time::Instant::now();
    let mut acc = 0.0f32;
    for y in 0..h {
        for x in 0..w {
            acc += grain_delta((x as f32, y as f32), &g, w as f32);
        }
    }
    let dt = t.elapsed();
    let px = (w * h) as f64;
    eprintln!("grain_delta: {:.1} ns/px single thread -> {:.0} ms for {:.1} MP, {:.2} s for 24 MP (acc {acc})",
        dt.as_nanos() as f64 / px, dt.as_secs_f64() * 1e3, px / 1e6, dt.as_secs_f64() / px * 24e6);
}

// ---- RFC-0022 slice 1: frame-relative size + footprint compensation ----

/// §3.4: the cell is a fixed fraction of the frame at any resolution, and a
/// 24 MP frame keeps the pre-RFC-0022 pixel scale exactly (1 + 5*size/100).
#[test]
fn grain_cell_px_is_frame_relative_and_equals_the_old_scale_at_24mp() {
    for size in [0.0f32, 25.0, 50.0, 100.0] {
        let old = 1.0 + (size / 100.0) * 5.0;
        assert!((grain_cell_px(size, 6000.0) - old).abs() < 1e-5, "size {size}");
    }
    for size in [0.0f32, 25.0, 70.0] {
        let a = grain_cell_px(size, 6000.0) / 6000.0;
        let b = grain_cell_px(size, 2048.0) / 2048.0;
        assert!((a - b).abs() < 1e-7, "size {size}: {a} vs {b}");
    }
}

/// The closed form `r(c)` tracks the measured pixel-integrated / point-sampled
/// std ratio. Ground truth: the continuous field (the generator evaluated at
/// any float coordinate) averaged over each pixel's 1x1 footprint with 8x8
/// supersampling, same method as RFC-0022 §3.4's appendix script.
#[test]
fn grain_footprint_ratio_matches_the_supersampled_ground_truth() {
    const N: usize = 72;
    const SS: usize = 8;
    for rho in [0.0f32, 0.5, 1.0] {
        for c in [0.25f32, 0.5, 0.75, 1.0, 1.5, 2.25, 3.5, 6.0] {
            let mut point = Vec::with_capacity(N * N);
            let mut boxed = Vec::with_capacity(N * N);
            for y in 0..N {
                for x in 0..N {
                    point.push(grain_particle_noise((x as f32 / c, y as f32 / c), rho, 0));
                    let mut acc = 0.0f32;
                    for sy in 0..SS {
                        for sx in 0..SS {
                            let fx = x as f32 + (sx as f32 + 0.5) / SS as f32;
                            let fy = y as f32 + (sy as f32 + 0.5) / SS as f32;
                            acc += grain_particle_noise((fx / c, fy / c), rho, 0);
                        }
                    }
                    boxed.push(acc / (SS * SS) as f32);
                }
            }
            let ratio = mean_std(&boxed).1 / mean_std(&point).1;
            let fit = grain_footprint_ratio(c, rho) as f64;
            eprintln!("rho {rho:.1} c {c:>4}: measured {ratio:.3} fit {fit:.3} ({:+.1}%)", (fit / ratio - 1.0) * 100.0);
            assert!((fit / ratio - 1.0).abs() < 0.05, "rho {rho} c {c}: measured {ratio} vs fit {fit}");
        }
    }
}

/// The milestone's own criterion, proven rather than assumed: the same frame
/// rendered at 6144 px and at 2048 px (a 3x box downsample apart) shows the
/// same per-pixel grain amplitude once the high-resolution render is
/// box-downsampled to the preview's grid. The residual (the spectrum is not
/// matched, only the amplitude) is printed, not hidden -- RFC-0022 §3.4.
#[test]
fn grain_preview_matches_a_downsampled_full_resolution_render() {
    const LO: f32 = 2048.0;
    const F: usize = 3;
    const HI: f32 = LO * F as f32;
    const N: usize = 64;
    let (ox, oy) = (200usize, 120usize);
    for size in [25.0f32, 60.0, 100.0] {
        let g = Grain { amount: 100.0, size, roughness: 50.0 };
        let mut lo = Vec::with_capacity(N * N);
        let mut avg = Vec::with_capacity(N * N);
        for y in 0..N {
            for x in 0..N {
                lo.push(grain_delta(((ox + x) as f32, (oy + y) as f32), &g, LO));
                let mut acc = 0.0f32;
                for dy in 0..F {
                    for dx in 0..F {
                        acc += grain_delta((((ox + x) * F + dx) as f32, ((oy + y) * F + dy) as f32), &g, HI);
                    }
                }
                avg.push(acc / (F * F) as f32);
            }
        }
        let (_, sd_lo) = mean_std(&lo);
        let (_, sd_hi) = mean_std(&avg);
        let ratio = sd_hi / sd_lo;
        let corr = |a: &[f32], b: &[f32]| {
            let (ma, sa) = mean_std(a);
            let (mb, sb) = mean_std(b);
            a.iter().zip(b).map(|(x, y)| (*x as f64 - ma) * (*y as f64 - mb)).sum::<f64>() / a.len() as f64 / (sa * sb)
        };
        eprintln!("size {size}: preview std {sd_lo:.4}, downsampled-export std {sd_hi:.4}, ratio {ratio:.3}, pixelwise corr {:.2}", corr(&lo, &avg));
        assert!((ratio - 1.0).abs() < 0.12, "size {size}: export/preview std ratio {ratio}");
    }
}

/// Dumps CPU `grain_delta` values for the golden table below and for the
/// real-GPU probe (`docs/rfc/RFC-0022-appendix/gpu_grain_probe.js`):
/// `cargo test --lib grain_reference_dump -- --ignored --nocapture`.
#[test]
#[ignore]
fn grain_reference_dump() {
    for &(le, size, rough, x, y) in GRAIN_GOLDEN_CASES {
        let g = Grain { amount: 100.0, size, roughness: rough };
        eprintln!("({le:?}, {size:?}, {rough:?}, {x:?}, {y:?}) -> {:?}", grain_delta((x, y), &g, le));
    }
}

/// (long edge, size, roughness, x, y) cases covering sub-pixel (preview-size
/// default), the 24 MP default, a 3x export, and both roughness extremes.
const GRAIN_GOLDEN_CASES: &[(f32, f32, f32, f32, f32)] = &[
    (6000.0, 25.0, 50.0, 123.0, 456.0),
    (2048.0, 25.0, 50.0, 700.0, 31.0),
    (2048.0, 0.0, 0.0, 5.0, 9.0),
    (2048.0, 100.0, 100.0, 1500.0, 1000.0),
    (6144.0, 60.0, 30.0, 3001.0, 777.0),
    (1000.0, 25.0, 50.0, 10.0, 10.0),
    (2048.0, 40.0, 100.0, 333.0, 222.0),
    (4000.0, 10.0, 0.0, 1234.0, 2345.0),
];

/// Pinned `grain_delta` values (same order as `GRAIN_GOLDEN_CASES`, Amount 100);
/// the real-GPU probe checks the WGSL against these same numbers, so CPU and
/// GPU are each tied to one table rather than only to each other.
const GRAIN_GOLDEN_VALUES: &[f32] = &[
    0.042875215, -0.011109176, -0.026815541, -0.042142715,
    0.025983753, -0.016688382, 0.00022242409, 0.050142936,
];

#[test]
fn grain_delta_matches_the_golden_table() {
    for (&(le, size, rough, x, y), &want) in GRAIN_GOLDEN_CASES.iter().zip(GRAIN_GOLDEN_VALUES) {
        let g = Grain { amount: 100.0, size, roughness: rough };
        let got = grain_delta((x, y), &g, le);
        assert!((got - want).abs() < 2e-6, "({le}, {size}, {rough}, {x}, {y}): {got} vs {want}");
    }
}
