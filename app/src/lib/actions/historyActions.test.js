import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

const api = vi.hoisted(() => ({ restoreHistoryEntry: vi.fn(), restoreSnapshot: vi.fn() }));
vi.mock("$lib/api/develop.js", async (importOriginal) => ({ ...(await importOriginal()), ...api }));
vi.mock("@tauri-apps/api/core", () => ({ convertFileSrc: (/** @type {string} */ p) => p }));
const regen = vi.hoisted(() => vi.fn());
vi.mock("$lib/actions/importActions.js", () => ({ regenerateThumbnailFor: regen }));

import { restoreTo, handleUndo, handleRedo, handleRestoreSnapshot, handleResetEditStack } from "./historyActions.js";
import { develop } from "$lib/state/develop.svelte.js";
import { masks } from "$lib/state/masks.svelte.js";
import { presets } from "$lib/state/presets.svelte.js";
import { upsertOp } from "$lib/api/develop.js";

const stack = (/** @type {number} */ exposure) => ({ schema_version: 1, ops: [{ op: "exposure", value: exposure }] });
const entry = (/** @type {number} */ id) => /** @type {any} */ ({ id, label: `e${id}` });

/** @type {string[]} */
let calls;
beforeEach(() => {
  vi.clearAllMocks();
  calls = [];
  vi.spyOn(develop, "clearPreview").mockImplementation(() => void calls.push("clearPreview"));
  vi.spyOn(develop, "cancelScheduledFlush").mockImplementation(() => void calls.push("cancel"));
  vi.spyOn(develop, "discardPendingLabel").mockImplementation(() => void calls.push("discard"));
  vi.spyOn(develop, "flushEditStack").mockImplementation(async () => void calls.push("flush"));
  develop.versionId = 10;
  develop.editStack = /** @type {any} */ (stack(1));
  develop.history = [entry(100), entry(101), entry(102)];
  develop.historyIndex = 2;
  masks.selectedMaskId = "m";
  masks.activeTool = "brush";
  presets.confirmingReset = true;
  api.restoreHistoryEntry.mockImplementation(async () => {
    calls.push("restore");
    return stack(9);
  });
  api.restoreSnapshot.mockImplementation(async () => {
    calls.push("restoreSnapshot");
    return [stack(7), [entry(1), entry(2), entry(3), entry(4)]];
  });
});
afterEach(() => vi.restoreAllMocks());

describe("restoreTo", () => {
  it("cancels the pending write (timer and label) BEFORE the restore, then swaps the stack wholesale", async () => {
    await restoreTo(0);
    expect(calls).toEqual(["clearPreview", "cancel", "discard", "restore"]);
    expect(api.restoreHistoryEntry).toHaveBeenCalledWith(10, 100);
    expect(develop.editStack).toEqual(stack(9));
    expect(develop.historyIndex).toBe(0);
  });

  it("clears the mask tool and selection, and regenerates the thumbnail of the restored version", async () => {
    await restoreTo(1);
    expect([masks.selectedMaskId, masks.activeTool]).toEqual([null, null]);
    expect(regen).toHaveBeenCalledWith(10);
  });

  it("captures the version before the await: switching image mid-restore does not retarget the regen", async () => {
    api.restoreHistoryEntry.mockImplementation(async () => {
      develop.versionId = 99;
      return stack(9);
    });
    await restoreTo(0);
    expect(regen).toHaveBeenCalledWith(10);
  });

  it.each([[-1], [3], [99]])("index %i is out of range: preview is cleared but nothing else happens", async (index) => {
    await restoreTo(index);
    expect(calls).toEqual(["clearPreview"]);
    expect(api.restoreHistoryEntry).not.toHaveBeenCalled();
    expect(develop.editStack).toEqual(stack(1));
    expect(masks.selectedMaskId).toBe("m");
  });

  it("does nothing with no open image", async () => {
    develop.versionId = null;
    await restoreTo(0);
    expect(api.restoreHistoryEntry).not.toHaveBeenCalled();
  });
});

describe("undo / redo", () => {
  it("undo steps one entry back; it is disabled at the first entry", async () => {
    handleUndo();
    expect(api.restoreHistoryEntry).toHaveBeenCalledWith(10, 101);
    api.restoreHistoryEntry.mockClear();
    develop.historyIndex = 0;
    handleUndo();
    expect(api.restoreHistoryEntry).not.toHaveBeenCalled();
    expect(develop.clearPreview).toHaveBeenCalledTimes(1); // only the first, enabled, undo got as far as restoreTo
  });

  it("redo steps one entry forward; it is disabled at the last entry", () => {
    develop.historyIndex = 0;
    handleRedo();
    expect(api.restoreHistoryEntry).toHaveBeenCalledWith(10, 101);
    api.restoreHistoryEntry.mockClear();
    develop.historyIndex = 2;
    handleRedo();
    expect(api.restoreHistoryEntry).not.toHaveBeenCalled();
    expect(develop.clearPreview).toHaveBeenCalledTimes(1);
  });
});

describe("handleRestoreSnapshot", () => {
  it("cancels pending writes first, then adopts the stack AND the fresh history, jumping to the newest row", async () => {
    await handleRestoreSnapshot(5);
    expect(calls).toEqual(["clearPreview", "cancel", "discard", "restoreSnapshot"]);
    expect(api.restoreSnapshot).toHaveBeenCalledWith(10, 5);
    expect(develop.editStack).toEqual(stack(7));
    expect(develop.history.map((h) => h.id)).toEqual([1, 2, 3, 4]);
    expect(develop.historyIndex).toBe(3);
  });

  it("clears the mask tool/selection and regenerates the thumbnail", async () => {
    await handleRestoreSnapshot(5);
    expect([masks.selectedMaskId, masks.activeTool]).toEqual([null, null]);
    expect(regen).toHaveBeenCalledWith(10);
  });

  it("with no open image only clears the preview", async () => {
    develop.versionId = null;
    await handleRestoreSnapshot(5);
    expect(calls).toEqual(["clearPreview"]);
    expect(api.restoreSnapshot).not.toHaveBeenCalled();
  });
});

describe("handleResetEditStack", () => {
  it("empties the stack, clears tool/selection, closes the confirm dialog and flushes immediately as Reset", () => {
    develop.editStack = upsertOp(develop.editStack, "contrast", 5);
    handleResetEditStack();
    expect(develop.editStack.ops).toEqual([]);
    expect([masks.selectedMaskId, masks.activeTool, presets.confirmingReset]).toEqual([null, null, false]);
    expect(develop.flushEditStack).toHaveBeenCalledWith("Reset");
  });

  it("does nothing with no open image", () => {
    develop.versionId = null;
    handleResetEditStack();
    expect(develop.editStack).toEqual(stack(1));
    expect(presets.confirmingReset).toBe(true);
    expect(develop.flushEditStack).not.toHaveBeenCalled();
  });
});
