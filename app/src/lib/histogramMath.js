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

/**
 * @typedef {Object} HistogramStats
 * @property {number} min - lowest bucket index (0..255) with any pixel, across all three channels
 * @property {number} max - highest bucket index (0..255) with any pixel, across all three channels
 * @property {number} mean - count-weighted average bucket index, across all three channels
 */

/** Summarizes a binned histogram into the tonal-range numbers the Develop
 * histogram's info readout shows: the overall min/max tonal value present
 * anywhere in the image, and the mean brightness. Deliberately combines
 * all three channels into one min/max/mean rather than three separate
 * triples -- this mirrors what the eye reads off a histogram's own
 * left/right extent (a single overall tonal range), not a per-channel
 * breakdown the UI doesn't otherwise show elsewhere.
 * @param {HistogramData} data
 * @returns {HistogramStats} */
export function computeHistogramStats(/** @type {HistogramData} */ data) {
  const channels = [data.r, data.g, data.b];
  let totalCount = 0;
  let weightedSum = 0;
  let min = 255;
  let max = 0;
  for (const counts of channels) {
    for (let i = 0; i < 256; i++) {
      const count = counts[i];
      if (count === 0) continue;
      totalCount += count;
      weightedSum += i * count;
      if (i < min) min = i;
      if (i > max) max = i;
    }
  }
  if (totalCount === 0) return { min: 0, max: 0, mean: 0 };
  return { min, max, mean: weightedSum / totalCount };
}
