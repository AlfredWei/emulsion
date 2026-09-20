import { describe, it, expect, vi, beforeEach } from "vitest";

const api = vi.hoisted(() => ({
  listPeople: vi.fn(),
  renamePerson: vi.fn(),
  reassignFace: vi.fn(),
  createPerson: vi.fn(),
  setFaceExcluded: vi.fn(),
  cancelFaceDetection: vi.fn(),
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
} from "./faceActions.js";
import { faces } from "$lib/state/faces.svelte.js";

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
