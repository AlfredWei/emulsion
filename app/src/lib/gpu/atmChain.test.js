import { describe, it, expect } from "vitest";
import { buildAtmLightChainSizes } from "./atmChain.js";

describe("buildAtmLightChainSizes", () => {
  it("divides by 8 (rounding up) each step until both dimensions reach 1", () => {
    expect(buildAtmLightChainSizes(64, 64)).toEqual([[8, 8], [1, 1]]);
    expect(buildAtmLightChainSizes(100, 20)).toEqual([[13, 3], [2, 1], [1, 1]]);
  });
  it("always yields at least one entry, even for a 1x1 source", () => {
    expect(buildAtmLightChainSizes(1, 1)).toEqual([[1, 1]]);
  });
});
