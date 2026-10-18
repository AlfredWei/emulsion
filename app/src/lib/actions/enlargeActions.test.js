import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("$lib/api/upscale.js", () => ({
  upscaleModelStatus: vi.fn(),
  upscaleDownloadModel: vi.fn(),
  upscaleCancelDownload: vi.fn(async () => {}),
  upscaleImportModel: vi.fn(),
  upscalePlan: vi.fn(),
  upscalePreview: vi.fn(),
  upscalePreviewClear: vi.fn(async () => {}),
  upscaleRun: vi.fn(),
  upscaleCancel: vi.fn(async () => {}),
  onUpscaleModelProgress: vi.fn(async () => () => {}),
  onUpscaleProgress: vi.fn(async () => () => {}),
  isUpscaleDownloadCancelled: (/** @type {unknown} */ e) => String(e).includes("download cancelled"),
  isUpscaleJobCancelled: (/** @type {unknown} */ e) => String(e).includes("upscale cancelled"),
}));
vi.mock("$lib/actions/libraryActions.js", () => ({
  refresh: vi.fn(async () => {}),
  refreshCollections: vi.fn(async () => {}),
  selectCollection: vi.fn(async () => {}),
  prioritizeThumbnail: vi.fn(),
}));
vi.mock("$lib/actions/upscaleActions.js", () => ({ revealPhotos: vi.fn(async () => true) }));
vi.mock("$lib/actions/denoiseActions.js", () => ({ formatDuration: (/** @type {number} */ ms) => `${Math.round(ms / 1000)} s` }));

import * as A from "./enlargeActions.js";
import * as api from "$lib/api/upscale.js";
import * as lib from "$lib/actions/libraryActions.js";
import * as nav from "$lib/actions/upscaleActions.js";
import { open } from "@tauri-apps/plugin-dialog";
import { upscale } from "$lib/state/upscale.svelte.js";
import { library } from "$lib/state/library.svelte.js";
import { selection } from "$lib/state/selection.svelte.js";
import { shell } from "$lib/state/shell.svelte.js";
import { defaultSettings } from "$lib/enlarge.js";

const m = /** @type {Record<string, import('vitest').Mock>} */ (/** @type {any} */ (api));
const l = /** @type {Record<string, import('vitest').Mock>} */ (/** @type {any} */ (lib));
const READY = { state: "ready", download_bytes: 0, host: "github.com" };
const MISSING = { state: "missing", download_bytes: 4_866_388, host: "github.com" };
const PLAN = { source_width: 6000, source_height: 4000, out_width: 24000, out_height: 16000, megapixels: 384, tiles: 460, seconds: 300, file_bytes: 1, very_large: false, export_gb: 23 };
const RESULT = { image_id: 9, version_id: 90, path: "/photos/a-Enlarged-4x.jpg", width: 24000, height: 16000, ms: 101_000 };
const PREVIEW = { before_path: "/p/before.png", after_path: "/p/after.jpg", width: 1000, height: 1000, region: { x: 875, y: 475, w: 250, h: 250 }, ms: 1800 };
const photo = (/** @type {object} */ extra = {}) => /** @type {any} */ ({ image_id: 1, version_id: 10, path: "/photos/a.jpg", ...extra });

/** @type {ReturnType<typeof vi.spyOn>} */
let notify;

beforeEach(() => {
  vi.clearAllMocks();
  notify = vi.spyOn(shell, "notify").mockImplementation(() => {});
  upscale.target = null;
  upscale.batch = null;
  upscale.settings = defaultSettings();
  upscale.fileName = "";
  upscale.plan = null;
  upscale.planError = "";
  upscale.modelDialog = null;
  upscale.modelStatus = null;
  upscale.dialogError = "";
  upscale.pendingRun = null;
  upscale.job = null;
  upscale.error = "";
  upscale.preview = null;
  upscale.previewKey = "";
  upscale.previewing = false;
  upscale.previewError = "";
  upscale.previewCentre = [0.5, 0.5];
  library.images = /** @type {any} */ ([photo()]);
  library.activeCollectionId = null;
  shell.activeModule = "library";
  selection.selectedIds = new Set([10]);
  m.upscaleModelStatus.mockResolvedValue(READY);
  m.upscalePlan.mockResolvedValue(PLAN);
  m.upscaleRun.mockResolvedValue(RESULT);
  m.upscalePreview.mockResolvedValue(PREVIEW);
});

