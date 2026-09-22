import { describe, it, expect, vi, beforeEach } from "vitest";
import { createKeyboardHandlers } from "./keyboard.js";
import { DEFAULT_SHORTCUTS } from "./shortcuts.js";
import { OVERLAY_CAPABLE_MASK_OPS } from "./api/develop.js";

// The vitest environment is DOM-free (see vitest.config.js); the handler does
// `target instanceof HTMLInputElement`, so provide inert stand-ins.
class FakeInput {
  /** @param {string} [type] */
  constructor(type = "text") {
    this.type = type;
  }
}
class FakeTextArea {}
class FakeSelect {}
beforeEach(() => {
  vi.stubGlobal("HTMLInputElement", FakeInput);
  vi.stubGlobal("HTMLTextAreaElement", FakeTextArea);
  vi.stubGlobal("HTMLSelectElement", FakeSelect);
});

/** A fake KeyboardContext: plain data plus spies. Overrides win. */
function makeCtx(over = {}) {
  return {
    activeModule: "library",
    exportItems: null,
    settingsOpen: false,
    backupPromptOpen: false,
    creatingSnapshot: false,
    creatingPreset: false,
    confirmingDeletePresetId: null,
    confirmingRemoval: false,
    creatingCollection: false,
    creatingSmartCollection: false,
    creatingCollectionWithImages: false,
    shortcuts: { ...DEFAULT_SHORTCUTS },
    libraryViewMode: /** @type {"grid" | "loupe" | "compare" | "survey" | "map"} */ ("grid"),
    spacePanning: false,
    selectedMask: /** @type {{ op?: string } | null} */ (null),
    showMaskOverlay: false,
    maskOverlaysVisible: true,
    showOriginal: false,
    selectedIds: new Set(/** @type {number[]} */ ([])),
    selectedId: /** @type {number | null} */ (null),
    filteredImages: [{ version_id: 11 }, { version_id: 12 }],
    selectedImage: /** @type {{ flag?: string, color_label?: string } | null} */ (null),
    handleUndo: vi.fn(),
    handleRedo: vi.fn(),
    selectNextImage: vi.fn(),
    selectPrevImage: vi.fn(),
    selectGridStep: vi.fn(),
    showMapView: vi.fn(),
    switchModule: vi.fn(async () => {}),
    openDevelop: vi.fn(async () => {}),
    handleSelectAll: vi.fn(),
    handleDeselectAll: vi.fn(),
    handleCompareNextCandidate: vi.fn(),
    handleComparePrevCandidate: vi.fn(),
    handleRatingChange: vi.fn(async () => {}),
    handleFlagChange: vi.fn(async () => {}),
    handleColorLabelChange: vi.fn(async () => {}),
    ...over,
  };
}

/** @param {ReturnType<typeof makeCtx>} ctx */
function setup(ctx) {
  const { handleGlobalKeydown, handleGlobalKeyup } = createKeyboardHandlers(ctx);
  /** @param {string} key @param {object} [mods] */
  const press = (key, mods = {}) => {
    const e = { key, target: {}, metaKey: false, ctrlKey: false, altKey: false, shiftKey: false, repeat: false, preventDefault: vi.fn(), ...mods };
    handleGlobalKeydown(/** @type {any} */ (e));
    return e;
  };
  /** @param {string} key */
  const release = (key) => handleGlobalKeyup(/** @type {any} */ ({ key }));
  return { press, release };
}

describe("typing targets", () => {
  it("ignores shortcuts while typing in a text input, textarea or select", () => {
    for (const target of [new FakeInput("text"), new FakeTextArea(), new FakeSelect()]) {
      const ctx = makeCtx({ selectedId: 1 });
      const { press } = setup(ctx);
      const e = press("3", { target });
      expect(ctx.handleRatingChange).not.toHaveBeenCalled();
      expect(e.preventDefault).not.toHaveBeenCalled();
    }
  });

  it("still handles keys when a range slider has focus", () => {
    const ctx = makeCtx({ selectedId: 1 });
    const { press } = setup(ctx);
    press("3", { target: new FakeInput("range") });
    expect(ctx.handleRatingChange).toHaveBeenCalledWith(1, 3);
  });
});

