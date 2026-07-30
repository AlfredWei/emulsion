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

/// One global exposure/contrast/saturation application -- factored out so
/// both the global pass and each mask's local pass (below) share the exact
/// same formula, mirroring how the WGSL shader factors its own
/// `apply_adjustments` helper for the same reason (M3 Slice 5).
fn apply_adjustments(rgb: [f32; 3], exposure_ev: f32, contrast: f32, saturation: f32) -> [f32; 3] {
    let mut c = rgb;
    for v in c.iter_mut() {
        *v *= 2f32.powf(exposure_ev);
    }
    for v in c.iter_mut() {
        *v = (*v - 0.5) * (1.0 + contrast / 100.0) + 0.5;
    }
    let luma = c[0] * 0.2126 + c[1] * 0.7152 + c[2] * 0.0722;
    for v in c.iter_mut() {
        *v = luma + (*v - luma) * (1.0 + saturation / 100.0);
    }
    c
}

/// A `linear_gradient_mask` op (M3 Slice 5), parsed from its opaque JSON
/// shape -- see MaskToolStrip.svelte/develop.js for how the frontend
/// creates these. Coordinates are normalized (0..1, matching the WGSL
/// shader's own `in.uv`), so they stay valid across preview resolutions.
struct LinearGradientMask {
    start: (f32, f32),
    end: (f32, f32),
    feather: f32,
    invert: bool,
    exposure: f32,
    contrast: f32,
    saturation: f32,
}

fn parse_linear_gradient_masks(ops: &[serde_json::Value]) -> Vec<LinearGradientMask> {
    ops.iter()
        .filter(|op| op.get("op").and_then(|v| v.as_str()) == Some("linear_gradient_mask"))
        .filter_map(|op| {
            let start = op.get("start")?;
            let end = op.get("end")?;
            Some(LinearGradientMask {
                start: (
                    start.get("x")?.as_f64()? as f32,
                    start.get("y")?.as_f64()? as f32,
                ),
                end: (
                    end.get("x")?.as_f64()? as f32,
                    end.get("y")?.as_f64()? as f32,
                ),
                feather: op.get("feather").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
                invert: op.get("invert").and_then(|v| v.as_bool()).unwrap_or(false),
                exposure: op.get("exposure").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
                contrast: op.get("contrast").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
                saturation: op
                    .get("saturation")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0) as f32,
            })
        })
        .collect()
}

/// Same projection-onto-segment parametrization as the WGSL shader: `t` is
/// 0 at `start`, 1 at `end`, extrapolated linearly beyond both (then
/// clamped). `feather` widens the transition band symmetrically around the
/// midpoint -- at `feather=50`, the pins themselves move to weight 0.25/
/// 0.75 rather than staying at 0/1 -- a deliberate choice matching real
/// Lightroom's own gradient-feather model (its feather handles are
/// separate outer lines beyond the pins), not a corner-only softening.
fn mask_weight(uv: (f32, f32), mask: &LinearGradientMask) -> f32 {
    let dx = mask.end.0 - mask.start.0;
    let dy = mask.end.1 - mask.start.1;
    let len2 = (dx * dx + dy * dy).max(0.000_001);
    let t = ((uv.0 - mask.start.0) * dx + (uv.1 - mask.start.1) * dy) / len2;
    let softness = (mask.feather / 100.0).clamp(0.0, 0.999);
    let mut weight = (t + softness) / (1.0 + 2.0 * softness);
    weight = weight.clamp(0.0, 1.0);
    if mask.invert {
        weight = 1.0 - weight;
    }
    weight
}