describe("openEnlargeDialog", () => {
  it("opens for the photo and fills in the plan", async () => {
    A.openEnlargeDialog(photo());
    expect(upscale.target).toEqual({ versionId: 10, imageId: 1, path: "/photos/a.jpg" });
    await vi.waitFor(() => expect(upscale.plan).toEqual(PLAN));
    expect(m.upscalePlan).toHaveBeenCalledWith(10, { kind: "scale", value: 4 }, 95);
    await vi.waitFor(() => expect(upscale.modelStatus).toEqual(READY));
  });

  it("refuses an enlargement, with the reason in the status line", () => {
    A.openEnlargeDialog(photo({ enlarged_scale: 4 }));
    expect(upscale.target).toBeNull();
    expect(notify).toHaveBeenCalledWith(expect.stringMatching(/compounding invented detail/));
  });

  it("refuses while a job runs", () => {
    upscale.job = { label: "x", done: 0, total: 0 };
    A.openEnlargeDialog(photo());
    expect(upscale.target).toBeNull();
    expect(notify).toHaveBeenCalledWith("An enlargement is already running");
  });

  it("keeps the form's settings between uses but not the file name", () => {
    upscale.settings.strength = 60;
    upscale.fileName = "mine";
    A.openEnlargeDialog(photo());
    expect(upscale.settings.strength).toBe(60);
    expect(upscale.fileName).toBe("");
  });
});

describe("refreshEnlargePlan", () => {
  it("shows the backend's reason when there is no plan", async () => {
    A.openEnlargeDialog(photo());
    await vi.waitFor(() => expect(upscale.plan).not.toBeNull());
    m.upscalePlan.mockRejectedValue(new Error("a long edge of 100 px is not larger than the photo's 6000 px"));
    upscale.settings.scale = "long_edge";
    upscale.settings.longEdge = "100";
    await A.refreshEnlargePlan();
    expect(upscale.plan).toBeNull();
    expect(upscale.planError).toBe("a long edge of 100 px is not larger than the photo's 6000 px");
  });

  it("asks nothing while the long edge is empty", async () => {
    A.openEnlargeDialog(photo());
    await vi.waitFor(() => expect(upscale.plan).not.toBeNull());
    m.upscalePlan.mockClear();
    upscale.settings.scale = "long_edge";
    await A.refreshEnlargePlan();
    expect(m.upscalePlan).not.toHaveBeenCalled();
    expect(upscale.plan).toBeNull();
  });

  it("drops an answer that arrives after a newer one", async () => {
    A.openEnlargeDialog(photo());
    await vi.waitFor(() => expect(upscale.plan).not.toBeNull());
    /** @type {(p: unknown) => void} */
    let slow = () => {};
    m.upscalePlan.mockImplementationOnce(() => new Promise((r) => (slow = r)));
    const first = A.refreshEnlargePlan();
    m.upscalePlan.mockResolvedValueOnce({ ...PLAN, out_width: 12000 });
    await A.refreshEnlargePlan();
    slow({ ...PLAN, out_width: 99 });
    await first;
    expect(upscale.plan?.out_width).toBe(12000);
  });
});

