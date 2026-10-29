// IPC wrapper for the adaptive-preset region resolvers (RFC-0035 B1, adaptive_commands.rs).

import { invoke } from "@tauri-apps/api/core";

/**
 * @typedef {"found" | "check" | "not_found"} AdaptiveOutcome
 * @typedef {Object} AdaptiveRegionResult
 * @property {AdaptiveOutcome} outcome
 * @property {number} coverage - share of the frame the region covers, 0..1 (0 when not found)
 * @property {string | null} reason
 * @property {Record<string, any> | null} mask - a mask op to store, without adjustments or id; null when not found
 */

/** Resolves one kind of region on one photograph, from its stored face rows (`face`: scan first) or its
 * unedited Develop preview (`sky`, `shadows`).
 * @param {number} imageId @param {string} path @param {string} contentHash @param {"sky" | "face" | "shadows"} region
 * @returns {Promise<AdaptiveRegionResult>} */
export function resolveAdaptiveRegion(imageId, path, contentHash, region) {
  return invoke("resolve_adaptive_region", { imageId, path, contentHash, region });
}
