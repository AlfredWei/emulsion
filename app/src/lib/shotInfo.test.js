import { describe, it, expect } from "vitest";
import { formatShutter, shotInfoParts } from "./shotInfo.js";

describe("shot info wording", () => {
  it("writes shutter speeds as fractions below a second", () => {
    expect(formatShutter(1 / 250)).toBe("1/250s");
    expect(formatShutter(0.5)).toBe("1/2s");
    expect(formatShutter(2)).toBe("2s");
  });

  it("lists ISO, focal length, aperture, shutter in that order", () => {
    expect(shotInfoParts({ iso: 200, focal_length: 50.4, aperture: 8, shutter_speed: 1 / 250 })).toEqual([
      "ISO 200",
      "50 mm",
      "f/8.0",
      "1/250s",
    ]);
  });

  it("leaves out what the file did not record", () => {
    expect(shotInfoParts({ iso: null, focal_length: null, aperture: 1.8, shutter_speed: null })).toEqual(["f/1.8"]);
    expect(shotInfoParts({})).toEqual([]);
    expect(shotInfoParts(null)).toEqual([]);
  });
});
