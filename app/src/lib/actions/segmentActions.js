// Select Subject (click-to-select, RFC-0026 slice 2c): arming the tool, the model consent /
// download / import dialog, preparing the image, turning clicks into a `segment_mask` (or a
// segment shape of the selected mask), switching between the model's candidates, and accepting or
// abandoning the result. Each writes the edit stack (`develop`), the tool state (`masks`) and the
// session state (`segment`), so they are actions, like maskActions.js (which they build on).
//
// The mask is created by the FIRST click that decodes and then refined in place by later clicks and
// candidate changes (so the user sees the real red overlay, with nothing special for a "preview");
// Esc removes it again, Enter / toggling the tool off keeps it.

import { open } from "@tauri-apps/plugin-dialog";
import { develop } from "$lib/state/develop.svelte.js";
import { masks } from "$lib/state/masks.svelte.js";
import { segment } from "$lib/state/segment.svelte.js";
import {
  segmentModelStatus,
  segmentDownloadModels,
  segmentImportModelFiles,
  segmentPrepare,
  segmentDecode,
  onSegmentModelProgress,
  isNotPrepared,
} from "$lib/api/segment.js";
import { createSegmentMask, findModifierOwner, updateMask, updateModifier } from "$lib/api/develop.js";
import { handleMaskCreated, handleAddShape, handleShapeRemoved, handleMaskDeleted } from "./maskActions.js";

/** @param {unknown} e */
function message(e) {
  const s = String(e);
  return s.replace(/^Error:\s*/, "");
}

/** Select Subject button: arms the tool (checking the model first), or keeps what it made and leaves. */
export async function handleSegmentToolToggle() {
  if (masks.activeTool === "segment") {
    if (segment.dialog !== null) handleSegmentDialogCancel();
    else handleSegmentAccept();
    return;
  }
  masks.shapeTarget = null;
  masks.activeTool = "segment";
  await startSegmentSession();
}

/** Add shape ▸ Subject in the mask panel: the next decoded click becomes a shape of the selected mask. */
export async function handleAddSegmentShape(/** @type {import('$lib/api/develop.js').Combine} */ combine) {
  handleAddShape(combine, "segment_mask");
  if (masks.activeTool !== "segment") return; // blocked (a cap), nothing armed
  await startSegmentSession();
}

/** Checks the model files and, when they are there, prepares the current image. */
export async function startSegmentSession() {
  segment.reset();
  const gen = segment.generation;
  segment.phase = "checking";
  try {
    const status = await segmentModelStatus();
    if (gen !== segment.generation) return;
    segment.modelStatus = status;
    if (status.state !== "ready") {
      segment.phase = "idle";
      segment.dialog = "consent";
      return;
    }
  } catch (e) {
    if (gen !== segment.generation) return;
    segment.phase = "failed";
    segment.error = message(e);
    return;
  }
  await prepareSegmentImage(gen);
}

/** @param {number} gen */
async function prepareSegmentImage(gen) {
  const hash = develop.imageContentHash;
  if (!develop.imagePath || !hash) {
    segment.phase = "failed";
    segment.error = "This image has no preview to select from yet.";
    return false;
  }
  segment.phase = "preparing";
  try {
    await segmentPrepare(develop.imagePath, hash);
  } catch (e) {
    if (gen !== segment.generation) return false;
    segment.phase = "failed";
    segment.error = message(e);
    return false;
  }
  if (gen !== segment.generation) return false;
  segment.preparedHash = hash;
  segment.phase = "ready";
  return true;
}

/** A canvas click while the tool is armed: `positive` adds to the selection, a negative click (Alt)
 * removes from it. Refines from the candidate currently chosen.
 * @param {{ x: number, y: number, positive: boolean }} click */
export async function handleSegmentClick(click) {
  if (segment.dialog !== null) return;
  if (segment.phase === "failed") {
    // A click retries a failed prepare (the message told the user why) rather than being swallowed.
    await startSegmentSession();
    return;
  }
  if (segment.phase !== "ready") return;
  const gen = segment.generation;
  const hash = develop.imageContentHash;
  if (!hash) return;
  const prompts = [...segment.prompts, { x: click.x, y: click.y, positive: click.positive }];
  const refine = segment.candidates[segment.candidateIndex]?.logits_png ?? null;
  segment.phase = "deciding";
  segment.error = "";
  /** @type {import('$lib/api/segment.js').SegmentCandidate[]} */
  let candidates;
  try {
    try {
      candidates = await segmentDecode(hash, prompts, refine);
    } catch (e) {
      if (!isNotPrepared(e)) throw e;
      // The helper restarted (idle exit) since the image was prepared: prepare again, decode once more.
      if (gen !== segment.generation) return;
      if (!(await prepareSegmentImage(gen))) return;
      segment.phase = "deciding";
      candidates = await segmentDecode(hash, prompts, refine);
    }
  } catch (e) {
    if (gen !== segment.generation) return;
    // The click that failed is dropped; the earlier selection stays as it was.
    segment.phase = "ready";
    segment.error = message(e);
    return;
  }
  if (gen !== segment.generation) return;
  if (candidates.length === 0) {
    segment.phase = "ready";
    segment.error = "The model returned no selection for that click.";
    return;
  }
  segment.prompts = prompts;
  segment.candidates = candidates;
  segment.candidateIndex = 0;
  writeCandidate();
  segment.phase = "ready";
}

