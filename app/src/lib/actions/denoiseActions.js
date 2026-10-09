// AI Denoise (RFC-0027 slice 3c): the Amount slider, the crop preview and the whole-photo job, the model
// consent / download / import dialog, and removing the kept copy. Each writes the edit stack (`develop`)
// and the session state (`denoise`), so they are actions, like segmentActions.js.
//
// The model never runs while the slider moves: Amount only mixes the kept copy in (the canvas rebuilds its
// source when the amount settles). A job is always something the user asked for, one at a time, and keeps
// running when they open another photo; it ends in a status line, and in the Amount being set to a starting
// value if the photo has none yet, so the result is visible straight away.

import { open } from "@tauri-apps/plugin-dialog";
import { develop } from "$lib/state/develop.svelte.js";
import { shell } from "$lib/state/shell.svelte.js";
import { denoise } from "$lib/state/denoise.svelte.js";
import {
  denoiseModelStatus,
  denoiseDownloadModel,
  denoiseCancelDownload,
  denoiseImportModel,
  denoiseRun,
  denoiseCancel,
  denoiseRemoveCache,
  onDenoiseModelProgress,
  onDenoiseProgress,
  isDownloadCancelled,
  isJobCancelled,
} from "$lib/api/denoise.js";
import { getAiDenoise, upsertAiDenoise } from "$lib/api/develop.js";

/** What Amount is set to when a photo's first denoise finishes: full strength reads as waxy on many photos
 * (RFC-0027 §6.2), so it starts short of it. A starting point to judge by eye, not a measured optimum. */
export const DEFAULT_AI_DENOISE_AMOUNT = 70;

/** @param {unknown} e */
function message(e) {
  return String(e).replace(/^Error:\s*/, "");
}

/** @param {string} path */
function baseName(path) {
  return path.split(/[\\/]/).pop() || path;
}

/** "42 s" / "1 min 41 s".
 * @param {number} ms */
export function formatDuration(ms) {
  const s = Math.max(1, Math.round(ms / 1000));
  return s < 60 ? `${s} s` : `${Math.floor(s / 60)} min ${s % 60} s`;
}

/** The Amount slider (and its step buttons): an ordinary edit-stack write with its own history label. */
export function handleAiDenoiseAmountChange(/** @type {Partial<{amount: number}>} */ patch) {
  develop.editStack = upsertAiDenoise(develop.editStack, patch);
  develop.scheduleFlush("AI Denoise");
}

/** "Preview on a crop": denoise one rectangle around the middle of the photo and show it before / after. */
export async function handleDenoisePreviewCrop() {
  await startJob("crop");
}

/** "Denoise whole photo". */
export async function handleDenoiseWhole() {
  await startJob("whole");
}

/** Stops the running job at its next tile. Nothing is kept from a stopped job. */
export function handleDenoiseCancel() {
  if (denoise.job === null) return;
  denoiseCancel().catch(() => {});
}

/** Deletes the kept copy of the open photo; the Amount stays in the stack but has nothing to mix. */
export async function handleDenoiseRemove() {
  const hash = develop.imageContentHash;
  if (!hash) return;
  try {
    await denoiseRemoveCache(hash);
  } catch (e) {
    denoise.error = message(e);
    return;
  }
  if (hash !== develop.imageContentHash) return;
  denoise.cache = null;
  denoise.crop = null;
  denoise.cacheVersion++;
}

/** Closes the crop preview. */
export function handleDenoiseDismissCrop() {
  denoise.crop = null;
}

/** @param {"crop" | "whole"} kind */
async function startJob(kind) {
  if (denoise.job !== null) return;
  const hash = develop.imageContentHash;
  const path = develop.imagePath;
  if (!hash || !path) {
    denoise.error = "This photo has no fingerprint yet; try again once it has finished importing.";
    return;
  }
  denoise.error = "";
  try {
    const status = await denoiseModelStatus();
    denoise.modelStatus = status;
    if (status.state !== "ready") {
      // Ask first; the job starts when the model is in, for the photo it was asked on.
      denoise.pendingJob = { kind, hash, path };
      denoise.dialogError = "";
      denoise.dialog = "consent";
      return;
    }
  } catch (e) {
    denoise.error = message(e);
    return;
  }
  await runJob(kind, hash, path);
}

