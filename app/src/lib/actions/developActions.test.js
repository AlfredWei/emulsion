import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

const api = vi.hoisted(() => ({
  previewHistoryEntry: vi.fn(),
  previewSnapshot: vi.fn(),
  addSnapshot: vi.fn(),
  deleteSnapshot: vi.fn(),
}));
vi.mock("$lib/api/develop.js", async (importOriginal) => ({ ...(await importOriginal()), ...api }));
vi.mock("@tauri-apps/api/core", () => ({ convertFileSrc: (/** @type {string} */ p) => p }));

import * as A from "./developActions.js";
import { develop } from "$lib/state/develop.svelte.js";
import { developView } from "$lib/state/developView.svelte.js";
import { library } from "$lib/state/library.svelte.js";
import { WB_PRESETS, computeAutoWhiteBalance, computeAutoTone, IDENTITY_CROP, upsertCrop } from "$lib/api/develop.js";
import { inscribedCropForAngle } from "$lib/cropMath.js";

/** @type {ReturnType<typeof vi.spyOn>} */
let schedule;
const labels = () => schedule.mock.calls.map((c) => c[0]);
const opNames = () => develop.editStack.ops.map((o) => /** @type {any} */ (o).op);

beforeEach(() => {
  vi.clearAllMocks();
  schedule = vi.spyOn(develop, "scheduleFlush").mockImplementation(() => {});
  develop.editStack = { schema_version: 1, ops: [] };
  develop.versionId = 10;
  develop.imagePath = "/p/a.raw";
  develop.history = [];
  develop.snapshots = [];
  develop.historyIndex = -1;
  develop.sourceWidth = 0;
  develop.sourceHeight = 0;
  develop.histogramData = null;
  develop.cropAspectLock = null;
  develop.showClippingOverlay = false;
  develop.hoverPixel = null;
  library.images = [];
});
afterEach(() => vi.restoreAllMocks());

describe("handleAdjustmentChange", () => {
  it("upserts the op (one entry per op, not a log) and schedules a flush under its display label", () => {
    A.handleAdjustmentChange("exposure", 0.5);
    A.handleAdjustmentChange("exposure", 0.9);
    expect(develop.editStack.ops).toEqual([{ op: "exposure", value: 0.9 }]);
    expect(developView.exposure).toBe(0.9);
    expect(labels()).toEqual(["Exposure", "Exposure"]);
  });

  it("uses the mapped label for each known scalar and the raw op name otherwise", () => {
    const known = { exposure: "Exposure", contrast: "Contrast", saturation: "Saturation", temperature: "Temperature", tint: "Tint", highlights: "Highlights", shadows: "Shadows", whites: "Whites", blacks: "Blacks", dehaze: "Dehaze", texture: "Texture", clarity: "Clarity" };
    for (const op of Object.keys(known)) A.handleAdjustmentChange(op, 1);
    A.handleAdjustmentChange("vibrance", 1);
    expect(labels()).toEqual([...Object.values(known), "vibrance"]);
  });
});

describe("structured adjustment handlers", () => {
  it.each([
    ["tone curve", () => A.handleToneCurveChange([{ x: 0, y: 0 }, { x: 0.5, y: 0.6 }, { x: 1, y: 1 }]), "Tone Curve"],
    ["HSL band", () => A.handleHslBandChange("red", { hue: 10 }), "HSL / Color Mixer"],
    ["split toning zone", () => A.handleSplitToningZoneChange("shadows", { hue: 200 }), "Split Toning"],
    ["split toning balance", () => A.handleSplitToningBalanceChange(30), "Split Toning"],
    ["vignette", () => A.handleVignetteChange({ amount: -20 }), "Vignette"],
    ["lens correction", () => A.handleLensCorrectionChange({ profile_enabled: true }), "Lens Corrections"],
    ["perspective", () => A.handlePerspectiveChange({ vertical: 5 }), "Perspective"],
    ["grain", () => A.handleGrainChange({ amount: 40 }), "Grain"],
    ["sharpen", () => A.handleSharpenChange({ amount: 60 }), "Sharpening"],
    ["luminance NR", () => A.handleLumaNRChange({ amount: 25 }), "Luminance Noise Reduction"],
    ["color NR", () => A.handleColorNRChange({ amount: 15 }), "Color Noise Reduction"],
  ])("%s: writes its op into the stack and schedules a labeled flush", (_name, run, label) => {
    const before = develop.editStack;
    run();
    expect(develop.editStack).not.toBe(before);
    expect(develop.editStack.ops.length).toBeGreaterThan(0);
    expect(labels()).toEqual([label]);
  });

  it("the values land where the per-adjustment views read them", () => {
    A.handleVignetteChange({ amount: -20 });
    A.handleGrainChange({ amount: 40 });
    A.handleSharpenChange({ amount: 60 });
    A.handleLumaNRChange({ amount: 25 });
    A.handleColorNRChange({ amount: 15 });
    A.handlePerspectiveChange({ vertical: 5 });
    expect(developView.vignette.amount).toBe(-20);
    expect(developView.grain.amount).toBe(40);
    expect(developView.sharpen.amount).toBe(60);
    expect(developView.lumaNR.amount).toBe(25);
    expect(developView.colorNR.amount).toBe(15);
    expect(developView.perspective.vertical).toBe(5);
  });
});

