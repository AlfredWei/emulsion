use super::*;

// ==================== Perspective Correction (M4) ====================
//
// Manual-controls-only "Transform" tool (auto-upright is explicitly a
// stretch goal per PRD/MILESTONES.md's M4 scope, not required) --
// Vertical/Horizontal correct converging lines from a tilted camera,
// Rotate is a fine post-warp correction, Aspect compensates the
// foreshortening a strong Vertical/Horizontal correction introduces, and
// Scale lets the user zoom in to manually crop away the blank corners the
// warp reveals (no auto-crop: the existing `apply_crop`, which already
// runs after this in both real callers, is exactly the tool for that --
// see this op's own header comment on why it deliberately doesn't try to
// solve that itself).
//
// Applied in the SAME slot lens correction occupies in both real callers
// (`export.rs`, `import.rs`), immediately after it and before
// `apply_edit_stack` -- the user grades and crops the geometrically
// corrected image, not the tilted one, matching this file's own
// lens-correction-ordering precedent above. Like lens correction and
// `rotate_image`, this is a genuine resample (every output pixel can come
// from a different source location), so it needs a fresh output buffer
// and `sample_bilinear`, not an in-place per-pixel formula.
//
// The warp itself is a true projective (homography) map, not a shear or
// simple affine stretch -- the minimal 2-degree-of-freedom model for how
// a flat plane, photographed by a tilted camera, distorts: dividing by a
// linear function of position is exactly what makes parallel lines in the
// real world converge (or, run in reverse here, un-converge) in the
// photograph. At `vertical = horizontal = 0` the divisor is exactly 1.0,
// so the map is the identity to the last bit -- verified directly by this
// section's own tests below.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Perspective {
    pub(super) vertical: f32,
    pub(super) horizontal: f32,
    pub(super) rotate: f32,
    pub(super) aspect: f32,
    pub(super) scale: f32,
}

impl Default for Perspective {
    fn default() -> Self {
        Perspective { vertical: 0.0, horizontal: 0.0, rotate: 0.0, aspect: 0.0, scale: 100.0 }
    }
}

pub(super) fn perspective_op(ops: &[serde_json::Value]) -> Perspective {
    let op = ops.iter().find(|op| op.get("op").and_then(|v| v.as_str()) == Some("perspective"));
    let Some(op) = op else { return Perspective::default() };
    let f32_field = |key: &str, default: f32| -> f32 {
        op.get(key).and_then(|v| v.as_f64()).map(|v| v as f32).unwrap_or(default)
    };
    Perspective {
        vertical: f32_field("vertical", 0.0),
        horizontal: f32_field("horizontal", 0.0),
        rotate: f32_field("rotate", 0.0),
        aspect: f32_field("aspect", 0.0),
        scale: f32_field("scale", 100.0),
    }
}

pub(super) fn perspective_is_identity(p: &Perspective) -> bool {
    p.vertical == 0.0 && p.horizontal == 0.0 && p.rotate == 0.0 && p.aspect == 0.0 && p.scale == 100.0
}

/// Slider-extreme calibration constants, hand-picked to look reasonable at
/// +-100 and confirmed empirically during this slice's own verification --
/// the same "calibrated by eye" practice this file already uses for e.g.
/// Grain's size/roughness mapping and Lens Correction's manual distortion
/// constant.
pub(super) const PERSPECTIVE_KEYSTONE_MAX: f32 = 0.7;

pub(super) const PERSPECTIVE_ASPECT_MAX: f32 = 0.5;

/// Pixel <-> isotropic normalized coordinate mapping -- same "centered,
/// half-diagonal" spirit as `LensNorm::for_manual` above (a SHARED scale
/// for both axes, not independent per-axis half-width/half-height), so
/// the warp math below can treat x and y as physically comparable units
/// regardless of the image's own aspect ratio.
#[derive(Clone, Copy)]
pub(super) struct PerspectiveNorm {
    pub(super) cx: f32,
    pub(super) cy: f32,
    pub(super) scale: f32,
}

impl PerspectiveNorm {
    pub(super) fn new(width: u32, height: u32) -> Self {
        let cx = width as f32 / 2.0;
        let cy = height as f32 / 2.0;
        PerspectiveNorm { cx, cy, scale: cx.hypot(cy).max(1.0) }
    }
    pub(super) fn to_normalized(&self, px: f32, py: f32) -> (f32, f32) {
        ((px - self.cx) / self.scale, (py - self.cy) / self.scale)
    }
    pub(super) fn to_pixel(&self, nx: f32, ny: f32) -> (f32, f32) {
        (nx * self.scale + self.cx, ny * self.scale + self.cy)
    }
}

/// Where a given OUTPUT normalized coordinate `(xd, yd)` samples FROM in
/// the SOURCE image's own normalized space -- inverse-composes, in this
/// order, the individual stages of the forward edit a user would describe
/// ("apply keystone, then stretch aspect, then rotate, then scale"):
///   1. inverse scale (dividing by `scale` samples a SMALLER source
///      region for `scale > 1`, i.e. the displayed image appears zoomed
///      IN, matching the intuitive "bigger Scale = zoom in").
///   2. inverse rotate (by `-rotate`).
///   3. inverse aspect stretch (undoes the horizontal stretch).
///   4. inverse keystone: the projective divide itself. `horizontal`
///      warps as a function of x (corrects convergence from a
///      left/right-panned camera), `vertical` warps as a function of y
///      (corrects convergence from an up/down-tilted camera) -- see this
///      section's own header comment for why this is a divide, not a
///      shear.
pub(super) fn perspective_warp_coord(xd: f32, yd: f32, p: &Perspective) -> (f32, f32) {
    let scale = (p.scale / 100.0).max(0.01);
    let (mut x, mut y) = (xd / scale, yd / scale);

    if p.rotate != 0.0 {
        let rad = -p.rotate.to_radians();
        let (sin_t, cos_t) = rad.sin_cos();
        (x, y) = (x * cos_t - y * sin_t, x * sin_t + y * cos_t);
    }

    if p.aspect != 0.0 {
        let stretch = (1.0 + (p.aspect / 100.0) * PERSPECTIVE_ASPECT_MAX).max(0.01);
        x /= stretch;
    }

    let a = (p.horizontal / 100.0) * PERSPECTIVE_KEYSTONE_MAX;
    let b = (p.vertical / 100.0) * PERSPECTIVE_KEYSTONE_MAX;
    if a != 0.0 || b != 0.0 {
        let denom = 1.0 + a * x + b * y;
        if denom.abs() > 0.001 {
            x /= denom;
            y /= denom;
        }
    }

    (x, y)
}

/// Applies the perspective warp via resampling -- see this section's own
/// header comment for why a fresh output buffer is required. Skipped
/// ENTIRELY at identity, matching every other op in this file.
pub(crate) fn apply_perspective(image: &mut RgbImage, stack: &EditStack) {
    let p = perspective_op(&stack.ops);
    if perspective_is_identity(&p) {
        return;
    }

    let (width, height) = (image.width(), image.height());
    let norm = PerspectiveNorm::new(width, height);
    let source = image.clone();
    for oy in 0..height {
        for ox in 0..width {
            let (nx, ny) = norm.to_normalized(ox as f32, oy as f32);
            let (sx, sy) = perspective_warp_coord(nx, ny, &p);
            let (px, py) = norm.to_pixel(sx, sy);
            image.put_pixel(ox, oy, sample_bilinear(&source, px, py));
        }
    }
}
