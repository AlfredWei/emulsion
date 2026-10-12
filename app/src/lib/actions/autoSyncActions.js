// Auto Sync (RFC-0028 §3.5): while the switch is on, each edit of the open Develop photo is written into the other
// selected photos -- only the ops that changed, with their absolute values, never the per-photo ones. The writes
// ride on the open photo's flush (`develop.autoSyncHook`), so a slider drag writes once per photo, not per frame.

import { presets } from "$lib/state/presets.svelte.js";
import { develop } from "$lib/state/develop.svelte.js";
import { selection } from "$lib/state/selection.svelte.js";
import { shell } from "$lib/state/shell.svelte.js";
import { applyOpsToVersions, regenerateThumbnail } from "$lib/api/develop.js";
import { diffSyncableOps } from "$lib/autoSync.js";
import { patchLocal } from "$lib/actions/libraryActions.js";

/** Switching Auto Sync on with more photos selected than this asks first: one edit means one write and one
 * thumbnail per photo. */
export const AUTO_SYNC_CONFIRM_THRESHOLD = 50;

/** The other selected photos an edit of `versionId` goes to now: none while Auto Sync is off or the edit is not on a
 * selected photo.
 * @param {number} versionId @returns {number[]} */
export function autoSyncTargets(versionId) {
  if (!presets.autoSyncOn || !selection.selectedIds.has(versionId)) return [];
  return [...selection.selectedIds].filter((id) => id !== versionId);
}

/** The switch: off -> on (after a confirmation for a large selection) and on -> off. Inert with fewer than two photos. */
export function handleAutoSyncToggle() {
  if (presets.autoSyncOn) {
    presets.autoSyncOn = false;
    return;
  }
  if (develop.versionId === null || selection.selectedIds.size < 2) return;
  if (selection.selectedIds.size > AUTO_SYNC_CONFIRM_THRESHOLD) {
    presets.confirmingAutoSync = true;
    return;
  }
  presets.autoSyncOn = true;
}

export function handleAutoSyncConfirmed() {
  presets.confirmingAutoSync = false;
  if (develop.versionId !== null && selection.selectedIds.size >= 2) presets.autoSyncOn = true;
}

/** Auto Sync never outlives its reasons: fewer than two photos selected, or Develop left. Says so when the selection
 * is the reason, because an edit that silently stopped syncing is the other way this feature can surprise. */
export function reconcileAutoSync() {
  const inDevelop = shell.activeModule === "develop";
  const enough = selection.selectedIds.size >= 2;
  if (!inDevelop || !enough) presets.confirmingAutoSync = false;
  if (!presets.autoSyncOn || (inDevelop && enough)) return;
  presets.autoSyncOn = false;
  if (inDevelop) shell.notify("Auto Sync is off: only one photo is selected");
}

/** After the open photo's edit was written: copy the changed ops to the targets in one catalog transaction, then
 * refresh their thumbnails. A failure is reported once and leaves the open photo's own edit alone.
 * @param {number} versionId @param {number[]} targets
 * @param {import('$lib/api/develop.js').EditStack} previous @param {import('$lib/api/develop.js').EditStack} next
 * @param {string} label */
export async function autoSyncAfterFlush(versionId, targets, previous, next, label) {
  const { ops, removed } = diffSyncableOps(previous, next);
  if (ops.length === 0 && removed.length === 0) return;
  try {
    const outcome = await applyOpsToVersions(targets, ops, removed, `Auto Sync: ${label}`);
    if (outcome.missing.length > 0) {
      const n = outcome.missing.length;
      shell.notify(`Auto Sync skipped ${n} photo${n === 1 ? "" : "s"} no longer in the catalog`);
    }
    await Promise.all(
      outcome.changed.map(async (id) => {
        const path = await regenerateThumbnail(id);
        if (path) patchLocal(id, { thumbnail_path: path });
      }),
    );
  } catch (/** @type {any} */ e) {
    shell.notify(`Auto Sync failed: ${e}`);
  }
}

/** Connects Auto Sync to the Develop store's flush. Called once at startup. */
export function installAutoSync() {
  develop.autoSyncHook = { targets: autoSyncTargets, onFlushed: autoSyncAfterFlush };
}
