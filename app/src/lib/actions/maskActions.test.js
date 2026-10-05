import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ convertFileSrc: (/** @type {string} */ p) => p }));

import * as A from "./maskActions.js";
import { develop } from "$lib/state/develop.svelte.js";
import { developView } from "$lib/state/developView.svelte.js";
import { masks } from "$lib/state/masks.svelte.js";
import { addMask, createLinearGradientMask, rgbToHsl, nearestHslBand, computeEyedropperWhiteBalance, upsertToneCurve, MIN_TONE_CURVE_X_GAP } from "$lib/api/develop.js";

/** @type {ReturnType<typeof vi.spyOn>} */
let schedule;
/** @type {import('vitest').MockInstance<(label?: string) => Promise<void>>} */
let flush;
const labels = () => schedule.mock.calls.map((c) => c[0]);

beforeEach(() => {
  vi.useFakeTimers();
  schedule = vi.spyOn(develop, "scheduleFlush").mockImplementation(() => {});
  flush = vi.spyOn(develop, "flushEditStack").mockImplementation(async () => {});
  develop.editStack = { schema_version: 1, ops: [] };
  develop.versionId = 10;
  develop.gpuFallbackActive = false;
  develop.highlightedHslBand = null;
  masks.activeTool = null;
  masks.selectedMaskId = null;
  masks.selectedShapeId = null;
  masks.shapeTarget = null;
  masks.colorRangeResampleTarget = null;
  masks.eyedropperTarget = null;
});
afterEach(() => {
  vi.restoreAllMocks();
  vi.useRealTimers();
});

const op = (/** @type {string} */ name) => develop.editStack.ops.find((o) => o.op === name);

describe("handleGpuFallback", () => {
  it("records the flag; going active also drops whatever tool was selected", () => {
    masks.activeTool = "brush";
    A.handleGpuFallback(false);
    expect([develop.gpuFallbackActive, masks.activeTool]).toEqual([false, "brush"]);
    A.handleGpuFallback(true);
    expect([develop.gpuFallbackActive, masks.activeTool]).toEqual([true, null]);
  });
});

