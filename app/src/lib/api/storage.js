// Thin wrapper around the Settings > Storage Tauri commands (see
// app/src-tauri/src/storage.rs) -- one small module per concern, matching
// backup.js's precedent.

import { invoke } from "@tauri-apps/api/core";

/**
 * @typedef {Object} StorageInfo
 * @property {string | null} cache_dir The user's override, or null to use the default app-data location.
 * @property {string} effective_dir The directory actually in effect right now.
 * @property {number} thumbnails_bytes
 * @property {number} previews_bytes
 */

/** @returns {Promise<StorageInfo>} */
export function getStorageInfo() {
  return invoke("get_storage_info");
}

/** Moves existing thumbnails + preview-cache files to `newDir` (or back to
 * the default location if `newDir` is null), rewriting the catalog's
 * stored thumbnail paths to match. Can take a real, visible amount of time
 * on a large library -- callers should show a busy state.
 * @returns {Promise<StorageInfo>} */
export function setCacheDir(/** @type {string | null} */ newDir) {
  return invoke("set_cache_dir", { newDir });
}
