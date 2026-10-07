import { describe, expect, test, vi } from "vitest";
import { drawLogitField } from "./segmentRaster.js";

describe("segmentRaster", () => {
  test("drawLogitField stretches the field over the whole layer with bilinear smoothing, no blending", () => {
    const ctx = { drawImage: vi.fn(), globalCompositeOperation: "multiply", imageSmoothingEnabled: false, imageSmoothingQuality: "high" };
    const field = /** @type {any} */ ({ width: 256, height: 256 });
    drawLogitField(/** @type {any} */ (ctx), field, 2048, 1365);
    expect(ctx.drawImage).toHaveBeenCalledWith(field, 0, 0, 2048, 1365);
    expect(ctx.imageSmoothingEnabled).toBe(true);
    expect(ctx.imageSmoothingQuality).toBe("low");
    expect(ctx.globalCompositeOperation).toBe("source-over");
  });
});
