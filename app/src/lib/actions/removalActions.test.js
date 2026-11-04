import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

const api = vi.hoisted(() => ({ removalAvailable: vi.fn(), removeObject: vi.fn(), removalCancel: vi.fn(), isRemovalCancelled: (/** @type {unknown} */ e) => String(e).includes("removal cancelled") }));
vi.mock("$lib/api/removal.js", () => api);
const dev = vi.hoisted(() => ({ getEditStack: vi.fn(), setEditStack: vi.fn() }));
vi.mock("$lib/api/develop.js", async (importOriginal) => ({ ...(await importOriginal()), getEditStack: dev.getEditStack, setEditStack: dev.setEditStack }));
const regen = vi.hoisted(() => vi.fn());
vi.mock("$lib/actions/importActions.js", () => ({ regenerateThumbnailFor: regen }));

import { handleRemoveToolToggle, leaveRemoveTool, handleRemoveMark, handleRemoveUnmark, handleRemoveClear, handleRemoveRun, handleRemoveCancel, handleRemoveUndoLast } from "./removalActions.js";
import { develop } from "$lib/state/develop.svelte.js";
import { masks } from "$lib/state/masks.svelte.js";
import { library } from "$lib/state/library.svelte.js";
import { removal } from "$lib/state/removal.svelte.js";
import { shell } from "$lib/state/shell.svelte.js";
import { getRemovals } from "$lib/api/develop.js";

const dab = (/** @type {number} */ x = 0.5, /** @type {number} */ r = 0.01) => /** @type {import('$lib/api/removal.js').MaskShape} */ ({ kind: "stroke", points: [[x, 0.5]], radius: r });
const done = (/** @type {number} */ id, reduced = false) => ({ patch_id: id, engine: "lama-512-places2", x: 0, y: 0, w: 8, h: 8, src_w: 100, src_h: 100, reduced, ms: 5 });

beforeEach(() => {
  vi.clearAllMocks();
  develop.versionId = 10;
  develop.editStack = { schema_version: 1, ops: [{ op: "exposure", value: 0.5 }] };
  library.images = /** @type {any} */ ([{ image_id: 7, version_id: 10, content_hash: "h1", width: 3000, height: 2000 }]);
  vi.spyOn(develop, "flushEditStack").mockImplementation(async () => {});
  masks.activeTool = null;
  masks.selectedMaskId = null;
  removal.clearMarks();
  removal.phase = "idle";
  removal.error = "";
  removal.notice = "";
  removal.available = null;
  api.removalAvailable.mockResolvedValue(true);
  api.removalCancel.mockResolvedValue(undefined);
  shell.notify("");
});
afterEach(() => vi.restoreAllMocks());

describe("arming the tool", () => {
  it("arms it as the active tool, drops a selected mask, and asks once whether the model is installed", async () => {
    masks.selectedMaskId = "m1";
    await handleRemoveToolToggle();
    expect(masks.activeTool).toBe("remove");
    expect(masks.selectedMaskId).toBeNull();
    expect(removal.available).toBe(true);
    expect(api.removalAvailable).toHaveBeenCalledTimes(1);
  });

  it("says unavailable when the model is missing or the check fails", async () => {
    api.removalAvailable.mockResolvedValue(false);
    await handleRemoveToolToggle();
    expect(removal.available).toBe(false);
    leaveRemoveTool();
    api.removalAvailable.mockRejectedValue(new Error("x"));
    await handleRemoveToolToggle();
    expect(removal.available).toBe(false);
  });

  it("toggling again leaves it and drops the marks not yet removed", async () => {
    await handleRemoveToolToggle();
    handleRemoveMark(dab());
    await handleRemoveToolToggle();
    expect(masks.activeTool).toBeNull();
    expect(removal.shapes).toEqual([]);
  });

  it("leaving while a job runs stops it", async () => {
    await handleRemoveToolToggle();
    removal.phase = "running";
    leaveRemoveTool();
    expect(api.removalCancel).toHaveBeenCalledTimes(1);
  });
});

describe("marks", () => {
  it("collects marks in order, drops the newest, or all", () => {
    handleRemoveMark(dab(0.1));
    handleRemoveMark(dab(0.2));
    handleRemoveMark(dab(0.3));
    handleRemoveUnmark();
    expect(removal.shapes.map((s) => s.points[0][0])).toEqual([0.1, 0.2]);
    handleRemoveClear();
    expect(removal.shapes).toEqual([]);
  });

  it("ignores marks while a job runs", () => {
    removal.phase = "running";
    handleRemoveMark(dab());
    handleRemoveUnmark();
    handleRemoveClear();
    expect(removal.shapes).toEqual([]);
  });
});

