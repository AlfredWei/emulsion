import { describe, it, expect } from "vitest";
import { binHistogramPixels, computeHistogramStats } from "./histogramMath.js";

/** Builds a raw RGBA8 pixel buffer from a list of [r,g,b] triples, in the
 * given channel order, alpha fixed at 255 (unused by binHistogramPixels
 * but present in any real readback). */
function makeBuffer(/** @type {[number, number, number][]} */ pixels, /** @type {"bgra" | "rgba"} */ order) {
  const data = new Uint8Array(pixels.length * 4);
  pixels.forEach(([r, g, b], i) => {
    const o = i * 4;
    if (order === "bgra") {
      data[o] = b;
      data[o + 1] = g;
      data[o + 2] = r;
    } else {
      data[o] = r;
      data[o + 1] = g;
      data[o + 2] = b;
    }
    data[o + 3] = 255;
  });
  return data;
}

describe("binHistogramPixels", () => {
  it("counts a single pixel into the correct bucket per channel, rgba order", () => {
    const data = makeBuffer([[10, 20, 30]], "rgba");
    const { r, g, b } = binHistogramPixels(data, "rgba");
    expect(r[10]).toBe(1);
    expect(g[20]).toBe(1);
    expect(b[30]).toBe(1);
    expect(r.reduce((a, v) => a + v, 0)).toBe(1);
  });

  it("counts a single pixel into the correct bucket per channel, bgra order", () => {
    const data = makeBuffer([[10, 20, 30]], "bgra");
    const { r, g, b } = binHistogramPixels(data, "bgra");
    expect(r[10]).toBe(1);
    expect(g[20]).toBe(1);
    expect(b[30]).toBe(1);
  });

  it("misreads channels if the wrong order is passed -- pinning why the caller must get this right", () => {
    // Same bytes, decoded as the OTHER order: R and B swap. This isn't a
    // desirable behavior to design around, it's the exact failure mode
    // presentationFormat's own real bgra8unorm/rgba8unorm ambiguity would
    // cause if DevelopCanvas.svelte ever got the order wrong -- pinned
    // here so a regression there shows up as a math test failure, not
    // only as a visually-wrong histogram no test would catch.
    const data = makeBuffer([[10, 20, 30]], "rgba");
    const { r, b } = binHistogramPixels(data, "bgra");
    expect(r[30]).toBe(1);
    expect(b[10]).toBe(1);
  });

  it("accumulates multiple pixels landing in the same bucket", () => {
    const data = makeBuffer(
      [
        [100, 0, 0],
        [100, 0, 0],
        [100, 50, 0],
      ],
      "rgba",
    );
    const { r, g } = binHistogramPixels(data, "rgba");
    expect(r[100]).toBe(3);
    expect(g[0]).toBe(2);
    expect(g[50]).toBe(1);
  });

  it("every channel's total count equals the pixel count, for any order", () => {
    const pixels = /** @type {[number, number, number][]} */ (
      Array.from({ length: 50 }, (_, i) => [i % 256, (i * 3) % 256, (i * 7) % 256])
    );
    for (const order of /** @type {const} */ (["rgba", "bgra"])) {
      const data = makeBuffer(pixels, order);
      const { r, g, b } = binHistogramPixels(data, order);
      const sum = (/** @type {Uint32Array} */ arr) => arr.reduce((a, v) => a + v, 0);
      expect(sum(r)).toBe(pixels.length);
      expect(sum(g)).toBe(pixels.length);
      expect(sum(b)).toBe(pixels.length);
    }
  });

  it("returns all-zero arrays for an empty buffer", () => {
    const { r, g, b } = binHistogramPixels(new Uint8Array(0), "rgba");
    expect(r.length).toBe(256);
    expect(g.every((v) => v === 0)).toBe(true);
    expect(b.every((v) => v === 0)).toBe(true);
  });

  it("ignores the alpha byte entirely -- varying it changes nothing", () => {
    const data1 = makeBuffer([[5, 5, 5]], "rgba");
    const data2 = makeBuffer([[5, 5, 5]], "rgba");
    data2[3] = 0; // alpha byte, would be index 3 in rgba order
    const h1 = binHistogramPixels(data1, "rgba");
    const h2 = binHistogramPixels(data2, "rgba");
    expect(h1.r[5]).toBe(h2.r[5]);
  });
});

describe("computeHistogramStats", () => {
  /** @returns {{r: Uint32Array, g: Uint32Array, b: Uint32Array}} */
  function emptyData() {
    return { r: new Uint32Array(256), g: new Uint32Array(256), b: new Uint32Array(256) };
  }

  it("returns all-zero stats for an empty histogram", () => {
    expect(computeHistogramStats(emptyData())).toEqual({ min: 0, max: 0, mean: 0 });
  });

  it("reports min/max/mean for a single-bucket single-channel histogram", () => {
    const data = emptyData();
    data.r[100] = 5;
    expect(computeHistogramStats(data)).toEqual({ min: 100, max: 100, mean: 100 });
  });

  it("finds the overall min/max across all three channels, not just one", () => {
    const data = emptyData();
    data.r[10] = 1;
    data.g[200] = 1;
    data.b[128] = 1;
    const stats = computeHistogramStats(data);
    expect(stats.min).toBe(10);
    expect(stats.max).toBe(200);
  });

  it("weights the mean by count, not just distinct buckets", () => {
    const data = emptyData();
    data.r[0] = 9; // nine pixels at 0
    data.r[100] = 1; // one pixel at 100
    // mean = (0*9 + 100*1) / 10 = 10
    expect(computeHistogramStats(data).mean).toBe(10);
  });

  it("ignores empty buckets between the min and max", () => {
    const data = emptyData();
    data.r[0] = 1;
    data.r[255] = 1;
    const stats = computeHistogramStats(data);
    expect(stats.min).toBe(0);
    expect(stats.max).toBe(255);
    expect(stats.mean).toBe(127.5);
  });
});
