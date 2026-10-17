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
import { targetOf, targetKey } from "$lib/enlarge.js";
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
  upscale.fileName = "";
  discardPreview();
  upscale.previewCentre = [0.5, 0.5];
  upscale.plan = null;
  upscale.planError = "";
  upscale.error = "";
  upscale.target = { versionId: image.version_id, imageId: image.image_id, path: image.path };
  // The dialog says up front whether the model is installed; a failed lookup is retried when Enlarge is pressed.
  upscaleModelStatus()
    .then((s) => (upscale.modelStatus = s))
    .catch(() => {});
  void refreshEnlargePlan();
}

/** Closes the dialog without doing anything (a running preview is stopped, its pictures deleted). */
export function closeEnlargeDialog() {
  if (upscale.previewing) upscaleCancel().catch(() => {});
  discardPreview();
  upscalePreviewClear().catch(() => {});
  upscale.target = null;
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
  if (!photo || !target || upscale.plan === null || upscale.job !== null || upscale.previewing) return;
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

/** The dialog's Enlarge button. */
export async function handleEnlarge() {
  const photo = upscale.target;
  const target = targetOf(upscale.settings);
  if (!photo || !target || upscale.plan === null || upscale.job !== null || upscale.previewing) return;
  /** @type {import('$lib/api/upscale.js').UpscaleRunOptions} */
  const options = {
    target,
    strength: Math.min(100, Math.max(1, Math.round(upscale.settings.strength))),
    jpegQuality: upscale.settings.quality,
    fileName: upscale.fileName.trim() === "" ? null : upscale.fileName.trim(),
    addToCollections: upscale.settings.addToCollections,
    metadataNote: upscale.settings.metadataNote,
  };
  const label = baseName(photo.path);
  const scale = Math.max(upscale.plan.out_width, upscale.plan.out_height) / Math.max(upscale.plan.source_width, upscale.plan.source_height);
  upscale.error = "";
  try {
    const status = await upscaleModelStatus();
    upscale.modelStatus = status;
    if (status.state !== "ready") {
      // Ask first; the job starts when the model is in. The Enlarge dialog stays open underneath.
      upscale.pendingRun = { versionId: photo.versionId, label, scale, options };
      upscale.dialogError = "";
      upscale.modelDialog = "consent";
      return;
    }
  } catch (e) {
    upscale.error = message(e);
    return;
  }
  closeEnlargeDialog();
  await runJob(photo, label, scale, options);
}

/** @param {{ versionId: number, imageId: number, path: string }} photo @param {string} label
 * @param {number} scale the planned enlargement factor @param {import('$lib/api/upscale.js').UpscaleRunOptions} options */
async function runJob(photo, label, scale, options) {
  upscale.job = { label, done: 0, total: 0 };
  /** @type {(() => void) | null} */
  let unlisten = null;
  try {
    unlisten = await onUpscaleProgress((p) => {
      if (upscale.job) {
        upscale.job.done = p.done;
        upscale.job.total = p.total;
      }
    });
    const r = await upscaleRun(photo.versionId, options);
    await finished(photo, label, scale, options, r);
  } catch (e) {
    if (isUpscaleJobCancelled(e)) shell.notify("Enlargement stopped. Nothing was written.");
    else shell.notify(`Enlarge failed: ${message(e)}`);
  } finally {
    unlisten?.();
    upscale.job = null;
  }
}

/** The new photo is in the catalog: show it in the list, and select it when the user is still looking at the
 * original (anywhere else, moving their selection would be a surprise).
 * @param {{ versionId: number }} photo @param {string} label @param {number} scale
 * @param {import('$lib/api/upscale.js').UpscaleRunOptions} options @param {import('$lib/api/upscale.js').UpscaleJobResult} r */
async function finished(photo, label, scale, options, r) {
  const stillOnOriginal = shell.activeModule === "library" && selection.selectedIds.size === 1 && selection.selectedIds.has(photo.versionId);
  await refresh();
  if (options.addToCollections) {
    await refreshCollections();
    library.manualMembership = new Map();
    if (library.activeCollectionId !== null) await selectCollection(library.activeCollectionId);
  }
  prioritizeThumbnail(r.version_id);
  if (stillOnOriginal) await revealPhotos([r.image_id]);
  shell.notify(`Enlarged ${label} ${scaleLabel(scale)} (${r.width} × ${r.height} px) in ${formatDuration(r.ms)}.`);
}

/** Stop in the status strip: ends the running job at its next tile. Nothing is written from a stopped job. */
export function handleEnlargeCancel() {
  if (upscale.job === null) return;
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
  const photo = upscale.target;
  upscale.pendingRun = null;
  if (!pending || !photo || photo.versionId !== pending.versionId) return;
  closeEnlargeDialog();
  await runJob(photo, pending.label, pending.scale, pending.options);
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
