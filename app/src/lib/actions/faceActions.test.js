import { describe, it, expect, vi, beforeEach } from "vitest";

const api = vi.hoisted(() => ({
  listPeople: vi.fn(),
  renamePerson: vi.fn(),
  reassignFace: vi.fn(),
  createPerson: vi.fn(),
  setFaceExcluded: vi.fn(),
  cancelFaceDetection: vi.fn(),
  getFacesForImage: vi.fn(),
  detectFacesForImages: vi.fn(),
  getDevelopPreview: vi.fn(),
}));
vi.mock("$lib/api/faces.js", () => api);
vi.mock("$lib/api/develop.js", () => ({ getDevelopPreview: api.getDevelopPreview }));
vi.mock("@tauri-apps/api/core", () => ({ convertFileSrc: (/** @type {string} */ p) => `asset://${p}` }));

import {
  refreshPeople,
  handleRenamePerson,
  handleReassignFace,
  handleCreatePersonAndTagFace,
  handleSetFaceExcluded,
  handleCancelFaceDetection,
  refreshCurrentImageFaces,
  runFaceDetection,
  handleDetectFacesForSelected,
  handleDetectFacesForSelection,
  handleDetectFacesForFolder,
} from "./faceActions.js";
import { faces } from "$lib/state/faces.svelte.js";
import { library } from "$lib/state/library.svelte.js";
import { selection } from "$lib/state/selection.svelte.js";
import { shell } from "$lib/state/shell.svelte.js";

const person = (/** @type {number} */ id, /** @type {string | null} */ name, /** @type {string | null} */ cover) => ({
  id,
  name,
  cover_image_path: cover,
});
const face = (/** @type {number} */ id, /** @type {number | null} */ personId, /** @type {string | null} */ personName = null) => ({
  id,
  person_id: personId,
  person_name: personName,
});

beforeEach(() => {
  vi.clearAllMocks();
  faces.people = [];
  faces.currentImageFaces = [];
  faces.avatarSourceUrls = {};
  faces.detectingFaces = false;
  faces.detectionCancelable = false;
  faces.scanProgress = null;
  library.images = [];
  library.searchQuery = "";
  selection.selectedId = null;
  selection.selectedIds = new Set();
  shell.notify("");
  for (const fn of Object.values(api)) fn.mockResolvedValue(undefined);
  api.listPeople.mockResolvedValue([]); // the real call returns an array
});

describe("refreshPeople", () => {
  it("loads people and resolves each new cover photo to a decoded preview URL, once per path", async () => {
    api.listPeople.mockResolvedValue([person(1, "A", "/p/a.raw"), person(2, "B", "/p/a.raw"), person(3, null, "/p/c.heic"), person(4, "D", null)]);
    api.getDevelopPreview.mockImplementation(async (/** @type {string} */ path) => ({ path: `/cache${path}.jpg` }));
    await refreshPeople();
    expect(faces.people).toHaveLength(4);
    expect(api.getDevelopPreview).toHaveBeenCalledTimes(2); // shared cover decoded once, null cover skipped
    expect(faces.avatarSourceUrls).toEqual({
      "/p/a.raw": "asset:///cache/p/a.raw.jpg",
      "/p/c.heic": "asset:///cache/p/c.heic.jpg",
    });
  });

  it("does not decode covers it already has, and keeps the existing entries", async () => {
    faces.avatarSourceUrls = { "/p/a.raw": "asset://old" };
    api.listPeople.mockResolvedValue([person(1, "A", "/p/a.raw"), person(2, "B", "/p/new.raw")]);
    api.getDevelopPreview.mockResolvedValue({ path: "/cache/new.jpg" });
    await refreshPeople();
    expect(api.getDevelopPreview).toHaveBeenCalledTimes(1);
    expect(api.getDevelopPreview).toHaveBeenCalledWith("/p/new.raw", null);
    expect(faces.avatarSourceUrls).toEqual({ "/p/a.raw": "asset://old", "/p/new.raw": "asset:///cache/new.jpg" });
  });

  it("makes no decode calls when every cover is already known", async () => {
    faces.avatarSourceUrls = { "/p/a.raw": "asset://old" };
    api.listPeople.mockResolvedValue([person(1, "A", "/p/a.raw")]);
    await refreshPeople();
    expect(api.getDevelopPreview).not.toHaveBeenCalled();
  });

  it("a failed decode records null for that path instead of failing the refresh", async () => {
    api.listPeople.mockResolvedValue([person(1, "A", "/p/bad.raw"), person(2, "B", "/p/ok.raw")]);
    api.getDevelopPreview.mockImplementation(async (/** @type {string} */ path) => {
      if (path === "/p/bad.raw") throw new Error("decode failed");
      return { path: "/cache/ok.jpg" };
    });
    await refreshPeople();
    expect(faces.avatarSourceUrls).toEqual({ "/p/bad.raw": null, "/p/ok.raw": "asset:///cache/ok.jpg" });
  });
});

