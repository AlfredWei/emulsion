import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

const api = vi.hoisted(() => ({ getEditStack: vi.fn(), getHistory: vi.fn(), getSnapshots: vi.fn(), lookupLensProfile: vi.fn() }));
vi.mock("$lib/api/develop.js", async (importOriginal) => ({ ...(await importOriginal()), ...api }));
vi.mock("@tauri-apps/api/core", () => ({ convertFileSrc: (/** @type {string} */ p) => p }));
const regen = vi.hoisted(() => vi.fn());
vi.mock("$lib/actions/importActions.js", () => ({ regenerateThumbnailFor: regen }));
const prioritize = vi.hoisted(() => vi.fn());
vi.mock("$lib/actions/libraryActions.js", () => ({ prioritizeThumbnail: prioritize }));
const people = vi.hoisted(() => vi.fn());
vi.mock("$lib/actions/faceActions.js", () => ({ refreshPeople: people }));

import { openDevelop, switchModule, handleExportClick, selectNextImage, selectPrevImage } from "./navigation.js";
import { develop } from "$lib/state/develop.svelte.js";
import { developView } from "$lib/state/developView.svelte.js";
import { masks } from "$lib/state/masks.svelte.js";
import { library } from "$lib/state/library.svelte.js";
import { selection } from "$lib/state/selection.svelte.js";
import { shell } from "$lib/state/shell.svelte.js";
import { print } from "$lib/state/print.svelte.js";
import { exportFlow } from "$lib/state/exportFlow.svelte.js";
import { setLensProfile } from "$lib/api/develop.js";

const image = (/** @type {number} */ id, /** @type {object} */ over = {}) =>
  /** @type {any} */ ({
    image_id: id,
    version_id: id * 10,
    path: `/p/${id}.raw`,
    camera_make: "Canon",
    camera_model: `R${id}`,
    lens_model: `RF${id}`,
    focal_length: 50 + id,
    aperture: 1.8,
    thumbnail_path: null,
    ...over,
  });
const stackOf = (/** @type {number} */ e) => ({ schema_version: 1, ops: [{ op: "exposure", value: e }] });

/** A promise the test resolves by hand. */
function deferred() {
  /** @type {(v?: any) => void} */
  let resolve = () => {};
  const promise = new Promise((r) => (resolve = r));
  return { promise, resolve };
}
const tick = () => new Promise((r) => setTimeout(r, 0));

/** @type {string[]} */
let order;
/** @type {import('vitest').MockInstance<(label?: string) => Promise<void>>} */
let flush;

beforeEach(() => {
  vi.clearAllMocks();
  order = [];
  library.images = [image(1), image(2), image(3)];
  selection.selectedId = null;
  selection.selectedIds = new Set();
  shell.activeModule = "library";
  develop.versionId = null;
  develop.imagePath = "";
  develop.editStack = { schema_version: 1, ops: [] };
  develop.history = [];
  develop.historyIndex = -1;
  develop.snapshots = [];
  develop.histogramData = { r: new Uint32Array(1), g: new Uint32Array(1), b: new Uint32Array(1) };
  develop.hoverPixel = { r: 1, g: 1, b: 1 };
  develop.showClippingOverlay = true;
  masks.activeTool = "brush";
  masks.selectedMaskId = "m";
  print.items = [];
  print.readyUrls = { 1: "x" };
  exportFlow.items = null;
  flush = vi.spyOn(develop, "flushEditStack").mockImplementation(async () => void order.push("flush"));
  regen.mockImplementation((/** @type {number | null} */ v) => void order.push(`regen:${v}`));
  api.getEditStack.mockResolvedValue(stackOf(1));
  api.getHistory.mockResolvedValue([{ id: 1 }, { id: 2 }, { id: 3 }]);
  api.getSnapshots.mockResolvedValue([{ id: 9 }]);
  api.lookupLensProfile.mockResolvedValue(null);
});
afterEach(() => vi.restoreAllMocks());

