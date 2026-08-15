// Develop histogram: pure, DOM/GPU-free pixel-binning math, pulled out of
// DevelopCanvas.svelte for the same reason cropMath.js's own math was --
// real, persistent test coverage instead of only ever being exercised
// through a throwaway empirical harness against the live app. See
// histogramMath.test.js.

/**
 * @typedef {Object} HistogramData
 * @property {Uint32Array} r
 * @property {Uint32Array} g
 * @property {Uint32Array} b
 */

/** Bins a readback of 8-bit-per-channel RGBA pixel data into three
 * 256-entry R/G/B count arrays. `channelOrder` is `"bgra"` or `"rgba"` --
 * WebGPU's preferred canvas format (`presentationFormat` in
 * DevelopCanvas.svelte) is commonly "bgra8unorm" on most desktop
 * Chromium, but the spec permits "rgba8unorm" too, so the caller passes
 * whichever order actually applies rather than this function assuming
 * one. Alpha (every 4th byte) is read but unused -- a histogram has
 * nothing to show for it.
 * @returns {HistogramData} */
export function binHistogramPixels(/** @type {Uint8Array} */ data, /** @type {"bgra" | "rgba"} */ channelOrder) {
  const r = new Uint32Array(256);
  const g = new Uint32Array(256);
  const b = new Uint32Array(256);
  const bgra = channelOrder === "bgra";
  for (let i = 0; i < data.length; i += 4) {
    if (bgra) {
      b[data[i]]++;
      g[data[i + 1]]++;
      r[data[i + 2]]++;
    } else {
      r[data[i]]++;
      g[data[i + 1]]++;
      b[data[i + 2]]++;
    }
  }
  return { r, g, b };
}
