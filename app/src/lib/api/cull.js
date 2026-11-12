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

/**
 * What the eye check says of one photograph (RFC-0031 §8.15). Faces too small or too uncertain to judge are counted in `faces` but
 * not in `judged`: the check says "cannot tell" rather than guess.
 * @typedef {Object} BlinkPhoto
 * @property {number} image_id
 * @property {number} faces - faces found
 * @property {number} judged - faces whose eyes could be told apart
 * @property {number} closed - judged faces with both eyes closed
 * @property {number} one_closed - judged faces with one eye closed (a wink, a squint, a half blink)
 */

/** The eye check for these photographs: stored answers as they are, the rest measured now (a second or so each).
 * `available` is false when a model it needs is missing: `reason` is "mesh_model" or "face_models".
 * @returns {Promise<{ available: boolean, reason: string, photos: BlinkPhoto[] }>} */
export function blinkForImages(/** @type {number[]} */ imageIds) {
  return invoke("blink_for_images", { imageIds });
}
