import { describe, it, expect } from "vitest";
import { nudgeValue } from "./stepMath.js";

describe("nudgeValue", () => {
  it("increments by a whole-number step", () => {
    expect(nudgeValue(10, 1, 1, -100, 100)).toBe(11);
  });

  it("decrements by a whole-number step", () => {
    expect(nudgeValue(10, -1, 1, -100, 100)).toBe(9);
  });

  it("clamps at the max bound", () => {
    expect(nudgeValue(100, 1, 1, -100, 100)).toBe(100);
  });

  it("clamps at the min bound", () => {
    expect(nudgeValue(-100, -1, 1, -100, 100)).toBe(-100);
  });

  it("clamps a value that would overshoot max by more than one step", () => {
    expect(nudgeValue(99.5, 1, 1, -100, 100)).toBe(100);
  });

  // Regression: plain `0.1 + 0.05` in JS floating point is
  // 0.15000000000000002, not 0.15 -- Exposure's own step size.
  it("avoids floating-point drift on a fractional step", () => {
    expect(nudgeValue(0.1, 1, 0.05, -5, 5)).toBe(0.15);
  });

  it("rounds a fractional step to its own decimal precision (Perspective Rotate, step 0.1)", () => {
    expect(nudgeValue(1.2, 1, 0.1, -10, 10)).toBe(1.3);
  });

  it("stays within a fractional step's max bound", () => {
    expect(nudgeValue(4.98, 1, 0.05, -5, 5)).toBe(5);
  });
});
