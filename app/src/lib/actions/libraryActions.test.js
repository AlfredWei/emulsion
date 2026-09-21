import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

const catalog = vi.hoisted(() => ({
  listCollections: vi.fn(),
  listCollectionImageIds: vi.fn(),
  listImages: vi.fn(),
  ensureThumbnail: vi.fn(),
  createCollection: vi.fn(),
  createSmartCollection: vi.fn(),
  deleteCollection: vi.fn(),
  removeImages: vi.fn(),
}));
const faceApi = vi.hoisted(() => ({ getImagesForPerson: vi.fn() }));
vi.mock("$lib/api/catalog.js", () => catalog);
const mapApi = vi.hoisted(() => ({ setGeoLocationBatch: vi.fn() }));
vi.mock("$lib/api/faces.js", () => faceApi);
vi.mock("$lib/api/map.js", () => mapApi);

import {
  loadPersonMembership,
  selectPerson,
  selectAllPhotos,
  selectLastImport,
  selectFolder,
  refreshCollections,
  loadManualMembership,
  handleResetFilters,
  selectCollection,
  refresh,
  patchLocal,
  handleBatchThumbnailsComplete,
  prioritizeThumbnail,
  handleCreateCollection,
  handleCreateSmartCollection,
  handleDeleteCollection,
  handleRemoveConfirmed,
  selectMapImages,
  showMapView,
  handleMapClusterSelect,
  handleMapAssignLocation,
  applyLocationLocally,
} from "./libraryActions.js";
import { library } from "$lib/state/library.svelte.js";
import { selection } from "$lib/state/selection.svelte.js";
import { develop } from "$lib/state/develop.svelte.js";
import { shell } from "$lib/state/shell.svelte.js";

const img = (/** @type {number} */ id, /** @type {object} */ over = {}) =>
  /** @type {any} */ ({ image_id: id, version_id: id * 10, path: `/p/a/b/${id}.jpg`, thumbnail_path: null, rating: 0, ...over });
const col = (/** @type {number} */ id, /** @type {boolean} */ is_smart = false) => /** @type {any} */ ({ id, name: `c${id}`, is_smart, rules: [] });

/** Which source fields are "on", in one comparable tuple. */
const sources = () => [library.activeCollectionId, library.activeFolderKey, library.showLastImportOnly, library.activePersonId];

beforeEach(() => {
  vi.clearAllMocks();
  library.libraryViewMode = "grid";
  library.images = [];
  library.collections = [];
  library.manualMembership = new Map();
  library.personMembership = new Map();
  selectAllPhotos();
  handleResetFilters();
  library.creatingCollection = false;
  library.creatingSmartCollection = false;
  for (const fn of [...Object.values(catalog), ...Object.values(faceApi), ...Object.values(mapApi)]) fn.mockResolvedValue(undefined);
});

