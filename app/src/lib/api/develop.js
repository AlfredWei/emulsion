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
 * @typedef {Object} LinearGradientMask
 * @property {"linear_gradient_mask"} op
 * @property {string} id
 * @property {{x: number, y: number}} start
 * @property {{x: number, y: number}} end
 * @property {number} feather
 * @property {boolean} invert
 * @property {number} exposure
 * @property {number} contrast
 * @property {number} saturation
 */

/**
 * @typedef {Object} EditStack
 * @property {number} schema_version
 * @property {Array<EditOp | LinearGradientMask>} ops
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
  const op = /** @type {any} */ (stack.ops.find((o) => o.op === opName));
  return op?.value ?? fallback;
}

/** Upsert an op by name (replace if present, append if not) — matches the
 * catalog's edit-stack model (ADR-0006): a slider's current value is a
 * single entry, not an append-only history log. */
export function upsertOp(/** @type {EditStack} */ stack, /** @type {string} */ opName, /** @type {number} */ value) {
  const ops = stack.ops.filter((o) => o.op !== opName);
  ops.push({ op: opName, value });
  return { ...stack, ops };
}

// M3 Slice 5: masks are id-keyed, multi-instance ops -- unlike the global
// sliders above (opValue/upsertOp's find-by-name-and-replace model), there
// can be several linear gradients on one image, so each needs a stable
// identity of its own, not a shared name slot. Matches the WGSL shader's
// fixed-size array (DevelopCanvas.svelte), which the UI must cap at.
export const MAX_MASKS = 8;

/** @returns {LinearGradientMask} */
export function createLinearGradientMask(
  /** @type {{x: number, y: number}} */ start,
  /** @type {{x: number, y: number}} */ end,
) {
  return {
    op: "linear_gradient_mask",
    id: crypto.randomUUID(),
    start,
    end,
    feather: 0,
    invert: false,
    exposure: 0,
    contrast: 0,
    saturation: 0,
  };
}

/** @returns {LinearGradientMask[]} */
export function listMasks(/** @type {EditStack} */ stack) {
  return /** @type {any} */ (stack.ops.filter((o) => o.op === "linear_gradient_mask"));
}

/** @returns {EditStack} */
export function addMask(/** @type {EditStack} */ stack, /** @type {LinearGradientMask} */ mask) {
  return { ...stack, ops: [...stack.ops, mask] };
}

/** @returns {EditStack} */
export function updateMask(
  /** @type {EditStack} */ stack,
  /** @type {string} */ id,
  /** @type {Partial<LinearGradientMask>} */ patch,
) {
  const ops = stack.ops.map((o) =>
    o.op === "linear_gradient_mask" && /** @type {any} */ (o).id === id ? { ...o, ...patch } : o,
  );
  return { ...stack, ops };
}

/** @returns {EditStack} */
export function removeMask(/** @type {EditStack} */ stack, /** @type {string} */ id) {
  const ops = stack.ops.filter((o) => !(o.op === "linear_gradient_mask" && /** @type {any} */ (o).id === id));
  return { ...stack, ops };
}