describe("crop", () => {
  it("an ordinary patch passes through", () => {
    A.handleCropChange({ x: 0.1, y: 0.2, width: 0.5, height: 0.5 });
    expect(developView.crop).toMatchObject({ x: 0.1, y: 0.2, width: 0.5, height: 0.5 });
    expect(labels()).toEqual(["Crop"]);
  });

  it("an angle-only patch re-fits the rect to the inscribed box for the new angle", () => {
    develop.sourceWidth = 6000;
    develop.sourceHeight = 4000;
    A.handleCropChange({ angle: 10 });
    expect(developView.crop.angle).toBe(10);
    expect(developView.crop.width).toBeLessThan(1);
    expect(developView.crop.height).toBeLessThan(1);
    // the pixel aspect ratio of the full frame (6000:4000) is kept
    expect((developView.crop.width * 6000) / (developView.crop.height * 4000)).toBeCloseTo(1.5, 3);
  });

  it("an angle-only patch without known source dimensions is applied as-is", () => {
    A.handleCropChange({ angle: 10 });
    expect(developView.crop).toMatchObject({ angle: 10, width: 1, height: 1 });
  });

  it("refuses a rect that would expose the rotated image's blank corners, without writing or scheduling", () => {
    A.handleCropChange({ angle: 30 }); // no source size yet: guard is off, so this commits
    schedule.mockClear();
    develop.sourceWidth = 6000;
    develop.sourceHeight = 4000;
    const before = develop.editStack;
    A.handleCropChange({ x: 0, y: 0, width: 1, height: 1 });
    expect(develop.editStack).toBe(before);
    expect(schedule).not.toHaveBeenCalled();
  });

  it("aspect preset: locks the ratio and reshapes to the largest centered box of that pixel ratio", () => {
    develop.sourceWidth = 6000;
    develop.sourceHeight = 4000;
    A.handleCropAspectPreset(1);
    expect(develop.cropAspectLock).toBe(1);
    expect((developView.crop.width * 6000) / (developView.crop.height * 4000)).toBeCloseTo(1, 3);
    const first = developView.crop;
    A.handleCropAspectPreset(1); // idempotent
    expect(developView.crop).toEqual(first);
    expect(labels()).toEqual(["Crop", "Crop"]);
  });

  it("aspect preset: fits the box to the current straighten angle", () => {
    develop.sourceWidth = 6000;
    develop.sourceHeight = 4000;
    A.handleCropChange({ angle: 10 });
    A.handleCropAspectPreset(1);
    const expected = inscribedCropForAngle(1, 6000, 4000, 10);
    expect(developView.crop).toMatchObject({ ...expected, angle: 10 });
    expect(developView.crop.width).toBeLessThan(/** @type {any} */ (inscribedCropForAngle(1, 6000, 4000, 0)).width);
  });

  it("an angle change with a degenerate (zero-height) crop is applied as-is", () => {
    develop.sourceWidth = 6000;
    develop.sourceHeight = 4000;
    develop.editStack = upsertCrop(develop.editStack, { x: 0.2, y: 0.2, width: 0.5, height: 0 });
    A.handleCropChange({ angle: 5 });
    expect(developView.crop).toMatchObject({ x: 0.2, y: 0.2, width: 0.5, height: 0, angle: 5 });
  });

  it("the blank-corner guard needs both source dimensions", () => {
    A.handleCropChange({ angle: 30 });
    develop.sourceWidth = 6000; // height still unknown
    A.handleCropChange({ x: 0, y: 0, width: 1, height: 1 });
    expect(developView.crop).toMatchObject({ width: 1, height: 1 });
  });

  it("aspect preset: null just unlocks", () => {
    develop.cropAspectLock = 1.5;
    A.handleCropAspectPreset(null);
    expect(develop.cropAspectLock).toBeNull();
    expect(develop.editStack.ops).toEqual([]);
    expect(schedule).not.toHaveBeenCalled();
  });

  it("reset: unlocks and restores the identity crop", () => {
    A.handleCropChange({ x: 0.2, y: 0.2, width: 0.4, height: 0.4 });
    develop.cropAspectLock = 2;
    A.handleCropReset();
    expect(develop.cropAspectLock).toBeNull();
    expect(developView.crop).toMatchObject(IDENTITY_CROP);
  });
});

