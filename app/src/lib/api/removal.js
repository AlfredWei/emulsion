// IPC wrappers for object removal (RFC-0032, ADR-0010). The commands live in removal_commands.rs; the work runs in the
// separate `emulsion-ai` helper process (LaMa), a second or several per separate thing marked.

import { invoke } from "@tauri-apps/api/core";

/**
 * One mark, in coordinates normalised to the whole photograph (0..1 on both axes).
 * A lasso is the inside of the closed outline; a stroke is a round brush dragged along `points`, `radius` a fraction of
 * the photograph's longer side.
 * A click-selected object is the model's 256 x 256 cut; `bbox` and `tint` are for the screen only (the job reads `logits`).
 * @typedef {{ kind: "polygon", points: [number, number][] } | { kind: "stroke", points: [number, number][], radius: number } | { kind: "segment", logits: string, bbox: [number, number, number, number], tint: string }} MaskShape
 */

/**
 * What one separate thing became: the stored patch (named by `patch_id` in the `remove` op) and where it is.
 * @typedef {Object} RemoveResult
 * @property {number} patch_id
 * @property {string} engine
 * @property {number} x @property {number} y @property {number} w @property {number} h
 * @property {number} src_w @property {number} src_h
 * @property {boolean} reduced - the hole was larger than the network sees at native size: the fill is softer
 * @property {number} ms
 */

/** Whether the removal model is installed. A build without it has no Remove.
 * @returns {Promise<boolean>} */
export function removalAvailable() {
  return invoke("removal_available");
}

/** Removes what the marks cover from the photograph of `versionId`. Stores the patches and returns them; the caller
 * names them in the edit stack. Fails with the reason as a string (`a removal is already running`, `removal cancelled`...).
 * @param {number} versionId @param {MaskShape[]} shapes @param {number} [growPx]
 * @returns {Promise<RemoveResult[]>} */
export function removeObject(versionId, shapes, growPx) {
  return invoke("remove_object", { versionId, shapes, growPx });
}

/** Stops the running removal; nothing is stored. A no-op when none runs.
 * @returns {Promise<void>} */
export function removalCancel() {
  return invoke("removal_cancel");
}

/** True when the error says the job was stopped by the user.
 * @param {unknown} e */
export function isRemovalCancelled(e) {
  return String(e).includes("removal cancelled");
}

/**
 * What is known of a removal without reading its pictures: the decode it was made on and whether it has a stored hole.
 * @typedef {{ patch_id: number, src_w: number, src_h: number, has_hole: boolean }} RemovalInfo
 */

/** What the catalog knows of each of `patchIds` (removals of `versionId`); a removal with no row is left out.
 * @param {number} versionId @param {number[]} patchIds
 * @returns {Promise<RemovalInfo[]>} */
export function removalInfo(versionId, patchIds) {
  return invoke("removal_info", { versionId, patchIds });
}

/**
 * The hole stored with a finished removal: where it sits in the photograph's full decode (`src_w` x `src_h`) and a gray
 * PNG (base64) of just its box, 255 inside. Null for a removal made before holes were kept.
 * @typedef {{ x: number, y: number, src_w: number, src_h: number, png: string }} RemovalHole
 * @param {number} versionId @param {number} patchId
 * @returns {Promise<RemovalHole | null>} */
export function removalHole(versionId, patchId) {
  return invoke("removal_hole", { versionId, patchId });
}

/** The id of the installed removal engine: a removal made with another one can be tried again with this one.
 * @returns {Promise<string>} */
export function removalEngine() {
  return invoke("removal_engine");
}

/** Runs removal `patchId` of `versionId` again from its stored hole and stores the result as a NEW patch (the old one stays,
 * so History can bring it back). The caller swaps the item in the `remove` op. Fails with the reason as a string
 * (`this removal has no stored hole`, `... no longer fits`, `removal cancelled`...).
 * @param {number} versionId @param {number} patchId
 * @returns {Promise<RemoveResult>} */
export function regenerateRemoval(versionId, patchId) {
  return invoke("regenerate_removal", { versionId, patchId });
}
