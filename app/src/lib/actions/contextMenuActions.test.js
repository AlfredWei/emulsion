import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ convertFileSrc: (/** @type {string} */ p) => p }));
vi.mock("$lib/actions/navigation.js", () => ({ openDevelop: vi.fn(), switchModule: vi.fn(async () => {}) }));
vi.mock("$lib/actions/faceActions.js", () => ({ handleDetectFacesForSelected: vi.fn(), handleDetectFacesForSelection: vi.fn() }));
vi.mock("$lib/actions/importActions.js", () => ({ handleImportFolder: vi.fn(), handleImportFiles: vi.fn() }));
vi.mock("$lib/actions/libraryActions.js", () => ({
  prioritizeThumbnail: vi.fn(),
  selectCollection: vi.fn(async () => {}),
  selectPerson: vi.fn(async () => {}),
  handleDeleteCollection: vi.fn(async () => {}),
}));
vi.mock("$lib/actions/historyActions.js", () => ({ handleRestoreSnapshot: vi.fn() }));
vi.mock("$lib/actions/developActions.js", () => ({ handleDeleteSnapshot: vi.fn() }));
vi.mock("$lib/actions/selectionActions.js", () => ({ handleSelectAll: vi.fn(), handleDeselectAll: vi.fn() }));
vi.mock("$lib/actions/metadataActions.js", () => ({ handleRatingChange: vi.fn(), handleFlagChange: vi.fn(), handleColorLabelChange: vi.fn() }));
vi.mock("$lib/actions/collectionsActions.js", () => ({ handleAddToCollectionSelect: vi.fn(), handleRemoveFromCollection: vi.fn() }));
vi.mock("$lib/actions/presetActions.js", () => ({
  handleCopySettingsFromPhoto: vi.fn(),
  handlePasteSettings: vi.fn(),
  handlePasteSettingsToSelection: vi.fn(),
  handleApplyPresetToSelection: vi.fn(),
  handleResetSettingsForSelection: vi.fn(),
  handleApplyPreset: vi.fn(),
  handleExportPreset: vi.fn(),
  handleDeletePresetRequest: vi.fn(),
  handleSaveCurrentAsPresetRequest: vi.fn(),
  handleImportPresetRequest: vi.fn(),
}));
vi.mock("$lib/api/system.js", () => ({ revealInFileManager: vi.fn() }));

import * as A from "./contextMenuActions.js";
import { openDevelop } from "$lib/actions/navigation.js";
import { handleRatingChange } from "$lib/actions/metadataActions.js";
import { handlePasteSettings, handlePasteSettingsToSelection, handleApplyPreset, handleExportPreset, handleDeletePresetRequest, handleSaveCurrentAsPresetRequest } from "$lib/actions/presetActions.js";
import { selectCollection, selectPerson, handleDeleteCollection } from "$lib/actions/libraryActions.js";
import { handleRestoreSnapshot } from "$lib/actions/historyActions.js";
import { handleDeleteSnapshot } from "$lib/actions/developActions.js";
import { handleSelectAll } from "$lib/actions/selectionActions.js";
import { faces } from "$lib/state/faces.svelte.js";
import { presets } from "$lib/state/presets.svelte.js";
import { library } from "$lib/state/library.svelte.js";
import { selection } from "$lib/state/selection.svelte.js";
import { shell } from "$lib/state/shell.svelte.js";
import { develop } from "$lib/state/develop.svelte.js";
import { exportFlow } from "$lib/state/exportFlow.svelte.js";
import { contextMenu } from "$lib/state/contextMenu.svelte.js";

const img = (/** @type {number} */ id) =>
  /** @type {any} */ ({ image_id: id, version_id: id * 10, path: `/p/${id}.jpg`, rating: 0, flag: "none", color_label: "none", faces_scanned: false });
const sel = (/** @type {number[]} */ all, /** @type {number} */ anchor = all[0]) => {
  selection.selectedId = anchor;
  selection.selectedIds = new Set(all);
};
const ids = () => [...selection.selectedIds].sort((a, b) => a - b);

