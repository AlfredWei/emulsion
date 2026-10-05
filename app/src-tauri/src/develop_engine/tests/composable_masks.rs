//! Composable masking (RFC-0025 slice 1a): `modifiers` on a weight-producing
//! mask. Weights are checked directly through `parse_masks` + `Mask::weight`
//! (hand values, derived on paper from the three fold formulas), and the
//! pixel path through `apply_edit_stack` for the set-algebra cases where every
//! weight is exactly 0 or 1. A 1x1 image samples at uv = (0.5, 0.5).
use super::*;
use serde_json::{json, Value};

const GRAY: f32 = 100.0 / 255.0;

fn weight_at_center(op: Value) -> f32 {
    let masks = parse_masks(&[op]);
    assert_eq!(masks.len(), 1, "op should parse as one mask");
    masks[0].weight((0.5, 0.5), 1.0, [GRAY; 3])
}

/// Linear gradient along x from `x0` to `x1`, feather 0: weight at uv.x = 0.5
/// is `(0.5 - x0) / (x1 - x0)`.
fn linear(x0: f32, x1: f32) -> Value {
    json!({ "op": "linear_gradient_mask", "start": {"x": x0, "y": 0.5}, "end": {"x": x1, "y": 0.5}, "feather": 0.0, "invert": false })
}

/// Weight exactly 1 at the image centre (inside, inverted radial).
fn full() -> Value {
    json!({ "op": "radial_gradient_mask", "center": {"x": 0.5, "y": 0.5}, "radiusX": 0.3, "radiusY": 0.3, "feather": 0.0, "invert": true })
}

/// Weight exactly 0 at the image centre (outside the ellipse, default invert).
fn none() -> Value {
    json!({ "op": "radial_gradient_mask", "center": {"x": 0.5, "y": 0.5}, "radiusX": 0.3, "radiusY": 0.3, "feather": 0.0, "invert": false })
}

fn with_mods(mut base: Value, mods: Vec<(&str, Value)>) -> Value {
    base["id"] = json!("m");
    base["exposure"] = json!(1.0);
    base["contrast"] = json!(0.0);
    base["saturation"] = json!(0.0);
    base["modifiers"] = Value::Array(
        mods.into_iter()
            .enumerate()
            .map(|(i, (combine, shape))| json!({ "id": format!("m{i}"), "combine": combine, "shape": shape }))
            .collect(),
    );
    base
}

fn pixel(op: Value) -> [u8; 3] {
    let mut image = RgbImage::from_pixel(1, 1, image::Rgb([100, 100, 100]));
    apply_edit_stack(&mut image, &EditStack { schema_version: 1, ops: vec![op] });
    image.get_pixel(0, 0).0
}

fn assert_near(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 1e-5, "expected {expected}, got {actual}");
}

// Base weight 0.5 (0.2 -> 0.8), modifier weight 0.25 (0.4 -> 0.8).
#[test]
fn fold_formulas_hand_values_on_soft_weights() {
    let base = || linear(0.2, 0.8);
    let m = || linear(0.4, 0.8);
    assert_near(weight_at_center(base()), 0.5);
    assert_near(weight_at_center(m()), 0.25);
    assert_near(weight_at_center(with_mods(base(), vec![("add", m())])), 0.5 + 0.25 - 0.125);
    assert_near(weight_at_center(with_mods(base(), vec![("subtract", m())])), 0.5 * 0.75);
    assert_near(weight_at_center(with_mods(base(), vec![("intersect", m())])), 0.5 * 0.25);
}

#[test]
fn modifiers_fold_left_in_listed_order() {
    // ((0.5 add 0.25) subtract 0.25) intersect 0.25
    let w = with_mods(
        linear(0.2, 0.8),
        vec![("add", linear(0.4, 0.8)), ("subtract", linear(0.4, 0.8)), ("intersect", linear(0.4, 0.8))],
    );
    assert_near(weight_at_center(w), 0.625 * 0.75 * 0.25);
}

#[test]
fn hard_weights_are_exact_set_algebra() {
    for (base, comb, m, expected) in [
        (full(), "subtract", full(), 0.0),
        (full(), "subtract", none(), 1.0),
        (none(), "add", full(), 1.0),
        (none(), "add", none(), 0.0),
        (full(), "intersect", full(), 1.0),
        (full(), "intersect", none(), 0.0),
        (none(), "intersect", full(), 0.0),
    ] {
        assert_eq!(weight_at_center(with_mods(base, vec![(comb, m)])), expected, "{comb}");
    }
}

