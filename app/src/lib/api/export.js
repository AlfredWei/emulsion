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
 */

/**
 * @typedef {Object} ExportResult
 * @property {string} source_path
 * @property {string | null} output_path
 * @property {string | null} error
 */

/** @returns {Promise<ExportResult[]>} */
export function exportImages(/** @type {ExportItem[]} */ items, /** @type {ExportOptions} */ options) {
  return invoke("export_images", { items, options });
}