/** Writes the chosen candidate (and the clicks) into the mask this session owns, creating it on the
 * first call. */
function writeCandidate() {
  const c = segment.candidates[segment.candidateIndex];
  if (!c) return;
  const prompts = segment.prompts.map((p) => ({ ...p }));
  if (segment.maskId === null) {
    const mask = createSegmentMask(c.logits_png, prompts, c.index);
    segment.maskId = mask.id;
    handleMaskCreated({ kind: "segment", mask });
    return;
  }
  const id = segment.maskId;
  const patch = { logits: c.logits_png, prompts, candidate: c.index };
  const owner = findModifierOwner(develop.editStack, id);
  develop.editStack =
    owner !== null
      ? updateModifier(develop.editStack, owner, id, { shape: /** @type {any} */ (patch) })
      : updateMask(develop.editStack, id, /** @type {any} */ (patch));
  develop.scheduleFlush("Select Subject");
}

/** Chooses another of the model's candidates for the same clicks (the panel's 1 / 2 / 3 or Tab). */
export function handleSegmentCandidate(/** @type {number} */ index) {
  if (index < 0 || index >= segment.candidates.length || index === segment.candidateIndex) return;
  segment.candidateIndex = index;
  writeCandidate();
}

/** Tab: the next candidate, wrapping. */
export function handleSegmentCycleCandidate() {
  const n = segment.candidates.length;
  if (n > 1) handleSegmentCandidate((segment.candidateIndex + 1) % n);
}

/** Enter / the button / toggling the tool off: keep the mask, leave the tool. */
export function handleSegmentAccept() {
  masks.activeTool = null;
  masks.shapeTarget = null;
}

/** Esc: remove what this session made and leave the tool. */
export function handleSegmentCancel() {
  const id = segment.maskId;
  masks.activeTool = null;
  masks.shapeTarget = null;
  if (id === null) return;
  if (findModifierOwner(develop.editStack, id) !== null) {
    handleShapeRemoved(id);
  } else if (masks.selectedMaskId === id) {
    handleMaskDeleted();
  }
}

// ---------------------------------------------------------------------------
// The model consent / download / import dialog (RFC-0026 §3.5)
// ---------------------------------------------------------------------------

/** "Download": the one scoped network use, after the user agreed in the dialog. */
export async function handleSegmentDownload() {
  segment.dialog = "downloading";
  segment.dialogError = "";
  segment.download = { downloaded: 0, total: segment.modelStatus?.download_bytes ?? 0 };
  /** @type {(() => void) | null} */
  let unlisten = null;
  try {
    unlisten = await onSegmentModelProgress((p) => (segment.download = p));
    const status = await segmentDownloadModels();
    segment.modelStatus = status;
    await modelsInstalled(status);
  } catch (e) {
    segment.dialog = "failed";
    segment.dialogError = message(e);
  } finally {
    unlisten?.();
  }
}

/** "I have the files": the user picks the encoder and decoder .onnx files; they are checked by name
 * and checksum in the backend before they are copied in. */
export async function handleSegmentImport() {
  segment.dialogError = "";
  const picked = await open({ multiple: true, filters: [{ name: "ONNX model files", extensions: ["onnx"] }] });
  if (picked === null) return;
  const paths = Array.isArray(picked) ? picked : [picked];
  try {
    const status = await segmentImportModelFiles(paths);
    segment.modelStatus = status;
    if (status.state === "ready") await modelsInstalled(status);
    else segment.dialogError = "Those files were accepted, but one of the two model files is still missing. Choose both files.";
  } catch (e) {
    segment.dialog = "failed";
    segment.dialogError = message(e);
  }
}

/** @param {import('$lib/api/segment.js').SegmentModelStatus} status */
async function modelsInstalled(status) {
  if (status.state !== "ready") {
    segment.dialog = "failed";
    segment.dialogError = "The download finished but the files did not verify.";
    return;
  }
  segment.dialog = null;
  if (masks.activeTool === "segment") await prepareSegmentImage(segment.generation);
}

/** Cancel / Esc / Not now: closes the dialog and leaves the tool (nothing was downloaded). */
export function handleSegmentDialogCancel() {
  if (segment.dialog === "downloading") return; // the download cannot be interrupted yet
  segment.dialog = null;
  segment.dialogError = "";
  masks.activeTool = null;
  masks.shapeTarget = null;
}

/** The dialog's "Try again" after a failed download or import. */
export function handleSegmentDialogRetry() {
  segment.dialog = "consent";
  segment.dialogError = "";
}