#[test]
fn order_changes_the_result() {
    // base 1; (add B=1, subtract C=1) -> 0, but (subtract C, add B) -> 1.
    let a = with_mods(full(), vec![("add", full()), ("subtract", full())]);
    let b = with_mods(full(), vec![("subtract", full()), ("add", full())]);
    assert_eq!(weight_at_center(a), 0.0);
    assert_eq!(weight_at_center(b), 1.0);
}

#[test]
fn a_mask_without_modifiers_renders_exactly_as_before() {
    let plain = {
        let mut o = full();
        o["id"] = json!("m");
        o["exposure"] = json!(1.0);
        o["contrast"] = json!(0.0);
        o["saturation"] = json!(0.0);
        o
    };
    let mut empty = plain.clone();
    empty["modifiers"] = json!([]);
    assert_eq!(pixel(plain.clone()), pixel(empty));
    assert_eq!(pixel(plain), [138, 138, 138]);
}

#[test]
fn subtracting_a_brush_removes_the_local_adjustment_there() {
    let dab = json!({ "x": 0.5, "y": 0.5, "radius": 0.2, "hardness": 100.0, "flow": 1.0, "mode": "add" });
    let brush = json!({ "op": "brush_mask", "dabs": [dab], "invert": false });
    assert_eq!(pixel(with_mods(full(), vec![("subtract", brush.clone())])), [100, 100, 100]);
    // The same brush intersected leaves the full adjustment in place.
    assert_eq!(pixel(with_mods(full(), vec![("intersect", brush)])), [138, 138, 138]);
}

#[test]
fn luminance_modifier_selects_by_the_pixel_value_entering_the_mask() {
    let lum = |lo: f32, hi: f32| json!({ "op": "luminance_range_mask", "rangeMin": lo, "rangeMax": hi, "feather": 0.0, "invert": false });
    // luma 0.39: inside 0-50, outside 60-100.
    assert_eq!(weight_at_center(with_mods(full(), vec![("intersect", lum(0.0, 50.0))])), 1.0);
    assert_eq!(weight_at_center(with_mods(full(), vec![("intersect", lum(60.0, 100.0))])), 0.0);
    assert_eq!(weight_at_center(with_mods(full(), vec![("subtract", lum(0.0, 50.0))])), 0.0);
}

#[test]
fn colour_modifier_selects_by_distance_to_the_reference() {
    let col = |c: f32| json!({ "op": "color_range_mask", "refColor": {"r": c, "g": c, "b": c}, "range": 10.0, "feather": 0.0, "invert": false });
    assert_eq!(weight_at_center(with_mods(full(), vec![("intersect", col(GRAY))])), 1.0);
    assert_eq!(weight_at_center(with_mods(full(), vec![("intersect", col(1.0))])), 0.0);
}

#[test]
fn every_weight_kind_can_be_the_base() {
    let dab = json!({ "x": 0.5, "y": 0.5, "radius": 0.2, "hardness": 100.0, "flow": 1.0, "mode": "add" });
    let bases = [
        linear(0.0, 0.01),
        full(),
        json!({ "op": "brush_mask", "dabs": [dab], "invert": false }),
        json!({ "op": "luminance_range_mask", "rangeMin": 0.0, "rangeMax": 100.0, "feather": 0.0, "invert": false }),
        json!({ "op": "color_range_mask", "refColor": {"r": GRAY, "g": GRAY, "b": GRAY}, "range": 10.0, "feather": 0.0, "invert": false }),
    ];
    for base in bases {
        let kind = base["op"].as_str().unwrap().to_string();
        assert_eq!(weight_at_center(with_mods(base.clone(), vec![("subtract", full())])), 0.0, "{kind}");
        assert_eq!(weight_at_center(with_mods(base, vec![("subtract", none())])), 1.0, "{kind}");
    }
}

