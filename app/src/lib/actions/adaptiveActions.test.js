import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

const api = vi.hoisted(() => ({ resolveAdaptiveRegion: vi.fn() }));
vi.mock("$lib/api/adaptive.js", () => api);
const regen = vi.hoisted(() => vi.fn());
vi.mock("$lib/actions/importActions.js", () => ({ regenerateThumbnailFor: regen }));
const faceDetect = vi.hoisted(() => ({ runFaceDetection: vi.fn() }));
vi.mock("$lib/actions/faceActions.js", () => faceDetect);
vi.mock("@tauri-apps/api/core", () => ({ convertFileSrc: (/** @type {string} */ p) => p }));

import { handleApplyAdaptivePreset } from "./adaptiveActions.js";
import { develop } from "$lib/state/develop.svelte.js";
import { library } from "$lib/state/library.svelte.js";
import { shell } from "$lib/state/shell.svelte.js";
import { listMasks } from "$lib/api/develop.js";

const result = (/** @type {any} */ over = {}) => ({ outcome: "found", coverage: 0.25, reason: null, mask: { op: "luminance_range_mask", rangeMin: 0, rangeMax: 25, feather: 30, invert: false }, ...over });

beforeEach(() => {
  vi.clearAllMocks();
  develop.versionId = 10;
  develop.imagePath = "/p/a.raw";
  develop.editStack = { schema_version: 1, ops: [{ op: "exposure", value: 0.5 }] };
  library.images = /** @type {any} */ ([{ image_id: 7, version_id: 10, content_hash: "h1", faces_scanned: true }]);
  vi.spyOn(develop, "clearPreview").mockImplementation(() => {});
  vi.spyOn(develop, "flushEditStack").mockImplementation(async () => {});
  shell.notify("");
});
afterEach(() => vi.restoreAllMocks());

describe("handleApplyAdaptivePreset", () => {
  it("resolves the region, writes the mask and flushes once under one label", async () => {
    api.resolveAdaptiveRegion.mockResolvedValue(result());
    const report = await handleApplyAdaptivePreset("cool-shadows", 60);
    expect(api.resolveAdaptiveRegion).toHaveBeenCalledWith(7, "/p/a.raw", "h1", "shadows");
    expect(listMasks(develop.editStack)).toHaveLength(1);
    expect(/** @type {any} */ (listMasks(develop.editStack)[0]).temperature).toBe(-4.8);
    expect(develop.flushEditStack).toHaveBeenCalledTimes(1);
    expect(develop.flushEditStack).toHaveBeenCalledWith("Adaptive: Cool the shadows 60 %");
    expect(regen).toHaveBeenCalledWith(10);
    expect(report?.[0].action).toBe("added");
  });

  it("writes nothing and flushes nothing when the region is not found, and says why", async () => {
    api.resolveAdaptiveRegion.mockResolvedValue(result({ outcome: "not_found", coverage: 0, reason: "73 % of the frame is already deep shadow", mask: null }));
    await handleApplyAdaptivePreset("cool-shadows", 100);
    expect(listMasks(develop.editStack)).toHaveLength(0);
    expect(develop.flushEditStack).not.toHaveBeenCalled();
    expect(shell.statusMessage).toContain("nothing changed");
    expect(shell.statusMessage).toContain("73 % of the frame is already deep shadow");
  });

  it("scans faces first when the photo has not been scanned", async () => {
    library.images = /** @type {any} */ ([{ image_id: 7, version_id: 10, content_hash: "h1", faces_scanned: false }]);
    api.resolveAdaptiveRegion.mockResolvedValue(result({ mask: { op: "segment_mask", logits: "AAAA", feather: 50, grow: 0, invert: false } }));
    await handleApplyAdaptivePreset("brighten-faces", 100);
    expect(faceDetect.runFaceDetection).toHaveBeenCalledWith([7], false);
    expect(faceDetect.runFaceDetection.mock.invocationCallOrder[0]).toBeLessThan(api.resolveAdaptiveRegion.mock.invocationCallOrder[0]);
  });

  it("does not scan faces for a preset that has no face rule", async () => {
    library.images = /** @type {any} */ ([{ image_id: 7, version_id: 10, content_hash: "h1", faces_scanned: false }]);
    api.resolveAdaptiveRegion.mockResolvedValue(result());
    await handleApplyAdaptivePreset("cool-shadows", 100);
    expect(faceDetect.runFaceDetection).not.toHaveBeenCalled();
  });

  it("never writes one photo's masks onto another when the photo changes mid-analysis", async () => {
    api.resolveAdaptiveRegion.mockImplementation(async () => {
      develop.versionId = 11;
      return result();
    });
    const before = develop.editStack;
    expect(await handleApplyAdaptivePreset("cool-shadows", 100)).toBeNull();
    expect(develop.editStack).toBe(before);
    expect(develop.flushEditStack).not.toHaveBeenCalled();
  });

  it("reports a failure instead of throwing", async () => {
    api.resolveAdaptiveRegion.mockRejectedValue(new Error("preview failed"));
    expect(await handleApplyAdaptivePreset("cool-shadows", 100)).toBeNull();
    expect(develop.flushEditStack).not.toHaveBeenCalled();
  });

  it("ignores a second request while one is running", async () => {
    /** @type {(v: any) => void} */
    let release = () => {};
    api.resolveAdaptiveRegion.mockImplementation(() => new Promise((r) => (release = r)));
    const first = handleApplyAdaptivePreset("cool-shadows", 100);
    expect(await handleApplyAdaptivePreset("cool-shadows", 100)).toBeNull();
    release(result());
    await first;
    expect(develop.flushEditStack).toHaveBeenCalledTimes(1);
  });
});
