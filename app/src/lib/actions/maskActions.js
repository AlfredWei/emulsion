// Local-adjustment mask and eyedropper workflows (RFC-0009 P6b, moved out of +page.svelte's script):
// placing, updating and deleting masks, the colour-range re-sample, and the four eyedropper
// destinations. Each writes the edit stack (`develop`) and the tool/selection state (`masks`), so
// they are actions rather than methods on either store.

import { develop } from "$lib/state/develop.svelte.js";
import { masks } from "$lib/state/masks.svelte.js";
import { updateMask, createRadialGradientMask, createBrushMask, createColorRangeMask, createSpotMask, createRedEyeMask, createLinearGradientMask, addMask, createLuminanceRangeMask, removeMask, rgbToHsl, upsertSplitToningZone, nearestHslBand, computeEyedropperWhiteBalance, upsertOp, sampleCurveLut, buildToneCurveLut, insertToneCurvePoint, upsertToneCurve } from "$lib/api/develop.js";
import { developView } from "$lib/state/developView.svelte.js";

// HSL band-jump eyedropper's transient navigation target -- NOT persisted
// edit-stack state, purely a "which band should the panel scroll to and
// highlight" signal, self-clearing after a fixed delay rather than on
// "the next unrelated interaction" (which would mean hooking an unbounded
// set of DOM listeners across the panel). Same fixed-timeout-reset-on-
// retrigger idiom as persistTimer's own debounce, just for UI feedback
// instead of persistence.
let hslBandHighlightTimer = /** @type {ReturnType<typeof setTimeout> | null} */ (null);

export function handleGpuFallback(/** @type {boolean} */ active) {
  develop.gpuFallbackActive = active;
  // A mask/crop tool selected before GPU became unavailable would
  // otherwise linger as "active" while its own panel/handles never
  // render (MaskToolStrip's buttons are disabled going forward, but
  // this clears whatever was already selected).
  if (active) masks.activeTool = null;
}

/** Toggle symmetry with MaskToolStrip's own onToolToggle: clicking the
 * eyedropper again while already resampling cancels it, matching how
 * clicking an active tool button a second time turns it off. */
export function handleResampleColorToggle() {
  if (masks.isResamplingColor) {
    masks.activeTool = null;
    masks.colorRangeResampleTarget = null;
    return;
  }
  if (masks.selectedMaskId === null) return;
  masks.activeTool = "color_range";
  masks.colorRangeResampleTarget = masks.selectedMaskId;
}

/** Commit path for a re-sample click -- patches the EXISTING mask
 * (unlike handleMaskCreated's color_range branch, which always adds a
 * new one) and, unlike the generic handleMaskUpdated slider path, also
 * exits resample mode afterward -- a re-sample is a one-shot action,
 * matching real Lightroom's own "click to pick, done" model for this
 * tool, not a mode you stay in. */
export function handleColorRangeResampled(
  /** @type {string} */ id,
  /** @type {{r: number, g: number, b: number}} */ refColor,
) {
  develop.editStack = updateMask(develop.editStack, id, { refColor });
  masks.colorRangeResampleTarget = null;
  masks.activeTool = null;
  develop.scheduleFlush("Adjust Color Range");
}

export function isEyedropperActive(/** @type {typeof masks.eyedropperTarget} */ target) {
  return masks.activeTool === "eyedropper" && masks.eyedropperTarget === target;
}

/** Toggle symmetry with handleResampleColorToggle above: clicking an
 * active eyedropper button again cancels it. */
export function handleEyedropperToggle(/** @type {typeof masks.eyedropperTarget} */ target) {
  if (masks.activeTool === "eyedropper" && masks.eyedropperTarget === target) {
    masks.activeTool = null;
    masks.eyedropperTarget = null;
    return;
  }
  masks.activeTool = "eyedropper";
  masks.eyedropperTarget = target;
}

