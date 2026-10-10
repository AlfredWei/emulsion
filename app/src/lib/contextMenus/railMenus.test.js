import { describe, it, expect, vi } from "vitest";
import { buildCollectionMenu, buildPersonMenu, buildSnapshotMenu, buildPresetMenu, buildPresetListMenu } from "./railMenus.js";
import { isItem } from "./model.js";

const items = (/** @type {import('./model.js').MenuEntry[]} */ entries) => entries.filter(isItem);
const ids = (/** @type {import('./model.js').MenuEntry[]} */ entries) => items(entries).map((e) => e.id);
const find = (/** @type {import('./model.js').MenuEntry[]} */ entries, /** @type {string} */ id) => items(entries).find((e) => e.id === id);

describe("collection menu", () => {
  const cmd = () => ({ open: vi.fn(), selectPhotos: vi.fn(), remove: vi.fn() });

  it("offers Show, Select and a confirmed Delete, in that order", () => {
    const menu = buildCollectionMenu({ name: "Trip", isSmart: false, count: 3 }, cmd());
    expect(ids(menu)).toEqual(["open", "select-photos", "delete-collection"]);
    const del = find(menu, "delete-collection");
    expect(del?.danger).toBe(true);
    expect(del?.confirm?.message).toContain("“Trip”");
    expect(del?.confirm?.message).toContain("photos stay in the catalog");
  });

  it("an empty collection keeps Select but disables it with the reason", () => {
    const sel = find(buildCollectionMenu({ name: "Trip", isSmart: false, count: 0 }, cmd()), "select-photos");
    expect(sel?.disabled).toBe(true);
    expect(sel?.reason).toBeTruthy();
  });

  it("a smart collection says so in the delete wording and the commands are wired", () => {
    const c = cmd();
    const menu = buildCollectionMenu({ name: "5 stars", isSmart: true, count: 2 }, c);
    expect(find(menu, "delete-collection")?.label).toBe("Delete Smart Collection…");
    find(menu, "open")?.run?.();
    find(menu, "select-photos")?.run?.();
    find(menu, "delete-collection")?.run?.();
    expect([c.open, c.selectPhotos, c.remove].map((f) => f.mock.calls.length)).toEqual([1, 1, 1]);
  });
});

describe("person menu", () => {
  it("names an unnamed person with the call to action and wires both commands", () => {
    const c = { showPhotos: vi.fn(), rename: vi.fn() };
    const menu = buildPersonMenu({ name: null, photoCount: 4 }, c);
    expect(find(menu, "rename-person")?.label).toBe("Name This Person…");
    find(menu, "show-photos")?.run?.();
    find(menu, "rename-person")?.run?.();
    expect(c.showPhotos).toHaveBeenCalledTimes(1);
    expect(c.rename).toHaveBeenCalledTimes(1);
  });

  it("a named person is renamed, and one with no photos cannot be shown", () => {
    const menu = buildPersonMenu({ name: "Ann", photoCount: 0 }, { showPhotos: vi.fn(), rename: vi.fn() });
    expect(find(menu, "rename-person")?.label).toBe("Rename…");
    expect(find(menu, "show-photos")?.disabled).toBe(true);
  });
});

describe("snapshot menu", () => {
  it("restores directly and confirms before deleting, naming the snapshot", () => {
    const c = { restore: vi.fn(), remove: vi.fn() };
    const menu = buildSnapshotMenu({ name: "Before crop" }, c);
    expect(ids(menu)).toEqual(["restore-snapshot", "delete-snapshot"]);
    expect(find(menu, "restore-snapshot")?.confirm).toBeUndefined();
    const del = find(menu, "delete-snapshot");
    expect(del?.danger).toBe(true);
    expect(del?.confirm?.message).toContain("“Before crop”");
    find(menu, "restore-snapshot")?.run?.();
    expect(c.restore).toHaveBeenCalledTimes(1);
  });
});

describe("preset menus", () => {
  it("a preset applies, exports and hands deletion to the existing confirmation (no second ask)", () => {
    const c = { apply: vi.fn(), exportPreset: vi.fn(), remove: vi.fn() };
    const menu = buildPresetMenu({ name: "Warm" }, c);
    expect(ids(menu)).toEqual(["apply-preset", "export-preset", "delete-preset"]);
    const del = find(menu, "delete-preset");
    expect(del?.danger).toBe(true);
    expect(del?.confirm).toBeUndefined();
    del?.run?.();
    expect(c.remove).toHaveBeenCalledTimes(1);
  });

  it("the list's own menu saves the current look or imports", () => {
    const c = { saveCurrent: vi.fn(), importPresets: vi.fn() };
    const menu = buildPresetListMenu(c);
    expect(ids(menu)).toEqual(["save-preset", "import-preset"]);
    find(menu, "save-preset")?.run?.();
    find(menu, "import-preset")?.run?.();
    expect([c.saveCurrent, c.importPresets].map((f) => f.mock.calls.length)).toEqual([1, 1]);
  });
});
