import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

const api = vi.hoisted(() => ({
  listPresets: vi.fn(),
  createPreset: vi.fn(),
  deletePreset: vi.fn(),
  exportPresetFile: vi.fn(),
  importPresetFile: vi.fn(),
  getEditStack: vi.fn(),
  setEditStack: vi.fn(),
  regenerateThumbnail: vi.fn(),
  previewEditStack: vi.fn(),
}));
vi.mock("$lib/api/develop.js", async (importOriginal) => ({ ...(await importOriginal()), ...api }));
const dialog = vi.hoisted(() => ({ open: vi.fn(), save: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => dialog);
vi.mock("@tauri-apps/api/core", () => ({ convertFileSrc: (/** @type {string} */ p) => p }));
const regen = vi.hoisted(() => vi.fn());
vi.mock("$lib/actions/importActions.js", () => ({ regenerateThumbnailFor: regen }));
const createSnapshot = vi.hoisted(() => vi.fn());
vi.mock("$lib/actions/developActions.js", () => ({ handleCreateSnapshot: createSnapshot }));

import * as A from "./presetActions.js";
import { develop } from "$lib/state/develop.svelte.js";
import { presets } from "$lib/state/presets.svelte.js";
import { selection } from "$lib/state/selection.svelte.js";
import { library } from "$lib/state/library.svelte.js";
import { shell } from "$lib/state/shell.svelte.js";
import { addMask, createBrushMask, upsertCrop } from "$lib/api/develop.js";

const stack = (/** @type {number} */ exposure, /** @type {any[]} */ extra = []) => ({ schema_version: 1, ops: [{ op: "exposure", value: exposure }, ...extra] });
const preset = (/** @type {number} */ id, /** @type {string} */ name, /** @type {any} */ edit_stack) => /** @type {any} */ ({ id, name, edit_stack });
const exposureOf = (/** @type {any} */ s) => s.ops.find((/** @type {any} */ o) => o.op === "exposure")?.value;

beforeEach(() => {
  vi.clearAllMocks();
  presets.list = [];
  presets.creatingPreset = false;
  presets.creatingSnapshot = false;
  presets.confirmingDeletePresetId = null;
  presets.applyingPreset = false;
  presets.pastingSettingsToSelection = false;
  presets.copySettingsDialogOpen = false;
  develop.versionId = 10;
  develop.imagePath = "/p/a.raw";
  develop.editStack = /** @type {any} */ (stack(1));
  develop.copiedSettings = null;
  library.images = [];
  selection.selectedId = null;
  selection.selectedIds = new Set();
  shell.notify("");
  vi.spyOn(develop, "clearPreview").mockImplementation(() => {});
  vi.spyOn(develop, "flushEditStack").mockImplementation(async () => {});
  api.regenerateThumbnail.mockResolvedValue("/t/new.jpg");
  api.setEditStack.mockResolvedValue([]);
});
afterEach(() => vi.restoreAllMocks());

describe("list and dialogs", () => {
  it("refreshPresets replaces the list from the catalog", async () => {
    api.listPresets.mockResolvedValue([preset(1, "A", stack(0))]);
    await A.refreshPresets();
    expect(presets.list.map((p) => p.id)).toEqual([1]);
  });

  it("the request handlers only open their dialog", () => {
    A.handleSaveCurrentAsPresetRequest();
    expect(presets.creatingPreset).toBe(true);
    A.handleDeletePresetRequest(4);
    expect(presets.confirmingDeletePresetId).toBe(4);
  });

  it("snapshot confirm closes its dialog and creates the snapshot under the given name", () => {
    presets.creatingSnapshot = true;
    A.handleCreateSnapshotConfirmed("Before crop");
    expect(presets.creatingSnapshot).toBe(false);
    expect(createSnapshot).toHaveBeenCalledWith("Before crop");
  });
});

describe("create / import / export / delete", () => {
  it("create: closes the dialog and saves the open edit stack WITHOUT its masks, crop, lens or perspective", async () => {
    presets.creatingPreset = true;
    develop.editStack = /** @type {any} */ (addMask(upsertCrop(develop.editStack, { x: 0, y: 0, width: 0.5, height: 0.5 }), /** @type {any} */ (createBrushMask("b1"))));
    api.createPreset.mockImplementation(async (/** @type {string} */ name) => preset(7, name, stack(1)));
    presets.list = [preset(1, "Old", stack(0))];
    await A.handleCreatePresetConfirmed("Warm");
    expect(presets.creatingPreset).toBe(false);
    const sent = api.createPreset.mock.calls[0];
    expect(sent[0]).toBe("Warm");
    expect(sent[1].ops.map((/** @type {any} */ o) => o.op)).toEqual(["exposure"]);
    expect(presets.list.map((p) => p.id)).toEqual([1, 7]);
  });

  it("delete: closes the confirm dialog first, removes it in the catalog, then from the list; no pending id does nothing", async () => {
    await A.handleDeletePresetConfirmed();
    expect(api.deletePreset).not.toHaveBeenCalled();
    presets.list = [preset(1, "A", stack(0)), preset(2, "B", stack(0))];
    presets.confirmingDeletePresetId = 1;
    await A.handleDeletePresetConfirmed();
    expect(presets.confirmingDeletePresetId).toBeNull();
    expect(api.deletePreset).toHaveBeenCalledWith(1);
    expect(presets.list.map((p) => p.id)).toEqual([2]);
  });

  it("delete: the list only changes after the catalog delete has finished", async () => {
    presets.list = [preset(1, "A", stack(0))];
    presets.confirmingDeletePresetId = 1;
    /** @type {() => void} */
    let release = () => {};
    api.deletePreset.mockReturnValue(new Promise((r) => (release = () => r(undefined))));
    const p = A.handleDeletePresetConfirmed();
    await Promise.resolve();
    expect(presets.list).toHaveLength(1);
    release();
    await p;
    expect(presets.list).toHaveLength(0);
  });

  it("export: unknown preset does nothing; a cancelled dialog writes nothing; success and failure are reported", async () => {
    await A.handleExportPreset(9);
    expect(dialog.save).not.toHaveBeenCalled();
    presets.list = [preset(1, "Warm", stack(2))];
    dialog.save.mockResolvedValueOnce(null);
    await A.handleExportPreset(1);
    expect(api.exportPresetFile).not.toHaveBeenCalled();
    dialog.save.mockResolvedValue("/out/Warm.json");
    await A.handleExportPreset(1);
    expect(dialog.save).toHaveBeenLastCalledWith({ defaultPath: "Warm.json", filters: [{ name: "Preset", extensions: ["json"] }] });
    expect(api.exportPresetFile).toHaveBeenCalledWith("Warm", stack(2), "/out/Warm.json");
    expect(shell.statusMessage).toBe('Exported "Warm"');
    api.exportPresetFile.mockRejectedValue("disk full");
    await A.handleExportPreset(1);
    expect(shell.statusMessage).toBe("Export preset failed: disk full");
  });

  it("import: a cancelled picker does nothing", async () => {
    dialog.open.mockResolvedValue(null);
    await A.handleImportPresetRequest();
    expect(api.importPresetFile).not.toHaveBeenCalled();
  });

  it("import: re-filters foreign ops, creates the preset, appends it and reports", async () => {
    dialog.open.mockResolvedValue("/in/x.json");
    api.importPresetFile.mockResolvedValue({ name: "Foreign", schema_version: 1, ops: [...stack(3).ops, { op: "crop", x: 0 }, createBrushMask("b")] });
    api.createPreset.mockImplementation(async (/** @type {string} */ name, /** @type {any} */ s) => preset(5, name, s));
    await A.handleImportPresetRequest();
    expect(api.createPreset.mock.calls[0][0]).toBe("Foreign");
    expect(api.createPreset.mock.calls[0][1].ops.map((/** @type {any} */ o) => o.op)).toEqual(["exposure"]);
    expect(presets.list.map((p) => p.id)).toEqual([5]);
    expect(shell.statusMessage).toBe('Imported "Foreign"');
  });

  it("import: a failing file is reported and leaves the list alone", async () => {
    dialog.open.mockResolvedValue("/in/bad.json");
    api.importPresetFile.mockRejectedValue("bad json");
    await A.handleImportPresetRequest();
    expect(shell.statusMessage).toBe("Import preset failed: bad json");
    expect(presets.list).toEqual([]);
  });
});

describe("handleApplyPreset (open image)", () => {
  beforeEach(() => {
    presets.list = [preset(1, "Punchy", stack(2, [{ op: "contrast", value: 30 }]))];
  });

  it("clears the hover preview, merges the preset over the stack, flushes IMMEDIATELY under its label, then regenerates", async () => {
    /** @type {string[]} */
    const order = [];
    /** @type {any} */ (develop.flushEditStack).mockImplementation(async (/** @type {string} */ l) => {
      await new Promise((r) => setTimeout(r, 0));
      order.push(`flush:${l}`);
    });
    regen.mockImplementation(() => void order.push("regen"));
    await A.handleApplyPreset(1);
    expect(develop.clearPreview).toHaveBeenCalled();
    expect(exposureOf(develop.editStack)).toBe(2);
    expect(develop.editStack.ops.map((o) => o.op).sort()).toEqual(["contrast", "exposure"]);
    expect(order).toEqual(["flush:Apply Preset: Punchy", "regen"]);
    expect(regen).toHaveBeenCalledWith(10);
  });

  it("does nothing for an unknown preset or with no open image (preview is still cleared)", async () => {
    await A.handleApplyPreset(99);
    expect(exposureOf(develop.editStack)).toBe(1);
    develop.versionId = null;
    await A.handleApplyPreset(1);
    expect(develop.flushEditStack).not.toHaveBeenCalled();
    expect(develop.clearPreview).toHaveBeenCalledTimes(2);
  });

  it("regenerates the version that was open when applying, not one opened during the flush", async () => {
    /** @type {any} */ (develop.flushEditStack).mockImplementation(async () => {
      develop.versionId = 55;
    });
    await A.handleApplyPreset(1);
    expect(regen).toHaveBeenCalledWith(10);
  });
});

describe("handleApplyPresetToSelection (Library batch)", () => {
  beforeEach(() => {
    presets.list = [preset(1, "Punchy", stack(5))];
    library.images = [10, 20, 30].map((v) => /** @type {any} */ ({ image_id: v / 10, version_id: v, thumbnail_path: null }));
    selection.selectedIds = new Set([10, 20]);
    api.getEditStack.mockImplementation(async () => stack(0, [{ op: "contrast", value: 4 }]));
  });

  it("ignores an empty value, an unknown preset and an empty selection", async () => {
    await A.handleApplyPresetToSelection("");
    await A.handleApplyPresetToSelection("99");
    selection.selectedIds = new Set();
    await A.handleApplyPresetToSelection("1");
    expect(api.setEditStack).not.toHaveBeenCalled();
    expect(presets.applyingPreset).toBe(false);
    expect(shell.statusMessage).toBe(""); // no "Applied … to 0 photos"
  });

  it("merges into each target's own stack (version-targeted), writes it labelled, patches the thumbnail, reports", async () => {
    await A.handleApplyPresetToSelection("1");
    expect(api.setEditStack.mock.calls.map((c) => c[0]).sort()).toEqual([10, 20]);
    for (const c of api.setEditStack.mock.calls) {
      expect(exposureOf(c[1])).toBe(5);
      expect(c[1].ops.map((/** @type {any} */ o) => o.op).sort()).toEqual(["contrast", "exposure"]);
      expect(c[2]).toBe("Apply Preset: Punchy");
    }
    expect(library.images.filter((i) => i.thumbnail_path === "/t/new.jpg").map((i) => i.version_id)).toEqual([10, 20]);
    expect(shell.statusMessage).toBe('Applied "Punchy" to 2 photos');
    expect(presets.applyingPreset).toBe(false);
  });

  it("singular wording for one photo", async () => {
    selection.selectedIds = new Set([30]);
    await A.handleApplyPresetToSelection("1");
    expect(shell.statusMessage).toBe('Applied "Punchy" to 1 photo');
  });

  it("holds the in-flight flag while working, and clears it on failure with a message", async () => {
    /** @type {(v: any) => void} */
    let release = () => {};
    api.getEditStack.mockReturnValueOnce(new Promise((r) => (release = r)));
    const p = A.handleApplyPresetToSelection("1");
    expect(presets.applyingPreset).toBe(true);
    release(stack(0));
    await p;
    api.getEditStack.mockRejectedValue("gone");
    await A.handleApplyPresetToSelection("1");
    expect(presets.applyingPreset).toBe(false);
    expect(shell.statusMessage).toBe("Apply preset failed: gone");
  });

  it("re-syncs the open Develop stack when it was one of the targets (so a later flush cannot clobber it)", async () => {
    develop.versionId = 20;
    develop.editStack = /** @type {any} */ (stack(1));
    api.getEditStack.mockImplementation(async (/** @type {number} */ v) => (v === 20 && api.setEditStack.mock.calls.length >= 2 ? stack(5) : stack(0)));
    await A.handleApplyPresetToSelection("1");
    expect(exposureOf(develop.editStack)).toBe(5);
  });

  it("leaves the open Develop stack alone when it was not a target", async () => {
    develop.versionId = 30;
    const before = develop.editStack;
    await A.handleApplyPresetToSelection("1");
    expect(develop.editStack).toBe(before);
  });
});

describe("copy / paste settings", () => {
  it("copy request needs an open image", () => {
    develop.versionId = null;
    A.handleCopySettingsRequest();
    expect(presets.copySettingsDialogOpen).toBe(false);
    develop.versionId = 10;
    A.handleCopySettingsRequest();
    expect(presets.copySettingsDialogOpen).toBe(true);
  });

  it("copy confirm closes the dialog and keeps only the chosen groups' ops (never masks/crop)", () => {
    develop.editStack = /** @type {any} */ (addMask(stack(1, [{ op: "contrast", value: 2 }, { op: "temperature", value: 3 }]), /** @type {any} */ (createBrushMask("b"))));
    presets.copySettingsDialogOpen = true;
    A.handleCopySettingsConfirmed(["basic_tone"]);
    expect(presets.copySettingsDialogOpen).toBe(false);
    expect(develop.copiedSettings?.ops.map((o) => o.op).sort()).toEqual(["contrast", "exposure"]);
    expect(shell.statusMessage).toBe("Copied settings");
  });

  it("paste: nothing copied, or no open image, does nothing", async () => {
    await A.handlePasteSettings();
    develop.copiedSettings = /** @type {any} */ (stack(4));
    develop.versionId = null;
    await A.handlePasteSettings();
    expect(develop.flushEditStack).not.toHaveBeenCalled();
    expect(shell.statusMessage).toBe("");
  });

  it("paste: merges over the open stack, flushes immediately as Paste Settings, regenerates AFTER the flush, reports", async () => {
    develop.copiedSettings = /** @type {any} */ (stack(4));
    /** @type {string[]} */
    const order = [];
    /** @type {any} */ (develop.flushEditStack).mockImplementation(async () => {
      await new Promise((r) => setTimeout(r, 0));
      order.push("flush");
    });
    regen.mockImplementation(() => void order.push("regen"));
    await A.handlePasteSettings();
    expect(order).toEqual(["flush", "regen"]);
    expect(exposureOf(develop.editStack)).toBe(4);
    expect(develop.flushEditStack).toHaveBeenCalledWith("Paste Settings");
    expect(regen).toHaveBeenCalledWith(10);
    expect(shell.statusMessage).toBe("Pasted settings");
  });

  describe("paste to selection", () => {
    beforeEach(() => {
      develop.copiedSettings = /** @type {any} */ (stack(4));
      library.images = [10, 20].map((v) => /** @type {any} */ ({ image_id: v / 10, version_id: v, thumbnail_path: null }));
      selection.selectedIds = new Set([10, 20]);
      api.getEditStack.mockImplementation(async () => stack(0, [{ op: "contrast", value: 1 }]));
    });

    it("does nothing without copied settings or without a selection", async () => {
      selection.selectedIds = new Set();
      await A.handlePasteSettingsToSelection();
      expect(shell.statusMessage).toBe("");
      develop.copiedSettings = null;
      selection.selectedIds = new Set([10]);
      await A.handlePasteSettingsToSelection();
      expect(api.setEditStack).not.toHaveBeenCalled();
      expect(shell.statusMessage).toBe(""); // neither a success nor a failure report
    });

    it("merges the copied settings into each target, labels the write, patches thumbnails, reports, and clears the flag", async () => {
      await A.handlePasteSettingsToSelection();
      expect(api.setEditStack.mock.calls.map((c) => c[0]).sort()).toEqual([10, 20]);
      for (const c of api.setEditStack.mock.calls) {
        expect(exposureOf(c[1])).toBe(4);
        expect(c[2]).toBe("Paste Settings");
      }
      expect(library.images.every((i) => i.thumbnail_path === "/t/new.jpg")).toBe(true);
      expect(shell.statusMessage).toBe("Pasted settings to 2 photos");
      expect(presets.pastingSettingsToSelection).toBe(false);
    });

    it("sets its own in-flight flag while working and reports failure", async () => {
      /** @type {(v: any) => void} */
      let release = () => {};
      api.getEditStack.mockReturnValueOnce(new Promise((r) => (release = r)));
      const p = A.handlePasteSettingsToSelection();
      expect(presets.pastingSettingsToSelection).toBe(true);
      release(stack(0));
      await p;
      api.getEditStack.mockRejectedValue("nope");
      await A.handlePasteSettingsToSelection();
      expect(shell.statusMessage).toBe("Paste settings failed: nope");
      expect(presets.pastingSettingsToSelection).toBe(false);
    });

    it("leaves the open Develop stack alone when it was not a target", async () => {
      develop.versionId = 30;
      const before = develop.editStack;
      await A.handlePasteSettingsToSelection();
      expect(develop.editStack).toBe(before);
    });

    it("re-syncs the open Develop stack when it was a target", async () => {
      develop.versionId = 10;
      api.getEditStack.mockImplementation(async () => (api.setEditStack.mock.calls.length >= 2 ? stack(4) : stack(0)));
      await A.handlePasteSettingsToSelection();
      expect(exposureOf(develop.editStack)).toBe(4);
    });
  });
});

describe("handlePeekPreset", () => {
  beforeEach(() => {
    presets.list = [preset(1, "Punchy", stack(2))];
    library.images = [/** @type {any} */ ({ version_id: 10, content_hash: "hash" })];
    vi.spyOn(develop, "schedulePreview").mockImplementation((/** @type {() => unknown} */ f) => void f());
  });

  it("previews the open stack with the preset merged in, keyed by path and content hash", () => {
    A.handlePeekPreset(1);
    expect(api.previewEditStack).toHaveBeenCalledTimes(1);
    const [path, hash, merged] = api.previewEditStack.mock.calls[0];
    expect([path, hash]).toEqual(["/p/a.raw", "hash"]);
    expect(exposureOf(merged)).toBe(2);
    expect(exposureOf(develop.editStack)).toBe(1); // the live stack is untouched
  });

  it("ignores unknown presets", () => {
    A.handlePeekPreset(99);
    expect(api.previewEditStack).not.toHaveBeenCalled();
  });
});
