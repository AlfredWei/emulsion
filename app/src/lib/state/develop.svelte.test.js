import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { flushSync } from "svelte";

const api = vi.hoisted(() => ({ setEditStack: vi.fn(), previewEditStack: vi.fn() }));
vi.mock("$lib/api/develop.js", async (importOriginal) => ({ ...(await importOriginal()), setEditStack: api.setEditStack, previewEditStack: api.previewEditStack }));
vi.mock("@tauri-apps/api/core", () => ({ convertFileSrc: (/** @type {string} */ p) => `asset://${p}` }));

import { DevelopStore } from "./develop.svelte.js";
import { createLibraryStore } from "./library.svelte.js";

const entry = (/** @type {number} */ id) => /** @type {any} */ ({ id, label: `e${id}` });
const stackWith = (/** @type {number} */ exposure) => ({ schema_version: 1, ops: [{ op: "exposure", value: exposure }] });

/** A promise whose resolution the test controls. */
function deferred() {
  /** @type {(v?: any) => void} */
  let resolve = () => {};
  const promise = new Promise((r) => (resolve = r));
  return { promise, resolve };
}

function setup() {
  const library = createLibraryStore();
  const develop = new DevelopStore(library);
  develop.versionId = 10;
  develop.imagePath = "/p/a.raw";
  develop.editStack = /** @type {any} */ (stackWith(1));
  return { library, develop };
}

beforeEach(() => {
  vi.clearAllMocks();
  api.setEditStack.mockResolvedValue([]);
});

describe("DevelopStore state", () => {
  it("starts with no image open", () => {
    const d = new DevelopStore(createLibraryStore());
    expect(d.versionId).toBeNull();
    expect(d.imagePath).toBe("");
    expect(d.editStack).toEqual({ schema_version: 1, ops: [] });
    expect([d.history, d.snapshots, d.historyIndex]).toEqual([[], [], -1]);
    expect([d.previewUrl, d.copiedSettings, d.histogramData, d.hoverPixel, d.cropAspectLock, d.highlightedHslBand]).toEqual([null, null, null, null, null, null]);
    expect([d.showClippingOverlay, d.showOriginal, d.spacePanning, d.gpuFallbackActive, d.cpuFallbackPreviewUrl]).toEqual([false, false, false, false, null]);
    expect([d.sourceWidth, d.sourceHeight]).toEqual([0, 0]);
    expect(d.hasPendingEdit).toBe(false);
    expect(d.hasPendingWork).toBe(false);
  });

  it("imageContentHash follows the open version in the library", () => {
    const { library, develop } = setup();
    library.images = [/** @type {any} */ ({ version_id: 10, content_hash: "abc" }), /** @type {any} */ ({ version_id: 20, content_hash: "def" })];
    expect(develop.imageContentHash).toBe("abc");
    develop.versionId = 20;
    expect(develop.imageContentHash).toBe("def");
    develop.versionId = 99;
    expect(develop.imageContentHash).toBeNull();
    develop.versionId = null;
    expect(develop.imageContentHash).toBeNull();
  });

  it("undo is possible past the first history entry only; redo until the newest", () => {
    const { develop } = setup();
    develop.history = [entry(1), entry(2), entry(3)];
    develop.historyIndex = 0;
    expect([develop.canUndo, develop.canRedo]).toEqual([false, true]);
    develop.historyIndex = 1;
    expect([develop.canUndo, develop.canRedo]).toEqual([true, true]);
    develop.historyIndex = 2;
    expect([develop.canUndo, develop.canRedo]).toEqual([true, false]);
    develop.history = [];
    develop.historyIndex = -1;
    expect([develop.canUndo, develop.canRedo]).toEqual([false, false]);
  });
});

