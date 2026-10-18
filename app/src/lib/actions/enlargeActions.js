// Enlarge (AI) (RFC-0029 slice 4d): opening the dialog, the plan under its form, the job, and the model consent /
// download / import dialog. Each writes the session state (`upscale`) and the library, so they are actions, like
// denoiseActions.js.
//
// A job is always something the user asked for, one at a time. It keeps running when the dialog is closed (the
// dialog closes the moment it starts): progress is in the status strip, and it ends in a status line and a new
// photo in the catalog, connected to the original.

import { open } from "@tauri-apps/plugin-dialog";
import { upscale } from "$lib/state/upscale.svelte.js";
import { library } from "$lib/state/library.svelte.js";
import { selection } from "$lib/state/selection.svelte.js";
import { shell } from "$lib/state/shell.svelte.js";
import {
  upscaleModelStatus,
  upscaleDownloadModel,
  upscaleCancelDownload,
  upscaleImportModel,
  upscalePlan,
  upscalePreview,
  upscalePreviewClear,
  upscaleRun,
  upscaleCancel,
  onUpscaleModelProgress,
  onUpscaleProgress,
  isUpscaleDownloadCancelled,
  isUpscaleJobCancelled,
} from "$lib/api/upscale.js";
import { refresh, refreshCollections, selectCollection, prioritizeThumbnail } from "$lib/actions/libraryActions.js";
import { revealPhotos } from "$lib/actions/upscaleActions.js";
import { targetOf, targetKey, splitEnlargeable, jobLabel, batchOutcome } from "$lib/enlarge.js";
import { scaleLabel } from "$lib/upscaleLinks.js";
import { formatDuration } from "$lib/actions/denoiseActions.js";

/** @param {unknown} e */
function message(e) {
  return String(e).replace(/^Error:\s*/, "");
}

/** @param {string} path */
function baseName(path) {
  return path.split(/[\\/]/).pop() || path;
}

/** Why Enlarge is not offered for this photo, or "" when it is. The same words are the menu's reason.
 * @param {import('$lib/api/catalog.js').ImageSummary} image */
export function enlargeBlockedReason(image) {
  if (image.enlarged_scale != null) return "Enlarge (AI) is turned off on an enlargement, to avoid compounding invented detail";
  return "";
}

/** Opens the dialog for one photo. @param {import('$lib/api/catalog.js').ImageSummary} image */
export function openEnlargeDialog(image) {
  const blocked = enlargeBlockedReason(image);
  if (blocked) {
    shell.notify(blocked);
    return;
  }
  if (upscale.job !== null) {
    shell.notify("An enlargement is already running");
    return;
  }
  resetForOpen();
  upscale.target = toTarget(image);
  // The dialog says up front whether the model is installed; a failed lookup is retried when Enlarge is pressed.
  upscaleModelStatus()
    .then((s) => (upscale.modelStatus = s))
    .catch(() => {});
  void refreshEnlargePlan();
}

/** The entry point of the menu and the info panel: one photo opens the dialog for it; several open it for the batch
 * (RFC-0029 §18), leaving out the enlargements among them.
 * @param {import('$lib/api/catalog.js').ImageSummary[]} images */
export function openEnlarge(images) {
  if (images.length === 1) {
    openEnlargeDialog(images[0]);
    return;
  }
  const { photos, skipped } = splitEnlargeable(images);
  if (photos.length === 0) {
    shell.notify("Enlarge (AI) is turned off on an enlargement, to avoid compounding invented detail");
    return;
  }
  if (photos.length === 1) {
    shell.notify(`${skipped} enlargement${skipped === 1 ? " was" : "s were"} left out.`);
    openEnlargeDialog(photos[0]);
    return;
  }
  if (upscale.job !== null) {
    shell.notify("An enlargement is already running");
    return;
  }
  resetForOpen();
  upscale.batch = { photos: photos.map(toTarget), skipped, items: null };
  upscale.target = upscale.batch.photos[0];
  upscaleModelStatus()
    .then((s) => (upscale.modelStatus = s))
    .catch(() => {});
  void refreshEnlargePlan();
}