describe("openDevelop", () => {
  it("I1/I2: awaits the flush of the PREVIOUS image before regenerating its thumbnail, and changes nothing until then", async () => {
    develop.versionId = 10;
    const slow = deferred();
    flush.mockImplementation(async () => {
      order.push("flush:start");
      await slow.promise;
      order.push("flush:done");
    });
    const p = openDevelop(20);
    await tick();
    expect(order).toEqual(["flush:start"]);
    expect(develop.versionId).toBe(10);
    expect(regen).not.toHaveBeenCalled();
    slow.resolve();
    await p;
    expect(order.slice(0, 3)).toEqual(["flush:start", "flush:done", "regen:10"]);
    expect(develop.versionId).toBe(20);
  });

  it("with nothing open before, regenerates for null (a no-op in the real helper)", async () => {
    await openDevelop(20);
    expect(regen).toHaveBeenCalledWith(null);
  });

  it("an unknown version stops after the flush and regeneration, leaving the session and module alone", async () => {
    develop.versionId = 10;
    await openDevelop(999);
    expect(order).toEqual(["flush", "regen:10"]);
    expect(prioritize).not.toHaveBeenCalled();
    expect(develop.versionId).toBe(10);
    expect(shell.activeModule).toBe("library");
    expect(api.getEditStack).not.toHaveBeenCalled();
  });

  it("opens the image: prioritises its thumbnail, sets id/path, clears the previous image's readouts", async () => {
    await openDevelop(20);
    expect(prioritize).toHaveBeenCalledWith(20);
    expect([develop.versionId, develop.imagePath]).toEqual([20, "/p/2.raw"]);
    expect([develop.histogramData, develop.hoverPixel, develop.showClippingOverlay]).toEqual([null, null, false]);
  });

  it("fetches the edit stack, history and snapshots fresh for that version and jumps to the newest history row", async () => {
    await openDevelop(30);
    for (const fn of [api.getEditStack, api.getHistory, api.getSnapshots]) expect(fn).toHaveBeenCalledWith(30);
    expect(develop.editStack).toEqual(stackOf(1));
    expect(develop.history.map((h) => h.id)).toEqual([1, 2, 3]);
    expect(develop.historyIndex).toBe(2);
    expect(develop.snapshots.map((s) => s.id)).toEqual([9]);
  });

  it("an empty history leaves the cursor at -1", async () => {
    api.getHistory.mockResolvedValue([]);
    await openDevelop(20);
    expect(develop.historyIndex).toBe(-1);
  });

  it("clears the mask tool and selection, and only then switches to the Develop module (even while the lens lookup is pending)", async () => {
    const lookup = deferred();
    api.lookupLensProfile.mockReturnValue(lookup.promise);
    const p = openDevelop(20);
    await tick();
    expect([masks.activeTool, masks.selectedMaskId]).toEqual([null, null]);
    expect(shell.activeModule).toBe("develop");
    lookup.resolve(null);
    await p;
  });

  describe("lens profile (I3)", () => {
    const match = /** @type {any} */ ({ name: "RF50", model: 1 });

    it("looks the profile up from the image's own EXIF", async () => {
      await openDevelop(20);
      expect(api.lookupLensProfile).toHaveBeenCalledWith({ cameraMake: "Canon", cameraModel: "R2", lensModel: "RF2", focalLength: 52, aperture: 1.8 });
    });

    it("bakes a new profile into the stack and persists it with an UNLABELED flush", async () => {
      api.lookupLensProfile.mockResolvedValue(match);
      await openDevelop(20);
      expect(developView.lensCorrection.profile).toEqual(match);
      expect(flush).toHaveBeenLastCalledWith();
      expect(order.filter((o) => o === "flush")).toHaveLength(2);
    });

    it("a profile identical to the one already in the stack skips the write entirely", async () => {
      api.getEditStack.mockResolvedValue(setLensProfile(stackOf(1), match));
      api.lookupLensProfile.mockResolvedValue(match);
      await openDevelop(20);
      expect(order.filter((o) => o === "flush")).toHaveLength(1); // only the leading flush
    });

    it("no match and none baked is also a no-op", async () => {
      await openDevelop(20);
      expect(order.filter((o) => o === "flush")).toHaveLength(1);
    });

    it("a stale open: a different image opened during the lookup means the profile is NOT applied", async () => {
      const lookup = deferred();
      api.lookupLensProfile.mockReturnValue(lookup.promise);
      const p = openDevelop(20);
      await tick();
      develop.versionId = 30; // the user moved on while the lookup was in flight
      const before = develop.editStack;
      lookup.resolve(match);
      await p;
      expect(develop.editStack).toBe(before);
      expect(order.filter((o) => o === "flush")).toHaveLength(1);
    });
  });

  it("quick succession: each open flushes/regenerates the id that was open at its own start", async () => {
    await openDevelop(10);
    await openDevelop(20);
    await openDevelop(30);
    expect(regen.mock.calls.map((c) => c[0])).toEqual([null, 10, 20]);
  });
});

