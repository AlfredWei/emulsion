import { describe, it, expect } from "vitest";
import { planHeal, MAX_HEAL_DABS } from "./healFromMarks.js";

const stroke = (/** @type {[number, number][]} */ points, radius = 0.01) => /** @type {any} */ ({ kind: "stroke", points, radius });
const polygon = (/** @type {[number, number][]} */ points) => /** @type {any} */ ({ kind: "polygon", points });
/** @returns {{ dabs: import('./healFromMarks.js').HealDab[], sourceOffset: { dx: number, dy: number } }} */
const ok = (/** @type {any} */ plan) => {
  expect(plan.reason).toBeUndefined();
  return plan;
};
const why = (/** @type {import('./healFromMarks.js').HealPlan} */ plan) => /** @type {{ reason: string }} */ (plan).reason;

describe("planHeal", () => {
  it("refuses no marks and a selected object whose cut did not come, saying why", () => {
    expect(why(planHeal([]))).toMatch(/Nothing/);
    expect(why(planHeal([/** @type {any} */ ({ kind: "segment", logits: "L", bbox: [0, 0, 1, 1], tint: "T" })]))).toMatch(/cut is missing/);
    expect(why(planHeal([stroke([[0.5, 0.5]]), /** @type {any} */ ({ kind: "segment", logits: "L", bbox: [0, 0, 1, 1], tint: "T" })]))).toMatch(/cut is missing/);
  });

  it("a one-point stroke is one dab with the stroke's radius, as a fraction of the width", () => {
    const { dabs } = ok(planHeal([stroke([[0.3, 0.4]], 0.02)], 1.5)); // landscape: width is the longer side
    expect(dabs).toEqual([{ x: 0.3, y: 0.4, radius: 0.02 }]);
    const portrait = ok(planHeal([stroke([[0.3, 0.4]], 0.02)], 0.5)); // width is half the longer side
    expect(portrait.dabs[0].radius).toBeCloseTo(0.04);
  });

  it("a stroke becomes dabs along its path, closer than a radius apart", () => {
    const { dabs } = ok(planHeal([stroke([[0.1, 0.5], [0.3, 0.5]], 0.01)]));
    expect(dabs.length).toBeGreaterThan(10);
    expect(dabs[0].x).toBeCloseTo(0.1);
    expect(dabs[dabs.length - 1].x).toBeGreaterThan(0.29);
    for (let i = 1; i < dabs.length; i++) expect(dabs[i].x - dabs[i - 1].x).toBeLessThanOrEqual(0.0051);
  });

  it("a lasso is covered inside and along its edge, and nothing lands far outside it", () => {
    const { dabs } = ok(planHeal([polygon([[0.4, 0.4], [0.6, 0.4], [0.6, 0.6], [0.4, 0.6]])]));
    expect(dabs.some((d) => Math.abs(d.x - 0.5) < 0.03 && Math.abs(d.y - 0.5) < 0.03)).toBe(true); // the middle
    expect(dabs.some((d) => d.x < 0.43)).toBe(true); // the edge
    for (const d of dabs) {
      expect(d.x).toBeGreaterThanOrEqual(0.4 - 1e-9);
      expect(d.x).toBeLessThanOrEqual(0.6 + 1e-9);
      expect(d.y).toBeGreaterThanOrEqual(0.4 - 1e-9);
      expect(d.y).toBeLessThanOrEqual(0.6 + 1e-9);
    }
  });

  it("a concave lasso leaves its notch uncovered by the inside grid", () => {
    // A "U": the notch (the gap between the arms) is outside.
    const u = polygon([[0.2, 0.2], [0.8, 0.2], [0.8, 0.8], [0.6, 0.8], [0.6, 0.4], [0.4, 0.4], [0.4, 0.8], [0.2, 0.8]]);
    const { dabs } = ok(planHeal([u]));
    expect(dabs.some((d) => Math.abs(d.x - 0.5) < 0.02 && d.y > 0.55 && d.y < 0.75)).toBe(false);
  });

  it("never returns more dabs than the cap, however big the lasso", () => {
    const big = polygon([[0.01, 0.01], [0.99, 0.01], [0.99, 0.99], [0.01, 0.99]]);
    expect(ok(planHeal([big, stroke([[0.1, 0.1], [0.9, 0.9]], 0.002)])).dabs.length).toBeLessThanOrEqual(MAX_HEAL_DABS);
  });

  it("copies from beside the area, on a side with room, far enough to clear it", () => {
    const left = ok(planHeal([stroke([[0.1, 0.5], [0.15, 0.5]], 0.01)]));
    expect(left.sourceOffset.dy).toBe(0);
    expect(left.sourceOffset.dx).toBeGreaterThan(0.07); // right of a 0.05-wide stroke plus margins
    const rightEdge = ok(planHeal([stroke([[0.9, 0.5], [0.95, 0.5]], 0.01)]));
    expect(rightEdge.sourceOffset.dx).toBeLessThan(-0.07); // no room on the right: copies from the left
  });

  it("when no side has room for a whole copy it still points to the roomiest side", () => {
    const wide = ok(planHeal([polygon([[0.02, 0.02], [0.98, 0.02], [0.98, 0.98], [0.02, 0.98]])]));
    expect(Math.abs(wide.sourceOffset.dx) + Math.abs(wide.sourceOffset.dy)).toBeGreaterThan(0);
  });
});

