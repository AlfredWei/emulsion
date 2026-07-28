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
 * @property {string | null} content_hash
 * @property {string | null} camera_make
 * @property {string | null} camera_model
 * @property {string | null} lens_model
 * @property {number | null} iso
 * @property {number | null} aperture
 * @property {number | null} shutter_speed
 * @property {number | null} focal_length
 * @property {string | null} captured_at
 * @property {string | null} caption
 * @property {string | null} copyright
 * @property {string | null} contact
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

/** Multi-file import (M2 Slice 1) -- the picker-dialog alternative to
 * importFolder's whole-directory walk. @returns {Promise<ImportSummary>} */
export function importFiles(/** @type {string[]} */ paths) {
  return invoke("import_files", { paths });
}

/** Backs the "Import Files…" dialog's filter list -- read at runtime
 * instead of hand-duplicating the supported-extension list into JS.
 * @returns {Promise<string[]>} */
export function getSupportedExtensions() {
  return invoke("get_supported_extensions");
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

/** IPTC (M2 Slice 2). `caption` is per-version; `copyright`/`contact` are
 * per-image -- see catalog.rs's ImageSummary doc comment for why. */
export function setCaption(/** @type {number} */ versionId, /** @type {string} */ caption) {
  return invoke("set_caption", { versionId, caption });
}

export function setCopyright(/** @type {number} */ imageId, /** @type {string} */ copyright) {
  return invoke("set_copyright", { imageId, copyright });
}

export function setContact(/** @type {number} */ imageId, /** @type {string} */ contact) {
  return invoke("set_contact", { imageId, contact });
}

/** Non-destructive removal (M2 Slice 3): catalog rows + app-owned derived
 * files only -- source files are never touched. Takes image_ids (whole-
 * image removal), not version_ids. @returns {Promise<void>} */
export function removeImages(/** @type {number[]} */ imageIds) {
  return invoke("remove_images", { imageIds });
}
