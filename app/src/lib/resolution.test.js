import { describe, it, expect } from "vitest";
import { resolutionOf, resolutionLine } from "./resolution.js";

describe("resolution wording", () => {
  it("writes size and megapixels", () => {
    expect(resolutionOf(6000, 4000)).toEqual({ size: "6000×4000", megapixels: "24 MP" });
    expect(resolutionLine(24000, 16000)).toBe("24000×16000 · 384 MP");
  });

  it("keeps one decimal below 10 MP", () => {
    expect(resolutionLine(3264, 2448)).toBe("3264×2448 · 8.0 MP");
    expect(resolutionLine(640, 480)).toBe("640×480 · 0.3 MP");
  });

  it("is empty when the size is not known", () => {
    expect(resolutionOf(null, 4000)).toBeNull();
    expect(resolutionOf(undefined, undefined)).toBeNull();
    expect(resolutionOf(0, 0)).toBeNull();
    expect(resolutionLine(null, null)).toBe("");
  });
});
