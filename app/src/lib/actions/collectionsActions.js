// Collection membership workflows (RFC-0009 P4b, moved out of +page.svelte's script): adding the
// selection to a collection (or a new one) and removing it from the active one. Each combines the
// `library` store, the `selection` store and `shell.notify` with the catalog IPC.

import { selection } from "$lib/state/selection.svelte.js";
import { library } from "$lib/state/library.svelte.js";
import { addImagesToCollection, createCollectionWithImages, removeImagesFromCollection } from "$lib/api/catalog.js";
import { loadManualMembership, refreshCollections } from "$lib/actions/libraryActions.js";
import { shell } from "$lib/state/shell.svelte.js";

// "Add to Collection…" toolbar picker, from a multi-selection.
export async function handleAddToCollectionSelect(/** @type {string} */ value) {
  if (!value) return;
  const imageIds = [...new Set(selection.selectedImages.map((img) => img.image_id))];
  if (imageIds.length === 0) return;
  if (value === "__new__") {
    library.pendingAddToCollectionImageIds = imageIds;
    library.creatingCollectionWithImages = true;
    return;
  }
  const collectionId = Number(value);
  await addImagesToCollection(collectionId, imageIds);
  // Invalidate by the collection id that was actually just mutated, not
  // by activeCollectionId -- those differ when adding to a DIFFERENT
  // collection than the one currently being viewed, and invalidating
  // the wrong one would silently leave the mutated one's cache stale.
  if (library.manualMembership.has(collectionId)) await loadManualMembership(collectionId);
  await refreshCollections();
  shell.notify(`Added ${imageIds.length} photo${imageIds.length === 1 ? "" : "s"} to collection`);
}

export async function handleCreateCollectionWithImages(/** @type {string} */ name) {
  library.creatingCollectionWithImages = false;
  const imageIds = library.pendingAddToCollectionImageIds;
  library.pendingAddToCollectionImageIds = [];
  await createCollectionWithImages(name, imageIds);
  await refreshCollections();
  shell.notify(`Added ${imageIds.length} photo${imageIds.length === 1 ? "" : "s"} to "${name}"`);
}

export async function handleRemoveFromCollection() {
  if (library.activeCollectionId === null) return;
  const imageIds = [...new Set(selection.selectedImages.map((img) => img.image_id))];
  if (imageIds.length === 0) return;
  await removeImagesFromCollection(library.activeCollectionId, imageIds);
  await loadManualMembership(library.activeCollectionId); // mutated === active here, still the right id
  await refreshCollections();
  const removedVersionIds = new Set(selection.selectedImages.map((img) => img.version_id));
  selection.selectedIds = new Set([...selection.selectedIds].filter((id) => !removedVersionIds.has(id)));
  if (selection.selectedId !== null && removedVersionIds.has(selection.selectedId)) selection.selectedId = null;
  shell.notify(`Removed ${imageIds.length} photo${imageIds.length === 1 ? "" : "s"} from collection`);
}
