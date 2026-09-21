import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { flushSync } from "svelte";

const api = vi.hoisted(() => ({ getSoftProofPreview: vi.fn() }));
vi.mock("$lib/api/develop.js", async (importOriginal) => ({ ...(await importOriginal()), getSoftProofPreview: api.getSoftProofPreview }));
vi.mock("@tauri-apps/api/core", () => ({ convertFileSrc: (/** @type {string} */ p) => `asset://${p}` }));

import { SoftProofStore, createSoftProofStore } from "./softProof.svelte.js";
import { DevelopStore } from "./develop.svelte.js";
import { createLibraryStore } from "./library.svelte.js";
import { upsertOp } from "$lib/api/develop.js";

function setup() {
  const develop = new DevelopStore(createLibraryStore());
  const proof = new SoftProofStore(develop);
  return { develop, proof };
}
const installed = (/** @type {SoftProofStore} */ proof) => {
  const stop = $effect.root(() => proof.install());
  flushSync();
  return stop;
};

beforeEach(() => {
  vi.clearAllMocks();
  vi.useFakeTimers();
  api.getSoftProofPreview.mockResolvedValue({ path: "/tmp/proof.png" });
});
afterEach(() => vi.useRealTimers());

describe("SoftProofStore state", () => {
  it("starts off, sRGB / relative intent, no gamut warning, nothing loading", () => {
    const { proof } = setup();
    expect([proof.enabled, proof.target, proof.intent, proof.gamutWarning, proof.customProfilePath]).toEqual([false, "srgb", "relative", false, null]);
    expect([proof.previewUrl, proof.loading]).toEqual([null, false]);
  });

  it("each factory call gets its own state", () => {
    const a = createSoftProofStore(setup().develop);
    const b = createSoftProofStore(setup().develop);
    a.enabled = true;
    a.target = "adobe-rgb";
    expect(b.enabled).toBe(false);
    expect(b.target).toBe("srgb");
  });

  it("profileLabel names the target, and the file name for a custom profile", () => {
    const { proof } = setup();
    expect(proof.profileLabel).toBe("sRGB");
    proof.target = "adobe-rgb";
    expect(proof.profileLabel).toBe("Adobe RGB");
    proof.target = "prophoto-rgb";
    expect(proof.profileLabel).toBe("ProPhoto RGB");
    proof.target = "custom";
    expect(proof.profileLabel).toBe("Custom Profile");
    proof.customProfilePath = "/icc/Printer Paper.icc";
    expect(proof.profileLabel).toBe("Printer Paper.icc");
    proof.customProfilePath = "C:\\icc\\Win Paper.icm";
    expect(proof.profileLabel).toBe("Win Paper.icm");
  });
});

describe("SoftProofStore.install: the debounced preview", () => {
  it("does nothing while proofing is off, and clears any stale preview", async () => {
    const { develop, proof } = setup();
    develop.versionId = 10;
    proof.previewUrl = "stale";
    proof.loading = true;
    const stop = installed(proof);
    expect([proof.previewUrl, proof.loading]).toEqual([null, false]);
    await vi.advanceTimersByTimeAsync(1000);
    expect(api.getSoftProofPreview).not.toHaveBeenCalled();
    stop();
  });

  it("does nothing with no image open even when on", async () => {
    const { proof } = setup();
    proof.enabled = true;
    const stop = installed(proof);
    await vi.advanceTimersByTimeAsync(1000);
    expect(api.getSoftProofPreview).not.toHaveBeenCalled();
    stop();
  });

  it("fetches once, 250 ms after the settings settle, with every setting, and exposes an asset URL", async () => {
    const { develop, proof } = setup();
    develop.versionId = 10;
    proof.enabled = true;
    proof.target = "custom";
    proof.customProfilePath = "/icc/p.icc";
    proof.intent = "perceptual";
    proof.gamutWarning = true;
    const stop = installed(proof);
    await vi.advanceTimersByTimeAsync(249);
    expect(api.getSoftProofPreview).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(1);
    expect(api.getSoftProofPreview).toHaveBeenCalledTimes(1);
    expect(api.getSoftProofPreview).toHaveBeenCalledWith(10, { target: "custom", custom_profile_path: "/icc/p.icc", intent: "perceptual", gamut_warning: true });
    expect(proof.loading).toBe(false);
    expect(proof.previewUrl).toBe("asset:///tmp/proof.png");
    stop();
  });

  it("reports loading while the render is in flight", async () => {
    const { develop, proof } = setup();
    develop.versionId = 10;
    proof.enabled = true;
    /** @type {(v: any) => void} */
    let done = () => {};
    api.getSoftProofPreview.mockReturnValue(new Promise((r) => (done = r)));
    const stop = installed(proof);
    await vi.advanceTimersByTimeAsync(250);
    expect(proof.loading).toBe(true);
    done({ path: "/x.png" });
    await vi.advanceTimersByTimeAsync(0);
    expect(proof.loading).toBe(false);
    stop();
  });

  it("re-arms the debounce on every change (one fetch for a burst), and refires on an edit-stack change", async () => {
    const { develop, proof } = setup();
    develop.versionId = 10;
    proof.enabled = true;
    const stop = installed(proof);
    await vi.advanceTimersByTimeAsync(200);
    proof.intent = "absolute";
    flushSync();
    await vi.advanceTimersByTimeAsync(200);
    expect(api.getSoftProofPreview).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(50);
    expect(api.getSoftProofPreview).toHaveBeenCalledTimes(1);
    develop.editStack = upsertOp(develop.editStack, "exposure", 1);
    flushSync();
    await vi.advanceTimersByTimeAsync(250);
    expect(api.getSoftProofPreview).toHaveBeenCalledTimes(2);
    stop();
  });

  it("refires when the open image changes", async () => {
    const { develop, proof } = setup();
    develop.versionId = 10;
    proof.enabled = true;
    const stop = installed(proof);
    await vi.advanceTimersByTimeAsync(250);
    develop.versionId = 11;
    flushSync();
    await vi.advanceTimersByTimeAsync(250);
    expect(api.getSoftProofPreview.mock.calls.map((c) => c[0])).toEqual([10, 11]);
    stop();
  });

  it("turning proofing off cancels a pending fetch and clears the preview", async () => {
    const { develop, proof } = setup();
    develop.versionId = 10;
    proof.enabled = true;
    const stop = installed(proof);
    await vi.advanceTimersByTimeAsync(100);
    proof.enabled = false;
    flushSync();
    await vi.advanceTimersByTimeAsync(1000);
    expect(api.getSoftProofPreview).not.toHaveBeenCalled();
    expect(proof.previewUrl).toBeNull();
    stop();
  });

  it("a failed render clears the preview and the loading flag", async () => {
    const { develop, proof } = setup();
    develop.versionId = 10;
    proof.enabled = true;
    proof.previewUrl = "stale";
    api.getSoftProofPreview.mockRejectedValue(new Error("no profile"));
    const stop = installed(proof);
    await vi.advanceTimersByTimeAsync(250);
    expect(proof.previewUrl).toBeNull();
    expect(proof.loading).toBe(false);
    stop();
  });

  it("stopping the effect cancels a pending fetch", async () => {
    const { develop, proof } = setup();
    develop.versionId = 10;
    proof.enabled = true;
    const stop = installed(proof);
    await vi.advanceTimersByTimeAsync(100);
    stop();
    await vi.advanceTimersByTimeAsync(1000);
    expect(api.getSoftProofPreview).not.toHaveBeenCalled();
  });
});
