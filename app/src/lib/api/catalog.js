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
 * @property {number | null} [exposure_bias]
 * @property {string | null} [metering_mode]
 * @property {string | null} [flash]
 * @property {number | null} [width]
 * @property {number | null} [height]
 * @property {number | null} [latitude]
 * @property {number | null} [longitude]
 * @property {number | null} [altitude]
 * @property {number | null} [file_size]
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

/** Set or update GPS coordinates. */
export function setGeoLocation(
  /** @type {number} */ imageId,
  /** @type {number | null} */ latitude,
  /** @type {number | null} */ longitude,
  /** @type {number | null} */ altitude,
) {
  return invoke("set_geo_location", { imageId, latitude, longitude, altitude });
}

/** Non-destructive removal (M2 Slice 3): catalog rows + app-owned derived
 * files only -- source files are never touched. Takes image_ids (whole-
 * image removal), not version_ids. @returns {Promise<void>} */
export function removeImages(/** @type {number[]} */ imageIds) {
  return invoke("remove_images", { imageIds });
}

/**
 * @typedef {Object} KeywordRef
 * @property {number} id
 * @property {string} name
 * @property {string} path
 */

/**
 * @typedef {Object} KeywordNode
 * @property {number} id
 * @property {string} name
 * @property {number | null} parent_id
 */

/** Keywording (M2 Slice 4). Resolves (creating any missing level) a
 * hierarchical keyword path and assigns the leaf to every id in
 * `imageIds` -- batches across a multi-selection in one call.
 * @returns {Promise<number>} the leaf keyword id */
export function assignKeywordPath(/** @type {number[]} */ imageIds, /** @type {string[]} */ path) {
  return invoke("assign_keyword_path", { imageIds, path });
}

/** Anchor-only, matching setCaption/setCopyright/setContact's precedent. */
export function removeKeywordFromImage(/** @type {number} */ imageId, /** @type {number} */ keywordId) {
  return invoke("remove_keyword_from_image", { imageId, keywordId });
}

/** @returns {Promise<KeywordRef[]>} */
export function getImageKeywords(/** @type {number} */ imageId) {
  return invoke("get_image_keywords", { imageId });
}

/** Backs the assignment input's autocomplete suggestions.
 * @returns {Promise<KeywordNode[]>} */
export function listKeywords() {
  return invoke("list_keywords");
}

/**
 * @typedef {Object} ImageKeywordAssignment
 * @property {number} image_id
 * @property {number} keyword_id
 */

/** Backs Smart Collections' "has keyword"/"untagged" rules -- fetched
 * once, flat, independent of listImages(). @returns {Promise<ImageKeywordAssignment[]>} */
export function listAllImageKeywords() {
  return invoke("list_all_image_keywords");
}

/**
 * A Smart Collection rule, opaque to the backend (M2 Slice 5) -- evaluated
 * entirely client-side, see collectionRules.js.
 * @typedef {Object} CollectionRule
 * @property {"rating" | "flag" | "color_label" | "keyword"} field
 * @property {">=" | "<=" | "==" | "has" | "empty"} op
 * @property {number | string} [value] -- omitted for op "empty"
 */

/**
 * @typedef {Object} CollectionSummary
 * @property {number} id
 * @property {string} name
 * @property {boolean} is_smart
 * @property {CollectionRule[] | null} rules
 * @property {number | null} count -- real count for manual, null for smart
 *   (the frontend computes a smart collection's real count client-side)
 */

/** Bare rail "+" with no selection. @returns {Promise<number>} the new collection id */
export function createCollection(/** @type {string} */ name) {
  return invoke("create_collection", { name });
}

/** "Add to Collection… -> New Collection…" from a multi-selection -- one
 * atomic create-then-populate call, not two, so a failure can't leave an
 * orphaned empty collection. @returns {Promise<number>} the new collection id */
export function createCollectionWithImages(/** @type {string} */ name, /** @type {number[]} */ imageIds) {
  return invoke("create_collection_with_images", { name, imageIds });
}

/** @returns {Promise<number>} the new collection id */
export function createSmartCollection(/** @type {string} */ name, /** @type {CollectionRule[]} */ rules) {
  return invoke("create_smart_collection", { name, rules });
}

export function updateSmartCollectionRules(
  /** @type {number} */ collectionId,
  /** @type {CollectionRule[]} */ rules,
) {
  return invoke("update_smart_collection_rules", { collectionId, rules });
}

export function deleteCollection(/** @type {number} */ collectionId) {
  return invoke("delete_collection", { collectionId });
}

export function addImagesToCollection(/** @type {number} */ collectionId, /** @type {number[]} */ imageIds) {
  return invoke("add_images_to_collection", { collectionId, imageIds });
}

export function removeImagesFromCollection(/** @type {number} */ collectionId, /** @type {number[]} */ imageIds) {
  return invoke("remove_images_from_collection", { collectionId, imageIds });
}

/** @returns {Promise<CollectionSummary[]>} */
export function listCollections() {
  return invoke("list_collections");
}

/** A manual collection's membership -- meaningless for a smart collection
 * (always empty; smart membership is computed client-side).
 * @returns {Promise<number[]>} */
export function listCollectionImageIds(/** @type {number} */ collectionId) {
  return invoke("list_collection_image_ids", { collectionId });
}