/** @param {import('$lib/api/catalog.js').ImageSummary} image */
function toTarget(image) {
  return { versionId: image.version_id, imageId: image.image_id, path: image.path };
}

function resetForOpen() {
  upscale.fileName = "";
  discardPreview();
  upscale.previewCentre = [0.5, 0.5];
  upscale.plan = null;
  upscale.planError = "";
  upscale.error = "";
  upscale.batch = null;
}

/** Closes the dialog without doing anything (a running preview is stopped, its pictures deleted). */
export function closeEnlargeDialog() {
  if (upscale.previewing) upscaleCancel().catch(() => {});
  discardPreview();
  upscalePreviewClear().catch(() => {});
  upscale.target = null;
  upscale.batch = null;
  upscale.error = "";
}

function discardPreview() {
  upscale.preview = null;
  upscale.previewKey = "";
  upscale.previewError = "";
}

/** The dialog's Preview: about 1 MP of the result at 1:1, around `centre` (the navigator's click) or where the last one
 * was. Strength is not an input: the dialog blends the two pictures on screen, so moving the slider never re-runs it.
 * @param {[number, number]} [centre] */
export async function handleEnlargePreview(centre) {
  const photo = upscale.target;
  const target = targetOf(upscale.settings);
  if (!photo || !target || upscale.batch !== null || upscale.plan === null || upscale.job !== null || upscale.previewing) return;
  if (centre) upscale.previewCentre = centre;
  upscale.previewError = "";
  if (upscale.modelStatus?.state === "missing") {
    upscale.previewError = "The enlargement model is not installed yet. Press Enlarge to install it (you will be asked first).";
    return;
  }
  const key = targetKey(target);
  upscale.previewing = true;
  try {
    const result = await upscalePreview(photo.versionId, target, upscale.previewCentre);
    // The dialog may have been closed, or moved to another photo or scale, while it ran.
    if (upscale.target?.versionId === photo.versionId && targetKey(targetOf(upscale.settings)) === key) {
      upscale.preview = result;
      upscale.previewKey = key;
    }
  } catch (e) {
    if (!isUpscaleJobCancelled(e)) upscale.previewError = message(e);
  } finally {
    upscale.previewing = false;
  }
}

let planSeq = 0;

/** Recomputes the numbers under the form for the photo, target and quality now set. Cheap (arithmetic only), so it
 * runs on every change. Answers that arrive late for an earlier form are dropped. */
export async function refreshEnlargePlan() {
  const seq = ++planSeq;
  const photo = upscale.target;
  const target = targetOf(upscale.settings);
  // A preview belongs to the scale it was made for.
  if (upscale.preview && upscale.previewKey !== targetKey(target)) discardPreview();
  if (!photo || !target) {
    upscale.plan = null;
    upscale.planError = "";
    if (upscale.batch) upscale.batch.items = null;
    return;
  }
  const batch = upscale.batch;
  if (batch) {
    // Pure arithmetic per photo; the ones that cannot take the size (a long edge below theirs) drop out of the totals.
    const answers = await Promise.all(
      batch.photos.map((p) =>
        upscalePlan(p.versionId, target, upscale.settings.quality).then(
          (plan) => ({ photo: p, plan, error: "" }),
          (e) => ({ photo: p, plan: null, error: message(e) }),
        ),
      ),
    );
    if (seq !== planSeq || upscale.batch !== batch) return;
    const items = answers.flatMap((a) => (a.plan ? [{ photo: a.photo, plan: a.plan }] : []));
    batch.items = items;
    upscale.planError = items.length === 0 ? (answers[0]?.error ?? "") : "";
    return;
  }
  try {
    const plan = await upscalePlan(photo.versionId, target, upscale.settings.quality);
    if (seq !== planSeq) return;
    upscale.plan = plan;
    upscale.planError = "";
  } catch (e) {
    if (seq !== planSeq) return;
    upscale.plan = null;
    upscale.planError = message(e);
  }
}

