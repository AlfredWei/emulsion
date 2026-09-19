//! Fixtures shared by more than one domain's tests.

use super::*;
use serde_json::json;

pub(super) fn two_test_images(catalog: &Catalog) -> (i64, i64) {
    let a = catalog
        .add_image_with_edit_stack("/a.CR3", "hash-a", 1, &EditStack::empty(), &crate::metadata::ImageMetadata::default())
        .unwrap();
    let b = catalog
        .add_image_with_edit_stack("/b.CR3", "hash-b", 2, &EditStack::empty(), &crate::metadata::ImageMetadata::default())
        .unwrap();
    (a, b)
}

// -- M3 History/Undo/Snapshots --------------------------------------

pub(super) fn stack_with(op: &str, value: f64) -> EditStack {
    EditStack { schema_version: 1, ops: vec![json!({"op": op, "value": value})] }
}
