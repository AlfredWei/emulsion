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