/** The dialog's Enlarge button: one photo, or every photo of a batch that can take the chosen size. */
export async function handleEnlarge() {
  const photo = upscale.target;
  const target = targetOf(upscale.settings);
  if (!photo || !target || upscale.job !== null || upscale.previewing) return;
  const planned = upscale.batch ? upscale.batch.items : upscale.plan ? [{ photo, plan: upscale.plan }] : null;
  if (!planned || planned.length === 0) return;
  /** @type {import('$lib/api/upscale.js').UpscaleRunOptions} */
  const options = {
    target,
    strength: Math.min(100, Math.max(1, Math.round(upscale.settings.strength))),
    jpegQuality: upscale.settings.quality,
    // A batch names every result by the default rule (the backend avoids clashes).
    fileName: upscale.batch || upscale.fileName.trim() === "" ? null : upscale.fileName.trim(),
    addToCollections: upscale.settings.addToCollections,
    metadataNote: upscale.settings.metadataNote,
  };
  const queue = planned.map(({ photo: p, plan }) => ({
    photo: p,
    name: baseName(p.path),
    scale: Math.max(plan.out_width, plan.out_height) / Math.max(plan.source_width, plan.source_height),
  }));
  upscale.error = "";
  try {
    const status = await upscaleModelStatus();
    upscale.modelStatus = status;
    if (status.state !== "ready") {
      // Ask first; the jobs start when the model is in. The Enlarge dialog stays open underneath.
      upscale.pendingRun = { queue, options };
      upscale.dialogError = "";
      upscale.modelDialog = "consent";
      return;
    }
  } catch (e) {
    upscale.error = message(e);
    return;
  }
  closeEnlargeDialog();
  await runJobs(queue, options);
}

/** Set by Stop; the batch loop reads it between photos (the running photo is stopped through the helper). */
let stopRequested = false;

/** Enlarges the queued photos one after another (RFC-0029 §18). One photo that fails does not stop the others; Stop
 * does, and what was finished stays. A single photo gets the single-photo wording.
 * @param {{ photo: { versionId: number, imageId: number, path: string }, name: string, scale: number }[]} queue
 * @param {import('$lib/api/upscale.js').UpscaleRunOptions} options */
async function runJobs(queue, options) {
  const count = queue.length;
  // Move the selection to the results only when the user is still looking at what was enlarged.
  const asked = new Set(queue.map((q) => q.photo.versionId));
  const stillOnSelection =
    shell.activeModule === "library" && selection.selectedIds.size === asked.size && [...selection.selectedIds].every((id) => asked.has(id));
  stopRequested = false;
  upscale.job = { label: jobLabel(queue[0].name, 0, count), done: 0, total: 0 };
  const started = Date.now();
  /** @type {import('$lib/api/upscale.js').UpscaleJobResult[]} */
  const results = [];
  /** @type {{ name: string, reason: string }[]} */
  const failed = [];
  let stopped = false;
  /** @type {(() => void) | null} */
  let unlisten = null;
  try {
    unlisten = await onUpscaleProgress((p) => {
      if (upscale.job) {
        upscale.job.done = p.done;
        upscale.job.total = p.total;
      }
    });
    for (let i = 0; i < count; i++) {
      if (stopRequested) {
        stopped = true;
        break;
      }
      upscale.job = { label: jobLabel(queue[i].name, i, count), done: 0, total: 0 };
      try {
        results.push(await upscaleRun(queue[i].photo.versionId, options));
      } catch (e) {
        if (isUpscaleJobCancelled(e)) {
          stopped = true;
          break;
        }
        failed.push({ name: queue[i].name, reason: message(e) });
      }
    }
  } catch (e) {
    failed.push({ name: queue[0].name, reason: message(e) });
  } finally {
    unlisten?.();
    upscale.job = null;
  }
  if (results.length > 0) await showResults(results, options, stillOnSelection);
  const ms = Date.now() - started;
  if (count === 1) {
    if (results.length === 1) {
      const r = results[0];
      shell.notify(`Enlarged ${queue[0].name} ${scaleLabel(queue[0].scale)} (${r.width} × ${r.height} px) in ${formatDuration(r.ms)}.`);
    } else if (failed.length > 0) shell.notify(`Enlarge failed: ${failed[0].reason}`);
    else shell.notify("Enlargement stopped. Nothing was written.");
  } else {
    shell.notify(batchOutcome({ done: results.length, count, failed, stopped, ms }, formatDuration));
  }
}

