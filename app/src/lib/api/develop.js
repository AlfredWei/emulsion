// Thin wrapper around the Develop-related Tauri commands (M1 Slice 3, see
// app/src-tauri/src/lib.rs) — keeps raw command-name strings out of components.

import { invoke } from "@tauri-apps/api/core";

/**
 * @typedef {Object} DevelopPreviewInfo
 * @property {string} path
 * @property {number} width
 * @property {number} height
 */

/**
 * @typedef {Object} EditOp
 * @property {string} op
 * @property {number} value
 */

/**
 * @typedef {Object} EditStack
 * @property {number} schema_version
 * @property {EditOp[]} ops
 */

/** @returns {Promise<DevelopPreviewInfo>} */
export function getDevelopPreview(/** @type {string} */ path) {
  return invoke("get_develop_preview", { path });
}

/** @returns {Promise<EditStack>} */
export function getEditStack(/** @type {number} */ versionId) {
  return invoke("get_edit_stack", { versionId });
}

/** @returns {Promise<void>} */
export function setEditStack(/** @type {number} */ versionId, /** @type {EditStack} */ stack) {
  return invoke("set_edit_stack", { versionId, stack });
}

/** Thumbnail refresh after a Develop edit -- call AFTER setEditStack has
 * already resolved, never chained onto that same call, so a slow/failed
 * regen can never delay the edit save or app quit. Resolves to `null`
 * (not an error) if regeneration failed for any reason -- see the Rust
 * command's doc comment.
 * @returns {Promise<string | null>} the new thumbnail_path, or null */
export function regenerateThumbnail(/** @type {number} */ versionId) {
  return invoke("regenerate_thumbnail", { versionId });
}

/** Find an op's current value by name, or a fallback if not present yet. */
export function opValue(/** @type {EditStack} */ stack, /** @type {string} */ opName, /** @type {number} */ fallback) {
  return stack.ops.find((o) => o.op === opName)?.value ?? fallback;
}

/** Upsert an op by name (replace if present, append if not) — matches the
 * catalog's edit-stack model (ADR-0006): a slider's current value is a
 * single entry, not an append-only history log. */
export function upsertOp(/** @type {EditStack} */ stack, /** @type {string} */ opName, /** @type {number} */ value) {
  const ops = stack.ops.filter((o) => o.op !== opName);
  ops.push({ op: opName, value });
  return { ...stack, ops };
}
