import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ convertFileSrc: (/** @type {string} */ p) => p }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("$lib/api/segment.js", () => ({
  segmentModelStatus: vi.fn(),
  segmentDownloadModels: vi.fn(),
  segmentCancelDownload: vi.fn(async () => {}),
  segmentImportModelFiles: vi.fn(),
  segmentPrepare: vi.fn(),
  segmentDecode: vi.fn(),
  segmentRelease: vi.fn(async () => {}),
  onSegmentModelProgress: vi.fn(async () => () => {}),
  isNotPrepared: (/** @type {unknown} */ e) => String(e).includes("not_prepared"),
  isCancelled: (/** @type {unknown} */ e) => String(e).includes("download cancelled"),
}));

import * as A from "./segmentActions.js";
import * as api from "$lib/api/segment.js";
import { open } from "@tauri-apps/plugin-dialog";
import { develop } from "$lib/state/develop.svelte.js";
import { library } from "$lib/state/library.svelte.js";
import { masks } from "$lib/state/masks.svelte.js";
import { segment } from "$lib/state/segment.svelte.js";
import { addMask, createRadialGradientMask, createSegmentMask, createModifier, addModifier, listModifiers } from "$lib/api/develop.js";

const m = /** @type {Record<string, import('vitest').Mock>} */ (/** @type {any} */ (api));
const READY = { state: "ready", download_bytes: 0, host: "huggingface.co" };
const MISSING = { state: "missing", download_bytes: 155_000_000, host: "huggingface.co" };
const cand = (/** @type {number} */ index, /** @type {string} */ png, iou = 0.9) => ({ index, iou, logits_png: png });

/** @type {ReturnType<typeof vi.spyOn>} */
let schedule;
const labels = () => schedule.mock.calls.map((c) => c[0]);
const stackMasks = () => develop.editStack.ops.filter((o) => o.op === "segment_mask");

beforeEach(() => {
  vi.clearAllMocks();
  schedule = vi.spyOn(develop, "scheduleFlush").mockImplementation(() => {});
  vi.spyOn(develop, "flushEditStack").mockImplementation(async () => {});
  library.images = /** @type {any} */ ([{ version_id: 10, content_hash: "hash-a" }]);
  develop.versionId = 10;
  develop.imagePath = "/photos/a.jpg";
  develop.editStack = { schema_version: 1, ops: [] };
  masks.activeTool = null;
  masks.selectedMaskId = null;
  masks.selectedShapeId = null;
  masks.shapeTarget = null;
  segment.reset();
  segment.dialog = null;
  segment.dialogError = "";
  segment.preparedHash = null;
  m.segmentModelStatus.mockResolvedValue(READY);
  m.segmentPrepare.mockResolvedValue({ encode_ms: 1500, cached: false });
});
afterEach(() => vi.restoreAllMocks());

/** Arms the tool on a ready model and waits for the encoder. */
async function armed() {
  await A.handleSegmentToolToggle();
  expect(segment.phase).toBe("ready");
}

describe("arming the tool", () => {
  it("opens the consent dialog, and prepares nothing, while the model files are missing", async () => {
    m.segmentModelStatus.mockResolvedValue(MISSING);
    await A.handleSegmentToolToggle();
    expect(masks.activeTool).toBe("segment");
    expect(segment.dialog).toBe("consent");
    expect(segment.modelStatus).toEqual(MISSING);
    expect(m.segmentPrepare).not.toHaveBeenCalled();
  });

  it("prepares the open image through the helper and becomes ready for clicks", async () => {
    await armed();
    expect(m.segmentPrepare).toHaveBeenCalledWith("/photos/a.jpg", "hash-a");
    expect(segment.preparedHash).toBe("hash-a");
  });

  it("shows a failed prepare's message and retries on the next click", async () => {
    m.segmentPrepare.mockRejectedValueOnce("image: could not read the preview");
    await A.handleSegmentToolToggle();
    expect(segment.phase).toBe("failed");
    expect(segment.error).toContain("could not read the preview");
    m.segmentDecode.mockResolvedValue([cand(0, "L0")]);
    await A.handleSegmentClick({ x: 0.5, y: 0.5, positive: true });
    expect(segment.phase).toBe("ready"); // the click restarted the session
    expect(m.segmentPrepare).toHaveBeenCalledTimes(2);
  });

  it("ignores an answer that arrives after the session was abandoned", async () => {
    /** @type {(v: any) => void} */
    let finish = () => {};
    m.segmentPrepare.mockReturnValue(new Promise((r) => (finish = r)));
    const p = A.handleSegmentToolToggle();
    await vi.waitFor(() => expect(m.segmentPrepare).toHaveBeenCalled());
    segment.reset(); // tool switched off, image changed...
    finish({ encode_ms: 1, cached: false });
    await p;
    expect(segment.phase).toBe("idle");
    expect(segment.preparedHash).toBeNull();
  });
});

