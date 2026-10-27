//! RFC-0036 R1: a luminance mask with `"basis": "original"` selects the same pixels whatever edits sit above it, and a
//! mask without it still reads the pixel as edited (the old WYSIWYG behaviour, pinned here beside the new one).
use super::*;
use serde_json::{json, Value};

const DARK: u8 = 60; // luma 0.235: inside a 0..30 % range
const BRIGHT: u8 = 200;

fn luminance(basis: Option<&str>) -> Value {
    let mut op = json!({
        "op": "luminance_range_mask", "id": "lum", "rangeMin": 0.0, "rangeMax": 30.0, "feather": 0.0, "invert": false,
        "exposure": 1.0, "contrast": 0.0, "saturation": 0.0,
    });
    if let Some(b) = basis {
        op["basis"] = json!(b);
    }
    op
}

/// A linear mask that is at full weight everywhere (the end point is far left of every pixel).
fn everything(exposure: f32) -> Value {
    json!({ "op": "linear_gradient_mask", "id": "all", "start": { "x": -1.0, "y": 0.5 }, "end": { "x": -0.9, "y": 0.5 },
            "feather": 0.0, "invert": false, "exposure": exposure, "contrast": 0.0, "saturation": 0.0 })
}

fn render(global_exposure: f32, masks: Vec<Value>) -> RgbImage {
    let mut image = RgbImage::new(2, 1);
    image.put_pixel(0, 0, image::Rgb([DARK; 3]));
    image.put_pixel(1, 0, image::Rgb([BRIGHT; 3]));
    let mut ops = vec![json!({ "op": "exposure", "value": global_exposure })];
    ops.extend(masks);
    apply_edit_stack(&mut image, &EditStack { schema_version: 1, ops });
    image
}

fn weight(op: Value, edited: [f32; 3], original: [f32; 3]) -> f32 {
    let masks = parse_masks(&[op]);
    assert_eq!(masks.len(), 1);
    masks[0].weight((0.5, 0.5), 1.0, edited, original)
}

#[test]
fn the_basis_decides_which_pixel_is_measured() {
    let (dark, bright) = ([0.235; 3], [0.78; 3]);
    // Edited far brighter than the original: the original basis still sees a shadow, the default does not.
    assert_eq!(weight(luminance(Some("original")), bright, dark), 1.0);
    assert_eq!(weight(luminance(None), bright, dark), 0.0);
    // And the other way round: an original that was bright, edited darker.
    assert_eq!(weight(luminance(Some("original")), dark, bright), 0.0);
    assert_eq!(weight(luminance(None), dark, bright), 1.0);
    // "edited" and an unknown word are the default, never a silent switch.
    assert_eq!(weight(luminance(Some("edited")), bright, dark), 0.0);
    assert_eq!(weight(luminance(Some("raw")), bright, dark), 0.0);
}

#[test]
fn the_selection_is_the_same_pixels_whatever_the_global_edit() {
    for ev in [-1.0, -0.5, 0.0, 0.7, 1.5, 3.0] {
        let with = render(ev, vec![luminance(Some("original"))]);
        let without = render(ev, vec![]);
        assert_eq!(with.get_pixel(1, 0), without.get_pixel(1, 0), "the bright pixel is never selected (global {ev} EV)");
        assert_ne!(with.get_pixel(0, 0), without.get_pixel(0, 0), "the dark pixel is always selected (global {ev} EV)");
    }
}

#[test]
fn the_default_basis_still_moves_with_the_edit() {
    // Pinned old behaviour: after +1.5 EV the dark pixel is out of the range, so the mask misses it.
    let with = render(1.5, vec![luminance(None)]);
    let without = render(1.5, vec![]);
    assert_eq!(with.get_pixel(0, 0), without.get_pixel(0, 0));
    // At 0 EV it is inside, so the same mask hits it.
    assert_ne!(render(0.0, vec![luminance(None)]).get_pixel(0, 0), render(0.0, vec![]).get_pixel(0, 0));
}

#[test]
fn an_earlier_mask_cannot_move_an_original_basis_selection() {
    // A full-weight +2 EV mask above it pushes the dark pixel far out of range for the default basis only.
    let above = everything(2.0);
    let original = render(0.0, vec![above.clone(), luminance(Some("original"))]);
    let only_above = render(0.0, vec![above.clone()]);
    assert_ne!(original.get_pixel(0, 0), only_above.get_pixel(0, 0), "selected despite the mask above");
    assert_eq!(original.get_pixel(1, 0), only_above.get_pixel(1, 0));
    let edited = render(0.0, vec![above.clone(), luminance(None)]);
    assert_eq!(edited.get_pixel(0, 0), only_above.get_pixel(0, 0), "the default basis misses it (old behaviour)");
}

#[test]
fn a_luminance_modifier_takes_the_basis_too() {
    let mut base = everything(1.0);
    base["modifiers"] = json!([{ "combine": "intersect", "shape": { "op": "luminance_range_mask", "rangeMin": 0.0, "rangeMax": 30.0,
        "feather": 0.0, "invert": false, "basis": "original" } }]);
    let global = 1.5;
    let masked = render(global, vec![base]);
    let plain = render(global, vec![]);
    assert_ne!(masked.get_pixel(0, 0), plain.get_pixel(0, 0), "the dark pixel is inside the modifier");
    assert_eq!(masked.get_pixel(1, 0), plain.get_pixel(1, 0), "the bright pixel is intersected away");
}

#[test]
fn a_stored_mask_without_a_basis_is_unchanged() {
    // The same ops that existed before RFC-0036 render exactly as the existing masks tests expect (see masks.rs); here
    // only that the absent field parses to the default.
    let parsed = parse_masks(&[luminance(None)]);
    match &parsed[0] {
        Mask::LuminanceRange(m) => assert!(!m.original_basis),
        _ => panic!("a luminance mask"),
    }
}
