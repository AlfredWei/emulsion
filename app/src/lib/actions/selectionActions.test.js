import { describe, it, expect, beforeEach } from "vitest";
import {
  targetVersionIds,
  selectGridStep,
  handleSelectAll,
  handleDeselectAll,
  handleSelect,
  handleCompareNextCandidate,
  handleComparePrevCandidate,
  handleCompareSwap,
  handleCompareMakeSelect,
} from "./selectionActions.js";
import { library } from "$lib/state/library.svelte.js";
import { selection } from "$lib/state/selection.svelte.js";

const img = (/** @type {number} */ id) =>
  /** @type {any} */ ({ image_id: id, version_id: id * 10, path: `/p/a/b/${id}.jpg`, rating: 0, flag: "none", color_label: "none" });
const ids = () => [...selection.selectedIds].sort((a, b) => a - b);
const pick = (/** @type {number} */ anchor, /** @type {number[]} */ all = [anchor]) => {
  selection.selectedId = anchor;
  selection.selectedIds = new Set(all);
};
const click = (/** @type {number} */ v, /** @type {Partial<MouseEvent>} */ e = {}) => handleSelect(v, /** @type {any} */ (e));

beforeEach(() => {
  library.images = [1, 2, 3, 4, 5].map(img);
  library.searchQuery = "";
  library.minRating = 0;
  library.libraryViewMode = "grid";
  library.compareCandidateId = null;
  selection.selectedId = null;
  selection.selectedIds = new Set();
});

describe("handleSelect", () => {
  it("a plain click replaces the selection and moves the anchor", () => {
    pick(10, [10, 20]);
    click(30);
    expect(selection.selectedId).toBe(30);
    expect(ids()).toEqual([30]);
  });

  it("Cmd/Ctrl-click adds an image and makes it the anchor", () => {
    pick(10);
    click(30, { metaKey: true });
    expect(selection.selectedId).toBe(30);
    expect(ids()).toEqual([10, 30]);
    click(40, { ctrlKey: true });
    expect(ids()).toEqual([10, 30, 40]);
  });

  it("Cmd-click on a selected image removes it; removing the anchor moves it to the last remaining", () => {
    pick(30, [10, 20, 30]);
    click(10, { metaKey: true }); // not the anchor
    expect(selection.selectedId).toBe(30);
    expect(ids()).toEqual([20, 30]);
    click(30, { metaKey: true }); // the anchor
    expect(ids()).toEqual([20]);
    expect(selection.selectedId).toBe(20);
    click(20, { metaKey: true }); // last one
    expect(selection.selectedId).toBeNull();
    expect(ids()).toEqual([]);
  });

  it("Shift-click selects the range from the anchor, in either direction, and keeps the anchor", () => {
    pick(20);
    click(40, { shiftKey: true });
    expect(ids()).toEqual([20, 30, 40]);
    expect(selection.selectedId).toBe(20);
    click(10, { shiftKey: true });
    expect(ids()).toEqual([10, 20]);
    expect(selection.selectedId).toBe(20);
  });

  it("the range follows the filtered order, not the whole catalog", () => {
    library.minRating = 0;
    library.images = [1, 2, 3, 4, 5].map((i) => ({ ...img(i), rating: i % 2 ? 5 : 0 })); // 1,3,5 rated
    library.minRating = 5;
    pick(10);
    click(50, { shiftKey: true });
    expect(ids()).toEqual([10, 30, 50]); // 20 and 40 are hidden, never pulled in
  });

  it("Shift-click with a stale or missing anchor behaves like a plain click", () => {
    pick(999);
    click(30, { shiftKey: true });
    expect(selection.selectedId).toBe(30);
    expect(ids()).toEqual([30]);
    selection.selectedId = null;
    selection.selectedIds = new Set();
    click(20, { shiftKey: true });
    expect(selection.selectedId).toBe(20);
    expect(ids()).toEqual([20]);
  });

  it("replaces the Set on every change (so reactivity fires)", () => {
    pick(10);
    const before = selection.selectedIds;
    click(20, { metaKey: true });
    expect(selection.selectedIds).not.toBe(before);
  });
});

