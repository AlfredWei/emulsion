import { describe, it, expect, vi, beforeEach } from "vitest";

const catalog = vi.hoisted(() => ({
  addImagesToCollection: vi.fn(),
  createCollectionWithImages: vi.fn(),
  removeImagesFromCollection: vi.fn(),
  listCollections: vi.fn(),
  listCollectionImageIds: vi.fn(),
  listImages: vi.fn(),
  ensureThumbnail: vi.fn(),
  createCollection: vi.fn(),
  createSmartCollection: vi.fn(),
  deleteCollection: vi.fn(),
}));
vi.mock("$lib/api/catalog.js", () => catalog);
vi.mock("$lib/api/faces.js", () => ({ getImagesForPerson: vi.fn() }));

import { handleAddToCollectionSelect, handleCreateCollectionWithImages, handleRemoveFromCollection } from "./collectionsActions.js";
import { library } from "$lib/state/library.svelte.js";
import { selection } from "$lib/state/selection.svelte.js";
import { shell } from "$lib/state/shell.svelte.js";

// Two versions (virtual copies) of image 1, to check ids are de-duplicated per image.
const images = /** @type {any[]} */ ([
  { image_id: 1, version_id: 10, path: "/p/a/b/1.jpg" },
  { image_id: 1, version_id: 11, path: "/p/a/b/1.jpg" },
  { image_id: 2, version_id: 20, path: "/p/a/b/2.jpg" },
  { image_id: 3, version_id: 30, path: "/p/a/b/3.jpg" },
]);

beforeEach(() => {
  vi.clearAllMocks();
  for (const fn of Object.values(catalog)) fn.mockResolvedValue(undefined);
  catalog.listCollections.mockResolvedValue([]);
  catalog.listCollectionImageIds.mockResolvedValue([]);
  library.images = images;
  library.collections = [];
  library.manualMembership = new Map();
  library.activeCollectionId = null;
  library.creatingCollectionWithImages = false;
  library.pendingAddToCollectionImageIds = [];
  selection.selectedId = null;
  selection.selectedIds = new Set();
  shell.notify("");
});

const select = (/** @type {number} */ anchor, /** @type {number[]} */ all) => {
  selection.selectedId = anchor;
  selection.selectedIds = new Set(all);
};

describe("handleAddToCollectionSelect", () => {
  it("ignores an empty pick and an empty selection", async () => {
    select(10, [10]);
    await handleAddToCollectionSelect("");
    selection.selectedIds = new Set();
    await handleAddToCollectionSelect("5");
    expect(catalog.addImagesToCollection).not.toHaveBeenCalled();
    expect(library.creatingCollectionWithImages).toBe(false);
  });

  it('"New collection…" stashes the de-duplicated image ids and opens the dialog without any IPC', async () => {
    select(10, [10, 11, 20]);
    await handleAddToCollectionSelect("__new__");
    expect(library.pendingAddToCollectionImageIds).toEqual([1, 2]);
    expect(library.creatingCollectionWithImages).toBe(true);
    expect(catalog.addImagesToCollection).not.toHaveBeenCalled();
  });

  it("adds to the chosen collection, reloads only a cached membership, refreshes the list and notifies", async () => {
    select(10, [10, 11, 20]);
    library.manualMembership = new Map([[5, new Set([9])]]);
    catalog.listCollectionImageIds.mockResolvedValue([9, 1, 2]);
    catalog.listCollections.mockResolvedValue([{ id: 5 }]);
    await handleAddToCollectionSelect("5");
    expect(catalog.addImagesToCollection).toHaveBeenCalledWith(5, [1, 2]);
    expect(library.manualMembership.get(5)).toEqual(new Set([9, 1, 2]));
    expect(library.collections).toHaveLength(1);
    expect(shell.statusMessage).toBe("Added 2 photos to collection");
  });

  it("invalidates the mutated collection, not the active one; skips the reload when it was never fetched", async () => {
    select(30, [30]);
    library.activeCollectionId = 7;
    library.manualMembership = new Map([[7, new Set()]]);
    await handleAddToCollectionSelect("5"); // 5 has no cached membership
    expect(catalog.listCollectionImageIds).not.toHaveBeenCalled();
    expect(shell.statusMessage).toBe("Added 1 photo to collection");
  });
});

describe("handleCreateCollectionWithImages", () => {
  it("closes the dialog, clears the pending ids, creates the collection with them and notifies", async () => {
    library.creatingCollectionWithImages = true;
    library.pendingAddToCollectionImageIds = [1, 2, 3];
    catalog.listCollections.mockResolvedValue([{ id: 8 }]);
    await handleCreateCollectionWithImages("Trip");
    expect(library.creatingCollectionWithImages).toBe(false);
    expect(library.pendingAddToCollectionImageIds).toEqual([]);
    expect(catalog.createCollectionWithImages).toHaveBeenCalledWith("Trip", [1, 2, 3]);
    expect(library.collections).toHaveLength(1);
    expect(shell.statusMessage).toBe('Added 3 photos to "Trip"');
  });

  it("uses the singular for one photo", async () => {
    library.pendingAddToCollectionImageIds = [1];
    await handleCreateCollectionWithImages("Solo");
    expect(shell.statusMessage).toBe('Added 1 photo to "Solo"');
  });
});

describe("handleRemoveFromCollection", () => {
  it("does nothing without an active collection or a selection", async () => {
    select(10, [10]);
    await handleRemoveFromCollection();
    library.activeCollectionId = 5;
    selection.selectedIds = new Set();
    await handleRemoveFromCollection();
    expect(catalog.removeImagesFromCollection).not.toHaveBeenCalled();
  });

  it("removes the selection from the active collection, reloads its membership, and drops the removed photos from the selection", async () => {
    library.activeCollectionId = 5;
    select(10, [10, 11, 30]);
    catalog.listCollectionImageIds.mockResolvedValue([2]);
    await handleRemoveFromCollection();
    expect(catalog.removeImagesFromCollection).toHaveBeenCalledWith(5, [1, 3]);
    expect(library.manualMembership.get(5)).toEqual(new Set([2]));
    expect(selection.selectedIds).toEqual(new Set());
    expect(selection.selectedId).toBeNull();
    expect(shell.statusMessage).toBe("Removed 2 photos from collection");
  });

  it("keeps an anchor that was not among the removed photos", async () => {
    library.activeCollectionId = 5;
    select(30, [20]); // anchor 30 is not part of the selection being removed
    await handleRemoveFromCollection();
    expect(selection.selectedId).toBe(30);
    expect(selection.selectedIds).toEqual(new Set());
  });
});