describe("clicking", () => {
  it("the first click creates a segment mask from the best candidate, selects it and stays armed", async () => {
    await armed();
    m.segmentDecode.mockResolvedValue([cand(2, "BEST", 0.95), cand(0, "OTHER", 0.6)]);
    await A.handleSegmentClick({ x: 0.3, y: 0.4, positive: true });
    expect(m.segmentDecode).toHaveBeenCalledWith("hash-a", [{ x: 0.3, y: 0.4, positive: true }], null);
    const [mask] = /** @type {any[]} */ (stackMasks());
    expect(mask).toMatchObject({ op: "segment_mask", logits: "BEST", candidate: 2, prompts: [{ x: 0.3, y: 0.4, positive: true }], feather: 0, grow: 0 });
    expect(masks.selectedMaskId).toBe(mask.id);
    expect(masks.activeTool).toBe("segment");
    expect(labels()).toEqual(["Select Subject"]);
    expect(segment.candidates).toHaveLength(2);
  });

  it("a refining click edits the same mask, feeding the chosen candidate back as the refine input", async () => {
    await armed();
    m.segmentDecode.mockResolvedValueOnce([cand(0, "FIRST")]);
    await A.handleSegmentClick({ x: 0.3, y: 0.4, positive: true });
    m.segmentDecode.mockResolvedValueOnce([cand(1, "SECOND")]);
    await A.handleSegmentClick({ x: 0.6, y: 0.4, positive: false });
    expect(m.segmentDecode).toHaveBeenLastCalledWith(
      "hash-a",
      [
        { x: 0.3, y: 0.4, positive: true },
        { x: 0.6, y: 0.4, positive: false },
      ],
      "FIRST",
    );
    const masksNow = /** @type {any[]} */ (stackMasks());
    expect(masksNow).toHaveLength(1);
    expect(masksNow[0]).toMatchObject({ logits: "SECOND", candidate: 1 });
    expect(masksNow[0].prompts).toHaveLength(2);
    expect(segment.prompts).toHaveLength(2);
  });

  it("switching candidate rewrites the mask's logits; Tab cycles and wraps", async () => {
    await armed();
    m.segmentDecode.mockResolvedValue([cand(0, "A"), cand(1, "B"), cand(2, "C")]);
    await A.handleSegmentClick({ x: 0.5, y: 0.5, positive: true });
    A.handleSegmentCandidate(2);
    expect(/** @type {any} */ (stackMasks()[0])).toMatchObject({ logits: "C", candidate: 2 });
    A.handleSegmentCycleCandidate();
    expect(/** @type {any} */ (stackMasks()[0])).toMatchObject({ logits: "A", candidate: 0 });
    expect(segment.candidateIndex).toBe(0);
  });

  it("re-prepares and retries once when the helper has restarted since prepare", async () => {
    await armed();
    m.segmentDecode.mockRejectedValueOnce("not_prepared: no embedding for hash-a").mockResolvedValueOnce([cand(0, "OK")]);
    await A.handleSegmentClick({ x: 0.5, y: 0.5, positive: true });
    expect(m.segmentPrepare).toHaveBeenCalledTimes(2);
    expect(m.segmentDecode).toHaveBeenCalledTimes(2);
    expect(/** @type {any} */ (stackMasks()[0]).logits).toBe("OK");
  });

  it("a failed decode drops that click, keeps the earlier selection and shows the message", async () => {
    await armed();
    m.segmentDecode.mockResolvedValueOnce([cand(0, "FIRST")]);
    await A.handleSegmentClick({ x: 0.3, y: 0.4, positive: true });
    m.segmentDecode.mockRejectedValueOnce("inference: out of memory");
    await A.handleSegmentClick({ x: 0.6, y: 0.4, positive: true });
    expect(segment.phase).toBe("ready");
    expect(segment.error).toContain("out of memory");
    expect(segment.prompts).toHaveLength(1);
    expect(/** @type {any} */ (stackMasks()[0]).logits).toBe("FIRST");
  });

  it("ignores clicks while a decode is running or the consent dialog is open", async () => {
    await armed();
    segment.phase = "deciding";
    await A.handleSegmentClick({ x: 0.5, y: 0.5, positive: true });
    segment.phase = "ready";
    segment.dialog = "consent";
    await A.handleSegmentClick({ x: 0.5, y: 0.5, positive: true });
    expect(m.segmentDecode).not.toHaveBeenCalled();
  });
});