describe("white balance and tone", () => {
  /** @param {number} bin @param {number} count */
  const hist = (bin, count) => {
    const r = new Uint32Array(256);
    r[bin] = count;
    return { r, g: new Uint32Array(r), b: new Uint32Array(r) };
  };

  it("auto WB with no histogram assumes mid-grey", () => {
    A.handleAutoWhiteBalance();
    const expected = computeAutoWhiteBalance({ r: 0.5, g: 0.5, b: 0.5 });
    expect(developView.temperature).toBe(expected.temperature);
    expect(developView.tint).toBe(expected.tint);
    expect(labels()).toEqual(["Auto White Balance"]);
  });

  it("auto WB averages the histogram (bin index / 255, weighted by count)", () => {
    develop.histogramData = hist(204, 1000); // every pixel at 0.8 in all channels
    A.handleAutoWhiteBalance();
    const expected = computeAutoWhiteBalance({ r: 204 / 255, g: 204 / 255, b: 204 / 255 });
    expect(developView.temperature).toBe(expected.temperature);
    expect(developView.tint).toBe(expected.tint);
  });

  it("an empty histogram falls back to mid-grey", () => {
    develop.histogramData = hist(0, 0);
    A.handleAutoWhiteBalance();
    const expected = computeAutoWhiteBalance({ r: 0.5, g: 0.5, b: 0.5 });
    expect(developView.temperature).toBe(expected.temperature);
  });

  it("WB preset: applies the preset's temperature/tint under its own label; unknown keys do nothing; 'auto' runs auto WB", () => {
    A.handleWbPresetChange("tungsten");
    expect([developView.temperature, developView.tint]).toEqual([WB_PRESETS.tungsten.temperature, WB_PRESETS.tungsten.tint]);
    expect(labels()).toEqual(["WB Profile: Tungsten"]);
    schedule.mockClear();
    A.handleWbPresetChange("nonsense");
    expect(schedule).not.toHaveBeenCalled();
    A.handleWbPresetChange("auto");
    expect(labels()).toEqual(["Auto White Balance"]);
  });

  it("auto WB with unequal channels writes temperature and tint to their own ops", () => {
    const r = new Uint32Array(256);
    const g = new Uint32Array(256);
    const b = new Uint32Array(256);
    r[200] = 10;
    g[100] = 10;
    b[50] = 10;
    develop.histogramData = { r, g, b };
    A.handleAutoWhiteBalance();
    const expected = computeAutoWhiteBalance({ r: 200 / 255, g: 100 / 255, b: 50 / 255 });
    expect(expected.temperature).not.toBe(expected.tint);
    expect(developView.temperature).toBe(expected.temperature);
    expect(developView.tint).toBe(expected.tint);
  });

  it("auto tone needs a histogram, then sets all six tone ops from computeAutoTone", () => {
    A.handleAutoTone();
    expect(schedule).not.toHaveBeenCalled();
    const r = new Uint32Array(256);
    r[26] = 1000;
    r[110] = 3000;
    r[254] = 500;
    develop.histogramData = { r, g: new Uint32Array(r), b: new Uint32Array(r) };
    A.handleAutoTone();
    const tone = computeAutoTone(develop.histogramData);
    expect(new Set([tone.highlights, tone.shadows, tone.whites, tone.blacks, tone.contrast]).size).toBeGreaterThan(3);
    expect(opNames().sort()).toEqual(["blacks", "contrast", "exposure", "highlights", "shadows", "whites"]);
    for (const k of /** @type {const} */ (["exposure", "contrast", "highlights", "shadows", "whites", "blacks"])) {
      expect(developView[k], k).toBe(tone[k]);
    }
    expect(labels()).toEqual(["Auto Tone"]);
  });
});

