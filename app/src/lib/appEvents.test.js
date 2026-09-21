import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

const tauri = vi.hoisted(() => ({
  listen: vi.fn(),
  getCurrentWindow: vi.fn(),
  getCurrentWebview: vi.fn(),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: tauri.listen }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: tauri.getCurrentWindow }));
vi.mock("@tauri-apps/api/webview", () => ({ getCurrentWebview: tauri.getCurrentWebview }));
const catalog = vi.hoisted(() => ({ listAllImageKeywords: vi.fn() }));
vi.mock("$lib/api/catalog.js", () => catalog);
const backup = vi.hoisted(() => ({ getBackupSettings: vi.fn(), isBackupDue: vi.fn() }));
vi.mock("$lib/api/backup.js", () => backup);
const queue = vi.hoisted(() => ({ flushThumbnailBatch: vi.fn() }));
vi.mock("$lib/thumbnailBatchQueue.js", () => queue);
const lib = vi.hoisted(() => ({ refresh: vi.fn(), refreshCollections: vi.fn(), handleBatchThumbnailsComplete: vi.fn() }));
vi.mock("$lib/actions/libraryActions.js", () => lib);
const imp = vi.hoisted(() => ({ handleDropImport: vi.fn(), regenerateThumbnailFor: vi.fn(), pollUntilThumbnailsReadyOnStartup: vi.fn() }));
vi.mock("$lib/actions/importActions.js", () => imp);
const pre = vi.hoisted(() => ({ refreshPresets: vi.fn() }));
vi.mock("$lib/actions/presetActions.js", () => pre);

import { installAppEvents } from "./appEvents.js";
import { shell } from "$lib/state/shell.svelte.js";
import { faces } from "$lib/state/faces.svelte.js";
import { importFlow } from "$lib/state/importFlow.svelte.js";
import { library } from "$lib/state/library.svelte.js";
import { develop } from "$lib/state/develop.svelte.js";

const tick = () => new Promise((r) => setTimeout(r, 0));

/** @type {Record<string, (e: any) => void>} */
let listeners;
/** @type {Record<string, ReturnType<typeof vi.fn>>} */
let unlisten;
/** @type {(e: any) => Promise<void>} */
let onClose;
/** @type {(e: any) => void} */
let onDragDrop;
/** @type {Record<string, (e: any) => void>} */
let windowListeners;
let destroy = vi.fn();
let handleMenuAction = vi.fn();
let cleanup = () => {};

const settings = /** @type {any} */ ({ folder: "/b" });
/** @param {Partial<{ pendingWork: boolean, pendingEdit: boolean }>} p */
function pending(p) {
  vi.spyOn(develop, "hasPendingWork", "get").mockReturnValue(!!p.pendingWork);
  vi.spyOn(develop, "hasPendingEdit", "get").mockReturnValue(!!p.pendingEdit);
}
const closeEvent = () => ({ preventDefault: vi.fn() });

