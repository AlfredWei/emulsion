import { describe, it, expect } from "vitest";
import { isProvisionalThumbnail, needsRealThumbnail } from "./thumbnails.js";

describe("thumbnail tiers", () => {
  it("-t.jpg (the camera's picture) and -s.jpg (the grid thumbnail) are provisional, nothing else is", () => {
    expect(isProvisionalThumbnail("/data/thumbnails/12-t.jpg")).toBe(true);
    expect(isProvisionalThumbnail("/data/thumbnails/12-s.jpg")).toBe(true);
    expect(isProvisionalThumbnail("/data/thumbnails/12.jpg")).toBe(false);
    expect(isProvisionalThumbnail("/data/thumbnails/12-ab12cd34.jpg")).toBe(false); // an edited picture
    expect(isProvisionalThumbnail("C:\\data\\thumbnails\\12-s.jpg")).toBe(true);
    expect(isProvisionalThumbnail(null)).toBe(false);
  });

  it("a photo needs its final thumbnail when it has none or only a provisional one", () => {
    expect(needsRealThumbnail(null)).toBe(true);
    expect(needsRealThumbnail(undefined)).toBe(true);
    expect(needsRealThumbnail("/t/1-t.jpg")).toBe(true);
    expect(needsRealThumbnail("/t/1-s.jpg")).toBe(true);
    expect(needsRealThumbnail("/t/1.jpg")).toBe(false);
  });
});
