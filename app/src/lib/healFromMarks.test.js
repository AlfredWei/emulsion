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
  it("refuses no marks and a selected object, saying why", () => {
    expect(why(planHeal([]))).toMatch(/Nothing/);
    expect(why(planHeal([/** @type {any} */ ({ kind: "segment", logits: "L", bbox: [0, 0, 1, 1], tint: "T" })]))).toMatch(/Brush and Lasso/);
    expect(why(planHeal([stroke([[0.5, 0.5]]), /** @type {any} */ ({ kind: "segment", logits: "L", bbox: [0, 0, 1, 1], tint: "T" })]))).toMatch(/Brush and Lasso/);
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
