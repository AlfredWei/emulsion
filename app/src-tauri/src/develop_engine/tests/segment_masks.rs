//! `segment_mask` (M6 slice 2b, RFC-0026): weights from hand-derived values on
//! small synthetic logit fields (a real SAM field is only needed for the size
//! check in the helper's tests). The field's byte `q` is the logit
//! `q/255*8 - 4`; a weight is `clamp(0.5 + (logit + grow*0.03)/(2h), 0, 1)` with
//! `h = 0.25 + 3.45*feather/100`.
use super::*;
use serde_json::{json, Value};

const N: usize = 256;

/// Base64 of an N x N 8-bit grayscale PNG with `q(x, y)` as byte values.
fn field(q: impl Fn(usize, usize) -> u8) -> String {
    use base64::Engine;
    use image::ImageEncoder;
    let raw: Vec<u8> = (0..N * N).map(|i| q(i % N, i / N)).collect();
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png).write_image(&raw, N as u32, N as u32, image::ExtendedColorType::L8).unwrap();
    base64::engine::general_purpose::STANDARD.encode(png)
}

fn segment(logits: String, feather: f32, grow: f32) -> Value {
    json!({ "op": "segment_mask", "id": "s", "logits": logits, "feather": feather, "grow": grow, "invert": false,
            "exposure": 1.0, "contrast": 0.0, "saturation": 0.0 })
}

fn weight(op: Value, uv: (f32, f32)) -> f32 {
    let masks = parse_masks(&[op]);
    assert_eq!(masks.len(), 1, "op should parse as one mask");
    masks[0].weight(uv, 1.0, [0.4; 3])
}

fn near(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 1e-4, "expected {expected}, got {actual}");
}

#[test]
fn saturated_fields_are_exactly_in_or_out_and_invert_flips() {
    let all_in = field(|_, _| 255); // logit +4
    let all_out = field(|_, _| 0); // logit -4
    for feather in [0.0, 50.0, 100.0] {
        assert_eq!(weight(segment(all_in.clone(), feather, 0.0), (0.3, 0.7)), 1.0, "feather {feather}");
        assert_eq!(weight(segment(all_out.clone(), feather, 0.0), (0.3, 0.7)), 0.0, "feather {feather}");
    }
    let mut inverted = segment(all_in, 0.0, 0.0);
    inverted["invert"] = json!(true);
    assert_eq!(weight(inverted, (0.5, 0.5)), 0.0);
}

#[test]
fn constant_field_hand_values_for_feather_and_grow() {
    // q = 191 -> logit 191/255*8 - 4 = 1.99216; feather 100 -> h = 3.7
    let f = field(|_, _| 191);
    near(weight(segment(f.clone(), 100.0, 0.0), (0.5, 0.5)), 0.76921); // 0.5 + 1.99216/7.4
    // feather 0: h = 0.25 -> saturates; feather 50: h = 1.975 -> also saturates here (0.5 + 1.99/3.95 > 1)
    assert_eq!(weight(segment(f.clone(), 0.0, 0.0), (0.5, 0.5)), 1.0);
    assert_eq!(weight(segment(f.clone(), 50.0, 0.0), (0.5, 0.5)), 1.0);
    // grow -50 shifts the logit by -1.5 -> 0.492157
    near(weight(segment(f.clone(), 100.0, -50.0), (0.5, 0.5)), 0.566508);
    // grow +100 pushes a mildly-out field in: q = 64 -> logit -1.99216; +3 -> 1.00784
    near(weight(segment(field(|_, _| 64), 100.0, 100.0), (0.5, 0.5)), 0.636195);
}

#[test]
fn logits_are_interpolated_bilinearly_before_the_ramp() {
    // Left half +4, right half -4: between texels 127 and 128 the logit crosses 0 at uv.x = 0.5.
    let half = field(|x, _| if x < N / 2 { 255 } else { 0 });
    for feather in [0.0, 60.0, 100.0] {
        near(weight(segment(half.clone(), feather, 0.0), (0.5, 0.5)), 0.5);
    }
    // A quarter of a texel left of the crossing: logit = 4*0.75 - 4*0.25 = 2 -> feather 100 (h = 3.7): 0.5 + 2/7.4.
    near(weight(segment(half.clone(), 100.0, 0.0), (127.75 / 256.0, 0.5)), 0.770_270);
    // Vertical interpolation: top half in, bottom out.
    let tb = field(|_, y| if y < N / 2 { 255 } else { 0 });
    near(weight(segment(tb, 100.0, 0.0), (0.5, 127.75 / 256.0)), 0.770_270);
    // The border clamps instead of wrapping or reading out of range.
    assert_eq!(weight(segment(half.clone(), 0.0, 0.0), (0.0, 0.5)), 1.0);
    assert_eq!(weight(segment(half, 0.0, 0.0), (1.0, 0.5)), 0.0);
}

