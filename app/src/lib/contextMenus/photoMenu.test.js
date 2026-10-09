import { describe, it, expect, vi } from "vitest";
import { buildPhotoMenu } from "./photoMenu.js";
import { buildGridBackgroundMenu } from "./gridMenu.js";
import { isItem } from "./model.js";

const img = (/** @type {number} */ id, /** @type {object} */ extra = {}) =>
  /** @type {any} */ ({ image_id: id, version_id: id * 10, path: `/p/${id}.jpg`, rating: 0, flag: "none", color_label: "none", ...extra });

const COMMANDS = [
  "openInDevelop", "openInLoupe", "compare", "survey", "setRating", "setFlag", "setColorLabel", "addToCollection", "removeFromCollection",
  "copySettings", "pasteSettings", "applyPreset", "resetSettings", "exportPhotos", "reveal", "copyPath", "detectFaces", "removeFromCatalog",
];
function commands() {
  return /** @type {any} */ (Object.fromEntries(COMMANDS.map((c) => [c, vi.fn()])));
}
/** @param {Partial<import('./photoMenu.js').PhotoMenuInput>} over */
function input(over = {}) {
  return /** @type {import('./photoMenu.js').PhotoMenuInput} */ ({
    images: [img(1)],
    module: "library",
    openVersionId: null,
    viewedCollection: null,
    manualCollections: [],
    presets: [],
    hasCopiedSettings: false,
    shortcuts: { rate3: "3", flagPick: "p", viewDevelop: "d", viewLoupe: "e" },
    ...over,
  });
}
/** Finds an item by id at any depth. @param {import('./model.js').MenuEntry[]} entries @param {string} id @returns {import('./model.js').MenuItem | undefined} */
function find(entries, id) {
  for (const e of entries) {
    if (!isItem(e)) continue;
    if (e.id === id) return e;
    const inner = e.children ? find(e.children, id) : undefined;
    if (inner) return inner;
  }
  return undefined;
}
const ids = (/** @type {import('./model.js').MenuEntry[]} */ entries) => entries.filter(isItem).map((e) => e.id);

describe("photo menu: one photo, Library", () => {
  const menu = buildPhotoMenu(input(), commands());

  it("offers the common operations in a stable order, with no header", () => {
    expect(ids(menu)).toEqual(["open-develop", "open-loupe", "rating", "flag", "color-label", "add-to-collection", "develop-settings", "export", "reveal", "copy-path", "detect-faces", "remove-from-catalog"]);
    expect("header" in menu[0]).toBe(false);
  });

  it("shows the user's shortcuts, not hard-coded ones", () => {
    expect(find(menu, "open-develop")?.shortcut).toBe("D");
    expect(find(menu, "rating-3")?.shortcut).toBe("3");
    expect(find(menu, "flag-pick")?.shortcut).toBe("P");
    expect(find(menu, "rating-5")?.shortcut).toBeUndefined();
    const rebound = buildPhotoMenu(input({ shortcuts: { viewDevelop: "q" } }), commands());
    expect(find(rebound, "open-develop")?.shortcut).toBe("Q");
  });

  it("marks the current rating, flag and label", () => {
    const m = buildPhotoMenu(input({ images: [img(1, { rating: 4, flag: "pick", color_label: "red" })] }), commands());
    expect(find(m, "rating-4")?.checked).toBe(true);
    expect(find(m, "rating-5")?.checked).toBe(false);
    expect(find(m, "flag-pick")?.checked).toBe(true);
    expect(find(m, "label-red")?.checked).toBe(true);
    expect(find(m, "rating-0")?.checked).toBe(false);
  });

  it("runs the matching command with the right argument", () => {
    const cmd = commands();
    const m = buildPhotoMenu(input({ presets: [{ id: 7, name: "Punch" }], manualCollections: [{ id: 3, name: "Trip" }] }), cmd);
    find(m, "rating-4")?.run?.();
    find(m, "flag-reject")?.run?.();
    find(m, "label-blue")?.run?.();
    find(m, "collection-3")?.run?.();
    find(m, "collection-new")?.run?.();
    find(m, "preset-7")?.run?.();
    find(m, "copy-settings")?.run?.();
    find(m, "reveal")?.run?.();
    find(m, "open-develop")?.run?.();
    expect(cmd.setRating).toHaveBeenCalledWith(4);
    expect(cmd.setFlag).toHaveBeenCalledWith("reject");
    expect(cmd.setColorLabel).toHaveBeenCalledWith("blue");
    expect(cmd.addToCollection).toHaveBeenNthCalledWith(1, 3);
    expect(cmd.addToCollection).toHaveBeenNthCalledWith(2, "new");
    expect(cmd.applyPreset).toHaveBeenCalledWith(7);
    expect(cmd.copySettings).toHaveBeenCalledWith(10);
    expect(cmd.reveal).toHaveBeenCalledWith("/p/1.jpg");
    expect(cmd.openInDevelop).toHaveBeenCalledWith(10);
  });

  it("Paste Settings is disabled, with a reason, until something was copied", () => {
    expect(find(menu, "paste-settings")?.disabled).toBe(true);
    expect(find(menu, "paste-settings")?.reason).toMatch(/Copy settings/);
    const m = buildPhotoMenu(input({ hasCopiedSettings: true }), commands());
    expect(find(m, "paste-settings")?.disabled).toBe(false);
  });

  it("the two destructive items are marked; Reset asks first, Remove leaves asking to the existing dialog", () => {
    expect(find(menu, "reset-settings")?.danger).toBe(true);
    expect(find(menu, "reset-settings")?.confirm?.confirmLabel).toBe("Reset");
    expect(find(menu, "remove-from-catalog")?.danger).toBe(true);
    expect(find(menu, "remove-from-catalog")?.confirm).toBeUndefined();
  });

  it("has no Apply Preset submenu without presets, and no remove-from-collection outside a collection", () => {
    expect(find(menu, "apply-preset")).toBeUndefined();
    expect(find(menu, "remove-from-collection")).toBeUndefined();
    const m = buildPhotoMenu(input({ viewedCollection: { id: 3, name: "Trip" } }), commands());
    expect(find(m, "remove-from-collection")?.label).toBe("Remove from “Trip”");
  });
});

