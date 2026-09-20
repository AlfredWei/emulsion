import { describe, it, expect } from "vitest";
import { createImportFlowStore } from "./importFlow.svelte.js";

const SETTINGS = /** @type {any} */ ({ enabled: true, interval_days: 7, last_backup_at: null });

describe("ImportFlowStore", () => {
  it("starts idle", () => {
    const s = createImportFlowStore();
    expect([s.importing, s.mergingHdr, s.mergingPanorama, s.isDraggingFiles]).toEqual([false, false, false, false]);
    expect(s.catalogProgress).toBeNull();
    expect(s.thumbnailProgress).toBeNull();
    expect(s.faceDetectionProgress).toBeNull();
    expect(s.hdrMergeProgress).toBeNull();
    expect(s.phase).toBe("cataloging");
    expect(s.supportedExtensions).toBeNull();
    expect(s.backupPromptOpen).toBe(false);
    expect(s.backupPromptSettings).toBeNull();
  });

  it("instances are independent", () => {
    const a = createImportFlowStore();
    const b = createImportFlowStore();
    a.importing = true;
    a.phase = "faces";
    expect(b.importing).toBe(false);
    expect(b.phase).toBe("cataloging");
  });

  describe("backup prompt bridge", () => {
    it("opens with the settings and stays pending until closed", async () => {
      const s = createImportFlowStore();
      let settled = false;
      const waiting = s.showBackupPromptAndWait(SETTINGS).then(() => (settled = true));
      expect(s.backupPromptOpen).toBe(true);
      expect(s.backupPromptSettings).toEqual(SETTINGS); // $state wraps it in a proxy, so equal, not identical
      await Promise.resolve();
      expect(settled).toBe(false); // the window-close handler is still blocked
      s.closeBackupPrompt();
      await waiting;
      expect(settled).toBe(true);
      expect(s.backupPromptOpen).toBe(false);
    });

    it("closing with no prompt open is harmless", () => {
      const s = createImportFlowStore();
      expect(() => s.closeBackupPrompt()).not.toThrow();
      expect(s.backupPromptOpen).toBe(false);
    });

    it("settles once: a second close does nothing", async () => {
      const s = createImportFlowStore();
      const waiting = s.showBackupPromptAndWait(SETTINGS);
      s.closeBackupPrompt();
      await waiting;
      expect(() => s.closeBackupPrompt()).not.toThrow();
    });

    it("can be shown again after it was closed", async () => {
      const s = createImportFlowStore();
      const first = s.showBackupPromptAndWait(SETTINGS);
      s.closeBackupPrompt();
      await first;
      const next = { ...SETTINGS, interval_days: 30 };
      const second = s.showBackupPromptAndWait(next);
      expect(s.backupPromptSettings).toEqual(next);
      expect(s.backupPromptOpen).toBe(true);
      s.closeBackupPrompt();
      await second;
    });
  });
});
