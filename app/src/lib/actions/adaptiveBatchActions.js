// Library "Apply Adaptive Preset to Selection" (RFC-0035 B3): the same resolve-and-write as the Develop apply, over the
// selected photos one by one, off the UI thread (the resolvers are Rust commands), with a report that lists EVERY photo:
// applied, flagged for a look, nothing found, kept because the user had changed the mask, or failed. Nothing is hidden,
// a mask the user changed is never replaced here (a batch cannot ask per photo), and each photo gets its own History row
// and its own undo, like Sync Settings does.

import { adaptiveBatch } from "$lib/state/adaptiveBatch.svelte.js";
import { develop } from "$lib/state/develop.svelte.js";
import { library } from "$lib/state/library.svelte.js";
import { selection } from "$lib/state/selection.svelte.js";
import { shell } from "$lib/state/shell.svelte.js";
import { resolveAdaptiveRegion } from "$lib/api/adaptive.js";
import { getEditStack, setEditStack, regenerateThumbnail } from "$lib/api/develop.js";
import { runFaceDetection } from "$lib/actions/faceActions.js";
import { patchLocal } from "$lib/actions/libraryActions.js";
import { ADAPTIVE_PRESETS, needsFaces, planAdaptiveApply, adaptiveLabel, batchStatus } from "$lib/adaptivePresets.js";

const nameOf = (/** @type {string} */ path) => path.split(/[\\/]/).pop() ?? path;

/** Opens the dialog on the Amount question for the current selection. */
export function handleOpenAdaptiveBatch(/** @type {string} */ presetId) {
  if (!ADAPTIVE_PRESETS.some((p) => p.id === presetId) || selection.selectedIds.size === 0 || adaptiveBatch.phase === "running") return;
  adaptiveBatch.presetId = presetId;
  adaptiveBatch.targets = [...selection.selectedIds];
  adaptiveBatch.rows = [];
  adaptiveBatch.done = 0;
  adaptiveBatch.cancelRequested = false;
  adaptiveBatch.phase = "confirm";
  adaptiveBatch.open = true;
}

export function handleCloseAdaptiveBatch() {
  if (adaptiveBatch.phase === "running") return;
  adaptiveBatch.open = false;
}

/** Stops after the photo being worked on; the report lists the photos done so far and says how many were left. */
export function handleCancelAdaptiveBatch() {
  adaptiveBatch.cancelRequested = true;
}

/** @param {import('$lib/api/catalog.js').ImageSummary} img @param {import('$lib/adaptivePresets.js').AdaptivePreset} preset @param {number} amount
 * @returns {Promise<import('$lib/adaptivePresets.js').BatchRow>} */
async function applyToPhoto(img, preset, amount) {
  const name = nameOf(img.path);
  try {
    if (!img.content_hash) return { versionId: img.version_id, name, status: "failed", report: [], error: "the photo has no content hash yet" };
    if (needsFaces(preset) && !library.images.find((i) => i.version_id === img.version_id)?.faces_scanned) {
      return { versionId: img.version_id, name, status: "failed", report: [], error: "faces could not be scanned in this photo" };
    }
    const results = [];
    for (const rule of preset.rules) results.push(await resolveAdaptiveRegion(img.image_id, img.path, img.content_hash, rule.region));
    const stack = await getEditStack(img.version_id);
    const plan = planAdaptiveApply(stack, preset, results, amount, false);
    if (plan.changed) {
      await setEditStack(img.version_id, plan.stack, adaptiveLabel(preset, amount));
      const thumb = await regenerateThumbnail(img.version_id);
      if (thumb) patchLocal(img.version_id, { thumbnail_path: thumb });
    }
    return { versionId: img.version_id, name, status: batchStatus(plan.report), report: plan.report };
  } catch (/** @type {any} */ e) {
    return { versionId: img.version_id, name, status: "failed", report: [], error: String(e) };
  }
}

/** Runs the batch for the photos fixed when the dialog opened. */
export async function handleRunAdaptiveBatch() {
  const preset = ADAPTIVE_PRESETS.find((p) => p.id === adaptiveBatch.presetId);
  if (!preset || adaptiveBatch.phase !== "confirm") return;
  const amount = adaptiveBatch.amount;
  const images = adaptiveBatch.targets.map((v) => library.images.find((i) => i.version_id === v)).filter((i) => i !== undefined);
  adaptiveBatch.phase = "running";
  adaptiveBatch.done = 0;
  adaptiveBatch.rows = [];
  // The open Develop photo, if it is one of the targets, writes its own pending edits first and reads the result back
  // after: otherwise its later flush would overwrite what this batch wrote with the stale in-memory stack.
  const openIsTarget = develop.versionId !== null && adaptiveBatch.targets.includes(develop.versionId);
  try {
    if (openIsTarget) await develop.flushEditStack();
    if (needsFaces(preset)) {
      const unscanned = images.filter((i) => !i.faces_scanned).map((i) => i.image_id);
      if (unscanned.length > 0) await runFaceDetection(unscanned, false);
    }
    for (const img of images) {
      if (adaptiveBatch.cancelRequested) break;
      adaptiveBatch.rows = [...adaptiveBatch.rows, await applyToPhoto(img, preset, amount)];
      adaptiveBatch.done = adaptiveBatch.rows.length;
    }
    if (openIsTarget && develop.versionId !== null) develop.editStack = await getEditStack(develop.versionId);
  } finally {
    adaptiveBatch.phase = "report";
  }
  const rows = adaptiveBatch.rows;
  const left = images.length - rows.length;
  const flagged = rows.filter((r) => r.status === "check").length;
  shell.notify(`${preset.name}: ${rows.filter((r) => r.status === "applied" || r.status === "check").length} of ${images.length} photos${flagged ? `, ${flagged} to check` : ""}${left > 0 ? `, ${left} not reached` : ""}`);
}

/** Makes the photos of one kind of report row the Library selection, so the user can go through them. */
export function handleSelectBatchRows(/** @type {import('$lib/adaptivePresets.js').BatchRow["status"][]} */ statuses) {
  const ids = adaptiveBatch.rows.filter((r) => statuses.includes(r.status)).map((r) => r.versionId);
  if (ids.length === 0) return;
  selection.selectedIds = new Set(ids);
  selection.selectedId = ids[0];
  adaptiveBatch.open = false;
}