describe("selectGridStep", () => {
  it("selects the first image when nothing is selected", () => {
    selectGridStep(1);
    expect(selection.selectedId).toBe(10);
    expect(ids()).toEqual([10]);
  });

  it("moves the anchor by the step and clamps at both ends", () => {
    pick(20);
    selectGridStep(2);
    expect(selection.selectedId).toBe(40);
    selectGridStep(10);
    expect(selection.selectedId).toBe(50);
    selectGridStep(-10);
    expect(selection.selectedId).toBe(10);
    expect(ids()).toEqual([10]);
  });

  it("with extend, selects the range from the old to the new position", () => {
    pick(30);
    selectGridStep(-2, true);
    expect(ids()).toEqual([10, 20, 30]);
    expect(selection.selectedId).toBe(10);
  });

  it("does nothing for an empty grid or an anchor outside the filter", () => {
    pick(999);
    selectGridStep(1);
    expect(selection.selectedId).toBe(999);
    library.searchQuery = "no-such-file";
    selection.selectedId = null;
    selectGridStep(1);
    expect(selection.selectedId).toBeNull();
  });
});

describe("select all / deselect all", () => {
  it("select all selects every filtered image and keeps a still-selected anchor", () => {
    library.minRating = 0;
    pick(30);
    handleSelectAll();
    expect(ids()).toEqual([10, 20, 30, 40, 50]);
    expect(selection.selectedId).toBe(30);
  });

  it("select all moves a missing anchor to the first image, and does nothing when nothing is shown", () => {
    pick(999);
    handleSelectAll();
    expect(selection.selectedId).toBe(10);
    library.searchQuery = "no-such-file";
    pick(20);
    handleSelectAll();
    expect(ids()).toEqual([20]);
  });

  it("deselect all outside the grid only returns to the grid", () => {
    pick(20, [20, 30]);
    library.libraryViewMode = "loupe";
    handleDeselectAll();
    expect(library.libraryViewMode).toBe("grid");
    expect(ids()).toEqual([20, 30]);
    handleDeselectAll();
    expect(ids()).toEqual([]);
    expect(selection.selectedId).toBeNull();
  });
});

describe("targetVersionIds", () => {
  it("an explicit id inside a multi-selection targets the whole selection", () => {
    pick(10, [10, 20, 30]);
    expect(targetVersionIds(20).sort()).toEqual([10, 20, 30]);
  });

  it("an explicit id outside the selection, or a lone selection, stays single-target", () => {
    pick(10, [10, 20]);
    expect(targetVersionIds(40)).toEqual([40]);
    pick(10, [10]);
    expect(targetVersionIds(10)).toEqual([10]);
  });

  it("with no id, targets the selection, else the anchor, else nothing", () => {
    pick(10, [10, 20]);
    expect(targetVersionIds(undefined).sort()).toEqual([10, 20]);
    expect(targetVersionIds(null).sort()).toEqual([10, 20]);
    selection.selectedIds = new Set();
    expect(targetVersionIds(undefined)).toEqual([10]);
    selection.selectedId = null;
    expect(targetVersionIds(undefined)).toEqual([]);
  });
});

describe("Compare navigation", () => {
  const pair = () => [selection.selectedId, library.compareCandidateId, ids()];

  it("next/prev move the candidate through the filtered list with wrap-around and pair it with the select image", () => {
    pick(10); // candidate defaults to the image after: 20
    handleCompareNextCandidate();
    expect(pair()).toEqual([10, 30, [10, 30]]);
    handleComparePrevCandidate();
    handleComparePrevCandidate();
    expect(pair()).toEqual([10, 10, [10]]); // the candidate may land on the select image itself
    handleComparePrevCandidate();
    expect(library.compareCandidateId).toBe(50);
  });

  it("next wraps from the last image to the first", () => {
    pick(10);
    library.compareCandidateId = 50;
    handleCompareNextCandidate();
    expect(library.compareCandidateId).toBe(10);
  });

  it("do nothing when the grid is empty", () => {
    library.searchQuery = "no-such-file";
    handleCompareNextCandidate();
    handleComparePrevCandidate();
    expect(library.compareCandidateId).toBeNull();
  });

  it("swap exchanges the select and candidate images", () => {
    pick(10);
    library.compareCandidateId = 40;
    handleCompareSwap();
    expect(pair()).toEqual([40, 10, [10, 40]]);
  });

  it("swap does nothing without a pair", () => {
    library.searchQuery = "no-such-file";
    handleCompareSwap();
    expect(selection.selectedId).toBeNull();
  });

  it("make-select promotes the candidate and picks the next image as the new candidate", () => {
    pick(10);
    library.compareCandidateId = 30;
    handleCompareMakeSelect();
    expect(pair()).toEqual([30, 40, [30, 40]]);
    library.compareCandidateId = 50;
    handleCompareMakeSelect();
    expect(pair()).toEqual([50, 10, [10, 50]]); // wraps
  });

  it("make-select with a single image just selects it", () => {
    library.images = [img(1)];
    handleCompareMakeSelect();
    expect(selection.selectedId).toBe(10);
    expect(ids()).toEqual([10]);
  });
});