describe("library module", () => {
  it("0-5 set the rating on the anchor image", () => {
    const ctx = makeCtx({ selectedId: 5 });
    const { press } = setup(ctx);
    for (const n of [0, 1, 2, 3, 4, 5]) {
      const e = press(String(n));
      expect(ctx.handleRatingChange).toHaveBeenLastCalledWith(5, n);
      expect(e.preventDefault).toHaveBeenCalled();
    }
  });

  it("P and X toggle their flag on and off; U always clears", () => {
    const ctx = makeCtx({ selectedId: 5, selectedImage: { flag: "none" } });
    const { press } = setup(ctx);
    press("p");
    expect(ctx.handleFlagChange).toHaveBeenLastCalledWith(5, "pick");
    press("x");
    expect(ctx.handleFlagChange).toHaveBeenLastCalledWith(5, "reject");
    ctx.selectedImage = { flag: "pick" };
    press("p");
    expect(ctx.handleFlagChange).toHaveBeenLastCalledWith(5, "none");
    ctx.selectedImage = { flag: "reject" };
    press("x");
    expect(ctx.handleFlagChange).toHaveBeenLastCalledWith(5, "none");
    press("u");
    expect(ctx.handleFlagChange).toHaveBeenLastCalledWith(5, "none");
  });

  it("6-9 toggle red/yellow/green/blue", () => {
    const ctx = makeCtx({ selectedId: 5, selectedImage: { color_label: "none" } });
    const { press } = setup(ctx);
    for (const [key, label] of [["6", "red"], ["7", "yellow"], ["8", "green"], ["9", "blue"]]) {
      ctx.selectedImage = { color_label: "none" };
      press(key);
      expect(ctx.handleColorLabelChange).toHaveBeenLastCalledWith(5, label);
      ctx.selectedImage = { color_label: label };
      press(key);
      expect(ctx.handleColorLabelChange).toHaveBeenLastCalledWith(5, "none");
    }
  });

  it("respects user-remapped shortcuts", () => {
    const ctx = makeCtx({ selectedId: 5, shortcuts: { ...DEFAULT_SHORTCUTS, rate3: "q" } });
    const { press } = setup(ctx);
    press("q");
    expect(ctx.handleRatingChange).toHaveBeenCalledWith(5, 3);
    ctx.handleRatingChange.mockClear();
    press("3"); // the old binding no longer fires
    expect(ctx.handleRatingChange).not.toHaveBeenCalled();
  });

  it("ignores letter shortcuts when a modifier is held", () => {
    const ctx = makeCtx({ selectedId: 5 });
    const { press } = setup(ctx);
    press("p", { metaKey: true });
    press("3", { ctrlKey: true });
    expect(ctx.handleFlagChange).not.toHaveBeenCalled();
    expect(ctx.handleRatingChange).not.toHaveBeenCalled();
  });

  it("Cmd/Ctrl+A selects all; Cmd/Ctrl+D and Escape deselect", () => {
    const ctx = makeCtx();
    const { press } = setup(ctx);
    press("a", { metaKey: true });
    expect(ctx.handleSelectAll).toHaveBeenCalledTimes(1);
    press("d", { ctrlKey: true });
    press("Escape");
    expect(ctx.handleDeselectAll).toHaveBeenCalledTimes(2);
  });

  it("Delete/Backspace ask for removal confirmation only when something is selected", () => {
    const empty = makeCtx();
    const a = setup(empty);
    const e = a.press("Delete");
    expect(empty.confirmingRemoval).toBe(false);
    expect(e.preventDefault).not.toHaveBeenCalled();

    const some = makeCtx({ selectedIds: new Set([1, 2]) });
    setup(some).press("Backspace");
    expect(some.confirmingRemoval).toBe(true);
  });

  it("does nothing while a modal is open", () => {
    for (const flag of ["settingsOpen", "backupPromptOpen", "confirmingRemoval", "creatingCollection", "creatingSmartCollection", "creatingCollectionWithImages"]) {
      const ctx = makeCtx({ selectedId: 5, [flag]: true });
      setup(ctx).press("3");
      expect(ctx.handleRatingChange).not.toHaveBeenCalled();
    }
    const ctx = makeCtx({ selectedId: 5, exportItems: [{ path: "/x.jpg", version_id: 1 }] });
    setup(ctx).press("3");
    expect(ctx.handleRatingChange).not.toHaveBeenCalled();
  });

  it("arrow keys step the selection; compare view steps its candidate instead", () => {
    const ctx = makeCtx();
    const { press } = setup(ctx);
    press("ArrowRight");
    expect(ctx.selectNextImage).toHaveBeenLastCalledWith(false);
    press("ArrowLeft", { shiftKey: true });
    expect(ctx.selectPrevImage).toHaveBeenLastCalledWith(true);

    ctx.libraryViewMode = "compare";
    press("ArrowRight");
    press("ArrowLeft");
    expect(ctx.handleCompareNextCandidate).toHaveBeenCalledTimes(1);
    expect(ctx.handleComparePrevCandidate).toHaveBeenCalledTimes(1);
  });

  it("Up/Down move a grid row (4 cells) in grid view and one photo otherwise", () => {
    const ctx = makeCtx();
    const { press } = setup(ctx);
    press("ArrowDown");
    expect(ctx.selectGridStep).toHaveBeenLastCalledWith(4, false);
    press("ArrowUp", { shiftKey: true });
    expect(ctx.selectGridStep).toHaveBeenLastCalledWith(-4, true);
    ctx.libraryViewMode = "loupe";
    press("ArrowDown");
    press("ArrowUp");
    expect(ctx.selectNextImage).toHaveBeenCalledTimes(1);
    expect(ctx.selectPrevImage).toHaveBeenCalledTimes(1);
  });

  it("view hotkeys switch modes; Loupe with no selection selects the first photo", () => {
    const ctx = makeCtx();
    const { press } = setup(ctx);
    press("c");
    expect(ctx.libraryViewMode).toBe("compare");
    press("n");
    expect(ctx.libraryViewMode).toBe("survey");
    press("g");
    expect(ctx.libraryViewMode).toBe("grid");

    press("e"); // nothing selected -> first filtered image
    expect(ctx.selectedId).toBe(11);
    expect([...ctx.selectedIds]).toEqual([11]);
    expect(ctx.libraryViewMode).toBe("loupe");
  });

  it("M opens the map through showMapView, and a rebound key follows the binding", () => {
    const ctx = makeCtx();
    const { press } = setup(ctx);
    const e = press("m");
    expect(ctx.showMapView).toHaveBeenCalledTimes(1);
    expect(e.preventDefault).toHaveBeenCalled();
    press("M"); // caps lock / shifted letter is treated the same as the other view keys
    expect(ctx.showMapView).toHaveBeenCalledTimes(2);

    const rebound = makeCtx({ shortcuts: { ...DEFAULT_SHORTCUTS, viewMap: "k" } });
    const r = setup(rebound);
    r.press("m");
    expect(rebound.showMapView).not.toHaveBeenCalled();
    r.press("k");
    expect(rebound.showMapView).toHaveBeenCalledTimes(1);
  });

  it("Loupe on an empty view does nothing; Enter behaves like E", () => {
    const empty = makeCtx({ filteredImages: [] });
    setup(empty).press("Enter");
    expect(empty.libraryViewMode).toBe("grid");
    expect(empty.selectedId).toBeNull();

    const some = makeCtx({ selectedId: 12 });
    setup(some).press("Enter");
    expect(some.libraryViewMode).toBe("loupe");
    expect(some.selectedId).toBe(12);
  });

  it("D opens Develop on the selection, else the first photo, else nothing", () => {
    const withSel = makeCtx({ selectedId: 12 });
    setup(withSel).press("d");
    expect(withSel.openDevelop).toHaveBeenCalledWith(12);
    const noSel = makeCtx();
    setup(noSel).press("d");
    expect(noSel.openDevelop).toHaveBeenCalledWith(11);
    const empty = makeCtx({ filteredImages: [] });
    setup(empty).press("d");
    expect(empty.openDevelop).not.toHaveBeenCalled();
  });

  it("Space toggles grid <-> loupe, but only enters loupe with a selection", () => {
    const ctx = makeCtx();
    const { press } = setup(ctx);
    press(" ");
    expect(ctx.libraryViewMode).toBe("grid");
    ctx.selectedId = 11;
    press(" ");
    expect(ctx.libraryViewMode).toBe("loupe");
    press(" ");
    expect(ctx.libraryViewMode).toBe("grid");
  });
});