beforeEach(async () => {
  vi.clearAllMocks();
  listeners = {};
  unlisten = {};
  windowListeners = {};
  destroy = vi.fn().mockResolvedValue(undefined);
  handleMenuAction = vi.fn();
  vi.stubGlobal("window", {
    addEventListener: (/** @type {string} */ n, /** @type {any} */ f) => (windowListeners[n] = f),
    removeEventListener: vi.fn((/** @type {string} */ n) => delete windowListeners[n]),
  });
  vi.stubGlobal("document", { activeElement: { blur: vi.fn() } });
  tauri.listen.mockImplementation(async (/** @type {string} */ name, /** @type {any} */ cb) => {
    listeners[name] = cb;
    return (unlisten[name] = vi.fn());
  });
  tauri.getCurrentWindow.mockReturnValue({
    onCloseRequested: vi.fn(async (/** @type {any} */ cb) => {
      onClose = cb;
      return (unlisten.close = vi.fn());
    }),
    destroy,
  });
  tauri.getCurrentWebview.mockReturnValue({
    onDragDropEvent: vi.fn(async (/** @type {any} */ cb) => {
      onDragDrop = cb;
      return (unlisten.dragDrop = vi.fn());
    }),
  });
  lib.refresh.mockResolvedValue(undefined);
  catalog.listAllImageKeywords.mockResolvedValue([{ image_id: 1, keyword_id: 2 }]);
  backup.getBackupSettings.mockResolvedValue(settings);
  backup.isBackupDue.mockReturnValue(false);
  shell.activeModule = "library";
  shell.settingsOpen = false;
  shell.shortcuts = {};
  importFlow.isDraggingFiles = false;
  library.allImageKeywords = [];
  develop.versionId = 10;
  vi.spyOn(develop, "flushPending").mockResolvedValue(/** @type {any} */ ([]));
  vi.spyOn(importFlow, "showBackupPromptAndWait").mockResolvedValue(undefined);
  pending({});
  cleanup = installAppEvents({ handleMenuAction });
  await tick();
});
afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe("startup", () => {
  it("refreshes the library, then polls for thumbnails; also loads collections, presets and keyword assignments", () => {
    expect(lib.refresh).toHaveBeenCalledTimes(1);
    expect(imp.pollUntilThumbnailsReadyOnStartup).toHaveBeenCalledTimes(1);
    expect(lib.refreshCollections).toHaveBeenCalledTimes(1);
    expect(pre.refreshPresets).toHaveBeenCalledTimes(1);
    expect(library.allImageKeywords).toEqual([{ image_id: 1, keyword_id: 2 }]);
  });

  it("the poll starts only after the refresh has resolved", async () => {
    vi.clearAllMocks();
    /** @type {() => void} */
    let release = () => {};
    lib.refresh.mockReturnValue(new Promise((r) => (release = () => r(undefined))));
    installAppEvents({ handleMenuAction });
    await tick();
    expect(imp.pollUntilThumbnailsReadyOnStartup).not.toHaveBeenCalled();
    release();
    await tick();
    expect(imp.pollUntilThumbnailsReadyOnStartup).toHaveBeenCalledTimes(1);
  });
});