describe("accepting and abandoning", () => {
  async function withMask() {
    await armed();
    m.segmentDecode.mockResolvedValue([cand(0, "A")]);
    await A.handleSegmentClick({ x: 0.5, y: 0.5, positive: true });
  }

  it("Accept keeps the mask and leaves the tool", async () => {
    await withMask();
    A.handleSegmentAccept();
    expect(masks.activeTool).toBeNull();
    expect(stackMasks()).toHaveLength(1);
  });

  it("toggling the tool off keeps the mask too", async () => {
    await withMask();
    await A.handleSegmentToolToggle();
    expect(masks.activeTool).toBeNull();
    expect(stackMasks()).toHaveLength(1);
  });

  it("Cancel removes the mask this session made and leaves the tool", async () => {
    await withMask();
    A.handleSegmentCancel();
    expect(masks.activeTool).toBeNull();
    expect(stackMasks()).toHaveLength(0);
    expect(masks.selectedMaskId).toBeNull();
  });

  it("Cancel before any click changes nothing in the edit stack", async () => {
    await armed();
    const before = develop.editStack;
    A.handleSegmentCancel();
    expect(develop.editStack).toBe(before);
    expect(masks.activeTool).toBeNull();
  });
});

describe("as a shape of the selected mask (Add shape ▸ Subject)", () => {
  it("the first decoded click becomes a modifier of that mask, and Cancel removes just it", async () => {
    const base = createRadialGradientMask({ x: 0.5, y: 0.5 }, 0.3, 0.3);
    develop.editStack = addMask(develop.editStack, base);
    masks.selectedMaskId = base.id;
    m.segmentDecode.mockResolvedValue([cand(0, "SHAPE")]);
    await A.handleAddSegmentShape("subtract");
    expect(masks.activeTool).toBe("segment");
    expect(segment.phase).toBe("ready");
    await A.handleSegmentClick({ x: 0.5, y: 0.5, positive: true });
    const owner = /** @type {any} */ (develop.editStack.ops.find((o) => o.op === "radial_gradient_mask"));
    const shapes = listModifiers(owner);
    expect(shapes).toHaveLength(1);
    expect(shapes[0]).toMatchObject({ combine: "subtract", shape: { op: "segment_mask", logits: "SHAPE" } });
    expect(stackMasks()).toHaveLength(0); // not a stand-alone mask
    expect(masks.selectedShapeId).toBe(shapes[0].id);
    expect(masks.activeTool).toBe("segment");

    m.segmentDecode.mockResolvedValueOnce([cand(1, "SHAPE2")]);
    await A.handleSegmentClick({ x: 0.2, y: 0.2, positive: true });
    expect(listModifiers(/** @type {any} */ (develop.editStack.ops[0]))[0].shape).toMatchObject({ logits: "SHAPE2", candidate: 1 });

    A.handleSegmentCancel();
    expect(listModifiers(/** @type {any} */ (develop.editStack.ops[0]))).toHaveLength(0);
    expect(develop.editStack.ops).toHaveLength(1);
  });

  it("does not arm anything when no mask is selected", async () => {
    await A.handleAddSegmentShape("add");
    expect(masks.activeTool).toBeNull();
    expect(m.segmentModelStatus).not.toHaveBeenCalled();
  });
});

