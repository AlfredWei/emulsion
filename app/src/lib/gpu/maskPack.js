// Packs the edit stack's masks into the two uniform arrays the mask pass reads (see the Mask and
// Modifier structs in shaders/common.js). Pure so the layout is unit-tested without a GPU; moved
// out of renderFrame.js when composable masking (RFC-0025) added the modifier array.

import { spotCentroidAndRadius } from "$lib/maskGeometry.js";
import { MAX_MASKS, MAX_MODIFIERS, MASK_STRIDE, MOD_STRIDE } from "./gpuHandles.js";

/** Mask `kind` codes in `params.z`; fs_mask's if-chain and Rust's `Mask` enum use the same order. */
export const MASK_KIND = Object.freeze({
  linear_gradient_mask: 0,
  radial_gradient_mask: 1,
  brush_mask: 2,
  luminance_range_mask: 3,
  color_range_mask: 4,
  spot_mask: 5,
  red_eye_mask: 6,
  // Weight-producing like 0-4, but numbered after the two special kinds so their codes never moved (RFC-0026).
  segment_mask: 7,
});

/** `combine.x` codes read by fs_mask. */
export const COMBINE_CODE = Object.freeze({ add: 0, subtract: 1, intersect: 2 });

/**
 * Writes one weight-producing shape (kinds 0-4 and 7) at `o`: start_end at o+0..3, feather o+4, invert
 * o+5, kind o+6, raster layer o+7 (brush and segment). Shared by a base mask and a modifier, so both are packed (and so
 * evaluated by `component_weight`) identically. Returns false for any other op.
 * @param {Float32Array} data @param {number} o @param {any} shape
 * @param {number} layer texture-array layer, used by brush only
 */
function packShape(data, o, shape, layer) {
  switch (shape.op) {
    case "linear_gradient_mask":
      data[o + 0] = shape.start.x;
      data[o + 1] = shape.start.y;
      data[o + 2] = shape.end.x;
      data[o + 3] = shape.end.y;
      data[o + 4] = shape.feather;
      break;
    case "radial_gradient_mask":
      data[o + 0] = shape.center.x;
      data[o + 1] = shape.center.y;
      data[o + 2] = shape.radiusX;
      data[o + 3] = shape.radiusY;
      data[o + 4] = shape.feather;
      break;
    case "brush_mask":
      data[o + 7] = layer;
      break;
    case "segment_mask":
      // start_end.x = grow, params.x = feather, params.w = the layer holding the upscaled logit field.
      data[o + 0] = shape.grow ?? 0;
      data[o + 4] = shape.feather ?? 0;
      data[o + 7] = layer;
      break;
    case "luminance_range_mask":
      data[o + 0] = shape.rangeMin;
      data[o + 1] = shape.rangeMax;
      data[o + 4] = shape.feather;
      break;
    case "color_range_mask":
      data[o + 0] = shape.refColor.r;
      data[o + 1] = shape.refColor.g;
      data[o + 2] = shape.refColor.b;
      data[o + 3] = shape.range;
      data[o + 4] = shape.feather;
      break;
    default:
      return false;
  }
  data[o + 5] = shape.invert ? 1 : 0;
  data[o + 6] = MASK_KIND[/** @type {keyof typeof MASK_KIND} */ (shape.op)];
  return true;
}

/**
 * @param {any[]} masks the image's masks in stack order
 * @param {(id: string) => number | undefined} brushLayerOf raster layer of a brush/spot mask or brush modifier, by id
 * @returns {{ maskData: Float32Array, modData: Float32Array, modCount: number }}
 *   Only the first MAX_MASKS masks and, in mask order, the first MAX_MODIFIERS modifiers are packed;
 *   the UI caps both (develop.js `modifierBlockedReason`), so truncation is a defensive no-op.
 */