describe("handleMaskCreated", () => {
  const p = { x: 0.1, y: 0.2 };
  const cases = /** @type {const} */ ([
    [{ kind: "linear_gradient", start: p, end: { x: 0.9, y: 0.8 } }, "linear_gradient_mask", "Add Linear Gradient", true],
    [{ kind: "radial_gradient", center: p, radiusX: 0.2, radiusY: 0.3 }, "radial_gradient_mask", "Add Radial Gradient", true],
    [{ kind: "brush", id: "b1" }, "brush_mask", "Add Brush Mask", false],
    [{ kind: "color_range", refColor: { r: 0.5, g: 0.4, b: 0.3 } }, "color_range_mask", "Add Color Range Mask", true],
    [{ kind: "spot", id: "s1", initialDab: { x: 0.5, y: 0.5, radius: 0.05 } }, "spot_mask", "Add Spot Removal", false],
    [{ kind: "red_eye", center: p, radiusX: 0.05, radiusY: 0.05 }, "red_eye_mask", "Add Red Eye Correction", true],
  ]);

  it.each(cases)("%j: adds the right mask, selects it, labels the flush, and keeps the tool only for painted strokes", (placement, opName, label, oneShot) => {
    masks.activeTool = "some_tool";
    A.handleMaskCreated(/** @type {any} */ (placement));
    expect(develop.editStack.ops).toHaveLength(1);
    const mask = /** @type {any} */ (develop.editStack.ops[0]);
    expect(mask.op).toBe(opName);
    expect(masks.selectedMaskId).toBe(mask.id);
    expect(masks.list.map((m) => m.id)).toEqual([mask.id]);
    expect(labels()).toEqual([label]);
    expect(masks.activeTool).toBe(oneShot ? null : "some_tool");
  });

  it("passes each shape's own geometry in the right argument order", () => {
    A.handleMaskCreated({ kind: "linear_gradient", start: { x: 0.1, y: 0.2 }, end: { x: 0.8, y: 0.9 } });
    expect(develop.editStack.ops[0]).toMatchObject({ start: { x: 0.1, y: 0.2 }, end: { x: 0.8, y: 0.9 } });
    A.handleMaskCreated({ kind: "radial_gradient", center: { x: 0.3, y: 0.4 }, radiusX: 0.21, radiusY: 0.32 });
    expect(develop.editStack.ops[1]).toMatchObject({ center: { x: 0.3, y: 0.4 }, radiusX: 0.21, radiusY: 0.32 });
    A.handleMaskCreated({ kind: "red_eye", center: { x: 0.6, y: 0.7 }, radiusX: 0.04, radiusY: 0.05 });
    expect(develop.editStack.ops[2]).toMatchObject({ center: { x: 0.6, y: 0.7 }, radiusX: 0.04, radiusY: 0.05 });
  });

  it("carries each placement's own geometry / id / colour into the mask", () => {
    A.handleMaskCreated({ kind: "brush", id: "b1" });
    expect(/** @type {any} */ (develop.editStack.ops[0]).id).toBe("b1");
    A.handleMaskCreated({ kind: "spot", id: "s1", initialDab: { x: 0.5, y: 0.5, radius: 0.05 } });
    expect(/** @type {any} */ (develop.editStack.ops[1]).id).toBe("s1");
    A.handleMaskCreated({ kind: "radial_gradient", center: { x: 0.3, y: 0.4 }, radiusX: 0.2, radiusY: 0.25 });
    expect(JSON.stringify(develop.editStack.ops[2])).toContain("0.3");
    A.handleMaskCreated({ kind: "color_range", refColor: { r: 0.11, g: 0.22, b: 0.33 } });
    expect(/** @type {any} */ (develop.editStack.ops[3]).refColor).toEqual({ r: 0.11, g: 0.22, b: 0.33 });
  });

  it("appends: earlier masks are kept and the newest becomes the selection", () => {
    A.handleMaskCreated({ kind: "brush", id: "b1" });
    A.handleMaskCreated({ kind: "brush", id: "b2" });
    expect(masks.list.map((m) => m.id)).toEqual(["b1", "b2"]);
    expect(masks.selectedMaskId).toBe("b2");
  });
});

describe("luminance range, update, delete", () => {
  it("luminance range is created on tool-select with no tool change", () => {
    masks.activeTool = "brush";
    A.handleCreateLuminanceRangeMask();
    const mask = /** @type {any} */ (develop.editStack.ops[0]);
    expect(mask.op).toBe("luminance_range_mask");
    expect(masks.selectedMaskId).toBe(mask.id);
    expect(masks.activeTool).toBe("brush");
    expect(labels()).toEqual(["Add Luminance Range Mask"]);
  });

  it("update patches one mask by id under the Edit Mask label", () => {
    A.handleMaskCreated({ kind: "brush", id: "b1" });
    A.handleMaskCreated({ kind: "brush", id: "b2" });
    schedule.mockClear();
    A.handleMaskUpdated("b1", { exposure: 0.7 });
    expect(/** @type {any} */ (masks.list[0]).exposure).toBe(0.7);
    expect(/** @type {any} */ (masks.list[1]).exposure).not.toBe(0.7);
    expect(labels()).toEqual(["Edit Mask"]);
  });

  it("delete: nothing selected does nothing", () => {
    develop.editStack = addMask(develop.editStack, /** @type {any} */ (createLinearGradientMask({ x: 0, y: 0 }, { x: 1, y: 1 })));
    A.handleMaskDeleted();
    expect(masks.list).toHaveLength(1);
    expect(flush).not.toHaveBeenCalled();
  });

  it("delete: removes the selected mask, clears the selection and flushes immediately (not debounced)", () => {
    A.handleMaskCreated({ kind: "brush", id: "b1" });
    A.handleMaskCreated({ kind: "brush", id: "b2" });
    masks.selectedMaskId = "b1";
    schedule.mockClear();
    A.handleMaskDeleted();
    expect(masks.list.map((m) => m.id)).toEqual(["b2"]);
    expect(masks.selectedMaskId).toBeNull();
    expect(flush).toHaveBeenCalledWith("Delete Mask");
    expect(schedule).not.toHaveBeenCalled();
  });
});