describe("window close", () => {
  it("always closes Settings and blurs the focused field first (so a typed IPTC value saves)", async () => {
    shell.settingsOpen = true;
    await onClose(closeEvent());
    expect(shell.settingsOpen).toBe(false);
    expect(/** @type {any} */ (document.activeElement).blur).toHaveBeenCalledTimes(1);
  });

  it("nothing pending and no backup due: lets the close proceed (no preventDefault, no flush, no destroy)", async () => {
    const e = closeEvent();
    await onClose(e);
    expect(e.preventDefault).not.toHaveBeenCalled();
    expect(develop.flushPending).not.toHaveBeenCalled();
    expect(destroy).not.toHaveBeenCalled();
  });

  it("an unreadable backup-settings fetch counts as not due and never blocks a clean quit", async () => {
    backup.getBackupSettings.mockRejectedValue(new Error("db"));
    const e = closeEvent();
    await onClose(e);
    expect(e.preventDefault).not.toHaveBeenCalled();
    expect(backup.isBackupDue).not.toHaveBeenCalled();
  });

  it("a pending edit: prevents the close FIRST, flushes, regenerates the open image's thumbnail (not awaited), flushes the batch queue, then destroys", async () => {
    pending({ pendingWork: true, pendingEdit: true });
    /** @type {string[]} */
    const order = [];
    const e = { preventDefault: vi.fn(() => void order.push("prevent")) };
    /** @type {any} */ (develop.flushPending).mockImplementation(async () => {
      await tick();
      order.push("flush");
    });
    imp.regenerateThumbnailFor.mockImplementation(() => void order.push("regen"));
    queue.flushThumbnailBatch.mockImplementation(() => void order.push("batch"));
    destroy.mockImplementation(async () => void order.push("destroy"));
    await onClose(e);
    expect(order).toEqual(["prevent", "flush", "regen", "batch", "destroy"]);
    expect(imp.regenerateThumbnailFor).toHaveBeenCalledWith(10);
    expect(queue.flushThumbnailBatch).toHaveBeenCalledWith(lib.handleBatchThumbnailsComplete);
  });

  it("the close handler does not finish until the window has actually been destroyed", async () => {
    pending({ pendingWork: true, pendingEdit: true });
    /** @type {() => void} */
    let done = () => {};
    destroy.mockReturnValue(new Promise((r) => (done = () => r(undefined))));
    let finished = false;
    const p = onClose(closeEvent()).then(() => (finished = true));
    await tick();
    expect(destroy).toHaveBeenCalled();
    expect(finished).toBe(false);
    done();
    await p;
    expect(finished).toBe(true);
  });

  it("only an in-flight IPTC save (no edit pending): flushes and waits, but regenerates nothing", async () => {
    pending({ pendingWork: true, pendingEdit: false });
    const e = closeEvent();
    await onClose(e);
    expect(e.preventDefault).toHaveBeenCalled();
    expect(develop.flushPending).toHaveBeenCalled();
    expect(imp.regenerateThumbnailFor).not.toHaveBeenCalled();
    expect(queue.flushThumbnailBatch).not.toHaveBeenCalled();
    expect(destroy).toHaveBeenCalled();
  });

  it("whether an edit was pending is read BEFORE the flush (the flush itself clears it)", async () => {
    let flushed = false;
    vi.spyOn(develop, "hasPendingWork", "get").mockReturnValue(true);
    vi.spyOn(develop, "hasPendingEdit", "get").mockImplementation(() => !flushed);
    /** @type {any} */ (develop.flushPending).mockImplementation(async () => {
      flushed = true;
    });
    await onClose(closeEvent());
    expect(imp.regenerateThumbnailFor).toHaveBeenCalledWith(10);
  });

  it("backup due with nothing pending: prevents the close, shows the prompt with the fetched settings, waits for it, then destroys", async () => {
    backup.isBackupDue.mockReturnValue(true);
    /** @type {string[]} */
    const order = [];
    /** @type {() => void} */
    let answer = () => {};
    /** @type {any} */ (importFlow.showBackupPromptAndWait).mockImplementation(
      () =>
        new Promise((r) => {
          order.push("prompt");
          answer = () => r(undefined);
        }),
    );
    destroy.mockImplementation(async () => void order.push("destroy"));
    const e = closeEvent();
    const p = onClose(e);
    await tick();
    expect(e.preventDefault).toHaveBeenCalled();
    expect(importFlow.showBackupPromptAndWait).toHaveBeenCalledWith(settings);
    expect(destroy).not.toHaveBeenCalled();
    answer();
    await p;
    expect(order).toEqual(["prompt", "destroy"]);
    expect(imp.regenerateThumbnailFor).not.toHaveBeenCalled();
  });

  it("no backup prompt when none is due, even if an edit was pending", async () => {
    pending({ pendingWork: true, pendingEdit: true });
    await onClose(closeEvent());
    expect(importFlow.showBackupPromptAndWait).not.toHaveBeenCalled();
  });

  it("the pending edit is flushed BEFORE the backup prompt appears", async () => {
    pending({ pendingWork: true, pendingEdit: true });
    backup.isBackupDue.mockReturnValue(true);
    /** @type {string[]} */
    const order = [];
    /** @type {any} */ (develop.flushPending).mockImplementation(async () => void order.push("flush"));
    /** @type {any} */ (importFlow.showBackupPromptAndWait).mockImplementation(async () => void order.push("prompt"));
    await onClose(closeEvent());
    expect(order).toEqual(["flush", "prompt"]);
  });
});