export function packMasks(masks, brushLayerOf) {
  const maskData = new Float32Array(MAX_MASKS * MASK_STRIDE);
  const modData = new Float32Array(MAX_MODIFIERS * MOD_STRIDE);
  let modCount = 0;
  masks.slice(0, MAX_MASKS).forEach((m, i) => {
    const o = i * MASK_STRIDE;
    if (m.op === "spot_mask") {
      // Spot copies pixel content rather than gating an adjustment: start_end = source offset +
      // dabs' centroid, params.x feather, params.y average radius (heal ring), params.w layer,
      // adjustments.x = mode. It carries no adjustments and takes no modifiers.
      const c = spotCentroidAndRadius(m.dabs);
      maskData[o + 0] = m.sourceOffset.dx;
      maskData[o + 1] = m.sourceOffset.dy;
      maskData[o + 2] = c.x;
      maskData[o + 3] = c.y;
      maskData[o + 4] = m.feather;
      maskData[o + 5] = c.avgRadius;
      maskData[o + 6] = MASK_KIND.spot_mask;
      maskData[o + 7] = brushLayerOf(m.id) ?? 0;
      maskData[o + 8] = m.mode === "heal" ? 1 : 0;
      return;
    }
    if (m.op === "red_eye_mask") {
      // Radial's ellipse geometry, with params.y = pupilSize and adjustments.w = darken. No
      // invert, no adjustments, no modifiers.
      maskData[o + 0] = m.center.x;
      maskData[o + 1] = m.center.y;
      maskData[o + 2] = m.radiusX;
      maskData[o + 3] = m.radiusY;
      maskData[o + 4] = m.feather;
      maskData[o + 5] = m.pupilSize;
      maskData[o + 6] = MASK_KIND.red_eye_mask;
      maskData[o + 11] = m.darken;
      return;
    }
    // Kinds 0-4 and 7. A real bug once lived here: an unconditional catch-all `else` treated "anything
    // that isn't radial or brush" as linear, so a kind with no .start/.end threw and aborted the
    // render of every mask. Unknown ops are skipped, never defaulted into a kind.
    if (!packShape(maskData, o, m, brushLayerOf(m.id) ?? 0)) return;
    maskData[o + 8] = m.exposure;
    maskData[o + 9] = m.contrast;
    maskData[o + 10] = m.saturation;

    const mods = /** @type {any[]} */ (m.modifiers ?? []);
    maskData[o + 12] = modCount; // first modifier index
    let n = 0;
    for (const mod of mods) {
      if (modCount >= MAX_MODIFIERS) break;
      const mo = modCount * MOD_STRIDE;
      if (!packShape(modData, mo, mod.shape, brushLayerOf(mod.id) ?? 0)) continue;
      modData[mo + 8] = COMBINE_CODE[/** @type {keyof typeof COMBINE_CODE} */ (mod.combine)] ?? 0;
      modCount += 1;
      n += 1;
    }
    maskData[o + 13] = n;
  });
  return { maskData, modData, modCount };
}

/**
 * Everything that owns a raster layer, in stack order: brush, spot and segment masks, and brush /
 * segment modifiers (keyed by the modifier's id, shaped like their stand-alone kind).
 * @param {any[]} masks
 * @returns {{ id: string, op: "brush_mask" | "spot_mask" | "segment_mask", dabs?: any[], logits?: string, feather?: number }[]}
 */
export function rasterTargets(masks) {
  /** @type {any[]} */
  const out = [];
  for (const m of masks) {
    if (m.op === "brush_mask" || m.op === "spot_mask" || m.op === "segment_mask") out.push(m);
    for (const mod of m.modifiers ?? []) {
      if (mod.shape.op === "brush_mask") out.push({ id: mod.id, op: "brush_mask", dabs: mod.shape.dabs });
      if (mod.shape.op === "segment_mask") out.push({ id: mod.id, op: "segment_mask", logits: mod.shape.logits });
    }
  }
  return out;
}
