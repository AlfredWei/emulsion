// What the Enlarge (AI) dialog says and sends (RFC-0029 §3.2): the target the form describes, the figures under it
// in words, and the default name. Pure, so the wording is tested without a DOM.

/** JPEG holds at most this many pixels on a side. */
export const MAX_SIDE = 65_535;

/**
 * @typedef {Object} EnlargeSettings
 * @property {"2" | "4" | "long_edge"} scale
 * @property {string} longEdge  the text field; only read when `scale` is "long_edge"
 * @property {number} strength  1..=100
 * @property {95 | 100} quality
 * @property {boolean} addToCollections
 * @property {boolean} metadataNote
 */

/** @returns {EnlargeSettings} */
export function defaultSettings() {
  return { scale: "4", longEdge: "", strength: 100, quality: 95, addToCollections: false, metadataNote: true };
}

/** The long-edge field as a whole number of pixels, or null when it is empty or not a number.
 * @param {string} text */
export function parseLongEdge(text) {
  const t = text.trim().replace(/,/g, "");
  if (!/^\d+$/.test(t)) return null;
  const n = Number(t);
  return n > 0 ? n : null;
}

/** The target the form describes, or null while it does not describe one (an empty long-edge field).
 * @param {EnlargeSettings} s @returns {import('$lib/api/upscale.js').UpscaleTarget | null} */
export function targetOf(s) {
  if (s.scale === "2") return { kind: "scale", value: 2 };
  if (s.scale === "4") return { kind: "scale", value: 4 };
  const px = parseLongEdge(s.longEdge);
  return px === null ? null : { kind: "long_edge", value: px };
}

/** "5 s", "42 s", "3 min", "1 h 5 min": an estimate, so it is rounded the way an estimate is read.
 * @param {number} seconds */
export function formatEstimate(seconds) {
  const s = Math.max(1, Math.round(seconds));
  if (s < 60) return `${s} s`;
  const min = Math.round(s / 60);
  if (min < 60) return `${min} min`;
  const h = Math.floor(min / 60);
  return min % 60 === 0 ? `${h} h` : `${h} h ${min % 60} min`;
}

/** "850 KB", "34 MB", "1.2 GB" (decimal units, like the rest of the app).
 * @param {number} bytes */
export function formatSize(bytes) {
  if (bytes < 1_000_000) return `${Math.max(1, Math.round(bytes / 1000))} KB`;
  if (bytes < 1_000_000_000) return `${Math.round(bytes / 1_000_000)} MB`;
  return `${(bytes / 1_000_000_000).toFixed(1)} GB`;
}

/** 1234567 -> "1,234,567". @param {number} n */
function grouped(n) {
  return String(Math.round(n)).replace(/\B(?=(\d{3})+(?!\d))/g, ",");
}

/** "24.0 MP" for a result; large ones lose the decimal. @param {number} mp */
export function formatMegapixels(mp) {
  return mp >= 100 ? `${Math.round(mp)} MP` : `${mp.toFixed(1)} MP`;
}

/**
 * The summary card's rows for a plan.
 * @param {import('$lib/api/upscale.js').UpscalePlan} plan
 * @returns {{ label: string, value: string }[]}
 */
export function planRows(plan) {
  return [
    { label: "Output", value: `${grouped(plan.out_width)} × ${grouped(plan.out_height)} px (${formatMegapixels(plan.megapixels)})` },
    { label: "Time", value: `about ${formatEstimate(plan.seconds)} on this computer` },
    { label: "File", value: `about ${formatSize(plan.file_bytes)}` },
  ];
}

/** The warning under the summary for a very large result, or "" (RFC-0029 §3.2).
 * @param {import('$lib/api/upscale.js').UpscalePlan} plan */
export function veryLargeWarning(plan) {
  if (!plan.very_large) return "";
  const gb = plan.export_gb >= 10 ? Math.round(plan.export_gb) : plan.export_gb.toFixed(1);
  return `${Math.round(plan.megapixels)} MP is very large: it needs several GB of disk and most editors cannot open it. Exporting or printing it later takes about ${gb} GB of memory.`;
}

/** The name the result gets when the name field is left empty, shown as its placeholder (the backend's rule).
 * @param {string} path the original's path @param {import('$lib/api/upscale.js').UpscaleTarget | null} target */
export function defaultStem(path, target) {
  const file = path.split(/[\\/]/).pop() ?? path;
  const dot = file.lastIndexOf(".");
  const stem = dot > 0 ? file.slice(0, dot) : file;
  if (!target) return `${stem}-Enlarged`;
  return target.kind === "scale" ? `${stem}-Enlarged-${target.value}x` : `${stem}-Enlarged-${target.value}px`;
}

/** Where the preview's region sits in the photo, as fractions for the navigator's rectangle.
 * @param {{ x: number, y: number, w: number, h: number }} region @param {number} sourceWidth @param {number} sourceHeight */
export function regionFractions(region, sourceWidth, sourceHeight) {
  if (!(sourceWidth > 0 && sourceHeight > 0)) return { left: 0, top: 0, width: 1, height: 1 };
  return { left: region.x / sourceWidth, top: region.y / sourceHeight, width: region.w / sourceWidth, height: region.h / sourceHeight };
}

/** A click in the navigator (pixel offsets inside its box) as a centre for the preview, 0..1 on both axes.
 * @param {number} offsetX @param {number} offsetY @param {number} boxWidth @param {number} boxHeight @returns {[number, number]} */
export function centreFromClick(offsetX, offsetY, boxWidth, boxHeight) {
  const clamp = (/** @type {number} */ v) => Math.min(1, Math.max(0, v));
  return boxWidth > 0 && boxHeight > 0 ? [clamp(offsetX / boxWidth), clamp(offsetY / boxHeight)] : [0.5, 0.5];
}

/** A key naming a target, so a preview made for another scale can be told apart.
 * @param {import('$lib/api/upscale.js').UpscaleTarget | null} target */
export function targetKey(target) {
  return target ? `${target.kind}:${target.value}` : "";
}

/** The directory the result is written to, for the "saved next to the original" line. @param {string} path */
export function folderOf(path) {
  const cut = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
  return cut > 0 ? path.slice(0, cut) : path;
}