/** The new photos are in the catalog: show them in the list, and select them when the user is still looking at the
 * originals (anywhere else, moving their selection would be a surprise).
 * @param {import('$lib/api/upscale.js').UpscaleJobResult[]} results @param {import('$lib/api/upscale.js').UpscaleRunOptions} options
 * @param {boolean} select */
async function showResults(results, options, select) {
  await refresh();
  if (options.addToCollections) {
    await refreshCollections();
    library.manualMembership = new Map();
    if (library.activeCollectionId !== null) await selectCollection(library.activeCollectionId);
  }
  for (const r of results) prioritizeThumbnail(r.version_id);
  if (select) await revealPhotos(results.map((r) => r.image_id));
}

/** Stop in the status strip: ends the running job at its next tile, and the rest of a batch with it. Nothing is
 * written from the stopped photo; the ones already finished stay. */
export function handleEnlargeCancel() {
  if (upscale.job === null) return;
  stopRequested = true;
  upscaleCancel().catch(() => {});
}

// ---------------------------------------------------------------------------
// The model consent / download / import dialog (RFC-0029 §3.5)
// ---------------------------------------------------------------------------

/** "Download": the one scoped network use, after the user agreed in the dialog. */
export async function handleEnlargeModelDownload() {
  upscale.modelDialog = "downloading";
  upscale.dialogError = "";
  upscale.download = { downloaded: 0, total: upscale.modelStatus?.download_bytes ?? 0 };
  /** @type {(() => void) | null} */
  let unlisten = null;
  try {
    unlisten = await onUpscaleModelProgress((p) => (upscale.download = p));
    const status = await upscaleDownloadModel();
    upscale.modelStatus = status;
    await modelInstalled(status);
  } catch (e) {
    if (isUpscaleDownloadCancelled(e)) {
      // The user's own Cancel: back out quietly. A partial file is kept and the next download resumes it.
      upscale.modelDialog = null;
      upscale.pendingRun = null;
    } else {
      upscale.modelDialog = "failed";
      upscale.dialogError = message(e);
    }
  } finally {
    unlisten?.();
  }
}

/** "I have the file": the user picks the .onnx file; it is checked by name and checksum before it is copied in. */
export async function handleEnlargeModelImport() {
  upscale.dialogError = "";
  const picked = await open({ multiple: false, filters: [{ name: "ONNX model file", extensions: ["onnx"] }] });
  if (picked === null) return;
  const path = Array.isArray(picked) ? picked[0] : picked;
  try {
    const status = await upscaleImportModel(path);
    upscale.modelStatus = status;
    await modelInstalled(status);
  } catch (e) {
    upscale.modelDialog = "failed";
    upscale.dialogError = message(e);
  }
}

/** @param {import('$lib/api/upscale.js').UpscaleModelStatus} status */
async function modelInstalled(status) {
  if (status.state !== "ready") {
    upscale.modelDialog = "failed";
    upscale.dialogError = "The file arrived but did not verify.";
    return;
  }
  upscale.modelDialog = null;
  const pending = upscale.pendingRun;
  upscale.pendingRun = null;
  if (!pending || !upscale.target) return;
  closeEnlargeDialog();
  await runJobs(pending.queue, pending.options);
}

/** Cancel / Esc on the model dialog: closes it. While a download runs it stops the download instead (the download
 * call then finishes with "download cancelled" and closes the dialog). */
export function handleEnlargeModelCancel() {
  if (upscale.modelDialog === "downloading") {
    upscaleCancelDownload().catch(() => {});
    return;
  }
  upscale.modelDialog = null;
  upscale.dialogError = "";
  upscale.pendingRun = null;
}

/** The dialog's "Try again" after a failed download or import. */
export function handleEnlargeModelRetry() {
  upscale.modelDialog = "consent";
  upscale.dialogError = "";
}
