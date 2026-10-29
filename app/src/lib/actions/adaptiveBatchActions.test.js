import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

const api = vi.hoisted(() => ({ resolveAdaptiveRegion: vi.fn() }));
vi.mock("$lib/api/adaptive.js", () => api);
const dev = vi.hoisted(() => ({ getEditStack: vi.fn(), setEditStack: vi.fn(), regenerateThumbnail: vi.fn() }));
vi.mock("$lib/api/develop.js", async (orig) => ({ ...(await orig()), ...dev }));
const faceDetect = vi.hoisted(() => ({ runFaceDetection: vi.fn() }));
vi.mock("$lib/actions/faceActions.js", () => faceDetect);
const lib = vi.hoisted(() => ({ patchLocal: vi.fn() }));
vi.mock("$lib/actions/libraryActions.js", () => lib);
vi.mock("@tauri-apps/api/core", () => ({ convertFileSrc: (/** @type {string} */ p) => p }));

import { handleOpenAdaptiveBatch, handleRunAdaptiveBatch, handleCancelAdaptiveBatch, handleSelectBatchRows, handleCloseAdaptiveBatch } from "./adaptiveBatchActions.js";
import { adaptiveBatch } from "$lib/state/adaptiveBatch.svelte.js";
import { library } from "$lib/state/library.svelte.js";
import { selection } from "$lib/state/selection.svelte.js";
import { develop } from "$lib/state/develop.svelte.js";
import { listMasks } from "$lib/api/develop.js";
import { planAdaptiveApply, ADAPTIVE_PRESETS } from "$lib/adaptivePresets.js";

const result = (/** @type {any} */ over = {}) => ({ outcome: "found", coverage: 0.25, reason: null, mask: { op: "luminance_range_mask", rangeMin: 0, rangeMax: 25, feather: 30, invert: false }, ...over });
const bare = () => ({ schema_version: 1, ops: [{ op: "exposure", value: 0.5 }] });
const img = (/** @type {number} */ n, /** @type {any} */ over = {}) => ({ image_id: n, version_id: n * 10, path: `/p/photo${n}.raw`, content_hash: `h${n}`, faces_scanned: true, ...over });

beforeEach(() => {
  vi.clearAllMocks();
  library.images = /** @type {any} */ ([img(1), img(2), img(3)]);
  selection.selectedIds = new Set([10, 20, 30]);
  develop.versionId = null;
  adaptiveBatch.open = false;
  adaptiveBatch.phase = "confirm";
  adaptiveBatch.amount = 100;
  dev.getEditStack.mockImplementation(async () => bare());
  dev.setEditStack.mockResolvedValue([]);
  dev.regenerateThumbnail.mockResolvedValue("/t.jpg");
  api.resolveAdaptiveRegion.mockResolvedValue(result());
});
afterEach(() => vi.restoreAllMocks());