describe("colour-range re-sample", () => {
  it("toggle: with no selected mask does nothing", () => {
    A.handleResampleColorToggle();
    expect([masks.activeTool, masks.colorRangeResampleTarget]).toEqual([null, null]);
  });

  it("toggle: starts resampling the selected mask, and a second toggle cancels it", () => {
    masks.selectedMaskId = "m1";
    A.handleResampleColorToggle();
    expect([masks.activeTool, masks.colorRangeResampleTarget, masks.isResamplingColor]).toEqual(["color_range", "m1", true]);
    A.handleResampleColorToggle();
    expect([masks.activeTool, masks.colorRangeResampleTarget, masks.isResamplingColor]).toEqual([null, null, false]);
  });

  it("commit: patches the EXISTING mask's refColor (no new mask), exits resample mode, labels the flush", () => {
    A.handleMaskCreated({ kind: "color_range", refColor: { r: 0.1, g: 0.1, b: 0.1 } });
    const id = /** @type {string} */ (masks.selectedMaskId);
    masks.activeTool = "color_range";
    masks.colorRangeResampleTarget = id;
    schedule.mockClear();
    A.handleColorRangeResampled(id, { r: 0.9, g: 0.8, b: 0.7 });
    expect(masks.list).toHaveLength(1);
    expect(/** @type {any} */ (masks.list[0]).refColor).toEqual({ r: 0.9, g: 0.8, b: 0.7 });
    expect([masks.colorRangeResampleTarget, masks.activeTool]).toEqual([null, null]);
    expect(labels()).toEqual(["Adjust Color Range"]);
  });
});

describe("eyedropper toggle", () => {
  it("activates the eyedropper for a target, reports it active, and toggling the same target cancels", () => {
    A.handleEyedropperToggle("hsl_band");
    expect([masks.activeTool, masks.eyedropperTarget]).toEqual(["eyedropper", "hsl_band"]);
    expect(A.isEyedropperActive("hsl_band")).toBe(true);
    expect(A.isEyedropperActive("white_balance")).toBe(false);
    A.handleEyedropperToggle("hsl_band");
    expect([masks.activeTool, masks.eyedropperTarget]).toEqual([null, null]);
    expect(A.isEyedropperActive("hsl_band")).toBe(false);
  });

  it("a matching target without the eyedropper tool active does not count as active: toggling activates", () => {
    masks.eyedropperTarget = "hsl_band";
    masks.activeTool = "brush";
    A.handleEyedropperToggle("hsl_band");
    expect([masks.activeTool, masks.eyedropperTarget]).toEqual(["eyedropper", "hsl_band"]);
  });

  it("toggling a different target switches to it rather than cancelling", () => {
    A.handleEyedropperToggle("hsl_band");
    A.handleEyedropperToggle("white_balance");
    expect([masks.activeTool, masks.eyedropperTarget]).toEqual(["eyedropper", "white_balance"]);
  });

  it("isEyedropperActive needs the eyedropper tool, not just a matching target", () => {
    masks.eyedropperTarget = "hsl_band";
    masks.activeTool = "brush";
    expect(A.isEyedropperActive("hsl_band")).toBe(false);
  });
});