describe("handleEnlarge", () => {
  async function open_() {
    A.openEnlargeDialog(photo());
    await vi.waitFor(() => expect(upscale.plan).not.toBeNull());
  }

  it("runs the job with the form's choices, closes the dialog, and reports", async () => {
    await open_();
    upscale.settings.strength = 70;
    upscale.settings.quality = 100;
    upscale.settings.addToCollections = true;
    upscale.fileName = "  big one ";
    await A.handleEnlarge();
    expect(m.upscaleRun).toHaveBeenCalledWith(10, {
      target: { kind: "scale", value: 4 },
      strength: 70,
      jpegQuality: 100,
      fileName: "big one",
      addToCollections: true,
      metadataNote: true,
    });
    expect(upscale.target).toBeNull();
    expect(upscale.job).toBeNull();
    expect(l.refresh).toHaveBeenCalled();
    expect(l.refreshCollections).toHaveBeenCalled();
    expect(l.prioritizeThumbnail).toHaveBeenCalledWith(90);
    expect(notify).toHaveBeenCalledWith("Enlarged a.jpg ×4 (24000 × 16000 px) in 101 s.");
  });

  it("sends no file name when the field is empty, and leaves collections alone when not asked", async () => {
    await open_();
    await A.handleEnlarge();
    expect(m.upscaleRun.mock.calls[0][1]).toMatchObject({ fileName: null, addToCollections: false });
    expect(l.refreshCollections).not.toHaveBeenCalled();
  });

  it("selects the new photo only while the user is still on the original", async () => {
    await open_();
    await A.handleEnlarge();
    expect(nav.revealPhotos).toHaveBeenCalledWith([9]);

    vi.mocked(nav.revealPhotos).mockClear();
    await open_();
    selection.selectedIds = new Set([11]);
    await A.handleEnlarge();
    expect(nav.revealPhotos).not.toHaveBeenCalled();
  });

  it("tracks progress while it runs", async () => {
    await open_();
    /** @type {(p: { done: number, total: number }) => void} */
    let push = () => {};
    m.onUpscaleProgress.mockImplementationOnce(async (/** @type {typeof push} */ h) => ((push = h), () => {}));
    /** @type {(v: unknown) => void} */
    let finish = () => {};
    m.upscaleRun.mockImplementationOnce(() => new Promise((r) => (finish = r)));
    const done = A.handleEnlarge();
    await vi.waitFor(() => expect(upscale.job).toEqual({ label: "a.jpg", done: 0, total: 0 }));
    await vi.waitFor(() => expect(m.upscaleRun).toHaveBeenCalled());
    push({ done: 3, total: 460 });
    expect(upscale.job).toEqual({ label: "a.jpg", done: 3, total: 460 });
    finish(RESULT);
    await done;
    expect(upscale.job).toBeNull();
  });

  it("says a stopped job wrote nothing", async () => {
    await open_();
    m.upscaleRun.mockRejectedValue(new Error("upscale cancelled"));
    await A.handleEnlarge();
    expect(notify).toHaveBeenCalledWith("Enlargement stopped. Nothing was written.");
    expect(l.refresh).not.toHaveBeenCalled();
    expect(upscale.job).toBeNull();
  });

  it("reports a failure and clears the job", async () => {
    await open_();
    m.upscaleRun.mockRejectedValue(new Error("the helper crashed"));
    await A.handleEnlarge();
    expect(notify).toHaveBeenCalledWith("Enlarge failed: the helper crashed");
    expect(upscale.job).toBeNull();
  });

  it("does nothing without a plan", async () => {
    A.openEnlargeDialog(photo());
    upscale.plan = null;
    await A.handleEnlarge();
    expect(m.upscaleRun).not.toHaveBeenCalled();
  });

  it("Stop calls the backend only while a job runs", () => {
    A.handleEnlargeCancel();
    expect(m.upscaleCancel).not.toHaveBeenCalled();
    upscale.job = { label: "x", done: 0, total: 0 };
    A.handleEnlargeCancel();
    expect(m.upscaleCancel).toHaveBeenCalled();
  });
});

