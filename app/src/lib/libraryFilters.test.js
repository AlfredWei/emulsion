import { describe, it, expect } from "vitest";
import {
  cameraLabel,
  selectBaseImages,
  applyLibraryFilters,
  cameraOptionsFor,
  lensOptionsFor,
} from "./libraryFilters.js";

/** @param {Partial<import('./api/catalog.js').ImageSummary> & { image_id: number }} o */
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
    captured_at: null,
    import_batch: 1,
    ...o,
  });
}

const NO_FILTERS = {
  searchQuery: "",
  flagFilter: /** @type {const} */ ("all"),
  minRating: 0,
  ratingOp: /** @type {const} */ (">="),
  colorLabelFilter: "all",
  fileTypeFilter: /** @type {const} */ ("all"),
  cameraFilter: "all",
  lensFilter: "all",
  dateFrom: "",
  dateTo: "",
};

/** @param {any[]} images @param {object} [over] */
function baseInputs(images, over = {}) {
  return {
    images,
    showLastImportOnly: false,
    lastImportBatchId: null,
    activeFolderKey: null,
    activePersonId: null,
    personMembership: new Map(),
    activeCollectionId: null,
    collections: [],
    manualMembership: new Map(),
    keywordIdsByImage: new Map(),
    ...over,
  };
}

/** @param {any[]} baseImages @param {object} [over] */
const filterInputs = (baseImages, over = {}) => ({ baseImages, ...NO_FILTERS, ...over });
const ids = (/** @type {any[]} */ list) => list.map((i) => i.image_id);

describe("cameraLabel", () => {
  it("joins make and model, and is null when both are missing", () => {
    expect(cameraLabel({ camera_make: "Canon", camera_model: "EOS R5" })).toBe("Canon EOS R5");
    expect(cameraLabel({ camera_make: "Canon", camera_model: null })).toBe("Canon");
    expect(cameraLabel({ camera_make: null, camera_model: null })).toBeNull();
  });
});

describe("selectBaseImages", () => {
  const images = [
    img({ image_id: 1, path: "/a/2026/wedding/1.jpg", import_batch: 1, rating: 5 }),
    img({ image_id: 2, path: "/a/2026/wedding/2.jpg", import_batch: 2, rating: 1 }),
    img({ image_id: 3, path: "/a/2026/trip/3.jpg", import_batch: 2, rating: 4 }),
  ];

  it("returns everything with no scope active", () => {
    expect(ids(selectBaseImages(baseInputs(images)))).toEqual([1, 2, 3]);
  });

  it("last-import scope keeps only that batch, and is empty when no batch exists", () => {
    expect(ids(selectBaseImages(baseInputs(images, { showLastImportOnly: true, lastImportBatchId: 2 })))).toEqual([2, 3]);
    expect(selectBaseImages(baseInputs(images, { showLastImportOnly: true, lastImportBatchId: null }))).toEqual([]);
  });

  it("last-import scope wins over a folder scope (checked first)", () => {
    const out = selectBaseImages(
      baseInputs(images, { showLastImportOnly: true, lastImportBatchId: 1, activeFolderKey: "2026/trip" }),
    );
    expect(ids(out)).toEqual([1]);
  });

  it("folder scope matches the last two parent directories", () => {
    expect(ids(selectBaseImages(baseInputs(images, { activeFolderKey: "2026/wedding" })))).toEqual([1, 2]);
  });

  it("person scope uses fetched membership; unfetched membership is empty, not everything", () => {
    const withMembers = baseInputs(images, { activePersonId: 7, personMembership: new Map([[7, new Set([3])]]) });
    expect(ids(selectBaseImages(withMembers))).toEqual([3]);
    expect(selectBaseImages(baseInputs(images, { activePersonId: 7 }))).toEqual([]);
  });

  it("manual collection scope uses membership; unfetched membership is empty", () => {
    const collections = [{ id: 5, name: "Best", is_smart: false }];
    const fetched = baseInputs(images, { activeCollectionId: 5, collections, manualMembership: new Map([[5, new Set([1, 3])]]) });
    expect(ids(selectBaseImages(fetched))).toEqual([1, 3]);
    expect(selectBaseImages(baseInputs(images, { activeCollectionId: 5, collections }))).toEqual([]);
  });

  it("smart collection scope evaluates its rules against every image", () => {
    const collections = [{ id: 9, name: "4+", is_smart: true, rules: [{ field: "rating", op: ">=", value: 4 }] }];
    expect(ids(selectBaseImages(baseInputs(images, { activeCollectionId: 9, collections })))).toEqual([1, 3]);
  });

  it("falls back to all images when the active collection id no longer exists", () => {
    expect(ids(selectBaseImages(baseInputs(images, { activeCollectionId: 404 })))).toEqual([1, 2, 3]);
  });

  it("reads scope state lazily: an earlier scope's branch never touches later inputs", () => {
    /** @type {string[]} */
    const reads = [];
    const spy = new Proxy(baseInputs(images, { showLastImportOnly: true, lastImportBatchId: 1 }), {
      get(target, prop, receiver) {
        reads.push(String(prop));
        return Reflect.get(target, prop, receiver);
      },
    });
    selectBaseImages(spy);
    expect(reads).not.toContain("activeFolderKey");
    expect(reads).not.toContain("collections");
    expect(reads).not.toContain("personMembership");
  });
});

