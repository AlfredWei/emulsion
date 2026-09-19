use super::*;

/// A `linear_gradient_mask` op (M3 Slice 5), parsed from its opaque JSON
/// shape -- see MaskToolStrip.svelte/develop.js for how the frontend
/// creates these. Coordinates are normalized (0..1, matching the WGSL
/// shader's own `in.uv`), so they stay valid across preview resolutions.
pub(super) struct LinearGradientMask {
    pub(super) start: (f32, f32),
    pub(super) end: (f32, f32),
    pub(super) feather: f32,
    pub(super) invert: bool,
    pub(super) exposure: f32,
    pub(super) contrast: f32,
    pub(super) saturation: f32,
}

pub(super) fn parse_linear_gradient_mask(op: &serde_json::Value) -> Option<LinearGradientMask> {
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
pub(super) fn mask_weight(uv: (f32, f32), mask: &LinearGradientMask) -> f32 {
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
pub(super) struct RadialGradientMask {
    pub(super) center: (f32, f32),
    pub(super) radius_x: f32,
    pub(super) radius_y: f32,
    pub(super) feather: f32,
    pub(super) invert: bool,
    pub(super) exposure: f32,
    pub(super) contrast: f32,
    pub(super) saturation: f32,
}

pub(super) fn parse_radial_gradient_mask(op: &serde_json::Value) -> Option<RadialGradientMask> {
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
pub(super) fn radial_mask_weight(uv: (f32, f32), mask: &RadialGradientMask) -> f32 {
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

/// A `red_eye_mask` op (M4): same elliptical click-drag placement as
/// `RadialGradientMask` (center/radius_x/radius_y/feather), but the effect
/// itself is redness-selective rather than a flat exposure/contrast/
/// saturation adjustment -- `pupil_size` controls how strict the red
/// detection is (see `red_eye_redness_factor`) and `darken` controls how
/// far the detected red is pulled toward gray (see `red_eye_local_color`).
/// No `invert`: the whole point of this tool is "correct what's inside the
/// oval," unlike Radial's dual vignette/spotlight use cases.
pub(super) struct RedEyeMask {
    pub(super) center: (f32, f32),
    pub(super) radius_x: f32,
    pub(super) radius_y: f32,
    pub(super) feather: f32,
    pub(super) pupil_size: f32,
    pub(super) darken: f32,
}

pub(super) fn parse_red_eye_mask(op: &serde_json::Value) -> Option<RedEyeMask> {
    let center = op.get("center")?;
    Some(RedEyeMask {
        center: (
            center.get("x")?.as_f64()? as f32,
            center.get("y")?.as_f64()? as f32,
        ),
        radius_x: op.get("radiusX")?.as_f64()? as f32,
        radius_y: op.get("radiusY")?.as_f64()? as f32,
        feather: op.get("feather").and_then(|v| v.as_f64()).unwrap_or(50.0) as f32,
        pupil_size: op
            .get("pupilSize")
            .and_then(|v| v.as_f64())
            .unwrap_or(50.0) as f32,
        darken: op.get("darken").and_then(|v| v.as_f64()).unwrap_or(50.0) as f32,
    })
}

/// Ellipse membership only (no invert) -- identical shape to
/// `radial_mask_weight`'s `inside_weight`, always applied inside since
/// red-eye correction has no outside/vignette use case.
pub(super) fn red_eye_ellipse_weight(uv: (f32, f32), mask: &RedEyeMask) -> f32 {
    let dx = (uv.0 - mask.center.0) / mask.radius_x;
    let dy = (uv.1 - mask.center.1) / mask.radius_y;
    let d = (dx * dx + dy * dy).sqrt();
    let softness = (mask.feather / 100.0).clamp(0.0, 0.999);
    let denom = (2.0 * softness).max(0.001);
    ((1.0 + softness - d) / denom).clamp(0.0, 1.0)
}

/// How strongly a pixel's own color reads as "red-eye red," 0 (not red at
/// all) to 1 (fully qualifies). `redness` is how far red sits above the
/// stronger of green/blue -- 0 for any neutral/green/blue-leaning pixel, up
/// to 1 for pure red. `pupil_size` sets the qualifying threshold: 0
/// requires near-pure red (threshold 1.0, strict), 100 accepts even a faint
/// red cast (threshold floored at 0.05, permissive) -- mirroring how a real
/// Pupil Size slider widens the correction to catch more of a larger/less-
/// saturated red-eye area.
pub(super) fn red_eye_redness_factor(rgb: [f32; 3], mask: &RedEyeMask) -> f32 {
    let redness = (rgb[0] - rgb[1].max(rgb[2])).max(0.0);
    let threshold = (1.0 - mask.pupil_size / 100.0).clamp(0.05, 1.0);
    (redness / threshold).clamp(0.0, 1.0)
}

pub(super) fn red_eye_weight(uv: (f32, f32), rgb: [f32; 3], mask: &RedEyeMask) -> f32 {
    red_eye_ellipse_weight(uv, mask) * red_eye_redness_factor(rgb, mask)
}

/// The corrected color a fully-selected red-eye pixel blends toward: pulled
/// to its own luminance (full desaturation -- a corrected pupil should read
/// as neutral gray, not just "less red") and darkened by up to 60% of that
/// luminance at `darken=100`, matching a real pupil's near-black look
/// without ever crushing to pure 0 regardless of slider position.
pub(super) fn red_eye_local_color(rgb: [f32; 3], mask: &RedEyeMask) -> [f32; 3] {
    let luma = rgb[0] * 0.2126 + rgb[1] * 0.7152 + rgb[2] * 0.0722;
    let darken_amt = (mask.darken / 100.0).clamp(0.0, 1.0);
    let target = luma * (1.0 - darken_amt * 0.6);
    [target, target, target]
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
pub(super) enum DabMode {
    Add,
    Erase,
}

pub(super) struct Dab {
    pub(super) x: f32,
    pub(super) y: f32,
    pub(super) radius: f32,
    pub(super) hardness: f32,
    pub(super) flow: f32,
    pub(super) mode: DabMode,
}

pub(super) struct BrushMask {
    pub(super) dabs: Vec<Dab>,
    pub(super) invert: bool,
    pub(super) exposure: f32,
    pub(super) contrast: f32,
    pub(super) saturation: f32,
}

pub(super) fn parse_brush_mask(op: &serde_json::Value) -> Option<BrushMask> {
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
pub(super) fn dab_falloff(uv: (f32, f32), dab: &Dab, aspect: f32) -> f32 {
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
pub(super) fn brush_mask_weight(uv: (f32, f32), mask: &BrushMask, aspect: f32) -> f32 {
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
pub(super) struct LuminanceRangeMask {
    pub(super) range_min: f32,
    pub(super) range_max: f32,
    pub(super) feather: f32,
    pub(super) invert: bool,
    pub(super) exposure: f32,
    pub(super) contrast: f32,
    pub(super) saturation: f32,
}

pub(super) fn parse_luminance_range_mask(op: &serde_json::Value) -> Option<LuminanceRangeMask> {
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
pub(super) fn luminance_mask_weight(rgb: [f32; 3], mask: &LuminanceRangeMask) -> f32 {
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

/// A `color_range_mask` op -- like luminance range, weight depends on pixel
/// VALUE (color) rather than position, but the shape is different: one
/// reference color and one tolerance, not two edges, so this is a
/// single-sided falloff (closer to `radial_mask_weight`'s inside/outside
/// shape, but in RGB-distance space instead of image space) rather than
/// luminance's two-sided `min(rising, falling)` band. `range`/`feather`
/// stored 0-100, matching every other mask kind's own scale convention.
pub(super) struct ColorRangeMask {
    pub(super) ref_color: [f32; 3],
    pub(super) range: f32,
    pub(super) feather: f32,
    pub(super) invert: bool,
    pub(super) exposure: f32,
    pub(super) contrast: f32,
    pub(super) saturation: f32,
}

pub(super) fn parse_color_range_mask(op: &serde_json::Value) -> Option<ColorRangeMask> {
    let rc = op.get("refColor")?;
    Some(ColorRangeMask {
        ref_color: [
            rc.get("r")?.as_f64()? as f32,
            rc.get("g")?.as_f64()? as f32,
            rc.get("b")?.as_f64()? as f32,
        ],
        range: op.get("range").and_then(|v| v.as_f64()).unwrap_or(25.0) as f32,
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

/// Single-sided trapezoid in RGB-distance space, same "raw expression,
/// clamp exactly once" style as `radial_mask_weight`/`luminance_mask_weight`.
/// `max_dist = sqrt(3)` is the largest possible Euclidean distance in the
/// RGB unit cube (e.g. black to white).
///
/// **A real bug caught by design review before implementation, not by
/// testing**: an earlier draft used
/// `weight = clamp((threshold + feather_width - dist) / denom, 0, 1)` --
/// algebraically tidier-looking, but wrong at `feather=0`
/// (`feather_width=0`, `denom` floors to `0.001`): at the exact sampled
/// pixel (`dist=0`), that formula evaluates to
/// `clamp((threshold + 0 - 0) / 0.001, 0, 1)`, and since the numerator's
/// `feather_width` term is what the formula relied on to guarantee full
/// weight right at the boundary, diluting it against the same `denom`
/// floor meant to prevent divide-by-zero meant the mask could select
/// LESS than the clicked pixel itself at low feather -- the single most
/// reachable "I want a strict color match" setting a user would pick,
/// silently producing an empty mask. Fixed below by removing
/// `feather_width` from the numerator entirely (`denom` is the transition
/// width added AFTER the threshold via the `+ 1.0`, not blended into it),
/// so `dist <= threshold` always yields exactly 1 no matter how small
/// `denom` is.
pub(super) fn color_mask_weight(rgb: [f32; 3], mask: &ColorRangeMask) -> f32 {
    let dx = rgb[0] - mask.ref_color[0];
    let dy = rgb[1] - mask.ref_color[1];
    let dz = rgb[2] - mask.ref_color[2];
    let dist = (dx * dx + dy * dy + dz * dz).sqrt();
    let max_dist = 3f32.sqrt();
    let threshold = (mask.range / 100.0).clamp(0.0, 1.0) * max_dist;
    let softness = (mask.feather / 100.0).clamp(0.0, 0.999);
    let feather_width = softness * max_dist * 0.5;
    let denom = feather_width.max(0.001);
    let mut weight = ((threshold - dist) / denom + 1.0).clamp(0.0, 1.0);
    if mask.invert {
        weight = 1.0 - weight;
    }
    weight
}

/// A single dab within a `spot_mask` op's painted stroke (M4 Slice 2:
/// brush-like spot removal, replacing the original single-circle model).
/// Same shape as a brush `Dab`, minus `hardness`/`flow`/`mode` -- a spot
/// dab's edge softness comes from the MASK's own single `feather` (applied
/// identically to every dab, see `spot_mask_weight`), not a per-dab
/// hardness/flow the way brush painting works, since spot removal has no
/// "build up opacity" concept.
pub(super) struct SpotDab {
    pub(super) x: f32,
    pub(super) y: f32,
    pub(super) radius: f32,
}

/// A `spot_mask` op (M4 Slice 1/2, Healing/Clone brush) -- structurally
/// unlike every mask kind above: those all GATE a parametric adjustment
/// (exposure/contrast/saturation) within a region; a spot mask instead
/// COPIES pixel CONTENT from a SOURCE offset into `dabs`, so it carries no
/// exposure/contrast/saturation/invert fields at all. `dabs` replaces the
/// original single dest-circle-plus-explicit-source-point model with a
/// painted STROKE (M4 Slice 2, per explicit user request for a brush-like
/// interaction with a movable result) -- `source_offset` is a SINGLE
/// (dx, dy) delta applied uniformly to every dab (real Photoshop clone-
/// stamp behavior: one offset for the whole stroke, not a per-dab source),
/// which is also what makes "drag to move the whole spot" trivial on the
/// frontend: translating every dab's (x, y) by the same delta leaves
/// `source_offset` correct with no recomputation needed. `heal_shift` is
/// NOT parsed from the op JSON -- it's computed once per `apply_edit_stack`
/// call from the dabs' centroid (see `compute_heal_shift`), and left at its
/// zeroed default for `mode: "clone"` masks (an unshifted sample IS a plain
/// clone).
pub(super) struct SpotMask {
    pub(super) dabs: Vec<SpotDab>,
    pub(super) feather: f32,
    pub(super) source_offset: (f32, f32),
    pub(super) heal: bool,
    pub(super) heal_shift: [f32; 3],
}

pub(super) fn parse_spot_mask(op: &serde_json::Value) -> Option<SpotMask> {
    let dabs = op
        .get("dabs")?
        .as_array()?
        .iter()
        .filter_map(|d| {
            Some(SpotDab {
                x: d.get("x")?.as_f64()? as f32,
                y: d.get("y")?.as_f64()? as f32,
                radius: d.get("radius")?.as_f64()? as f32,
            })
        })
        .collect();
    let offset = op.get("sourceOffset")?;
    Some(SpotMask {
        dabs,
        feather: op.get("feather").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
        source_offset: (
            offset.get("dx")?.as_f64()? as f32,
            offset.get("dy")?.as_f64()? as f32,
        ),
        heal: op.get("mode").and_then(|v| v.as_str()) == Some("heal"),
        heal_shift: [0.0, 0.0, 0.0],
    })
}

/// Same aspect-corrected true-circle distance as `dab_falloff`, but with
/// `radial_mask_weight`'s feather-band formula (a spot dab is a plain
/// circle, not a hardness-stepped brush dab) -- always "inside" semantics,
/// no `invert`: replacing pixel content everywhere EXCEPT a small circle
/// would never be a sensible spot-removal operation. Takes the MAX across
/// every dab in the stroke (same accumulation as `brush_mask_weight`'s own
/// `Add` dabs), one shared `feather` for the whole stroke rather than a
/// per-dab hardness.
pub(super) fn spot_mask_weight(uv: (f32, f32), mask: &SpotMask, aspect: f32) -> f32 {
    let softness = (mask.feather / 100.0).clamp(0.0, 0.999);
    let denom = (2.0 * softness).max(0.001);
    let mut weight = 0.0f32;
    for dab in &mask.dabs {
        if dab.radius <= 0.0 {
            continue;
        }
        let dx = uv.0 - dab.x;
        let dy = (uv.1 - dab.y) * aspect;
        let d = (dx * dx + dy * dy).sqrt();
        let normalized_d = d / dab.radius;
        let w = ((1.0 + softness - normalized_d) / denom).clamp(0.0, 1.0);
        weight = weight.max(w);
    }
    weight
}

/// The unweighted centroid of a spot mask's dabs, plus their average
/// radius -- used as a single representative dest/source-ring center for
/// `compute_heal_shift` (a whole-stroke heal shift is still one shared
/// value, same "simplified, not true Poisson blending" scope as before,
/// just now anchored to the stroke's centroid instead of a single dest
/// point). Returns `((0,0), 0)` for an empty stroke, a defensive case that
/// shouldn't reach here in practice (the frontend never creates a spot
/// mask with zero dabs) but avoids a divide-by-zero rather than assuming.
pub(super) fn spot_centroid_and_radius(dabs: &[SpotDab]) -> ((f32, f32), f32) {
    if dabs.is_empty() {
        return ((0.0, 0.0), 0.0);
    }
    let n = dabs.len() as f32;
    let (sx, sy, sr) = dabs.iter().fold((0.0, 0.0, 0.0), |acc, d| {
        (acc.0 + d.x, acc.1 + d.y, acc.2 + d.radius)
    });
    ((sx / n, sy / n), sr / n)
}

pub(super) const HEAL_RING_SAMPLES: usize = 24;

pub(super) const HEAL_RING_FACTOR: f32 = 1.15;

/// Average color of `HEAL_RING_SAMPLES` points on a ring just outside
/// `radius` around `center`, read from `pre_mask` (nearest-pixel, not
/// bilinear -- a small simplification consistent with this module's
/// existing "not byte-exact, tolerance-tested" bar). Points that land
/// outside `[0,1]` UV (a spot near the frame edge) are skipped rather than
/// clamped into the image -- clamping would bias the mean toward the edge
/// pixel's own color, repeated many times over. Same aspect-correction
/// inverse of `spot_mask_weight`'s own `dy * aspect`: here the angle's
/// pixel-space y-component is divided BY `aspect` to get back to
/// normalized UV.
pub(super) fn sample_ring_mean(
    center: (f32, f32),
    radius: f32,
    pre_mask: &[[f32; 3]],
    width: usize,
    height: usize,
    aspect: f32,
) -> [f32; 3] {
    let mut sum = [0.0f32; 3];
    let mut count = 0.0f32;
    let r = radius * HEAL_RING_FACTOR;
    for i in 0..HEAL_RING_SAMPLES {
        let theta = (i as f32 / HEAL_RING_SAMPLES as f32) * std::f32::consts::TAU;
        let ux = center.0 + theta.cos() * r;
        let uy = center.1 + (theta.sin() * r) / aspect;
        if !(0.0..=1.0).contains(&ux) || !(0.0..=1.0).contains(&uy) {
            continue;
        }
        let sx = ((ux * width as f32) as i64).clamp(0, width as i64 - 1) as usize;
        let sy = ((uy * height as f32) as i64).clamp(0, height as i64 - 1) as usize;
        let c = pre_mask[sy * width + sx];
        sum[0] += c[0];
        sum[1] += c[1];
        sum[2] += c[2];
        count += 1.0;
    }
    if count == 0.0 {
        return [0.0, 0.0, 0.0];
    }
    [sum[0] / count, sum[1] / count, sum[2] / count]
}

/// The simplified stand-in for true seamless (Poisson) blending this
/// slice ships with Heal mode: shift the ENTIRE sampled source patch by
/// one constant per-channel delta (destination surround mean minus source
/// surround mean), rather than smoothly varying the shift across the
/// patch. Cheap (one ring sample per spot mask, not per pixel) and fixes
/// the common case (a uniformish destination area with a different
/// average tone/color than the source) without attempting true gradient-
/// domain blending, which is out of scope for this slice.
pub(super) fn compute_heal_shift(
    m: &SpotMask,
    pre_mask: &[[f32; 3]],
    width: usize,
    height: usize,
    aspect: f32,
) -> [f32; 3] {
    let (centroid, avg_radius) = spot_centroid_and_radius(&m.dabs);
    let source_centroid = (centroid.0 + m.source_offset.0, centroid.1 + m.source_offset.1);
    let dest_mean = sample_ring_mean(centroid, avg_radius, pre_mask, width, height, aspect);
    let source_mean = sample_ring_mean(source_centroid, avg_radius, pre_mask, width, height, aspect);
    [
        dest_mean[0] - source_mean[0],
        dest_mean[1] - source_mean[1],
        dest_mean[2] - source_mean[2],
    ]
}

/// Wraps either mask kind so `parse_masks` can preserve the edit stack's
/// TRUE op order across mixed kinds -- the frontend's `masks` array (built
/// from one unfiltered pass over `stack.ops`, see `develop.js`'s
/// `listMasks`) and the WGSL shader's packed array both already do this;
/// parsing linear and radial into two separate `Vec`s and applying "all
/// linear, then all radial" would silently diverge from that order
/// whenever a user interleaves the two kinds, which is a real parity gap,
/// not just a style choice.
pub(super) enum Mask {
    Linear(LinearGradientMask),
    Radial(RadialGradientMask),
    Brush(BrushMask),
    LuminanceRange(LuminanceRangeMask),
    ColorRange(ColorRangeMask),
    Spot(SpotMask),
    RedEye(RedEyeMask),
}

impl Mask {
    /// `aspect` (image height/width) is consumed by `Brush` and `Spot`;
    /// `rgb` is only consumed by `LuminanceRange` -- the first mask kind
    /// whose weight depends on pixel VALUE, not just position. Because
    /// `rgb` is the same mutating accumulator `apply_edit_stack`'s pixel
    /// loop threads through every mask in stack order, a luminance-range
    /// mask's effective selection now depends on which masks precede it in
    /// the stack (their adjustments have already been blended into `rgb`
    /// by the time this mask's own weight is evaluated) -- the correct
    /// WYSIWYG behavior (select pixels as currently graded, matching what
    /// the user sees), not an oversight; see the parity test exercising
    /// this explicitly below.
    pub(super) fn weight(&self, uv: (f32, f32), aspect: f32, rgb: [f32; 3]) -> f32 {
        match self {
            Mask::Linear(m) => mask_weight(uv, m),
            Mask::Radial(m) => radial_mask_weight(uv, m),
            Mask::Brush(m) => brush_mask_weight(uv, m, aspect),
            Mask::LuminanceRange(m) => luminance_mask_weight(rgb, m),
            Mask::ColorRange(m) => color_mask_weight(rgb, m),
            Mask::Spot(m) => spot_mask_weight(uv, m, aspect),
            Mask::RedEye(m) => red_eye_weight(uv, rgb, m),
        }
    }

    /// The color this mask would fully replace `rgb` with at `uv`, before
    /// `weight` blends it in -- every parametric mask kind computes this
    /// via `apply_adjustments` on the CURRENT pixel's own color (unchanged
    /// from before this method existed, just renamed from the old
    /// `adjustments()` + inline `apply_adjustments` call at the one call
    /// site). `Spot` is the odd one out: its "local" color comes from
    /// sampling a DIFFERENT pixel (`source` offset from `dest`) in the
    /// fully-graded `pre_mask` buffer, not from adjusting this pixel's own
    /// `rgb` -- see `apply_edit_stack`'s own doc comment for why that
    /// requires `pre_mask` to already be complete (a two-pass split, not
    /// the single fused loop every other mask kind is compatible with).
    pub(super) fn local_color(
        &self,
        uv: (f32, f32),
        rgb: [f32; 3],
        pre_mask: &[[f32; 3]],
        width: usize,
        height: usize,
    ) -> [f32; 3] {
        match self {
            Mask::Spot(m) => {
                let sample_uv = (uv.0 + m.source_offset.0, uv.1 + m.source_offset.1);
                let sx = ((sample_uv.0 * width as f32) as i64).clamp(0, width as i64 - 1) as usize;
                let sy =
                    ((sample_uv.1 * height as f32) as i64).clamp(0, height as i64 - 1) as usize;
                let sampled = pre_mask[sy * width + sx];
                [
                    (sampled[0] + m.heal_shift[0]).clamp(0.0, 1.0),
                    (sampled[1] + m.heal_shift[1]).clamp(0.0, 1.0),
                    (sampled[2] + m.heal_shift[2]).clamp(0.0, 1.0),
                ]
            }
            Mask::RedEye(m) => red_eye_local_color(rgb, m),
            _ => {
                let (exposure, contrast, saturation) = match self {
                    Mask::Linear(m) => (m.exposure, m.contrast, m.saturation),
                    Mask::Radial(m) => (m.exposure, m.contrast, m.saturation),
                    Mask::Brush(m) => (m.exposure, m.contrast, m.saturation),
                    Mask::LuminanceRange(m) => (m.exposure, m.contrast, m.saturation),
                    Mask::ColorRange(m) => (m.exposure, m.contrast, m.saturation),
                    Mask::Spot(_) => unreachable!(),
                    Mask::RedEye(_) => unreachable!(),
                };
                apply_adjustments(rgb, exposure, contrast, saturation)
            }
        }
    }
}

pub(super) fn parse_masks(ops: &[serde_json::Value]) -> Vec<Mask> {
    ops.iter()
        .filter_map(|op| match op.get("op").and_then(|v| v.as_str()) {
            Some("linear_gradient_mask") => parse_linear_gradient_mask(op).map(Mask::Linear),
            Some("radial_gradient_mask") => parse_radial_gradient_mask(op).map(Mask::Radial),
            Some("brush_mask") => parse_brush_mask(op).map(Mask::Brush),
            Some("luminance_range_mask") => parse_luminance_range_mask(op).map(Mask::LuminanceRange),
            Some("color_range_mask") => parse_color_range_mask(op).map(Mask::ColorRange),
            Some("spot_mask") => parse_spot_mask(op).map(Mask::Spot),
            Some("red_eye_mask") => parse_red_eye_mask(op).map(Mask::RedEye),
            _ => None,
        })
        .collect()
}
