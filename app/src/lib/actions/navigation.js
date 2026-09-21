// Navigation between images and modules (RFC-0009 P6c, moved out of +page.svelte's script): stepping
// through the filtered Library / Develop filmstrip, opening an image in Develop, switching module
// and clicking Export. These cross develop, masks, print, faces, shell and the export flow, and each
// runs exactly the statements it did in the page, in the same order (RFC-0009 §3.6, invariants
// I1-I3 and I6: previous version captured before the reassignment and any await; flush awaited
// before the thumbnail regeneration; the stale-open guard after the awaited lens lookup; Print's
// item snapshot taken once on entry).

import { library } from "$lib/state/library.svelte.js";
import { selection } from "$lib/state/selection.svelte.js";
import { shell } from "$lib/state/shell.svelte.js";
import { develop } from "$lib/state/develop.svelte.js";
import { regenerateThumbnailFor } from "$lib/actions/importActions.js";
import { prioritizeThumbnail } from "$lib/actions/libraryActions.js";
import { getEditStack, getHistory, getSnapshots, lookupLensProfile, setLensProfile } from "$lib/api/develop.js";
import { masks } from "$lib/state/masks.svelte.js";
import { developView } from "$lib/state/developView.svelte.js";
import { print } from "$lib/state/print.svelte.js";
import { exportFlow } from "$lib/state/exportFlow.svelte.js";
import { refreshPeople } from "$lib/actions/faceActions.js";

// Keyboard navigation & selection helpers
export function selectNextImage(/** @type {boolean=} */ extend) {
  if (library.filteredImages.length === 0) return;
  if (selection.selectedId === null) {
    const first = library.filteredImages[0];
    selection.selectedId = first.version_id;
    selection.selectedIds = new Set([first.version_id]);
    return;
  }
  const idx = library.filteredImages.findIndex((img) => img.version_id === selection.selectedId);
  if (idx === -1) {
    const first = library.filteredImages[0];
    selection.selectedId = first.version_id;
    selection.selectedIds = new Set([first.version_id]);
    return;
  }
  if (idx < library.filteredImages.length - 1) {
    const nextImg = library.filteredImages[idx + 1];
    if (extend) {
      const next = new Set(selection.selectedIds);
      next.add(nextImg.version_id);
      selection.selectedIds = next;
      selection.selectedId = nextImg.version_id;
    } else {
      selection.selectedId = nextImg.version_id;
      selection.selectedIds = new Set([nextImg.version_id]);
    }
    if (shell.activeModule === "develop") {
      openDevelop(nextImg.version_id);
    }
  }
}

export function selectPrevImage(/** @type {boolean=} */ extend) {
  if (library.filteredImages.length === 0) return;
  if (selection.selectedId === null) {
    const last = library.filteredImages[library.filteredImages.length - 1];
    selection.selectedId = last.version_id;
    selection.selectedIds = new Set([last.version_id]);
    return;
  }
  const idx = library.filteredImages.findIndex((img) => img.version_id === selection.selectedId);
  if (idx === -1) {
    const first = library.filteredImages[0];
    selection.selectedId = first.version_id;
    selection.selectedIds = new Set([first.version_id]);
    return;
  }
  if (idx > 0) {
    const prevImg = library.filteredImages[idx - 1];
    if (extend) {
      const next = new Set(selection.selectedIds);
      next.add(prevImg.version_id);
      selection.selectedIds = next;
      selection.selectedId = prevImg.version_id;
    } else {
      selection.selectedId = prevImg.version_id;
      selection.selectedIds = new Set([prevImg.version_id]);
    }
    if (shell.activeModule === "develop") {
      openDevelop(prevImg.version_id);
    }
  }
}

