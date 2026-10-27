// Applying an adaptive preset to the open Develop photo (RFC-0035 §3.3): resolve each rule's region in Rust, turn
// the found ones into masks scaled by Amount, and write them as ONE labelled flush (one History row, one undo).
// A mask is never re-resolved by opening the photo later; applying again is the explicit "Re-apply".

import { develop } from "$lib/state/develop.svelte.js";
import { library } from "$lib/state/library.svelte.js";
import { shell } from "$lib/state/shell.svelte.js";
import { resolveAdaptiveRegion } from "$lib/api/adaptive.js";
import { regenerateThumbnailFor } from "$lib/actions/importActions.js";
import { runFaceDetection } from "$lib/actions/faceActions.js";
import { ADAPTIVE_PRESETS, planAdaptiveApply, describeReport, adaptiveLabel } from "$lib/adaptivePresets.js";

let running = false;

/** @param {string} presetId @param {number} amount 0-100
 * @returns {Promise<import('$lib/adaptivePresets.js').RuleReport[] | null>} the per-rule report, or null when nothing ran */
export async function handleApplyAdaptivePreset(presetId, amount) {
  const preset = ADAPTIVE_PRESETS.find((p) => p.id === presetId);
  const versionId = develop.versionId;
  if (!preset || versionId === null || running) return null;
  const image = library.images.find((img) => img.version_id === versionId);
  const contentHash = develop.imageContentHash;
  if (!image || !contentHash) return null;
  running = true;
  develop.clearPreview();
  try {
    if (preset.rules.some((r) => r.region === "face") && !image.faces_scanned) await runFaceDetection([image.image_id], false);
    const results = [];
    for (const rule of preset.rules) results.push(await resolveAdaptiveRegion(image.image_id, develop.imagePath, contentHash, rule.region));
    // The user may have moved to another photo while Rust was looking: never write one photo's masks onto another.
    if (develop.versionId !== versionId) {
      shell.notify(`${preset.name}: the photo changed while it was being analysed, nothing applied`);
      return null;
    }
    const plan = planAdaptiveApply(develop.editStack, preset, results, amount);
    if (plan.changed) {
      develop.editStack = plan.stack;
      await develop.flushEditStack(adaptiveLabel(preset, amount));
      regenerateThumbnailFor(versionId);
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
