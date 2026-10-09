// IPC wrappers for AI denoise (RFC-0027 slice 3c). The commands live in denoise_commands.rs; the model runs
// in the separate `emulsion-ai` helper, so a job takes seconds (a crop preview) to minutes (a whole photo)
// and reports progress as events. A call can fail with a message the UI shows as is.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

/**
 * @typedef {Object} DenoiseModelStatus
 * @property {"ready" | "missing"} state
 * @property {number} download_bytes - bytes still to fetch (0 when ready)
 * @property {string} host - what a download would contact
 */

/**
 * The whole-photo result kept for a photo.
 * @typedef {Object} DenoiseCacheInfo
 * @property {string} model - the model id that made it
 * @property {number} width
 * @property {number} height
 * @property {number} bytes
 */

/**
 * A finished job. A crop preview also names the same rectangle of the undenoised photo and the rectangle.
 * @typedef {Object} DenoiseJobResult
 * @property {string} path - the cache entry (whole photo) or the "after" crop file (crop preview)
 * @property {number} width
 * @property {number} height
 * @property {number} tiles
 * @property {number} ms
 * @property {string | null} before_path
 * @property {{ x: number, y: number, w: number, h: number } | null} region
 */

/** Whether the denoise model file is installed and valid, and what a download costs.
 * @returns {Promise<DenoiseModelStatus>} */
export function denoiseModelStatus() {
  return invoke("denoise_model_status");
}

/** Fetches the model; call only after the user agreed (the consent dialog). Resumes a partial file.
 * @returns {Promise<DenoiseModelStatus>} */
export function denoiseDownloadModel() {
  return invoke("denoise_download_model");
}

/** Stops a running download; it then rejects with "download cancelled". A no-op otherwise.
 * @returns {Promise<void>} */
export function denoiseCancelDownload() {
  return invoke("denoise_cancel_download");
}

/** The offline path: verify and install a user-chosen model file.
 * @param {string} path @returns {Promise<DenoiseModelStatus>} */
export function denoiseImportModel(path) {
  return invoke("denoise_import_model", { path });
}

/** The whole-photo result kept for this photo, or null.
 * @param {string} contentHash @returns {Promise<DenoiseCacheInfo | null>} */
export function denoiseCacheInfo(contentHash) {
  return invoke("denoise_cache_info", { contentHash });
}

/** Runs a job and resolves when it ends. Without `cropCentre` the whole photo is denoised into the cache;
 * with it (`[x, y]`, 0..1 of the photo) only a rectangle around that point, for the before/after preview.
 * Rejects with "denoise cancelled" after `denoiseCancel`, "a denoise job is already running", or the cause.
 * @param {string} path @param {string} contentHash @param {[number, number] | null} cropCentre
 * @returns {Promise<DenoiseJobResult>} */
export function denoiseRun(path, contentHash, cropCentre = null) {
  return invoke("denoise_run", { path, contentHash, cropCentre });
}

/** Stops the running job at its next tile. A no-op when none runs.
 * @returns {Promise<void>} */
export function denoiseCancel() {
  return invoke("denoise_cancel");
}

/** Deletes this photo's kept results.
 * @param {string} contentHash @returns {Promise<void>} */
export function denoiseRemoveCache(contentHash) {
  return invoke("denoise_remove_cache", { contentHash });
}

/** The Develop canvas's source with the kept denoised copy mixed in at `amount`; null = use the plain
 * preview (amount 0, nothing kept, or not of this picture). `full` asks for the 1:1 tier.
 * @param {string} path @param {string} contentHash @param {number} amount @param {boolean} full
 * @returns {Promise<{ path: string, width: number, height: number, is_smart_preview: boolean } | null>} */
export function getDenoisedDevelopPreview(path, contentHash, amount, full) {
  return invoke("get_denoised_develop_preview", { path, contentHash, amount, full });
}

/** Model download progress events (`{downloaded, total}` bytes). Resolves to the unlisten function.
 * @param {(p: { downloaded: number, total: number }) => void} handler */
export function onDenoiseModelProgress(handler) {
  return listen("denoise-model-progress", (/** @type {{ payload: { downloaded: number, total: number } }} */ e) => handler(e.payload));
}

/** Job progress events (`{done, total}` tiles). Resolves to the unlisten function.
 * @param {(p: { done: number, total: number }) => void} handler */
export function onDenoiseProgress(handler) {
  return listen("denoise-progress", (/** @type {{ payload: { done: number, total: number } }} */ e) => handler(e.payload));
}

/** True for the rejection a cancelled model download ends with.
 * @param {unknown} err */
export function isDownloadCancelled(err) {
  return String(err).includes("download cancelled");
}

/** True for the rejection a cancelled job ends with.
 * @param {unknown} err */
export function isJobCancelled(err) {
  return String(err).includes("denoise cancelled");
}

/** Which of these photos would export without the AI Denoise their edit asks for (no kept copy).
 * @param {number[]} versionIds @returns {Promise<{ version_id: number, name: string, path: string, content_hash: string }[]>} */
export function denoiseMissingForExport(versionIds) {
  return invoke("denoise_missing_for_export", { versionIds });
}
