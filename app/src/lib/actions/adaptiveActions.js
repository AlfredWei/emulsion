// Applying an adaptive preset to the open Develop photo (RFC-0035 §3.3): resolve each rule's region in Rust, turn
// the found ones into masks scaled by Amount, and write them as ONE labelled flush (one History row, one undo).
// A mask is never re-resolved by opening the photo later; applying again is the explicit "Re-apply".

import { develop } from "$lib/state/develop.svelte.js";
import { library } from "$lib/state/library.svelte.js";
import { shell } from "$lib/state/shell.svelte.js";
import { masks } from "$lib/state/masks.svelte.js";
import { resolveAdaptiveRegion } from "$lib/api/adaptive.js";
import { regenerateThumbnailFor } from "$lib/actions/importActions.js";
import { runFaceDetection } from "$lib/actions/faceActions.js";
import { ADAPTIVE_PRESETS, needsFaces, planAdaptiveApply, describeReport, adaptiveLabel } from "$lib/adaptivePresets.js";

let running = false;

/** @param {string} presetId @param {number} amount 0-100
 * @param {{ replaceEdited?: boolean }} [options] `replaceEdited`: the user agreed to replace a mask they had changed
 * @returns {Promise<import('$lib/adaptivePresets.js').RuleReport[] | null>} the per-rule report, or null when nothing ran */
export async function handleApplyAdaptivePreset(presetId, amount, options = {}) {
  const preset = ADAPTIVE_PRESETS.find((p) => p.id === presetId);
  const versionId = develop.versionId;
  if (!preset || versionId === null || running) return null;
  const image = library.images.find((img) => img.version_id === versionId);
  const contentHash = develop.imageContentHash;
  // Captured with the hash: a photo switch mid-run must not pair this photo's id and hash with the next one's path.
  const imagePath = develop.imagePath;
  if (!image || !contentHash || !imagePath) return null;
  running = true;
  develop.clearPreview();
  try {
    if (needsFaces(preset) && !image.faces_scanned) await runFaceDetection([image.image_id], false);
    const results = [];
    for (const rule of preset.rules) results.push(await resolveAdaptiveRegion(image.image_id, imagePath, contentHash, rule.region));
    // The user may have moved to another photo while Rust was looking: never write one photo's masks onto another.
    if (develop.versionId !== versionId) {
      shell.notify(`${preset.name}: the photo changed while it was being analysed, nothing applied`);
      return null;
    }
    const plan = planAdaptiveApply(develop.editStack, preset, results, amount, options.replaceEdited === true);
    if (plan.changed) {
      const before = develop.editStack;
      develop.editStack = plan.stack;
      try {
        await develop.flushEditStack(adaptiveLabel(preset, amount));
      } catch (e) {
        // Not persisted: do not leave masks on the canvas that the catalog never got.
        if (develop.versionId === versionId) develop.editStack = before;
        throw e;
      }
      regenerateThumbnailFor(versionId);
      // Show what was found: the user cannot tell what a resolver selected until the mask is open with its overlay on.
      if (develop.versionId === versionId && plan.added.length > 0) {
        masks.activeTool = null;
        masks.showMaskOverlay = true;
        masks.selectedMaskId = plan.added[0];
      }
    }
    shell.notify(`${preset.name} ${amount} %: ${plan.changed ? "" : "nothing changed. "}${describeReport(plan.report)}`);
    return plan.report;
  } catch (/** @type {any} */ e) {
    shell.notify(`${preset.name} failed: ${e}`);
    return null;
  } finally {
    running = false;
  }
}
