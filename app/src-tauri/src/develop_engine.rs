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

fn parse_linear_gradient_mask(op: &serde_json::Value) -> Option<LinearGradientMask> {
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

/// A `radial_gradient_mask` op (M3 Slice 6). `radius_x`/`radius_y` are
/// independent normalized fractions (of image width/height respectively)
/// so an on-screen-circular placement (equal *native pixel* radius on both
/// axes, computed by the frontend) round-trips correctly regardless of the
/// image's own aspect ratio.
struct RadialGradientMask {
    center: (f32, f32),
    radius_x: f32,
    radius_y: f32,
    feather: f32,
    invert: bool,
    exposure: f32,
    contrast: f32,
    saturation: f32,
}

fn parse_radial_gradient_mask(op: &serde_json::Value) -> Option<RadialGradientMask> {
    let center = op.get("center")?;
    Some(RadialGradientMask {
        center: (
            center.get("x")?.as_f64()? as f32,
            center.get("y")?.as_f64()? as f32,
        ),
        radius_x: op.get("radiusX")?.as_f64()? as f32,
        radius_y: op.get("radiusY")?.as_f64()? as f32,
        feather: op.get("feather").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
        invert: op.get("invert").and_then(|v| v.as_bool()).unwrap_or(false),
        exposure: op.get("exposure").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
        contrast: op.get("contrast").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
        saturation: op
            .get("saturation")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0) as f32,
    })
}

/// Same ellipse-distance parametrization as the WGSL shader: `d` is 0 at
/// the center, 1 at the ellipse boundary, growing beyond it outside.
/// `insideWeight` is 1 at/near the center regardless of feather; at
/// feather=0 the transition band is `d` in `[0.999, 1.0]` (width 0.001,
/// sitting just inside the boundary, not symmetric around it), widening to
/// roughly `[0.001, 1.999]` as feather approaches 100. Default
/// (`invert=false`) applies the effect OUTSIDE the ellipse -- real
/// Lightroom's own Radial Filter convention (its classic vignette use
/// case); `invert=true` applies it inside (spotlight/subject use case).
fn radial_mask_weight(uv: (f32, f32), mask: &RadialGradientMask) -> f32 {
    let dx = (uv.0 - mask.center.0) / mask.radius_x;
    let dy = (uv.1 - mask.center.1) / mask.radius_y;
    let d = (dx * dx + dy * dy).sqrt();
    let softness = (mask.feather / 100.0).clamp(0.0, 0.999);
    let denom = (2.0 * softness).max(0.001);
    let inside_weight = ((1.0 + softness - d) / denom).clamp(0.0, 1.0);
    if mask.invert {
        inside_weight
    } else {
        1.0 - inside_weight
    }
}

/// A single paint dab within a `brush_mask` op (M3 Slice 7). `radius` is a
/// normalized fraction of image WIDTH only (a single scalar, unlike
/// radial's independent `radius_x`/`radius_y`) -- the frontend rasterizes
/// dabs directly in an offscreen canvas sized to the image's own native
/// pixel resolution, where `radius * nativeWidth` used for both dimensions
/// of `ctx.arc()` is inherently a true circle with no separate axis scaling
/// needed. This CPU path has no offscreen canvas, so it reconstructs the
/// same true-circle-in-pixel-space behavior via `aspect` (height/width) --
/// see `dab_falloff`. `hardness`/`flow` are baked in per-dab at paint time
/// from whatever the brush tool's settings were when that dab was placed
/// (real Lightroom's own brush-options model), not globally editable after
/// the fact.
#[derive(Clone, Copy)]
enum DabMode {
    Add,
    Erase,
}

struct Dab {
    x: f32,
    y: f32,
    radius: f32,
    hardness: f32,
    flow: f32,
    mode: DabMode,
}

struct BrushMask {
    dabs: Vec<Dab>,
    invert: bool,
    exposure: f32,
    contrast: f32,
    saturation: f32,
}

fn parse_brush_mask(op: &serde_json::Value) -> Option<BrushMask> {
    let dabs = op
        .get("dabs")?
        .as_array()?
        .iter()
        .filter_map(|d| {
            Some(Dab {
                x: d.get("x")?.as_f64()? as f32,
                y: d.get("y")?.as_f64()? as f32,
                radius: d.get("radius")?.as_f64()? as f32,
                hardness: d.get("hardness").and_then(|v| v.as_f64()).unwrap_or(50.0) as f32,
                flow: d.get("flow").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32,
                mode: if d.get("mode").and_then(|v| v.as_str()) == Some("erase") {
                    DabMode::Erase
                } else {
                    DabMode::Add
                },
            })
        })
        .collect();
    Some(BrushMask {
        dabs,
        invert: op.get("invert").and_then(|v| v.as_bool()).unwrap_or(false),
        exposure: op.get("exposure").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
        contrast: op.get("contrast").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
        saturation: op
            .get("saturation")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0) as f32,
    })
}

