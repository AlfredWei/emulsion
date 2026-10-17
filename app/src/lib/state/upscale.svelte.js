// Enlarge (AI) session state (RFC-0029 slice 4d): the photo the dialog is open for, the form's settings (kept
// between uses), the plan under the form, the model consent / download dialog, and the running job. Pure view
// state, never persisted; what is kept is the enlargement itself, a new photo in the catalog.
//
// The workflows that call the backend are actions (actions/enlargeActions.js), the same split as denoise.svelte.js.

import { defaultSettings } from "$lib/enlarge.js";

export class UpscaleStore {
  /** The photo the dialog is open for; null = closed.
   * @type {null | { versionId: number, imageId: number, path: string }} */
  target = $state(null);
  settings = $state(defaultSettings());
  /** The result name typed in the dialog; empty = the default. Reset for every photo. */
  fileName = $state("");
  /** @type {import('$lib/api/upscale.js').UpscalePlan | null} */
  plan = $state(null);
  /** Why there is no plan ("a long edge of N px is not larger than the photo's…"), or "". */
  planError = $state("");

  /** Model consent / progress / failure dialog; null = closed.
   * @type {null | "consent" | "downloading" | "failed"} */
  modelDialog = $state(null);
  /** @type {import('$lib/api/upscale.js').UpscaleModelStatus | null} */
  modelStatus = $state(null);
  download = $state({ downloaded: 0, total: 0 });
  dialogError = $state("");
  /** The job the model dialog was opened for; it starts once the model is installed.
   * @type {null | { versionId: number, label: string, scale: number, options: import('$lib/api/upscale.js').UpscaleRunOptions }} */
  pendingRun = null;

  /** The running job, or null. One at a time; it keeps running when the dialog is closed.
   * @type {null | { label: string, done: number, total: number }} */
  job = $state(null);
  /** A failure shown in the dialog when it is still open. */
  error = $state("");
}

export const upscale = new UpscaleStore();