describe("handleEyedropperSampled", () => {
  const color = { r: 0.8, g: 0.4, b: 0.2 };

  it("always ends the eyedropper gesture, and a missing target changes nothing else", () => {
    masks.activeTool = "eyedropper";
    A.handleEyedropperSampled(color);
    expect(masks.activeTool).toBeNull();
    expect(develop.editStack.ops).toEqual([]);
    expect(schedule).not.toHaveBeenCalled();
  });

  it.each([
    ["split_toning_shadows", "shadows"],
    ["split_toning_highlights", "highlights"],
  ])("%s: writes the sampled hue and saturation (percent) into the %s zone", (target, zone) => {
    masks.activeTool = "eyedropper";
    masks.eyedropperTarget = /** @type {any} */ (target);
    A.handleEyedropperSampled(color);
    const { h, s } = rgbToHsl(color.r, color.g, color.b);
    const st = /** @type {any} */ (developView.splitToning);
    expect(st[zone].hue).toBeCloseTo(h);
    expect(st[zone].saturation).toBeCloseTo(s * 100);
    const other = zone === "shadows" ? "highlights" : "shadows";
    expect(st[other].saturation).not.toBeCloseTo(s * 100);
    expect(labels()).toEqual(["Split Toning"]);
    expect([masks.activeTool, masks.eyedropperTarget]).toEqual([null, null]);
  });

  it("hsl_band: navigation only (no edit, no flush); the highlight clears itself after 1.5 s", () => {
    masks.eyedropperTarget = "hsl_band";
    const green = { r: 0.2, g: 0.8, b: 0.3 };
    A.handleEyedropperSampled(green);
    const { h } = rgbToHsl(green.r, green.g, green.b);
    expect(nearestHslBand(h)).not.toBe(nearestHslBand(0.5));
    expect(develop.highlightedHslBand).toBe(nearestHslBand(h));
    expect(develop.editStack.ops).toEqual([]);
    expect(schedule).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1499);
    expect(develop.highlightedHslBand).not.toBeNull();
    vi.advanceTimersByTime(1);
    expect(develop.highlightedHslBand).toBeNull();
  });

  it("hsl_band: a second sample restarts the 1.5 s window", () => {
    masks.eyedropperTarget = "hsl_band";
    A.handleEyedropperSampled(color);
    vi.advanceTimersByTime(1000);
    masks.eyedropperTarget = "hsl_band";
    A.handleEyedropperSampled(color);
    vi.advanceTimersByTime(1000);
    expect(develop.highlightedHslBand).not.toBeNull();
    vi.advanceTimersByTime(500);
    expect(develop.highlightedHslBand).toBeNull();
  });

  it("white_balance: writes temperature and tint from the eyedropper formula", () => {
    masks.eyedropperTarget = "white_balance";
    A.handleEyedropperSampled(color);
    const wb = computeEyedropperWhiteBalance(color);
    expect(wb.temperature).not.toBe(wb.tint);
    expect(developView.temperature).toBe(wb.temperature);
    expect(developView.tint).toBe(wb.tint);
    expect(labels()).toEqual(["White Balance Eyedropper"]);
  });

  it("tone_curve_point: inserts a point at the sampled lightness, on the current curve (shape unchanged)", () => {
    masks.eyedropperTarget = "tone_curve_point";
    A.handleEyedropperSampled({ r: 0.5, g: 0.5, b: 0.5 });
    const pts = developView.toneCurvePoints;
    expect(pts).toHaveLength(3);
    expect(pts[1].x).toBeCloseTo(0.5);
    expect(pts[1].y).toBeCloseTo(0.5); // identity curve: y seeded at the curve's own value
    expect(labels()).toEqual(["Tone Curve"]);
  });

  it("tone_curve_point: a point too close to an existing one is rejected without an edit", () => {
    develop.editStack = upsertToneCurve(develop.editStack, [{ x: 0, y: 0 }, { x: 0.5, y: 0.6 }, { x: 1, y: 1 }]);
    masks.eyedropperTarget = "tone_curve_point";
    const before = develop.editStack;
    A.handleEyedropperSampled({ r: 0.5 + MIN_TONE_CURVE_X_GAP / 4, g: 0.5 + MIN_TONE_CURVE_X_GAP / 4, b: 0.5 + MIN_TONE_CURVE_X_GAP / 4 });
    expect(develop.editStack).toBe(before);
    expect(schedule).not.toHaveBeenCalled();
    expect([masks.activeTool, masks.eyedropperTarget]).toEqual([null, null]);
  });
});

