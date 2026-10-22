// Similar view state (RFC-0031 slice C): whether the view is on, the Similarity slider, the groups last fetched from the
// backend, how far the background analysis has got, which group is open, and a pending "Reject the others". What is
// derived for display (groups joined with photos) lives here too; the work (IPC, polling, rejecting) is in
// lib/actions/similarActions.js.

import { library } from "./library.svelte.js";
import { buildGroupViews } from "$lib/similar.js";

const STORAGE_KEY = "emulsion_similarity_v1";
export const DEFAULT_SIMILARITY = 0.5;

function storedSimilarity() {
  try {
    const raw = Number(localStorage.getItem(STORAGE_KEY));
    if (localStorage.getItem(STORAGE_KEY) !== null && Number.isFinite(raw)) return Math.min(1, Math.max(0, raw));
  } catch {
    // no storage: the default
  }
  return DEFAULT_SIMILARITY;
}

export class SimilarStore {
  /** @param {import('./library.svelte.js').LibraryStore} library */
  constructor(library) {
    this.library = library;
  }

  /** The Similar entry of the rail is selected. Cleared by every other rail source. */
  active = $state(false);
  similarity = $state(storedSimilarity());
  /** @type {import('$lib/api/cull.js').SimilarGroup[]} */
  groups = $state([]);
  /** @type {{ analysed: number, total: number } | null} */
  status = $state(null);
  loaded = $state(false);
  /** The open group, by its key (first member's image id); null shows the list of groups. */
  openKey = $state(/** @type {number | null} */ (null));
  /** A "Reject the others" waiting for its confirmation. */
  pendingReject = $state(/** @type {{ keepVersionId: number, versionIds: number[] } | null} */ (null));

  groupViews = $derived(buildGroupViews(this.groups, this.library.images));
  openGroup = $derived(this.openKey === null ? null : (this.groupViews.find((g) => g.key === this.openKey) ?? null));
  analysing = $derived(this.status !== null && this.status.analysed < this.status.total);

  /** Every other rail source calls this: harmless when the view is not on. */
  leave() {
    this.active = false;
    this.openKey = null;
    this.pendingReject = null;
  }

  saveSimilarity() {
    try {
      localStorage.setItem(STORAGE_KEY, String(this.similarity));
    } catch {
      // not persisted; fine
    }
  }
}

/** @param {import('./library.svelte.js').LibraryStore} lib */
export function createSimilarStore(lib) {
  return new SimilarStore(lib);
}

export const similar = createSimilarStore(library);
