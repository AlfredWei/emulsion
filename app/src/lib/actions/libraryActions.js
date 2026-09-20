// Library operations (RFC-0009 P4a, moved out of +page.svelte's script): everything that reads or
// writes only the `library` store plus IPC -- switching the source (All Photos / Last Import /
// folder / collection / person), refreshing the image list and collections, optimistic local
// patches, thumbnail-batch application, and collection create/delete. Workflows that also touch
// selection or Develop (`handleRemoveConfirmed`, compare navigation, …) move with P4b/P5.

import { getImagesForPerson } from "$lib/api/faces.js";
import { library } from "$lib/state/library.svelte.js";
import {
  listCollections,
  listCollectionImageIds,
  listImages,
  ensureThumbnail,
  createCollection,
  createSmartCollection,
  deleteCollection,
} from "$lib/api/catalog.js";

export async function loadPersonMembership(/** @type {number} */ personId) {
  const memberIds = await getImagesForPerson(personId);
  library.personMembership = new Map(library.personMembership).set(personId, new Set(memberIds));
}

export async function selectPerson(/** @type {number} */ personId) {
  library.activeCollectionId = null;
  library.activeFolderKey = null;
  library.showLastImportOnly = false;
  library.activePersonId = personId;
  if (!library.personMembership.has(personId)) await loadPersonMembership(personId);
}

export function selectAllPhotos() {
  library.activeCollectionId = null;
  library.activeFolderKey = null;
  library.showLastImportOnly = false;
  library.activePersonId = null;
}

export function selectLastImport() {
  library.activeCollectionId = null;
  library.activeFolderKey = null;
  library.showLastImportOnly = true;
  library.activePersonId = null;
}

export function selectFolder(/** @type {string} */ key) {
  library.activeCollectionId = null;
  library.activeFolderKey = key;
  library.showLastImportOnly = false;
  library.activePersonId = null;
}

export async function refreshCollections() {
  library.collections = await listCollections();
}

export async function loadManualMembership(/** @type {number} */ collectionId) {
  const memberIds = await listCollectionImageIds(collectionId);
  library.manualMembership = new Map(library.manualMembership).set(collectionId, new Set(memberIds));
}

export function handleResetFilters() {
  library.searchQuery = "";
  library.flagFilter = "all";
  library.minRating = 0;
  library.ratingOp = ">=";
  library.colorLabelFilter = "all";
  library.fileTypeFilter = "all";
  library.cameraFilter = "all";
  library.lensFilter = "all";
  library.dateFrom = "";
  library.dateTo = "";
}

export async function selectCollection(/** @type {number | null} */ collectionId) {
  library.activeCollectionId = collectionId;
  library.activeFolderKey = null;
  library.showLastImportOnly = false;
  library.activePersonId = null;
  if (collectionId !== null && !library.manualMembership.has(collectionId)) {
    const collection = library.collections.find((c) => c.id === collectionId);
    if (collection && !collection.is_smart) await loadManualMembership(collectionId);
  }
}

export async function refresh() {
  library.images = await listImages();
}

// Optimistic local update (UX-DESIGN.md §5) so culling feels instant --
// the write still goes to the real catalog, this just avoids waiting on
// a round trip + full refetch before the UI reflects the change.
export function patchLocal(/** @type {number} */ versionId, /** @type {Partial<import('$lib/api/catalog.js').ImageSummary>} */ patch) {
  library.images = library.images.map((img) => (img.version_id === versionId ? { ...img, ...patch } : img));
}

// Thumbnail refresh after a Develop edit -- entirely separate from
// pendingSave/pendingIptcSave on purpose. Chaining this onto the same
// promise flushEditStack's callers await would silently reintroduce the
// exact "app hangs unable to quit" class of bug M1 Slice 6 already fixed
// once for the edit-stack flush itself -- a slow/failed thumbnail regen
// must never be able to delay a save or block app quit. Never awaited by
// any caller. Only called from real "done editing this image for now"
// transitions (leaving Develop, exporting, closing) -- not from the bare
// Thumbnail batch update: coalesces multiple regeneration requests within
// a 150ms debounce window into a single atomic update via Promise.all, so
// all components consuming thumbnails (grid, filmstrip, metadata panel,
// histogram) update together rather than staggered.
export function handleBatchThumbnailsComplete(/** @type {Map<number, string | null>} */ results) {
  // Update all thumbnails in one pass: map over images array, apply paths
  // for all version IDs that are in the results map, leave others unchanged.
  library.images = library.images.map((img) => {
    const newPath = results.get(img.version_id);
    if (newPath !== undefined) {
      return { ...img, thumbnail_path: newPath };
    }
    return img;
  });
}

/** "Jump the queue" for one image's thumbnail (import.rs's
 * `ensure_thumbnail`): called wherever the user opens a specific photo
 * to view it (Loupe, Develop) so that photo's own Library-grid
 * thumbnail exists as soon as possible, regardless of how far behind it
 * the background backfill pass (backfillMissingThumbnails, below) is --
 * previously a photo near the end of a large just-imported folder could
 * sit behind hundreds of others in that pass's strict FIFO order even
 * after the user had already looked right at it. Only calls the backend
 * at all when there's actually a gap to close (`thumbnail_path` is
 * still null) -- the common case (already has one) stays a pure local
 * lookup, no IPC round trip. Fire-and-forget: never blocks entering
 * Loupe/Develop, which already render the real pixels via
 * getGradedDevelopPreview independent of `thumbnail_path` -- this only
 * fixes how long the GRID CELL (and filmstrip) keep showing a blank
 * placeholder for a photo that's already been viewed. */
export function prioritizeThumbnail(/** @type {number} */ versionId) {
  const image = library.images.find((img) => img.version_id === versionId);
  if (!image || image.thumbnail_path !== null) return;
  ensureThumbnail(versionId)
    .then((thumbnailPath) => {
      if (thumbnailPath) patchLocal(versionId, { thumbnail_path: thumbnailPath });
    })
    .catch(() => {});
}

// Collections (M2 Slice 5). Rename/edit-existing-smart-collection-rules
// UI is deliberately deferred (matching this codebase's precedent for
// `add_image_with_metadata`/`add_edit_stack` -- a lower-level building
// block kept ready without a UI trigger yet): the rail's "+" only ever
// creates fresh collections; changing an existing one means delete and
// recreate for now.
export async function handleCreateCollection(/** @type {string} */ name) {
  library.creatingCollection = false;
  await createCollection(name);
  await refreshCollections();
}

export async function handleCreateSmartCollection(
  /** @type {string} */ name,
  /** @type {import('$lib/api/catalog.js').CollectionRule[]} */ rules,
) {
  library.creatingSmartCollection = false;
  await createSmartCollection(name, rules);
  await refreshCollections();
}

export async function handleDeleteCollection(/** @type {number} */ collectionId, /** @type {MouseEvent} */ event) {
  event.stopPropagation(); // don't also trigger selectCollection
  await deleteCollection(collectionId);
  if (library.activeCollectionId === collectionId) library.activeCollectionId = null;
  await refreshCollections();
}
