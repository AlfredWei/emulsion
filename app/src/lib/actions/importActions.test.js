import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

const catalog = vi.hoisted(() => ({
  importFolder: vi.fn(),
  importFiles: vi.fn(),
  getSupportedExtensions: vi.fn(),
  backfillMissingThumbnails: vi.fn(),
  mergeHdrBracket: vi.fn(),
  mergePanorama: vi.fn(),
  listImages: vi.fn(),
  ensureThumbnail: vi.fn(),
  listCollections: vi.fn(),
  listCollectionImageIds: vi.fn(),
  createCollection: vi.fn(),
  createSmartCollection: vi.fn(),
  deleteCollection: vi.fn(),
}));
const faceApi = vi.hoisted(() => ({
  detectFacesForImportBatch: vi.fn(),
  listPeople: vi.fn(),
  getImagesForPerson: vi.fn(),
  renamePerson: vi.fn(),
  reassignFace: vi.fn(),
  createPerson: vi.fn(),
  setFaceExcluded: vi.fn(),
  cancelFaceDetection: vi.fn(),
  getFacesForImage: vi.fn(),
  detectFacesForImages: vi.fn(),
}));
const dialog = vi.hoisted(() => ({ open: vi.fn() }));
const queue = vi.hoisted(() => ({ queueThumbnailRegeneration: vi.fn() }));
vi.mock("$lib/api/catalog.js", () => catalog);
vi.mock("$lib/api/faces.js", () => faceApi);
vi.mock("$lib/api/develop.js", () => ({ getDevelopPreview: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ convertFileSrc: (/** @type {string} */ p) => p }));
vi.mock("@tauri-apps/plugin-dialog", () => dialog);
vi.mock("$lib/thumbnailBatchQueue.js", () => queue);

import {
  runImport,
  handleImportFolder,
  handleImportFiles,
  handleDropImport,
  handleMergeHdrBracket,
  handleMergePanorama,
  regenerateThumbnailFor,
  pollUntilThumbnailsReadyOnStartup,
} from "./importActions.js";
import { handleBatchThumbnailsComplete } from "./libraryActions.js";
import { importFlow } from "$lib/state/importFlow.svelte.js";
import { faces } from "$lib/state/faces.svelte.js";
import { library } from "$lib/state/library.svelte.js";
import { selection } from "$lib/state/selection.svelte.js";
import { shell } from "$lib/state/shell.svelte.js";

const summary = (/** @type {object} */ over = {}) => ({ imported: 3, skipped_duplicates: 1, failed: 0, import_batch: 7, ...over });
const photo = (/** @type {number} */ id, /** @type {string | null} */ thumb = "/t.jpg", /** @type {number} */ vid = id * 10) =>
  /** @type {any} */ ({ image_id: id, version_id: vid, path: `/p/a/b/${id}.jpg`, thumbnail_path: thumb });

beforeEach(() => {
  vi.clearAllMocks();
  for (const fn of [...Object.values(catalog), ...Object.values(faceApi), dialog.open]) fn.mockResolvedValue(undefined);
  faceApi.listPeople.mockResolvedValue([]);
  catalog.listImages.mockResolvedValue([]);
  importFlow.importing = false;
  importFlow.phase = "cataloging";
  importFlow.supportedExtensions = /** @type {any} */ (null);
  importFlow.mergingHdr = false;
  importFlow.mergingPanorama = false;
  faces.confirmingDetectionOnImport = false;
  library.images = [];
  selection.selectedId = null;
  selection.selectedIds = new Set();
  shell.notify("");
});

