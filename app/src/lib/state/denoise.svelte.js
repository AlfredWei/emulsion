// AI Denoise session state (RFC-0027 slice 3c): the model consent / download dialog, the kept denoised
// copy of the open photo, the running job, and the last crop preview. Pure view state, never persisted:
// what is kept in the edit stack is the `ai_denoise` amount; what is kept on disk is the helper's result.
//
// The workflows that call the backend are actions (actions/denoiseActions.js), the same split as
// segment.svelte.js. `install()` registers the one effect that follows the open photo (its kept copy is
// looked up whenever the photo changes) and is called once from a component's <script>.

import { untrack } from "svelte";
import { denoiseCacheInfo } from "$lib/api/denoise.js";
import { develop } from "./develop.svelte.js";

export class DenoiseStore {
  /** @param {import('./develop.svelte.js').DevelopStore} develop */
  constructor(develop) {
    this.develop = develop;
  }

  /** Model consent / progress / failure dialog; null = closed.
   * @type {null | "consent" | "downloading" | "failed"} */
  dialog = $state(null);
  /** @type {import('$lib/api/denoise.js').DenoiseModelStatus | null} */
  modelStatus = $state(null);
  download = $state({ downloaded: 0, total: 0 });
  dialogError = $state("");
  /** The job the dialog was opened for, on the photo it was asked on; it starts once the model is installed.
   * @type {null | { kind: "crop" | "whole", hash: string, path: string }} */
  pendingJob = null;

  /** The kept denoised copy of the photo `cacheHash` names (null = none, or not looked up yet).
   * @type {import('$lib/api/denoise.js').DenoiseCacheInfo | null} */
  cache = $state(null);
  cacheHash = $state(/** @type {string | null} */ (null));
  /** Bumped whenever the kept copy of the open photo is made or removed, so the canvas rebuilds its source. */
  cacheVersion = $state(0);

  /** The running job, or null. `hash` is the photo it works on, which may no longer be the open one.
   * @type {null | { kind: "crop" | "whole", hash: string, label: string, done: number, total: number }} */
  job = $state(null);
  error = $state("");
  /** The Export dialog's "denoise these first" run: which photo of how many, and whether Stop was pressed.
   * @type {null | { index: number, total: number, stopped: boolean }} */
  batch = $state(null);
  /** The last crop preview, for the photo `hash`.
   * @type {null | { hash: string, before: string, after: string, region: { x: number, y: number, w: number, h: number }, ms: number }} */
  crop = $state(null);

  /** The kept copy belongs to the open photo (it can be stale for a moment while a new lookup is in flight). */
  hasResult = $derived(this.cache !== null && this.cacheHash !== null && this.cacheHash === this.develop.imageContentHash);
  /** A job is running for the open photo. */
  jobHere = $derived(this.job !== null && this.job.hash === this.develop.imageContentHash);

  /** Looks up the kept copy of the open photo and stores it. A failed lookup reads as "none". */
  async refreshCache() {
    const hash = this.develop.imageContentHash;
    if (!hash) {
      this.cache = null;
      this.cacheHash = null;
      return;
    }
    /** @type {import('$lib/api/denoise.js').DenoiseCacheInfo | null} */
    let info = null;
    try {
      info = await denoiseCacheInfo(hash);
    } catch {
      info = null;
    }
    // The photo may have changed while the lookup ran: that answer belongs to the earlier one.
    if (hash !== this.develop.imageContentHash) return;
    this.cache = info;
    this.cacheHash = hash;
  }

  install() {
    // The kept copy and the crop preview follow the open photo.
    $effect(() => {
      const hash = this.develop.imageContentHash;
      // Only the photo is a dependency: the rest is written here, and reading it would re-run this.
      untrack(() => {
        if (this.cacheHash !== hash) {
          this.cache = null;
          this.cacheHash = hash;
        }
        if (this.crop !== null && this.crop.hash !== hash) this.crop = null;
        this.error = "";
        if (hash) this.refreshCache();
      });
    });
  }
}

/** @param {import('./develop.svelte.js').DevelopStore} d */
export function createDenoiseStore(d) {
  return new DenoiseStore(d);
}

export const denoise = createDenoiseStore(develop);