export async function openDevelop(/** @type {number} */ versionId) {
  // Captured before developVersionId is reassigned below -- the same
  // capture-before-reassignment shape flushEditStack itself already
  // uses, which is what keeps this race-free even if the user clicks
  // through several images in quick succession (each flush/regen closes
  // over the id it actually applies to, not whatever developVersionId
  // happens to be by the time the async work runs).
  const previousVersionId = develop.versionId;
  // Awaited -- regenerate_thumbnail's own Rust command re-reads the edit
  // stack fresh from the catalog rather than trusting a client-supplied
  // one (see lib.rs's own doc comment on that command), which means it
  // could race flushEditStack's own catalog write if the two IPC calls
  // were fired back-to-back without awaiting: neither Tauri's own
  // command dispatch nor the underlying SQLite write is guaranteed to
  // land before the very next command's own read starts. A real,
  // code-verified hazard (two dependent IPC calls previously fired
  // without awaiting the first) -- not independently confirmed as a
  // reproduced user-visible symptom (an attempt to reproduce one was
  // confounded by reusing identical edit-stack values across test runs,
  // which produces an identical, correctly-unchanged content-addressed
  // thumbnail path regardless of ordering), but the same class of
  // "unawaited dependent write" bug this project already found and
  // fixed once this session (flushEditStack's own now-removed
  // persistTimer gate) -- worth closing on that precedent alone.
  await develop.flushEditStack();
  regenerateThumbnailFor(previousVersionId);
  const image = library.images.find((img) => img.version_id === versionId);
  if (!image) return;
  prioritizeThumbnail(versionId);
  develop.versionId = versionId;
  develop.imagePath = image.path;
  // Cleared, not left stale, on every open -- the new image's own real
  // histogram arrives shortly via DevelopCanvas's own GPU readback, but
  // showing the PREVIOUS image's histogram in the meantime would be
  // actively misleading, not just momentarily stale.
  develop.histogramData = null;
  develop.hoverPixel = null;
  develop.showClippingOverlay = false;
  // History/Snapshots (M3): re-fetched fresh on every open, not carried
  // over from whatever the previous image's panel showed -- switching
  // images via the filmstrip must never leave a stale History/Snapshots
  // list on screen for a different photo.
  const [stack, freshHistory, freshSnapshots] = await Promise.all([
    getEditStack(versionId),
    getHistory(versionId),
    getSnapshots(versionId),
  ]);
  develop.editStack = stack;
  develop.history = freshHistory;
  develop.historyIndex = freshHistory.length - 1;
  develop.snapshots = freshSnapshots;
  masks.activeTool = null;
  masks.selectedMaskId = null;
  shell.activeModule = "develop";

  // Lens Corrections (M3): re-resolved fresh on every open, matching
  // History/Snapshots' own "never carry over the previous photo's data"
  // discipline above -- this photo's own EXIF, not whatever the last
  // photo's profile happened to be. A no-op (same value already baked,
  // or no match either time) skips the write entirely rather than
  // idempotently re-flushing on every single open. NOT run through
  // scheduleFlush/a history label -- this is resolved equipment data,
  // not a user-facing edit (see develop.js's own doc comment on
  // `setLensProfile`); `flushEditStack()` with no label is the same
  // silent, unlabeled persist its own doc comment already documents for
  // exactly this "idempotent no-op-content rewrite" case.
  const profile = await lookupLensProfile({
    cameraMake: image.camera_make,
    cameraModel: image.camera_model,
    lensModel: image.lens_model,
    focalLength: image.focal_length,
    aperture: image.aperture,
  });
  if (develop.versionId === versionId && JSON.stringify(profile) !== JSON.stringify(developView.lensCorrection.profile)) {
    develop.editStack = setLensProfile(develop.editStack, profile);
    develop.flushEditStack();
  }
}

export async function switchModule(/** @type {string} */ target) {
  if (shell.activeModule === "develop" && target !== "develop") {
    // Awaited -- same unawaited-dependent-IPC-calls hazard openDevelop's
    // own flush/regen pair guards against, see that function's own doc
    // comment.
    await develop.flushEditStack();
    regenerateThumbnailFor(develop.versionId);
    masks.activeTool = null;
    masks.selectedMaskId = null;
  }
  if (target === "print") {
    // Snapshot what Print will act on -- same source `currentExportItems`
    // already derives (open Develop image, else Library's selection) --
    // taken once on entry so the print job doesn't silently change out
    // from under the user if they alter Library's selection afterward
    // (Library's own grid is hidden while inside Print, same as Develop).
    print.items = exportFlow.currentItems;
    print.readyUrls = {};
  }
  if (target === "people") {
    refreshPeople();
  }
  shell.activeModule = target;
}

export async function handleExportClick() {
  // If a slider was just dragged, the debounced save may not have
  // landed yet -- flush it first so Export reads the value currently
  // on screen, not the last-persisted one. Awaited for the same
  // unawaited-dependent-IPC-calls hazard openDevelop's own flush/regen
  // pair guards against, see that function's own doc comment.
  if (shell.activeModule === "develop") {
    await develop.flushEditStack();
    regenerateThumbnailFor(develop.versionId);
  }
  // null stays the "closed" sentinel -- never open with an empty list.
  exportFlow.items = exportFlow.currentItems.length > 0 ? exportFlow.currentItems : null;
}
