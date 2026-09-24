// Develop edits that touch only the `develop` store (RFC-0009 P5b, moved out of +page.svelte's script):
// the per-adjustment change handlers (each upserts its op into the edit stack and schedules the
// debounced, labeled flush), crop and white-balance/tone helpers, the readouts DevelopCanvas reports
// up, history/snapshot hover previews and snapshot create/delete. Workflows that also write the mask
// selection (`restoreTo`, `openDevelop`, mask create/delete, reset) move after the `masks` store.

import { develop } from "$lib/state/develop.svelte.js";
import {
  upsertOp,
  upsertToneCurve,
  upsertHslBand,
  upsertSplitToningZone,
  upsertSplitToningBalance,
  upsertVignette,
  upsertLensCorrection,
  upsertPerspective,
  upsertGrain,
  upsertSharpen,
  upsertLumaNr,
  upsertColorNr,
  upsertCrop,
  IDENTITY_CROP,
  computeAutoWhiteBalance,
  WB_PRESETS,
  computeAutoTone,
  previewHistoryEntry,
  previewSnapshot,
  addSnapshot,
  deleteSnapshot,
  togglePanelVisibility,
  resetPanel,
} from "$lib/api/develop.js";
import { developView } from "$lib/state/developView.svelte.js";
import { inscribedCropForAngle, cropRectFitsRotatedBounds } from "$lib/cropMath.js";

// Human-readable History labels for handleAdjustmentChange's generic
// single-scalar ops -- falls back to the raw opName (still readable
// enough, e.g. "vibrance") for any op added later without a mapping
// entry, rather than needing this list kept in lockstep with every op.
const ADJUSTMENT_LABELS = /** @type {Record<string, string>} */ ({
  exposure: "Exposure",
  contrast: "Contrast",
  saturation: "Saturation",
  temperature: "Temperature",
  tint: "Tint",
  highlights: "Highlights",
  shadows: "Shadows",
  whites: "Whites",
  blacks: "Blacks",
  dehaze: "Dehaze",
  texture: "Texture",
  clarity: "Clarity",
});

export function handleAdjustmentChange(/** @type {string} */ opName, /** @type {number} */ value) {
  develop.editStack = upsertOp(develop.editStack, opName, value);
  develop.scheduleFlush(ADJUSTMENT_LABELS[opName] ?? opName);
}

// RFC-0013: human-readable History labels for the 12 op-bearing panels'
// visibility/reset actions -- same fallback-to-key shape ADJUSTMENT_LABELS
// uses above.
const PANEL_LABELS = /** @type {Record<string, string>} */ ({
  basic: "Basic",
  tone_curve: "Tone Curve",
  hsl: "HSL / Color Mixer",
  split_toning: "Split Toning",
  texture_clarity: "Texture & Clarity",
  dehaze: "Dehaze",
  sharpening: "Sharpening",
  noise_reduction: "Noise Reduction",
  vignette: "Vignette",
  grain: "Grain",
  lens_corrections: "Lens Corrections",
  perspective: "Perspective",
});

/** Toggles a panel's own visibility (RFC-0013) -- independent of its
 * values, which are never touched here. */
export function handleTogglePanelVisibility(/** @type {string} */ panel) {
  develop.editStack = togglePanelVisibility(develop.editStack, panel);
  develop.scheduleFlush(`${PANEL_LABELS[panel] ?? panel} Visibility`);
}

/** Reverts just one panel's own values to default (RFC-0013) -- never
 * touches that panel's visibility marker. */
export function handleResetPanel(/** @type {string} */ panel) {
  develop.editStack = resetPanel(develop.editStack, panel);
  develop.scheduleFlush(`Reset ${PANEL_LABELS[panel] ?? panel}`);
}

export function handleToneCurveChange(/** @type {readonly {x: number, y: number}[]} */ points) {
  develop.editStack = upsertToneCurve(develop.editStack, points);
  develop.scheduleFlush("Tone Curve");
}

export function handleHslBandChange(
  /** @type {string} */ bandName,
  /** @type {Partial<{hue: number, saturation: number, luminance: number}>} */ patch,
) {
  develop.editStack = upsertHslBand(develop.editStack, bandName, patch);
  develop.scheduleFlush("HSL / Color Mixer");
}

export function handleSplitToningZoneChange(
  /** @type {"shadows" | "highlights"} */ zone,
  /** @type {Partial<{hue: number, saturation: number}>} */ patch,
) {
  develop.editStack = upsertSplitToningZone(develop.editStack, zone, patch);
  develop.scheduleFlush("Split Toning");
}

export function handleSplitToningBalanceChange(/** @type {number} */ balance) {
  develop.editStack = upsertSplitToningBalance(develop.editStack, balance);
  develop.scheduleFlush("Split Toning");
}

