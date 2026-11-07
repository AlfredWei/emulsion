// "Heal instead" (RFC-0032 §16, §18): the Remove tool's marks, or a finished removal's stored hole, turned into the dabs of
// a Spot Removal (heal) mask. Pure geometry, no state. A brush stroke becomes dabs along its path, a lasso outline becomes
// dabs covering its inside and its edge, and a raster (a click-selected object's cut, a removal's stored hole) becomes one
// dab for each grid cell it touches.

/** Most dabs one heal mask gets; the spacing grows to stay under it. */
export const MAX_HEAL_DABS = 600;

/** Gray value from which a mask pixel is inside (the cut and the stored hole both use 128 and up). */
const INSIDE_FROM = 128;

/**
 * A raster mask placed on the photograph: `box` is where it sits (x0, y0, x1, y1, normalised to the photograph), `data` one
 * byte per pixel, row by row, `w` x `h` of them; a byte of 128 or more is inside.
 * @typedef {{ box: [number, number, number, number], w: number, h: number, data: ArrayLike<number> }} MaskGrid
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
 * The dabs and the source offset that heal `shapes` and the raster `grids`, or why they cannot be.
 * Lengths are measured in units of the photograph's longer side (a brush radius is); `aspect` is width / height.
 * A `segment` shape (a click-selected object) has no geometry: its cut must come in `grids`, one for each such shape.
 * @param {import('$lib/api/removal.js').MaskShape[]} shapes @param {number} [aspect] @param {MaskGrid[]} [grids]
 * @returns {HealPlan}
 */
export function planHeal(shapes, aspect = 1, grids = []) {
  if (shapes.length === 0 && grids.length === 0) return { reason: "Nothing is marked" };
  if (shapes.filter((s) => s.kind === "segment").length > grids.length) return { reason: "The selected object's cut is missing" };
  const wl = aspect >= 1 ? 1 : aspect; // the photograph's width, in longer-side units
  const hl = aspect >= 1 ? 1 / aspect : 1; // its height
  const preparedGrids = grids.map((g) => prepareGrid(g, wl, hl));

  /** @type {HealDab[]} */
  let dabs = [];
  // Spacing multiplier: raised until the dab count fits.
  for (let scale = 1; scale < 64; scale *= 1.5) {
    dabs = [];
    for (const s of shapes) {
      if (s.kind === "stroke") dabs.push(...strokeDabs(s.points, s.radius * scale, wl, hl));
      else if (s.kind === "polygon") dabs.push(...polygonDabs(s.points, scale, wl, hl));
    }
    for (const g of preparedGrids) dabs.push(...gridDabs(g, scale, wl));
    if (dabs.length <= MAX_HEAL_DABS) break;
  }
  if (dabs.length === 0) return { reason: "The marks are too small to heal" };

  return { dabs, sourceOffset: sourceFor(dabs, wl, hl) };
}

/** What `gridDabs` needs of a grid, found once: the pixels inside, their extent, and a pixel's size in longer-side units.
 * @param {MaskGrid} g @param {number} wl @param {number} hl */
function prepareGrid(g, wl, hl) {
  const pu = ((g.box[2] - g.box[0]) * wl) / g.w;
  const pv = ((g.box[3] - g.box[1]) * hl) / g.h;
  let minx = Infinity, miny = Infinity, maxx = -1, maxy = -1;
  /** @type {number[]} */
  const inside = [];
  for (let i = 0; i < g.w * g.h; i++) {
    if (g.data[i] < INSIDE_FROM) continue;
    const x = i % g.w, y = (i - x) / g.w;
    if (x < minx) minx = x;
    if (x > maxx) maxx = x;
    if (y < miny) miny = y;
    if (y > maxy) maxy = y;
    inside.push(x, y);
  }
  return { g, pu, pv, minx, miny, maxx, maxy, inside, extent: inside.length === 0 ? 0 : Math.max((maxx - minx + 1) * pu, (maxy - miny + 1) * pv) };
}

/** One dab for each grid cell (about a tenth of the hole's longer side) the hole touches, big enough to cover the cell.
 * @param {ReturnType<typeof prepareGrid>} p @param {number} scale @param {number} wl */
function gridDabs(p, scale, wl) {
  if (p.inside.length === 0) return [];
  const spacing = Math.max(p.extent / 10, 0.004) * scale; // longer-side units
  const cw = spacing / p.pu, ch = spacing / p.pv; // a cell, in mask pixels
  const radius = (spacing * 0.8) / wl;
  /** @type {Set<number>} */
  const cells = new Set();
  for (let i = 0; i < p.inside.length; i += 2) cells.add(Math.floor((p.inside[i] - p.minx) / cw) + Math.floor((p.inside[i + 1] - p.miny) / ch) * 65536);
  const { box, w, h } = p.g;
  return [...cells].map((c) => {
    const cx = c % 65536, cy = (c - cx) / 65536;
    const px = Math.min(p.minx + (cx + 0.5) * cw, p.maxx + 1);
    const py = Math.min(p.miny + (cy + 0.5) * ch, p.maxy + 1);
    return { x: box[0] + (px / w) * (box[2] - box[0]), y: box[1] + (py / h) * (box[3] - box[1]), radius };
  });
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