describe("flushEditStack", () => {
  it("with no image open writes nothing and resolves", async () => {
    const d = new DevelopStore(createLibraryStore());
    await expect(d.flushEditStack("Crop")).resolves.toBeUndefined();
    expect(api.setEditStack).not.toHaveBeenCalled();
    expect(d.hasPendingEdit).toBe(false);
  });

  it("writes the open version's stack under the given label and adopts the fresh history, moving the cursor to the newest", async () => {
    const { develop } = setup();
    api.setEditStack.mockResolvedValue([entry(1), entry(2)]);
    await develop.flushEditStack("Exposure");
    expect(api.setEditStack).toHaveBeenCalledWith(10, develop.editStack, "Exposure");
    expect(develop.history).toEqual([entry(1), entry(2)]);
    expect(develop.historyIndex).toBe(1);
  });

  it("an unlabeled flush persists the stack but must not move the undo cursor", async () => {
    const { develop } = setup();
    develop.historyIndex = 0;
    api.setEditStack.mockResolvedValue([entry(1), entry(2), entry(3)]);
    await develop.flushEditStack();
    expect(api.setEditStack).toHaveBeenCalledWith(10, develop.editStack, undefined);
    expect(develop.history).toHaveLength(3);
    expect(develop.historyIndex).toBe(0);
  });

  it("I4: takes the version id and stack at call time, even if the open image changes before the write settles", async () => {
    const { develop } = setup();
    const write = deferred();
    api.setEditStack.mockReturnValue(write.promise);
    const stackAtCall = develop.editStack;
    const done = develop.flushEditStack("Exposure");
    develop.versionId = 20;
    develop.editStack = /** @type {any} */ (stackWith(5));
    write.resolve([entry(1)]);
    await done;
    expect(api.setEditStack).toHaveBeenCalledTimes(1);
    expect(api.setEditStack.mock.calls[0][0]).toBe(10);
    expect(api.setEditStack.mock.calls[0][1]).toBe(stackAtCall);
  });

  it("returns the real write promise: resolves only after the history is adopted", async () => {
    const { develop } = setup();
    const write = deferred();
    api.setEditStack.mockReturnValue(write.promise);
    let settled = false;
    const done = develop.flushEditStack("X").then(() => (settled = true));
    await Promise.resolve();
    expect(settled).toBe(false);
    write.resolve([entry(7)]);
    await done;
    expect(develop.history).toEqual([entry(7)]);
  });

  it("is 'pending' while the write is in flight and not after", async () => {
    const { develop } = setup();
    const write = deferred();
    api.setEditStack.mockReturnValue(write.promise);
    const done = develop.flushEditStack("X");
    expect(develop.hasPendingEdit).toBe(true);
    write.resolve([]);
    await done;
    expect(develop.hasPendingEdit).toBe(false);
  });
});

describe("scheduleFlush (debounced, labeled)", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("writes once, 250 ms after the last call, under the last label", async () => {
    const { develop } = setup();
    develop.scheduleFlush("Exposure");
    await vi.advanceTimersByTimeAsync(200);
    develop.scheduleFlush("Contrast"); // resets the timer, replaces the label
    await vi.advanceTimersByTimeAsync(249);
    expect(api.setEditStack).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(1);
    expect(api.setEditStack).toHaveBeenCalledTimes(1);
    expect(api.setEditStack).toHaveBeenCalledWith(10, develop.editStack, "Contrast");
  });

  it("is pending until it fires", async () => {
    const { develop } = setup();
    develop.scheduleFlush("Exposure");
    expect(develop.hasPendingEdit).toBe(true);
    expect(develop.hasPendingWork).toBe(true);
    await vi.advanceTimersByTimeAsync(250);
    expect(develop.hasPendingEdit).toBe(false);
  });

  it("an early explicit flush uses the scheduled label, consumes it, and cancels the timer (no second write)", async () => {
    const { develop } = setup();
    develop.scheduleFlush("Exposure");
    await develop.flushEditStack();
    expect(api.setEditStack).toHaveBeenLastCalledWith(10, develop.editStack, "Exposure");
    await vi.advanceTimersByTimeAsync(1000);
    expect(api.setEditStack).toHaveBeenCalledTimes(1);
    await develop.flushEditStack(); // the label was consumed
    expect(api.setEditStack).toHaveBeenLastCalledWith(10, develop.editStack, undefined);
  });

  it("an explicit label wins over the scheduled one, which is still consumed", async () => {
    const { develop } = setup();
    develop.scheduleFlush("Exposure");
    await develop.flushEditStack("Reset");
    expect(api.setEditStack).toHaveBeenLastCalledWith(10, develop.editStack, "Reset");
    await develop.flushEditStack();
    expect(api.setEditStack).toHaveBeenLastCalledWith(10, develop.editStack, undefined);
  });

  it("cancelScheduledFlush stops the write but leaves the label for a later flush; discardPendingLabel drops it", async () => {
    const { develop } = setup();
    develop.scheduleFlush("Exposure");
    develop.cancelScheduledFlush();
    await vi.advanceTimersByTimeAsync(1000);
    expect(api.setEditStack).not.toHaveBeenCalled();
    expect(develop.hasPendingEdit).toBe(false);
    await develop.flushEditStack();
    expect(api.setEditStack).toHaveBeenLastCalledWith(10, develop.editStack, "Exposure");

    develop.scheduleFlush("Contrast");
    develop.cancelScheduledFlush();
    develop.discardPendingLabel();
    await develop.flushEditStack();
    expect(api.setEditStack).toHaveBeenLastCalledWith(10, develop.editStack, undefined);
  });

  it("cancelScheduledFlush with nothing scheduled is harmless", () => {
    const { develop } = setup();
    expect(() => develop.cancelScheduledFlush()).not.toThrow();
  });
});

