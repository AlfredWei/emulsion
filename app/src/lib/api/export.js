// Thin wrapper around the Export Tauri command (M1 Slice 5, see
// app/src-tauri/src/export.rs) — keeps raw command-name strings out of
// components.

import { invoke } from "@tauri-apps/api/core";

/**
 * @typedef {Object} ExportItem
 * @property {string} path
 * @property {number} version_id
 */

/**
 * @typedef {Object} ExportOptions
 * @property {string} destination_dir
 * @property {number | null} long_edge
 * @property {number} quality
 * @property {number | null} plugin_id
 * @property {MetadataWriteOptions} metadata
 */

/**
 * Which metadata groups to embed in each exported JPEG. `gps` only applies
 * when `exif` is true.
 * @typedef {Object} MetadataWriteOptions
 * @property {boolean} exif
 * @property {boolean} iptc
 * @property {boolean} gps
 */

/**
 * @typedef {Object} ExportResult
 * @property {string} source_path
 * @property {string | null} output_path
 * @property {string | null} error
 * @property {string | null} plugin_error
 * @property {string | null} metadata_warning
 */

/** @returns {Promise<ExportResult[]>} */
export function exportImages(/** @type {ExportItem[]} */ items, /** @type {ExportOptions} */ options) {
  return invoke("export_images", { items, options });
}

/**
 * Export plugins (M5, RFC-0006) — the v0 extensibility surface's CRUD, kept
 * here rather than a dedicated exportPlugins.js: it's this module's own
 * concern (a post-export hook), same "colocate with the concern it belongs
 * to" precedent develop.js's own preset commands already follow rather
 * than one file per Tauri command.
 *
 * @typedef {Object} ExportPlugin
 * @property {number} id
 * @property {string} name
 * @property {string} command
 * @property {string[]} args_template
 * @property {string} created_at
 */

/** @returns {Promise<ExportPlugin[]>} */
export function listExportPlugins() {
  return invoke("list_export_plugins");
}

/** @returns {Promise<ExportPlugin>} */
export function addExportPlugin(/** @type {string} */ name, /** @type {string} */ command, /** @type {string[]} */ argsTemplate) {
  return invoke("add_export_plugin", { name, command, argsTemplate });
}

/** @returns {Promise<void>} */
export function updateExportPlugin(
  /** @type {number} */ id,
  /** @type {string} */ name,
  /** @type {string} */ command,
  /** @type {string[]} */ argsTemplate,
) {
  return invoke("update_export_plugin", { id, name, command, argsTemplate });
}

/** @returns {Promise<void>} */
export function deleteExportPlugin(/** @type {number} */ id) {
  return invoke("delete_export_plugin", { id });
}
