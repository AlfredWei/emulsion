import { describe, it, expect, vi, beforeEach } from "vitest";
import { flushSync } from "svelte";

vi.mock("@tauri-apps/api/core", () => ({ convertFileSrc: (/** @type {string} */ p) => p }));
vi.mock("$lib/api/segment.js", () => ({ segmentRelease: vi.fn(async () => {}) }));

import { segmentRelease } from "$lib/api/segment.js";
import { SegmentStore } from "./segment.svelte.js";
import { MaskStore } from "./masks.svelte.js";
import { DevelopStore } from "./develop.svelte.js";
import { createLibraryStore } from "./library.svelte.js";

function setup() {
  const library = createLibraryStore();
  library.images = /** @type {any} */ ([
    { version_id: 1, content_hash: "h1" },
    { version_id: 2, content_hash: "h2" },
  ]);
  const develop = new DevelopStore(library);
  develop.versionId = 1;
  const masks = new MaskStore(develop);
  const shell = $state({ activeModule: "develop" });
  const segment = new SegmentStore(masks, develop, shell);
  const stop = $effect.root(() => segment.install());
  flushSync();
  return { develop, masks, segment, shell, stop };
}

beforeEach(() => vi.clearAllMocks());

describe("SegmentStore.install: the session ends with its tool, its image and its module", () => {
  it("resets the session when the tool is switched off, keeping nothing", () => {
    const { masks, segment, stop } = setup();
    masks.activeTool = "segment";
    segment.phase = "ready";
    segment.prompts = [{ x: 0.1, y: 0.2, positive: true }];
    segment.maskId = "m1";
    flushSync();
    expect(segment.phase).toBe("ready"); // still armed
    masks.activeTool = null;
    flushSync();
    expect([segment.phase, segment.prompts, segment.maskId, segment.candidates]).toEqual(["idle", [], null, []]);
    stop();
  });

  it("releases the helper's embedding and resets when the open image changes", () => {
    const { develop, masks, segment, stop } = setup();
    masks.activeTool = "segment";
    segment.phase = "ready";
    segment.preparedHash = "h1";
    develop.versionId = 2;
    flushSync();
    expect(segmentRelease).toHaveBeenCalledTimes(1);
    expect(segment.preparedHash).toBeNull();
    expect(segment.phase).toBe("idle");
    stop();
  });

  it("releases it when Develop is left, and not before anything was prepared", () => {
    const { shell, masks, segment, stop } = setup();
    shell.activeModule = "library";
    flushSync();
    expect(segmentRelease).not.toHaveBeenCalled();
    shell.activeModule = "develop";
    masks.activeTool = "segment";
    segment.phase = "ready";
    segment.preparedHash = "h1";
    flushSync();
    shell.activeModule = "library";
    flushSync();
    expect(segmentRelease).toHaveBeenCalledTimes(1);
    expect(segment.preparedHash).toBeNull();
    stop();
  });
});