describe("runImport", () => {
  it("cataloging -> thumbnails: reports the summary, refreshes twice, backfills this batch, and resets progress", async () => {
    catalog.listImages.mockResolvedValue([photo(1)]);
    /** @type {any[]} */
    const seen = [];
    catalog.backfillMissingThumbnails.mockImplementation(async () => {
      seen.push([importFlow.importing, importFlow.phase]);
    });
    importFlow.catalogProgress = { current: 3, total: 3 };
    const done = runImport(async () => /** @type {any} */ (summary({ imported: 0 })));
    expect(importFlow.importing).toBe(true);
    expect(importFlow.catalogProgress).toBeNull(); // cleared at the start
    await done;
    expect(shell.statusMessage).toBe("Imported 0, 1 already in library, 0 failed");
    expect(catalog.listImages).toHaveBeenCalledTimes(2);
    expect(catalog.backfillMissingThumbnails).toHaveBeenCalledWith(7);
    expect(seen).toEqual([[true, "thumbnails"]]);
    expect(library.images).toHaveLength(1);
    expect([importFlow.importing, importFlow.catalogProgress, importFlow.thumbnailProgress, importFlow.faceDetectionProgress]).toEqual([false, null, null, null]);
  });

  it("clears the previous status message when it starts", async () => {
    shell.notify("old message");
    /** @type {string[]} */
    const during = [];
    await runImport(async () => {
      during.push(shell.statusMessage);
      return null;
    });
    expect(during).toEqual([""]);
  });

  it("a cancelled dialog (null summary) changes nothing and still ends the import", async () => {
    await runImport(async () => null);
    expect(catalog.backfillMissingThumbnails).not.toHaveBeenCalled();
    expect(catalog.listImages).not.toHaveBeenCalled();
    expect(shell.statusMessage).toBe("");
    expect(importFlow.importing).toBe(false);
  });

  it("asks about face detection only when something was imported, and runs it on yes", async () => {
    const done = runImport(async () => /** @type {any} */ (summary()));
    await vi.waitFor(() => expect(faces.confirmingDetectionOnImport).toBe(true));
    expect(faces.pendingImportBatchSize).toBe(3);
    faces.confirmDetectionPrompt();
    await done;
    expect(faceApi.detectFacesForImportBatch).toHaveBeenCalledWith(7);
    expect(faceApi.listPeople).toHaveBeenCalled();
    expect(importFlow.phase).toBe("faces");
  });

  it("does nothing more on no", async () => {
    const done = runImport(async () => /** @type {any} */ (summary()));
    await vi.waitFor(() => expect(faces.confirmingDetectionOnImport).toBe(true));
    faces.cancelDetectionPrompt();
    await done;
    expect(faceApi.detectFacesForImportBatch).not.toHaveBeenCalled();
    expect(importFlow.phase).toBe("thumbnails");
  });

  it("a failing detection pass is appended to the status, not a failed import", async () => {
    faceApi.detectFacesForImportBatch.mockRejectedValue(new Error("no network"));
    const done = runImport(async () => /** @type {any} */ (summary()));
    await vi.waitFor(() => expect(faces.confirmingDetectionOnImport).toBe(true));
    faces.confirmDetectionPrompt();
    await done;
    expect(shell.statusMessage).toBe("Imported 3, 1 already in library, 0 failed (face detection failed: Error: no network)");
  });

  it("an error anywhere reports 'Import failed' and still ends the import", async () => {
    await runImport(async () => {
      throw new Error("disk full");
    });
    expect(shell.statusMessage).toBe("Import failed: Error: disk full");
    expect(importFlow.importing).toBe(false);
    catalog.backfillMissingThumbnails.mockRejectedValue(new Error("decode"));
    await runImport(async () => /** @type {any} */ (summary({ imported: 0 })));
    expect(shell.statusMessage).toBe("Import failed: Error: decode");
    expect(importFlow.importing).toBe(false);
  });
});

describe("import entry points", () => {
  it("folder: imports the chosen directory, and does nothing when the dialog is cancelled", async () => {
    dialog.open.mockResolvedValueOnce(null);
    await handleImportFolder();
    expect(catalog.importFolder).not.toHaveBeenCalled();
    dialog.open.mockResolvedValueOnce("/photos/trip");
    catalog.importFolder.mockResolvedValue(summary({ imported: 0 }));
    await handleImportFolder();
    expect(dialog.open).toHaveBeenLastCalledWith({ directory: true, multiple: false });
    expect(catalog.importFolder).toHaveBeenCalledWith("/photos/trip");
  });

  it("files: fetches the supported extensions once, filters the dialog by them, and imports the picks", async () => {
    catalog.getSupportedExtensions.mockResolvedValue(["jpg", "nef"]);
    dialog.open.mockResolvedValue(["/a.jpg", "/b.nef"]);
    catalog.importFiles.mockResolvedValue(summary({ imported: 0 }));
    await handleImportFiles();
    await handleImportFiles();
    expect(catalog.getSupportedExtensions).toHaveBeenCalledTimes(1);
    expect(dialog.open).toHaveBeenLastCalledWith({ multiple: true, filters: [{ name: "Photos", extensions: ["jpg", "nef"] }] });
    expect(catalog.importFiles).toHaveBeenCalledWith(["/a.jpg", "/b.nef"]);
  });

  it("files: a cancelled dialog imports nothing", async () => {
    catalog.getSupportedExtensions.mockResolvedValue(["jpg"]);
    dialog.open.mockResolvedValue(null);
    await handleImportFiles();
    expect(catalog.importFiles).not.toHaveBeenCalled();
  });

  it("drop: ignores an empty or missing drop, and imports dropped paths", async () => {
    expect(handleDropImport([])).toBeUndefined();
    expect(handleDropImport(/** @type {any} */ (undefined))).toBeUndefined();
    expect(importFlow.importing).toBe(false);
    catalog.importFiles.mockResolvedValue(summary({ imported: 0 }));
    await handleDropImport(["/x.jpg"]);
    expect(catalog.importFiles).toHaveBeenCalledWith(["/x.jpg"]);
  });
});

