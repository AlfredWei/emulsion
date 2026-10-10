import { describe, it, expect, vi } from "vitest";
import { buildPanelHeaderMenu, buildCanvasMenu, buildMaskMenu, buildShapeMenu } from "./developMenus.js";
import { isItem } from "./model.js";

const items = (/** @type {import('./model.js').MenuEntry[]} */ entries) => entries.filter(isItem);
const ids = (/** @type {import('./model.js').MenuEntry[]} */ entries) => items(entries).map((e) => e.id);
/** @param {import('./model.js').MenuEntry[]} entries @param {string} id @returns {import('./model.js').MenuItem | undefined} */
function find(entries, id) {
  for (const e of entries) {
    if (!isItem(e)) continue;
    if (e.id === id) return e;
    const inner = e.children ? find(e.children, id) : undefined;
    if (inner) return inner;
  }
  return undefined;
}

describe("panel header menu", () => {
  const cmd = () => ({ toggleVisibility: vi.fn(), reset: vi.fn(), solo: vi.fn(), showAll: vi.fn() });
  const state = (/** @type {object} */ over = {}) => ({ title: "Vignette", hidden: false, othersVisible: true, anyHidden: false, ...over });

  it("offers hide, reset, solo and show-all, naming the panel", () => {
    const menu = buildPanelHeaderMenu(state(), cmd());
    expect(ids(menu)).toEqual(["toggle-panel", "reset-panel", "solo-panel", "show-all-panels"]);
    expect(find(menu, "toggle-panel")?.label).toBe("Hide Vignette");
    expect(find(menu, "reset-panel")?.label).toBe("Reset Vignette");
    expect(find(menu, "reset-panel")?.confirm).toBeUndefined();
  });

  it("a hidden panel is offered Show; Show All is enabled only when something is hidden", () => {
    const menu = buildPanelHeaderMenu(state({ hidden: true, anyHidden: true }), cmd());
    expect(find(menu, "toggle-panel")?.label).toBe("Show Vignette");
    expect(find(menu, "show-all-panels")?.disabled).toBe(false);
    const none = buildPanelHeaderMenu(state(), cmd());
    expect(find(none, "show-all-panels")?.disabled).toBe(true);
    expect(find(none, "show-all-panels")?.reason).toBeTruthy();
  });

  it("Solo is pointless (disabled, with the reason) when this is the only visible panel, but fine if it is hidden", () => {
    const only = buildPanelHeaderMenu(state({ othersVisible: false }), cmd());
    expect(find(only, "solo-panel")?.disabled).toBe(true);
    expect(find(only, "solo-panel")?.reason).toContain("only visible");
    expect(find(buildPanelHeaderMenu(state({ othersVisible: false, hidden: true }), cmd()), "solo-panel")?.disabled).toBe(false);
  });

  it("each row runs its command", () => {
    const c = cmd();
    const menu = buildPanelHeaderMenu(state({ anyHidden: true }), c);
    for (const id of ["toggle-panel", "reset-panel", "solo-panel", "show-all-panels"]) find(menu, id)?.run?.();
    expect([c.toggleVisibility, c.reset, c.solo, c.showAll].map((f) => f.mock.calls.length)).toEqual([1, 1, 1, 1]);
  });
});