describe("develop module", () => {
  const dev = (/** @type {object} */ o = {}) => makeCtx({ activeModule: "develop", ...o });

  it("Cmd/Ctrl+Z undoes, +Shift redoes, Ctrl+Y redoes", () => {
    const ctx = dev();
    const { press } = setup(ctx);
    press("z", { metaKey: true });
    expect(ctx.handleUndo).toHaveBeenCalledTimes(1);
    press("z", { ctrlKey: true, shiftKey: true });
    press("y", { ctrlKey: true });
    expect(ctx.handleRedo).toHaveBeenCalledTimes(2);
  });

  it("arrows move to the previous/next photo without extending the selection", () => {
    const ctx = dev();
    const { press } = setup(ctx);
    press("ArrowRight");
    expect(ctx.selectNextImage).toHaveBeenLastCalledWith(false);
    press("ArrowLeft");
    expect(ctx.selectPrevImage).toHaveBeenLastCalledWith(false);
  });

  it("G and E return to Library in Grid / Loupe", () => {
    const ctx = dev();
    const { press } = setup(ctx);
    press("g");
    expect(ctx.switchModule).toHaveBeenLastCalledWith("library");
    expect(ctx.libraryViewMode).toBe("grid");
    press("e");
    expect(ctx.libraryViewMode).toBe("loupe");
  });

  it("Space starts panning (not on auto-repeat) and keyup releases it", () => {
    const ctx = dev();
    const { press, release } = setup(ctx);
    press(" ", { repeat: true });
    expect(ctx.spacePanning).toBe(false);
    press(" ");
    expect(ctx.spacePanning).toBe(true);
    release("x");
    expect(ctx.spacePanning).toBe(true);
    release(" ");
    expect(ctx.spacePanning).toBe(false);
  });

  it("backslash toggles before/after; H toggles mask pins", () => {
    const ctx = dev();
    const { press } = setup(ctx);
    press("\\");
    expect(ctx.showOriginal).toBe(true);
    press("\\");
    expect(ctx.showOriginal).toBe(false);
    press("h");
    expect(ctx.maskOverlaysVisible).toBe(false);
  });

  it("O toggles the mask overlay only for overlay-capable mask ops", () => {
    const capable = dev({ selectedMask: { op: OVERLAY_CAPABLE_MASK_OPS[0] } });
    setup(capable).press("o");
    expect(capable.showMaskOverlay).toBe(true);

    const other = dev({ selectedMask: { op: "linear_gradient_mask" } });
    setup(other).press("o");
    expect(other.showMaskOverlay).toBe(false);

    const none = dev();
    setup(none).press("o");
    expect(none.showMaskOverlay).toBe(false);
  });

  it("does nothing while a Develop dialog is open", () => {
    for (const over of [{ creatingSnapshot: true }, { creatingPreset: true }, { confirmingDeletePresetId: 3 }, { settingsOpen: true }, { backupPromptOpen: true }]) {
      const ctx = dev(over);
      setup(ctx).press("z", { metaKey: true });
      expect(ctx.handleUndo).not.toHaveBeenCalled();
    }
  });

  it("does not run Library-only shortcuts (rating keys are inert in Develop)", () => {
    const ctx = dev({ selectedId: 5 });
    setup(ctx).press("3");
    expect(ctx.handleRatingChange).not.toHaveBeenCalled();
  });
});

describe("other modules", () => {
  it("Print has no global shortcuts", () => {
    const ctx = makeCtx({ activeModule: "print", selectedId: 5 });
    const { press } = setup(ctx);
    press("3");
    press("a", { metaKey: true });
    expect(ctx.handleRatingChange).not.toHaveBeenCalled();
    expect(ctx.handleSelectAll).not.toHaveBeenCalled();
  });
});
