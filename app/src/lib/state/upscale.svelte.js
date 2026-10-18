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
  /** Set when the dialog enlarges several photos at once (RFC-0029 §18): `target` is then the first of them and only
   * says the dialog is open. `items` are the photos that can be enlarged to the chosen size, with their plans (null
   * until the first answer); `skipped` counts the enlargements the selection held, which are left out.
   * @type {null | { photos: { versionId: number, imageId: number, path: string }[], skipped: number, items: null | { photo: { versionId: number, imageId: number, path: string }, plan: import('$lib/api/upscale.js').UpscalePlan }[] }} */
  batch = $state(null);
  settings = $state(defaultSettings());
  /** The result name typed in the dialog; empty = the default. Reset for every photo. */
  fileName = $state("");
  /** @type {import('$lib/api/upscale.js').UpscalePlan | null} */
  plan = $state(null);
  /** Why there is no plan ("a long edge of N px is not larger than the photo's…"), or "". */
  planError = $state("");

  /** The Preview (RFC-0029 §3.4): about 1 MP of the result around `previewCentre`, plain and model, blended on screen
   * by Strength. `previewKey` names the target it was made for, so a different scale discards it.
   * @type {import('$lib/api/upscale.js').UpscalePreview | null} */
  preview = $state(null);
  previewKey = "";
  previewing = $state(false);
  previewError = $state("");
  /** Where in the photo the preview is centred, 0..1 on both axes. */
  previewCentre = $state(/** @type {[number, number]} */ ([0.5, 0.5]));

  /** Model consent / progress / failure dialog; null = closed.
   * @type {null | "consent" | "downloading" | "failed"} */
  modelDialog = $state(null);
  /** @type {import('$lib/api/upscale.js').UpscaleModelStatus | null} */
  modelStatus = $state(null);
  download = $state({ downloaded: 0, total: 0 });
  dialogError = $state("");
  /** The jobs the model dialog was opened for; they start once the model is installed.
   * @type {null | { queue: { photo: { versionId: number, imageId: number, path: string }, name: string, scale: number }[], options: import('$lib/api/upscale.js').UpscaleRunOptions }} */
  pendingRun = null;

  /** The running job, or null. One at a time (a batch runs its photos one after another); it keeps running when the
   * dialog is closed.
   * @type {null | { label: string, done: number, total: number }} */
  job = $state(null);
  /** A failure shown in the dialog when it is still open. */
  error = $state("");
}

export const upscale = new UpscaleStore();