describe("source switching", () => {
  it("each source turns exactly its own field on and the other three off", () => {
    library.activeCollectionId = 1;
    library.activeFolderKey = "x/y";
    library.showLastImportOnly = true;
    library.activePersonId = 3;

    selectAllPhotos();
    expect(sources()).toEqual([null, null, false, null]);

    selectLastImport();
    expect(sources()).toEqual([null, null, true, null]);

    selectFolder("2026/trip");
    expect(sources()).toEqual([null, "2026/trip", false, null]);

    library.activeCollectionId = 1;
    selectFolder("a/b");
    expect(sources()).toEqual([null, "a/b", false, null]);
  });

  it("selectMapImages turns on the map selection (a copy of the ids) and clears every other source", () => {
    library.activeCollectionId = 1;
    library.activeFolderKey = "x/y";
    library.showLastImportOnly = true;
    library.activePersonId = 3;
    const ids = [4, 5, 5];
    selectMapImages(ids);
    expect(sources()).toEqual([null, null, false, null]);
    expect(library.activeMapImageIds).toEqual(new Set([4, 5]));
    ids.push(6);
    expect(library.activeMapImageIds?.has(6)).toBe(false);
    selectMapImages(new Set());
    expect(library.activeMapImageIds).toEqual(new Set()); // empty is still "on"
  });

  it("showMapView opens the map and drops an earlier map selection so the pins show the whole source", () => {
    selectMapImages([1]);
    library.libraryViewMode = "grid";
    showMapView();
    expect(library.libraryViewMode).toBe("map");
    expect(library.activeMapImageIds).toBeNull();
  });

  it("showMapView keeps the other sources and filters", () => {
    library.activeFolderKey = "a/b";
    library.minRating = 3;
    showMapView();
    expect(library.activeFolderKey).toBe("a/b");
    expect(library.minRating).toBe(3);
  });

  it("handleMapClusterSelect scopes Library to the cluster's photos and returns to the grid", () => {
    library.libraryViewMode = "map";
    library.activeFolderKey = "a/b";
    handleMapClusterSelect([2, 3]);
    expect(library.libraryViewMode).toBe("grid");
    expect(library.activeMapImageIds).toEqual(new Set([2, 3]));
    expect(sources()).toEqual([null, null, false, null]);
  });

  it("applyLocationLocally sets the location on every version of the listed photos only", () => {
    library.images = [img(1), img(1, { version_id: 11 }), img(2)];
    applyLocationLocally([1], 10, 20);
    expect(library.images.map((i) => [i.latitude, i.longitude])).toEqual([[10, 20], [10, 20], [undefined, undefined]]);
  });

  it("handleMapAssignLocation saves once for the distinct photos, mirrors it locally and reports", async () => {
    library.images = [img(1), img(2), img(3)];
    mapApi.setGeoLocationBatch.mockResolvedValue(2);
    expect(await handleMapAssignLocation([1, 2, 2], 48.5, 2.5)).toBe(true);
    expect(mapApi.setGeoLocationBatch).toHaveBeenCalledWith([1, 2], 48.5, 2.5);
    expect(library.images.map((i) => i.latitude)).toEqual([48.5, 48.5, undefined]);
    expect(shell.statusMessage).toBe("Set location for 2 photos");
  });

  it("handleMapAssignLocation wraps a repeated-world longitude into range before saving", async () => {
    library.images = [img(1)];
    mapApi.setGeoLocationBatch.mockResolvedValue(1);
    await handleMapAssignLocation([1], 10, 190);
    expect(mapApi.setGeoLocationBatch).toHaveBeenCalledWith([1], 10, -170);
    expect(library.images[0].longitude).toBe(-170);
  });

  it("handleMapAssignLocation changes nothing when the save fails, or with no photos or a bad coordinate", async () => {
    library.images = [img(1)];
    mapApi.setGeoLocationBatch.mockRejectedValue("db locked");
    expect(await handleMapAssignLocation([1], 1, 2)).toBe(false);
    expect(library.images[0].latitude).toBeUndefined();
    expect(shell.statusMessage).toBe("Could not set location: db locked");
    mapApi.setGeoLocationBatch.mockClear();
    expect(await handleMapAssignLocation([], 1, 2)).toBe(false);
    expect(await handleMapAssignLocation([1], NaN, 2)).toBe(false);
    expect(mapApi.setGeoLocationBatch).not.toHaveBeenCalled();
  });

  it("every other source switch turns the map selection off", async () => {
    faceApi.getImagesForPerson.mockResolvedValue([1]);
    library.collections = [col(1)];
    catalog.listCollectionImageIds.mockResolvedValue([1]);
    /** @type {[string, () => unknown][]} */
    const switches = [
      ["all photos", () => selectAllPhotos()],
      ["last import", () => selectLastImport()],
      ["folder", () => selectFolder("a/b")],
      ["person", () => selectPerson(4)],
      ["collection", () => selectCollection(1)],
    ];
    for (const [name, run] of switches) {
      selectMapImages([1]);
      await run();
      expect(library.activeMapImageIds, name).toBeNull();
    }
  });

  it("with a map selection active, filteredImages is exactly those photos", () => {
    library.images = [img(1), img(2), img(3)];
    selectMapImages([2]);
    expect(library.filteredImages.map((i) => i.image_id)).toEqual([2]);
    selectAllPhotos();
    expect(library.filteredImages).toHaveLength(3);
  });

  it("selectPerson switches to the person and fetches their membership once", async () => {
    faceApi.getImagesForPerson.mockResolvedValue([5, 6]);
    library.activeCollectionId = 1;
    library.activeFolderKey = "x/y";
    library.showLastImportOnly = true;
    await selectPerson(4);
    expect(sources()).toEqual([null, null, false, 4]);
    expect(library.personMembership.get(4)).toEqual(new Set([5, 6]));
    await selectPerson(4);
    expect(faceApi.getImagesForPerson).toHaveBeenCalledTimes(1); // cached
  });

  it("selectPerson updates the source synchronously, before the membership fetch resolves", () => {
    faceApi.getImagesForPerson.mockReturnValue(new Promise(() => {}));
    library.activeFolderKey = "x/y";
    selectPerson(4);
    expect(sources()).toEqual([null, null, false, 4]);
  });

  it("selectCollection fetches membership for an uncached manual collection only", async () => {
    library.collections = [col(1), col(2, true)];
    catalog.listCollectionImageIds.mockResolvedValue([9]);
    library.activeFolderKey = "x/y";
    library.activePersonId = 3;
    await selectCollection(1);
    expect(sources()).toEqual([1, null, false, null]);
    expect(library.manualMembership.get(1)).toEqual(new Set([9]));

    await selectCollection(1); // cached
    await selectCollection(2); // smart: evaluated locally, nothing to fetch
    await selectCollection(null); // All Photos
    expect(catalog.listCollectionImageIds).toHaveBeenCalledTimes(1);
    expect(sources()).toEqual([null, null, false, null]);
  });

  it("selectCollection for an id that is not in the list fetches nothing", async () => {
    await selectCollection(42);
    expect(library.activeCollectionId).toBe(42);
    expect(catalog.listCollectionImageIds).not.toHaveBeenCalled();
  });
});