describe("drag and drop", () => {
  it("enter/over shows the drop overlay only in the Library module; leave hides it", () => {
    onDragDrop({ payload: { type: "enter" } });
    expect(importFlow.isDraggingFiles).toBe(true);
    onDragDrop({ payload: { type: "leave" } });
    expect(importFlow.isDraggingFiles).toBe(false);
    shell.activeModule = "develop";
    onDragDrop({ payload: { type: "over" } });
    expect(importFlow.isDraggingFiles).toBe(false);
    shell.activeModule = "library";
    onDragDrop({ payload: { type: "over" } });
    expect(importFlow.isDraggingFiles).toBe(true);
  });

  it("drop hides the overlay and imports the dropped paths", () => {
    importFlow.isDraggingFiles = true;
    onDragDrop({ payload: { type: "drop", paths: ["/a.jpg", "/b.jpg"] } });
    expect(importFlow.isDraggingFiles).toBe(false);
    expect(imp.handleDropImport).toHaveBeenCalledWith(["/a.jpg", "/b.jpg"]);
  });

  it.each([[[]], [undefined]])("a drop with no paths (%j) imports nothing", (paths) => {
    onDragDrop({ payload: { type: "drop", paths } });
    expect(imp.handleDropImport).not.toHaveBeenCalled();
  });

  it("outside Tauri (getCurrentWebview throws) the rest still installs", async () => {
    tauri.getCurrentWebview.mockImplementation(() => {
      throw new Error("no tauri");
    });
    listeners = {};
    expect(() => installAppEvents({ handleMenuAction })).not.toThrow();
    await tick();
    expect(Object.keys(listeners)).toContain("menu-action");
  });
});

describe("shortcuts and menu", () => {
  it("shortcuts-updated replaces the shortcut map; an event without detail is ignored", () => {
    windowListeners["shortcuts-updated"]({ detail: { rate: "r" } });
    expect(shell.shortcuts).toEqual({ rate: "r" });
    windowListeners["shortcuts-updated"]({});
    expect(shell.shortcuts).toEqual({ rate: "r" });
  });

  it("menu-action events go to the page's menu handler", () => {
    listeners["menu-action"]({ payload: "file.import" });
    expect(handleMenuAction).toHaveBeenCalledWith("file.import");
  });

  it("if listen throws (outside Tauri) installation does not throw", () => {
    tauri.listen.mockImplementation(() => {
      throw new Error("no tauri");
    });
    expect(() => installAppEvents({ handleMenuAction })).not.toThrow();
  });
});

describe("progress streams", () => {
  const fields = () =>
    /** @type {Record<string, any>} */ ({
      "import-progress": importFlow.catalogProgress,
      "thumbnail-progress": importFlow.thumbnailProgress,
      "hdr-merge-progress": importFlow.hdrMergeProgress,
      "face-detection-progress": importFlow.faceDetectionProgress,
      "face-detection-scan-progress": faces.scanProgress,
    });

  beforeEach(() => {
    importFlow.catalogProgress = null;
    importFlow.thumbnailProgress = null;
    importFlow.hdrMergeProgress = null;
    importFlow.faceDetectionProgress = null;
    faces.scanProgress = null;
  });

  it.each(["import-progress", "thumbnail-progress", "hdr-merge-progress", "face-detection-progress", "face-detection-scan-progress"])(
    "%s lands in its own field and no other",
    (name) => {
      listeners[name]({ payload: { current: 3, total: 9 } });
      for (const [n, value] of Object.entries(fields())) {
        expect(value, n).toEqual(n === name ? { current: 3, total: 9 } : null);
      }
    },
  );
});

describe("cleanup", () => {
  it("removes the window listener and calls every registered unlisten", () => {
    cleanup();
    for (const k of ["close", "dragDrop", "menu-action", "import-progress", "thumbnail-progress", "hdr-merge-progress", "face-detection-progress", "face-detection-scan-progress"]) {
      expect(unlisten[k], k).toHaveBeenCalledTimes(1);
    }
    expect(windowListeners["shortcuts-updated"]).toBeUndefined();
  });

  it("is safe to call before the listeners have finished registering", () => {
    tauri.listen.mockImplementation(() => new Promise(() => {}));
    const c = installAppEvents({ handleMenuAction });
    expect(() => c()).not.toThrow();
  });
});