describe("switchModule", () => {
  beforeEach(() => {
    shell.activeModule = "develop";
    develop.versionId = 20;
    develop.imagePath = "/p/2.raw";
  });

  it("leaving Develop: awaits the flush, THEN regenerates the open image, clears the mask tool/selection, and only then switches", async () => {
    const slow = deferred();
    flush.mockImplementation(async () => {
      order.push("flush:start");
      await slow.promise;
      order.push("flush:done");
    });
    const p = switchModule("library");
    await tick();
    expect(shell.activeModule).toBe("develop");
    expect(regen).not.toHaveBeenCalled();
    expect(masks.activeTool).toBe("brush");
    slow.resolve();
    await p;
    expect(order).toEqual(["flush:start", "flush:done", "regen:20"]);
    expect([masks.activeTool, masks.selectedMaskId]).toEqual([null, null]);
    expect(shell.activeModule).toBe("library");
  });

  it("switching to Develop from Develop does not flush or clear anything", async () => {
    await switchModule("develop");
    expect(flush).not.toHaveBeenCalled();
    expect(masks.activeTool).toBe("brush");
  });

  it("coming from another module does not flush or touch the mask state", async () => {
    shell.activeModule = "library";
    await switchModule("print");
    expect(flush).not.toHaveBeenCalled();
    expect(regen).not.toHaveBeenCalled();
    expect([masks.activeTool, masks.selectedMaskId]).toEqual(["brush", "m"]);
  });

  it("I6: entering Print from Develop snapshots the OPEN image (module still develop when read), clears ready URLs", async () => {
    selection.selectedIds = new Set([10, 30]);
    await switchModule("print");
    expect(print.items).toEqual([{ path: "/p/2.raw", version_id: 20 }]);
    expect(print.readyUrls).toEqual({});
    expect(shell.activeModule).toBe("print");
  });

  it("I6: entering Print from Library snapshots the selection once; later selection changes do not alter it", async () => {
    shell.activeModule = "library";
    develop.versionId = null;
    selection.selectedIds = new Set([10, 30]);
    await switchModule("print");
    expect(print.items.map((i) => i.version_id)).toEqual([10, 30]);
    selection.selectedIds = new Set([20]);
    expect(print.items.map((i) => i.version_id)).toEqual([10, 30]);
  });

  it("entering a module other than Print leaves Print's snapshot alone", async () => {
    print.items = [{ path: "/keep", version_id: 1 }];
    print.readyUrls = { 1: "ready" };
    await switchModule("people");
    await switchModule("library");
    await switchModule("develop");
    expect(print.items).toEqual([{ path: "/keep", version_id: 1 }]);
    expect(print.readyUrls).toEqual({ 1: "ready" });
  });

  it("only entering People refreshes people, and it still switches", async () => {
    await switchModule("library");
    expect(people).not.toHaveBeenCalled();
    await switchModule("people");
    expect(people).toHaveBeenCalledTimes(1);
    expect(shell.activeModule).toBe("people");
  });
});

