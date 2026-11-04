// Object removal (RFC-0032 slice E): arming the tool, the marks, running the job, and writing its patches into the
// edit stack. The job is the backend's (`remove_object`); what lands in the stack is one `remove` op whose items name
// the stored patches (api/develop.js), written like any edit so it gets a history row and can be undone.

import { develop } from "$lib/state/develop.svelte.js";
import { masks } from "$lib/state/masks.svelte.js";
import { removal, LARGE_MARK_FRACTION } from "$lib/state/removal.svelte.js";
import { shell } from "$lib/state/shell.svelte.js";
import { regenerateThumbnailFor } from "$lib/actions/importActions.js";
import { removalAvailable, removeObject, removalCancel, isRemovalCancelled } from "$lib/api/removal.js";
import { addRemovals, getEditStack, getRemovals, removeRemoval, setEditStack } from "$lib/api/develop.js";

/** @param {unknown} e */
function message(e) {
  return String(e).replace(/^Error:\s*/, "");
}

/** The Remove button: arms the tool (checking the model once), or leaves it, dropping marks not yet removed. */
export async function handleRemoveToolToggle() {
  if (removal.active) {
    leaveRemoveTool();
    return;
  }
  masks.shapeTarget = null;
  masks.selectedMaskId = null;
  masks.activeTool = "remove";
  removal.error = "";
  removal.notice = "";
  removal.clearMarks();
  try {
    removal.available = await removalAvailable();
  } catch {
    removal.available = false;
  }
}

/** Leaves the tool. Marks not yet removed are dropped; a running job is stopped. */
export function leaveRemoveTool() {
  if (removal.phase === "running") void removalCancel();
  removal.clearMarks();
  if (masks.activeTool === "remove") masks.activeTool = null;
}

/** @param {import('$lib/api/removal.js').MaskShape} shape */
export function handleRemoveMark(shape) {
  if (removal.phase === "running") return;
  removal.shapes = [...removal.shapes, shape];
  removal.guarding = false;
  removal.error = "";
  removal.notice = "";
}

/** Drops the last mark (Backspace / Undo while marks are pending). */
export function handleRemoveUnmark() {
  if (removal.phase === "running" || removal.shapes.length === 0) return;
  removal.shapes = removal.shapes.slice(0, -1);
  removal.guarding = false;
}

export function handleRemoveClear() {
  if (removal.phase === "running") return;
  removal.clearMarks();
}

/** Stops the running job; nothing is stored. */
export function handleRemoveCancel() {
  if (removal.phase === "running") void removalCancel();
}

/** Runs the removal on the marks. Large marks ask once first (`confirmLarge` is the second press). */
export async function handleRemoveRun(confirmLarge = false) {
  const versionId = develop.versionId;
  if (versionId === null || removal.phase === "running" || removal.shapes.length === 0) return;
  const aspect = develop.imageNativeWidth && develop.imageNativeHeight ? develop.imageNativeWidth / develop.imageNativeHeight : 1;
  if (!confirmLarge && removal.extent(aspect) > LARGE_MARK_FRACTION) {
    removal.guarding = true;
    return;
  }
  removal.guarding = false;
  removal.phase = "running";
  removal.error = "";
  removal.notice = "";
  // A plain copy for the IPC call (the marks are reactive state).
  const shapes = JSON.parse(JSON.stringify(removal.shapes));
  try {
    const results = await removeObject(versionId, shapes);
    const added = results.map((r) => ({ patch: r.patch_id, engine: r.engine }));
    await writeRemovals(versionId, added);
    removal.clearMarks();
    const n = results.length;
    removal.notice = `Removed ${n} ${n === 1 ? "thing" : "things"}${results.some((r) => r.reduced) ? " (a large area: the fill may look soft)" : ""}`;
  } catch (e) {
    if (isRemovalCancelled(e)) removal.notice = "Cancelled";
    else removal.error = message(e);
  } finally {
    removal.phase = "idle";
  }
}

/** Names the patches in the stack of `versionId`: through the open photo's store when it is still the open one (so the
 * canvas, the history list and the flush all follow), straight to the catalog when the user moved on meanwhile.
 * @param {number} versionId @param {{ patch: number, engine: string }[]} added */
async function writeRemovals(versionId, added) {
  if (develop.versionId === versionId) {
    develop.editStack = addRemovals(develop.editStack, added);
    await develop.flushEditStack("Remove object");
  } else {
    await setEditStack(versionId, addRemovals(await getEditStack(versionId), added), "Remove object");
    shell.notify("Removed from the photo you had open");
  }
  regenerateThumbnailFor(versionId);
}

/** Takes the newest removal out of the stack again (its patch stays stored, so History can bring it back). */
export async function handleRemoveUndoLast() {
  const items = getRemovals(develop.editStack);
  if (items.length === 0 || removal.phase === "running") return;
  develop.editStack = removeRemoval(develop.editStack, items[items.length - 1].patch);
  removal.notice = "";
  await develop.flushEditStack("Undo remove");
  regenerateThumbnailFor(develop.versionId);
}