#[test]
fn invalid_or_ineligible_modifiers_are_skipped() {
    let spot = json!({ "op": "spot_mask", "dabs": [], "sourceOffset": {"dx": 0.1, "dy": 0.0} });
    let red_eye = json!({ "op": "red_eye_mask", "center": {"x": 0.5, "y": 0.5}, "radiusX": 0.3, "radiusY": 0.3 });
    let mut op = with_mods(full(), vec![("subtract", spot), ("subtract", red_eye), ("subtract", json!({ "no": "op" }))]);
    // An unknown combine and a missing shape are skipped too.
    op["modifiers"].as_array_mut().unwrap().push(json!({ "combine": "xor", "shape": full() }));
    op["modifiers"].as_array_mut().unwrap().push(json!({ "combine": "subtract" }));
    assert_eq!(weight_at_center(op), 1.0);
}

#[test]
fn modifiers_inside_a_modifier_shape_are_ignored() {
    let mut shape = full();
    shape["modifiers"] = json!([{ "combine": "subtract", "shape": full() }]);
    // If the nested subtract applied, the shape would weigh 0 and Subtract would leave the base at 1.
    assert_eq!(weight_at_center(with_mods(full(), vec![("subtract", shape)])), 0.0);
}

#[test]
fn spot_and_red_eye_ignore_a_modifiers_array() {
    let red_eye = json!({ "op": "red_eye_mask", "center": {"x": 0.5, "y": 0.5}, "radiusX": 0.3, "radiusY": 0.3, "feather": 0.0, "pupilSize": 50.0, "darken": 0.0 });
    let mut with = red_eye.clone();
    with["modifiers"] = json!([{ "combine": "subtract", "shape": full() }]);
    let a = weight_at_center(red_eye);
    let b = weight_at_center(with);
    assert_eq!(a, b);
}

#[test]
fn weights_stay_in_unit_range_for_arbitrary_stacks() {
    let w = with_mods(
        linear(0.2, 0.8),
        vec![("add", linear(0.1, 0.9)), ("add", full()), ("subtract", linear(0.4, 0.8)), ("intersect", linear(0.3, 0.7))],
    );
    let v = weight_at_center(w);
    assert!((0.0..=1.0).contains(&v));
}

/// The catalog stores an edit stack as opaque JSON text (`EditStack.ops: Vec<Value>`), so a mask's
/// `modifiers` must survive serialize -> parse unchanged and render the same afterwards.
#[test]
fn modifiers_survive_the_catalog_json_round_trip_and_render_identically() {
    let op = with_mods(
        full(),
        vec![
            ("subtract", json!({ "op": "brush_mask", "invert": false, "dabs": [
                { "x": 0.5, "y": 0.5, "radius": 0.2, "hardness": 100.0, "flow": 1.0, "mode": "add" } ] })),
            ("intersect", linear(0.2, 0.8)),
        ],
    );
    let stack = EditStack { schema_version: 1, ops: vec![op] };
    let text = serde_json::to_string(&stack).unwrap();
    let back: EditStack = serde_json::from_str(&text).unwrap();
    assert_eq!(back, stack);
    let render = |s: &EditStack| {
        let mut image = RgbImage::from_pixel(8, 8, image::Rgb([100, 100, 100]));
        apply_edit_stack(&mut image, s);
        image.into_raw()
    };
    assert_eq!(render(&back), render(&stack));
}

// ---------------------------------------------------------------------------
// Reference dump for the real-GPU probe (docs/rfc/RFC-0025-appendix/gpu_mask_probe.js).
// The probe renders the shipped `fs_mask` on a real GPU over the same NxN
// image and compares it with these CPU pixels. Run:
//   COMPOSABLE_MASK_DUMP=/path/out.json cargo test --lib composable_mask_reference_dump -- --ignored
// ---------------------------------------------------------------------------

const DUMP_SIZE: u32 = 512;

fn dump_image() -> RgbImage {
    RgbImage::from_fn(DUMP_SIZE, DUMP_SIZE, |x, y| {
        image::Rgb([(x * 255 / (DUMP_SIZE - 1)) as u8, (y * 255 / (DUMP_SIZE - 1)) as u8, ((x + y) * 255 / (2 * (DUMP_SIZE - 1))) as u8])
    })
}