export function handleMaskCreated(
  /** @type {
   *   | { kind: "linear_gradient", start: {x:number,y:number}, end: {x:number,y:number} }
   *   | { kind: "radial_gradient", center: {x:number,y:number}, radiusX: number, radiusY: number }
   *   | { kind: "brush", id: string }
   *   | { kind: "color_range", refColor: {r:number,g:number,b:number} }
   *   | { kind: "spot", id: string, initialDab: {x:number,y:number,radius:number} }
   *   | { kind: "red_eye", center: {x:number,y:number}, radiusX: number, radiusY: number }
   * } */ placement,
) {
  // Every kind gets its own explicit branch before the final
  // createLinearGradientMask fallback (not appended after it) -- an
  // untyped fallback assuming "unrecognized = linear" is a real bug
  // class already shipped and fixed once elsewhere in this codebase
  // (DevelopCanvas.svelte's mask-packing loop); a color-range placement
  // has no `.start`/`.end` at all, so hitting this fallback by mistake
  // would construct a broken linear mask and crash later.
  const mask =
    placement.kind === "radial_gradient"
      ? createRadialGradientMask(placement.center, placement.radiusX, placement.radiusY)
      : placement.kind === "brush"
        ? createBrushMask(placement.id)
        : placement.kind === "color_range"
          ? createColorRangeMask(placement.refColor)
          : placement.kind === "spot"
            ? createSpotMask(placement.initialDab, placement.id)
            : placement.kind === "red_eye"
              ? createRedEyeMask(placement.center, placement.radiusX, placement.radiusY)
              : createLinearGradientMask(placement.start, placement.end);
  develop.editStack = addMask(develop.editStack, mask);
  masks.selectedMaskId = mask.id;
  // Real Lightroom drops back to selection after placing a gradient, but
  // a brush stroke should keep the Brush tool active (painting is
  // inherently multi-stroke -- see DevelopCanvas.svelte's brush-state
  // doc comment) rather than force a re-click of the tool for every dab.
  // Spot removal is now also a painted stroke (M4 Slice 2), so it stays
  // active the same way; only color range and the gradients are one-shot
  // placements that fall through the `!== "brush"` reset below.
  if (placement.kind !== "brush" && placement.kind !== "spot") masks.activeTool = null;
  const label =
    placement.kind === "radial_gradient"
      ? "Add Radial Gradient"
      : placement.kind === "brush"
        ? "Add Brush Mask"
        : placement.kind === "color_range"
          ? "Add Color Range Mask"
          : placement.kind === "spot"
            ? "Add Spot Removal"
            : placement.kind === "red_eye"
              ? "Add Red Eye Correction"
              : "Add Linear Gradient";
  develop.scheduleFlush(label);
}

// Luminance range has no geometry to place, so it doesn't go through
// handleMaskCreated's placement-dispatch shape at all -- MaskToolStrip's
// button calls this directly (real Lightroom's own behavior: this mask
// kind is created on tool-select, no canvas interaction needed).
export function handleCreateLuminanceRangeMask() {
  const mask = createLuminanceRangeMask();
  develop.editStack = addMask(develop.editStack, mask);
  masks.selectedMaskId = mask.id;
  develop.scheduleFlush("Add Luminance Range Mask");
}

export function handleMaskUpdated(/** @type {string} */ id, /** @type {Record<string, unknown>} */ patch) {
  develop.editStack = updateMask(develop.editStack, id, patch);
  develop.scheduleFlush("Edit Mask");
}

export function handleMaskDeleted() {
  if (masks.selectedMaskId === null) return;
  develop.editStack = removeMask(develop.editStack, masks.selectedMaskId);
  masks.selectedMaskId = null;
  develop.flushEditStack("Delete Mask");
}

/** Commit path for all four eyedropper destinations -- see
 * `masks.eyedropperTarget`'s own doc comment for why one shared gesture
 * routes here. One-shot: resets activeTool/eyedropperTarget immediately,
 * matching handleColorRangeResampled's own "click to pick, done" model. */
export function handleEyedropperSampled(/** @type {{r: number, g: number, b: number}} */ color) {
  const target = masks.eyedropperTarget;
  masks.activeTool = null;
  masks.eyedropperTarget = null;
  if (target === null) return;
  const { h, s, l } = rgbToHsl(color.r, color.g, color.b);

  if (target === "split_toning_shadows" || target === "split_toning_highlights") {
    const zone = target === "split_toning_shadows" ? "shadows" : "highlights";
    develop.editStack = upsertSplitToningZone(develop.editStack, zone, { hue: h, saturation: s * 100 });
    develop.scheduleFlush("Split Toning");
    return;
  }
  if (target === "hsl_band") {
    // Navigation only -- deliberately no editStack write, no persist.
    // HSL's own sliders are relative hue/sat/lum shifts, not an absolute
    // color a sampled pixel could set; this just finds "which band".
    develop.highlightedHslBand = nearestHslBand(h);
    if (hslBandHighlightTimer) clearTimeout(hslBandHighlightTimer);
    hslBandHighlightTimer = setTimeout(() => (develop.highlightedHslBand = null), 1500);
    return;
  }
  if (target === "white_balance") {
    const { temperature, tint } = computeEyedropperWhiteBalance(color);
    develop.editStack = upsertOp(develop.editStack, "temperature", temperature);
    develop.editStack = upsertOp(develop.editStack, "tint", tint);
    develop.scheduleFlush("White Balance Eyedropper");
    return;
  }
  if (target === "tone_curve_point") {
    // x comes from the sampled pixel's own lightness. Note: like every
    // eyedropper here, this samples the ORIGINAL SOURCE pixel, not the
    // graded preview (see DevelopCanvas.svelte's sampleSourcePixel doc
    // comment) -- for Tone Curve specifically this means the inserted
    // point's x itself (not just a selectivity parameter, as for the
    // other three destinations) can visibly diverge from "the tone the
    // user thinks they clicked" on a heavily-graded image. A named,
    // accepted limitation, not a bug.
    //
    // y is seeded at the curve's OWN current value at that x, so
    // insertion alone never changes the curve's visible shape until the
    // new point is dragged.
    const y = sampleCurveLut(buildToneCurveLut(developView.toneCurvePoints), l);
    const next = insertToneCurvePoint(developView.toneCurvePoints, l, y);
    if (next !== developView.toneCurvePoints) {
      develop.editStack = upsertToneCurve(develop.editStack, next);
      develop.scheduleFlush("Tone Curve");
    }
  }
}