describe("photo menu: several photos", () => {
  const images = [img(1, { rating: 3 }), img(2, { rating: 3 }), img(3, { rating: 5 })];
  const menu = buildPhotoMenu(input({ images, hasCopiedSettings: true }), commands());

  it("names the target in a header and keeps the same shape (plus Compare / Survey)", () => {
    expect(menu[0]).toEqual({ header: "3 photos" });
    expect(ids(menu)).toContain("compare-survey");
    expect(ids(menu)).toEqual(expect.arrayContaining(["rating", "flag", "develop-settings", "export", "reveal", "copy-path", "remove-from-catalog"]));
  });

  it("disables what needs a single photo, with a reason, instead of hiding it", () => {
    for (const id of ["copy-settings", "reveal", "copy-path"]) {
      const it = find(menu, id);
      expect(it?.disabled).toBe(true);
      expect(it?.reason).toBe("Choose a single photo");
    }
  });

  it("checks a value only when every photo shares it", () => {
    const same = buildPhotoMenu(input({ images: [img(1, { rating: 3 }), img(2, { rating: 3 })] }), commands());
    expect(find(same, "rating-3")?.checked).toBe(true);
    expect(find(menu, "rating-3")?.checked).toBe(false);
    expect(find(menu, "rating-5")?.checked).toBe(false);
  });

  it("says how many photos Paste and Reset act on", () => {
    expect(find(menu, "paste-settings")?.label).toBe("Paste Settings to 3 photos");
    expect(find(menu, "reset-settings")?.label).toBe("Reset Settings (3 photos)…");
    expect(find(menu, "reset-settings")?.confirm?.message).toMatch(/3 photos/);
  });
});

describe("photo menu: Develop filmstrip", () => {
  it("omits Open in Develop on the photo that is already open, and says Open in Library", () => {
    const m = buildPhotoMenu(input({ module: "develop", openVersionId: 10 }), commands());
    expect(find(m, "open-develop")).toBeUndefined();
    expect(find(m, "open-loupe")?.label).toBe("Open in Library");
    const other = buildPhotoMenu(input({ module: "develop", openVersionId: 99 }), commands());
    expect(find(other, "open-develop")).toBeDefined();
  });

  it("never offers the collection removal or Compare / Survey there", () => {
    const m = buildPhotoMenu(input({ module: "develop", images: [img(1), img(2)], viewedCollection: { id: 1, name: "x" } }), commands());
    expect(find(m, "remove-from-collection")).toBeUndefined();
    expect(find(m, "compare-survey")).toBeUndefined();
  });
});

describe("photo menu: nothing under the pointer", () => {
  it("is empty for no photos", () => {
    expect(buildPhotoMenu(input({ images: [] }), commands())).toEqual([]);
  });
});

describe("grid background menu", () => {
  const cmd = { selectAll: vi.fn(), deselectAll: vi.fn(), importFolder: vi.fn(), importFiles: vi.fn() };

  it("offers selection and import, disabling what has nothing to act on", () => {
    const m = buildGridBackgroundMenu({ photoCount: 12, selectedCount: 0 }, cmd);
    expect(ids(m)).toEqual(["select-all", "deselect-all", "import-folder", "import-files"]);
    expect(find(m, "deselect-all")?.disabled).toBe(true);
    expect(find(m, "select-all")?.disabled).toBe(false);
    const empty = buildGridBackgroundMenu({ photoCount: 0, selectedCount: 0 }, cmd);
    expect(find(empty, "select-all")?.disabled).toBe(true);
  });
});