/// Same formula as `DevelopCanvas.svelte`'s WGSL fragment shader, in the
/// same order, kept in `f32` to track the shader's precision. Not required
/// to be byte-identical to the shader's GPU output (their source
/// resolutions differ anyway -- full-res export vs. the downsampled
/// Develop preview) -- sourced from the same formula, tested against the
/// same hand-derived expected values, same tolerance the shader's own
/// numeric smoke test already uses.
///
/// Local adjustments (`linear_gradient_mask` ops) are applied AFTER the
/// global exposure/contrast/saturation pass, in stack order -- matches
/// real Lightroom's own layering (local adjustments grade on top of the
/// globally-graded image) and keeps this in the same order the WGSL
/// shader applies them.
pub(crate) fn apply_edit_stack(image: &mut RgbImage, stack: &EditStack) {
    let exposure_ev = op_value(&stack.ops, "exposure");
    let contrast = op_value(&stack.ops, "contrast");
    let saturation = op_value(&stack.ops, "saturation");
    let masks = parse_linear_gradient_masks(&stack.ops);

    let (width, height) = (image.width(), image.height());

    for (x, y, pixel) in image.enumerate_pixels_mut() {
        let mut rgb = apply_adjustments(
            [
                pixel[0] as f32 / 255.0,
                pixel[1] as f32 / 255.0,
                pixel[2] as f32 / 255.0,
            ],
            exposure_ev,
            contrast,
            saturation,
        );

        if !masks.is_empty() {
            // Pixel-center sampling, matching how a texture lookup samples
            // at the middle of a texel -- not required to be exact, same
            // "not byte-identical, tested to tolerance" bar as the rest of
            // this module.
            let uv = (
                (x as f32 + 0.5) / width as f32,
                (y as f32 + 0.5) / height as f32,
            );
            for mask in &masks {
                let weight = mask_weight(uv, mask);
                let local = apply_adjustments(rgb, mask.exposure, mask.contrast, mask.saturation);
                for c in 0..3 {
                    rgb[c] += (local[c] - rgb[c]) * weight;
                }
            }
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

    /// A 1x1 test image always samples at uv=(0.5,0.5) (pixel-center
    /// sampling) -- so these cases vary the MASK's start/end to place that
    /// fixed point at the desired relative position (before/after/at the
    /// gradient), rather than varying the image. Each expected value
    /// hand-computed precisely via script (not eyeballed) before writing
    /// the assertion.
    fn mask_stack(
        start: (f32, f32),
        end: (f32, f32),
        feather: f32,
        exposure: f32,
    ) -> EditStack {
        EditStack {
            schema_version: 1,
            ops: vec![serde_json::json!({
                "op": "linear_gradient_mask",
                "id": "test-mask",
                "start": { "x": start.0, "y": start.1 },
                "end": { "x": end.0, "y": end.1 },
                "feather": feather,
                "invert": false,
                "exposure": exposure,
                "contrast": 0.0,
                "saturation": 0.0,
            })],
        }
    }

    fn assert_mask_pixel(stack: EditStack, expected: [i32; 3]) {
        let mut image = RgbImage::from_pixel(1, 1, image::Rgb([100, 100, 100]));
        apply_edit_stack(&mut image, &stack);
        let pixel = image.get_pixel(0, 0);
        for (actual, expected) in pixel.0.iter().zip(expected.iter()) {
            assert!(
                (*actual as i32 - expected).abs() <= 2,
                "expected ~{expected:?}, got {actual} (full pixel {:?})",
                pixel.0
            );
        }
    }

    #[test]
    fn linear_gradient_mask_before_start_gets_no_local_adjustment() {
        assert_mask_pixel(mask_stack((0.6, 0.5), (0.9, 0.5), 0.0, 1.0), [100, 100, 100]);
    }

    #[test]
    fn linear_gradient_mask_after_end_gets_full_local_adjustment() {
        assert_mask_pixel(mask_stack((0.1, 0.5), (0.4, 0.5), 0.0, 1.0), [200, 200, 200]);
    }

    #[test]
    fn linear_gradient_mask_midpoint_blends_halfway() {
        assert_mask_pixel(mask_stack((0.2, 0.5), (0.8, 0.5), 0.0, 1.0), [150, 150, 150]);
    }

    /// Feather widens the transition band around the midpoint rather than
    /// only softening the corners -- at feather=50 the pixel sitting
    /// exactly AT the start point (t=0) gets weight 0.25, not 0. A
    /// deliberate choice matching real Lightroom's own feather model (see
    /// `mask_weight`'s doc comment) -- this test pins that behavior down.
    #[test]
    fn linear_gradient_mask_feather_moves_the_anchor_off_zero() {
        assert_mask_pixel(mask_stack((0.5, 0.5), (0.8, 0.5), 50.0, 1.0), [125, 125, 125]);
    }
}