describe("canvas menu", () => {
  const COMMANDS = ["undo", "redo", "toggleBeforeAfter", "zoomFit", "zoomActual", "toggleClipping", "toggleMaskOverlay", "copySettings", "pasteSettings", "resetSettings"];
  const cmd = () => /** @type {any} */ (Object.fromEntries(COMMANDS.map((c) => [c, vi.fn()])));
  const state = (/** @type {object} */ over = {}) => ({
    undoLabel: "Exposure",
    redoLabel: null,
    showOriginal: false,
    clipping: false,
    maskOverlay: true,
    hasMasks: true,
    hasCopiedSettings: false,
    ...over,
  });

  it("names what Undo and Redo would step over, and disables the one with nothing to do", () => {
    const menu = buildCanvasMenu(state({ redoLabel: "Contrast" }), cmd());
    expect(find(menu, "undo")?.label).toBe("Undo Exposure");
    expect(find(menu, "redo")?.label).toBe("Redo Contrast");
    const none = buildCanvasMenu(state({ undoLabel: null }), cmd());
    expect(find(none, "undo")?.disabled).toBe(true);
    expect(find(none, "undo")?.label).toBe("Undo");
    expect(find(none, "redo")?.disabled).toBe(true);
    expect(find(none, "redo")?.reason).toBe("Nothing to redo");
  });

  it("the view toggles show their current state", () => {
    const menu = buildCanvasMenu(state({ showOriginal: true, clipping: true, maskOverlay: false }), cmd());
    expect(find(menu, "before-after")?.checked).toBe(true);
    expect(find(menu, "clipping")?.checked).toBe(true);
    expect(find(menu, "mask-overlay")?.checked).toBe(false);
  });

  it("Mask Overlay is disabled with a reason when the photo has no masks; Zoom is a submenu of Fit and 100%", () => {
    const menu = buildCanvasMenu(state({ hasMasks: false }), cmd());
    expect(find(menu, "mask-overlay")?.disabled).toBe(true);
    expect(find(menu, "mask-overlay")?.reason).toBeTruthy();
    expect(ids(find(menu, "zoom")?.children ?? [])).toEqual(["zoom-fit", "zoom-100"]);
  });

  it("Paste is disabled until something was copied; Reset is red and hands over to the existing confirmation", () => {
    const menu = buildCanvasMenu(state(), cmd());
    expect(find(menu, "paste-settings")?.disabled).toBe(true);
    expect(find(buildCanvasMenu(state({ hasCopiedSettings: true }), cmd()), "paste-settings")?.disabled).toBe(false);
    const reset = find(menu, "reset-settings");
    expect(reset?.danger).toBe(true);
    expect(reset?.confirm).toBeUndefined();
  });

  it("each command is wired", () => {
    const c = cmd();
    const menu = buildCanvasMenu(state({ redoLabel: "x", hasCopiedSettings: true }), c);
    for (const id of ["undo", "redo", "before-after", "zoom-fit", "zoom-100", "clipping", "mask-overlay", "copy-settings", "paste-settings", "reset-settings"]) find(menu, id)?.run?.();
    for (const name of COMMANDS) expect(c[name]).toHaveBeenCalledTimes(1);
  });
});

describe("mask menu", () => {
  const cmd = () => ({ toggleInvert: vi.fn(), toggleOverlay: vi.fn(), close: vi.fn(), remove: vi.fn() });
  const state = (/** @type {object} */ over = {}) => ({ title: "Brush", canInvert: true, inverted: false, overlayCapable: true, overlayOn: true, ...over });

  it("a brush mask offers Invert, Show Overlay, Close and a confirmed Delete, in that order", () => {
    const menu = buildMaskMenu(state(), cmd());
    expect(ids(menu)).toEqual(["invert-mask", "mask-overlay", "close-mask", "delete-mask"]);
    const del = find(menu, "delete-mask");
    expect(del?.label).toBe("Delete Brush…");
    expect(del?.danger).toBe(true);
    expect(del?.confirm?.message).toContain("undo");
  });

  it("a spot or red-eye mask has no Invert, and a gradient has no overlay row (as in the panel)", () => {
    const spot = buildMaskMenu(state({ title: "Spot Removal", canInvert: false, overlayCapable: false }), cmd());
    expect(ids(spot)).toEqual(["close-mask", "delete-mask"]);
  });

  it("the checks show the current invert and overlay state and the commands run", () => {
    const c = cmd();
    const menu = buildMaskMenu(state({ inverted: true, overlayOn: false }), c);
    expect(find(menu, "invert-mask")?.checked).toBe(true);
    expect(find(menu, "mask-overlay")?.checked).toBe(false);
    for (const id of ["invert-mask", "mask-overlay", "close-mask", "delete-mask"]) find(menu, id)?.run?.();
    expect([c.toggleInvert, c.toggleOverlay, c.close, c.remove].map((f) => f.mock.calls.length)).toEqual([1, 1, 1, 1]);
  });
});

describe("shape menu", () => {
  it("edits or removes a shape; the shape being edited cannot be 'edited' again; Remove asks first", () => {
    const c = { select: vi.fn(), remove: vi.fn() };
    const menu = buildShapeMenu({ name: "Radial", selected: true }, c);
    expect(find(menu, "select-shape")?.disabled).toBe(true);
    expect(find(menu, "select-shape")?.reason).toBeTruthy();
    const remove = find(menu, "remove-shape");
    expect(remove?.danger).toBe(true);
    expect(remove?.confirm?.message).toContain("radial");
    expect(find(buildShapeMenu({ name: "Radial", selected: false }, c), "select-shape")?.disabled).toBe(false);
    find(menu, "remove-shape")?.run?.();
    expect(c.remove).toHaveBeenCalledTimes(1);
  });
});
