import { describe, it, expect } from "vitest";
import { isPlaceholderThumbnail, needsRealThumbnail } from "./thumbnails.js";

describe("thumbnail tiers", () => {
  it("a -t.jpg file is the tier-0 placeholder, nothing else is", () => {
    expect(isPlaceholderThumbnail("/data/thumbnails/12-t.jpg")).toBe(true);
    expect(isPlaceholderThumbnail("/data/thumbnails/12.jpg")).toBe(false);
    expect(isPlaceholderThumbnail("/data/thumbnails/12-ab12cd34.jpg")).toBe(false);
    expect(isPlaceholderThumbnail(null)).toBe(false);
  });

  it("a photo needs its real thumbnail when it has none or only the placeholder", () => {
    expect(needsRealThumbnail(null)).toBe(true);
    expect(needsRealThumbnail(undefined)).toBe(true);
    expect(needsRealThumbnail("/t/1-t.jpg")).toBe(true);
    expect(needsRealThumbnail("/t/1.jpg")).toBe(false);
  });
});
