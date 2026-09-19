// Pure geometry helpers for the Develop canvas's mask overlays. Moved verbatim out of
// DevelopCanvas.svelte; they read no component state.

/** Clips the INFINITE line through `p` in direction `dir` (need not be
 * unit length) against the [0,1]x[0,1] normalized-uv box, via
 * Liang-Barsky. Returns the clipped segment's two endpoints, or `null`
 * if the line never enters the box at all -- the correct semantic for a
 * feather boundary that's genuinely off the edge of the photo (there's
 * nothing to show), not "pick a length and hope it reaches." A fixed
 * half-length was tried first and found to be structurally wrong (not
 * just under-tuned): a boundary point outside the frame along the
 * gradient's OWN axis can never be brought back in by extending a
 * PERPENDICULAR line further, since the perpendicular offset can't
 * correct a coordinate the axis direction is orthogonal to. */
export function clipLineToUnitBox(/** @type {{x:number,y:number}} */ p, /** @type {{x:number,y:number}} */ dir) {
  let t0 = -Infinity;
  let t1 = Infinity;
  const edges = [
    { pk: -dir.x, qk: p.x }, // x >= 0
    { pk: dir.x, qk: 1 - p.x }, // x <= 1
    { pk: -dir.y, qk: p.y }, // y >= 0
    { pk: dir.y, qk: 1 - p.y }, // y <= 1
  ];
  for (const { pk, qk } of edges) {
    if (pk === 0) {
      if (qk < 0) return null; // parallel to this edge and entirely outside it
      continue;
    }
    const r = qk / pk;
    if (pk < 0) {
      t0 = Math.max(t0, r);
    } else {
      t1 = Math.min(t1, r);
    }
  }
  if (t0 > t1) return null;
  return {
    x1: p.x + t0 * dir.x,
    y1: p.y + t0 * dir.y,
    x2: p.x + t1 * dir.x,
    y2: p.y + t1 * dir.y,
  };
}

/** The two feather-boundary guide lines for a linear mask (perpendicular
 * to the gradient axis, at the weight=0 and weight=1 points) -- derived
 * directly from the SAME `t` formula the WGSL shader/develop_engine.rs
 * use (`weight = clamp((t+softness)/(1+2*softness), 0, 1)`): weight=0 at
 * t=-softness, weight=1 at t=1+softness. Returns `null` entirely when
 * feather is 0 (nothing additional to show -- the existing single axis
 * line already IS the boundary in that case), and each individual line
 * can independently be `null` if that boundary falls off-frame. */
export function linearFeatherLines(/** @type {any} */ mask) {
  if (!mask.feather || mask.feather <= 0) return null;
  const softness = Math.min(mask.feather / 100, 0.999);
  const dx = mask.end.x - mask.start.x;
  const dy = mask.end.y - mask.start.y;
  const len = Math.hypot(dx, dy) || 1;
  const perp = { x: -dy / len, y: dx / len };
  const p0 = { x: mask.start.x - softness * dx, y: mask.start.y - softness * dy };
  const p1 = { x: mask.end.x + softness * dx, y: mask.end.y + softness * dy };
  return { zero: clipLineToUnitBox(p0, perp), one: clipLineToUnitBox(p1, perp) };
}

/** The two feather-boundary ellipses for a radial mask -- derived from
 * the SAME ellipse-distance formula the shader/develop_engine.rs use:
 * insideWeight is 1 at d=(1-softness), 0 at d=(1+softness), so those are
 * exactly the ellipses at radiusX/radiusY scaled by (1-softness) and
 * (1+softness). `null` when feather is 0 -- the existing single ellipse
 * at the raw radius already IS the boundary in that case, and two
 * coincident stroked shapes would alpha-composite to a visibly heavier
 * line than one (a real regression, not just redundant markup). */
export function radialFeatherRadii(/** @type {any} */ mask) {
  if (!mask.feather || mask.feather <= 0) return null;
  const softness = Math.min(mask.feather / 100, 0.999);
  return {
    inner: { rx: mask.radiusX * (1 - softness), ry: mask.radiusY * (1 - softness) },
    outer: { rx: mask.radiusX * (1 + softness), ry: mask.radiusY * (1 + softness) },
  };
}

/** Unweighted centroid of a spot mask's dabs, plus their average radius
 * -- mirrors `develop_engine.rs`'s own `spot_centroid_and_radius`
 * exactly, since both sides need the SAME representative point (this
 * one drives the move/source-offset handle positions and the WGSL
 * heal-ring sampling via the mask-data buffer; the Rust one drives
 * export-time heal-ring sampling). */
export function spotCentroidAndRadius(/** @type {import('$lib/api/develop.js').SpotDab[]} */ dabs) {
  if (dabs.length === 0) return { x: 0, y: 0, avgRadius: 0 };
  let sx = 0;
  let sy = 0;
  let sr = 0;
  for (const d of dabs) {
    sx += d.x;
    sy += d.y;
    sr += d.radius;
  }
  return { x: sx / dabs.length, y: sy / dabs.length, avgRadius: sr / dabs.length };
}