/// One dab's own contribution at `uv`: 0 outside its radius, 1 within the
/// `hardness`-controlled inner stop, linearly fading to 0 at the radius
/// (matching the frontend's white-center-to-black-edge radial-gradient
/// rasterization), scaled by `flow`. `aspect` (image height/width)
/// converts the dab's width-only `radius` into a true circle in pixel
/// space: a y-distance in normalized uv space covers more actual pixels
/// than an equal x-distance whenever the image isn't square, so it must be
/// scaled up by `aspect` before comparing against a radius that's only
/// ever expressed as a fraction of width.
fn dab_falloff(uv: (f32, f32), dab: &Dab, aspect: f32) -> f32 {
    if dab.radius <= 0.0 {
        return 0.0;
    }
    let dx = uv.0 - dab.x;
    let dy = (uv.1 - dab.y) * aspect;
    let d = (dx * dx + dy * dy).sqrt();
    let normalized_d = d / dab.radius;
    if normalized_d >= 1.0 {
        return 0.0;
    }
    let hard_stop = (dab.hardness / 100.0).clamp(0.0, 1.0);
    let base = if normalized_d <= hard_stop {
        1.0
    } else {
        let denom = (1.0 - hard_stop).max(0.0001);
        (1.0 - (normalized_d - hard_stop) / denom).clamp(0.0, 1.0)
    };
    base * dab.flow.clamp(0.0, 1.0)
}

/// Dabs are accumulated in stack (paint) order, not just unioned as a set --
/// `add` dabs take the max with the running weight (matches the frontend's
/// `"lighter"` canvas compositing: overlapping add dabs build up coverage
/// but don't exceed what a single fully-opaque dab would give), `erase`
/// dabs multiplicatively reduce the running weight toward 0 (matches the
/// frontend's `"multiply"` compositing for erase) -- the same formula both
/// renderers agree on, not an approximation of one by the other.
fn brush_mask_weight(uv: (f32, f32), mask: &BrushMask, aspect: f32) -> f32 {
    let mut weight = 0.0f32;
    for dab in &mask.dabs {
        let falloff = dab_falloff(uv, dab, aspect);
        match dab.mode {
            DabMode::Add => weight = weight.max(falloff),
            DabMode::Erase => weight *= 1.0 - falloff,
        }
    }
    if mask.invert {
        1.0 - weight
    } else {
        weight
    }
}

/// A `luminance_range_mask` op -- the first mask kind whose weight depends
/// on pixel VALUE (luminance) rather than pixel POSITION (every earlier
/// kind computed weight from `uv`/`aspect` alone). `range_min`/`range_max`/
/// `feather` are stored 0-100, matching linear/radial's own `feather`
/// scale convention rather than a separate 0-1 scale -- `feather` here
/// means something different from theirs (a band WIDTH around each of two
/// edges, not a single boundary), so it's edited via a dedicated Min/Max/
/// Feather block in MaskEditorPanel.svelte, not the shared Feather row.
struct LuminanceRangeMask {
    range_min: f32,
    range_max: f32,
    feather: f32,
    invert: bool,
    exposure: f32,
    contrast: f32,
    saturation: f32,
}

fn parse_luminance_range_mask(op: &serde_json::Value) -> Option<LuminanceRangeMask> {
    Some(LuminanceRangeMask {
        range_min: op.get("rangeMin")?.as_f64()? as f32,
        range_max: op.get("rangeMax")?.as_f64()? as f32,
        feather: op.get("feather").and_then(|v| v.as_f64()).unwrap_or(20.0) as f32,
        invert: op.get("invert").and_then(|v| v.as_bool()).unwrap_or(false),
        exposure: op.get("exposure").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
        contrast: op.get("contrast").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
        saturation: op
            .get("saturation")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0) as f32,
    })
}

