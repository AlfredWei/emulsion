import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

const api = vi.hoisted(() => ({ removalAvailable: vi.fn(), removeObject: vi.fn(), removalCancel: vi.fn(), removalHole: vi.fn(), removalsWithMask: vi.fn(), isRemovalCancelled: (/** @type {unknown} */ e) => String(e).includes("removal cancelled") }));
vi.mock("$lib/api/removal.js", () => api);
const dev = vi.hoisted(() => ({ getEditStack: vi.fn(), setEditStack: vi.fn() }));
vi.mock("$lib/api/develop.js", async (importOriginal) => ({ ...(await importOriginal()), getEditStack: dev.getEditStack, setEditStack: dev.setEditStack }));
const seg = vi.hoisted(() => ({ segmentModelStatus: vi.fn(), segmentPrepare: vi.fn(), segmentDecode: vi.fn(), segmentRelease: vi.fn(), isNotPrepared: (/** @type {unknown} */ e) => String(e).includes("not_prepared") }));
vi.mock("$lib/api/segment.js", () => seg);
const cut = vi.hoisted(() => ({ readCut: vi.fn() }));
vi.mock("$lib/segmentCut.js", () => cut);
const raster = vi.hoisted(() => ({ readMaskPng: vi.fn() }));
vi.mock("$lib/maskRaster.js", () => raster);
const regen = vi.hoisted(() => vi.fn());
vi.mock("$lib/actions/importActions.js", () => ({ regenerateThumbnailFor: regen }));

import { handleRemoveToolToggle, leaveRemoveTool, handleRemoveMark, handleRemoveUnmark, handleRemoveClear, handleRemoveRun, handleRemoveCancel, handleRemoveUndoLast, handleRemoveDelete, handleRemoveHealInstead, handleRemoveHealRemoval, refreshHealable, handleRemoveMode, handleRemoveSelectClick, handleRemoveNextGuess } from "./removalActions.js";
import { develop } from "$lib/state/develop.svelte.js";
import { masks } from "$lib/state/masks.svelte.js";
import { library } from "$lib/state/library.svelte.js";
import { removal } from "$lib/state/removal.svelte.js";
import { shell } from "$lib/state/shell.svelte.js";
import { getRemovals, listMasks, MAX_MASKS, addRemovals } from "$lib/api/develop.js";

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
  removal.mode = "brush";
  removal.selectPhase = "idle";
  removal.selectNote = "";
  removal.preparedHash = null;
  develop.imagePath = "/p/a.jpg";
  develop.imageContentHash = "h1";
  seg.segmentModelStatus.mockResolvedValue({ state: "ready", download_bytes: 0, host: "" });
  seg.segmentPrepare.mockResolvedValue({ encode_ms: 1, cached: false });
  seg.segmentRelease.mockResolvedValue(undefined);
  seg.segmentDecode.mockImplementation(async () => [{ index: 0, iou: 0.9, logits_png: "L0" }, { index: 1, iou: 0.8, logits_png: "L1" }]);
  cut.readCut.mockImplementation(async (/** @type {string} */ l) => ({ bbox: [0.2, 0.2, 0.4, 0.5], tint: `tint-${l}`, coverage: 0.05 }));
  raster.readMaskPng.mockResolvedValue({ w: 40, h: 30, data: new Uint8Array(40 * 30).fill(255) });
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
    expect(removal.shapes.map((s) => /** @type {any} */ (s).points[0][0])).toEqual([0.1, 0.2]);
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

describe("deleting a listed removal", () => {
  const three = () => ({ schema_version: 1, ops: [{ op: "remove", items: [{ patch: 1, engine: "e" }, { patch: 2, engine: "e" }, { patch: 3, engine: "e" }] }] });

  it("takes a removal other than the newest out, keeps the others, under its own label", async () => {
    develop.editStack = /** @type {any} */ (three());
    await handleRemoveDelete(2);
    expect(getRemovals(develop.editStack).map((i) => i.patch)).toEqual([1, 3]);
    expect(develop.flushEditStack).toHaveBeenCalledWith("Delete removal");
    expect(regen).toHaveBeenCalledWith(10);
  });

  it("does nothing for a patch the stack does not name, or while a removal is running", async () => {
    develop.editStack = /** @type {any} */ (three());
    await handleRemoveDelete(9);
    removal.phase = "running";
    await handleRemoveDelete(1);
    expect(getRemovals(develop.editStack).map((i) => i.patch)).toEqual([1, 2, 3]);
    expect(develop.flushEditStack).not.toHaveBeenCalled();
  });
});

