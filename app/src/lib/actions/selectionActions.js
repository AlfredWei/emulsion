// Library selection operations (RFC-0009 P4b, moved out of +page.svelte's script): click / Shift-range /
// Cmd-toggle, keyboard and grid stepping, select-all / deselect-all, batch-action targeting, and the
// Compare view's navigation. They write the `selection` store (and `library.compareCandidateId` /
// `libraryViewMode`), reading `library.filteredImages`. `selectNextImage`/`selectPrevImage` stay in
// the page until Develop has a store (they call `openDevelop`).

import { selection } from "$lib/state/selection.svelte.js";
import { library } from "$lib/state/library.svelte.js";

// Batch rate/flag/color-label (MILESTONES.md M2 scope, deferred from
// Slice 3's multi-select work): Lightroom-style -- acting on a cell that
// is part of an active multi-selection applies to the whole selection,
// not just that one cell. Acting on a cell OUTSIDE the current
// selection (or when only one image is selected) stays single-target,
// unaffected by an unrelated selection elsewhere. When called without an
// explicit versionId (e.g. from toolbar / metadata panel / hotkey), applies
// to all currently selected images (or the anchor image).
export function targetVersionIds(/** @type {number | null | undefined} */ versionId) {
  if (versionId !== undefined && versionId !== null) {
    if (selection.selectedIds.size > 1 && selection.selectedIds.has(versionId)) {
      return [...selection.selectedIds];
    }
    return [versionId];
  }
  if (selection.selectedIds.size > 0) return [...selection.selectedIds];
  return selection.selectedId !== null ? [selection.selectedId] : [];
}

export function selectGridStep(/** @type {number} */ step, /** @type {boolean=} */ extend) {
  if (library.filteredImages.length === 0) return;
  if (selection.selectedId === null) {
    const first = library.filteredImages[0];
    selection.selectedId = first.version_id;
    selection.selectedIds = new Set([first.version_id]);
    return;
  }
  const idx = library.filteredImages.findIndex((img) => img.version_id === selection.selectedId);
  if (idx === -1) return;
  const targetIdx = Math.max(0, Math.min(library.filteredImages.length - 1, idx + step));
  const targetImg = library.filteredImages[targetIdx];
  if (!targetImg) return;
  if (extend) {
    const [from, to] = idx <= targetIdx ? [idx, targetIdx] : [targetIdx, idx];
    selection.selectedIds = new Set(library.filteredImages.slice(from, to + 1).map((img) => img.version_id));
    selection.selectedId = targetImg.version_id;
  } else {
    selection.selectedId = targetImg.version_id;
    selection.selectedIds = new Set([targetImg.version_id]);
  }
}

export function handleSelectAll() {
  if (library.filteredImages.length === 0) return;
  selection.selectedIds = new Set(library.filteredImages.map((img) => img.version_id));
  if (selection.selectedId === null || !selection.selectedIds.has(selection.selectedId)) {
    selection.selectedId = library.filteredImages[0].version_id;
  }
}

export function handleDeselectAll() {
  if (library.libraryViewMode !== "grid") {
    library.libraryViewMode = "grid";
    return;
  }
  selection.selectedIds = new Set();
  selection.selectedId = null;
}