/// Trapezoidal falloff around `[range_min, range_max]` (both 0-100,
/// divided to 0-1 here to compare against luma), same "raw expression,
/// clamp exactly once at the end" style as `mask_weight`/`radial_mask_weight`
/// -- taking `min()` of the two UNCLAMPED slope expressions before the
/// single final clamp is what keeps this correct outside the range (a
/// naive per-term-clamped version would incorrectly clamp a
/// far-outside-range luma back up toward 1 instead of 0, since a clamped
/// "rising" term alone doesn't know it's also past the falling edge).
/// `feather=0` gives `feather_width=0`, and `denom` floors to `0.001` --
/// a near-hard step exactly at the range boundaries, matching the same
/// `0.001`-floor pattern `radial_mask_weight` already established at its
/// own `feather=0`.
fn luminance_mask_weight(rgb: [f32; 3], mask: &LuminanceRangeMask) -> f32 {
    let luma = rgb[0] * 0.2126 + rgb[1] * 0.7152 + rgb[2] * 0.0722;
    let range_min = mask.range_min / 100.0;
    let range_max = mask.range_max / 100.0;
    let softness = (mask.feather / 100.0).clamp(0.0, 0.999);
    let feather_width = softness * 0.5;
    let denom = feather_width.max(0.001);
    let rising = (luma - (range_min - feather_width)) / denom;
    let falling = (range_max + feather_width - luma) / denom;
    let mut weight = rising.min(falling).clamp(0.0, 1.0);
    if mask.invert {
        weight = 1.0 - weight;
    }
    weight
}

/// Wraps either mask kind so `parse_masks` can preserve the edit stack's
/// TRUE op order across mixed kinds -- the frontend's `masks` array (built
/// from one unfiltered pass over `stack.ops`, see `develop.js`'s
/// `listMasks`) and the WGSL shader's packed array both already do this;
/// parsing linear and radial into two separate `Vec`s and applying "all
/// linear, then all radial" would silently diverge from that order
/// whenever a user interleaves the two kinds, which is a real parity gap,
/// not just a style choice.
enum Mask {
    Linear(LinearGradientMask),
    Radial(RadialGradientMask),
    Brush(BrushMask),
    LuminanceRange(LuminanceRangeMask),
}

impl Mask {
    /// `aspect` (image height/width) is only consumed by `Brush`; `rgb` is
    /// only consumed by `LuminanceRange` -- the first mask kind whose
    /// weight depends on pixel VALUE, not just position. Because `rgb` is
    /// the same mutating accumulator `apply_edit_stack`'s pixel loop
    /// threads through every mask in stack order, a luminance-range mask's
    /// effective selection now depends on which masks precede it in the
    /// stack (their adjustments have already been blended into `rgb` by
    /// the time this mask's own weight is evaluated) -- the correct
    /// WYSIWYG behavior (select pixels as currently graded, matching what
    /// the user sees), not an oversight; see the parity test exercising
    /// this explicitly below.
    fn weight(&self, uv: (f32, f32), aspect: f32, rgb: [f32; 3]) -> f32 {
        match self {
            Mask::Linear(m) => mask_weight(uv, m),
            Mask::Radial(m) => radial_mask_weight(uv, m),
            Mask::Brush(m) => brush_mask_weight(uv, m, aspect),
            Mask::LuminanceRange(m) => luminance_mask_weight(rgb, m),
        }
    }

    fn adjustments(&self) -> (f32, f32, f32) {
        match self {
            Mask::Linear(m) => (m.exposure, m.contrast, m.saturation),
            Mask::Radial(m) => (m.exposure, m.contrast, m.saturation),
            Mask::Brush(m) => (m.exposure, m.contrast, m.saturation),
            Mask::LuminanceRange(m) => (m.exposure, m.contrast, m.saturation),
        }
    }
}

fn parse_masks(ops: &[serde_json::Value]) -> Vec<Mask> {
    ops.iter()
        .filter_map(|op| match op.get("op").and_then(|v| v.as_str()) {
            Some("linear_gradient_mask") => parse_linear_gradient_mask(op).map(Mask::Linear),
            Some("radial_gradient_mask") => parse_radial_gradient_mask(op).map(Mask::Radial),
            Some("brush_mask") => parse_brush_mask(op).map(Mask::Brush),
            Some("luminance_range_mask") => parse_luminance_range_mask(op).map(Mask::LuminanceRange),
            _ => None,
        })
        .collect()
}