/** A w x h mask with `inside(x, y)` marking the pixels inside. @returns {import('./healFromMarks.js').MaskGrid} */
const grid = (/** @type {[number, number, number, number]} */ box, /** @type {number} */ w, /** @type {number} */ h, /** @type {(x: number, y: number) => boolean} */ inside) => {
  const data = new Uint8Array(w * h);
  for (let y = 0; y < h; y++) for (let x = 0; x < w; x++) data[y * w + x] = inside(x, y) ? 255 : 0;
  return { box, w, h, data };
};

describe("planHeal from a raster mask", () => {
  it("an empty mask is too small to heal", () => {
    expect(why(planHeal([], 1.5, [grid([0, 0, 1, 1], 8, 8, () => false)]))).toMatch(/too small/);
  });

  it("covers the whole of a hole and puts nothing far outside it", () => {
    // A 60 x 40 pixel hole at (100, 50) of a 400 x 300 photograph: the stored-hole case.
    const box = /** @type {[number, number, number, number]} */ ([100 / 400, 50 / 300, 160 / 400, 90 / 300]);
    const { dabs } = ok(planHeal([], 400 / 300, [grid(box, 60, 40, () => true)]));
    expect(dabs.length).toBeGreaterThan(8);
    for (const d of dabs) {
      expect(d.x).toBeGreaterThan(box[0]);
      expect(d.x).toBeLessThan(box[2]);
      expect(d.y).toBeGreaterThan(box[1]);
      expect(d.y).toBeLessThan(box[3]);
    }
    // Every pixel of the hole is under some dab (radius is a fraction of the width, so y distances scale by the aspect).
    for (const [px, py] of [[100, 50], [159, 50], [100, 89], [159, 89], [130, 70]]) {
      const x = (px + 0.5) / 400, y = (py + 0.5) / 300;
      expect(dabs.some((d) => Math.hypot(x - d.x, ((y - d.y) * 300) / 400) <= d.radius)).toBe(true);
    }
  });

  it("leaves a gap in a ring where the ring is not, and covers the ring", () => {
    const ring = grid([0, 0, 1, 1], 100, 100, (x, y) => Math.hypot(x - 50, y - 50) < 40 && Math.hypot(x - 50, y - 50) > 30);
    const { dabs } = ok(planHeal([], 1, [ring]));
    expect(dabs.some((d) => Math.hypot(d.x - 0.5, d.y - 0.5) < 0.1)).toBe(false); // the middle is outside
    expect(dabs.some((d) => d.x < 0.2 && Math.abs(d.y - 0.5) < 0.1)).toBe(true); // the left of the ring
  });

  it("a click-selected object is healed from its cut, together with other marks", () => {
    const cut = grid([0, 0, 1, 1], 256, 256, (x, y) => x > 100 && x < 140 && y > 60 && y < 100);
    const segment = /** @type {any} */ ({ kind: "segment", logits: "L", bbox: [0, 0, 1, 1], tint: "T" });
    const { dabs } = ok(planHeal([segment, stroke([[0.8, 0.8]], 0.01)], 1, [cut]));
    expect(dabs.some((d) => Math.abs(d.x - 120 / 256) < 0.05 && Math.abs(d.y - 80 / 256) < 0.05)).toBe(true);
    expect(dabs.some((d) => d.x === 0.8 && d.y === 0.8)).toBe(true);
  });

  it("never returns more dabs than the cap, however big the hole", () => {
    expect(ok(planHeal([], 1, [grid([0, 0, 1, 1], 300, 300, () => true)])).dabs.length).toBeLessThanOrEqual(MAX_HEAL_DABS);
  });
});
