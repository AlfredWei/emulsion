import { describe, it, expect } from "vitest";
import { createLibraryStore } from "./library.svelte.js";
import { createSelectionStore } from "./selection.svelte.js";

const img = (/** @type {number} */ id, /** @type {object} */ over = {}) =>
  /** @type {any} */ ({
    image_id: id,
    version_id: id * 10,
    path: `/photos/2026/trip/img${id}.jpg`,
    rating: 0,
    flag: "none",
    color_label: "none",
    camera_make: null,
    camera_model: null,
    lens_model: null,
    import_batch: null,
    ...over,
  });

function setup(/** @type {number[]} */ imageIds = [1, 2, 3, 4]) {
  const library = createLibraryStore();
  const selection = createSelectionStore(library);
  library.images = imageIds.map((i) => img(i));
  return { library, selection };
}
const vids = (/** @type {any[]} */ list) => list.map((i) => i.version_id);

describe("SelectionStore", () => {
  it("starts with nothing selected", () => {
    const { selection } = setup([]);
    expect(selection.selectedId).toBeNull();
    expect(selection.selectedIds).toEqual(new Set());
    expect(selection.selectedImage).toBeNull();
    expect(selection.selectedImages).toEqual([]);
    expect(selection.keywordTargetImageIds).toEqual([]);
    expect(selection.compareSelectImage).toBeNull();
    expect(selection.compareCandidateImage).toBeNull();
  });

  it("stores are isolated per factory pair", () => {
    const a = setup();
    const b = setup();
    a.selection.selectedId = 10;
    expect(b.selection.selectedId).toBeNull();
  });

  describe("selectedImage / selectedImages", () => {
    it("look the ids up in the library's full list (not the filtered one)", () => {
      const { library, selection } = setup();
      library.flagFilter = "pick"; // filters everything out
      selection.selectedId = 20;
      selection.selectedIds = new Set([20, 40]);
      expect(selection.selectedImage?.image_id).toBe(2);
      expect(vids(selection.selectedImages)).toEqual([20, 40]);
    });

    it("selectedImage is null for an id that is no longer in the library, and follows library changes", () => {
      const { library, selection } = setup();
      selection.selectedId = 20;
      library.images = library.images.filter((i) => i.version_id !== 20);
      expect(selection.selectedImage).toBeNull();
    });
  });

  describe("keywordTargetImageIds", () => {
    it("is the whole selection's image ids when there is one", () => {
      const { selection } = setup();
      selection.selectedId = 10;
      selection.selectedIds = new Set([10, 30]);
      expect(selection.keywordTargetImageIds).toEqual([1, 3]);
    });

    it("a one-image selection wins over a different anchor", () => {
      const { selection } = setup();
      selection.selectedId = 10;
      selection.selectedIds = new Set([30]);
      expect(selection.keywordTargetImageIds).toEqual([3]);
    });

    it("falls back to the anchor image alone, then to nothing", () => {
      const { selection } = setup();
      selection.selectedId = 20;
      expect(selection.keywordTargetImageIds).toEqual([2]);
      selection.selectedId = null;
      expect(selection.keywordTargetImageIds).toEqual([]);
    });
  });

  describe("Compare pair", () => {
    it("compareSelectImage is the anchor if it is in the filtered set, else the first filtered image", () => {
      const { library, selection } = setup();
      expect(selection.compareSelectImage?.version_id).toBe(10); // nothing selected
      selection.selectedId = 30;
      expect(selection.compareSelectImage?.version_id).toBe(30);
      library.minRating = 5; // everything filtered out
      expect(selection.compareSelectImage).toBeNull();
      library.minRating = 0;
      library.searchQuery = "img4"; // anchor filtered out, one image left
      expect(selection.compareSelectImage?.version_id).toBe(40);
    });

    it("compareCandidateImage prefers the explicit candidate id when it is filtered in", () => {
      const { library, selection } = setup();
      selection.selectedId = 10;
      selection.selectedIds = new Set([10, 20]);
      library.compareCandidateId = 40;
      expect(selection.compareCandidateImage?.version_id).toBe(40);
      library.compareCandidateId = 999; // not in the list: falls through to the next rule
      expect(selection.compareCandidateImage?.version_id).toBe(20);
    });

    it("otherwise takes the other member of a multi-selection", () => {
      const { selection } = setup();
      selection.selectedId = 30;
      selection.selectedIds = new Set([30, 10]);
      expect(selection.compareCandidateImage?.version_id).toBe(10);
    });

    it("otherwise takes the image after the select image, wrapping at the end", () => {
      const { selection } = setup();
      selection.selectedId = 20;
      selection.selectedIds = new Set([20]);
      expect(selection.compareCandidateImage?.version_id).toBe(30);
      selection.selectedId = 40;
      selection.selectedIds = new Set([40]);
      expect(selection.compareCandidateImage?.version_id).toBe(10);
    });

    it("with a single filtered image the candidate is that image itself", () => {
      const { selection } = setup([1]);
      expect(selection.compareCandidateImage?.version_id).toBe(10);
    });
  });
});
