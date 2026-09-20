import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

const { updateBackupSettings } = vi.hoisted(() => ({ updateBackupSettings: vi.fn() }));
vi.mock("$lib/api/backup.js", () => ({ updateBackupSettings }));

import { handleBackupDone, handleBackupSkip } from "./backupActions.js";
import { importFlow } from "$lib/state/importFlow.svelte.js";

const SETTINGS = /** @type {any} */ ({ enabled: true, interval_days: 7, last_backup_at: "2020-01-01T00:00:00.000Z" });

beforeEach(() => {
  vi.clearAllMocks();
  updateBackupSettings.mockResolvedValue(undefined);
  importFlow.closeBackupPrompt();
});
afterEach(() => vi.useRealTimers());

describe("backup prompt handlers", () => {
  it("Back Up Now: saves the settings as given and releases the waiting close handler", async () => {
    const waiting = importFlow.showBackupPromptAndWait(SETTINGS);
    handleBackupDone(SETTINGS);
    await waiting;
    expect(updateBackupSettings).toHaveBeenCalledWith(SETTINGS);
    expect(importFlow.backupPromptOpen).toBe(false);
  });

  it("Skip This Time: resets the due-clock to now so it doesn't re-prompt on the very next close", async () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-09-20T12:00:00.000Z"));
    const waiting = importFlow.showBackupPromptAndWait(SETTINGS);
    handleBackupSkip(SETTINGS);
    await waiting;
    expect(updateBackupSettings).toHaveBeenCalledWith({ ...SETTINGS, last_backup_at: "2026-09-20T12:00:00.000Z" });
    expect(importFlow.backupPromptOpen).toBe(false);
  });

  it("a failing settings save never blocks the prompt from closing (quit must not hang)", async () => {
    updateBackupSettings.mockRejectedValue(new Error("db locked"));
    for (const handler of [handleBackupDone, handleBackupSkip]) {
      const waiting = importFlow.showBackupPromptAndWait(SETTINGS);
      handler(SETTINGS);
      await waiting; // would hang forever if the rejection were awaited or rethrown
      expect(importFlow.backupPromptOpen).toBe(false);
    }
  });
});
