import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

const cullApi = vi.hoisted(() => ({ listSimilarGroups: vi.fn(), cullAnalysisStatus: vi.fn() }));
const catalog = vi.hoisted(() => ({ setFlag: vi.fn(), listImages: vi.fn() }));
vi.mock("$lib/api/cull.js", () => cullApi);
vi.mock("$lib/api/catalog.js", () => catalog);

import { refreshSimilar, showSimilar, leaveSimilar, setSimilarity, openSimilarGroup, closeSimilarGroup, askRejectOthers, cancelRejectOthers, confirmRejectOthers } from "./similarActions.js";
import { selectAllPhotos } from "./libraryActions.js";
import { library } from "$lib/state/library.svelte.js";
import { selection } from "$lib/state/selection.svelte.js";
import { similar } from "$lib/state/similar.svelte.js";
import { shell } from "$lib/state/shell.svelte.js";

const img = (/** @type {number} */ id, /** @type {object} */ over = {}) =>
  /** @type {any} */ ({ image_id: id, version_id: id * 10, path: `/p/${id}.jpg`, flag: "none", rating: 0, color_label: "none", ...over });
const member = (/** @type {number} */ id, /** @type {object} */ over = {}) => ({ image_id: id, version_id: id * 10, relative_sharpness: 1, soft: false, ...over });
const GROUPS = [{ members: [member(1), member(2, { relative_sharpness: 0.3, soft: true }), member(3)], best_image_id: 1 }];

beforeEach(() => {
  vi.useFakeTimers();
  vi.clearAllMocks();
  cullApi.listSimilarGroups.mockResolvedValue(GROUPS);
  cullApi.cullAnalysisStatus.mockResolvedValue({ analysed: 3, total: 3 });
  catalog.setFlag.mockResolvedValue(undefined);
  library.images = [1, 2, 3, 4].map((i) => img(i));
  similar.leave();
  similar.groups = [];
  similar.status = null;
  similar.loaded = false;
  similar.similarity = 0.5;
  selection.selectedId = null;
  selection.selectedIds = new Set();
  vi.spyOn(shell, "notify").mockImplementation(() => {});
});

afterEach(() => {
  leaveSimilar();
  vi.useRealTimers();
});

describe("showSimilar / refreshSimilar", () => {
  it("turns the view on, leaves other rail sources, and fetches the groups with the slider position", async () => {
    library.activeFolderKey = "/a";
    library.showLastImportOnly = true;
    similar.similarity = 0.7;
    showSimilar();
    expect(similar.active).toBe(true);
    expect(library.activeFolderKey).toBeNull();
    expect(library.showLastImportOnly).toBe(false);
    await vi.advanceTimersByTimeAsync(0);
    expect(cullApi.listSimilarGroups).toHaveBeenCalledWith(0.7);
    expect(similar.loaded).toBe(true);
    expect(similar.groupViews).toHaveLength(1);
    expect(similar.status).toEqual({ analysed: 3, total: 3 });
  });

  it("keeps what is shown when the backend fails", async () => {
    similar.groups = GROUPS;
    cullApi.listSimilarGroups.mockRejectedValueOnce(new Error("boom"));
    vi.spyOn(console, "error").mockImplementation(() => {});
    await refreshSimilar();
    expect(similar.groups).toEqual(GROUPS);
  });

  it("an older answer never replaces a newer one", async () => {
    /** @type {(v: any) => void} */ let slow = () => {};
    cullApi.listSimilarGroups.mockImplementationOnce(() => new Promise((r) => (slow = r)));
    const first = refreshSimilar();
    cullApi.listSimilarGroups.mockResolvedValueOnce([]);
    await refreshSimilar();
    slow(GROUPS);
    await first;
    expect(similar.groups).toEqual([]);
  });

  it("any other rail source switches the view off", () => {
    showSimilar();
    similar.openKey = 1;
    selectAllPhotos();
    expect(similar.active).toBe(false);
    expect(similar.openKey).toBeNull();
  });
});