describe("membership caches", () => {
  it("replace the Map (so reactivity fires) and keep other entries", async () => {
    library.manualMembership = new Map([[1, new Set([1])]]);
    const before = library.manualMembership;
    catalog.listCollectionImageIds.mockResolvedValue([2, 3]);
    await loadManualMembership(2);
    expect(library.manualMembership).not.toBe(before);
    expect([...library.manualMembership.keys()]).toEqual([1, 2]);

    faceApi.getImagesForPerson.mockResolvedValue([8]);
    await loadPersonMembership(5);
    expect(library.personMembership.get(5)).toEqual(new Set([8]));
  });
});

describe("handleResetFilters", () => {
  it("restores all ten filter fields to their defaults and leaves the source alone", () => {
    library.searchQuery = "q";
    library.flagFilter = "pick";
    library.minRating = 3;
    library.ratingOp = "=";
    library.colorLabelFilter = "red";
    library.fileTypeFilter = "raw";
    library.cameraFilter = "Canon";
    library.lensFilter = "50";
    library.dateFrom = "2026-01-01";
    library.dateTo = "2026-02-01";
    library.activeFolderKey = "x/y";
    handleResetFilters();
    expect([library.searchQuery, library.flagFilter, library.minRating, library.ratingOp]).toEqual(["", "all", 0, ">="]);
    expect([library.colorLabelFilter, library.fileTypeFilter, library.cameraFilter, library.lensFilter, library.dateFrom, library.dateTo]).toEqual([
      "all",
      "all",
      "all",
      "all",
      "",
      "",
    ]);
    expect(library.activeFolderKey).toBe("x/y");
  });
});

