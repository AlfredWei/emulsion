//! The develop engine (M3 Slice 4) — the single canonical, CPU-side
//! reference implementation of edit-stack interpretation, extracted from
//! `export.rs` where it previously lived despite that module's actual
//! concern being JPEG file export, not op interpretation. Two real call
//! sites already relied on it independently before this move
//! (`export::export_one`'s full-resolution render, `import.rs`'s
//! post-edit thumbnail regeneration) — that ad hoc sharing was the actual
//! smell this module fixes.
//!
//! **This is not the only implementation of this formula, and can't be**:
//! `DevelopCanvas.svelte`'s WGSL fragment shader applies the identical
//! exposure -> contrast -> saturation math for the *interactive* Develop
//! preview, and per ADR-0004 (validated by the M0 spike — streaming
//! Rust/wgpu-rendered frames into the webview over IPC measured ~300ms/
//! frame, 3x over the <100ms interactive budget) that path is
//! architecturally required to keep running in-webview via the browser's
//! real WebGPU API. There is no way to unify the two into one executable
//! implementation without introducing native `wgpu` for a headless/
//! offscreen export render, which was considered and deliberately
//! deferred to M5 (which already owns "GPU pipeline... with correct,
//! seamless CPU fallback" as dedicated scope) rather than done here as a
//! side effect of this basic-infra cleanup slice — see ADR-0004's dated
//! update for the full reasoning.
//!
//! **The parity obligation is real but manual**: this module and
//! `DevelopCanvas.svelte`'s `fs_main` must be kept in sync by hand
//! whenever the formula changes. The test table below is the actual
//! safety net for that — broader than a single data point specifically
//! so a future change to one side has a real reference to check against,
//! not just "doesn't crash."

use crate::catalog::EditStack;
use image::RgbImage;

/// Edit-stack ops have no meaningful array order (both this and the WGSL
/// shader always apply exposure -> contrast -> saturation regardless of
/// how they're stored) -- look each one up by name, defaulting to a no-op
/// value so an image never opened in Develop still renders as a clean
/// passthrough.
fn op_value(ops: &[serde_json::Value], name: &str) -> f32 {
    ops.iter()
        .find(|op| op.get("op").and_then(|v| v.as_str()) == Some(name))
        .and_then(|op| op.get("value"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0) as f32
}

/// Same formula as `DevelopCanvas.svelte`'s WGSL fragment shader, in the
/// same order, kept in `f32` to track the shader's precision. Not required
/// to be byte-identical to the shader's GPU output (their source
/// resolutions differ anyway -- full-res export vs. the downsampled
/// Develop preview) -- sourced from the same formula, tested against the
/// same hand-derived expected values, same tolerance the shader's own
/// numeric smoke test already uses.
pub(crate) fn apply_edit_stack(image: &mut RgbImage, stack: &EditStack) {
    let exposure_ev = op_value(&stack.ops, "exposure");
    let contrast = op_value(&stack.ops, "contrast");
    let saturation = op_value(&stack.ops, "saturation");

    for pixel in image.pixels_mut() {
        let mut rgb = [
            pixel[0] as f32 / 255.0,
            pixel[1] as f32 / 255.0,
            pixel[2] as f32 / 255.0,
        ];

        for c in rgb.iter_mut() {
            *c *= 2f32.powf(exposure_ev);
        }
        for c in rgb.iter_mut() {
            *c = (*c - 0.5) * (1.0 + contrast / 100.0) + 0.5;
        }

        let luma = rgb[0] * 0.2126 + rgb[1] * 0.7152 + rgb[2] * 0.0722;
        for c in rgb.iter_mut() {
            *c = luma + (*c - luma) * (1.0 + saturation / 100.0);
        }

        for (channel, value) in pixel.0.iter_mut().zip(rgb.iter()) {
            *channel = (value.clamp(0.0, 1.0) * 255.0).round() as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stack_with(ops: &[(&str, f32)]) -> EditStack {
        EditStack {
            schema_version: 1,
            ops: ops
                .iter()
                .map(|(op, value)| serde_json::json!({ "op": op, "value": value }))
                .collect(),
        }
    }

    /// Applies `apply_edit_stack` to a single pixel and asserts against a
    /// hand-derived expected value (±2/255, matching the WGSL shader's own
    /// established tolerance in m1-slice3-smoke) -- the parity table this
    /// module's doc comment promises.
    fn assert_pixel(input: [u8; 3], ops: &[(&str, f32)], expected: [i32; 3]) {
        let mut image = RgbImage::from_pixel(1, 1, image::Rgb(input));
        apply_edit_stack(&mut image, &stack_with(ops));
        let pixel = image.get_pixel(0, 0);
        for (actual, expected) in pixel.0.iter().zip(expected.iter()) {
            assert!(
                (*actual as i32 - expected).abs() <= 2,
                "expected ~{expected:?}, got {actual} (full pixel {:?}, ops {ops:?})",
                pixel.0
            );
        }
    }

    #[test]
    fn apply_edit_stack_is_a_passthrough_with_no_ops() {
        assert_pixel([100, 120, 140], &[], [100, 120, 140]);
    }

    /// The combined case from the WGSL shader's own numeric smoke test
    /// (m1-slice3-smoke): (153,51,51) + exposure+0.5/contrast+10/
    /// saturation+30 -> (255,56,56).
    #[test]
    fn apply_edit_stack_matches_the_shaders_hand_derived_combined_value() {
        assert_pixel(
            [153, 51, 51],
            &[("exposure", 0.5), ("contrast", 10.0), ("saturation", 30.0)],
            [255, 56, 56],
        );
    }

    #[test]
    fn apply_edit_stack_pure_exposure_doubles_toward_white() {
        assert_pixel([100, 100, 100], &[("exposure", 1.0)], [200, 200, 200]);
    }

    #[test]
    fn apply_edit_stack_pure_negative_exposure_halves_toward_black() {
        assert_pixel([200, 200, 200], &[("exposure", -1.0)], [100, 100, 100]);
    }

    #[test]
    fn apply_edit_stack_pure_contrast_pushes_a_bright_pixel_brighter() {
        assert_pixel([200, 200, 200], &[("contrast", 50.0)], [236, 236, 236]);
    }

    #[test]
    fn apply_edit_stack_full_desaturation_collapses_to_luma() {
        assert_pixel([200, 100, 50], &[("saturation", -100.0)], [118, 118, 118]);
    }

    #[test]
    fn apply_edit_stack_clamps_past_white_instead_of_wrapping() {
        assert_pixel([250, 250, 250], &[("exposure", 2.0)], [255, 255, 255]);
    }
}
