use super::*;
use super::support::dehaze_test_image;

/// RFC-0013: hidden-panel filtering. A stack with a `panel_hidden` marker
/// for a panel renders identically to the same stack with that panel's own
/// op removed entirely and no marker -- "hidden" and "absent" must produce
/// byte-identical output, since every downstream apply_* function is meant
/// to be completely unaware panel visibility exists.
#[test]
fn hidden_panel_renders_the_same_as_the_op_being_absent() {
    let hidden_stack = EditStack {
        schema_version: 1,
        ops: vec![
            serde_json::json!({ "op": "dehaze", "value": 50.0 }),
            serde_json::json!({ "op": "panel_hidden", "panel": "dehaze" }),
        ],
    };
    let absent_stack = EditStack { schema_version: 1, ops: vec![] };

    let mut hidden_image = dehaze_test_image(30, 30, [150, 150, 150], [220, 220, 220]);
    apply_edit_stack(&mut hidden_image, &effective_stack_for_render(&hidden_stack));

    let mut absent_image = dehaze_test_image(30, 30, [150, 150, 150], [220, 220, 220]);
    apply_edit_stack(&mut absent_image, &effective_stack_for_render(&absent_stack));

    assert_eq!(hidden_image, absent_image);
}

/// A stack with no `panel_hidden` markers at all is returned unchanged
/// (structurally, not just numerically -- same ops, same order).
#[test]
fn no_hidden_panels_leaves_the_stack_unchanged() {
    let stack = EditStack {
        schema_version: 1,
        ops: vec![
            serde_json::json!({ "op": "exposure", "value": 20.0 }),
            serde_json::json!({ "op": "dehaze", "value": 50.0 }),
        ],
    };
    assert_eq!(effective_stack_for_render(&stack), stack);
}

/// Hiding one panel leaves every other panel's ops completely untouched --
/// a multi-panel stack, asserting only the hidden one's own op disappears.
#[test]
fn hiding_one_panel_does_not_affect_other_panels_ops() {
    let stack = EditStack {
        schema_version: 1,
        ops: vec![
            serde_json::json!({ "op": "exposure", "value": 20.0 }),
            serde_json::json!({ "op": "dehaze", "value": 50.0 }),
            serde_json::json!({ "op": "vignette", "amount": 30.0, "midpoint": 50.0, "feather": 50.0 }),
            serde_json::json!({ "op": "panel_hidden", "panel": "dehaze" }),
        ],
    };
    let effective = effective_stack_for_render(&stack);
    let names: Vec<&str> = effective.ops.iter().filter_map(|o| o.get("op").and_then(|v| v.as_str())).collect();
    assert_eq!(names, vec!["exposure", "vignette"]);
}

/// Hiding a MULTI-op panel (Noise Reduction: luma_nr + color_nr) strips
/// both of that panel's ops, not just one -- catches a mapping that only
/// covers a panel's first op name.
#[test]
fn hiding_a_multi_op_panel_strips_every_op_it_owns() {
    let stack = EditStack {
        schema_version: 1,
        ops: vec![
            serde_json::json!({ "op": "luma_nr", "amount": 40.0, "detail": 50.0, "contrast": 0.0 }),
            serde_json::json!({ "op": "color_nr", "amount": 40.0, "detail": 50.0 }),
            serde_json::json!({ "op": "panel_hidden", "panel": "noise_reduction" }),
        ],
    };
    let effective = effective_stack_for_render(&stack);
    assert!(effective.ops.is_empty());
}

/// The `panel_hidden` marker itself never reaches apply_edit_stack (which
/// would otherwise just ignore an op name it doesn't recognize, but it
/// shouldn't be there in the first place).
#[test]
fn panel_hidden_marker_is_itself_stripped() {
    let stack = EditStack {
        schema_version: 1,
        ops: vec![serde_json::json!({ "op": "panel_hidden", "panel": "vignette" })],
    };
    assert!(effective_stack_for_render(&stack).ops.is_empty());
}
