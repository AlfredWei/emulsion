// Thin wrapper around the face detection / People view Tauri commands
// (M5 Slice 6, RFC-0005, see app/src-tauri/src/lib.rs) -- keeps raw
// command-name strings out of components, matching backup.js/print.js's
// precedent of one small module per concern.

import { invoke } from "@tauri-apps/api/core";

/**
 * @typedef {Object} FaceRow
 * @property {number} id
 * @property {number} image_id
 * @property {number | null} person_id
 * @property {string | null} person_name
 * @property {number} bbox_x
 * @property {number} bbox_y
 * @property {number} bbox_w
 * @property {number} bbox_h
 */

/**
 * @typedef {Object} PersonRow
 * @property {number} id
 * @property {string | null} name
 * @property {number | null} cover_face_id
 * @property {number} photo_count
 * @property {string | null} cover_image_path
 * @property {number | null} cover_bbox_x
 * @property {number | null} cover_bbox_y
 * @property {number | null} cover_bbox_w
 * @property {number | null} cover_bbox_h
 */

/** Detects + embeds + incrementally clusters faces for one import batch
 * (ImportSummary.import_batch). Emits `"face-detection-progress"`
 * `{current, total}` events while it runs, same shape as
 * `"thumbnail-progress"`. Fetches/caches the YuNet/SFace model files on
 * first call, so the first run of this per catalog needs network access.
 * @returns {Promise<void>} */
export function detectFacesForImportBatch(/** @type {number} */ importBatch) {
  return invoke("detect_faces_for_import_batch", { importBatch });
}

/** The explicit "Find People" action: re-clusters every non-excluded face
 * in the whole catalog from scratch, reusing each face's existing person
 * id where it has one so it doesn't orphan names already typed.
 * @returns {Promise<void>} */
export function reclusterFaces() {
  return invoke("recluster_faces");
}

/** @returns {Promise<PersonRow[]>} */
export function listPeople() {
  return invoke("list_people");
}

/** @returns {Promise<FaceRow[]>} */
export function getFacesForImage(/** @type {number} */ imageId) {
  return invoke("get_faces_for_image", { imageId });
}

/** Creates a person with `coverFaceId` as its cover -- does NOT itself
 * assign that face to the new person; pair with `reassignFace` (and
 * usually `renamePerson`) right after.
 * @returns {Promise<number>} the new person's id */
export function createPerson(/** @type {number} */ coverFaceId) {
  return invoke("create_person", { coverFaceId });
}

/** @returns {Promise<void>} */
export function renamePerson(/** @type {number} */ personId, /** @type {string | null} */ name) {
  return invoke("rename_person", { personId, name });
}

/** Backs both the tag popover's "existing suggestion" pick and the face
 * menu's "Reassign to…" -- `personId: null` clears the assignment back to
 * unclustered rather than deleting the face.
 * @returns {Promise<void>} */
export function reassignFace(/** @type {number} */ faceId, /** @type {number | null} */ personId) {
  return invoke("reassign_face", { faceId, personId });
}

/** Backs the face menu's "Not a face" action -- kept, not deleted; also
 * clears the face's `person_id` on exclude.
 * @returns {Promise<void>} */
export function setFaceExcluded(/** @type {number} */ faceId, /** @type {boolean} */ excluded) {
  return invoke("set_face_excluded", { faceId, excluded });
}
