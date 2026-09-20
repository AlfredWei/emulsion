// Library selection state (RFC-0009 §3.3, P4b). `selectedIds` is the full multi-select (M2 Slice
// 3); `selectedId` is the anchor/primary -- the last plainly-clicked image, which drives
// MetadataPanel, Shift-range endpoints, and any single-image concern. Both are reassigned
// immutably on every change: Svelte 5's $state doesn't deep-proxy Set, so in-place
// .add()/.delete() would silently not react.
//
// Selection depends on the library (store DAG: selection -> library), which is why the derived
// values that combine the two -- `selectedImage(s)`, the keyword target, and the Compare view's
// select/candidate pair -- live here rather than in `library`. The store takes its library as a
// constructor argument so tests can build an isolated pair; the singleton is wired to the app's.
//
// Operations on the selection (click/range/step/select-all, Compare navigation) are in
// lib/actions/selectionActions.js.

import { library } from "./library.svelte.js";

export class SelectionStore {
  /** @param {import('./library.svelte.js').LibraryStore} library */
  constructor(library) {
    // Set before any derived below is first read (deriveds are lazy).
    this.library = library;
  }

  selectedId = $state(/** @type {number | null} */ (null));
  selectedIds = $state(/** @type {Set<number>} */ (new Set()));

  selectedImage = $derived(this.library.images.find((img) => img.version_id === this.selectedId) ?? null);
  selectedImages = $derived(this.library.images.filter((img) => this.selectedIds.has(img.version_id)));

  // Who a newly-typed keyword in MetadataPanel gets assigned to (M2 Slice
  // 4): the whole current Library selection when there is one, else just
  // the anchor image -- unconditional on "the acted-on cell is part of
  // the selection" (unlike targetVersionIds) since there's no
  // per-cell click event here, just "apply to whatever's selected".
  keywordTargetImageIds = $derived(
    this.selectedImages.length > 0
      ? this.selectedImages.map((img) => img.image_id)
      : this.selectedImage
        ? [this.selectedImage.image_id]
        : [],
  );

  compareSelectImage = $derived.by(() => {
    if (this.selectedId !== null) {
      const match = this.library.filteredImages.find((img) => img.version_id === this.selectedId);
      if (match) return match;
    }
    return this.library.filteredImages[0] ?? null;
  });

  compareCandidateImage = $derived.by(() => {
    if (this.library.compareCandidateId !== null) {
      const match = this.library.filteredImages.find((img) => img.version_id === this.library.compareCandidateId);
      if (match) return match;
    }
    if (this.selectedIds.size >= 2) {
      const otherId = [...this.selectedIds].find((id) => id !== this.selectedId);
      if (otherId != null) {
        const match = this.library.filteredImages.find((img) => img.version_id === otherId);
        if (match) return match;
      }
    }
    if (this.compareSelectImage && this.library.filteredImages.length > 1) {
      const selIdx = this.library.filteredImages.findIndex((img) => img.version_id === this.compareSelectImage.version_id);
      if (selIdx >= 0) {
        return this.library.filteredImages[(selIdx + 1) % this.library.filteredImages.length];
      }
    }
    return this.compareSelectImage;
  });
}

/** @param {import('./library.svelte.js').LibraryStore} lib */
export function createSelectionStore(lib) {
  return new SelectionStore(lib);
}

export const selection = createSelectionStore(library);