describe("IPTC saves and the window-close flush", () => {
  it("trackIptcSave makes work pending until the write settles, without counting as an edit save", async () => {
    const { develop } = setup();
    const write = deferred();
    develop.trackIptcSave(/** @type {any} */ (write.promise));
    expect(develop.hasPendingWork).toBe(true);
    expect(develop.hasPendingEdit).toBe(false);
    write.resolve();
    await vi.waitFor(() => expect(develop.hasPendingWork).toBe(false));
  });

  it("flushPending flushes the edit and waits for both it and the in-flight IPTC write", async () => {
    const { develop } = setup();
    const edit = deferred();
    const iptc = deferred();
    api.setEditStack.mockReturnValue(edit.promise);
    develop.trackIptcSave(/** @type {any} */ (iptc.promise));
    let settled = false;
    const done = develop.flushPending().then(() => (settled = true));
    expect(api.setEditStack).toHaveBeenCalledTimes(1);
    edit.resolve([]);
    await new Promise((r) => setTimeout(r, 0)); // let every microtask run
    expect(settled).toBe(false); // IPTC still in flight
    iptc.resolve();
    await done;
    expect(develop.hasPendingWork).toBe(false);
  });

  it("flushPending with nothing in flight resolves after a single (unlabeled) write", async () => {
    const { develop } = setup();
    await develop.flushPending();
    expect(api.setEditStack).toHaveBeenCalledTimes(1);
    expect(api.setEditStack).toHaveBeenCalledWith(10, develop.editStack, undefined);
  });
});

describe("hover preview", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  const info = (/** @type {string} */ path) => /** @type {any} */ ({ path });

  it("fetches after a 120 ms debounce and exposes the result as an asset URL", async () => {
    const { develop } = setup();
    const fetch = vi.fn().mockResolvedValue(info("/cache/a.jpg"));
    develop.schedulePreview(fetch);
    await vi.advanceTimersByTimeAsync(119);
    expect(fetch).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(1);
    expect(fetch).toHaveBeenCalledTimes(1);
    expect(develop.previewUrl).toBe("asset:///cache/a.jpg");
  });

  it("scanning across rows fetches only the last one", async () => {
    const { develop } = setup();
    const a = vi.fn().mockResolvedValue(info("/a.jpg"));
    const b = vi.fn().mockResolvedValue(info("/b.jpg"));
    develop.schedulePreview(a);
    await vi.advanceTimersByTimeAsync(60);
    develop.schedulePreview(b);
    await vi.advanceTimersByTimeAsync(200);
    expect(a).not.toHaveBeenCalled();
    expect(develop.previewUrl).toBe("asset:///b.jpg");
  });

  it("a slow earlier render that resolves after a newer request is discarded", async () => {
    const { develop } = setup();
    const slow = deferred();
    develop.schedulePreview(() => /** @type {any} */ (slow.promise));
    await vi.advanceTimersByTimeAsync(120); // slow is now in flight
    develop.schedulePreview(() => Promise.resolve(info("/new.jpg")));
    await vi.advanceTimersByTimeAsync(120);
    expect(develop.previewUrl).toBe("asset:///new.jpg");
    slow.resolve(info("/stale.jpg"));
    await Promise.resolve();
    await Promise.resolve();
    expect(develop.previewUrl).toBe("asset:///new.jpg");
  });

  it("clearPreview drops the URL, cancels a pending fetch, and invalidates one in flight", async () => {
    const { develop } = setup();
    develop.previewUrl = "asset:///old.jpg";
    const pending = vi.fn().mockResolvedValue(info("/p.jpg"));
    develop.schedulePreview(pending);
    develop.clearPreview();
    expect(develop.previewUrl).toBeNull();
    await vi.advanceTimersByTimeAsync(500);
    expect(pending).not.toHaveBeenCalled();

    const slow = deferred();
    develop.schedulePreview(() => /** @type {any} */ (slow.promise));
    await vi.advanceTimersByTimeAsync(120);
    develop.clearPreview();
    slow.resolve(info("/late.jpg"));
    await Promise.resolve();
    await Promise.resolve();
    expect(develop.previewUrl).toBeNull();
  });

  it("does nothing when no image path is set, and swallows a failed render", async () => {
    const { develop } = setup();
    develop.imagePath = /** @type {any} */ (null);
    const fetch = vi.fn().mockResolvedValue(info("/x.jpg"));
    develop.schedulePreview(fetch);
    await vi.advanceTimersByTimeAsync(500);
    expect(fetch).not.toHaveBeenCalled();

    develop.imagePath = "/p/a.raw";
    develop.schedulePreview(() => Promise.reject(new Error("decode")));
    await vi.advanceTimersByTimeAsync(500);
    expect(develop.previewUrl).toBeNull();
  });
});

