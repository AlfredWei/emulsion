import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

const api = vi.hoisted(() => ({ applyOpsToVersions: vi.fn(), regenerateThumbnail: vi.fn(), getEditStack: vi.fn(), setEditStack: vi.fn() }));
vi.mock("$lib/api/develop.js", async (importOriginal) => ({ ...(await importOriginal()), ...api }));
vi.mock("@tauri-apps/api/core", () => ({ convertFileSrc: (/** @type {string} */ p) => p }));

import * as A from "./autoSyncActions.js";
import { develop } from "$lib/state/develop.svelte.js";
import { presets } from "$lib/state/presets.svelte.js";
import { selection } from "$lib/state/selection.svelte.js";
import { library } from "$lib/state/library.svelte.js";
import { shell } from "$lib/state/shell.svelte.js";

const stack = (/** @type {any[]} */ ...ops) => ({ schema_version: 1, ops });
const range = (/** @type {number} */ n) => Array.from({ length: n }, (_, i) => i + 1);

beforeEach(() => {
  vi.clearAllMocks();
  presets.autoSyncOn = false;
  presets.confirmingAutoSync = false;
  develop.versionId = 1;
  develop.autoSyncHook = null;
  selection.selectedId = 1;
  selection.selectedIds = new Set([1, 2, 3]);
  shell.activeModule = "develop";
  shell.notify("");
  library.images = [];
  api.applyOpsToVersions.mockResolvedValue({ changed: [2, 3], missing: [] });
  api.regenerateThumbnail.mockResolvedValue("/t/new.jpg");
});
afterEach(() => vi.restoreAllMocks());

describe("the switch", () => {
  it("turns on with two or more photos selected, and off again", () => {
    A.handleAutoSyncToggle();
    expect(presets.autoSyncOn).toBe(true);
    A.handleAutoSyncToggle();
    expect(presets.autoSyncOn).toBe(false);
  });

  it("is inert with a single photo selected, or with no photo open", () => {
    selection.selectedIds = new Set([1]);
    A.handleAutoSyncToggle();
    expect(presets.autoSyncOn).toBe(false);
    selection.selectedIds = new Set([1, 2]);
    develop.versionId = null;
    A.handleAutoSyncToggle();
    expect(presets.autoSyncOn).toBe(false);
  });

  it("asks first above the threshold, and only turns on once confirmed", () => {
    selection.selectedIds = new Set(range(A.AUTO_SYNC_CONFIRM_THRESHOLD + 1));
    A.handleAutoSyncToggle();
    expect(presets.autoSyncOn).toBe(false);
    expect(presets.confirmingAutoSync).toBe(true);
    A.handleAutoSyncConfirmed();
    expect(presets.confirmingAutoSync).toBe(false);
    expect(presets.autoSyncOn).toBe(true);
  });

  it("does not ask at exactly the threshold", () => {
    selection.selectedIds = new Set(range(A.AUTO_SYNC_CONFIRM_THRESHOLD));
    A.handleAutoSyncToggle();
    expect(presets.confirmingAutoSync).toBe(false);
    expect(presets.autoSyncOn).toBe(true);
  });

  it("a confirmation that arrives after the selection shrank to one photo does not turn it on", () => {
    presets.confirmingAutoSync = true;
    selection.selectedIds = new Set([1]);
    A.handleAutoSyncConfirmed();
    expect(presets.autoSyncOn).toBe(false);
  });
});

describe("reconcileAutoSync", () => {
  it("switches off, and says why, when the selection drops to one photo", () => {
    presets.autoSyncOn = true;
    selection.selectedIds = new Set([1]);
    A.reconcileAutoSync();
    expect(presets.autoSyncOn).toBe(false);
    expect(shell.statusMessage).toBe("Auto Sync is off: only one photo is selected");
  });

  it("switches off quietly when Develop is left", () => {
    presets.autoSyncOn = true;
    shell.activeModule = "library";
    A.reconcileAutoSync();
    expect(presets.autoSyncOn).toBe(false);
    expect(shell.statusMessage).toBe("");
  });

  it("leaves it on while two or more photos are selected in Develop", () => {
    presets.autoSyncOn = true;
    A.reconcileAutoSync();
    expect(presets.autoSyncOn).toBe(true);
  });

  it("closes a pending confirmation that no longer applies", () => {
    presets.confirmingAutoSync = true;
    shell.activeModule = "library";
    A.reconcileAutoSync();
    expect(presets.confirmingAutoSync).toBe(false);
  });
});