describe("running the removal", () => {
  it("sends the marks, names the patches in the stack under one label, clears the marks and refreshes the thumbnail", async () => {
    handleRemoveMark(dab(0.3));
    handleRemoveMark(dab(0.4));
    api.removeObject.mockResolvedValue([done(4), done(5)]);
    await handleRemoveRun();
    expect(api.removeObject).toHaveBeenCalledWith(10, [dab(0.3), dab(0.4)]);
    expect(getRemovals(develop.editStack)).toEqual([{ patch: 4, engine: "lama-512-places2" }, { patch: 5, engine: "lama-512-places2" }]);
    expect(develop.editStack.ops.find((o) => o.op === "exposure")).toBeTruthy();
    expect(develop.flushEditStack).toHaveBeenCalledWith("Remove object");
    expect(regen).toHaveBeenCalledWith(10);
    expect(removal.shapes).toEqual([]);
    expect(removal.notice).toBe("Removed 2 things");
    expect(removal.phase).toBe("idle");
  });

  it("a later removal is added after the earlier ones", async () => {
    develop.editStack = /** @type {any} */ ({ schema_version: 1, ops: [{ op: "remove", items: [{ patch: 1, engine: "lama-512-places2" }] }] });
    handleRemoveMark(dab());
    api.removeObject.mockResolvedValue([done(2, true)]);
    await handleRemoveRun();
    expect(getRemovals(develop.editStack).map((i) => i.patch)).toEqual([1, 2]);
    expect(removal.notice).toContain("Removed 1 thing");
    expect(removal.notice).toContain("soft");
  });

  it("a large mark asks first; the second press runs it", async () => {
    handleRemoveMark({ kind: "polygon", points: [[0.1, 0.1], [0.8, 0.1], [0.8, 0.8]] });
    await handleRemoveRun();
    expect(removal.guarding).toBe(true);
    expect(api.removeObject).not.toHaveBeenCalled();
    api.removeObject.mockResolvedValue([done(3)]);
    await handleRemoveRun(true);
    expect(api.removeObject).toHaveBeenCalledTimes(1);
    expect(removal.guarding).toBe(false);
  });

  it("a failure keeps the marks, shows the reason as is, and writes nothing", async () => {
    handleRemoveMark(dab());
    api.removeObject.mockRejectedValue(new Error("the removal model is not installed"));
    await handleRemoveRun();
    expect(removal.error).toBe("the removal model is not installed");
    expect(removal.shapes).toHaveLength(1);
    expect(getRemovals(develop.editStack)).toEqual([]);
    expect(develop.flushEditStack).not.toHaveBeenCalled();
    expect(removal.phase).toBe("idle");
  });

  it("a cancelled job is not an error and keeps the marks", async () => {
    handleRemoveMark(dab());
    api.removeObject.mockRejectedValue("removal cancelled");
    await handleRemoveRun();
    expect(removal.error).toBe("");
    expect(removal.notice).toBe("Cancelled");
    expect(removal.shapes).toHaveLength(1);
  });

  it("does nothing without marks, without a photo, or while one is running", async () => {
    await handleRemoveRun();
    handleRemoveMark(dab());
    develop.versionId = null;
    await handleRemoveRun();
    develop.versionId = 10;
    removal.phase = "running";
    await handleRemoveRun();
    expect(api.removeObject).not.toHaveBeenCalled();
  });

  it("when the user moved to another photo meanwhile, the patches go to the photo they were made on", async () => {
    handleRemoveMark(dab());
    api.removeObject.mockImplementation(async () => {
      develop.versionId = 99;
      develop.editStack = { schema_version: 1, ops: [] };
      return [done(8)];
    });
    dev.getEditStack.mockResolvedValue({ schema_version: 1, ops: [{ op: "exposure", value: 1 }] });
    dev.setEditStack.mockResolvedValue([]);
    await handleRemoveRun();
    expect(develop.flushEditStack).not.toHaveBeenCalled();
    expect(develop.editStack.ops).toEqual([]);
    expect(dev.setEditStack).toHaveBeenCalledTimes(1);
    const [v, stack, label] = dev.setEditStack.mock.calls[0];
    expect([v, label]).toEqual([10, "Remove object"]);
    expect(getRemovals(stack)).toEqual([{ patch: 8, engine: "lama-512-places2" }]);
    expect(regen).toHaveBeenCalledWith(10);
  });

  it("cancel asks the backend to stop only while a job runs", () => {
    handleRemoveCancel();
    expect(api.removalCancel).not.toHaveBeenCalled();
    removal.phase = "running";
    handleRemoveCancel();
    expect(api.removalCancel).toHaveBeenCalledTimes(1);
  });
});

describe("undoing the newest removal", () => {
  it("takes the newest item out of the stack, under its own label", async () => {
    develop.editStack = /** @type {any} */ ({ schema_version: 1, ops: [{ op: "remove", items: [{ patch: 1, engine: "e" }, { patch: 2, engine: "e" }] }] });
    await handleRemoveUndoLast();
    expect(getRemovals(develop.editStack).map((i) => i.patch)).toEqual([1]);
    expect(develop.flushEditStack).toHaveBeenCalledWith("Undo remove");
    await handleRemoveUndoLast();
    expect(getRemovals(develop.editStack)).toEqual([]);
    await handleRemoveUndoLast();
    expect(develop.flushEditStack).toHaveBeenCalledTimes(2);
  });
});