describe("the model dialog", () => {
  beforeEach(async () => {
    m.segmentModelStatus.mockResolvedValue(MISSING);
    await A.handleSegmentToolToggle();
  });

  it("Download fetches the files, closes the dialog and prepares the image", async () => {
    m.segmentDownloadModels.mockResolvedValue(READY);
    await A.handleSegmentDownload();
    expect(segment.dialog).toBeNull();
    expect(m.segmentPrepare).toHaveBeenCalledWith("/photos/a.jpg", "hash-a");
    expect(segment.phase).toBe("ready");
  });

  it("a failed download shows the reason and Try again returns to the consent step", async () => {
    m.segmentDownloadModels.mockRejectedValue("failed to download encoder: timed out");
    await A.handleSegmentDownload();
    expect(segment.dialog).toBe("failed");
    expect(segment.dialogError).toContain("timed out");
    A.handleSegmentDialogRetry();
    expect(segment.dialog).toBe("consent");
    expect(segment.dialogError).toBe("");
  });

  it("I have the files installs what the user picked, with the backend's verification", async () => {
    /** @type {any} */ (open).mockResolvedValue(["/x/encoder.onnx", "/x/decoder.onnx"]);
    m.segmentImportModelFiles.mockResolvedValue(READY);
    await A.handleSegmentImport();
    expect(m.segmentImportModelFiles).toHaveBeenCalledWith(["/x/encoder.onnx", "/x/decoder.onnx"]);
    expect(segment.dialog).toBeNull();
    expect(segment.phase).toBe("ready");
  });

  it("importing only one of the two files says the other is still needed", async () => {
    /** @type {any} */ (open).mockResolvedValue("/x/encoder.onnx");
    m.segmentImportModelFiles.mockResolvedValue(MISSING);
    await A.handleSegmentImport();
    expect(segment.dialog).toBe("consent");
    expect(segment.dialogError).toContain("still missing");
  });

  it("a rejected file shows the backend's message", async () => {
    /** @type {any} */ (open).mockResolvedValue("/x/model.onnx");
    m.segmentImportModelFiles.mockRejectedValue("model.onnx is not one of the expected model files");
    await A.handleSegmentImport();
    expect(segment.dialog).toBe("failed");
    expect(segment.dialogError).toContain("not one of the expected");
  });

  it("choosing no file does nothing", async () => {
    /** @type {any} */ (open).mockResolvedValue(null);
    await A.handleSegmentImport();
    expect(m.segmentImportModelFiles).not.toHaveBeenCalled();
    expect(segment.dialog).toBe("consent");
  });

  it("Cancel closes the dialog and leaves the tool", () => {
    segment.dialog = "consent";
    A.handleSegmentDialogCancel();
    expect(segment.dialog).toBeNull();
    expect(masks.activeTool).toBeNull();
  });

  it("Cancel during a download asks the backend to stop it; the download ending 'cancelled' then closes the dialog quietly", async () => {
    /** @type {(e: any) => void} */
    let fail = () => {};
    m.segmentDownloadModels.mockReturnValue(new Promise((_r, rej) => (fail = rej)));
    const running = A.handleSegmentDownload();
    await vi.waitFor(() => expect(segment.dialog).toBe("downloading"));
    A.handleSegmentDialogCancel();
    expect(m.segmentCancelDownload).toHaveBeenCalledTimes(1);
    expect(segment.dialog).toBe("downloading"); // closes only when the download itself ends
    fail("download cancelled");
    await running;
    expect(segment.dialog).toBeNull();
    expect(segment.dialogError).toBe("");
    expect(masks.activeTool).toBeNull();
  });
});