export function handleVignetteChange(
  /** @type {Partial<{amount: number, midpoint: number, feather: number}>} */ patch,
) {
  develop.editStack = upsertVignette(develop.editStack, patch);
  develop.scheduleFlush("Vignette");
}

export function handleLensCorrectionChange(
  /** @type {Partial<{profile_enabled: boolean, distortion_amount: number, vignette_amount: number, ca_amount: number, manual_distortion: number, manual_ca: number}>} */ patch,
) {
  develop.editStack = upsertLensCorrection(develop.editStack, patch);
  develop.scheduleFlush("Lens Corrections");
}

export function handlePerspectiveChange(
  /** @type {Partial<{vertical: number, horizontal: number, rotate: number, aspect: number, scale: number}>} */ patch,
) {
  develop.editStack = upsertPerspective(develop.editStack, patch);
  develop.scheduleFlush("Perspective");
}

export function handleGrainChange(
  /** @type {Partial<{amount: number, size: number, roughness: number}>} */ patch,
) {
  develop.editStack = upsertGrain(develop.editStack, patch);
  develop.scheduleFlush("Grain");
}

export function handleSharpenChange(
  /** @type {Partial<{amount: number, radius: number, detail: number, masking: number}>} */ patch,
) {
  develop.editStack = upsertSharpen(develop.editStack, patch);
  develop.scheduleFlush("Sharpening");
}

export function handleLumaNRChange(
  /** @type {Partial<{amount: number, detail: number, contrast: number}>} */ patch,
) {
  develop.editStack = upsertLumaNr(develop.editStack, patch);
  develop.scheduleFlush("Luminance Noise Reduction");
}

export function handleColorNRChange(
  /** @type {Partial<{amount: number, detail: number}>} */ patch,
) {
  develop.editStack = upsertColorNr(develop.editStack, patch);
  develop.scheduleFlush("Color Noise Reduction");
}

/** Ordinary field patches (drag/resize handles) pass through unchanged.
 * An ANGLE-only patch (the straighten slider, see `onCropAngleChange`
 * below) is special-cased: real Lightroom re-fits the crop rect to the
 * largest inner-fit box of the SAME aspect ratio for the new angle,
 * recentered on the image, rather than leaving the old rect in place to
 * expose the newly-rotated image's blanked-out corners (see
 * `inscribedCropForAngle`'s own doc comment for the geometry and its
 * "centered-only" scope cut -- this is that function's one caller). */
export function handleCropChange(
  /** @type {Partial<{x: number, y: number, width: number, height: number, angle: number}>} */ patch,
) {
  let next = patch;
  if (typeof patch.angle === "number" && patch.angle !== developView.crop.angle && developView.crop.width > 0 && developView.crop.height > 0) {
    const pixelRatio = develop.sourceWidth > 0 && develop.sourceHeight > 0 ? (developView.crop.width * develop.sourceWidth) / (developView.crop.height * develop.sourceHeight) : null;
    const inscribed = pixelRatio ? inscribedCropForAngle(pixelRatio, develop.sourceWidth, develop.sourceHeight, patch.angle) : null;
    if (inscribed) next = { ...inscribed, angle: patch.angle };
  }
  // Last-resort guard against ever committing a rect that exposes the
  // rotated image's blanked-out corners (see DevelopCanvas.svelte's own
  // matching drag-time check, which is what actually stops this in the
  // interactive path -- this is the safety net for every OTHER caller of
  // handleCropChange, e.g. a future one that doesn't go through that
  // drag code at all). Falls back to the full merged rect against
  // `crop`, since `next` may be a partial patch.
  const merged = { x: developView.crop.x, y: developView.crop.y, width: developView.crop.width, height: developView.crop.height, angle: developView.crop.angle, ...next };
  if (develop.sourceWidth > 0 && develop.sourceHeight > 0 && !cropRectFitsRotatedBounds(merged, develop.sourceWidth, develop.sourceHeight, merged.angle)) {
    return;
  }
  develop.editStack = upsertCrop(develop.editStack, next);
  develop.scheduleFlush("Crop");
}

/** Reshapes the crop rect to the given PIXEL aspect ratio: the largest
 * rect of that ratio centered in the full image, INNER-FIT to the
 * current straighten angle (see `inscribedCropForAngle`'s doc comment --
 * at angle 0 it's identical to `largestCenteredCropForRatio`, so this
 * covers that case too without a branch). Deliberately NOT based on the
 * current rect's own size -- earlier it shrunk the current rect to fit
 * within its own previous bounding box, which (combined with the
 * uncorrected ratio) compounded into a smaller rect on every click.
 * Recomputing fresh from the full image each time is idempotent:
 * clicking the same preset twice in a row is always a no-op. `null`
 * just unlocks without reshaping anything ("Free"). */