describe("Apply Adaptive Preset to Selection (B3)", () => {
  it("opens on the Amount question for the selection, and ignores an unknown preset or an empty selection", () => {
    handleOpenAdaptiveBatch("nonsense");
    expect(adaptiveBatch.open).toBe(false);
    selection.selectedIds = new Set();
    handleOpenAdaptiveBatch("cool-shadows");
    expect(adaptiveBatch.open).toBe(false);
    selection.selectedIds = new Set([10, 20, 30]);
    handleOpenAdaptiveBatch("cool-shadows");
    expect(adaptiveBatch.open).toBe(true);
    expect(adaptiveBatch.phase).toBe("confirm");
    expect(adaptiveBatch.targets).toEqual([10, 20, 30]);
  });

  it("resolves and writes each photo on its own, one History label each, and reports every photo", async () => {
    handleOpenAdaptiveBatch("cool-shadows");
    await handleRunAdaptiveBatch();
    expect(api.resolveAdaptiveRegion.mock.calls.map((c) => c.slice(0, 3))).toEqual([[1, "/p/photo1.raw", "h1"], [2, "/p/photo2.raw", "h2"], [3, "/p/photo3.raw", "h3"]]);
    expect(dev.setEditStack).toHaveBeenCalledTimes(3);
    expect(dev.setEditStack.mock.calls[0][2]).toBe("Adaptive: Cool the shadows 100 %");
    expect(listMasks(dev.setEditStack.mock.calls[1][1])).toHaveLength(1);
    expect(adaptiveBatch.phase).toBe("report");
    expect(adaptiveBatch.rows.map((r) => [r.name, r.status])).toEqual([["photo1.raw", "applied"], ["photo2.raw", "applied"], ["photo3.raw", "applied"]]);
  });

  it("a photo where the region is not found is listed and left untouched; a flagged one is listed as to check", async () => {
    api.resolveAdaptiveRegion
      .mockResolvedValueOnce(result({ outcome: "not_found", coverage: 0, reason: "73 % of the frame is already deep shadow", mask: null }))
      .mockResolvedValueOnce(result({ outcome: "check", reason: "only 5 % of the frame" }))
      .mockResolvedValueOnce(result());
    handleOpenAdaptiveBatch("cool-shadows");
    await handleRunAdaptiveBatch();
    expect(adaptiveBatch.rows.map((r) => r.status)).toEqual(["nothing", "check", "applied"]);
    expect(dev.setEditStack).toHaveBeenCalledTimes(2);
    expect(dev.setEditStack.mock.calls.map((c) => c[0])).toEqual([20, 30]);
  });

  it("a mask the user changed is kept, not replaced, and the photo is listed as kept", async () => {
    const preset = /** @type {any} */ (ADAPTIVE_PRESETS.find((p) => p.id === "cool-shadows"));
    const written = planAdaptiveApply(bare(), preset, [result()], 100).stack;
    const mine = { ...written, ops: written.ops.map((o) => (/** @type {any} */ (o).source ? { ...o, exposure: 0.4 } : o)) };
    dev.getEditStack.mockImplementation(async (/** @type {number} */ v) => (v === 20 ? mine : bare()));
    handleOpenAdaptiveBatch("cool-shadows");
    await handleRunAdaptiveBatch();
    expect(adaptiveBatch.rows.map((r) => r.status)).toEqual(["applied", "kept", "applied"]);
    expect(dev.setEditStack.mock.calls.map((c) => c[0])).toEqual([10, 30]);
  });

  it("one failing photo does not stop the rest and says why", async () => {
    api.resolveAdaptiveRegion.mockRejectedValueOnce(new Error("preview failed")).mockResolvedValue(result());
    handleOpenAdaptiveBatch("cool-shadows");
    await handleRunAdaptiveBatch();
    expect(adaptiveBatch.rows.map((r) => r.status)).toEqual(["failed", "applied", "applied"]);
    expect(adaptiveBatch.rows[0].error).toContain("preview failed");
  });

  it("Stop leaves the photos not reached out of the report and says how many", async () => {
    api.resolveAdaptiveRegion.mockImplementation(async () => {
      handleCancelAdaptiveBatch();
      return result();
    });
    handleOpenAdaptiveBatch("cool-shadows");
    await handleRunAdaptiveBatch();
    expect(adaptiveBatch.rows).toHaveLength(1);
    expect(adaptiveBatch.phase).toBe("report");
    expect(adaptiveBatch.targets).toHaveLength(3);
  });

  it("scans the unscanned photos once for a face preset, and fails a photo whose scan did not happen", async () => {
    library.images = /** @type {any} */ ([img(1, { faces_scanned: false }), img(2, { faces_scanned: false }), img(3)]);
    faceDetect.runFaceDetection.mockImplementation(async () => {
      library.images = library.images.map((i) => (i.image_id === 1 ? { ...i, faces_scanned: true } : i));
    });
    handleOpenAdaptiveBatch("brighten-faces");
    await handleRunAdaptiveBatch();
    expect(faceDetect.runFaceDetection).toHaveBeenCalledTimes(1);
    expect(faceDetect.runFaceDetection).toHaveBeenCalledWith([1, 2], false);
    expect(adaptiveBatch.rows.map((r) => r.status)).toEqual(["applied", "failed", "applied"]);
    expect(adaptiveBatch.rows[1].error).toContain("faces");
  });

  it("the open Develop photo writes its pending edits first and reads the batch's result back", async () => {
    develop.versionId = 20;
    const flush = vi.spyOn(develop, "flushEditStack").mockImplementation(async () => {});
    handleOpenAdaptiveBatch("cool-shadows");
    await handleRunAdaptiveBatch();
    expect(flush).toHaveBeenCalledTimes(1);
    expect(flush.mock.invocationCallOrder[0]).toBeLessThan(api.resolveAdaptiveRegion.mock.invocationCallOrder[0]);
    expect(dev.getEditStack).toHaveBeenLastCalledWith(20);
  });

  it("Select these makes the chosen kind of photos the Library selection and closes the dialog", async () => {
    api.resolveAdaptiveRegion.mockResolvedValueOnce(result({ outcome: "check", reason: "x" })).mockResolvedValue(result());
    handleOpenAdaptiveBatch("cool-shadows");
    await handleRunAdaptiveBatch();
    handleSelectBatchRows(["check"]);
    expect([...selection.selectedIds]).toEqual([10]);
    expect(adaptiveBatch.open).toBe(false);
  });

  it("cannot be closed or started again while running", async () => {
    let release = () => {};
    api.resolveAdaptiveRegion.mockImplementation(() => new Promise((r) => (release = () => r(result()))));
    handleOpenAdaptiveBatch("cool-shadows");
    const run = handleRunAdaptiveBatch();
    await Promise.resolve();
    handleCloseAdaptiveBatch();
    expect(adaptiveBatch.open).toBe(true);
    handleOpenAdaptiveBatch("deepen-sky");
    expect(adaptiveBatch.presetId).toBe("cool-shadows");
    for (let i = 0; i < 3; i++) {
      await new Promise((r) => setTimeout(r, 0));
      release();
    }
    await run;
  });
});