describe("composable masking: shapes (RFC-0025 slice 1b)", () => {
  const radial = { kind: "radial_gradient", center: { x: 0.5, y: 0.5 }, radiusX: 0.3, radiusY: 0.3 };
  const owner = () => /** @type {any} */ (develop.editStack.ops[0]);

  /** One radial mask, selected. */
  function oneMask() {
    A.handleMaskCreated(/** @type {any} */ (radial));
    schedule.mockClear();
  }

  it("Add shape arms the placement tool for the chosen kind and mask, without touching the stack", () => {
    oneMask();
    const before = develop.editStack;
    A.handleAddShape("subtract", "radial_gradient_mask");
    expect(masks.activeTool).toBe("radial_gradient");
    expect(masks.shapeTarget).toEqual({ maskId: owner().id, combine: "subtract", tool: "radial_gradient" });
    expect(develop.editStack).toBe(before);
    for (const [op, tool] of /** @type {const} */ ([["linear_gradient_mask", "linear_gradient"], ["brush_mask", "brush"], ["color_range_mask", "color_range"]])) {
      A.handleAddShape("add", op);
      expect([masks.activeTool, masks.shapeTarget?.tool]).toEqual([tool, tool]);
    }
  });

  it("the next placement becomes a shape of that mask: selected, one flush, no new mask, tool released", () => {
    oneMask();
    A.handleAddShape("intersect", "linear_gradient_mask");
    A.handleMaskCreated({ kind: "linear_gradient", start: { x: 0.1, y: 0.5 }, end: { x: 0.9, y: 0.5 } });
    expect(develop.editStack.ops).toHaveLength(1);
    const mods = owner().modifiers;
    expect(mods).toHaveLength(1);
    expect(mods[0]).toMatchObject({ combine: "intersect", shape: { op: "linear_gradient_mask", start: { x: 0.1, y: 0.5 } } });
    expect("exposure" in mods[0].shape).toBe(false); // a shape carries no adjustments
    expect(masks.selectedMaskId).toBe(owner().id);
    expect(masks.selectedShapeId).toBe(mods[0].id);
    expect(masks.shapeTarget).toBeNull();
    expect(masks.activeTool).toBeNull();
    expect(labels()).toEqual(["Add Shape"]);
  });

  it("a brush shape keeps the Brush tool, uses the canvas's id, and later strokes patch the shape", () => {
    oneMask();
    A.handleAddShape("subtract", "brush_mask");
    A.handleMaskCreated({ kind: "brush", id: "sb1" });
    expect(masks.activeTool).toBe("brush");
    expect(masks.shapeTarget).toBeNull();
    expect(masks.selectedShapeId).toBe("sb1");
    A.handleMaskUpdated("sb1", { dabs: [{ x: 0.5, y: 0.5, radius: 0.1, hardness: 70, flow: 1, mode: "add" }] });
    expect(owner().modifiers[0].shape.dabs).toHaveLength(1);
    expect(owner().modifiers[0].shape.op).toBe("brush_mask");
    expect(labels()).toEqual(["Add Shape", "Edit Mask Shape"]);
    expect(develop.editStack.ops).toHaveLength(1);
  });

  it("a colour-range shape takes the clicked colour; luminance range is created at once with no tool", () => {
    oneMask();
    A.handleAddShape("add", "color_range_mask");
    A.handleMaskCreated({ kind: "color_range", refColor: { r: 0.1, g: 0.2, b: 0.3 } });
    expect(owner().modifiers[0].shape.refColor).toEqual({ r: 0.1, g: 0.2, b: 0.3 });
    masks.activeTool = null;
    A.handleAddShape("subtract", "luminance_range_mask");
    expect(masks.activeTool).toBeNull();
    expect(masks.shapeTarget).toBeNull();
    expect(owner().modifiers.map((/** @type {any} */ m) => [m.combine, m.shape.op])).toEqual([
      ["add", "color_range_mask"],
      ["subtract", "luminance_range_mask"],
    ]);
  });

  it("with nothing selected Add shape does nothing; an ineligible placement is refused and the arming consumed", () => {
    A.handleAddShape("add", "radial_gradient_mask");
    expect([masks.activeTool, masks.shapeTarget]).toEqual([null, null]);
    oneMask();
    masks.activeTool = null;
    // a spot placement is not an eligible shape
    A.handleMaskCreated({ kind: "spot", id: "s", initialDab: { x: 0.5, y: 0.5, radius: 0.05 } });
    masks.selectedMaskId = owner().id;
    masks.shapeTarget = { maskId: owner().id, combine: "add", tool: "brush" };
    masks.activeTool = "brush";
    A.handleMaskCreated({ kind: "spot", id: "s2", initialDab: { x: 0.5, y: 0.5, radius: 0.05 } });
    expect(owner().modifiers).toBeUndefined();
    expect(masks.shapeTarget).toBeNull();
  });

  it("edits to a shape go to the shape (sliders, handles, re-sample); the mask is untouched", () => {
    oneMask();
    A.handleAddShape("add", "linear_gradient_mask");
    A.handleMaskCreated({ kind: "linear_gradient", start: { x: 0, y: 0 }, end: { x: 1, y: 1 } });
    const sid = /** @type {string} */ (masks.selectedShapeId);
    schedule.mockClear();
    A.handleMaskUpdated(sid, { feather: 33 });
    expect(owner().modifiers[0].shape.feather).toBe(33);
    expect(owner().feather).not.toBe(33);
    expect(labels()).toEqual(["Edit Mask Shape"]);
    A.handleMaskUpdated(owner().id, { exposure: 1.1 });
    expect(owner().exposure).toBe(1.1);
    expect(labels()).toEqual(["Edit Mask Shape", "Edit Mask"]);
  });

  it("combine change, selection by id, and removal", () => {
    oneMask();
    A.handleAddShape("add", "linear_gradient_mask");
    A.handleMaskCreated({ kind: "linear_gradient", start: { x: 0, y: 0 }, end: { x: 1, y: 1 } });
    const sid = /** @type {string} */ (masks.selectedShapeId);
    A.handleShapeCombineChanged(sid, "subtract");
    expect(owner().modifiers[0].combine).toBe("subtract");
    A.handleShapeSelected(null);
    expect(masks.selectedShapeId).toBeNull();
    A.handleMaskSelected(sid); // a click on the shape's handle selects the shape under its owner
    expect([masks.selectedMaskId, masks.selectedShapeId]).toEqual([owner().id, sid]);
    A.handleMaskSelected(owner().id); // a click on the mask's own handle selects the base shape
    expect([masks.selectedMaskId, masks.selectedShapeId]).toEqual([owner().id, null]);
    masks.selectedShapeId = sid;
    A.handleShapeRemoved(sid);
    expect(owner().modifiers).toBeUndefined();
    expect(masks.selectedShapeId).toBeNull();
    expect(flush).toHaveBeenCalledWith("Remove Mask Shape");
  });

  it("re-sampling a colour shape patches the shape and exits resample mode", () => {
    oneMask();
    A.handleAddShape("add", "color_range_mask");
    A.handleMaskCreated({ kind: "color_range", refColor: { r: 0, g: 0, b: 0 } });
    const sid = /** @type {string} */ (masks.selectedShapeId);
    A.handleResampleColorToggle();
    expect(masks.colorRangeResampleTarget).toBe(sid); // the shape, not its mask
    A.handleColorRangeResampled(sid, { r: 0.9, g: 0.8, b: 0.7 });
    expect(owner().modifiers[0].shape.refColor).toEqual({ r: 0.9, g: 0.8, b: 0.7 });
    expect([masks.colorRangeResampleTarget, masks.activeTool]).toEqual([null, null]);
  });
});
