// Develop history workflows (RFC-0009 P6b, moved out of +page.svelte's script): jumping to a
// history entry (undo/redo/History-panel click), restoring a snapshot, and resetting the edit stack.
// Each replaces the edit stack wholesale, so each first cancels the pending debounced write, and
// each clears the mask tool/selection (`masks`) and regenerates the Library thumbnail.

import { develop } from "$lib/state/develop.svelte.js";
import { restoreHistoryEntry, restoreSnapshot, resetEditStack } from "$lib/api/develop.js";
import { masks } from "$lib/state/masks.svelte.js";
import { regenerateThumbnailFor } from "$lib/actions/importActions.js";
import { presets } from "$lib/state/presets.svelte.js";

/** Moves the live edit stack to `history[index]` -- undo, redo, and a
 * History-panel row click are all this same call, just with a
 * different `index`. See `history`/`historyIndex`'s own doc comment for
 * why this needs no server-side cursor concept at all. */
export async function restoreTo(/** @type {number} */ index) {
  develop.clearPreview();
  if (develop.versionId === null || index < 0 || index >= develop.history.length) return;
  const versionId = develop.versionId;
  const entryId = develop.history[index].id;
  // A restore overwrites editStack wholesale -- cancel any debounced
  // write still pending first, or it could fire afterward under a now-
  // stale label and silently stomp the just-restored state.
  develop.cancelScheduledFlush();
  develop.discardPendingLabel();
  develop.editStack = await restoreHistoryEntry(versionId, entryId);
  develop.historyIndex = index;
  masks.selectedMaskId = null;
  masks.activeTool = null;
  regenerateThumbnailFor(versionId);
}

export function handleUndo() {
  if (develop.canUndo) restoreTo(develop.historyIndex - 1);
}

export function handleRedo() {
  if (develop.canRedo) restoreTo(develop.historyIndex + 1);
}

/** Unlike restoreTo/restoreHistoryEntry, restoring a snapshot IS a new,
 * undoable edit of its own (see Catalog::restore_snapshot's doc
 * comment) -- the returned history list already includes its own
 * "Restore Snapshot: {name}" row, so this jumps historyIndex straight
 * to newest rather than searching for that row's position. */
export async function handleRestoreSnapshot(/** @type {number} */ snapshotId) {
  develop.clearPreview();
  if (develop.versionId === null) return;
  const versionId = develop.versionId;
  develop.cancelScheduledFlush();
  develop.discardPendingLabel();
  const [stack, freshHistory] = await restoreSnapshot(versionId, snapshotId);
  develop.editStack = stack;
  develop.history = freshHistory;
  develop.historyIndex = freshHistory.length - 1;
  masks.selectedMaskId = null;
  masks.activeTool = null;
  regenerateThumbnailFor(versionId);
}

export function handleResetEditStack() {
  if (develop.versionId === null) return;
  develop.editStack = resetEditStack(develop.editStack);
  masks.selectedMaskId = null;
  masks.activeTool = null;
  presets.confirmingReset = false;
  develop.flushEditStack("Reset");
}