describe("autoSyncTargets", () => {
  it("is every other selected photo while on, and nothing while off", () => {
    expect(A.autoSyncTargets(1)).toEqual([]);
    presets.autoSyncOn = true;
    expect(A.autoSyncTargets(1)).toEqual([2, 3]);
    expect(A.autoSyncTargets(2)).toEqual([1, 3]);
  });

  it("is nothing for a photo that is not selected", () => {
    presets.autoSyncOn = true;
    expect(A.autoSyncTargets(99)).toEqual([]);
  });
});

describe("autoSyncAfterFlush", () => {
  it("sends only the changed op, absolute, with the Auto Sync label, then refreshes the changed thumbnails", async () => {
    await A.autoSyncAfterFlush(1, [2, 3], stack({ op: "exposure", value: 0.2 }, { op: "contrast", value: 5 }), stack({ op: "exposure", value: 0.8 }, { op: "contrast", value: 5 }), "Exposure");
    expect(api.applyOpsToVersions).toHaveBeenCalledWith([2, 3], [{ op: "exposure", value: 0.8 }], [], "Auto Sync: Exposure");
    expect(api.regenerateThumbnail.mock.calls.map((c) => c[0]).sort()).toEqual([2, 3]);
  });

  it("sends a removed op by name (a reset)", async () => {
    await A.autoSyncAfterFlush(1, [2], stack({ op: "exposure", value: 0.8 }), stack(), "Reset");
    expect(api.applyOpsToVersions).toHaveBeenCalledWith([2], [], ["exposure"], "Auto Sync: Reset");
  });

  it("does nothing when only a per-photo op changed", async () => {
    await A.autoSyncAfterFlush(1, [2, 3], stack(), stack({ op: "crop", x: 0.1, y: 0, width: 0.9, height: 1 }, { op: "brush_mask", id: "m" }), "Crop");
    expect(api.applyOpsToVersions).not.toHaveBeenCalled();
  });

  it("does not regenerate thumbnails of photos whose stack did not change", async () => {
    api.applyOpsToVersions.mockResolvedValue({ changed: [3], missing: [] });
    await A.autoSyncAfterFlush(1, [2, 3], stack(), stack({ op: "exposure", value: 1 }), "Exposure");
    expect(api.regenerateThumbnail.mock.calls.map((c) => c[0])).toEqual([3]);
  });

  it("counts photos that left the catalog", async () => {
    api.applyOpsToVersions.mockResolvedValue({ changed: [3], missing: [2] });
    await A.autoSyncAfterFlush(1, [2, 3], stack(), stack({ op: "exposure", value: 1 }), "Exposure");
    expect(shell.statusMessage).toBe("Auto Sync skipped 1 photo no longer in the catalog");
  });

  it("reports a failed batch once and does not throw", async () => {
    api.applyOpsToVersions.mockRejectedValue("disk full");
    await expect(A.autoSyncAfterFlush(1, [2, 3], stack(), stack({ op: "exposure", value: 1 }), "Exposure")).resolves.toBeUndefined();
    expect(shell.statusMessage).toBe("Auto Sync failed: disk full");
    expect(api.regenerateThumbnail).not.toHaveBeenCalled();
  });
});

describe("through the Develop flush", () => {
  it("a labelled flush of the open photo reaches the other selected photos, and only that photo is written by setEditStack", async () => {
    A.installAutoSync();
    presets.autoSyncOn = true;
    develop.editStack = /** @type {any} */ (stack({ op: "exposure", value: 0.8 }));
    api.getEditStack.mockResolvedValue(stack({ op: "exposure", value: 0.2 }));
    api.setEditStack.mockResolvedValue([]);
    await develop.flushEditStack("Exposure");
    expect(api.setEditStack).toHaveBeenCalledTimes(1);
    expect(api.setEditStack.mock.calls[0][0]).toBe(1);
    expect(api.applyOpsToVersions).toHaveBeenCalledWith([2, 3], [{ op: "exposure", value: 0.8 }], [], "Auto Sync: Exposure");
  });

  it("does not sync while the switch is off", async () => {
    A.installAutoSync();
    develop.editStack = /** @type {any} */ (stack({ op: "exposure", value: 0.8 }));
    api.setEditStack.mockResolvedValue([]);
    await develop.flushEditStack("Exposure");
    expect(api.getEditStack).not.toHaveBeenCalled();
    expect(api.applyOpsToVersions).not.toHaveBeenCalled();
  });
});
