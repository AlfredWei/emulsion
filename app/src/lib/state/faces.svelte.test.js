import { describe, it, expect } from "vitest";
import { createFacesStore } from "./faces.svelte.js";

describe("FacesStore", () => {
  it("starts empty and idle", () => {
    const f = createFacesStore();
    expect(f.people).toEqual([]);
    expect(f.currentImageFaces).toEqual([]);
    expect([f.showFaceRects, f.detectingFaces, f.detectionCancelable]).toEqual([false, false, false]);
    expect(f.hoveredFaceId).toBeNull();
    expect(f.scanProgress).toBeNull();
    expect(f.avatarSourceUrls).toEqual({});
    expect([f.confirmingDetectionOnImport, f.pendingImportBatchSize]).toEqual([false, 0]);
  });

  describe("import-time detection prompt", () => {
    it("opens the prompt with the batch size, and resolves true when confirmed", async () => {
      const f = createFacesStore();
      const answer = f.promptDetectionOnImport(12);
      expect(f.confirmingDetectionOnImport).toBe(true);
      expect(f.pendingImportBatchSize).toBe(12);
      f.confirmDetectionPrompt();
      expect(f.confirmingDetectionOnImport).toBe(false);
      await expect(answer).resolves.toBe(true);
    });

    it("resolves false when cancelled", async () => {
      const f = createFacesStore();
      const answer = f.promptDetectionOnImport(3);
      f.cancelDetectionPrompt();
      expect(f.confirmingDetectionOnImport).toBe(false);
      await expect(answer).resolves.toBe(false);
    });

    it("a stray confirm/cancel with no prompt open is harmless", () => {
      const f = createFacesStore();
      expect(() => f.confirmDetectionPrompt()).not.toThrow();
      expect(() => f.cancelDetectionPrompt()).not.toThrow();
      expect(f.confirmingDetectionOnImport).toBe(false);
    });

    it("an answer is delivered once: a second confirm does not re-resolve or throw", async () => {
      const f = createFacesStore();
      const answer = f.promptDetectionOnImport(1);
      f.confirmDetectionPrompt();
      f.cancelDetectionPrompt(); // late click on the (already closed) dialog
      await expect(answer).resolves.toBe(true);
    });

    it("the dialog handlers work unbound, as the template passes them", async () => {
      const f = createFacesStore();
      const { confirmDetectionPrompt } = f;
      const answer = f.promptDetectionOnImport(2);
      confirmDetectionPrompt();
      await expect(answer).resolves.toBe(true);
    });

    it("can be prompted again after an answer", async () => {
      const f = createFacesStore();
      const first = f.promptDetectionOnImport(1);
      f.cancelDetectionPrompt();
      await first;
      const second = f.promptDetectionOnImport(5);
      expect(f.pendingImportBatchSize).toBe(5);
      f.confirmDetectionPrompt();
      await expect(second).resolves.toBe(true);
    });
  });
});