describe("the model dialog", () => {
  async function pending() {
    m.upscaleModelStatus.mockResolvedValue(MISSING);
    A.openEnlargeDialog(photo());
    await vi.waitFor(() => expect(upscale.plan).not.toBeNull());
    await A.handleEnlarge();
  }

  it("asks before downloading, keeping the Enlarge dialog open underneath", async () => {
    await pending();
    expect(upscale.modelDialog).toBe("consent");
    expect(upscale.target).not.toBeNull();
    expect(m.upscaleRun).not.toHaveBeenCalled();
    expect(upscale.pendingRun?.queue).toMatchObject([{ photo: { versionId: 10 }, name: "a.jpg", scale: 4 }]);
  });

  it("runs the job once the download lands", async () => {
    await pending();
    m.upscaleDownloadModel.mockResolvedValue(READY);
    await A.handleEnlargeModelDownload();
    expect(upscale.modelDialog).toBeNull();
    expect(m.upscaleRun).toHaveBeenCalledWith(10, expect.objectContaining({ strength: 100 }));
    expect(upscale.target).toBeNull();
  });

  it("backs out quietly when the download is cancelled", async () => {
    await pending();
    m.upscaleDownloadModel.mockRejectedValue(new Error("download cancelled"));
    await A.handleEnlargeModelDownload();
    expect(upscale.modelDialog).toBeNull();
    expect(upscale.pendingRun).toBeNull();
    expect(upscale.target).not.toBeNull();
    expect(m.upscaleRun).not.toHaveBeenCalled();
  });

  it("shows a failed download with its cause and lets the user retry", async () => {
    await pending();
    m.upscaleDownloadModel.mockRejectedValue(new Error("no network"));
    await A.handleEnlargeModelDownload();
    expect(upscale.modelDialog).toBe("failed");
    expect(upscale.dialogError).toBe("no network");
    A.handleEnlargeModelRetry();
    expect(upscale.modelDialog).toBe("consent");
  });

  it("imports a chosen file and runs the job", async () => {
    await pending();
    vi.mocked(open).mockResolvedValue("/x/realesr_general_x4v3.onnx");
    m.upscaleImportModel.mockResolvedValue(READY);
    await A.handleEnlargeModelImport();
    expect(m.upscaleImportModel).toHaveBeenCalledWith("/x/realesr_general_x4v3.onnx");
    expect(m.upscaleRun).toHaveBeenCalled();
  });

  it("an import that is refused is shown as a failure", async () => {
    await pending();
    vi.mocked(open).mockResolvedValue("/x/other.onnx");
    m.upscaleImportModel.mockRejectedValue(new Error("not the expected model file"));
    await A.handleEnlargeModelImport();
    expect(upscale.modelDialog).toBe("failed");
    expect(upscale.dialogError).toBe("not the expected model file");
  });

  it("Cancel closes the consent dialog and forgets the job; during a download it stops the download instead", async () => {
    await pending();
    upscale.modelDialog = "downloading";
    A.handleEnlargeModelCancel();
    expect(m.upscaleCancelDownload).toHaveBeenCalled();
    expect(upscale.modelDialog).toBe("downloading");
    upscale.modelDialog = "consent";
    A.handleEnlargeModelCancel();
    expect(upscale.modelDialog).toBeNull();
    expect(upscale.pendingRun).toBeNull();
  });
});

