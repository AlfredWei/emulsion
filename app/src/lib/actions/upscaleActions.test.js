import { describe, it, expect, beforeEach, vi } from "vitest";

const getUpscaleLinks = vi.fn();
vi.mock("$lib/api/catalog.js", () => ({ getUpscaleLinks: (/** @type {number} */ id) => getUpscaleLinks(id) }));
const switchModule = vi.fn();
vi.mock("$lib/actions/navigation.js", () => ({ switchModule: (/** @type {string} */ m) => switchModule(m) }));
vi.mock("$lib/actions/libraryActions.js", () => ({
  prioritizeThumbnail: () => {},
  handleResetFilters: () => {
    library.searchQuery = "";
    library.enlargedFilter = false;
    library.minRating = 0;
  },
}));

import { revealPhotos, goToOriginal, showEnlargements } from "./upscaleActions.js";
import { library } from "$lib/state/library.svelte.js";
import { selection } from "$lib/state/selection.svelte.js";
import { shell } from "$lib/state/shell.svelte.js";

const img = (/** @type {number} */ id, /** @type {object} */ extra = {}) =>
  /** @type {any} */ ({ image_id: id, version_id: id * 10, path: `/p/${id}.jpg`, rating: 0, flag: "none", color_label: "none", thumbnail_path: "t", ...extra });

beforeEach(() => {
  getUpscaleLinks.mockReset();
  switchModule.mockClear();
  library.images = [img(1), img(2, { enlarged_scale: 4, enlarged_from: 1 }), img(3, { enlarged_scale: 4, enlarged_from: null }), img(4, { enlarged_scale: 2, enlarged_from: 1 })];
  library.activeCollectionId = null;
  library.activeFolderKey = null;
  library.showLastImportOnly = false;
  library.activePersonId = null;
  library.activeMapImageIds = null;
  library.searchQuery = "";
  library.enlargedFilter = false;
  library.minRating = 0;
  library.libraryViewMode = "grid";
  selection.selectedId = null;
  selection.selectedIds = new Set();
  shell.activeModule = "library";
  shell.notify("");
});

describe("revealPhotos", () => {
  it("selects the photos, the first as the active one", async () => {
    expect(await revealPhotos([4, 2])).toBe(true);
    expect([...selection.selectedIds].sort((a, b) => a - b)).toEqual([20, 40]);
    expect(selection.selectedId).toBe(20); // library order, not argument order
  });

  it("leaves another module for the Library", async () => {
    shell.activeModule = "develop";
    await revealPhotos([1]);
    expect(switchModule).toHaveBeenCalledWith("library");
  });

  it("clears the scope and the filters when they would hide the photo, and only then", async () => {
    library.searchQuery = "nothing matches this";
    library.activeFolderKey = "/elsewhere";
    await revealPhotos([2]);
    expect(library.activeFolderKey).toBeNull();
    expect(library.searchQuery).toBe("");
    library.searchQuery = "";
    library.minRating = 0;
    library.activeFolderKey = null;
    // visible already: a narrowing that still shows the photo is left alone
    library.images = [img(1), img(2, { rating: 3 })];
    library.minRating = 3;
    await revealPhotos([2]);
    expect(library.minRating).toBe(3);
  });

  it("leaves the loupe or survey for the grid and reports a miss", async () => {
    library.libraryViewMode = "loupe";
    await revealPhotos([1]);
    expect(library.libraryViewMode).toBe("grid");
    expect(await revealPhotos([999])).toBe(false);
  });
});

describe("goToOriginal", () => {
  it("selects the original of an enlargement", async () => {
    await goToOriginal(2);
    expect(selection.selectedId).toBe(10);
  });

  it("says so when the original was removed, and selects nothing", async () => {
    await goToOriginal(3);
    expect(selection.selectedId).toBeNull();
    expect(shell.statusMessage).toMatch(/original was removed/i);
  });

  it("ignores a photo that is not an enlargement", async () => {
    await goToOriginal(1);
    expect(selection.selectedId).toBeNull();
  });
});

describe("showEnlargements", () => {
  it("selects every enlargement of the photo", async () => {
    getUpscaleLinks.mockResolvedValue({ origin: null, enlargements: [{ result_image_id: 4 }, { result_image_id: 2 }] });
    await showEnlargements(1);
    expect(getUpscaleLinks).toHaveBeenCalledWith(1);
    expect([...selection.selectedIds].sort((a, b) => a - b)).toEqual([20, 40]);
  });

  it("does nothing when there are none, and survives a failing read", async () => {
    getUpscaleLinks.mockResolvedValueOnce({ origin: null, enlargements: [] });
    await showEnlargements(1);
    expect(selection.selectedIds.size).toBe(0);
    getUpscaleLinks.mockRejectedValueOnce("boom");
    await expect(showEnlargements(1)).resolves.toBeUndefined();
    expect(selection.selectedIds.size).toBe(0);
  });
});