describe("Heal instead", () => {
  const sched = () => vi.spyOn(develop, "scheduleFlush").mockImplementation(() => {});

  it("turns the pending brush marks into one selected Spot Removal mask, drops the marks and leaves the tool", async () => {
    const flush = sched();
    const notify = vi.spyOn(shell, "notify").mockImplementation(() => {});
    await handleRemoveToolToggle();
    handleRemoveMark({ kind: "stroke", points: [[0.2, 0.5], [0.3, 0.5]], radius: 0.01 });
    await handleRemoveHealInstead();
    const spots = listMasks(develop.editStack).filter((m) => m.op === "spot_mask");
    expect(spots.length).toBe(1);
    const spot = /** @type {any} */ (spots[0]);
    expect(spot.mode).toBe("heal");
    expect(spot.dabs.length).toBeGreaterThan(5);
    expect(spot.sourceOffset.dx).toBeGreaterThan(0);
    expect(masks.selectedMaskId).toBe(spot.id);
    expect(flush).toHaveBeenCalledWith("Heal instead");
    expect(removal.count).toBe(0);
    expect(removal.active).toBe(false);
    expect(api.removeObject).not.toHaveBeenCalled();
    expect(notify).toHaveBeenCalled();
  });

  it("heals a selected object from its cut, together with a brush mark", async () => {
    const flush = sched();
    await handleRemoveToolToggle();
    removal.selection = /** @type {any} */ ({ prompts: [], candidates: [], index: 0, mark: { kind: "segment", logits: "L", bbox: [0.1, 0.1, 0.2, 0.2], tint: "T" } });
    raster.readMaskPng.mockResolvedValue({ w: 10, h: 10, data: Uint8Array.from({ length: 100 }, (_, i) => ((i % 10) >= 2 && (i % 10) < 5 && Math.floor(i / 10) >= 2 && Math.floor(i / 10) < 5 ? 255 : 0)) });
    await handleRemoveHealInstead();
    expect(raster.readMaskPng).toHaveBeenCalledWith("L");
    const spot = /** @type {any} */ (listMasks(develop.editStack).find((m) => m.op === "spot_mask"));
    expect(spot.dabs.length).toBeGreaterThan(0);
    for (const d of spot.dabs) expect(Math.abs(d.x - 0.35)).toBeLessThan(0.2);
    expect(flush).toHaveBeenCalledWith("Heal instead");
    expect(removal.active).toBe(false);
  });

  it("says so when a selected object's cut cannot be read, and keeps the marks and the tool", async () => {
    const flush = sched();
    const notify = vi.spyOn(shell, "notify").mockImplementation(() => {});
    await handleRemoveToolToggle();
    removal.selection = /** @type {any} */ ({ prompts: [], candidates: [], index: 0, mark: { kind: "segment", logits: "L", bbox: [0.1, 0.1, 0.2, 0.2], tint: "T" } });
    raster.readMaskPng.mockResolvedValue(null);
    await handleRemoveHealInstead();
    expect(notify).toHaveBeenCalledWith(expect.stringMatching(/could not be read/));
    expect(listMasks(develop.editStack)).toEqual([]);
    expect(flush).not.toHaveBeenCalled();
    expect(removal.count).toBe(1);
    expect(removal.active).toBe(true);
  });

  it("does nothing at the mask limit but says why, and keeps the marks", async () => {
    sched();
    const notify = vi.spyOn(shell, "notify").mockImplementation(() => {});
    develop.editStack = /** @type {any} */ ({ schema_version: 1, ops: Array.from({ length: MAX_MASKS }, (_, i) => ({ op: "radial_gradient_mask", id: `m${i}` })) });
    await handleRemoveToolToggle();
    handleRemoveMark({ kind: "stroke", points: [[0.5, 0.5]], radius: 0.01 });
    await handleRemoveHealInstead();
    expect(notify).toHaveBeenCalledWith(expect.stringMatching(/Maximum/));
    expect(listMasks(develop.editStack).length).toBe(MAX_MASKS);
    expect(removal.count).toBe(1);
  });

  it("does nothing with no marks or while a removal runs", async () => {
    const flush = sched();
    await handleRemoveToolToggle();
    await handleRemoveHealInstead();
    handleRemoveMark({ kind: "stroke", points: [[0.5, 0.5]], radius: 0.01 });
    removal.phase = "running";
    await handleRemoveHealInstead();
    expect(flush).not.toHaveBeenCalled();
    expect(listMasks(develop.editStack)).toEqual([]);
  });
});

