// Reads a gray mask PNG (base64) into bytes, for planning a Heal from it (healFromMarks.js): a click-selected object's cut
// or a finished removal's stored hole. Both are "128 or more is inside".

/**
 * @param {string} pngBase64
 * @returns {Promise<{ w: number, h: number, data: Uint8Array } | null>} null when the PNG cannot be read
 */
export async function readMaskPng(pngBase64) {
  let img;
  try {
    img = await loadImage(`data:image/png;base64,${pngBase64}`);
  } catch {
    return null;
  }
  const w = img.naturalWidth, h = img.naturalHeight;
  const canvas = document.createElement("canvas");
  canvas.width = w;
  canvas.height = h;
  const ctx = canvas.getContext("2d", { willReadFrequently: true });
  if (!ctx || w === 0 || h === 0) return null;
  ctx.drawImage(img, 0, 0);
  const rgba = ctx.getImageData(0, 0, w, h).data;
  const data = new Uint8Array(w * h);
  for (let i = 0; i < data.length; i++) data[i] = rgba[i * 4];
  return { w, h, data };
}

/** @param {string} src @returns {Promise<HTMLImageElement>} */
function loadImage(src) {
  return new Promise((resolve, reject) => {
    const img = new Image();
    img.onload = () => resolve(img);
    img.onerror = () => reject(new Error("unreadable mask"));
    img.src = src;
  });
}
