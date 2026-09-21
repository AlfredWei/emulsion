import { describe, it, expect } from "vitest";
import { flushSync } from "svelte";
import { MaskStore, createMaskStore } from "./masks.svelte.js";
import { DevelopStore } from "./develop.svelte.js";
import { createLibraryStore } from "./library.svelte.js";
import { addMask, removeMask, createLinearGradientMask, upsertOp } from "$lib/api/develop.js";

function setup() {
  const develop = new DevelopStore(createLibraryStore());
  const masks = new MaskStore(develop);
  return { develop, masks };
}

/** Runs `install()` in an effect root; returns the stop function. */
function installed(/** @type {MaskStore} */ masks) {
  const stop = $effect.root(() => masks.install());
  flushSync();
  return stop;
}

describe("MaskStore state", () => {
  it("starts with no tool, no selection, the documented brush defaults and overlays on", () => {
    const { masks } = setup();
    expect([masks.activeTool, masks.selectedMaskId, masks.colorRangeResampleTarget, masks.eyedropperTarget]).toEqual([null, null, null, null]);
    expect([masks.brushSize, masks.brushHardness, masks.brushFlow, masks.eraseMode, masks.spotBrushSize]).toEqual([0.05, 70, 1, false, 0.02]);
    expect([masks.maskOverlaysVisible, masks.showMaskOverlay]).toEqual([true, true]);
    expect(masks.list).toEqual([]);
    expect(masks.selectedMask).toBeNull();
    expect(masks.isResamplingColor).toBe(false);
  });

  it("each factory call gets its own state", () => {
    const a = createMaskStore(setup().develop);
    const b = createMaskStore(setup().develop);
    a.activeTool = "brush";
    a.brushSize = 0.5;
    expect(b.activeTool).toBeNull();
    expect(b.brushSize).toBe(0.05);
  });

  it("list holds only the mask ops of the develop edit stack, and follows every replacement of it", () => {
    const { develop, masks } = setup();
    const m1 = createLinearGradientMask({ x: 0, y: 0 }, { x: 1, y: 1 });
    develop.editStack = upsertOp(addMask(develop.editStack, /** @type {any} */ (m1)), "exposure", 1);
    expect(masks.list.map((m) => m.id)).toEqual([m1.id]);
    develop.editStack = removeMask(develop.editStack, m1.id);
    expect(masks.list).toEqual([]);
  });

  it("selectedMask is the listed mask with the selected id, and null once it is gone or unknown", () => {
    const { develop, masks } = setup();
    const m1 = createLinearGradientMask({ x: 0, y: 0 }, { x: 1, y: 1 });
    const m2 = createLinearGradientMask({ x: 0, y: 0 }, { x: 0, y: 1 });
    develop.editStack = addMask(addMask(develop.editStack, /** @type {any} */ (m1)), /** @type {any} */ (m2));
    masks.selectedMaskId = m2.id;
    expect(masks.selectedMask?.id).toBe(m2.id);
    masks.selectedMaskId = "nope";
    expect(masks.selectedMask).toBeNull();
    masks.selectedMaskId = m1.id;
    develop.editStack = removeMask(develop.editStack, m1.id);
    expect(masks.selectedMask).toBeNull();
  });

  it("isResamplingColor is true only while the resample target is the selected mask", () => {
    const { masks } = setup();
    masks.selectedMaskId = "a";
    masks.colorRangeResampleTarget = "a";
    expect(masks.isResamplingColor).toBe(true);
    masks.colorRangeResampleTarget = "b";
    expect(masks.isResamplingColor).toBe(false);
    masks.colorRangeResampleTarget = null;
    masks.selectedMaskId = null;
    expect(masks.isResamplingColor).toBe(false); // null === null must not count
  });
});

describe("MaskStore.install: self-cleaning targets", () => {
  it("keeps a colour-range resample target while the tool is active and the target is selected", () => {
    const { masks } = setup();
    const stop = installed(masks);
    masks.activeTool = "color_range";
    masks.selectedMaskId = "a";
    masks.colorRangeResampleTarget = "a";
    flushSync();
    expect(masks.colorRangeResampleTarget).toBe("a");
    stop();
  });

  it("clears the resample target when the tool changes", () => {
    const { masks } = setup();
    const stop = installed(masks);
    masks.activeTool = "color_range";
    masks.selectedMaskId = "a";
    masks.colorRangeResampleTarget = "a";
    flushSync();
    masks.activeTool = "brush";
    flushSync();
    expect(masks.colorRangeResampleTarget).toBeNull();
    stop();
  });

  it("clears the resample target when the tool is switched off", () => {
    const { masks } = setup();
    const stop = installed(masks);
    masks.activeTool = "color_range";
    masks.selectedMaskId = "a";
    masks.colorRangeResampleTarget = "a";
    flushSync();
    masks.activeTool = null;
    flushSync();
    expect(masks.colorRangeResampleTarget).toBeNull();
    stop();
  });

  it("clears the resample target when a different mask gets selected", () => {
    const { masks } = setup();
    const stop = installed(masks);
    masks.activeTool = "color_range";
    masks.selectedMaskId = "a";
    masks.colorRangeResampleTarget = "a";
    flushSync();
    masks.selectedMaskId = "b";
    flushSync();
    expect(masks.colorRangeResampleTarget).toBeNull();
    stop();
  });

  it("clears an invalid target the moment it is set", () => {
    const { masks } = setup();
    const stop = installed(masks);
    masks.colorRangeResampleTarget = "a"; // tool is not color_range
    flushSync();
    expect(masks.colorRangeResampleTarget).toBeNull();
    stop();
  });

  it("keeps an eyedropper target while the eyedropper tool is active and clears it otherwise", () => {
    const { masks } = setup();
    const stop = installed(masks);
    masks.activeTool = "eyedropper";
    masks.eyedropperTarget = "hsl_band";
    flushSync();
    expect(masks.eyedropperTarget).toBe("hsl_band");
    masks.activeTool = "brush";
    flushSync();
    expect(masks.eyedropperTarget).toBeNull();
    stop();
  });

  it("the two effects are independent: the eyedropper target does not depend on the selected mask", () => {
    const { masks } = setup();
    const stop = installed(masks);
    masks.activeTool = "eyedropper";
    masks.eyedropperTarget = "white_balance";
    masks.selectedMaskId = "x";
    flushSync();
    masks.selectedMaskId = "y";
    flushSync();
    expect(masks.eyedropperTarget).toBe("white_balance");
    stop();
  });

  it("does nothing until installed, and stops reacting after the stop function runs", () => {
    const { masks } = setup();
    masks.colorRangeResampleTarget = "a";
    masks.eyedropperTarget = "hsl_band";
    flushSync();
    expect([masks.colorRangeResampleTarget, masks.eyedropperTarget]).toEqual(["a", "hsl_band"]); // not installed yet
    const stop = installed(masks);
    expect([masks.colorRangeResampleTarget, masks.eyedropperTarget]).toEqual([null, null]);
    stop();
    masks.colorRangeResampleTarget = "a";
    masks.eyedropperTarget = "hsl_band";
    flushSync();
    expect([masks.colorRangeResampleTarget, masks.eyedropperTarget]).toEqual(["a", "hsl_band"]);
  });
});
