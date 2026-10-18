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

/**
 * Splits a selection into the photos Enlarge (AI) accepts and the count it leaves out (enlargements are not enlarged
 * again, RFC-0029 §3.2).
 * @param {import('$lib/api/catalog.js').ImageSummary[]} images
 * @returns {{ photos: import('$lib/api/catalog.js').ImageSummary[], skipped: number }}
 */
export function splitEnlargeable(images) {
  const photos = images.filter((i) => i.enlarged_scale == null);
  return { photos, skipped: images.length - photos.length };
}

/** "1 photo" / "5 photos". @param {number} n */
function photos(n) {
  return `${n} photo${n === 1 ? "" : "s"}`;
}

/**
 * The summary card's rows for a batch: the totals over the photos that can be enlarged to the chosen size.
 * @param {import('$lib/api/upscale.js').UpscalePlan[]} plans  one per photo that can be enlarged
 * @param {number} chosen  how many photos the dialog was opened for
 * @returns {{ label: string, value: string }[]}
 */
export function batchRows(plans, chosen) {
  const sum = (/** @type {(p: import('$lib/api/upscale.js').UpscalePlan) => number} */ f) => plans.reduce((a, p) => a + f(p), 0);
  const left = chosen - plans.length;
  return [
    { label: "Photos", value: left > 0 ? `${photos(plans.length)} (${photos(left)} cannot be enlarged to this size)` : photos(plans.length) },
    { label: "Output", value: `${formatMegapixels(sum((p) => p.megapixels))} in total` },
    { label: "Time", value: `about ${formatEstimate(sum((p) => p.seconds))} on this computer` },
    { label: "Files", value: `about ${formatSize(sum((p) => p.file_bytes))} in total` },
  ];
}

/** The warning under a batch summary when some results are very large, or "".
 * @param {import('$lib/api/upscale.js').UpscalePlan[]} plans */
export function batchVeryLargeWarning(plans) {
  const n = plans.filter((p) => p.very_large).length;
  if (n === 0) return "";
  return `${photos(n)} would come out very large (over 100 MP): each needs several GB of disk and most editors cannot open them.`;
}

/** The line under the dialog's title in a batch.
 * @param {number} count the photos that will be enlarged @param {number} skipped enlargements left out of the selection */
export function batchSourceLine(count, skipped) {
  return skipped > 0 ? `${photos(count)} (${skipped} ${skipped === 1 ? "enlargement is" : "enlargements are"} left out)` : photos(count);
}

/** The job's name in the status strip: the file, and where it is in a batch.
 * @param {string} name @param {number} index 0-based @param {number} count */
export function jobLabel(name, index, count) {
  return count > 1 ? `${name} (${index + 1} of ${count})` : name;
}

/**
 * What a batch's notification says when it ends.
 * @param {{ done: number, count: number, failed: { name: string, reason: string }[], stopped: boolean, ms: number }} r
 * @param {(ms: number) => string} formatDuration
 */
export function batchOutcome(r, formatDuration) {
  const parts = [];
  parts.push(r.stopped ? `Stopped after ${r.done} of ${r.count} photos.` : `Enlarged ${r.done} of ${photos(r.count)} in ${formatDuration(r.ms)}.`);
  if (r.failed.length > 0) {
    const first = r.failed[0];
    parts.push(`${r.failed.length} failed: ${first.name} (${first.reason})${r.failed.length > 1 ? " and more" : ""}.`);
  }
  if (r.stopped && r.done > 0) parts.push("The finished ones are in the catalog.");
  return parts.join(" ");
}
