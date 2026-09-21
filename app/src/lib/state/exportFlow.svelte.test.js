import { describe, it, expect } from "vitest";
import { createExportFlowStore } from "./exportFlow.svelte.js";
import { createShellStore } from "./shell.svelte.js";
import { createLibraryStore } from "./library.svelte.js";
import { createSelectionStore } from "./selection.svelte.js";
import { DevelopStore } from "./develop.svelte.js";

const img = (/** @type {number} */ id) => /** @type {any} */ ({ image_id: id, version_id: id * 10, path: `/p/${id}.jpg` });

function setup() {
  const library = createLibraryStore();
  const shell = createShellStore({ panelWidths: { history: 0, develop: 0 }, shortcuts: {} });
  const selection = createSelectionStore(library);
  const develop = new DevelopStore(library);
  library.images = [img(1), img(2), img(3)];
  return { library, shell, selection, develop, flow: createExportFlowStore(shell, develop, selection) };
}

describe("ExportFlowStore", () => {
  it("starts closed and empty", () => {
    const { flow } = setup();
    expect(flow.items).toBeNull();
    expect(flow.currentItems).toEqual([]);
  });

  it("each factory call gets its own dialog state", () => {
    const a = setup().flow;
    const b = setup().flow;
    a.items = [{ path: "/x", version_id: 1 }];
    expect(b.items).toBeNull();
  });

  describe("currentItems", () => {
    it("in Develop with an image open: just that image, whatever is selected in the Library", () => {
      const { shell, selection, develop, flow } = setup();
      shell.activeModule = "develop";
      develop.versionId = 20;
      develop.imagePath = "/p/2.jpg";
      selection.selectedIds = new Set([10, 30]);
      expect(flow.currentItems).toEqual([{ path: "/p/2.jpg", version_id: 20 }]);
    });

    it("outside Develop, an open Develop image does not count: the Library selection wins", () => {
      const { shell, selection, develop, flow } = setup();
      shell.activeModule = "library";
      develop.versionId = 20;
      develop.imagePath = "/p/2.jpg";
      selection.selectedIds = new Set([10, 30]);
      expect(flow.currentItems).toEqual([
        { path: "/p/1.jpg", version_id: 10 },
        { path: "/p/3.jpg", version_id: 30 },
      ]);
    });

    it("in Develop with nothing open falls through to the selection", () => {
      const { shell, selection, flow } = setup();
      shell.activeModule = "develop";
      selection.selectedIds = new Set([30]);
      expect(flow.currentItems).toEqual([{ path: "/p/3.jpg", version_id: 30 }]);
    });

    it("with no multi-selection but an anchor, uses the anchor image", () => {
      const { selection, flow } = setup();
      selection.selectedId = 20;
      expect(flow.currentItems).toEqual([{ path: "/p/2.jpg", version_id: 20 }]);
    });

    it("is empty with nothing selected or open", () => {
      const { shell, flow } = setup();
      shell.activeModule = "print";
      expect(flow.currentItems).toEqual([]);
    });

    it("follows the module, the open image and the selection as they change", () => {
      const { shell, selection, develop, flow } = setup();
      selection.selectedIds = new Set([10]);
      expect(flow.currentItems.map((i) => i.version_id)).toEqual([10]);
      develop.versionId = 20;
      develop.imagePath = "/p/2.jpg";
      shell.activeModule = "develop";
      expect(flow.currentItems.map((i) => i.version_id)).toEqual([20]);
      shell.activeModule = "library";
      selection.selectedIds = new Set([10, 30]);
      expect(flow.currentItems.map((i) => i.version_id)).toEqual([10, 30]);
    });
  });
});