describe("handleRenamePerson", () => {
  it("updates the people list and the current photo's faces immediately, then saves", async () => {
    faces.people = /** @type {any} */ ([person(1, "Old", null), person(2, "Other", null)]);
    faces.currentImageFaces = /** @type {any} */ ([face(10, 1, "Old"), face(11, 2, "Other")]);
    api.renamePerson.mockImplementation(async () => {
      // by the time the IPC call starts, the local state is already updated (optimistic)
      expect(faces.people[0].name).toBe("New");
    });
    await handleRenamePerson(1, "New");
    expect(faces.people.map((p) => p.name)).toEqual(["New", "Other"]);
    expect(faces.currentImageFaces.map((f) => f.person_name)).toEqual(["New", "Other"]);
    expect(api.renamePerson).toHaveBeenCalledWith(1, "New");
  });

  it("clearing a name sets it to null", async () => {
    faces.people = /** @type {any} */ ([person(1, "Old", null)]);
    await handleRenamePerson(1, null);
    expect(faces.people[0].name).toBeNull();
  });
});

describe("handleReassignFace", () => {
  it("relabels the face with the person's name, saves, then refreshes people", async () => {
    faces.people = /** @type {any} */ ([person(7, "Sam", null)]);
    faces.currentImageFaces = /** @type {any} */ ([face(10, null), face(11, 3, "Kim")]);
    api.listPeople.mockResolvedValue([person(7, "Sam", null)]);
    await handleReassignFace(10, 7);
    expect(faces.currentImageFaces).toEqual([face(10, 7, "Sam"), face(11, 3, "Kim")]);
    expect(api.reassignFace).toHaveBeenCalledWith(10, 7);
    expect(api.listPeople).toHaveBeenCalledTimes(1);
  });

  it("personId null clears the face back to unclustered", async () => {
    faces.currentImageFaces = /** @type {any} */ ([face(10, 7, "Sam")]);
    await handleReassignFace(10, null);
    expect(faces.currentImageFaces[0]).toMatchObject({ person_id: null, person_name: null });
    expect(api.reassignFace).toHaveBeenCalledWith(10, null);
  });

  it("an unknown person id leaves the name null", async () => {
    faces.currentImageFaces = /** @type {any} */ ([face(10, null)]);
    await handleReassignFace(10, 999);
    expect(faces.currentImageFaces[0]).toMatchObject({ person_id: 999, person_name: null });
  });
});

describe("handleCreatePersonAndTagFace", () => {
  it("creates the person, names it, then assigns the face -- in that order", async () => {
    /** @type {string[]} */
    const order = [];
    api.createPerson.mockImplementation(async () => (order.push("create"), 42));
    api.renamePerson.mockImplementation(async () => void order.push("rename"));
    api.reassignFace.mockImplementation(async () => void order.push("reassign"));
    await handleCreatePersonAndTagFace(10, "Alex");
    expect(order).toEqual(["create", "rename", "reassign"]);
    expect(api.createPerson).toHaveBeenCalledWith(10);
    expect(api.renamePerson).toHaveBeenCalledWith(42, "Alex");
    expect(api.reassignFace).toHaveBeenCalledWith(10, 42);
  });
});

describe("handleSetFaceExcluded", () => {
  it("excluding removes the face from the current photo immediately", async () => {
    faces.currentImageFaces = /** @type {any} */ ([face(10, null), face(11, null)]);
    await handleSetFaceExcluded(10, true);
    expect(faces.currentImageFaces.map((f) => f.id)).toEqual([11]);
    expect(api.setFaceExcluded).toHaveBeenCalledWith(10, true);
    expect(api.listPeople).toHaveBeenCalled();
  });

  it("un-excluding does not touch the local face list", async () => {
    faces.currentImageFaces = /** @type {any} */ ([face(10, null)]);
    await handleSetFaceExcluded(10, false);
    expect(faces.currentImageFaces).toHaveLength(1);
    expect(api.setFaceExcluded).toHaveBeenCalledWith(10, false);
  });
});

describe("handleCancelFaceDetection", () => {
  it("asks the backend to cancel, and swallows a failure", async () => {
    api.cancelFaceDetection.mockRejectedValue(new Error("nothing running"));
    expect(() => handleCancelFaceDetection()).not.toThrow();
    await Promise.resolve();
    expect(api.cancelFaceDetection).toHaveBeenCalledTimes(1);
  });
});

const photo = (/** @type {number} */ imageId, /** @type {boolean} */ scanned = false, /** @type {number} */ versionId = imageId * 10) =>
  /** @type {any} */ ({ image_id: imageId, version_id: versionId, path: `/p/a/b/${imageId}.jpg`, faces_scanned: scanned });

describe("refreshCurrentImageFaces", () => {
  it("clears the faces when nothing is selected, without any IPC", async () => {
    faces.currentImageFaces = [/** @type {any} */ (face(1, null))];
    await refreshCurrentImageFaces();
    expect(faces.currentImageFaces).toEqual([]);
    expect(api.getFacesForImage).not.toHaveBeenCalled();
  });

  it("loads the faces of the selected photo by image id (not version id)", async () => {
    library.images = [photo(4, false, 41)];
    selection.selectedId = 41;
    api.getFacesForImage.mockResolvedValue([face(7, null)]);
    await refreshCurrentImageFaces();
    expect(api.getFacesForImage).toHaveBeenCalledWith(4);
    expect(faces.currentImageFaces).toEqual([face(7, null)]);
  });
});