describe("refresh and refreshCollections", () => {
  it("replace images / collections with what the catalog returns", async () => {
    catalog.listImages.mockResolvedValue([img(1), img(2)]);
    catalog.listCollections.mockResolvedValue([col(3)]);
    await refresh();
    await refreshCollections();
    expect(library.images.map((i) => i.image_id)).toEqual([1, 2]);
    expect(library.collections.map((c) => c.id)).toEqual([3]);
  });
});

describe("local image patches", () => {
  it("patchLocal changes only the matching version, immutably", () => {
    library.images = [img(1), img(2)];
    const before = library.images;
    patchLocal(20, { rating: 4 });
    expect(library.images).not.toBe(before);
    expect(library.images.map((i) => i.rating)).toEqual([0, 4]);
  });

  it("patchLocal for an unknown version changes nothing", () => {
    library.images = [img(1)];
    patchLocal(999, { rating: 5 });
    expect(library.images.map((i) => i.rating)).toEqual([0]);
  });

  it("handleBatchThumbnailsComplete applies every result in one pass, including an explicit null", () => {
    library.images = [img(1, { thumbnail_path: "old" }), img(2, { thumbnail_path: "keep" }), img(3, { thumbnail_path: "gone" })];
    handleBatchThumbnailsComplete(new Map([[10, "/t/1.jpg"], [30, null]]));
    expect(library.images.map((i) => i.thumbnail_path)).toEqual(["/t/1.jpg", "keep", null]);
  });

  it("prioritizeThumbnail generates one only for an image that has none, then patches it in", async () => {
    library.images = [img(1), img(2, { thumbnail_path: "/t/2.jpg" })];
    catalog.ensureThumbnail.mockResolvedValue("/t/1.jpg");
    prioritizeThumbnail(20); // already has one: pure local lookup
    prioritizeThumbnail(999); // not in the list
    expect(catalog.ensureThumbnail).not.toHaveBeenCalled();
    prioritizeThumbnail(10);
    expect(catalog.ensureThumbnail).toHaveBeenCalledWith(10);
    await vi.waitFor(() => expect(library.images[0].thumbnail_path).toBe("/t/1.jpg"));
  });

  it("prioritizeThumbnail swallows a failed generation and leaves the placeholder", async () => {
    library.images = [img(1)];
    catalog.ensureThumbnail.mockRejectedValue(new Error("decode failed"));
    expect(() => prioritizeThumbnail(10)).not.toThrow();
    await Promise.resolve();
    await Promise.resolve();
    expect(library.images[0].thumbnail_path).toBeNull();
  });
});

describe("collections", () => {
  it("handleCreateCollection closes the dialog first, then creates and refreshes", async () => {
    library.creatingCollection = true;
    catalog.createCollection.mockImplementation(async () => {
      expect(library.creatingCollection).toBe(false);
    });
    catalog.listCollections.mockResolvedValue([col(1)]);
    await handleCreateCollection("Trips");
    expect(catalog.createCollection).toHaveBeenCalledWith("Trips");
    expect(library.collections.map((c) => c.id)).toEqual([1]);
  });

  it("handleCreateSmartCollection passes the rules through", async () => {
    library.creatingSmartCollection = true;
    const rules = [/** @type {any} */ ({ field: "rating", op: ">=", value: 4 })];
    catalog.listCollections.mockResolvedValue([col(2, true)]);
    await handleCreateSmartCollection("Best", rules);
    expect(library.creatingSmartCollection).toBe(false);
    expect(catalog.createSmartCollection).toHaveBeenCalledWith("Best", rules);
    expect(library.collections.map((c) => c.id)).toEqual([2]);
  });

  it("handleDeleteCollection stops the click, clears the source only if it was the deleted one, and refreshes", async () => {
    const event = /** @type {any} */ ({ stopPropagation: vi.fn() });
    catalog.listCollections.mockResolvedValue([]);
    library.activeCollectionId = 5;
    await handleDeleteCollection(5, event);
    expect(event.stopPropagation).toHaveBeenCalled();
    expect(catalog.deleteCollection).toHaveBeenCalledWith(5);
    expect(library.activeCollectionId).toBeNull();

    library.activeCollectionId = 6;
    await handleDeleteCollection(5, event);
    expect(library.activeCollectionId).toBe(6);
  });
});