describe.each([
  ["HDR", handleMergeHdrBracket, catalog.mergeHdrBracket, "mergingHdr", "Merged 2 photos into one HDR image", "HDR merge failed"],
  ["panorama", handleMergePanorama, catalog.mergePanorama, "mergingPanorama", "Stitched 2 photos into one panorama", "Panorama merge failed"],
])("%s merge", (_name, handler, api, flag, okMessage, failMessage) => {
  const pickTwo = () => {
    library.images = [photo(1), photo(2)];
    selection.selectedId = 10;
    selection.selectedIds = new Set([10, 20]);
  };

  it("needs two distinct source photos: one photo, or two versions of it, does nothing", async () => {
    library.images = [photo(1, "/t.jpg", 10), photo(1, "/t.jpg", 11)];
    selection.selectedId = 10;
    selection.selectedIds = new Set([10, 11]);
    await handler();
    expect(api).not.toHaveBeenCalled();
    expect(/** @type {any} */ (importFlow)[flag]).toBe(false);
  });

  it("merges the de-duplicated image ids, selects the result, and prioritizes its thumbnail", async () => {
    pickTwo();
    api.mockResolvedValue(9);
    catalog.listImages.mockResolvedValue([photo(1), photo(2), photo(9, null, 90)]);
    catalog.ensureThumbnail.mockResolvedValue("/t/9.jpg");
    /** @type {boolean[]} */
    const during = [];
    api.mockImplementation(async () => {
      during.push(/** @type {any} */ (importFlow)[flag]);
      return 9;
    });
    await handler();
    expect(api).toHaveBeenCalledWith([1, 2]);
    expect(during).toEqual([true]);
    expect(selection.selectedId).toBe(90);
    expect(selection.selectedIds).toEqual(new Set([90]));
    expect(shell.statusMessage).toBe(okMessage);
    expect(catalog.ensureThumbnail).toHaveBeenCalledWith(90);
    expect(/** @type {any} */ (importFlow)[flag]).toBe(false);
  });

  it("keeps the current selection when the result is not in the refreshed list", async () => {
    pickTwo();
    api.mockResolvedValue(9);
    catalog.listImages.mockResolvedValue([photo(1), photo(2)]);
    await handler();
    expect(selection.selectedId).toBe(10);
    expect(shell.statusMessage).toBe(okMessage);
  });

  it("reports a failure and still clears its in-flight flag", async () => {
    pickTwo();
    api.mockRejectedValue(new Error("not aligned"));
    await handler();
    expect(shell.statusMessage).toBe(`${failMessage}: Error: not aligned`);
    expect(/** @type {any} */ (importFlow)[flag]).toBe(false);
  });
});

describe("regenerateThumbnailFor", () => {
  it("queues a regeneration whose completion patches the grid, and ignores a null version", () => {
    regenerateThumbnailFor(null);
    expect(queue.queueThumbnailRegeneration).not.toHaveBeenCalled();
    regenerateThumbnailFor(30);
    expect(queue.queueThumbnailRegeneration).toHaveBeenCalledWith(30, handleBatchThumbnailsComplete);
  });
});

describe("pollUntilThumbnailsReadyOnStartup", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("returns at once when every photo already has a thumbnail", async () => {
    library.images = [photo(1)];
    await pollUntilThumbnailsReadyOnStartup();
    expect(catalog.listImages).not.toHaveBeenCalled();
  });

  it("re-fetches every 1.5 s until nothing is missing", async () => {
    library.images = [photo(1, null)];
    catalog.listImages.mockResolvedValueOnce([photo(1, null)]).mockResolvedValueOnce([photo(1, "/t.jpg")]);
    const done = pollUntilThumbnailsReadyOnStartup();
    await vi.advanceTimersByTimeAsync(1499);
    expect(catalog.listImages).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(1);
    expect(catalog.listImages).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(1500);
    await done;
    expect(catalog.listImages).toHaveBeenCalledTimes(2);
  });

  it("gives up after 10 attempts for a permanently missing thumbnail", async () => {
    library.images = [photo(1, null)];
    catalog.listImages.mockResolvedValue([photo(1, null)]);
    const done = pollUntilThumbnailsReadyOnStartup();
    await vi.advanceTimersByTimeAsync(1500 * 12);
    await done;
    expect(catalog.listImages).toHaveBeenCalledTimes(10);
  });

  it("a second call while one is polling does nothing, and polling can start again afterwards", async () => {
    library.images = [photo(1, null)];
    catalog.listImages.mockResolvedValue([photo(1, "/t.jpg")]);
    const first = pollUntilThumbnailsReadyOnStartup();
    const second = pollUntilThumbnailsReadyOnStartup();
    await vi.advanceTimersByTimeAsync(1500);
    await Promise.all([first, second]);
    expect(catalog.listImages).toHaveBeenCalledTimes(1);
    library.images = [photo(1, null)];
    const third = pollUntilThumbnailsReadyOnStartup();
    await vi.advanceTimersByTimeAsync(1500);
    await third;
    expect(catalog.listImages).toHaveBeenCalledTimes(2);
  });
});
