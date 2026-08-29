// Thin wrapper around the Print Tauri command (M4, final scope item, see
// app/src-tauri/src/print.rs) — keeps raw command-name strings out of
// components.

import { invoke } from "@tauri-apps/api/core";

/**
 * @typedef {Object} PrintColorManagement
 * @property {import("./develop.js").SoftProofSettings | null} profile
 */

/**
 * @typedef {Object} PrintReadyResult
 * @property {number} version_id
 * @property {string | null} path
 * @property {number | null} width
 * @property {number | null} height
 * @property {string | null} error
 */

/** @returns {Promise<PrintReadyResult[]>} */
export function getPrintReadyImages(
  /** @type {number[]} */ versionIds,
  /** @type {PrintColorManagement} */ colorManagement,
) {
  return invoke("get_print_ready_images", { versionIds, colorManagement });
}

/**
 * @typedef {Object} PdfLayout
 * @property {"single" | "contact-sheet"} template
 * @property {"fit" | "fill"} fit_mode
 * @property {number} rows
 * @property {number} cols
 * @property {number} cell_spacing_in
 */

/**
 * @typedef {Object} PdfPageSetup
 * @property {number} width_in
 * @property {number} height_in
 * @property {number} margin_top_in
 * @property {number} margin_right_in
 * @property {number} margin_bottom_in
 * @property {number} margin_left_in
 */

/**
 * @typedef {Object} PrintPdfRequest
 * @property {number[]} version_ids
 * @property {string} destination_path
 * @property {PdfLayout} layout
 * @property {PdfPageSetup} page
 * @property {PrintColorManagement} color_management
 */

/** "Export as PDF" -- a direct, one-step alternative to the `window.print()`
 * flow (see PrintLayoutView.svelte): writes a real PDF straight to
 * `destination_path` (picked via a native save dialog, same as
 * Export/preset-export's own file-picker convention), no interactive OS
 * print dialog involved. Reuses the exact same full-resolution,
 * color-managed raster `get_print_ready_images` already generates and
 * caches -- see `print.rs`'s own doc comment on `export_pdf`.
 * @returns {Promise<void>} */
export function exportPrintPdf(/** @type {PrintPdfRequest} */ request) {
  return invoke("export_print_pdf", { request });
}

/** Fixed paper-size list, inches -- matches this slice's scope cut (a fixed
 * dropdown, not free-form custom page sizes). A4/A3 are shown under their
 * familiar metric names even though the values stored/used throughout the
 * Print module are inches, matching every other unit in this panel. */
export const PAPER_SIZES = {
  letter: { name: "Letter (8.5 × 11 in)", widthIn: 8.5, heightIn: 11 },
  legal: { name: "Legal (8.5 × 14 in)", widthIn: 8.5, heightIn: 14 },
  a4: { name: "A4 (210 × 297 mm)", widthIn: 8.27, heightIn: 11.69 },
  a3: { name: "A3 (297 × 420 mm)", widthIn: 11.69, heightIn: 16.54 },
  "4x6": { name: "4 × 6 in", widthIn: 4, heightIn: 6 },
  "5x7": { name: "5 × 7 in", widthIn: 5, heightIn: 7 },
};
