// IPC wrappers for click-to-select (RFC-0026 slice 2c). The commands live in segment_commands.rs;
// inference runs in the separate `emulsion-ai` helper process, so a first call can take seconds
// (process start + session load + encoder) and a call can fail with a message the UI shows as is.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

/**
 * @typedef {Object} SegmentModelStatus
 * @property {"ready" | "missing"} state
 * @property {number} download_bytes - bytes still to fetch (0 when ready)
 * @property {string} host - what a download would contact
 */

/**
 * @typedef {Object} SegmentCandidate
 * @property {number} index - the candidate's index in model order
 * @property {number} iou - the model's own quality estimate, 0..1
 * @property {string} logits_png - base64 PNG, 256x256 8-bit gray (the stored `logits` of a segment mask)
 */

/** Whether the model files are installed and valid, and what a download costs.
 * @returns {Promise<SegmentModelStatus>} */
export function segmentModelStatus() {
  return invoke("segment_model_status");
}

/** Fetches the model files; call only after the user agreed (the consent dialog).
 * @returns {Promise<SegmentModelStatus>} */
export function segmentDownloadModels() {
  return invoke("segment_download_models");
}

/** The offline path: verify and install user-chosen model files.
 * @param {string[]} paths @returns {Promise<SegmentModelStatus>} */
export function segmentImportModelFiles(paths) {
  return invoke("segment_import_model_files", { paths });
}

/** Runs the encoder for this image unless the helper still has its embedding.
 * @param {string} path @param {string} contentHash
 * @returns {Promise<{ encode_ms: number, cached: boolean }>} */
export function segmentPrepare(path, contentHash) {
  return invoke("segment_prepare", { path, contentHash });
}

/** The candidates for these prompts, best predicted IoU first. `refine` is the chosen candidate's
 * `logits_png` from the previous call. Rejects with `not_prepared...` after a helper restart.
 * @param {string} contentHash @param {import('./develop.js').SegmentPrompt[]} prompts @param {string | null} refine
 * @returns {Promise<SegmentCandidate[]>} */
export function segmentDecode(contentHash, prompts, refine = null) {
  return invoke("segment_decode", { contentHash, prompts, refine });
}

/** Frees the helper's cached embedding. Never starts the helper.
 * @returns {Promise<void>} */
export function segmentRelease() {
  return invoke("segment_release");
}

/** Download progress events (`{downloaded, total}` bytes). Resolves to the unlisten function.
 * @param {(p: { downloaded: number, total: number }) => void} handler */
export function onSegmentModelProgress(handler) {
  return listen("segment-model-progress", (/** @type {{ payload: { downloaded: number, total: number } }} */ e) => handler(e.payload));
}

/** True for the decoder's "the helper restarted, prepare again" failure.
 * @param {unknown} err */
export function isNotPrepared(err) {
  return String(err).includes("not_prepared");
}
