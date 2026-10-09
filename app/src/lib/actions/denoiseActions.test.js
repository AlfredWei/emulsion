import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ convertFileSrc: (/** @type {string} */ p) => p }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("$lib/api/denoise.js", () => ({
  denoiseModelStatus: vi.fn(),
  denoiseDownloadModel: vi.fn(),
  denoiseCancelDownload: vi.fn(async () => {}),
  denoiseImportModel: vi.fn(),
  denoiseCacheInfo: vi.fn(),
  denoiseRun: vi.fn(),
  denoiseCancel: vi.fn(async () => {}),
  denoiseRemoveCache: vi.fn(async () => {}),
  onDenoiseModelProgress: vi.fn(async () => () => {}),
  onDenoiseProgress: vi.fn(async () => () => {}),
  isDownloadCancelled: (/** @type {unknown} */ e) => String(e).includes("download cancelled"),
  isJobCancelled: (/** @type {unknown} */ e) => String(e).includes("denoise cancelled"),
}));

import * as A from "./denoiseActions.js";
import * as api from "$lib/api/denoise.js";
import { open } from "@tauri-apps/plugin-dialog";
import { develop } from "$lib/state/develop.svelte.js";
import { library } from "$lib/state/library.svelte.js";
import { shell } from "$lib/state/shell.svelte.js";
import { denoise } from "$lib/state/denoise.svelte.js";
import { getAiDenoise } from "$lib/api/develop.js";

const m = /** @type {Record<string, import('vitest').Mock>} */ (/** @type {any} */ (api));
const READY = { state: "ready", download_bytes: 0, host: "github.com" };
const MISSING = { state: "missing", download_bytes: 117_316_644, host: "github.com" };
const KEPT = { model: "nafnet_sidd_w32", width: 6000, height: 4000, bytes: 40_000_000 };
const WHOLE = { path: "/cache/hash-a.png", width: 6000, height: 4000, tiles: 117, ms: 101_000, before_path: null, region: null };
const CROP = { path: "/cache/crop-after.png", width: 992, height: 736, tiles: 4, ms: 3400, before_path: "/cache/crop-before.png", region: { x: 10, y: 20, w: 992, h: 736 } };

/** @type {ReturnType<typeof vi.spyOn>} */
let schedule;
/** @type {ReturnType<typeof vi.spyOn>} */
let notify;