describe("DevelopStore.installCpuFallback", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    api.previewEditStack.mockResolvedValue({ path: "/tmp/cpu.png" });
  });
  afterEach(() => vi.useRealTimers());

  const installed = (/** @type {DevelopStore} */ develop) => {
    const stop = $effect.root(() => develop.installCpuFallback());
    flushSync();
    return stop;
  };

  it("does nothing while the GPU works, and clears a stale fallback preview", async () => {
    const { develop } = setup();
    develop.cpuFallbackPreviewUrl = "stale";
    const stop = installed(develop);
    expect(develop.cpuFallbackPreviewUrl).toBeNull();
    await vi.advanceTimersByTimeAsync(1000);
    expect(api.previewEditStack).not.toHaveBeenCalled();
    stop();
  });

  it("with the GPU unavailable, renders the full current stack CPU-side 250 ms after the last change", async () => {
    const { develop } = setup();
    develop.gpuFallbackActive = true;
    const stop = installed(develop);
    await vi.advanceTimersByTimeAsync(249);
    expect(api.previewEditStack).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(1);
    expect(api.previewEditStack).toHaveBeenCalledTimes(1);
    expect(api.previewEditStack).toHaveBeenCalledWith("/p/a.raw", null, develop.editStack);
    expect(develop.cpuFallbackPreviewUrl).toBe("asset:///tmp/cpu.png");
    stop();
  });

  it("passes the open image's content hash (smart-preview key) along", async () => {
    const { library, develop } = setup();
    library.images = [/** @type {any} */ ({ version_id: 10, content_hash: "h1" })];
    develop.gpuFallbackActive = true;
    const stop = installed(develop);
    await vi.advanceTimersByTimeAsync(250);
    expect(api.previewEditStack).toHaveBeenCalledWith("/p/a.raw", "h1", develop.editStack);
    stop();
  });

  it("a burst of edits renders once, with the latest stack", async () => {
    const { develop } = setup();
    develop.gpuFallbackActive = true;
    const stop = installed(develop);
    await vi.advanceTimersByTimeAsync(200);
    develop.editStack = /** @type {any} */ (stackWith(2));
    flushSync();
    await vi.advanceTimersByTimeAsync(200);
    develop.editStack = /** @type {any} */ (stackWith(3));
    flushSync();
    await vi.advanceTimersByTimeAsync(250);
    expect(api.previewEditStack).toHaveBeenCalledTimes(1);
    expect(api.previewEditStack.mock.calls[0][2]).toEqual(stackWith(3));
    stop();
  });

  it("a failed render clears the fallback URL", async () => {
    const { develop } = setup();
    develop.gpuFallbackActive = true;
    develop.cpuFallbackPreviewUrl = "stale";
    api.previewEditStack.mockRejectedValue(new Error("decode"));
    const stop = installed(develop);
    await vi.advanceTimersByTimeAsync(250);
    expect(develop.cpuFallbackPreviewUrl).toBeNull();
    stop();
  });

  it("turning the fallback off cancels a pending render and clears the URL; stopping cancels too", async () => {
    const { develop } = setup();
    develop.gpuFallbackActive = true;
    const stop = installed(develop);
    await vi.advanceTimersByTimeAsync(100);
    develop.gpuFallbackActive = false;
    flushSync();
    await vi.advanceTimersByTimeAsync(1000);
    expect(api.previewEditStack).not.toHaveBeenCalled();
    develop.gpuFallbackActive = true;
    flushSync();
    await vi.advanceTimersByTimeAsync(100);
    stop();
    await vi.advanceTimersByTimeAsync(1000);
    expect(api.previewEditStack).not.toHaveBeenCalled();
  });
});
