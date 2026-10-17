// How a photo's pixel size is written wherever it is shown (grid cell, Develop info bar). Pure, so the wording is
// tested without a DOM. The Library info panel keeps its own longer line ("6000 × 4000 (24.0 MP)").

/** "24 MP", "8.3 MP", "0.3 MP": whole numbers from 10 MP up, one decimal below. @param {number} pixels */
function megapixelText(pixels) {
  const mp = pixels / 1_000_000;
  return mp >= 10 ? `${Math.round(mp)} MP` : `${mp.toFixed(1)} MP`;
}

/**
 * The size of a photo as `{ size: "6000×4000", megapixels: "24 MP" }`, or null when it is not known (an import that
 * could not read it yet).
 * @param {number | null | undefined} width @param {number | null | undefined} height
 */
export function resolutionOf(width, height) {
  if (!width || !height || width <= 0 || height <= 0) return null;
  return { size: `${width}×${height}`, megapixels: megapixelText(width * height) };
}

/** One line: "6000×4000 · 24 MP", or "" when unknown.
 * @param {number | null | undefined} width @param {number | null | undefined} height */
export function resolutionLine(width, height) {
  const r = resolutionOf(width, height);
  return r ? `${r.size} · ${r.megapixels}` : "";
}
