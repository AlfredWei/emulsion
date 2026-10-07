// Click-select (Select Subject) session state, RFC-0026 slice 2c: where the tool is in its
// check-model / prepare / click cycle, the clicks so far, the candidates the last click produced,
// the mask or shape being built, and the model consent/download dialog. Pure view state, never
// persisted: what is kept in the edit stack is the `segment_mask` itself (logits + prompts).
//
// The workflows that also write the edit stack or call the helper are actions
// (actions/segmentActions.js), same split as masks.svelte.js. `install()` registers the one
// self-cleaning effect (the session is only valid while its tool is active, on the image it was
// prepared for) and is called once from a component's <script>, next to `masks.install()`.

import { segmentRelease } from "$lib/api/segment.js";
import { masks } from "./masks.svelte.js";
import { develop } from "./develop.svelte.js";

export class SegmentStore {
  /** @param {import('./masks.svelte.js').MaskStore} masks @param {import('./develop.svelte.js').DevelopStore} develop */
  constructor(masks, develop) {
    this.masks = masks;
    this.develop = develop;
  }

  /** `checking` model files -> `preparing` (encoder) -> `ready` for clicks -> `deciding` (a click is
   * being decoded) -> back to `ready`; `failed` shows `error` and a click retries.
   * @type {"idle" | "checking" | "preparing" | "ready" | "deciding" | "failed"} */
  phase = $state("idle");
  error = $state("");
  /** @type {import('$lib/api/develop.js').SegmentPrompt[]} */
  prompts = $state([]);
  /** @type {import('$lib/api/segment.js').SegmentCandidate[]} */
  candidates = $state([]);
  candidateIndex = $state(0);
  /** The mask (or shape) this session created and keeps refining; null until the first click lands. */
  maskId = $state(/** @type {string | null} */ (null));
  /** Content hash the helper currently holds an embedding for (so it can be released on image change). */
  preparedHash = /** @type {string | null} */ (null);
  /** Bumped on every reset so an answer for an abandoned session is ignored. */
  generation = 0;

  /** Consent / progress / failure dialog for the model files; null = closed.
   * @type {null | "consent" | "downloading" | "failed"} */
  dialog = $state(null);
  /** @type {import('$lib/api/segment.js').SegmentModelStatus | null} */
  modelStatus = $state(null);
  download = $state({ downloaded: 0, total: 0 });
  dialogError = $state("");

  busy = $derived(this.phase === "checking" || this.phase === "preparing" || this.phase === "deciding");

  /** Forgets the click session (not the mask it made, and not the dialog). */
  reset() {
    this.generation++;
    this.phase = "idle";
    this.error = "";
    this.prompts = [];
    this.candidates = [];
    this.candidateIndex = 0;
    this.maskId = null;
  }

  install() {
    // The session ends with the tool; the helper's embedding with the image it belongs to.
    $effect(() => {
      if (this.masks.activeTool !== "segment" && (this.phase !== "idle" || this.maskId !== null)) this.reset();
    });
    $effect(() => {
      const hash = this.develop.imageContentHash;
      if (this.preparedHash !== null && hash !== this.preparedHash) {
        this.preparedHash = null;
        this.reset();
        segmentRelease().catch(() => {});
      }
    });
  }
}

/** @param {import('./masks.svelte.js').MaskStore} m @param {import('./develop.svelte.js').DevelopStore} d */
export function createSegmentStore(m, d) {
  return new SegmentStore(m, d);
}

export const segment = createSegmentStore(masks, develop);
