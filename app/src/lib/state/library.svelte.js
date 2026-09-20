// Library state (RFC-0009 §3.3, P4a): the catalog's image list, the active source (All Photos /
// Last Import / a folder / a Collection / a Person), the filter bar, and what is derived from
// them. Filtering and sorting themselves are pure functions in lib/libraryFilters.js; the derived
// fields below just wrap them, handing over `this` (whose fields have exactly the names those
// functions read) so each derived tracks the same lazy reads as before.
//
// What is NOT here: `selectedImage`/`selectedIds` (selection store, P4b), and derived values that
// also read selection or Develop state (`compareSelectImage`, `compareCandidateImage`,
// `developFilmstripImages`) -- the store DAG is selection -> library, never the reverse, so those
// stay with the store that owns their other input.
//
// Operations on this state (source switching, IPC refreshes, collection create/delete) are in
// lib/actions/libraryActions.js.

import { selectBaseImages, applyLibraryFilters, cameraOptionsFor, lensOptionsFor } from "$lib/libraryFilters.js";
import { buildKeywordIdsByImage } from "$lib/collectionRules.js";
import { buildFolderEntries } from "$lib/libraryFolders.js";

export class LibraryStore {
  /** @type {import('$lib/api/catalog.js').ImageSummary[]} */
  images = $state([]);
  confirmingRemoval = $state(false);
  libraryViewMode = $state(/** @type {"grid" | "loupe" | "compare" | "survey"} */ ("grid"));
  libraryZoomLevel = $state(1);

  compareCandidateId = $state(/** @type {number | null} */ (null));

  // Collections (M2 Slice 5). `activeCollectionId === null` means "All
  // Photos" (no filter). `manualMembership` caches a manual collection's
  // image_id membership by collection id, fetched on click -- invalidated
  // by whichever collection id was actually just mutated (add/remove-from-
  // collection), never by `activeCollectionId`: those differ exactly when
  // the toolbar adds a selection to a DIFFERENT collection than the one
  // currently being viewed, and invalidating the wrong one would silently
  // leave the mutated collection's cache stale.
  collections = $state(/** @type {import('$lib/api/catalog.js').CollectionSummary[]} */ ([]));
  activeCollectionId = $state(/** @type {number | null} */ (null));
  manualMembership = $state(/** @type {Map<number, Set<number>>} */ (new Map()));

  // Folders / Last Import (M4 Library slice). Four library "sources" --
  // All Photos, Last Import, a real folder, and a Person -- are mutually
  // exclusive with each other and with a Collection, so only one of
  // `activeCollectionId` / `activeFolderKey` / `showLastImportOnly` /
  // `activePersonId` is ever "on" at a time; `baseImages` below checks
  // them in that same order.
  activeFolderKey = $state(/** @type {string | null} */ (null));
  showLastImportOnly = $state(false);

  // People rail filter (2026-09-18 user request): double-clicking a
  // person in CatalogRail's People section scopes Library to just their
  // photos -- the gap RFC-0005 §7 explicitly left open ("no replacement
  // for browsing/filtering the whole catalog by named person"). Same
  // fetch-on-demand-and-cache shape as `manualMembership` above for a
  // manual collection: a person's photo set needs a real query
  // (`get_images_for_person`, a `faces`/`person_id` join), it isn't
  // already sitting on `ImageSummary`.
  activePersonId = $state(/** @type {number | null} */ (null));
  personMembership = $state(/** @type {Map<number, Set<number>>} */ (new Map()));

  allImageKeywords = $state(/** @type {import('$lib/api/catalog.js').ImageKeywordAssignment[]} */ ([]));

  // Library Filters
  searchQuery = $state("");
  flagFilter = $state(/** @type {"all" | "pick" | "unflagged" | "reject"} */ ("all"));
  minRating = $state(0);
  ratingOp = $state(/** @type {">=" | "="} */ (">="));
  colorLabelFilter = $state("all");
  fileTypeFilter = $state(/** @type {"all" | "raw" | "jpeg"} */ ("all"));
  cameraFilter = $state("all");
  lensFilter = $state("all");
  dateFrom = $state("");
  dateTo = $state("");

  // Collection dialog flags (folded in from the old `collectionsUI` idea, RFC-0009 §8.4).
  creatingCollection = $state(false);
  creatingSmartCollection = $state(false);
  creatingCollectionWithImages = $state(false);
  pendingAddToCollectionImageIds = $state(/** @type {number[]} */ ([]));

  folderEntries = $derived(buildFolderEntries(this.images));

  /** The most recent import's batch id, or null before any tagged import
   * has happened (a pre-existing catalog whose rows predate this column).
   * `import_batch` is optional/nullable on ImageSummary, so this only
   * considers rows that actually have one. */
  lastImportBatchId = $derived.by(() => {
    let max = /** @type {number | null} */ (null);
    for (const img of this.images) {
      const batch = img.import_batch;
      if (batch != null && (max === null || batch > max)) max = batch;
    }
    return max;
  });

  keywordIdsByImage = $derived(buildKeywordIdsByImage(this.allImageKeywords));

  baseImages = $derived.by(() => selectBaseImages(this));

  cameraOptions = $derived(cameraOptionsFor(this.baseImages));
  lensOptions = $derived(lensOptionsFor(this.baseImages));

  filteredImages = $derived.by(() => applyLibraryFilters(this));

  activeCollection = $derived(this.collections.find((c) => c.id === this.activeCollectionId) ?? null);

  manualCollections = $derived(this.collections.filter((c) => !c.is_smart));
}

export function createLibraryStore() {
  return new LibraryStore();
}

export const library = createLibraryStore();