describe("handleRemoveConfirmed", () => {
  afterEach(() => vi.unstubAllGlobals());
  beforeEach(() => {
    vi.stubGlobal("document", { activeElement: null });
    // Two virtual copies of image 1 (versions 10, 11) and a separate image 2.
    library.images = [img(1), img(1, { version_id: 11 }), img(2), img(3)];
    library.confirmingRemoval = true;
    selection.selectedId = 10;
    selection.selectedIds = new Set([10, 11, 20]);
    develop.versionId = null;
    develop.imagePath = "";
    shell.notify("");
    catalog.removeImages.mockResolvedValue(undefined);
  });

  it("closes the dialog, removes each source photo once, drops every selected version and clears the selection", async () => {
    await handleRemoveConfirmed();
    expect(library.confirmingRemoval).toBe(false);
    expect(catalog.removeImages).toHaveBeenCalledWith([1, 2]); // deduped by image_id
    expect(library.images.map((i) => i.version_id)).toEqual([30]);
    expect(selection.selectedId).toBeNull();
    expect(selection.selectedIds.size).toBe(0);
    expect(shell.statusMessage).toBe("Removed 2 photos from catalog");
  });

  it("singular wording for one photo", async () => {
    selection.selectedIds = new Set([20]);
    await handleRemoveConfirmed();
    expect(shell.statusMessage).toBe("Removed 1 photo from catalog");
  });

  it("does nothing beyond closing the dialog when nothing is selected", async () => {
    selection.selectedIds = new Set();
    await handleRemoveConfirmed();
    expect(library.confirmingRemoval).toBe(false);
    expect(catalog.removeImages).not.toHaveBeenCalled();
    expect(library.images).toHaveLength(4);
  });

  it("on an IPC failure reports it and leaves library and selection alone", async () => {
    catalog.removeImages.mockRejectedValue("disk full");
    await handleRemoveConfirmed();
    expect(shell.statusMessage).toBe("Remove failed: disk full");
    expect(library.images).toHaveLength(4);
    expect(selection.selectedIds.size).toBe(3);
  });

  it("clears the Develop session, and cancels its pending save, when the open version was removed", async () => {
    const cancel = vi.spyOn(develop, "cancelScheduledFlush");
    develop.versionId = 11;
    develop.imagePath = "/p/a/b/1.jpg";
    await handleRemoveConfirmed();
    expect(cancel).toHaveBeenCalled();
    expect(develop.versionId).toBeNull();
    expect(develop.imagePath).toBe("");
    cancel.mockRestore();
  });

  it("leaves the Develop session alone when the open version was not removed", async () => {
    const cancel = vi.spyOn(develop, "cancelScheduledFlush");
    develop.versionId = 30;
    develop.imagePath = "/p/a/b/3.jpg";
    await handleRemoveConfirmed();
    expect(cancel).not.toHaveBeenCalled();
    expect(develop.versionId).toBe(30);
    expect(develop.imagePath).toBe("/p/a/b/3.jpg");
    cancel.mockRestore();
  });

  it("blurs the focused element first so a pending IPTC blur-save fires before the rows vanish", async () => {
    const blur = vi.fn();
    vi.stubGlobal("document", { activeElement: { blur } });
    await handleRemoveConfirmed();
    expect(blur).toHaveBeenCalledTimes(1);
  });
});