fn dump_cases() -> Vec<(&'static str, Vec<Value>)> {
    let adj = |mut o: Value, e: f64, c: f64, s: f64| {
        o["id"] = json!("m");
        o["exposure"] = json!(e);
        o["contrast"] = json!(c);
        o["saturation"] = json!(s);
        o
    };
    let modifiers = |mut o: Value, mods: Vec<(&str, Value)>| {
        o["modifiers"] = Value::Array(
            mods.into_iter()
                .enumerate()
                .map(|(i, (c, s))| json!({ "id": format!("mod{i}"), "combine": c, "shape": s }))
                .collect(),
        );
        o
    };
    let radial = |cx: f64, cy: f64, r: f64, feather: f64, invert: bool| {
        json!({ "op": "radial_gradient_mask", "center": {"x": cx, "y": cy}, "radiusX": r, "radiusY": r, "feather": feather, "invert": invert })
    };
    let linear = |x0: f64, y0: f64, x1: f64, y1: f64, feather: f64| {
        json!({ "op": "linear_gradient_mask", "start": {"x": x0, "y": y0}, "end": {"x": x1, "y": y1}, "feather": feather, "invert": false })
    };
    let lum = |lo: f64, hi: f64, feather: f64| {
        json!({ "op": "luminance_range_mask", "rangeMin": lo, "rangeMax": hi, "feather": feather, "invert": false })
    };
    let brush = json!({ "op": "brush_mask", "invert": false, "dabs": [
        { "x": 0.5, "y": 0.5, "radius": 0.15, "hardness": 100.0, "flow": 1.0, "mode": "add" } ] });
    let colour = json!({ "op": "color_range_mask", "refColor": {"r": 0.502, "g": 0.502, "b": 0.502}, "range": 30.0, "feather": 40.0, "invert": false });

    vec![
        ("plain radial, no modifiers", vec![adj(radial(0.5, 0.5, 0.4, 40.0, true), 1.0, 0.0, 0.0)]),
        // Stand-alone brush masks (no modifiers): the baseline for the brush-modifier case below,
        // since a hard-edged dab is rasterized by Canvas2D on the GPU path and analytically on the CPU.
        (
            "plain brush, hardness 100",
            vec![adj(json!({ "op": "brush_mask", "invert": false, "dabs": [
                { "x": 0.5, "y": 0.5, "radius": 0.15, "hardness": 100.0, "flow": 1.0, "mode": "add" } ] }), 1.0, 0.0, 0.0)],
        ),
        (
            "plain brush, hardness 50",
            vec![adj(json!({ "op": "brush_mask", "invert": false, "dabs": [
                { "x": 0.5, "y": 0.5, "radius": 0.15, "hardness": 50.0, "flow": 1.0, "mode": "add" } ] }), 1.0, 0.0, 0.0)],
        ),
        (
            "radial minus brush",
            vec![adj(modifiers(radial(0.5, 0.5, 0.4, 30.0, true), vec![("subtract", brush)]), 1.0, 0.0, 0.0)],
        ),
        (
            "linear, intersect luminance, add radial",
            vec![adj(
                modifiers(
                    linear(0.1, 0.5, 0.9, 0.5, 20.0),
                    vec![("intersect", lum(20.0, 70.0, 30.0)), ("add", radial(0.9, 0.9, 0.2, 50.0, true))],
                ),
                0.8,
                20.0,
                -30.0,
            )],
        ),
        (
            "colour minus linear",
            vec![adj(modifiers(colour, vec![("subtract", linear(0.5, 0.2, 0.5, 0.8, 0.0))]), -0.8, 0.0, 0.0)],
        ),
        (
            "second mask reads the first's result",
            vec![
                adj(radial(0.5, 0.5, 0.5, 20.0, true), 1.5, 0.0, 0.0),
                adj(modifiers(lum(30.0, 80.0, 20.0), vec![("intersect", linear(0.0, 0.5, 1.0, 0.5, 0.0))]), -1.0, 0.0, 0.0),
            ],
        ),
    ]
}

#[test]
#[ignore]
fn composable_mask_reference_dump() {
    let Ok(path) = std::env::var("COMPOSABLE_MASK_DUMP") else {
        panic!("set COMPOSABLE_MASK_DUMP to the output path");
    };
    let cases: Vec<Value> = dump_cases()
        .into_iter()
        .map(|(name, ops)| {
            let mut image = dump_image();
            apply_edit_stack(&mut image, &EditStack { schema_version: 1, ops: ops.clone() });
            json!({ "name": name, "ops": ops, "pixels": image.into_raw() })
        })
        .collect();
    std::fs::write(&path, serde_json::to_string(&json!({ "size": DUMP_SIZE, "cases": cases })).unwrap()).unwrap();
}