// Just enough DOM for the entry points: elements that answer closest() and carry a dataset.
class FakeElement {}
class FakeHTMLElement extends FakeElement {
  /** @param {{ photo?: number, grid?: boolean, text?: boolean, collection?: number, person?: number, snapshot?: number, preset?: number, presets?: boolean }} kind */
  constructor(kind = {}) {
    super();
    this.kind = kind;
    /** @type {Record<string, string>} */
    this.dataset = {};
    if (kind.photo !== undefined) this.dataset.ctxPhoto = String(kind.photo);
    if (kind.collection !== undefined) this.dataset.ctxCollection = String(kind.collection);
    if (kind.person !== undefined) this.dataset.ctxPerson = String(kind.person);
    if (kind.snapshot !== undefined) this.dataset.ctxSnapshot = String(kind.snapshot);
    if (kind.preset !== undefined) this.dataset.ctxPreset = String(kind.preset);
    if (kind.presets) this.dataset.ctxPresets = "";
    if (kind.grid) this.dataset.ctxGrid = "";
    this.focus = vi.fn();
    this.getBoundingClientRect = () => ({ left: 100, bottom: 200 });
  }
  /** @param {string} sel */
  closest(sel) {
    // The real selector lists every surface attribute; the fake matches when it carries any of them.
    if (sel.includes("data-ctx-photo") && Object.keys(this.dataset).length > 0) return this;
    if (sel.includes("input")) return this.kind.text ? this : null;
    return null;
  }
}
const mouse = (/** @type {FakeHTMLElement} */ target) => /** @type {any} */ ({ target, clientX: 30, clientY: 40, preventDefault: vi.fn() });

beforeEach(() => {
  vi.clearAllMocks();
  vi.stubGlobal("Element", FakeElement);
  vi.stubGlobal("HTMLElement", FakeHTMLElement);
  vi.stubGlobal("document", { activeElement: null });
  library.images = [1, 2, 3, 4].map(img);
  library.collections = [];
  library.activeCollectionId = null;
  library.confirmingRemoval = false;
  library.libraryViewMode = "grid";
  selection.selectedId = null;
  selection.selectedIds = new Set();
  shell.activeModule = "library";
  shell.shortcuts = {};
  develop.versionId = null;
  develop.copiedSettings = null;
  contextMenu.close();
  contextMenu.cancelConfirm();
  exportFlow.items = null;
});
afterEach(() => vi.unstubAllGlobals());

describe("the target rule", () => {
  it("a photo inside the selection acts on the whole selection and leaves it alone", () => {
    sel([10, 20, 30], 10);
    const t = A.resolveTargets(20, "library");
    expect(t.map((i) => i.version_id)).toEqual([10, 20, 30]);
    expect(ids()).toEqual([10, 20, 30]);
    expect(selection.selectedId).toBe(10);
  });

  it("a photo outside the selection becomes the only selection first", () => {
    sel([10, 20], 10);
    const t = A.resolveTargets(40, "library");
    expect(t.map((i) => i.version_id)).toEqual([40]);
    expect(ids()).toEqual([40]);
    expect(selection.selectedId).toBe(40);
  });

  it("in Develop the filmstrip has no multi-selection, so even a photo in a stale Library selection acts alone", () => {
    sel([10, 20, 30], 10);
    expect(A.resolveTargets(20, "develop").map((i) => i.version_id)).toEqual([20]);
    expect(ids()).toEqual([20]);
  });
});

describe("right-click entry point", () => {
  it("leaves the native menu to text fields", () => {
    const e = mouse(new FakeHTMLElement({ text: true }));
    A.handleContextMenu(e);
    expect(e.preventDefault).not.toHaveBeenCalled();
    expect(contextMenu.isOpen).toBe(false);
  });

  it("opens the photo menu for a photo, naming a multi-selection", () => {
    sel([10, 20], 10);
    const e = mouse(new FakeHTMLElement({ photo: 20 }));
    A.handleContextMenu(e);
    expect(e.preventDefault).toHaveBeenCalled();
    expect(contextMenu.isOpen).toBe(true);
    expect([contextMenu.x, contextMenu.y]).toEqual([30, 40]);
    expect(contextMenu.items[0]).toEqual({ header: "2 photos" });
    expect(contextMenu.fromKeyboard).toBe(false);
  });

  it("opens the empty-area menu on the Library grid only", () => {
    A.handleContextMenu(mouse(new FakeHTMLElement({ grid: true })));
    expect(contextMenu.isOpen).toBe(true);
    expect(contextMenu.items.some((i) => "id" in i && i.id === "select-all")).toBe(true);
    contextMenu.close();
    shell.activeModule = "develop";
    A.handleContextMenu(mouse(new FakeHTMLElement({ grid: true })));
    expect(contextMenu.isOpen).toBe(false);
  });

  it("suppresses the browser menu on anything else and shows none; a second right-click closes the first", () => {
    A.handleContextMenu(mouse(new FakeHTMLElement({ photo: 10 })));
    expect(contextMenu.isOpen).toBe(true);
    const e = mouse(new FakeHTMLElement());
    A.handleContextMenu(e);
    expect(e.preventDefault).toHaveBeenCalled();
    expect(contextMenu.isOpen).toBe(false);
  });
});

