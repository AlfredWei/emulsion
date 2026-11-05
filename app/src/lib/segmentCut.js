// Reads a click-select cut (the 256 x 256 gray PNG the model's candidates carry, base64) for the Remove tool: where the
// selected object is (a box, normalised to the photograph) and a tinted copy to lay over the photograph so the user
// sees what the click took. A gray value of 128 or more is "inside", the same cut the removal job makes on the Rust side.

/** Highest gray value that is still outside (logit <= 0): the PNG is quantised so that 127.5 is zero. */
const INSIDE_FROM = 128;

/**
 * @typedef {Object} Cut
 * @property {[number, number, number, number]} bbox - x0, y0, x1, y1, normalised to the photograph
 * @property {string} tint - a PNG data URL, the object in the removal mark's colour, clear elsewhere
 * @property {number} coverage - the share of the photograph the object covers, 0..1
 */

/** @param {string} logitsPng base64 PNG @returns {Promise<Cut | null>} null when nothing is selected or the PNG cannot be read */
export async function readCut(logitsPng) {
  let img;
  try {
    img = await loadImage(`data:image/png;base64,${logitsPng}`);
  } catch {
    return null;
  }
  const n = img.naturalWidth;
  const canvas = document.createElement("canvas");
  canvas.width = n;
  canvas.height = img.naturalHeight;
  const ctx = canvas.getContext("2d");
  if (!ctx) return null;
  ctx.drawImage(img, 0, 0);
  const src = ctx.getImageData(0, 0, canvas.width, canvas.height);
  const out = ctx.createImageData(canvas.width, canvas.height);
  let x0 = Infinity, y0 = Infinity, x1 = -1, y1 = -1, count = 0;
  for (let i = 0; i < canvas.width * canvas.height; i++) {
    if (src.data[i * 4] < INSIDE_FROM) continue;
    const x = i % canvas.width, y = (i - x) / canvas.width;
    x0 = Math.min(x0, x); x1 = Math.max(x1, x);
    y0 = Math.min(y0, y); y1 = Math.max(y1, y);
    count++;
    out.data.set([255, 70, 70, 110], i * 4);
  }
  if (count === 0) return null;
  ctx.putImageData(out, 0, 0);
  return {
    bbox: [x0 / canvas.width, y0 / canvas.height, (x1 + 1) / canvas.width, (y1 + 1) / canvas.height],
    tint: canvas.toDataURL("image/png"),
    coverage: count / (canvas.width * canvas.height),
  };
}

/** @param {string} src @returns {Promise<HTMLImageElement>} */
function loadImage(src) {
  return new Promise((resolve, reject) => {
    const img = new Image();
    img.onload = () => resolve(img);
    img.onerror = () => reject(new Error("unreadable cut"));
    img.src = src;
  });
}
