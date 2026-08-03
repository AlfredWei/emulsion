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
 * @typedef {Object} RadialGradientMask
 * @property {"radial_gradient_mask"} op
 * @property {string} id
 * @property {{x: number, y: number}} center
 * @property {number} radiusX
 * @property {number} radiusY
 * @property {number} feather
 * @property {boolean} invert
 * @property {number} exposure
 * @property {number} contrast
 * @property {number} saturation
 */

/**
 * @typedef {Object} Dab
 * @property {number} x
 * @property {number} y
 * @property {number} radius - normalized fraction of image WIDTH only (a
 *   single scalar, not radiusX/radiusY like radial masks) -- rasterization
 *   happens directly in the offscreen canvas's own native-pixel space
 *   (DevelopCanvas.svelte), so `radius * nativeWidth` used for both
 *   dimensions of `ctx.arc()` is inherently a true circle in image-pixel
 *   space, with no separate axis scaling needed.
 * @property {number} hardness - 0 (fully soft) .. 100 (harder edge), baked
 *   into this dab's own radial-gradient falloff at paint time.
 * @property {number} flow - 0..1, baked in at paint time.
 * @property {"add" | "erase"} mode
 */

/**
 * @typedef {Object} BrushMask
 * @property {"brush_mask"} op
 * @property {string} id
 * @property {Dab[]} dabs
 * @property {boolean} invert
 * @property {number} exposure
 * @property {number} contrast
 * @property {number} saturation
 */

/**
 * @typedef {Object} LuminanceRangeMask
 * @property {"luminance_range_mask"} op
 * @property {string} id
 * @property {number} rangeMin - 0-100, matching linear/radial's own feather
 *   scale convention (not a separate 0-1 scale).
 * @property {number} rangeMax - 0-100.
 * @property {number} feather - 0-100, band WIDTH around each of the two
 *   range edges (a different meaning from linear/radial's single-boundary
 *   feather, so it's shown via a dedicated Min/Max/Feather block in
 *   MaskEditorPanel.svelte, not the shared Feather row).
 * @property {boolean} invert
 * @property {number} exposure
 * @property {number} contrast
 * @property {number} saturation
 */

/** @typedef {LinearGradientMask | RadialGradientMask | BrushMask | LuminanceRangeMask} Mask */

/**
 * @typedef {Object} EditStack
 * @property {number} schema_version
 * @property {Array<EditOp | Mask>} ops
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

// M3 Slice 5/6: masks are id-keyed, multi-instance ops -- unlike the global
// sliders above (opValue/upsertOp's find-by-name-and-replace model), there
// can be several masks (of possibly different kinds) on one image, so each
// needs a stable identity of its own, not a shared name slot. Matches the
// WGSL shader's fixed-size array (DevelopCanvas.svelte), which the UI must
// cap at -- a combined cap across every kind, not per-kind, since it's a
// hardware/uniform-array-size limit.
export const MAX_MASKS = 8;

// An explicit allowlist, not an implicit naming convention (e.g.
// `endsWith("_mask")`) -- simpler and more robust, since it can't silently
// misclassify some future non-mask op that happens to share the suffix.
const MASK_OP_NAMES = ["linear_gradient_mask", "radial_gradient_mask", "brush_mask", "luminance_range_mask"];

// Mask kinds with no on-canvas geometry to show (brush's painted region,
// luminance range's pixel-value-based selection) get a toggleable colored
// overlay -- linear/radial keep their existing dashed-outline-only
// feedback instead, a deliberate scope decision from the overlay slice
// (PROGRESS.md), preserved here as the single place this list lives so the
// hotkey gate (+page.svelte) and the checkbox gate (MaskEditorPanel.svelte)
// can't drift apart.
export const OVERLAY_CAPABLE_MASK_OPS = ["brush_mask", "luminance_range_mask"];

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

/** M3 Slice 6: default feather=50 (not 0 like linear's default) and
 * invert=false meaning the effect applies OUTSIDE the ellipse -- both
 * deliberately match real Lightroom's own Radial Filter convention (its
 * classic vignette use case; checking Invert flips it to the
 * spotlight/subject-enhancement inside application).
 * @returns {RadialGradientMask} */
export function createRadialGradientMask(
  /** @type {{x: number, y: number}} */ center,
  /** @type {number} */ radiusX,
  /** @type {number} */ radiusY,
) {
  return {
    op: "radial_gradient_mask",
    id: crypto.randomUUID(),
    center,
    radiusX,
    radiusY,
    feather: 50,
    invert: false,
    exposure: 0,
    contrast: 0,
    saturation: 0,
  };
}

/** M3 Slice 7: no mask-level `feather` (unlike linear/radial) -- softness
 * is baked per-dab at paint time from the current tool setting (real
 * Lightroom's own brush-options model: Size/Feather/Flow apply to
 * whatever gets painted NEXT, not editable globally after the fact).
 * Accepts an optional `id` -- DevelopCanvas.svelte generates the id itself
 * (needed synchronously, before this creation call round-trips back down
 * as a prop) and passes it through, rather than discarding a locally-
 * generated one here.
 * @returns {BrushMask} */
export function createBrushMask(/** @type {string=} */ id) {
  return {
    op: "brush_mask",
    id: id ?? crypto.randomUUID(),
    dabs: [],
    invert: false,
    exposure: 0,
    contrast: 0,
    saturation: 0,
  };
}

/** Defaults (30/70/20) select midtones out of the box -- gives the user
 * something visible to tune immediately, rather than an empty or
 * full-frame selection. No canvas interaction needed to create this kind
 * (see DevelopCanvas.svelte/MaskToolStrip.svelte -- it's created directly
 * on tool-button click, real Lightroom's own behavior for this mask kind).
 * @returns {LuminanceRangeMask} */
export function createLuminanceRangeMask() {
  return {
    op: "luminance_range_mask",
    id: crypto.randomUUID(),
    rangeMin: 30,
    rangeMax: 70,
    feather: 20,
    invert: false,
    exposure: 0,
    contrast: 0,
    saturation: 0,
  };
}

/** @returns {Mask[]} */
export function listMasks(/** @type {EditStack} */ stack) {
  return /** @type {any} */ (stack.ops.filter((o) => MASK_OP_NAMES.includes(o.op)));
}

/** @returns {EditStack} */
export function addMask(/** @type {EditStack} */ stack, /** @type {Mask} */ mask) {
  return { ...stack, ops: [...stack.ops, mask] };
}

/** @returns {EditStack} */
export function updateMask(
  /** @type {EditStack} */ stack,
  /** @type {string} */ id,
  /** @type {Partial<Mask>} */ patch,
) {
  const ops = stack.ops.map((o) =>
    MASK_OP_NAMES.includes(o.op) && /** @type {any} */ (o).id === id
      ? /** @type {any} */ ({ ...o, ...patch })
      : o,
  );
  return { ...stack, ops };
}

/** @returns {EditStack} */
export function removeMask(/** @type {EditStack} */ stack, /** @type {string} */ id) {
  const ops = stack.ops.filter((o) => !(MASK_OP_NAMES.includes(o.op) && /** @type {any} */ (o).id === id));
  return { ...stack, ops };
}