describe("Shift+F10 / the Menu key", () => {
  const key = (/** @type {object} */ over) => /** @type {any} */ ({ key: "F10", shiftKey: true, target: new FakeHTMLElement(), preventDefault: vi.fn(), ...over });

  it("opens the focused photo's menu beside it, from the keyboard", () => {
    const photo = new FakeHTMLElement({ photo: 30 });
    /** @type {any} */ (document).activeElement = photo;
    const e = key({});
    expect(A.handleContextMenuKey(e)).toBe(true);
    expect(e.preventDefault).toHaveBeenCalled();
    expect(contextMenu.isOpen).toBe(true);
    expect(contextMenu.fromKeyboard).toBe(true);
    expect(contextMenu.returnFocusTo).toBe(photo);
    expect(ids()).toEqual([30]);
  });

  it("does nothing for other keys, a focus that is not a photo, or a text field", () => {
    /** @type {any} */ (document).activeElement = new FakeHTMLElement({ photo: 30 });
    expect(A.handleContextMenuKey(key({ key: "F9" }))).toBe(false);
    expect(A.handleContextMenuKey(key({ shiftKey: false }))).toBe(false);
    expect(A.handleContextMenuKey(key({ key: "ContextMenu", shiftKey: false, target: new FakeHTMLElement({ text: true }) }))).toBe(false);
    /** @type {any} */ (document).activeElement = new FakeHTMLElement();
    expect(A.handleContextMenuKey(key({}))).toBe(false);
    expect(contextMenu.isOpen).toBe(false);
  });
});

describe("the commands", () => {
  it("Rating acts on the resolved selection through the existing action", () => {
    const cmd = A.photoCommands([img(1)], "library");
    cmd.setRating(4);
    expect(handleRatingChange).toHaveBeenCalledWith(undefined, 4);
  });

  it("Paste on the open Develop photo uses Develop's own path; anything else the batch path", () => {
    develop.versionId = 10;
    A.photoCommands([img(1)], "develop").pasteSettings();
    expect(handlePasteSettings).toHaveBeenCalledTimes(1);
    A.photoCommands([img(2)], "develop").pasteSettings();
    A.photoCommands([img(1), img(2)], "library").pasteSettings();
    expect(handlePasteSettingsToSelection).toHaveBeenCalledTimes(2);
  });

  it("Export opens the dialog with exactly the menu's photos, not the open Develop photo", async () => {
    develop.versionId = 10;
    const flush = vi.spyOn(develop, "flushEditStack").mockResolvedValue(undefined);
    await A.photoCommands([img(2), img(3)], "develop").exportPhotos();
    expect(flush).toHaveBeenCalled();
    expect(exportFlow.items).toEqual([
      { path: "/p/2.jpg", version_id: 20 },
      { path: "/p/3.jpg", version_id: 30 },
    ]);
  });

  it("Remove from Catalog hands over to the existing confirmation; Open in Develop opens that photo", () => {
    const cmd = A.photoCommands([img(1)], "library");
    cmd.removeFromCatalog();
    expect(library.confirmingRemoval).toBe(true);
    cmd.openInDevelop(30);
    expect(openDevelop).toHaveBeenCalledWith(30);
  });

  it("Open in Loupe selects the photo and switches the Library view", async () => {
    shell.activeModule = "library";
    await A.photoCommands([img(3)], "library").openInLoupe(30);
    expect(library.libraryViewMode).toBe("loupe");
    expect(ids()).toEqual([30]);
  });
});