describe("click-select", () => {
  const click = (/** @type {number} */ x, /** @type {boolean} */ positive = true, another = false) => handleRemoveSelectClick({ x, y: 0.5, positive, another });

  async function armSelect() {
    await handleRemoveToolToggle();
    await handleRemoveMode("select");
  }

  it("checks the model, prepares the photo, and a click takes the best candidate as the pending mark", async () => {
    await armSelect();
    expect(removal.selectPhase).toBe("ready");
    expect(seg.segmentPrepare).toHaveBeenCalledWith("/p/a.jpg", "h1");
    await click(0.3);
    expect(seg.segmentDecode).toHaveBeenCalledWith("h1", [{ x: 0.3, y: 0.5, positive: true }], null);
    expect(removal.count).toBe(1);
    expect(removal.shapes).toEqual([]);
    expect(removal.allShapes()[0]).toMatchObject({ kind: "segment", logits: "L0", bbox: [0.2, 0.2, 0.4, 0.5] });
    expect(removal.selectPhase).toBe("ready");
  });

  it("further clicks refine the same object from the candidate shown; Alt-click takes a part away", async () => {
    await armSelect();
    await click(0.3);
    await click(0.35, false);
    expect(seg.segmentDecode).toHaveBeenLastCalledWith("h1", [{ x: 0.3, y: 0.5, positive: true }, { x: 0.35, y: 0.5, positive: false }], "L0");
    expect(removal.count).toBe(1);
  });

  it("Shift-click starts another object, keeping the first as a mark", async () => {
    await armSelect();
    await click(0.3);
    await click(0.7, true, true);
    expect(removal.shapes).toHaveLength(1);
    expect(removal.count).toBe(2);
    expect(seg.segmentDecode).toHaveBeenLastCalledWith("h1", [{ x: 0.7, y: 0.5, positive: true }], null);
  });

  it("an Alt-click with nothing selected does nothing", async () => {
    await armSelect();
    await click(0.3, false);
    expect(seg.segmentDecode).not.toHaveBeenCalled();
    expect(removal.count).toBe(0);
  });

  it("Tab's next guess swaps the candidate, wrapping round", async () => {
    await armSelect();
    await click(0.3);
    await handleRemoveNextGuess();
    expect(removal.allShapes()[0]).toMatchObject({ logits: "L1" });
    await handleRemoveNextGuess();
    expect(removal.allShapes()[0]).toMatchObject({ logits: "L0" });
  });

  it("a click that selects nothing is dropped and the earlier selection stays", async () => {
    await armSelect();
    await click(0.3);
    cut.readCut.mockResolvedValueOnce(null);
    await click(0.9);
    expect(removal.selectNote).toBe("Nothing was selected there.");
    expect(removal.allShapes()[0]).toMatchObject({ logits: "L0" });
    expect(removal.selectPhase).toBe("ready");
  });

  it("a restarted helper is prepared again and the click decoded once more", async () => {
    await armSelect();
    seg.segmentDecode.mockRejectedValueOnce(new Error("not_prepared: restart"));
    await click(0.3);
    expect(seg.segmentPrepare).toHaveBeenCalledTimes(2);
    expect(removal.count).toBe(1);
  });

  it("without the model it says how to get it and takes no clicks; Brush still works", async () => {
    seg.segmentModelStatus.mockResolvedValue({ state: "missing", download_bytes: 5, host: "x" });
    await armSelect();
    expect(removal.selectPhase).toBe("idle");
    expect(removal.selectNote).toContain("Select Subject");
    await click(0.3);
    expect(seg.segmentDecode).not.toHaveBeenCalled();
    await handleRemoveMode("brush");
    handleRemoveMark(dab());
    expect(removal.count).toBe(1);
  });

  it("changing mode keeps the selected object as a mark; Backspace drops the pending object first, then the last mark", async () => {
    await armSelect();
    await click(0.3);
    await handleRemoveMode("brush");
    expect(removal.selection).toBeNull();
    expect(removal.shapes).toHaveLength(1);
    await handleRemoveMode("select");
    await click(0.7);
    expect(removal.count).toBe(2);
    handleRemoveUnmark();
    expect(removal.count).toBe(1);
    handleRemoveUnmark();
    expect(removal.count).toBe(0);
  });

  it("the job gets the cut of a selected object and nothing of its screen-only box and tint", async () => {
    api.removeObject.mockResolvedValue([done(5)]);
    await armSelect();
    await click(0.3);
    handleRemoveMark(dab(0.8));
    await handleRemoveRun(true); // the two marks together are wide enough to ask first
    const sent = api.removeObject.mock.calls[0][1];
    expect(sent).toEqual([{ kind: "segment", logits: "L0" }, { kind: "stroke", points: [[0.8, 0.5]], radius: 0.01 }]);
    expect(removal.count).toBe(0);
  });

  it("leaving the tool frees the helper's embedding", async () => {
    await armSelect();
    leaveRemoveTool();
    expect(seg.segmentRelease).toHaveBeenCalledTimes(1);
    expect(removal.selectPhase).toBe("idle");
  });
});