describe("polling while photos are still being analysed", () => {
  it("fetches the groups again when the analysed count moves, and not otherwise", async () => {
    cullApi.cullAnalysisStatus.mockResolvedValue({ analysed: 1, total: 3 });
    showSimilar();
    await vi.advanceTimersByTimeAsync(0);
    cullApi.listSimilarGroups.mockClear();

    await vi.advanceTimersByTimeAsync(2000); // same count: no refetch
    expect(cullApi.listSimilarGroups).not.toHaveBeenCalled();

    cullApi.cullAnalysisStatus.mockResolvedValue({ analysed: 2, total: 3 });
    await vi.advanceTimersByTimeAsync(2000);
    expect(cullApi.listSimilarGroups).toHaveBeenCalledTimes(1);
  });

  it("stops when the view is left", async () => {
    showSimilar();
    await vi.advanceTimersByTimeAsync(0);
    cullApi.cullAnalysisStatus.mockClear();
    leaveSimilar();
    await vi.advanceTimersByTimeAsync(10000);
    expect(cullApi.cullAnalysisStatus).not.toHaveBeenCalled();
  });
});

describe("setSimilarity", () => {
  it("remembers the position and fetches once the slider rests", async () => {
    showSimilar();
    await vi.advanceTimersByTimeAsync(0);
    cullApi.listSimilarGroups.mockClear();
    setSimilarity(0.2);
    setSimilarity(0.3);
    setSimilarity(0.4);
    expect(similar.similarity).toBe(0.4);
    expect(cullApi.listSimilarGroups).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(200);
    expect(cullApi.listSimilarGroups).toHaveBeenCalledTimes(1);
    expect(cullApi.listSimilarGroups).toHaveBeenCalledWith(0.4);
  });

  it("clamps to 0..1", () => {
    setSimilarity(5);
    expect(similar.similarity).toBe(1);
    setSimilarity(-1);
    expect(similar.similarity).toBe(0);
  });
});

describe("opening a group and rejecting the others", () => {
  async function openFirst() {
    showSimilar();
    await vi.advanceTimersByTimeAsync(0);
    openSimilarGroup(similar.groupViews[0].key);
  }

  it("opens with the suggested frame as the only selected photo", async () => {
    await openFirst();
    expect(similar.openGroup?.frames).toHaveLength(3);
    expect(selection.selectedId).toBe(10);
    expect([...selection.selectedIds]).toEqual([10]);
  });

  it("opens with the first frame selected when none is suggested", async () => {
    cullApi.listSimilarGroups.mockResolvedValue([{ members: GROUPS[0].members, best_image_id: null }]);
    await openFirst();
    expect(selection.selectedId).toBe(10);
  });

  it("going back closes the group and any pending confirmation", async () => {
    await openFirst();
    askRejectOthers(10);
    closeSimilarGroup();
    expect(similar.openKey).toBeNull();
    expect(similar.pendingReject).toBeNull();
  });

  it("asks first: the others, never the kept frame, and nothing is saved yet", async () => {
    await openFirst();
    askRejectOthers(20);
    expect(similar.pendingReject).toEqual({ keepVersionId: 20, versionIds: [10, 30] });
    expect(catalog.setFlag).not.toHaveBeenCalled();
    cancelRejectOthers();
    expect(similar.pendingReject).toBeNull();
    expect(catalog.setFlag).not.toHaveBeenCalled();
  });

  it("confirming flags exactly those photos, locally and in the catalog, and only the flag", async () => {
    await openFirst();
    askRejectOthers(10);
    await confirmRejectOthers();
    expect(catalog.setFlag.mock.calls.map((c) => c.join(":")).sort()).toEqual(["20:reject", "30:reject"]);
    expect(library.images.map((i) => i.flag)).toEqual(["none", "reject", "reject", "none"]);
    expect(similar.pendingReject).toBeNull();
    expect(shell.notify).toHaveBeenCalledWith("Rejected 2 photos");
  });

  it("says so and asks nothing when the others are already rejected", async () => {
    await openFirst();
    library.images = library.images.map((i) => (i.version_id === 20 || i.version_id === 30 ? { ...i, flag: "reject" } : i));
    askRejectOthers(10);
    expect(similar.pendingReject).toBeNull();
    expect(shell.notify).toHaveBeenCalledWith("Nothing to reject: the other photos are already rejected");
  });

  it("puts the list back from the catalog when saving fails", async () => {
    await openFirst();
    askRejectOthers(10);
    catalog.setFlag.mockRejectedValue(new Error("disk"));
    catalog.listImages.mockResolvedValue([1, 2, 3, 4].map((i) => img(i)));
    vi.spyOn(console, "error").mockImplementation(() => {});
    await confirmRejectOthers();
    expect(library.images.every((i) => i.flag === "none")).toBe(true);
    expect(shell.notify).toHaveBeenCalledWith("Could not save the Reject flags");
  });
});