beforeEach(() => {
  vi.clearAllMocks();
  schedule = vi.spyOn(develop, "scheduleFlush").mockImplementation(() => {});
  notify = vi.spyOn(shell, "notify").mockImplementation(() => {});
  library.images = /** @type {any} */ ([
    { version_id: 10, content_hash: "hash-a" },
    { version_id: 11, content_hash: "hash-b" },
  ]);
  develop.versionId = 10;
  develop.imagePath = "/photos/a.jpg";
  develop.editStack = { schema_version: 1, ops: [] };
  denoise.dialog = null;
  denoise.dialogError = "";
  denoise.modelStatus = null;
  denoise.pendingJob = null;
  denoise.job = null;
  denoise.error = "";
  denoise.crop = null;
  denoise.cache = null;
  denoise.cacheHash = "hash-a";
  denoise.cacheVersion = 0;
  m.denoiseModelStatus.mockResolvedValue(READY);
  m.denoiseCacheInfo.mockResolvedValue(KEPT);
  m.denoiseRun.mockResolvedValue(WHOLE);
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("Amount", () => {
  it("writes the ai_denoise op and schedules a history entry", () => {
    A.handleAiDenoiseAmountChange({ amount: 55 });
    expect(getAiDenoise(develop.editStack).amount).toBe(55);
    expect(schedule).toHaveBeenCalledWith("AI Denoise");
  });
});

describe("whole-photo job", () => {
  it("runs on the open photo, shows progress, keeps the result and starts the amount at a default", async () => {
    /** @type {((p: { done: number, total: number }) => void) | null} */
    let report = null;
    m.denoiseRun.mockImplementation(async () => {
      expect(denoise.job).toMatchObject({ kind: "whole", hash: "hash-a", label: "a.jpg" });
      report?.({ done: 5, total: 117 });
      expect(denoise.job).toMatchObject({ done: 5, total: 117 });
      return WHOLE;
    });
    m.onDenoiseProgress.mockImplementation(async (/** @type {any} */ h) => {
      report = h;
      return () => {};
    });

    await A.handleDenoiseWhole();

    expect(m.denoiseRun).toHaveBeenCalledWith("/photos/a.jpg", "hash-a", null);
    expect(denoise.job).toBeNull();
    expect(denoise.hasResult).toBe(true);
    expect(denoise.cacheVersion).toBe(1);
    expect(getAiDenoise(develop.editStack).amount).toBe(A.DEFAULT_AI_DENOISE_AMOUNT);
    expect(schedule).toHaveBeenCalledWith("AI Denoise");
    expect(notify).toHaveBeenCalledWith("Denoised a.jpg in 1 min 41 s.");
  });

  it("leaves an amount the user already chose alone", async () => {
    develop.editStack = /** @type {any} */ ({ schema_version: 1, ops: [{ op: "ai_denoise", amount: 30 }] });
    await A.handleDenoiseWhole();
    expect(getAiDenoise(develop.editStack).amount).toBe(30);
    expect(schedule).not.toHaveBeenCalled();
  });

  it("allows one job at a time", async () => {
    let finish = () => {};
    m.denoiseRun.mockImplementation(() => new Promise((resolve) => (finish = () => resolve(WHOLE))));
    const first = A.handleDenoiseWhole();
    await vi.waitFor(() => expect(denoise.job).not.toBeNull());
    await A.handleDenoisePreviewCrop();
    expect(m.denoiseRun).toHaveBeenCalledTimes(1);
    finish();
    await first;
  });

  it("a job for a photo that is no longer open touches neither its edit stack nor the open photo's state", async () => {
    let finish = () => {};
    m.denoiseRun.mockImplementation(() => new Promise((resolve) => (finish = () => resolve(WHOLE))));
    const job = A.handleDenoiseWhole();
    await vi.waitFor(() => expect(denoise.job).not.toBeNull());
    develop.versionId = 11; // the user opened another photo meanwhile
    develop.imagePath = "/photos/b.jpg";
    expect(denoise.jobHere).toBe(false);
    finish();
    await job;
    expect(denoise.cacheVersion).toBe(0);
    expect(getAiDenoise(develop.editStack).amount).toBe(0);
    expect(m.denoiseCacheInfo).not.toHaveBeenCalled();
    expect(notify).toHaveBeenCalledWith("Denoised a.jpg in 1 min 41 s.");
  });

  it("a stopped job says so and sets no error", async () => {
    m.denoiseRun.mockRejectedValue("denoise cancelled");
    await A.handleDenoiseWhole();
    expect(notify).toHaveBeenCalledWith("Denoise stopped.");
    expect(denoise.error).toBe("");
    expect(denoise.job).toBeNull();
    expect(denoise.hasResult).toBe(false);
  });

  it("Cancel asks the backend to stop the running job, and does nothing when none runs", async () => {
    A.handleDenoiseCancel();
    expect(m.denoiseCancel).not.toHaveBeenCalled();
    let finish = () => {};
    m.denoiseRun.mockImplementation(() => new Promise((_, reject) => (finish = () => reject("denoise cancelled"))));
    const job = A.handleDenoiseWhole();
    await vi.waitFor(() => expect(denoise.job).not.toBeNull());
    A.handleDenoiseCancel();
    expect(m.denoiseCancel).toHaveBeenCalledTimes(1);
    finish();
    await job;
  });

  it("a failure shows its message for the open photo and in the status line", async () => {
    m.denoiseRun.mockRejectedValue("Error: inference failed: out of memory");
    await A.handleDenoiseWhole();
    expect(denoise.error).toBe("inference failed: out of memory");
    expect(notify).toHaveBeenCalledWith("Denoise failed: inference failed: out of memory");
    expect(denoise.job).toBeNull();
  });

  it("a photo without a fingerprint cannot be denoised yet", async () => {
    library.images = /** @type {any} */ ([{ version_id: 10, content_hash: null }]);
    await A.handleDenoiseWhole();
    expect(m.denoiseRun).not.toHaveBeenCalled();
    expect(denoise.error).toMatch(/fingerprint/);
  });
});

describe("crop preview", () => {
  it("denoises around the middle and keeps the before and after files, changing nothing else", async () => {
    m.denoiseRun.mockResolvedValue(CROP);
    await A.handleDenoisePreviewCrop();
    expect(m.denoiseRun).toHaveBeenCalledWith("/photos/a.jpg", "hash-a", [0.5, 0.5]);
    expect(denoise.crop).toEqual({ hash: "hash-a", before: "/cache/crop-before.png", after: "/cache/crop-after.png", region: CROP.region, ms: 3400 });
    expect(denoise.cacheVersion).toBe(0);
    expect(schedule).not.toHaveBeenCalled();
    A.handleDenoiseDismissCrop();
    expect(denoise.crop).toBeNull();
  });

  it("a reply without its pictures is an error, not a blank preview", async () => {
    m.denoiseRun.mockResolvedValue({ ...CROP, before_path: null });
    await A.handleDenoisePreviewCrop();
    expect(denoise.crop).toBeNull();
    expect(denoise.error).toMatch(/without its pictures/);
  });
});

describe("removing the kept copy", () => {
  it("deletes it, forgets it and makes the canvas rebuild its source", async () => {
    denoise.cache = KEPT;
    denoise.crop = { hash: "hash-a", before: "b", after: "a", region: { x: 0, y: 0, w: 1, h: 1 }, ms: 1 };
    await A.handleDenoiseRemove();
    expect(m.denoiseRemoveCache).toHaveBeenCalledWith("hash-a");
    expect(denoise.cache).toBeNull();
    expect(denoise.crop).toBeNull();
    expect(denoise.cacheVersion).toBe(1);
  });

  it("keeps everything when the delete fails", async () => {
    denoise.cache = KEPT;
    m.denoiseRemoveCache.mockRejectedValueOnce("disk is read-only");
    await A.handleDenoiseRemove();
    expect(denoise.cache).toEqual(KEPT);
    expect(denoise.error).toBe("disk is read-only");
    expect(denoise.cacheVersion).toBe(0);
  });
});

describe("the model dialog", () => {
  beforeEach(() => {
    m.denoiseModelStatus.mockResolvedValue(MISSING);
  });

  it("asks first when the model is missing, and runs nothing", async () => {
    await A.handleDenoiseWhole();
    expect(denoise.dialog).toBe("consent");
    expect(denoise.modelStatus).toEqual(MISSING);
    expect(denoise.pendingJob).toEqual({ kind: "whole", hash: "hash-a", path: "/photos/a.jpg" });
    expect(m.denoiseDownloadModel).not.toHaveBeenCalled();
    expect(m.denoiseRun).not.toHaveBeenCalled();
  });

  it("Download fetches the model and then runs the job it was opened for, on that photo", async () => {
    await A.handleDenoisePreviewCrop();
    m.denoiseDownloadModel.mockResolvedValue(READY);
    m.denoiseRun.mockResolvedValue(CROP);
    develop.versionId = 11; // the user moved on while it downloaded
    develop.imagePath = "/photos/b.jpg";
    await A.handleDenoiseDownload();
    expect(denoise.dialog).toBeNull();
    expect(denoise.pendingJob).toBeNull();
    expect(m.denoiseRun).toHaveBeenCalledWith("/photos/a.jpg", "hash-a", [0.5, 0.5]);
  });

  it("shows download progress from the backend's events", async () => {
    await A.handleDenoiseWhole();
    /** @type {((p: { downloaded: number, total: number }) => void) | null} */
    let report = null;
    m.onDenoiseModelProgress.mockImplementation(async (/** @type {any} */ h) => {
      report = h;
      return () => {};
    });
    m.denoiseDownloadModel.mockImplementation(async () => {
      expect(denoise.dialog).toBe("downloading");
      expect(denoise.download).toEqual({ downloaded: 0, total: 117_316_644 });
      report?.({ downloaded: 50_000_000, total: 117_316_644 });
      expect(denoise.download.downloaded).toBe(50_000_000);
      return READY;
    });
    await A.handleDenoiseDownload();
  });

  it("a cancelled download closes quietly and forgets the job; a failed one offers Try again", async () => {
    await A.handleDenoiseWhole();
    m.denoiseDownloadModel.mockRejectedValueOnce("download cancelled");
    await A.handleDenoiseDownload();
    expect(denoise.dialog).toBeNull();
    expect(denoise.pendingJob).toBeNull();
    expect(denoise.dialogError).toBe("");

    await A.handleDenoiseWhole();
    m.denoiseDownloadModel.mockRejectedValueOnce("Error: connection reset");
    await A.handleDenoiseDownload();
    expect(denoise.dialog).toBe("failed");
    expect(denoise.dialogError).toBe("connection reset");
    A.handleDenoiseDialogRetry();
    expect(denoise.dialog).toBe("consent");
    expect(denoise.dialogError).toBe("");
  });

  it("Cancel stops a running download instead of closing, and closes the dialog otherwise", async () => {
    await A.handleDenoiseWhole();
    denoise.dialog = "downloading";
    A.handleDenoiseDialogCancel();
    expect(m.denoiseCancelDownload).toHaveBeenCalledTimes(1);
    expect(denoise.dialog).toBe("downloading"); // closed by the download call finishing

    denoise.dialog = "consent";
    A.handleDenoiseDialogCancel();
    expect(denoise.dialog).toBeNull();
    expect(denoise.pendingJob).toBeNull();
  });

  it("I have the file installs a chosen model and carries on", async () => {
    await A.handleDenoiseWhole();
    /** @type {any} */ (open).mockResolvedValue("/x/nafnet_sidd_w32.onnx");
    m.denoiseImportModel.mockResolvedValue(READY);
    await A.handleDenoiseImport();
    expect(m.denoiseImportModel).toHaveBeenCalledWith("/x/nafnet_sidd_w32.onnx");
    expect(denoise.dialog).toBeNull();
    expect(m.denoiseRun).toHaveBeenCalledTimes(1);

    // Declining the picker changes nothing; a wrong file is a failure with its message.
    await A.handleDenoiseWhole();
    /** @type {any} */ (open).mockResolvedValue(null);
    await A.handleDenoiseImport();
    expect(m.denoiseImportModel).toHaveBeenCalledTimes(1);
    /** @type {any} */ (open).mockResolvedValue("/x/other.onnx");
    m.denoiseImportModel.mockRejectedValueOnce("/x/other.onnx is not the denoise model file");
    await A.handleDenoiseImport();
    expect(denoise.dialog).toBe("failed");
    expect(denoise.dialogError).toMatch(/not the denoise model file/);
  });
});

describe("the store follows the open photo", () => {
  it("looks the kept copy up for the photo and ignores an answer for one that is no longer open", async () => {
    await denoise.refreshCache();
    expect(m.denoiseCacheInfo).toHaveBeenCalledWith("hash-a");
    expect(denoise.hasResult).toBe(true);

    /** @type {(v: typeof KEPT | null) => void} */
    let answer = () => {};
    m.denoiseCacheInfo.mockImplementationOnce(() => new Promise((resolve) => (answer = resolve)));
    denoise.cache = null;
    const lookup = denoise.refreshCache();
    develop.versionId = 11;
    answer(KEPT);
    await lookup;
    expect(denoise.cache).toBeNull();
    expect(denoise.hasResult).toBe(false);
  });

  it("a failed lookup reads as no copy", async () => {
    m.denoiseCacheInfo.mockRejectedValueOnce("io");
    await denoise.refreshCache();
    expect(denoise.cache).toBeNull();
  });
});

describe("formatDuration", () => {
  it("is seconds under a minute, then minutes and seconds", () => {
    expect(A.formatDuration(400)).toBe("1 s");
    expect(A.formatDuration(42_000)).toBe("42 s");
    expect(A.formatDuration(60_000)).toBe("1 min 0 s");
    expect(A.formatDuration(101_000)).toBe("1 min 41 s");
  });
});