// Multi-select click semantics (M2 Slice 3), standard file-manager
// behavior: plain click replaces the selection and moves the anchor;
// Cmd/Ctrl toggles one image in/out; Shift selects the contiguous range
// (in `filteredImages` order) from the anchor to the clicked image.
//
// The range is computed over `filteredImages`, NOT the full `images`
// array (M2 Slice 5 fix): while no collection filter is active the two
// are identical, but once a collection filters the grid, indexing into
// the unfiltered `images` array would compute a range over catalog-wide
// positions that don't correspond to what's on screen -- Shift-click
// could silently pull hidden/filtered-out images into the selection,
// which then flows into remove/batch-culling/keyword-assignment against
// images the user never saw or selected. LibraryGrid's virtualization
// is a separate, narrower concern (it only slices what's *rendered*
// within the already-filtered set for scroll performance) and doesn't
// affect this.
export function handleSelect(/** @type {number} */ versionId, /** @type {MouseEvent=} */ event) {
  if (event?.shiftKey && selection.selectedId !== null) {
    const anchorIndex = library.filteredImages.findIndex((img) => img.version_id === selection.selectedId);
    const clickedIndex = library.filteredImages.findIndex((img) => img.version_id === versionId);
    if (anchorIndex !== -1 && clickedIndex !== -1) {
      const [from, to] = anchorIndex <= clickedIndex ? [anchorIndex, clickedIndex] : [clickedIndex, anchorIndex];
      selection.selectedIds = new Set(library.filteredImages.slice(from, to + 1).map((img) => img.version_id));
      return; // anchor stays put, Lightroom/Finder-style
    }
    // Stale anchor (e.g. it was just removed, or is outside the current
    // filter): fall through to plain select.
  }
  if (event?.metaKey || event?.ctrlKey) {
    const next = new Set(selection.selectedIds);
    if (next.has(versionId)) {
      next.delete(versionId);
      if (selection.selectedId === versionId) {
        selection.selectedId = next.size > 0 ? [...next][next.size - 1] : null;
      }
    } else {
      next.add(versionId);
      selection.selectedId = versionId;
    }
    selection.selectedIds = next;
    return;
  }
  selection.selectedId = versionId;
  selection.selectedIds = new Set([versionId]);
}

export function handleCompareNextCandidate() {
  if (library.filteredImages.length === 0) return;
  const curCandidate = selection.compareCandidateImage;
  const curSelect = selection.compareSelectImage;
  const cIdx = curCandidate
    ? library.filteredImages.findIndex((img) => img.version_id === curCandidate.version_id)
    : 0;
  const nextIdx = (cIdx + 1) % library.filteredImages.length;
  const nextCand = library.filteredImages[nextIdx];
  library.compareCandidateId = nextCand.version_id;
  if (curSelect) {
    selection.selectedIds = new Set([curSelect.version_id, nextCand.version_id]);
  }
}

export function handleComparePrevCandidate() {
  if (library.filteredImages.length === 0) return;
  const curCandidate = selection.compareCandidateImage;
  const curSelect = selection.compareSelectImage;
  const cIdx = curCandidate
    ? library.filteredImages.findIndex((img) => img.version_id === curCandidate.version_id)
    : 0;
  const prevIdx = (cIdx - 1 + library.filteredImages.length) % library.filteredImages.length;
  const prevCand = library.filteredImages[prevIdx];
  library.compareCandidateId = prevCand.version_id;
  if (curSelect) {
    selection.selectedIds = new Set([curSelect.version_id, prevCand.version_id]);
  }
}

export function handleCompareSwap() {
  const curSelect = selection.compareSelectImage;
  const curCand = selection.compareCandidateImage;
  if (!curSelect || !curCand) return;
  const oldSelId = curSelect.version_id;
  const oldCandId = curCand.version_id;
  selection.selectedId = oldCandId;
  library.compareCandidateId = oldSelId;
  selection.selectedIds = new Set([oldCandId, oldSelId]);
}

export function handleCompareMakeSelect() {
  const curCand = selection.compareCandidateImage;
  if (!curCand) return;
  selection.selectedId = curCand.version_id;
  const newSelIdx = library.filteredImages.findIndex((img) => img.version_id === curCand.version_id);
  if (library.filteredImages.length > 1) {
    const nextCandIdx = (newSelIdx + 1) % library.filteredImages.length;
    library.compareCandidateId = library.filteredImages[nextCandIdx].version_id;
    selection.selectedIds = new Set([selection.selectedId, library.compareCandidateId]);
  } else {
    selection.selectedIds = new Set([selection.selectedId]);
  }
}
