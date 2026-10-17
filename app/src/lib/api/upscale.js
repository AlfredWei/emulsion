// IPC wrappers for AI enlargement, "Enlarge (AI)" (RFC-0029 slice 4d). The commands live in upscale_commands.rs;
// the model runs in the separate `emulsion-ai` helper, so a job takes seconds to minutes and reports progress as
// events. A call can fail with a message the UI shows as is.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

/**
 * @typedef {Object} UpscaleModelStatus
 * @property {"ready" | "missing"} state
 * @property {number} download_bytes - bytes still to fetch (0 when ready)
 * @property {string} host - what a download would contact
 */

/** How big to make the picture: a factor of 2 or 4, or the long edge in pixels.
 * @typedef {{ kind: "scale", value: 2 | 4 } | { kind: "long_edge", value: number }} UpscaleTarget */

/** What the dialog shows before anything runs (rules of thumb, not promises).
 * @typedef {Object} UpscalePlan
 * @property {number} source_width - the photo as it will be rendered (after its crop)
 * @property {number} source_height
 * @property {number} out_width
 * @property {number} out_height
 * @property {number} megapixels - of the result
 * @property {number} tiles
 * @property {number} seconds - estimated time on this computer
 * @property {number} file_bytes - rough JPEG size at the chosen quality
 * @property {boolean} very_large
 * @property {number} export_gb - what exporting or printing the result later needs
 */

/** @typedef {Object} UpscaleJobResult
 * @property {number} image_id - the new photo
 * @property {number} version_id
 * @property {string} path
 * @property {number} width
 * @property {number} height
 * @property {number} ms
 */

/** @typedef {Object} UpscaleRunOptions
 * @property {UpscaleTarget} target
 * @property {number} strength - 1..=100
 * @property {number} jpegQuality - 95 or 100
 * @property {string | null} fileName - without the extension; null = the default name
 * @property {boolean} addToCollections - put the result in the original's manual collections
 * @property {boolean} metadataNote - write "enlarged with AI" into the JPEG's own EXIF
 */

/** @typedef {Object} UpscalePreview
 * @property {string} before_path - the plain (model-free) enlargement of the region, a PNG
 * @property {string} after_path - the model's result at full strength, a JPEG
 * @property {number} width - both pictures' size, in pixels of the result
 * @property {number} height
 * @property {{ x: number, y: number, w: number, h: number }} region - the rectangle of the photo they show, in its pixels
 * @property {number} ms
 */

/** Whether the enlargement model file is installed and valid, and what a download costs.
 * @returns {Promise<UpscaleModelStatus>} */
export function upscaleModelStatus() {
  return invoke("upscale_model_status");
}

/** Fetches the model; call only after the user agreed (the consent dialog). Resumes a partial file.
 * @returns {Promise<UpscaleModelStatus>} */
export function upscaleDownloadModel() {
  return invoke("upscale_download_model");
}

/** Stops a running download; it then rejects with "download cancelled". A no-op otherwise.
 * @returns {Promise<void>} */
export function upscaleCancelDownload() {
  return invoke("upscale_cancel_download");
}

/** The offline path: verify and install a user-chosen model file.
 * @param {string} path @returns {Promise<UpscaleModelStatus>} */
export function upscaleImportModel(path) {
  return invoke("upscale_import_model", { path });
}

/** The numbers for the dialog's summary. Pure arithmetic: nothing is rendered. Rejects for an enlargement.
 * @param {number} versionId @param {UpscaleTarget} target @param {number} jpegQuality
 * @returns {Promise<UpscalePlan>} */
export function upscalePlan(versionId, target, jpegQuality) {
  return invoke("upscale_plan", { versionId, target, jpegQuality });
}

/** Enlarges a photo and adds the result to the catalog as a connected photo; resolves when it ends. Rejects with
 * "upscale cancelled" after `upscaleCancel` (nothing is written), "an enlargement is already running", or the cause.
 * @param {number} versionId @param {UpscaleRunOptions} options
 * @returns {Promise<UpscaleJobResult>} */
export function upscaleRun(versionId, { target, strength, jpegQuality, fileName, addToCollections, metadataNote }) {
  return invoke("upscale_run", { versionId, target, strength, jpegQuality, fileName, addToCollections, metadataNote });
}

/** About one megapixel of the result at 1:1 around `centre` (0..1 of the photo on both axes): the plain enlargement
 * and the model's picture, to blend on screen. Shares the job slot with `upscaleRun`; `upscaleCancel` stops it.
 * Rejects with "upscale cancelled" or the cause.
 * @param {number} versionId @param {UpscaleTarget} target @param {[number, number]} centre
 * @returns {Promise<UpscalePreview>} */
export function upscalePreview(versionId, target, centre) {
  return invoke("upscale_preview", { versionId, target, centreX: centre[0], centreY: centre[1] });
}

/** Deletes the preview pictures (the dialog calls it when it closes).
 * @returns {Promise<void>} */
export function upscalePreviewClear() {
  return invoke("upscale_preview_clear");
}

/** Stops the running job at its next tile. A no-op when none runs.
 * @returns {Promise<void>} */
export function upscaleCancel() {
  return invoke("upscale_cancel");
}

/** Model download progress events (`{downloaded, total}` bytes). Resolves to the unlisten function.
 * @param {(p: { downloaded: number, total: number }) => void} handler */
export function onUpscaleModelProgress(handler) {
  return listen("upscale-model-progress", (/** @type {{ payload: { downloaded: number, total: number } }} */ e) => handler(e.payload));
}

/** Job progress events (`{done, total}` tiles). Resolves to the unlisten function.
 * @param {(p: { done: number, total: number }) => void} handler */
export function onUpscaleProgress(handler) {
  return listen("upscale-progress", (/** @type {{ payload: { done: number, total: number } }} */ e) => handler(e.payload));
}

/** True for the rejection a cancelled model download ends with.
 * @param {unknown} err */
export function isUpscaleDownloadCancelled(err) {
  return String(err).includes("download cancelled");
}

/** True for the rejection a cancelled job ends with.
 * @param {unknown} err */
export function isUpscaleJobCancelled(err) {
  return String(err).includes("upscale cancelled");
}