/// Same formula as `DevelopCanvas.svelte`'s WGSL fragment shader, in the
/// same order, kept in `f32` to track the shader's precision. Not required
/// to be byte-identical to the shader's GPU output (their source
/// resolutions differ anyway -- full-res export vs. the downsampled
/// Develop preview) -- sourced from the same formula, tested against the
/// same hand-derived expected values, same tolerance the shader's own
/// numeric smoke test already uses.
///
/// Local adjustments (mask ops) are applied AFTER the global exposure/
/// contrast/saturation pass, in true stack order (via the unified `Mask`
/// enum -- see its own doc comment for why parsing kinds into separate
/// `Vec`s and applying "all of one kind, then all of the other" would be a
/// real parity gap once a user interleaves linear and radial masks) --
/// matches real Lightroom's own layering (local adjustments grade on top
/// of the globally-graded image) and the WGSL shader's own order.
pub(crate) fn apply_edit_stack(image: &mut RgbImage, stack: &EditStack) {
    let exposure_ev = op_value(&stack.ops, "exposure");
    let contrast = op_value(&stack.ops, "contrast");
    let saturation = op_value(&stack.ops, "saturation");
    let masks = parse_masks(&stack.ops);

    let (width, height) = (image.width(), image.height());
    // Only consumed by brush masks (see Mask::weight) -- converts a dab's
    // width-only `radius` into a true circle in pixel space regardless of
    // the image's own aspect ratio.
    let aspect = height as f32 / width as f32;

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
                let weight = mask.weight(uv, aspect, rgb);
                let (m_exposure, m_contrast, m_saturation) = mask.adjustments();
                let local = apply_adjustments(rgb, m_exposure, m_contrast, m_saturation);
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

    /// Radial gradient masks (M3 Slice 6). A 1x1 test image always samples
    /// at uv=(0.5,0.5) -- these cases vary the mask's center/radius so that
    /// fixed point lands at the desired position relative to the ellipse
    /// (center, well outside, etc). Each expected value hand-computed
    /// precisely via script (not eyeballed), same pattern as the linear
    /// cases above.
    fn radial_mask_stack(
        center: (f32, f32),
        radius_x: f32,
        radius_y: f32,
        feather: f32,
        invert: bool,
        exposure: f32,
    ) -> EditStack {
        EditStack {
            schema_version: 1,
            ops: vec![serde_json::json!({
                "op": "radial_gradient_mask",
                "id": "test-radial-mask",
                "center": { "x": center.0, "y": center.1 },
                "radiusX": radius_x,
                "radiusY": radius_y,
                "feather": feather,
                "invert": invert,
                "exposure": exposure,
                "contrast": 0.0,
                "saturation": 0.0,
            })],
        }
    }

    /// Default (invert=false) applies OUTSIDE the ellipse -- a pixel at
    /// dead center is fully "inside", so gets NO local adjustment.
    #[test]
    fn radial_gradient_mask_center_default_outside_gets_no_local_adjustment() {
        assert_mask_pixel(
            radial_mask_stack((0.5, 0.5), 0.3, 0.3, 0.0, false, 1.0),
            [100, 100, 100],
        );
    }

    /// Same geometry, invert=true (applies INSIDE) -- center gets the full
    /// local adjustment.
    #[test]
    fn radial_gradient_mask_center_inverted_gets_full_local_adjustment() {
        assert_mask_pixel(
            radial_mask_stack((0.5, 0.5), 0.3, 0.3, 0.0, true, 1.0),
            [200, 200, 200],
        );
    }

    /// Well outside the ellipse, default (outside) -- gets the full local
    /// adjustment (the opposite of the center case above).
    #[test]
    fn radial_gradient_mask_outside_default_gets_full_local_adjustment() {
        assert_mask_pixel(
            radial_mask_stack((0.1, 0.1), 0.05, 0.05, 0.0, false, 1.0),
            [200, 200, 200],
        );
    }

    /// Center is always fully "inside" regardless of feather -- a heavily
    /// feathered, inverted mask still gives the center the full effect.
    #[test]
    fn radial_gradient_mask_center_stays_fully_inside_even_when_feathered() {
        assert_mask_pixel(
            radial_mask_stack((0.5, 0.5), 0.3, 0.3, 50.0, true, 1.0),
            [200, 200, 200],
        );
    }

    /// Well outside, feathered, inverted (inside-only effect) -- stays
    /// unaffected, confirming feathering doesn't leak the inside effect
    /// arbitrarily far outward.
    #[test]
    fn radial_gradient_mask_outside_feathered_inverted_stays_unaffected() {
        assert_mask_pixel(
            radial_mask_stack((0.1, 0.1), 0.05, 0.05, 50.0, true, 1.0),
            [100, 100, 100],
        );
    }

    /// Brush masks (M3 Slice 7). A 1x1 test image always samples at
    /// uv=(0.5,0.5) -- these cases place dabs at hand-computed positions so
    /// that fixed point lands at the desired distance/falloff, same
    /// pattern as the linear/radial cases above. `aspect` is 1.0 for a 1x1
    /// image, so dab.radius (width-only) behaves as a plain circle here.
    #[allow(clippy::too_many_arguments)]
    fn dab(x: f32, y: f32, radius: f32, hardness: f32, flow: f32, mode: &str) -> serde_json::Value {
        serde_json::json!({ "x": x, "y": y, "radius": radius, "hardness": hardness, "flow": flow, "mode": mode })
    }

    fn brush_mask_stack(dabs: Vec<serde_json::Value>, invert: bool, exposure: f32) -> EditStack {
        EditStack {
            schema_version: 1,
            ops: vec![serde_json::json!({
                "op": "brush_mask",
                "id": "test-brush-mask",
                "dabs": dabs,
                "invert": invert,
                "exposure": exposure,
                "contrast": 0.0,
                "saturation": 0.0,
            })],
        }
    }

    /// A hard-edged dab (hardness=100) dead center gets the full local
    /// adjustment -- same shape as the radial "center" case.
    #[test]
    fn brush_mask_dab_at_center_gets_full_local_adjustment() {
        assert_mask_pixel(
            brush_mask_stack(vec![dab(0.5, 0.5, 0.3, 100.0, 1.0, "add")], false, 1.0),
            [200, 200, 200],
        );
    }

    /// Exactly at the dab's radius boundary -- normalized_d=1.0 is
    /// excluded (falloff=0), matching `dab_falloff`'s `>= 1.0` cutoff.
    #[test]
    fn brush_mask_dab_exactly_at_radius_boundary_gets_no_local_adjustment() {
        assert_mask_pixel(
            brush_mask_stack(vec![dab(0.2, 0.5, 0.3, 100.0, 1.0, "add")], false, 1.0),
            [100, 100, 100],
        );
    }

    /// Well outside any dab's radius -- no local adjustment.
    #[test]
    fn brush_mask_pixel_outside_every_dab_gets_no_local_adjustment() {
        assert_mask_pixel(
            brush_mask_stack(vec![dab(0.1, 0.1, 0.05, 100.0, 1.0, "add")], false, 1.0),
            [100, 100, 100],
        );
    }

    /// Two overlapping ADD dabs at the identical position/radius/hardness
    /// (hardness=0, so the fixed sample point at distance 0.15 from a
    /// radius-0.3 dab center falls exactly halfway through the falloff
    /// band, giving weight 0.5) must union via max, not sum -- two
    /// identical partial-coverage dabs still give weight 0.5, not 1.0.
    #[test]
    fn brush_mask_overlapping_add_dabs_union_via_max_not_sum() {
        assert_mask_pixel(
            brush_mask_stack(
                vec![
                    dab(0.35, 0.5, 0.3, 0.0, 1.0, "add"),
                    dab(0.35, 0.5, 0.3, 0.0, 1.0, "add"),
                ],
                false,
                1.0,
            ),
            [150, 150, 150],
        );
    }

    /// An ERASE dab reduces the running weight MULTIPLICATIVELY, not by
    /// subtraction -- an add dab giving weight 0.5, followed by an erase
    /// dab with its own falloff 0.5 at the same spot, gives weight
    /// 0.5*(1-0.5)=0.25 (multiplicative). A subtractive formula
    /// (0.5-0.5=0) would produce a visibly different pixel ([100,100,100]
    /// instead of [125,125,125]), so this test distinguishes the two.
    #[test]
    fn brush_mask_erase_dab_reduces_weight_multiplicatively() {
        assert_mask_pixel(
            brush_mask_stack(
                vec![
                    dab(0.35, 0.5, 0.3, 0.0, 1.0, "add"),
                    dab(0.35, 0.5, 0.3, 0.0, 1.0, "erase"),
                ],
                false,
                1.0,
            ),
            [125, 125, 125],
        );
    }

    /// Luminance range masks -- the first kind whose weight depends on
    /// pixel VALUE, not just uv position. Unlike the geometric masks
    /// above, the sample point (uv=(0.5,0.5) via `assert_mask_pixel`'s
    /// fixed 1x1 image) never moves -- these cases instead vary the
    /// STARTING pixel's own gray value to land its luma at the desired
    /// position relative to the range. Each expected value hand-computed
    /// precisely (not eyeballed), same discipline as every mask kind above.
    fn luminance_mask_stack(
        range_min: f32,
        range_max: f32,
        feather: f32,
        invert: bool,
        exposure: f32,
    ) -> EditStack {
        EditStack {
            schema_version: 1,
            ops: vec![serde_json::json!({
                "op": "luminance_range_mask",
                "id": "test-luminance-mask",
                "rangeMin": range_min,
                "rangeMax": range_max,
                "feather": feather,
                "invert": invert,
                "exposure": exposure,
                "contrast": 0.0,
                "saturation": 0.0,
            })],
        }
    }

    fn assert_luminance_pixel(gray: u8, stack: EditStack, expected: [i32; 3]) {
        let mut image = RgbImage::from_pixel(1, 1, image::Rgb([gray, gray, gray]));
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

    /// gray=26 -> luma~0.102, well below range [0.3,0.7] with feather=0 --
    /// no local adjustment.
    #[test]
    fn luminance_range_mask_below_range_gets_no_local_adjustment() {
        assert_luminance_pixel(26, luminance_mask_stack(30.0, 70.0, 0.0, false, 1.0), [26, 26, 26]);
    }

    /// gray=128 -> luma~0.502, inside [0.3,0.7] with feather=0 -- full
    /// local adjustment (exposure+1.0 doubles toward white, clamped).
    #[test]
    fn luminance_range_mask_inside_range_gets_full_local_adjustment() {
        assert_luminance_pixel(128, luminance_mask_stack(30.0, 70.0, 0.0, false, 1.0), [255, 255, 255]);
    }

    /// gray=230 -> luma~0.902, well above range [0.3,0.7] with feather=0 --
    /// no local adjustment.
    #[test]
    fn luminance_range_mask_above_range_gets_no_local_adjustment() {
        assert_luminance_pixel(230, luminance_mask_stack(30.0, 70.0, 0.0, false, 1.0), [230, 230, 230]);
    }

    /// gray=51 -> luma=0.2 exactly, feather=40 -> feather_width=0.2, so
    /// this sits exactly halfway through the rising edge below range_min
    /// (0.3-0.2=0.1 at weight 0, 0.3 at weight 1, 0.2 is the midpoint) --
    /// weight=0.5, half-strength local adjustment.
    #[test]
    fn luminance_range_mask_feathered_edge_blends_halfway() {
        assert_luminance_pixel(51, luminance_mask_stack(30.0, 70.0, 40.0, false, 1.0), [76, 76, 76]);
    }

    /// Order-dependency (design point 4): a linear mask with full weight
    /// at the test point doubles gray=64 (luma~0.251) to ~0.502 BEFORE the
    /// luminance-range mask (range [45,55], feather=0) evaluates its own
    /// weight against the ALREADY-DOUBLED rgb -- 0.502 falls INSIDE
    /// [0.45,0.55], so the luminance mask's own +0.5EV boost also applies,
    /// landing at ~181. If mask order didn't matter (an incorrect
    /// implementation evaluating luminance weight against the ORIGINAL,
    /// pre-linear-mask rgb=0.251, which falls OUTSIDE the range), the
    /// luminance mask would have no effect at all and the result would
    /// stop at ~128 (just the linear mask's own doubling) -- a value this
    /// test's ±2 tolerance cannot accidentally satisfy alongside 181,
    /// making this a real, discriminating test, not just a smoke check.
    #[test]
    fn luminance_range_mask_selection_depends_on_preceding_masks_effect() {
        let stack = EditStack {
            schema_version: 1,
            ops: vec![
                serde_json::json!({
                    "op": "linear_gradient_mask",
                    "id": "order-test-linear",
                    "start": { "x": 0.1, "y": 0.5 },
                    "end": { "x": 0.4, "y": 0.5 },
                    "feather": 0.0,
                    "invert": false,
                    "exposure": 1.0,
                    "contrast": 0.0,
                    "saturation": 0.0,
                }),
                serde_json::json!({
                    "op": "luminance_range_mask",
                    "id": "order-test-luminance",
                    "rangeMin": 45.0,
                    "rangeMax": 55.0,
                    "feather": 0.0,
                    "invert": false,
                    "exposure": 0.5,
                    "contrast": 0.0,
                    "saturation": 0.0,
                }),
            ],
        };
        assert_luminance_pixel(64, stack, [181, 181, 181]);
    }
}
