// Decoding a segment mask's stored logit field (RFC-0026) into something the shared raster-layer
// plumbing can draw. The stored form is a base64 8-bit grayscale PNG, 256x256, byte q = logit
// q/255*8 - 4; the GPU layer is image-sized, so the field is drawn upscaled with the canvas's
// bilinear filter and fs_mask turns the sampled byte back into a logit (develop_engine/masks.rs's
// segment_mask_weight is the CPU twin that interpolates the 256x256 field itself).

/** @type {Map<string, ImageBitmap>} base64 PNG -> decoded field; insertion-ordered, used as an LRU. */
const decoded = new Map();
/** @type {Set<string>} */
const pending = new Set();
const CACHE_LIMIT = 16; // a decoded field is 256 KB

/** @param {string} b64 @returns {Blob} */
function pngBlob(b64) {
  const bin = atob(b64);
  const bytes = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
  return new Blob([bytes], { type: "image/png" });
}

/**
 * The decoded field for `logits`, or null while it is still being decoded (decoding is
 * asynchronous; `onReady` fires once when it lands so the caller can render again). A damaged
 * PNG stays null forever and never calls `onReady`, leaving the mask's layer black (weight 0).
 * @param {string} logits base64 PNG
 * @param {() => void} onReady
 * @returns {ImageBitmap | null}
 */
export function decodedLogitField(logits, onReady) {
  const hit = decoded.get(logits);
  if (hit) {
    decoded.delete(logits);
    decoded.set(logits, hit); // most recently used
    return hit;
  }
  if (!pending.has(logits)) {
    pending.add(logits);
    // No colour management or alpha premultiplication: the bytes ARE the data.
    createImageBitmap(pngBlob(logits), { colorSpaceConversion: "none", premultiplyAlpha: "none" })
      .then((bitmap) => {
        decoded.set(logits, bitmap);
        while (decoded.size > CACHE_LIMIT) {
          const oldest = /** @type {string} */ (decoded.keys().next().value);
          decoded.get(oldest)?.close();
          decoded.delete(oldest);
        }
        onReady();
      })
      .catch(() => {})
      .finally(() => pending.delete(logits));
  }
  return null;
}

/**
 * Draws the field over the whole layer canvas (bilinear upscale).
 * @param {OffscreenCanvasRenderingContext2D} ctx
 * @param {ImageBitmap} field
 * @param {number} width @param {number} height
 */
export function drawLogitField(ctx, field, width, height) {
  ctx.globalCompositeOperation = "source-over";
  ctx.imageSmoothingEnabled = true;
  ctx.imageSmoothingQuality = "low"; // plain bilinear; "high" would add a sharper-than-linear kernel
  ctx.drawImage(field, 0, 0, width, height);
}
