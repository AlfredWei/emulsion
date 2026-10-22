// IPC wrappers for the culling aids (RFC-0031). The commands live in lib.rs (`list_similar_groups`,
// `cull_analysis_status`); the grouping itself is cull_groups.rs.

import { invoke } from "@tauri-apps/api/core";

/**
 * One frame of a group.
 * @typedef {Object} SimilarMember
 * @property {number} image_id
 * @property {number} version_id
 * @property {number} relative_sharpness - this frame's score as a share of the group's best (1 = the best)
 * @property {boolean} soft - under half of the group's best
 */

/**
 * @typedef {Object} SimilarGroup
 * @property {SimilarMember[]} members - chronological
 * @property {number | null} best_image_id - the sharpest frame, only when it clearly beats the runner-up
 */

/** Groups of similar photos, newest group first. `similarity` is the 0 (stricter) to 1 (looser) slider.
 * @returns {Promise<SimilarGroup[]>} */
export function listSimilarGroups(/** @type {number} */ similarity) {
  return invoke("list_similar_groups", { similarity });
}

/** How many photos the background analysis has measured, of how many.
 * @returns {Promise<{ analysed: number, total: number }>} */
export function cullAnalysisStatus() {
  return invoke("cull_analysis_status");
}
