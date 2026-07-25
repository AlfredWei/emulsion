// Thin wrapper around the Tauri commands defined in app/src-tauri/src/lib.rs
// (M1 Slice 1/2) — keeps raw command-name strings out of components.

import { invoke } from "@tauri-apps/api/core";

/**
 * @typedef {Object} ImageSummary
 * @property {number} image_id
 * @property {number} version_id
 * @property {string} path
 * @property {string | null} thumbnail_path
 * @property {number} rating
 * @property {string} flag
 * @property {string} color_label
 * @property {string} added_at
 */

/**
 * @typedef {Object} ImportSummary
 * @property {number} imported
 * @property {number} skipped_duplicates
 * @property {number} failed
 */

/** @returns {Promise<ImportSummary>} */
export function importFolder(/** @type {string} */ path) {
  return invoke("import_folder", { path });
}

/** @returns {Promise<ImageSummary[]>} */
export function listImages() {
  return invoke("list_images");
}

export function setRating(/** @type {number} */ versionId, /** @type {number} */ rating) {
  return invoke("set_rating", { versionId, rating });
}

export function setFlag(/** @type {number} */ versionId, /** @type {string} */ flag) {
  return invoke("set_flag", { versionId, flag });
}

export function setColorLabel(/** @type {number} */ versionId, /** @type {string} */ colorLabel) {
  return invoke("set_color_label", { versionId, colorLabel });
}