export function handleCropAspectPreset(/** @type {number | null} */ ratio) {
  develop.cropAspectLock = ratio;
  if (ratio === null) return;
  const next = inscribedCropForAngle(ratio, develop.sourceWidth, develop.sourceHeight, developView.crop.angle);
  if (!next) return;
  handleCropChange({ ...next, angle: developView.crop.angle });
}

export function handleCropReset() {
  develop.cropAspectLock = null;
  handleCropChange(IDENTITY_CROP);
}

export function handleAutoWhiteBalance() {
  let avgRgb = { r: 0.5, g: 0.5, b: 0.5 };
  if (develop.histogramData) {
    let rSum = 0,
      gSum = 0,
      bSum = 0,
      count = 0;
    for (let i = 0; i < 256; i++) {
      rSum += develop.histogramData.r[i] * (i / 255);
      gSum += develop.histogramData.g[i] * (i / 255);
      bSum += develop.histogramData.b[i] * (i / 255);
      count += develop.histogramData.r[i];
    }
    if (count > 0) {
      avgRgb = { r: rSum / count, g: gSum / count, b: bSum / count };
    }
  }
  const { temperature, tint } = computeAutoWhiteBalance(avgRgb);
  develop.editStack = upsertOp(develop.editStack, "temperature", temperature);
  develop.editStack = upsertOp(develop.editStack, "tint", tint);
  develop.scheduleFlush("Auto White Balance");
}

export function handleWbPresetChange(/** @type {string} */ presetKey) {
  if (presetKey === "auto") {
    handleAutoWhiteBalance();
    return;
  }
  const preset = WB_PRESETS[/** @type {keyof typeof WB_PRESETS} */ (presetKey)];
  if (!preset) return;
  develop.editStack = upsertOp(develop.editStack, "temperature", preset.temperature);
  develop.editStack = upsertOp(develop.editStack, "tint", preset.tint);
  develop.scheduleFlush(`WB Profile: ${preset.name}`);
}

export function handleAutoTone() {
  if (!develop.histogramData) return;
  const tone = computeAutoTone(develop.histogramData);
  develop.editStack = upsertOp(develop.editStack, "exposure", tone.exposure);
  develop.editStack = upsertOp(develop.editStack, "contrast", tone.contrast);
  develop.editStack = upsertOp(develop.editStack, "highlights", tone.highlights);
  develop.editStack = upsertOp(develop.editStack, "shadows", tone.shadows);
  develop.editStack = upsertOp(develop.editStack, "whites", tone.whites);
  develop.editStack = upsertOp(develop.editStack, "blacks", tone.blacks);
  develop.scheduleFlush("Auto Tone");
}

export function handleSourceDimensions(/** @type {number} */ width, /** @type {number} */ height) {
  develop.sourceWidth = width;
  develop.sourceHeight = height;
}

export function handleHistogramUpdate(/** @type {{r: Uint32Array, g: Uint32Array, b: Uint32Array}} */ data) {
  develop.histogramData = data;
}

export function handleToggleClippingOverlay() {
  develop.showClippingOverlay = !develop.showClippingOverlay;
}

export function handleHoverPixel(/** @type {{r: number, g: number, b: number} | null} */ rgb) {
  develop.hoverPixel = rgb;
}

export function handlePeekHistory(/** @type {number} */ index) {
  if (develop.versionId === null || develop.imagePath === null || index < 0 || index >= develop.history.length) return;
  const versionId = develop.versionId;
  const entryId = develop.history[index].id;
  const path = develop.imagePath;
  const contentHash = develop.imageContentHash;
  develop.schedulePreview(() => previewHistoryEntry(versionId, entryId, path, contentHash));
}

export function handlePeekSnapshot(/** @type {number} */ snapshotId) {
  if (develop.versionId === null || develop.imagePath === null) return;
  const versionId = develop.versionId;
  const path = develop.imagePath;
  const contentHash = develop.imageContentHash;
  develop.schedulePreview(() => previewSnapshot(versionId, snapshotId, path, contentHash));
}

/** Creates a named save point from whatever's CURRENTLY on screen --
 * flushes any pending debounced edit first so the snapshot never misses
 * the last slider tick. */
export async function handleCreateSnapshot(/** @type {string} */ name) {
  if (develop.versionId === null) return;
  await develop.flushEditStack();
  const versionId = develop.versionId;
  const snapshot = await addSnapshot(versionId, name);
  develop.snapshots = [...develop.snapshots, snapshot];
}

export async function handleDeleteSnapshot(/** @type {number} */ snapshotId) {
  if (develop.versionId === null) return;
  await deleteSnapshot(develop.versionId, snapshotId);
  develop.snapshots = develop.snapshots.filter((s) => s.id !== snapshotId);
}
