// "Heal instead" (RFC-0032 §16): the Remove tool's pending marks, turned into the dabs of a Spot Removal (heal) mask.
// Pure geometry, no state. A brush stroke becomes dabs along its path, a lasso outline becomes dabs covering its inside
// and its edge. A click-selected object (a model cut, not geometry) is not converted.

/** Most dabs one heal mask gets; the spacing grows to stay under it. */
export const MAX_HEAL_DABS = 600;

/**
 * @typedef {{ x: number, y: number, radius: number }} HealDab - normalised centre; `radius` is a fraction of the photograph's WIDTH (a Spot dab's convention)
 * @typedef {{ dabs: HealDab[], sourceOffset: { dx: number, dy: number } } | { reason: string }} HealPlan
 */

/** Even-odd point-in-polygon test. @param {[number, number][]} poly @param {number} x @param {number} y */
function inside(poly, x, y) {
  let hit = false;
  for (let i = 0, j = poly.length - 1; i < poly.length; j = i++) {
    const [xi, yi] = poly[i];
    const [xj, yj] = poly[j];
    if (yi > y !== yj > y && x < ((xj - xi) * (y - yi)) / (yj - yi) + xi) hit = !hit;
  }
  return hit;
}

/**
 * The dabs and the source offset that heal `shapes`, or why they cannot be.
 * Lengths are measured in units of the photograph's longer side (a brush radius is); `aspect` is width / height.
 * @param {import('$lib/api/removal.js').MaskShape[]} shapes @param {number} [aspect]
 * @returns {HealPlan}
 */
export function planHeal(shapes, aspect = 1) {
  if (shapes.length === 0) return { reason: "Nothing is marked" };
  if (shapes.some((s) => s.kind === "segment")) return { reason: "Heal works on Brush and Lasso marks, not on a selected object" };
  const wl = aspect >= 1 ? 1 : aspect; // the photograph's width, in longer-side units
  const hl = aspect >= 1 ? 1 / aspect : 1; // its height

  /** @type {HealDab[]} */
  let dabs = [];
  // Spacing multiplier: raised until the dab count fits.
  for (let scale = 1; scale < 64; scale *= 1.5) {
    dabs = [];
    for (const s of shapes) {
      if (s.kind === "stroke") dabs.push(...strokeDabs(s.points, s.radius * scale, wl, hl));
      else if (s.kind === "polygon") dabs.push(...polygonDabs(s.points, scale, wl, hl));
    }
    if (dabs.length <= MAX_HEAL_DABS) break;
  }
  if (dabs.length === 0) return { reason: "The marks are too small to heal" };

  return { dabs, sourceOffset: sourceFor(dabs, wl, hl) };
}

/** @param {[number, number][]} points @param {number} r radius in longer-side units @param {number} wl @param {number} hl */
function strokeDabs(points, r, wl, hl) {
  /** @type {HealDab[]} */
  const out = [];
  const step = Math.max(r * 0.5, 0.0015);
  const radius = r / wl;
  if (points.length === 1) return [{ x: points[0][0], y: points[0][1], radius }];
  let carried = step; // distance walked since the last dab; the first point always gets one
  for (let i = 0; i < points.length - 1; i++) {
    const [ax, ay] = points[i];
    const [bx, by] = points[i + 1];
    const len = Math.hypot((bx - ax) * wl, (by - ay) * hl);
    if (len === 0) continue;
    for (let d = step - carried; d <= len; d += step) {
      const t = d / len;
      out.push({ x: ax + (bx - ax) * t, y: ay + (by - ay) * t, radius });
      carried = len - d;
    }
    if (carried >= len) carried = Math.min(carried, step);
  }
  if (out.length === 0) out.push({ x: points[0][0], y: points[0][1], radius });
  return out;
}

/** @param {[number, number][]} poly @param {number} scale @param {number} wl @param {number} hl */
function polygonDabs(poly, scale, wl, hl) {
  if (poly.length < 3) return [];
  const xs = poly.map((p) => p[0]);
  const ys = poly.map((p) => p[1]);
  const x0 = Math.min(...xs), x1 = Math.max(...xs), y0 = Math.min(...ys), y1 = Math.max(...ys);
  const longer = Math.max((x1 - x0) * wl, (y1 - y0) * hl);
  const spacing = Math.max(longer / 10, 0.004) * scale; // longer-side units
  const r = spacing * 0.8;
  const radius = r / wl;
  /** @type {HealDab[]} */
  const out = [];
  const sx = spacing / wl, sy = spacing / hl;
  for (let y = y0 + sy / 2; y <= y1; y += sy) for (let x = x0 + sx / 2; x <= x1; x += sx) if (inside(poly, x, y)) out.push({ x, y, radius });
  // The outline, so the edge of a thin or concave shape is covered too.
  out.push(...strokeDabs([...poly, poly[0]], r, wl, hl).map((d) => ({ ...d, radius })));
  return out;
}

/** Where to copy from: beside the healed area, on the side with room (right, left, below, above), by the area's size.
 * @param {HealDab[]} dabs @param {number} wl @param {number} hl */
function sourceFor(dabs, wl, hl) {
  const mw = Math.max(...dabs.map((d) => d.radius)); // margin, in width units
  const mh = (mw * wl) / hl; // the same length in height units
  const x0 = Math.min(...dabs.map((d) => d.x)) - mw, x1 = Math.max(...dabs.map((d) => d.x)) + mw;
  const y0 = Math.min(...dabs.map((d) => d.y)) - mh, y1 = Math.max(...dabs.map((d) => d.y)) + mh;
  const gap = 0.02;
  const dx = x1 - x0 + gap, dy = y1 - y0 + gap;
  if (x1 + dx <= 1) return { dx, dy: 0 };
  if (x0 - dx >= 0) return { dx: -dx, dy: 0 };
  if (y1 + dy <= 1) return { dx: 0, dy };
  if (y0 - dy >= 0) return { dx: 0, dy: -dy };
  // No side has room for a whole copy: the roomiest side, as far as it goes.
  const room = [[1 - x1, 1, 0], [x0, -1, 0], [1 - y1, 0, 1], [y0, 0, -1]].sort((a, b) => b[0] - a[0])[0];
  return { dx: room[1] * Math.max(room[0], 0.01), dy: room[2] * Math.max(room[0], 0.01) };
}
