import { describe, it, expect, vi, beforeEach } from "vitest";

const catalog = vi.hoisted(() => ({
  setRating: vi.fn(),
  setFlag: vi.fn(),
  setColorLabel: vi.fn(),
  setCaption: vi.fn(),
  setCopyright: vi.fn(),
  setContact: vi.fn(),
}));
vi.mock("$lib/api/catalog.js", () => catalog);

import { handleRatingChange, handleFlagChange, handleColorLabelChange, handleCaptionChange, handleCopyrightChange, handleContactChange } from "./metadataActions.js";
import { library } from "$lib/state/library.svelte.js";
import { selection } from "$lib/state/selection.svelte.js";
import { develop } from "$lib/state/develop.svelte.js";

const img = (/** @type {number} */ id, /** @type {number} */ version_id = id * 10) =>
  /** @type {any} */ ({ image_id: id, version_id, rating: 0, flag: "none", color_label: "none", caption: "", copyright: "", contact: "" });

beforeEach(() => {
  vi.clearAllMocks();
  for (const fn of Object.values(catalog)) fn.mockResolvedValue(undefined);
  library.images = [img(1), img(2), img(3)];
  selection.selectedId = null;
  selection.selectedIds = new Set();
});

const byVersion = (/** @type {number} */ v) => library.images.find((i) => i.version_id === v);

describe.each([
  ["rating", handleRatingChange, catalog.setRating, "rating", 4, 0],
  ["flag", handleFlagChange, catalog.setFlag, "flag", "pick", "none"],
  ["color label", handleColorLabelChange, catalog.setColorLabel, "color_label", "red", "none"],
])("%s", (_n, handler, api, field, value, initial) => {
  it("acts on the whole selection when the acted-on cell is part of a multi-selection", async () => {
    selection.selectedIds = new Set([10, 20]);
    await handler(10, /** @type {never} */ (value));
    expect(api.mock.calls.map((c) => c[0]).sort()).toEqual([10, 20]);
    expect(api).toHaveBeenCalledWith(10, value);
    expect(/** @type {any} */ (byVersion(10))[field]).toBe(value);
    expect(/** @type {any} */ (byVersion(20))[field]).toBe(value);
    expect(/** @type {any} */ (byVersion(30))[field]).toBe(initial);
  });

  it("acts only on the cell when it is outside the selection", async () => {
    selection.selectedIds = new Set([10, 20]);
    await handler(30, /** @type {never} */ (value));
    expect(api.mock.calls.map((c) => c[0])).toEqual([30]);
    expect(/** @type {any} */ (byVersion(10))[field]).toBe(initial);
  });

  it("with no cell (keyboard shortcut) acts on the selection, then on the anchor", async () => {
    selection.selectedIds = new Set([20]);
    await handler(undefined, /** @type {never} */ (value));
    expect(api.mock.calls.map((c) => c[0])).toEqual([20]);
    api.mockClear();
    selection.selectedIds = new Set();
    selection.selectedId = 30;
    await handler(null, /** @type {never} */ (value));
    expect(api.mock.calls.map((c) => c[0])).toEqual([30]);
  });

  it("does nothing when there is no target", async () => {
    await handler(undefined, /** @type {never} */ (value));
    expect(api).not.toHaveBeenCalled();
  });

  it("patches the library before the catalog write resolves (optimistic)", async () => {
    /** @type {() => void} */
    let release = () => {};
    api.mockReturnValue(new Promise((r) => (release = () => r(undefined))));
    const p = handler(10, /** @type {never} */ (value));
    expect(/** @type {any} */ (byVersion(10))[field]).toBe(value);
    release();
    await p;
  });
});

describe("IPTC fields", () => {
  it("caption: patches the version locally and hands the write to develop.trackIptcSave", () => {
    const write = Promise.resolve();
    catalog.setCaption.mockReturnValue(write);
    const track = vi.spyOn(develop, "trackIptcSave");
    handleCaptionChange(20, "hello");
    expect(byVersion(20)?.caption).toBe("hello");
    expect(catalog.setCaption).toHaveBeenCalledWith(20, "hello");
    expect(track).toHaveBeenCalledWith(write);
    track.mockRestore();
  });

  it("copyright and contact: patch every version of the image (they are per-photo), and are tracked", () => {
    library.images = [img(1, 10), img(1, 11), img(2, 20)];
    const track = vi.spyOn(develop, "trackIptcSave");
    handleCopyrightChange(1, "(c) me");
    handleContactChange(2, "me@x");
    expect(library.images.map((i) => i.copyright)).toEqual(["(c) me", "(c) me", ""]);
    expect(library.images.map((i) => i.contact)).toEqual(["", "", "me@x"]);
    expect(catalog.setCopyright).toHaveBeenCalledWith(1, "(c) me");
    expect(catalog.setContact).toHaveBeenCalledWith(2, "me@x");
    expect(track).toHaveBeenCalledTimes(2);
    track.mockRestore();
  });

  it("flushPending waits for a tracked IPTC write", async () => {
    /** @type {() => void} */
    let release = () => {};
    catalog.setCaption.mockReturnValue(new Promise((r) => (release = () => r(undefined))));
    handleCaptionChange(10, "x");
    let done = false;
    const flushed = develop.flushPending().then(() => (done = true));
    await new Promise((r) => setTimeout(r, 0));
    expect(done).toBe(false);
    release();
    await flushed;
    expect(done).toBe(true);
  });
});