describe("the preview", () => {
  async function opened() {
    A.openEnlargeDialog(photo());
    await vi.waitFor(() => expect(upscale.plan).not.toBeNull());
    await vi.waitFor(() => expect(upscale.modelStatus).toEqual(READY));
  }

  it("asks for the middle of the photo at the chosen target and keeps the result", async () => {
    await opened();
    await A.handleEnlargePreview();
    expect(m.upscalePreview).toHaveBeenCalledWith(10, { kind: "scale", value: 4 }, [0.5, 0.5]);
    expect(upscale.preview).toEqual(PREVIEW);
    expect(upscale.previewing).toBe(false);
    expect(upscale.previewError).toBe("");
  });

  it("moves the preview to where the navigator was clicked, and remembers it", async () => {
    await opened();
    await A.handleEnlargePreview([0.2, 0.8]);
    expect(m.upscalePreview).toHaveBeenCalledWith(10, expect.anything(), [0.2, 0.8]);
    expect(upscale.previewCentre).toEqual([0.2, 0.8]);
  });

  it("is dropped when the scale changes, and kept when only Strength does", async () => {
    await opened();
    await A.handleEnlargePreview();
    upscale.settings.strength = 40;
    await A.refreshEnlargePlan();
    expect(upscale.preview).not.toBeNull();
    upscale.settings.scale = "2";
    await A.refreshEnlargePlan();
    expect(upscale.preview).toBeNull();
  });

  it("shows a failure, and a stopped preview is not a failure", async () => {
    await opened();
    m.upscalePreview.mockRejectedValueOnce(new Error("the helper crashed"));
    await A.handleEnlargePreview();
    expect(upscale.previewError).toBe("the helper crashed");
    expect(upscale.preview).toBeNull();
    m.upscalePreview.mockRejectedValueOnce(new Error("upscale cancelled"));
    await A.handleEnlargePreview();
    expect(upscale.previewError).toBe("");
  });

  it("does not ask for the model to be downloaded: it says Enlarge will", async () => {
    await opened();
    upscale.modelStatus = /** @type {any} */ (MISSING);
    await A.handleEnlargePreview();
    expect(m.upscalePreview).not.toHaveBeenCalled();
    expect(upscale.previewError).toMatch(/Press Enlarge to install it/);
  });

  it("ignores an answer that arrives after the dialog moved on", async () => {
    await opened();
    /** @type {(v: unknown) => void} */
    let finish = () => {};
    m.upscalePreview.mockImplementationOnce(() => new Promise((r) => (finish = r)));
    const running = A.handleEnlargePreview();
    expect(upscale.previewing).toBe(true);
    A.closeEnlargeDialog();
    finish(PREVIEW);
    await running;
    expect(upscale.preview).toBeNull();
    expect(upscale.previewing).toBe(false);
  });

  it("closing the dialog stops a running preview and deletes the pictures; Enlarge waits for it", async () => {
    await opened();
    /** @type {(v: unknown) => void} */
    let finish = () => {};
    m.upscalePreview.mockImplementationOnce(() => new Promise((r) => (finish = r)));
    const running = A.handleEnlargePreview();
    await A.handleEnlarge();
    expect(m.upscaleRun).not.toHaveBeenCalled();
    A.closeEnlargeDialog();
    expect(m.upscaleCancel).toHaveBeenCalled();
    expect(m.upscalePreviewClear).toHaveBeenCalled();
    finish(PREVIEW);
    await running;
  });

  it("starting the job clears the preview pictures", async () => {
    await opened();
    await A.handleEnlargePreview();
    await A.handleEnlarge();
    expect(m.upscalePreviewClear).toHaveBeenCalled();
    expect(upscale.preview).toBeNull();
  });
});

