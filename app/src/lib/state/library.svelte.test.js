import { describe, it, expect } from "vitest";
import { createLibraryStore } from "./library.svelte.js";

/** @param {Partial<import('$lib/api/catalog.js').ImageSummary> & { image_id: number }} o */
function img(o) {
  return /** @type {any} */ ({
    version_id: o.image_id * 10,
    path: `/photos/2026/trip/img${o.image_id}.jpg`,
    rating: 0,
    flag: "none",
    color_label: "none",
    camera_make: null,
    camera_model: null,
    lens_model: null,
    import_batch: null,
    ...o,
  });
}

const ids = (/** @type {any[]} */ list) => list.map((i) => i.image_id);

describe("LibraryStore", () => {
  it("starts on All Photos with no filters, grid view and nothing open", () => {
    const l = createLibraryStore();
    expect(l.images).toEqual([]);
    expect(l.libraryViewMode).toBe("grid");
    expect(l.libraryZoomLevel).toBe(1);
    expect([l.activeCollectionId, l.activeFolderKey, l.activePersonId, l.showLastImportOnly, l.activeMapImageIds]).toEqual([null, null, null, false, null]);
    expect([l.searchQuery, l.flagFilter, l.minRating, l.ratingOp]).toEqual(["", "all", 0, ">="]);
    expect([l.colorLabelFilter, l.fileTypeFilter, l.cameraFilter, l.lensFilter, l.dateFrom, l.dateTo]).toEqual(["all", "all", "all", "all", "", ""]);
    expect([l.confirmingRemoval, l.creatingCollection, l.creatingSmartCollection, l.creatingCollectionWithImages]).toEqual([false, false, false, false]);
    expect(l.pendingAddToCollectionImageIds).toEqual([]);
    expect(l.compareCandidateId).toBeNull();
    expect(l.filteredImages).toEqual([]);
    expect(l.lastImportBatchId).toBeNull();
  });

  it("each factory call gets its own state", () => {
    const a = createLibraryStore();
    const b = createLibraryStore();
    a.images = [img({ image_id: 1 })];
    a.searchQuery = "x";
    expect(b.images).toEqual([]);
    expect(b.searchQuery).toBe("");
  });

  describe("filteredImages", () => {
    it("is every image with no source or filter, and follows reassignment of images", () => {
      const l = createLibraryStore();
      l.images = [img({ image_id: 1 }), img({ image_id: 2 })];
      expect(ids(l.filteredImages)).toEqual([1, 2]);
      l.images = [...l.images, img({ image_id: 3 })];
      expect(ids(l.filteredImages)).toEqual([1, 2, 3]);
    });

    it("narrows by the filter-bar fields and widens again when they reset", () => {
      const l = createLibraryStore();
      l.images = [img({ image_id: 1, flag: "pick", rating: 5 }), img({ image_id: 2, rating: 2 }), img({ image_id: 3, flag: "reject" })];
      l.flagFilter = "pick";
      expect(ids(l.filteredImages)).toEqual([1]);
      l.flagFilter = "all";
      l.minRating = 2;
      expect(ids(l.filteredImages)).toEqual([1, 2]);
      l.ratingOp = "=";
      expect(ids(l.filteredImages)).toEqual([2]);
      l.minRating = 0;
      expect(ids(l.filteredImages)).toEqual([1, 2, 3]);
    });

    it("applies the active source before the filters", () => {
      const l = createLibraryStore();
      l.images = [
        img({ image_id: 1, path: "/p/2026/wedding/a.jpg", flag: "pick" }),
        img({ image_id: 2, path: "/p/2026/wedding/b.jpg" }),
        img({ image_id: 3, path: "/p/2026/trip/c.jpg", flag: "pick" }),
      ];
      l.activeFolderKey = "2026/wedding";
      expect(ids(l.baseImages)).toEqual([1, 2]);
      l.flagFilter = "pick";
      expect(ids(l.filteredImages)).toEqual([1]);
    });

    it("scopes a manual collection to its cached membership, and to nothing until it is fetched", () => {
      const l = createLibraryStore();
      l.images = [img({ image_id: 1 }), img({ image_id: 2 })];
      l.collections = [/** @type {any} */ ({ id: 7, name: "M", is_smart: false })];
      l.activeCollectionId = 7;
      expect(l.filteredImages).toEqual([]); // membership not fetched yet
      l.manualMembership = new Map([[7, new Set([2])]]);
      expect(ids(l.filteredImages)).toEqual([2]);
    });

    it("scopes a map selection to the listed photos, and follows a change of the set", () => {
      const l = createLibraryStore();
      l.images = [img({ image_id: 1 }), img({ image_id: 2 }), img({ image_id: 3 })];
      l.activeMapImageIds = new Set([1, 3]);
      expect(ids(l.filteredImages)).toEqual([1, 3]);
      l.activeMapImageIds = new Set([2]);
      expect(ids(l.filteredImages)).toEqual([2]);
      l.activeMapImageIds = new Set();
      expect(l.filteredImages).toEqual([]);
      l.activeMapImageIds = null;
      expect(ids(l.filteredImages)).toEqual([1, 2, 3]);
    });

    it("scopes a person to their cached membership", () => {
      const l = createLibraryStore();
      l.images = [img({ image_id: 1 }), img({ image_id: 2 })];
      l.activePersonId = 4;
      expect(l.filteredImages).toEqual([]);
      l.personMembership = new Map([[4, new Set([1])]]);
      expect(ids(l.filteredImages)).toEqual([1]);
    });

    it("evaluates a smart collection's rules against the keyword assignments", () => {
      const l = createLibraryStore();
      l.images = [img({ image_id: 1 }), img({ image_id: 2 })];
      l.collections = [/** @type {any} */ ({ id: 9, name: "S", is_smart: true, rules: [{ field: "keyword", op: "has", value: 5 }] })];
      l.activeCollectionId = 9;
      l.allImageKeywords = [{ image_id: 2, keyword_id: 5 }];
      expect(l.keywordIdsByImage.get(2)).toEqual(new Set([5]));
      expect(ids(l.filteredImages)).toEqual([2]);
    });
  });

  describe("lastImportBatchId and Last Import scope", () => {
    it("is the highest import_batch, ignoring rows without one", () => {
      const l = createLibraryStore();
      l.images = [img({ image_id: 1, import_batch: 2 }), img({ image_id: 2, import_batch: 5 }), img({ image_id: 3 }), img({ image_id: 4, import_batch: 5 })];
      expect(l.lastImportBatchId).toBe(5);
      l.showLastImportOnly = true;
      expect(ids(l.filteredImages)).toEqual([2, 4]);
    });

    it("is null, and Last Import shows nothing, when no row is tagged", () => {
      const l = createLibraryStore();
      l.images = [img({ image_id: 1 })];
      expect(l.lastImportBatchId).toBeNull();
      l.showLastImportOnly = true;
      expect(l.filteredImages).toEqual([]);
    });
  });

  it("folderEntries groups by the last two directories, sorted, skipping shallow paths", () => {
    const l = createLibraryStore();
    l.images = [
      img({ image_id: 1, path: "/p/2026/wedding/a.jpg" }),
      img({ image_id: 2, path: "/p/2026/wedding/b.jpg" }),
      img({ image_id: 3, path: "/p/2025/trip/c.jpg" }),
      img({ image_id: 4, path: "/d.jpg" }),
    ];
    expect(l.folderEntries).toEqual([
      { key: "2025/trip", count: 1 },
      { key: "2026/wedding", count: 2 },
    ]);
  });

  it("cameraOptions and lensOptions come from the base set, not the filtered one", () => {
    const l = createLibraryStore();
    l.images = [
      img({ image_id: 1, camera_make: "Canon", camera_model: "R5", lens_model: "RF 50", flag: "pick" }),
      img({ image_id: 2, camera_make: "Sony", camera_model: "A7", lens_model: "FE 35" }),
    ];
    l.flagFilter = "pick";
    expect(l.cameraOptions).toEqual(["Canon R5", "Sony A7"]);
    expect(l.lensOptions).toEqual(["FE 35", "RF 50"]);
  });

  it("activeCollection and manualCollections follow the collections list", () => {
    const l = createLibraryStore();
    l.collections = [
      /** @type {any} */ ({ id: 1, name: "Manual", is_smart: false }),
      /** @type {any} */ ({ id: 2, name: "Smart", is_smart: true, rules: [] }),
    ];
    expect(l.activeCollection).toBeNull();
    expect(l.manualCollections.map((c) => c.id)).toEqual([1]);
    l.activeCollectionId = 2;
    expect(l.activeCollection?.name).toBe("Smart");
    l.activeCollectionId = 99; // deleted elsewhere
    expect(l.activeCollection).toBeNull();
  });
});
