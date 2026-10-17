// Moving around the AI-enlargement connection (RFC-0029 §3.2): "Go to original" and "Show enlargements" in the
// photo menu and the Connected photos block. Both land in the Library with the target photo(s) selected, clearing
// whatever scope or filter would hide them.

import { library } from "$lib/state/library.svelte.js";
import { selection } from "$lib/state/selection.svelte.js";
import { shell } from "$lib/state/shell.svelte.js";
import { getUpscaleLinks } from "$lib/api/catalog.js";
import { handleResetFilters, prioritizeThumbnail } from "$lib/actions/libraryActions.js";
import { switchModule } from "$lib/actions/navigation.js";

/** Selects the photos with these image ids (all of them, the first as the active one) and makes them visible:
 * leaves Develop, shows the whole catalog and drops the filter bar's narrowing when they would be hidden.
 * Returns false when none of them is in the catalog list. @param {number[]} imageIds */
export async function revealPhotos(imageIds) {
  const targets = library.images.filter((img) => imageIds.includes(img.image_id));
  if (targets.length === 0) return false;
  if (shell.activeModule !== "library") await switchModule("library");
  const visible = new Set(library.filteredImages.map((img) => img.image_id));
  if (!targets.every((t) => visible.has(t.image_id))) {
    library.activeCollectionId = null;
    library.activeFolderKey = null;
    library.showLastImportOnly = false;
    library.activePersonId = null;
    library.activeMapImageIds = null;
    handleResetFilters();
  }
  library.libraryViewMode = "grid";
  selection.selectedId = targets[0].version_id;
  selection.selectedIds = new Set(targets.map((t) => t.version_id));
  for (const t of targets) prioritizeThumbnail(t.version_id);
  return true;
}

/** Selects the original of an enlargement. @param {number} imageId the enlargement's own image id */
export async function goToOriginal(imageId) {
  const image = library.images.find((img) => img.image_id === imageId);
  if (!image || image.enlarged_scale == null) return;
  if (image.enlarged_from == null) {
    shell.notify("The original was removed from the catalog");
    return;
  }
  if (!(await revealPhotos([image.enlarged_from]))) shell.notify("The original is not in the catalog");
}

/** Selects every enlargement made from this photo. @param {number} imageId the original's image id */
export async function showEnlargements(imageId) {
  try {
    const { enlargements } = await getUpscaleLinks(imageId);
    if (enlargements.length === 0) {
      shell.notify("This photo has no enlargements");
      return;
    }
    if (!(await revealPhotos(enlargements.map((e) => e.result_image_id)))) shell.notify("The enlargements are not in the catalog");
  } catch (/** @type {any} */ e) {
    shell.notify(`Could not read the enlargements: ${e}`);
  }
}