describe("a batch (RFC-0029 §18)", () => {
  const p = (/** @type {number} */ n, /** @type {object} */ extra = {}) => photo({ image_id: n, version_id: n * 10, path: `/photos/p${n}.jpg`, ...extra });
  const SECOND = { ...RESULT, image_id: 8, version_id: 80 };

  async function openBatch(/** @type {any[]} */ images = [p(1), p(2), p(3)]) {
    A.openEnlarge(images);
    await vi.waitFor(() => expect(upscale.batch?.items).not.toBeNull());
  }

  it("opens one dialog for several photos, leaving out the enlargements", async () => {
    await openBatch([p(1), p(2), p(3, { enlarged_scale: 4 })]);
    expect(upscale.batch?.photos.map((x) => x.versionId)).toEqual([10, 20]);
    expect(upscale.batch?.skipped).toBe(1);
    expect(upscale.batch?.items).toHaveLength(2);
    expect(m.upscalePlan).toHaveBeenCalledTimes(2);
  });

  it("opens the single dialog when one photo is left, and refuses a selection of enlargements only", () => {
    A.openEnlarge([p(1), p(2, { enlarged_scale: 4 })]);
    expect(upscale.batch).toBeNull();
    expect(upscale.target?.versionId).toBe(10);
    expect(notify).toHaveBeenCalledWith("1 enlargement was left out.");
    A.closeEnlargeDialog();
    notify.mockClear();
    A.openEnlarge([p(1, { enlarged_scale: 4 }), p(2, { enlarged_scale: 2 })]);
    expect(upscale.target).toBeNull();
    expect(notify).toHaveBeenCalledWith(expect.stringMatching(/compounding invented detail/));
  });

  it("drops the photos that cannot take the size from the totals", async () => {
    m.upscalePlan.mockImplementation(async (/** @type {number} */ id) => {
      if (id === 20) throw new Error("a long edge of 8000 px is not larger than the photo's 8000 px");
      return PLAN;
    });
    await openBatch();
    expect(upscale.batch?.items?.map((i) => i.photo.versionId)).toEqual([10, 30]);
    expect(upscale.planError).toBe("");
  });

  it("runs every photo in turn with default names and reports the total", async () => {
    m.upscaleRun.mockResolvedValueOnce(RESULT).mockResolvedValueOnce(SECOND).mockResolvedValueOnce({ ...RESULT, image_id: 7, version_id: 70 });
    await openBatch();
    upscale.fileName = "ignored";
    selection.selectedIds = new Set([10, 20, 30]);
    await A.handleEnlarge();
    expect(m.upscaleRun.mock.calls.map((c) => c[0])).toEqual([10, 20, 30]);
    expect(m.upscaleRun.mock.calls.every((c) => c[1].fileName === null)).toBe(true);
    expect(upscale.job).toBeNull();
    expect(l.refresh).toHaveBeenCalledTimes(1);
    expect(nav.revealPhotos).toHaveBeenCalledWith([9, 8, 7]);
    expect(notify).toHaveBeenCalledWith(expect.stringMatching(/^Enlarged 3 of 3 photos in /));
  });

  it("names the photo and its place in the status strip", async () => {
    await openBatch();
    /** @type {string[]} */
    const labels = [];
    m.upscaleRun.mockImplementation(async () => {
      labels.push(upscale.job?.label ?? "");
      return RESULT;
    });
    await A.handleEnlarge();
    expect(labels).toEqual(["p1.jpg (1 of 3)", "p2.jpg (2 of 3)", "p3.jpg (3 of 3)"]);
  });

  it("carries on after a photo fails and says which", async () => {
    m.upscaleRun.mockResolvedValueOnce(RESULT).mockRejectedValueOnce(new Error("out of memory")).mockResolvedValueOnce(SECOND);
    await openBatch();
    await A.handleEnlarge();
    expect(m.upscaleRun).toHaveBeenCalledTimes(3);
    expect(notify).toHaveBeenCalledWith(expect.stringMatching(/Enlarged 2 of 3 photos in .* 1 failed: p2\.jpg \(out of memory\)\./));
  });

  it("Stop ends the batch and keeps what was finished", async () => {
    m.upscaleRun.mockImplementation(async (/** @type {number} */ id) => {
      if (id === 20) {
        A.handleEnlargeCancel();
        throw new Error("upscale cancelled");
      }
      return RESULT;
    });
    await openBatch();
    await A.handleEnlarge();
    expect(m.upscaleRun.mock.calls.map((c) => c[0])).toEqual([10, 20]);
    expect(m.upscaleCancel).toHaveBeenCalled();
    expect(l.refresh).toHaveBeenCalled();
    expect(notify).toHaveBeenCalledWith("Stopped after 1 of 3 photos. The finished ones are in the catalog.");
  });

  it("asks for the model once, then runs the whole queue", async () => {
    m.upscaleModelStatus.mockResolvedValue(MISSING);
    await openBatch([p(1), p(2)]);
    await A.handleEnlarge();
    expect(upscale.modelDialog).toBe("consent");
    expect(upscale.pendingRun?.queue).toHaveLength(2);
    m.upscaleDownloadModel.mockResolvedValue(READY);
    await A.handleEnlargeModelDownload();
    expect(m.upscaleRun).toHaveBeenCalledTimes(2);
  });

  it("has no Preview", async () => {
    await openBatch();
    await A.handleEnlargePreview();
    expect(m.upscalePreview).not.toHaveBeenCalled();
  });
});