describe("handleExportClick", () => {
  beforeEach(() => {
    selection.selectedIds = new Set([10, 30]);
  });

  it("in Develop: awaits the flush, regenerates the open image, then opens the dialog with just that image", async () => {
    shell.activeModule = "develop";
    develop.versionId = 20;
    develop.imagePath = "/p/2.raw";
    const slow = deferred();
    flush.mockImplementation(async () => {
      await slow.promise;
      order.push("flush");
    });
    const p = handleExportClick();
    await tick();
    expect(exportFlow.items).toBeNull();
    slow.resolve();
    await p;
    expect(order).toEqual(["flush", "regen:20"]);
    expect(exportFlow.items).toEqual([{ path: "/p/2.raw", version_id: 20 }]);
  });

  it("in Library: no flush; opens with the selection", async () => {
    await handleExportClick();
    expect(flush).not.toHaveBeenCalled();
    expect(exportFlow.items?.map((i) => i.version_id)).toEqual([10, 30]);
  });

  it("with nothing to export the dialog stays closed (null, never an empty list)", async () => {
    selection.selectedIds = new Set();
    await handleExportClick();
    expect(exportFlow.items).toBeNull();
  });
});

describe("selectNextImage / selectPrevImage", () => {
  const ids = () => [...selection.selectedIds];

  it.each([
    ["next", selectNextImage],
    ["prev", selectPrevImage],
  ])("%s: does nothing with an empty (filtered) library", (_n, step) => {
    library.images = [];
    step();
    expect([selection.selectedId, ids()]).toEqual([null, []]);
  });

  it("next with nothing selected selects the first; prev selects the last", () => {
    selectNextImage();
    expect([selection.selectedId, ids()]).toEqual([10, [10]]);
    selection.selectedId = null;
    selection.selectedIds = new Set();
    selectPrevImage();
    expect([selection.selectedId, ids()]).toEqual([30, [30]]);
  });

  it("an anchor that is not in the filtered list resets to the first image (both directions)", () => {
    selection.selectedId = 999;
    selectNextImage();
    expect(selection.selectedId).toBe(10);
    selection.selectedId = 999;
    selectPrevImage();
    expect(selection.selectedId).toBe(10);
  });

  it("steps one image and replaces the selection", () => {
    selection.selectedId = 20;
    selection.selectedIds = new Set([20, 10]);
    selectNextImage();
    expect([selection.selectedId, ids()]).toEqual([30, [30]]);
    selectPrevImage();
    expect([selection.selectedId, ids()]).toEqual([20, [20]]);
  });

  it("extend adds the stepped-to image to the selection and moves the anchor", () => {
    selection.selectedId = 20;
    selection.selectedIds = new Set([20]);
    selectNextImage(true);
    expect([selection.selectedId, ids().sort()]).toEqual([30, [20, 30]]);
    selection.selectedId = 20;
    selection.selectedIds = new Set([20]);
    selectPrevImage(true);
    expect([selection.selectedId, ids().sort()]).toEqual([10, [10, 20]]);
  });

  it("does not wrap: next at the last and prev at the first are no-ops", () => {
    selection.selectedId = 30;
    selection.selectedIds = new Set([30]);
    selectNextImage();
    expect(selection.selectedId).toBe(30);
    selection.selectedId = 10;
    selection.selectedIds = new Set([10]);
    selectPrevImage();
    expect(selection.selectedId).toBe(10);
  });

  it("in Library it never opens Develop", async () => {
    selection.selectedId = 10;
    selectNextImage();
    selection.selectedId = 30;
    selectPrevImage();
    await tick();
    expect(api.getEditStack).not.toHaveBeenCalled();
    expect(shell.activeModule).toBe("library");
  });

  it("in Develop, stepping opens the new image there (and only when it actually stepped)", async () => {
    shell.activeModule = "develop";
    develop.versionId = 10;
    selection.selectedId = 10;
    selectNextImage();
    await vi.waitFor(() => expect(develop.versionId).toBe(20));
    expect(api.getEditStack).toHaveBeenCalledWith(20);
    api.getEditStack.mockClear();
    selection.selectedId = 30;
    selectNextImage(); // at the end: no step, no open
    await tick();
    expect(api.getEditStack).not.toHaveBeenCalled();
    develop.versionId = 30;
    selectPrevImage();
    await vi.waitFor(() => expect(develop.versionId).toBe(20));
    expect(api.getEditStack).toHaveBeenCalledWith(20);
  });

  it("the first-image fallbacks do not open Develop, even in Develop (as before)", async () => {
    shell.activeModule = "develop";
    selectNextImage(); // nothing selected -> first
    await tick();
    expect(selection.selectedId).toBe(10);
    expect(api.getEditStack).not.toHaveBeenCalled();
  });
});
