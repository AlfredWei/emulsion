import { describe, it, expect } from "vitest";
import { clipLineToUnitBox, linearFeatherLines, radialFeatherRadii, spotCentroidAndRadius } from "./maskGeometry.js";

describe("clipLineToUnitBox", () => {
  it("clips a horizontal line through the middle to the full box width", () => {
    expect(clipLineToUnitBox({ x: 0.5, y: 0.5 }, { x: 1, y: 0 })).toEqual({ x1: 0, y1: 0.5, x2: 1, y2: 0.5 });
  });
  it("returns null for a line parallel to and outside an edge", () => {
    expect(clipLineToUnitBox({ x: 0.5, y: 1.5 }, { x: 1, y: 0 })).toBeNull();
  });
  it("returns null for a line that misses the box entirely", () => {
    expect(clipLineToUnitBox({ x: 2, y: 2 }, { x: 1, y: -1 })).toBeNull();
  });
});

describe("linearFeatherLines", () => {
  const mask = { start: { x: 0.25, y: 0.5 }, end: { x: 0.75, y: 0.5 } };
  it("is null when feather is zero", () => {
    expect(linearFeatherLines({ ...mask, feather: 0 })).toBeNull();
  });
  it("puts the weight=0 and weight=1 lines softness-beyond each endpoint, perpendicular to the axis", () => {
    const lines = linearFeatherLines({ ...mask, feather: 20 });
    // softness 0.2, axis length 0.5: zero-line at x = 0.25 - 0.1, one-line at x = 0.75 + 0.1
    expect(lines?.zero?.x1).toBeCloseTo(0.15);
    expect(lines?.zero?.x2).toBeCloseTo(0.15);
    expect(lines?.one?.x1).toBeCloseTo(0.85);
    expect(lines?.one?.x2).toBeCloseTo(0.85);
  });
});

describe("radialFeatherRadii", () => {
  it("is null when feather is zero", () => {
    expect(radialFeatherRadii({ feather: 0, radiusX: 0.3, radiusY: 0.2 })).toBeNull();
  });
  it("scales the radii by (1 - softness) and (1 + softness)", () => {
    const r = radialFeatherRadii({ feather: 50, radiusX: 0.4, radiusY: 0.2 });
    expect(r?.inner.rx).toBeCloseTo(0.2);
    expect(r?.outer.ry).toBeCloseTo(0.3);
  });
  it("caps softness below 1 so the inner ellipse never collapses to nothing", () => {
    const r = radialFeatherRadii({ feather: 500, radiusX: 1, radiusY: 1 });
    expect(r?.inner.rx).toBeGreaterThan(0);
  });
});

describe("spotCentroidAndRadius", () => {
  it("is the origin with zero radius for no dabs", () => {
    expect(spotCentroidAndRadius([])).toEqual({ x: 0, y: 0, avgRadius: 0 });
  });
  it("averages positions and radii (unweighted)", () => {
    const dabs = /** @type {any} */ ([{ x: 0.2, y: 0.4, radius: 0.1 }, { x: 0.6, y: 0.8, radius: 0.3 }]);
    const c = spotCentroidAndRadius(dabs);
    expect(c.x).toBeCloseTo(0.4);
    expect(c.y).toBeCloseTo(0.6);
    expect(c.avgRadius).toBeCloseTo(0.2);
  });
});