describe("canvas readouts", () => {
  it("are plain setters on the develop store", () => {
    A.handleSourceDimensions(6000, 4000);
    expect([develop.sourceWidth, develop.sourceHeight]).toEqual([6000, 4000]);
    const data = { r: new Uint32Array(1), g: new Uint32Array(1), b: new Uint32Array(1) };
    A.handleHistogramUpdate(data);
    expect(develop.histogramData).toEqual(data);
    A.handleHoverPixel({ r: 1, g: 2, b: 3 });
    expect(develop.hoverPixel).toEqual({ r: 1, g: 2, b: 3 });
    A.handleHoverPixel(null);
    expect(develop.hoverPixel).toBeNull();
    A.handleToggleClippingOverlay();
    expect(develop.showClippingOverlay).toBe(true);
    A.handleToggleClippingOverlay();
    expect(develop.showClippingOverlay).toBe(false);
  });
});

describe("hover previews", () => {
  beforeEach(() => {
    vi.spyOn(develop, "schedulePreview").mockImplementation((/** @type {() => unknown} */ fetch) => {
      fetch();
    });
    library.images = [/** @type {any} */ ({ version_id: 10, content_hash: "hash" })];
    develop.history = [/** @type {any} */ ({ id: 100 }), /** @type {any} */ ({ id: 101 })];
  });

  it("history: asks for the entry's preview with the open version, path and content hash", () => {
    A.handlePeekHistory(1);
    expect(api.previewHistoryEntry).toHaveBeenCalledWith(10, 101, "/p/a.raw", "hash");
  });

  it("history: ignores no open image and out-of-range rows", () => {
    A.handlePeekHistory(-1);
    A.handlePeekHistory(2);
    develop.versionId = null;
    A.handlePeekHistory(0);
    expect(api.previewHistoryEntry).not.toHaveBeenCalled();
  });

  it("snapshot: asks for the snapshot's preview; does nothing with no open image", () => {
    A.handlePeekSnapshot(7);
    expect(api.previewSnapshot).toHaveBeenCalledWith(10, 7, "/p/a.raw", "hash");
    develop.versionId = null;
    A.handlePeekSnapshot(8);
    expect(api.previewSnapshot).toHaveBeenCalledTimes(1);
  });
});

describe("snapshots", () => {
  it("create: flushes the pending edit first, then saves and appends the snapshot", async () => {
    /** @type {string[]} */
    const order = [];
    vi.spyOn(develop, "flushEditStack").mockImplementation(async () => {
      order.push("flush");
    });
    api.addSnapshot.mockImplementation(async () => {
      order.push("add");
      return { id: 5, name: "Before crop" };
    });
    develop.snapshots = [/** @type {any} */ ({ id: 1, name: "old" })];
    await A.handleCreateSnapshot("Before crop");
    expect(order).toEqual(["flush", "add"]);
    expect(api.addSnapshot).toHaveBeenCalledWith(10, "Before crop");
    expect(develop.snapshots.map((s) => s.id)).toEqual([1, 5]);
  });

  it("delete: a failed catalog delete leaves the list untouched", async () => {
    develop.snapshots = [/** @type {any} */ ({ id: 1 })];
    api.deleteSnapshot.mockRejectedValue("nope");
    await expect(A.handleDeleteSnapshot(1)).rejects.toBe("nope");
    expect(develop.snapshots.map((s) => s.id)).toEqual([1]);
  });

  it("create/delete do nothing with no open image", async () => {
    develop.versionId = null;
    await A.handleCreateSnapshot("x");
    await A.handleDeleteSnapshot(1);
    expect(api.addSnapshot).not.toHaveBeenCalled();
    expect(api.deleteSnapshot).not.toHaveBeenCalled();
  });

  it("delete: removes it in the catalog, then from the list", async () => {
    develop.snapshots = [/** @type {any} */ ({ id: 1 }), /** @type {any} */ ({ id: 2 })];
    await A.handleDeleteSnapshot(1);
    expect(api.deleteSnapshot).toHaveBeenCalledWith(10, 1);
    expect(develop.snapshots.map((s) => s.id)).toEqual([2]);
  });
});
