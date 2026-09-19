// Library filtering (RFC-0009 P2, moved out of +page.svelte's script).
//
// Pure functions over plain data: which images the active folder/collection/person/last-import
// scope selects (`selectBaseImages`), the filter bar's narrowing on top of that
// (`applyLibraryFilters`), and the camera/lens dropdown options. The page still owns the state and
// wraps these in `$derived`; `inputs` is a getter object there, so the derived reads exactly the
// same signals, in the same order, that it did when this code lived inline.

import { matchesRules } from "$lib/collectionRules.js";
import { folderKeyForPath } from "$lib/libraryFolders.js";

/**
 * @typedef {import('$lib/api/catalog.js').ImageSummary} ImageSummary
 * @typedef {import('$lib/api/catalog.js').CollectionSummary} CollectionSummary
 *
 * @typedef {Object} BaseImageInputs
 * @property {ImageSummary[]} images
 * @property {boolean} showLastImportOnly
 * @property {number | null} lastImportBatchId
 * @property {string | null} activeFolderKey
 * @property {number | null} activePersonId
 * @property {Map<number, Set<number>>} personMembership
 * @property {number | null} activeCollectionId
 * @property {CollectionSummary[]} collections
 * @property {Map<number, Set<number>>} manualMembership
 * @property {Map<number, Set<number>>} keywordIdsByImage
 *
 * @typedef {Object} FilterInputs
 * @property {ImageSummary[]} baseImages
 * @property {string} searchQuery
 * @property {"all" | "pick" | "unflagged" | "reject"} flagFilter
 * @property {number} minRating
 * @property {">=" | "="} ratingOp
 * @property {string} colorLabelFilter
 * @property {"all" | "raw" | "jpeg"} fileTypeFilter
 * @property {string} cameraFilter
 * @property {string} lensFilter
 * @property {string} dateFrom
 * @property {string} dateTo
 */

/** "Make Model" (e.g. "Canon EOS 5D Mark III"), same join convention as MetadataPanel's own camera row. */
export function cameraLabel(/** @type {{ camera_make?: string|null, camera_model?: string|null }} */ img) {
  return [img.camera_make, img.camera_model].filter(Boolean).join(" ") || null;
}

// Base image set for active folder/collection/person
/** @param {BaseImageInputs} inputs */
export function selectBaseImages(inputs) {
  if (inputs.showLastImportOnly) {
    return inputs.lastImportBatchId === null ? [] : inputs.images.filter((img) => img.import_batch === inputs.lastImportBatchId);
  }
  if (inputs.activeFolderKey !== null) {
    return inputs.images.filter((img) => folderKeyForPath(img.path) === inputs.activeFolderKey);
  }
  if (inputs.activePersonId !== null) {
    const memberIds = inputs.personMembership.get(inputs.activePersonId);
    if (!memberIds) return []; // membership not fetched yet
    return inputs.images.filter((img) => memberIds.has(img.image_id));
  }
  if (inputs.activeCollectionId === null) return inputs.images;
  const collection = inputs.collections.find((c) => c.id === inputs.activeCollectionId);
  if (!collection) return inputs.images;
  if (collection.is_smart) {
    const rules = collection.rules ?? [];
    return inputs.images.filter((img) => matchesRules(img, rules, inputs.keywordIdsByImage));
  }
  const memberIds = inputs.manualMembership.get(inputs.activeCollectionId);
  if (!memberIds) return []; // membership not fetched yet
  return inputs.images.filter((img) => memberIds.has(img.image_id));
}

// The image set the Library grid and filmstrip show after applying active filters
/** @param {FilterInputs} inputs */
export function applyLibraryFilters(inputs) {
  let result = inputs.baseImages;

  // Search text query
  const q = inputs.searchQuery.trim().toLowerCase();
  if (q) {
    result = result.filter((img) => {
      const name = (img.path.split(/[/\\]/).pop() || "").toLowerCase();
      const path = img.path.toLowerCase();
      const make = (img.camera_make || "").toLowerCase();
      const model = (img.camera_model || "").toLowerCase();
      const lens = (img.lens_model || "").toLowerCase();
      const caption = (img.caption || "").toLowerCase();
      const copyright = (img.copyright || "").toLowerCase();
      const contact = (img.contact || "").toLowerCase();
      return (
        name.includes(q) ||
        path.includes(q) ||
        make.includes(q) ||
        model.includes(q) ||
        lens.includes(q) ||
        caption.includes(q) ||
        copyright.includes(q) ||
        contact.includes(q)
      );
    });
  }

  // Flag filter
  if (inputs.flagFilter === "pick") {
    result = result.filter((img) => img.flag === "pick");
  } else if (inputs.flagFilter === "unflagged") {
    result = result.filter((img) => img.flag === "none" || !img.flag);
  } else if (inputs.flagFilter === "reject") {
    result = result.filter((img) => img.flag === "reject");
  }

  // Star Rating filter
  if (inputs.minRating > 0) {
    if (inputs.ratingOp === ">=") {
      result = result.filter((img) => img.rating >= inputs.minRating);
    } else {
      result = result.filter((img) => img.rating === inputs.minRating);
    }
  }

  // Color label filter
  if (inputs.colorLabelFilter !== "all") {
    result = result.filter((img) => img.color_label === inputs.colorLabelFilter);
  }

  // File type filter
  if (inputs.fileTypeFilter === "raw") {
    result = result.filter(
      (img) =>
        !img.path.toLowerCase().endsWith(".jpg") && !img.path.toLowerCase().endsWith(".jpeg"),
    );
  } else if (inputs.fileTypeFilter === "jpeg") {
    result = result.filter(
      (img) =>
        img.path.toLowerCase().endsWith(".jpg") || img.path.toLowerCase().endsWith(".jpeg"),
    );
  }

  // Camera filter
  if (inputs.cameraFilter !== "all") {
    result = result.filter((img) => cameraLabel(img) === inputs.cameraFilter);
  }

  // Lens filter
  if (inputs.lensFilter !== "all") {
    result = result.filter((img) => img.lens_model === inputs.lensFilter);
  }

  // Date-taken range filter (captured_at is an ISO 8601 string; comparing its
  // YYYY-MM-DD date portion lexicographically against the <input type="date"> values
  // avoids parsing either side as a Date/timezone).
  if (inputs.dateFrom) {
    result = result.filter((img) => img.captured_at && img.captured_at.slice(0, 10) >= inputs.dateFrom);
  }
  if (inputs.dateTo) {
    result = result.filter((img) => img.captured_at && img.captured_at.slice(0, 10) <= inputs.dateTo);
  }

  return result;
}

// Distinct camera/lens values within the active folder/collection scope, for the filter dropdowns
/** @param {import('$lib/api/catalog.js').ImageSummary[]} baseImages */
export function cameraOptionsFor(baseImages) {
  return [...new Set(baseImages.map(cameraLabel).filter((v) => v !== null))].sort();
}
/** @param {import('$lib/api/catalog.js').ImageSummary[]} baseImages */
export function lensOptionsFor(baseImages) {
  return [...new Set(baseImages.map((img) => img.lens_model || "").filter((v) => v !== ""))].sort();
}