describe("applyLibraryFilters", () => {
  const images = [
    img({ image_id: 1, flag: "pick", rating: 5, color_label: "red", camera_make: "Canon", camera_model: "R5", lens_model: "RF 50", captured_at: "2026-03-10T10:00:00Z", caption: "Sunset over the pier" }),
    img({ image_id: 2, flag: "reject", rating: 2, color_label: "blue", camera_make: "Nikon", camera_model: "Z6", lens_model: "Z 24-70", captured_at: "2026-04-01T09:00:00Z", path: "/photos/2026/trip/raw2.nef" }),
    img({ image_id: 3, flag: "none", rating: 3, color_label: "none", captured_at: null, copyright: "(c) Alice" }),
  ];
  const run = (/** @type {object} */ over) => ids(applyLibraryFilters(filterInputs(images, over)));

  it("passes everything through with no filters", () => {
    expect(run({})).toEqual([1, 2, 3]);
  });

  it("search matches file name, path, camera, lens, caption, copyright and contact, case-insensitively", () => {
    expect(run({ searchQuery: "  SUNSET " })).toEqual([1]);
    expect(run({ searchQuery: "img1" })).toEqual([1]);
    expect(run({ searchQuery: "nikon" })).toEqual([2]);
    expect(run({ searchQuery: "z 24" })).toEqual([2]);
    expect(run({ searchQuery: "alice" })).toEqual([3]);
    expect(run({ searchQuery: "nothing matches this" })).toEqual([]);
  });

  it("flag filter: pick, reject, and unflagged (both 'none' and missing)", () => {
    expect(run({ flagFilter: "pick" })).toEqual([1]);
    expect(run({ flagFilter: "reject" })).toEqual([2]);
    expect(run({ flagFilter: "unflagged" })).toEqual([3]);
    const noFlagField = [img({ image_id: 4, flag: undefined })];
    expect(ids(applyLibraryFilters(filterInputs(noFlagField, { flagFilter: "unflagged" })))).toEqual([4]);
  });

  it("rating filter: >= and exact", () => {
    expect(run({ minRating: 3, ratingOp: ">=" })).toEqual([1, 3]);
    expect(run({ minRating: 3, ratingOp: "=" })).toEqual([3]);
    expect(run({ minRating: 0, ratingOp: "=" })).toEqual([1, 2, 3]); // 0 means "no rating filter"
  });

  it("color label filter", () => {
    expect(run({ colorLabelFilter: "red" })).toEqual([1]);
    expect(run({ colorLabelFilter: "none" })).toEqual([3]);
  });

  it("file type filter: jpeg vs raw (anything not .jpg/.jpeg)", () => {
    expect(run({ fileTypeFilter: "jpeg" })).toEqual([1, 3]);
    expect(run({ fileTypeFilter: "raw" })).toEqual([2]);
  });

  it("camera and lens filters", () => {
    expect(run({ cameraFilter: "Nikon Z6" })).toEqual([2]);
    expect(run({ lensFilter: "RF 50" })).toEqual([1]);
  });

  it("date range compares the YYYY-MM-DD prefix and drops images with no capture date", () => {
    expect(run({ dateFrom: "2026-03-15" })).toEqual([2]);
    expect(run({ dateTo: "2026-03-10" })).toEqual([1]);
    expect(run({ dateFrom: "2026-03-10", dateTo: "2026-04-01" })).toEqual([1, 2]);
  });

  it("combines filters (AND)", () => {
    expect(run({ flagFilter: "pick", minRating: 5, colorLabelFilter: "red" })).toEqual([1]);
    expect(run({ flagFilter: "pick", colorLabelFilter: "blue" })).toEqual([]);
  });

  it("does not read the rating operator unless a minimum rating is set", () => {
    /** @type {string[]} */
    const reads = [];
    const spy = new Proxy(filterInputs(images), {
      get(target, prop, receiver) {
        reads.push(String(prop));
        return Reflect.get(target, prop, receiver);
      },
    });
    applyLibraryFilters(spy);
    expect(reads).not.toContain("ratingOp");
  });
});

describe("cameraOptionsFor / lensOptionsFor", () => {
  const images = [
    img({ image_id: 1, camera_make: "Nikon", camera_model: "Z6", lens_model: "Z 24-70" }),
    img({ image_id: 2, camera_make: "Canon", camera_model: "R5", lens_model: "" }),
    img({ image_id: 3, camera_make: "Canon", camera_model: "R5", lens_model: "RF 50" }),
    img({ image_id: 4 }),
  ];

  it("returns distinct, sorted labels and skips missing values", () => {
    expect(cameraOptionsFor(images)).toEqual(["Canon R5", "Nikon Z6"]);
    expect(lensOptionsFor(images)).toEqual(["RF 50", "Z 24-70"]);
    expect(cameraOptionsFor([])).toEqual([]);
  });
});