/** @param {"crop" | "whole"} kind @param {string} hash @param {string} path */
async function runJob(kind, hash, path) {
  const label = baseName(path);
  denoise.job = { kind, hash, label, done: 0, total: 0 };
  /** @type {(() => void) | null} */
  let unlisten = null;
  try {
    unlisten = await onDenoiseProgress((p) => {
      if (denoise.job) {
        denoise.job.done = p.done;
        denoise.job.total = p.total;
      }
    });
    const r = await denoiseRun(path, hash, kind === "crop" ? [0.5, 0.5] : null);
    if (kind === "crop") {
      if (!r.before_path || !r.region) throw new Error("the crop preview came back without its pictures");
      if (hash === develop.imageContentHash) denoise.crop = { hash, before: r.before_path, after: r.path, region: r.region, ms: r.ms };
    } else {
      await wholePhotoDone(hash, label, r.ms);
    }
  } catch (e) {
    if (isJobCancelled(e)) {
      shell.notify("Denoise stopped.");
    } else {
      if (hash === develop.imageContentHash) denoise.error = message(e);
      shell.notify(`Denoise failed: ${message(e)}`);
    }
  } finally {
    unlisten?.();
    denoise.job = null;
  }
}

/** @param {string} hash @param {string} label @param {number} ms */
async function wholePhotoDone(hash, label, ms) {
  if (hash === develop.imageContentHash) {
    await denoise.refreshCache();
    denoise.cacheVersion++;
    if (getAiDenoise(develop.editStack).amount === 0) {
      develop.editStack = upsertAiDenoise(develop.editStack, { amount: DEFAULT_AI_DENOISE_AMOUNT });
      develop.scheduleFlush("AI Denoise");
    }
  }
  shell.notify(`Denoised ${label} in ${formatDuration(ms)}.`);
}

// ---------------------------------------------------------------------------
// The model consent / download / import dialog (RFC-0027 §3.5)
// ---------------------------------------------------------------------------

/** "Download": the one scoped network use, after the user agreed in the dialog. */
export async function handleDenoiseDownload() {
  denoise.dialog = "downloading";
  denoise.dialogError = "";
  denoise.download = { downloaded: 0, total: denoise.modelStatus?.download_bytes ?? 0 };
  /** @type {(() => void) | null} */
  let unlisten = null;
  try {
    unlisten = await onDenoiseModelProgress((p) => (denoise.download = p));
    const status = await denoiseDownloadModel();
    denoise.modelStatus = status;
    await modelInstalled(status);
  } catch (e) {
    if (isDownloadCancelled(e)) {
      // The user's own Cancel: back out quietly. A partial file is kept and the next download resumes it.
      denoise.dialog = null;
      denoise.pendingJob = null;
    } else {
      denoise.dialog = "failed";
      denoise.dialogError = message(e);
    }
  } finally {
    unlisten?.();
  }
}

/** "I have the file": the user picks the .onnx file; it is checked by name and checksum before it is copied in. */
export async function handleDenoiseImport() {
  denoise.dialogError = "";
  const picked = await open({ multiple: false, filters: [{ name: "ONNX model file", extensions: ["onnx"] }] });
  if (picked === null) return;
  const path = Array.isArray(picked) ? picked[0] : picked;
  try {
    const status = await denoiseImportModel(path);
    denoise.modelStatus = status;
    await modelInstalled(status);
  } catch (e) {
    denoise.dialog = "failed";
    denoise.dialogError = message(e);
  }
}

/** @param {import('$lib/api/denoise.js').DenoiseModelStatus} status */
async function modelInstalled(status) {
  if (status.state !== "ready") {
    denoise.dialog = "failed";
    denoise.dialogError = "The file arrived but did not verify.";
    return;
  }
  denoise.dialog = null;
  const pending = denoise.pendingJob;
  denoise.pendingJob = null;
  if (pending) await runJob(pending.kind, pending.hash, pending.path);
}

/** Cancel / Esc: closes the dialog. While a download runs it stops the download instead (the download
 * call then finishes with "download cancelled" and closes the dialog). */
export function handleDenoiseDialogCancel() {
  if (denoise.dialog === "downloading") {
    denoiseCancelDownload().catch(() => {});
    return;
  }
  denoise.dialog = null;
  denoise.dialogError = "";
  denoise.pendingJob = null;
}

/** The dialog's "Try again" after a failed download or import. */
export function handleDenoiseDialogRetry() {
  denoise.dialog = "consent";
  denoise.dialogError = "";
}