#[test]
fn a_damaged_or_wrong_sized_field_is_skipped_not_a_failure() {
    let mut op = segment("not base64 !!".into(), 0.0, 0.0);
    assert!(parse_masks(&[op.clone()]).is_empty());
    op["logits"] = json!("aGVsbG8="); // valid base64, not a PNG
    assert!(parse_masks(&[op.clone()]).is_empty());
    // A valid PNG of the wrong size.
    use base64::Engine;
    use image::ImageEncoder;
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png).write_image(&[0u8; 16], 4, 4, image::ExtendedColorType::L8).unwrap();
    op["logits"] = json!(base64::engine::general_purpose::STANDARD.encode(png));
    assert!(parse_masks(&[op.clone()]).is_empty());
    op.as_object_mut().unwrap().remove("logits");
    assert!(parse_masks(&[op]).is_empty());
}

#[test]
fn a_segment_works_as_a_modifier_and_as_a_base_with_modifiers() {
    let linear = |x0: f32, x1: f32| json!({ "op": "linear_gradient_mask", "start": {"x": x0, "y": 0.5}, "end": {"x": x1, "y": 0.5}, "feather": 0.0, "invert": false });
    let seg_shape = |q: u8| json!({ "op": "segment_mask", "logits": field(move |_, _| q), "feather": 100.0, "grow": 0.0, "invert": false });
    let with = |mut base: Value, combine: &str, shape: Value| {
        base["id"] = json!("m");
        base["exposure"] = json!(1.0);
        base["modifiers"] = json!([{ "id": "mod", "combine": combine, "shape": shape }]);
        base
    };
    // Base linear weight 0.5 at the centre; segment shape weight 0.769210 (q = 191, feather 100).
    let c = 0.769_210_f32;
    near(weight(with(linear(0.0, 1.0), "subtract", seg_shape(191)), (0.5, 0.5)), 0.5 * (1.0 - c));
    near(weight(with(linear(0.0, 1.0), "intersect", seg_shape(191)), (0.5, 0.5)), 0.5 * c);
    near(weight(with(linear(0.0, 1.0), "add", seg_shape(191)), (0.5, 0.5)), 0.5 + c - 0.5 * c);
    // Segment as the base with a linear subtract: 0.769210 * (1 - 0.5).
    let mut base = segment(field(|_, _| 191), 100.0, 0.0);
    base["modifiers"] = json!([{ "id": "mod", "combine": "subtract", "shape": linear(0.0, 1.0) }]);
    near(weight(base, (0.5, 0.5)), c * 0.5);
}

#[test]
fn the_pixel_path_applies_the_adjustment_through_the_segment_weight() {
    let run = |op: Value| {
        let mut image = RgbImage::from_pixel(1, 1, image::Rgb([100, 100, 100]));
        apply_edit_stack(&mut image, &EditStack { schema_version: 1, ops: vec![op] });
        image.get_pixel(0, 0).0
    };
    assert_eq!(run(segment(field(|_, _| 255), 0.0, 0.0)), [138, 138, 138], "fully inside = the full +1 EV");
    assert_eq!(run(segment(field(|_, _| 0), 0.0, 0.0)), [100, 100, 100], "fully outside = untouched");
}

#[test]
fn a_segment_mask_survives_the_catalog_json_round_trip_and_renders_identically() {
    let op = segment(field(|x, y| ((x * 255) / N + (y * 255) / N).min(255) as u8), 30.0, 10.0);
    let stack = EditStack { schema_version: 1, ops: vec![op] };
    let back: EditStack = serde_json::from_str(&serde_json::to_string(&stack).unwrap()).unwrap();
    for uv in [(0.1, 0.2), (0.5, 0.5), (0.9, 0.8)] {
        near(weight(stack.ops[0].clone(), uv), weight(back.ops[0].clone(), uv));
    }
}