describe("runFaceDetection", () => {
  it("is a no-op with no ids or while a run is already in flight", async () => {
    await runFaceDetection([], true);
    faces.detectingFaces = true;
    await runFaceDetection([1], true);
    expect(api.detectFacesForImages).not.toHaveBeenCalled();
  });

  it("sets progress and cancelability while running, then marks the photos scanned and refreshes people", async () => {
    library.images = [photo(1), photo(2), photo(3)];
    /** @type {any[]} */
    const during = [];
    api.detectFacesForImages.mockImplementation(async () => {
      during.push([faces.detectingFaces, faces.detectionCancelable, faces.scanProgress]);
    });
    api.listPeople.mockResolvedValue([person(1, "A", null)]);
    await runFaceDetection([1, 3], true);
    expect(api.detectFacesForImages).toHaveBeenCalledWith([1, 3]);
    expect(during).toEqual([[true, true, { current: 0, total: 2 }]]);
    expect(library.images.map((i) => i.faces_scanned)).toEqual([true, false, true]);
    expect(faces.people).toHaveLength(1);
    expect([faces.detectingFaces, faces.detectionCancelable, faces.scanProgress]).toEqual([false, false, null]);
  });

  it("reports a failure and still clears the in-flight state, leaving photos unscanned", async () => {
    library.images = [photo(1)];
    api.detectFacesForImages.mockRejectedValue(new Error("model download failed"));
    await runFaceDetection([1], false);
    expect(shell.statusMessage).toBe("Face detection failed: Error: model download failed");
    expect(library.images[0].faces_scanned).toBe(false);
    expect([faces.detectingFaces, faces.detectionCancelable, faces.scanProgress]).toEqual([false, false, null]);
  });
});

describe("manual detection entry points", () => {
  it("per-photo: runs on the selected photo only, and does nothing without one", async () => {
    handleDetectFacesForSelected();
    expect(api.detectFacesForImages).not.toHaveBeenCalled();
    library.images = [photo(1), photo(2, false, 21)];
    selection.selectedId = 21;
    handleDetectFacesForSelected();
    expect(api.detectFacesForImages).toHaveBeenCalledWith([2]);
    await vi.waitFor(() => expect(faces.detectingFaces).toBe(false));
  });

  it("per-photo is not cancelable; the batch and folder runs are", async () => {
    library.images = [photo(1), photo(2)];
    selection.selectedId = 10;
    selection.selectedIds = new Set([10, 20]);
    /** @type {boolean[]} */
    const cancelable = [];
    api.detectFacesForImages.mockImplementation(async () => {
      cancelable.push(faces.detectionCancelable);
    });
    handleDetectFacesForSelected();
    await vi.waitFor(() => expect(faces.detectingFaces).toBe(false));
    library.images = [photo(1), photo(2)];
    handleDetectFacesForSelection();
    await vi.waitFor(() => expect(faces.detectingFaces).toBe(false));
    library.images = [photo(1), photo(2)]; // the runs above marked them scanned
    handleDetectFacesForFolder();
    await vi.waitFor(() => expect(faces.detectingFaces).toBe(false));
    expect(cancelable).toEqual([false, true, true]);
  });

  it("selection: skips already-scanned photos, de-scoped to the selection", async () => {
    library.images = [photo(1, true), photo(2), photo(3), photo(4)];
    selection.selectedId = 10;
    selection.selectedIds = new Set([10, 20, 30]);
    handleDetectFacesForSelection();
    expect(api.detectFacesForImages).toHaveBeenCalledWith([2, 3]);
    await vi.waitFor(() => expect(faces.detectingFaces).toBe(false));
  });

  it("selection: tells the user when everything selected was already scanned", () => {
    library.images = [photo(1, true), photo(2, true)];
    selection.selectedId = 10;
    selection.selectedIds = new Set([10, 20]);
    handleDetectFacesForSelection();
    expect(api.detectFacesForImages).not.toHaveBeenCalled();
    expect(shell.statusMessage).toBe("Every selected photo has already been scanned for faces.");
  });

  it("folder: uses whatever the current view shows (filtered), not the whole catalog", async () => {
    library.images = [photo(1), photo(2), photo(3, true)];
    library.searchQuery = "/2.jpg";
    handleDetectFacesForFolder();
    expect(api.detectFacesForImages).toHaveBeenCalledWith([2]);
    await vi.waitFor(() => expect(faces.detectingFaces).toBe(false));
  });

  it("folder: tells the user when the view is fully scanned", () => {
    library.images = [photo(1, true)];
    handleDetectFacesForFolder();
    expect(api.detectFacesForImages).not.toHaveBeenCalled();
    expect(shell.statusMessage).toBe("Every photo in this view has already been scanned for faces.");
  });
});
