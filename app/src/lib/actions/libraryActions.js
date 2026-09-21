// Library operations (RFC-0009 P4a, P5b, moved out of +page.svelte's script): everything that reads or
// writes the `library` store plus IPC -- switching the source (All Photos / Last Import / folder /
// collection / person / map selection), refreshing the image list and collections, optimistic local patches,
// thumbnail-batch application, collection create/delete, and removing photos from the catalog
// (which also clears the selection and, if it was open, the Develop session).

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
  removeImages,
} from "$lib/api/catalog.js";
import { selection } from "$lib/state/selection.svelte.js";
import { shell } from "$lib/state/shell.svelte.js";
import { develop } from "$lib/state/develop.svelte.js";

export async function loadPersonMembership(/** @type {number} */ personId) {
  const memberIds = await getImagesForPerson(personId);
  library.personMembership = new Map(library.personMembership).set(personId, new Set(memberIds));
}

export async function selectPerson(/** @type {number} */ personId) {
  library.activeCollectionId = null;
  library.activeFolderKey = null;
  library.showLastImportOnly = false;
  library.activePersonId = personId;
  library.activeMapImageIds = null;
  if (!library.personMembership.has(personId)) await loadPersonMembership(personId);
}

export function selectAllPhotos() {
  library.activeCollectionId = null;
  library.activeFolderKey = null;
  library.showLastImportOnly = false;
  library.activePersonId = null;
  library.activeMapImageIds = null;
}

export function selectLastImport() {
  library.activeCollectionId = null;
  library.activeFolderKey = null;
  library.showLastImportOnly = true;
  library.activePersonId = null;
  library.activeMapImageIds = null;
}

export function selectFolder(/** @type {string} */ key) {
  library.activeCollectionId = null;
  library.activeFolderKey = key;
  library.showLastImportOnly = false;
  library.activePersonId = null;
  library.activeMapImageIds = null;
}

/** Scopes Library to the photos of a clicked map pin or cluster (M5.5 map view). Takes `image_id`s. */
export function selectMapImages(/** @type {Iterable<number>} */ imageIds) {
  library.activeCollectionId = null;
  library.activeFolderKey = null;
  library.showLastImportOnly = false;
  library.activePersonId = null;
  library.activeMapImageIds = new Set(imageIds);
}

/** Opens the world map. Drops any earlier map selection first so the pins show the whole current source
 * (with its filters), not just what the last click narrowed it to. */
export function showMapView() {
  library.activeMapImageIds = null;
  library.libraryViewMode = "map";
}

/** A pin or cluster was clicked: scope Library to its photos and show them in the grid. */
export function handleMapClusterSelect(/** @type {Iterable<number>} */ imageIds) {
  selectMapImages(imageIds);
  library.libraryViewMode = "grid";
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
  library.activeMapImageIds = null;
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

// Non-destructive removal (M2 Slice 3): catalog rows + app-owned derived
// files only -- the backend never touches source files. `await` the
// command BEFORE filtering local state: the other order would let an
// in-flight pollUntilThumbnailsReady refresh() momentarily resurrect the
// removed rows in the UI.
export async function handleRemoveConfirmed() {
  library.confirmingRemoval = false;
  // Symmetry with the close handler: force any in-progress IPTC edit's
  // blur-save to fire before the rows it targets can disappear.
  /** @type {HTMLElement | null} */ (document.activeElement)?.blur();

  const imageIds = [...new Set(selection.selectedImages.map((img) => img.image_id))];
  if (imageIds.length === 0) return;
  try {
    await removeImages(imageIds);
  } catch (/** @type {any} */ e) {
    shell.notify(`Remove failed: ${e}`);
    return;
  }
  const removedVersionIds = new Set(selection.selectedImages.map((img) => img.version_id));
  library.images = library.images.filter((img) => !removedVersionIds.has(img.version_id));
  shell.notify(`Removed ${imageIds.length} photo${imageIds.length === 1 ? "" : "s"} from catalog`);
  selection.selectedId = null;
  selection.selectedIds = new Set();
  // If the image open in Develop was just removed, clear that state too --
  // otherwise the develop branch keeps rendering a deleted image, and a
  // pending debounced edit-stack save would fire a pointless IPC call
  // against the deleted version.
  if (develop.versionId !== null && removedVersionIds.has(develop.versionId)) {
    develop.cancelScheduledFlush();
    develop.versionId = null;
    develop.imagePath = "";
  }
}