describe("Heal instead for a finished removal", () => {
  const hole = { x: 100, y: 50, src_w: 400, src_h: 300, png: "H" };
  beforeEach(() => {
    develop.editStack = addRemovals(develop.editStack, [{ patch: 5, engine: "lama-512-places2" }, { patch: 6, engine: "lama-512-places2" }]);
    api.removalHole.mockResolvedValue(hole);
  });

  it("swaps the removal for a selected heal mask over its hole, in one edit", async () => {
    const flush = vi.spyOn(develop, "flushEditStack").mockImplementation(async () => {});
    vi.spyOn(shell, "notify").mockImplementation(() => {});
    await handleRemoveHealRemoval(5);
    expect(api.removalHole).toHaveBeenCalledWith(10, 5);
    expect(getRemovals(develop.editStack).map((i) => i.patch)).toEqual([6]);
    const spot = /** @type {any} */ (listMasks(develop.editStack).find((m) => m.op === "spot_mask"));
    expect(spot.mode).toBe("heal");
    // The 40 x 30 hole at (100, 50) of a 400 x 300 photograph: every dab lies inside its box.
    for (const d of spot.dabs) {
      expect(d.x).toBeGreaterThan(0.25 - 1e-9);
      expect(d.x).toBeLessThan(0.35 + 1e-9);
      expect(d.y).toBeGreaterThan(50 / 300 - 1e-9);
      expect(d.y).toBeLessThan(80 / 300 + 1e-9);
    }
    expect(masks.selectedMaskId).toBe(spot.id);
    expect(flush).toHaveBeenCalledTimes(1);
    expect(flush).toHaveBeenCalledWith("Heal instead");
    expect(regen).toHaveBeenCalledWith(10);
  });

  it("takes the last removal out of the op altogether", async () => {
    vi.spyOn(shell, "notify").mockImplementation(() => {});
    develop.editStack = addRemovals({ schema_version: 1, ops: [] }, [{ patch: 5, engine: "lama-512-places2" }]);
    await handleRemoveHealRemoval(5);
    expect(develop.editStack.ops.some((o) => o.op === "remove")).toBe(false);
  });

  it("changes nothing for a removal with no stored hole, saying so", async () => {
    const flush = vi.spyOn(develop, "flushEditStack").mockImplementation(async () => {});
    const notify = vi.spyOn(shell, "notify").mockImplementation(() => {});
    api.removalHole.mockResolvedValue(null);
    await handleRemoveHealRemoval(5);
    expect(notify).toHaveBeenCalledWith(expect.stringMatching(/no stored hole/));
    expect(getRemovals(develop.editStack).length).toBe(2);
    expect(flush).not.toHaveBeenCalled();
  });

  it("changes nothing at the mask limit, saying so", async () => {
    const flush = vi.spyOn(develop, "flushEditStack").mockImplementation(async () => {});
    const notify = vi.spyOn(shell, "notify").mockImplementation(() => {});
    develop.editStack = { ...develop.editStack, ops: [...develop.editStack.ops, ...Array.from({ length: MAX_MASKS }, (_, i) => /** @type {any} */ ({ op: "radial_gradient_mask", id: `m${i}` }))] };
    await handleRemoveHealRemoval(5);
    expect(notify).toHaveBeenCalledWith(expect.stringMatching(/Maximum/));
    expect(getRemovals(develop.editStack).length).toBe(2);
    expect(flush).not.toHaveBeenCalled();
  });

  it("ignores a removal that is not in the stack, and anything while a removal runs", async () => {
    const flush = vi.spyOn(develop, "flushEditStack").mockImplementation(async () => {});
    await handleRemoveHealRemoval(99);
    removal.phase = "running";
    await handleRemoveHealRemoval(5);
    expect(api.removalHole).not.toHaveBeenCalled();
    expect(flush).not.toHaveBeenCalled();
  });
});

describe("refreshHealable", () => {
  it("keeps the patches the catalog says have a hole, and none for no removals", async () => {
    api.removalsWithMask.mockResolvedValue([6]);
    await refreshHealable(10, [5, 6]);
    expect(api.removalsWithMask).toHaveBeenCalledWith(10, [5, 6]);
    expect(removal.healable).toEqual([6]);
    await refreshHealable(10, []);
    expect(removal.healable).toEqual([]);
  });

  it("offers none when the catalog cannot answer", async () => {
    api.removalsWithMask.mockRejectedValue(new Error("x"));
    removal.healable = [6];
    await refreshHealable(10, [6]);
    expect(removal.healable).toEqual([]);
  });
});