describe("rail and Develop-rail surfaces", () => {
  beforeEach(() => {
    library.collections = /** @type {any} */ ([{ id: 5, name: "Trip", is_smart: false, count: 2, rules: [] }]);
    faces.people = /** @type {any} */ ([{ id: 7, name: "Ann", photo_count: 3 }]);
    faces.renamingPersonId = null;
    develop.snapshots = /** @type {any} */ ([{ id: 3, name: "Before crop", created_at: "" }]);
    presets.list = /** @type {any} */ ([{ id: 9, name: "Warm", edit_stack: { ops: [] } }]);
  });
  const run = (/** @type {string} */ id) => {
    const item = /** @type {any} */ (contextMenu.items.find((i) => "id" in i && i.id === id));
    return item.run();
  };

  it("a collection row opens its menu without touching the photo selection; Show and Select go through the existing actions", async () => {
    sel([10], 10);
    A.handleContextMenu(mouse(new FakeHTMLElement({ collection: 5 })));
    expect(contextMenu.isOpen).toBe(true);
    expect(ids()).toEqual([10]);
    await run("open");
    expect(selectCollection).toHaveBeenCalledWith(5);
    await run("select-photos");
    expect(handleSelectAll).toHaveBeenCalledTimes(1);
  });

  it("Delete on a collection asks first (the menu's confirm) and then deletes without a pointer event", () => {
    A.handleContextMenu(mouse(new FakeHTMLElement({ collection: 5 })));
    const del = /** @type {any} */ (contextMenu.items.find((i) => "id" in i && i.id === "delete-collection"));
    expect(del.confirm).toBeTruthy();
    del.run();
    expect(handleDeleteCollection).toHaveBeenCalledWith(5);
  });

  it("a person row: Show Photos selects the person, Rename asks the rail to start its inline editor", async () => {
    A.handleContextMenu(mouse(new FakeHTMLElement({ person: 7 })));
    await run("show-photos");
    expect(selectPerson).toHaveBeenCalledWith(7);
    await run("rename-person");
    expect(faces.renamingPersonId).toBe(7);
  });

  it("a snapshot row restores or deletes that snapshot", () => {
    A.handleContextMenu(mouse(new FakeHTMLElement({ snapshot: 3 })));
    run("restore-snapshot");
    expect(handleRestoreSnapshot).toHaveBeenCalledWith(3);
    A.handleContextMenu(mouse(new FakeHTMLElement({ snapshot: 3 })));
    run("delete-snapshot");
    expect(handleDeleteSnapshot).toHaveBeenCalledWith(3);
  });

  it("a preset row applies, exports, and requests deletion of that preset", () => {
    A.handleContextMenu(mouse(new FakeHTMLElement({ preset: 9 })));
    run("apply-preset");
    expect(handleApplyPreset).toHaveBeenCalledWith(9);
    A.handleContextMenu(mouse(new FakeHTMLElement({ preset: 9 })));
    run("export-preset");
    expect(handleExportPreset).toHaveBeenCalledWith(9);
    A.handleContextMenu(mouse(new FakeHTMLElement({ preset: 9 })));
    run("delete-preset");
    expect(handleDeletePresetRequest).toHaveBeenCalledWith(9);
  });

  it("the presets list's empty space has its own menu", () => {
    A.handleContextMenu(mouse(new FakeHTMLElement({ presets: true })));
    run("save-preset");
    expect(handleSaveCurrentAsPresetRequest).toHaveBeenCalledTimes(1);
  });

  it("a row whose data is gone (deleted since render) opens nothing", () => {
    A.handleContextMenu(mouse(new FakeHTMLElement({ preset: 99 })));
    expect(contextMenu.isOpen).toBe(false);
  });

  it("Shift+F10 on a focused row opens its menu and returns focus to the element that had it", () => {
    const button = new FakeHTMLElement({ snapshot: 3 });
    /** @type {any} */ (document).activeElement = button;
    expect(A.handleContextMenuKey(/** @type {any} */ ({ key: "F10", shiftKey: true, target: new FakeHTMLElement(), preventDefault: vi.fn() }))).toBe(true);
    expect(contextMenu.isOpen).toBe(true);
    expect(contextMenu.returnFocusTo).toBe(button);
  });
});