describe("refining a saved Subject mask", () => {
  const prompts = [{ x: 0.3, y: 0.4, positive: true }];
  const saved = () => ({ ...createSegmentMask("SAVED", prompts, 1, "seg-1"), feather: 10, grow: 5 });

  beforeEach(() => {
    develop.editStack = addMask(develop.editStack, saved());
    masks.selectedMaskId = "seg-1";
  });

  it("resumes the saved clicks: the next click refines from the stored logits and edits the same mask", async () => {
    await A.handleSegmentRefine();
    expect(masks.activeTool).toBe("segment");
    expect(segment.phase).toBe("ready");
    expect(segment.prompts).toEqual(prompts);
    expect(segment.maskId).toBe("seg-1");
    m.segmentDecode.mockResolvedValue([cand(0, "REFINED")]);
    await A.handleSegmentClick({ x: 0.6, y: 0.4, positive: false });
    expect(m.segmentDecode).toHaveBeenCalledWith(
      "hash-a",
      [
        { x: 0.3, y: 0.4, positive: true },
        { x: 0.6, y: 0.4, positive: false },
      ],
      "SAVED",
    );
    const masksNow = /** @type {any[]} */ (stackMasks());
    expect(masksNow).toHaveLength(1);
    expect(masksNow[0]).toMatchObject({ id: "seg-1", logits: "REFINED", candidate: 0, feather: 10, grow: 5 });
    expect(masksNow[0].prompts).toHaveLength(2);
  });

  it("Cancel after a refining click puts the saved selection back and keeps the mask", async () => {
    await A.handleSegmentRefine();
    m.segmentDecode.mockResolvedValue([cand(0, "REFINED")]);
    await A.handleSegmentClick({ x: 0.6, y: 0.4, positive: false });
    A.handleSegmentCancel();
    expect(masks.activeTool).toBeNull();
    expect(stackMasks()).toHaveLength(1);
    expect(/** @type {any} */ (stackMasks()[0])).toMatchObject({ logits: "SAVED", candidate: 1, prompts });
  });

  it("Cancel before any click changes nothing", async () => {
    await A.handleSegmentRefine();
    const before = develop.editStack;
    A.handleSegmentCancel();
    expect(develop.editStack).toBe(before);
  });

  it("refines a Subject SHAPE of the selected mask, addressed by the shape's id", async () => {
    develop.editStack = { schema_version: 1, ops: [] };
    const base = createRadialGradientMask({ x: 0.5, y: 0.5 }, 0.3, 0.3);
    const seg = saved();
    develop.editStack = addMask(develop.editStack, base);
    develop.editStack = addModifier(develop.editStack, base.id, createModifier("subtract", seg, "shape-1"));
    masks.selectedMaskId = base.id;
    masks.selectedShapeId = "shape-1";
    await A.handleSegmentRefine();
    expect(segment.maskId).toBe("shape-1");
    m.segmentDecode.mockResolvedValue([cand(2, "SHAPE-REFINED")]);
    await A.handleSegmentClick({ x: 0.2, y: 0.2, positive: true });
    const owner = /** @type {any} */ (develop.editStack.ops[0]);
    expect(listModifiers(owner)[0].shape).toMatchObject({ logits: "SHAPE-REFINED", candidate: 2 });
  });

  it("with the model missing it asks first, and resumes the saved clicks once the model is installed", async () => {
    m.segmentModelStatus.mockResolvedValue(MISSING);
    await A.handleSegmentRefine();
    expect(segment.dialog).toBe("consent");
    m.segmentDownloadModels.mockResolvedValue(READY);
    await A.handleSegmentDownload();
    expect(segment.phase).toBe("ready");
    expect(segment.maskId).toBe("seg-1");
    expect(segment.prompts).toEqual(prompts);
  });

  it("does nothing for a mask with no stored clicks, or any other kind of mask", async () => {
    develop.editStack = { schema_version: 1, ops: [] };
    develop.editStack = addMask(develop.editStack, createSegmentMask("NOPROMPTS", undefined, undefined, "seg-2"));
    masks.selectedMaskId = "seg-2";
    await A.handleSegmentRefine();
    expect(masks.activeTool).toBeNull();
    const radial = createRadialGradientMask({ x: 0.5, y: 0.5 }, 0.3, 0.3);
    develop.editStack = addMask(develop.editStack, radial);
    masks.selectedMaskId = radial.id;
    await A.handleSegmentRefine();
    expect(masks.activeTool).toBeNull();
    expect(m.segmentModelStatus).not.toHaveBeenCalled();
  });
});
