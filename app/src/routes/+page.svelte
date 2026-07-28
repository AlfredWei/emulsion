<script>
  import "$lib/styles/tokens.css";
  import { open } from "@tauri-apps/plugin-dialog";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { onMount } from "svelte";
  import LibraryGrid from "$lib/components/LibraryGrid.svelte";
  import DevelopCanvas from "$lib/components/DevelopCanvas.svelte";
  import DevelopPanel from "$lib/components/DevelopPanel.svelte";
  import ExportDialog from "$lib/components/ExportDialog.svelte";
  import ConfirmDialog from "$lib/components/ConfirmDialog.svelte";
  import TextPromptDialog from "$lib/components/TextPromptDialog.svelte";
  import SmartCollectionDialog from "$lib/components/SmartCollectionDialog.svelte";
  import MetadataPanel from "$lib/components/MetadataPanel.svelte";
  import {
    importFolder,
    importFiles,
    getSupportedExtensions,
    listImages,
    setRating,
    setFlag,
    setColorLabel,
    setCaption,
    setCopyright,
    setContact,
    removeImages,
    listAllImageKeywords,
    createCollection,
    createCollectionWithImages,
    createSmartCollection,
    updateSmartCollectionRules,
    deleteCollection,
    addImagesToCollection,
    removeImagesFromCollection,
    listCollections,
    listCollectionImageIds,
  } from "$lib/api/catalog.js";
  import { getEditStack, setEditStack, opValue, upsertOp } from "$lib/api/develop.js";
  import { buildKeywordIdsByImage, matchesRules } from "$lib/collectionRules.js";

  /** @type {import('$lib/api/catalog.js').ImageSummary[]} */
  let images = $state([]);
  // Multi-select (M2 Slice 3): `selectedIds` is the full selection,
  // `selectedId` stays as the anchor/primary -- the last plainly-clicked
  // image, which drives MetadataPanel, Shift-range endpoints, and any
  // single-image concern. Reassigned immutably on every change: Svelte 5's
  // $state doesn't deep-proxy Set, so in-place .add()/.delete() would
  // silently not react -- and reassignment matches the images-array idiom
  // used everywhere else in this file anyway.
  let selectedId = $state(/** @type {number | null} */ (null));
  let selectedIds = $state(/** @type {Set<number>} */ (new Set()));
  let confirmingRemoval = $state(false);
  let activeModule = $state("library"); // "library" | "develop"
  let importing = $state(false);
  let statusMessage = $state("");

  // Collections (M2 Slice 5). `activeCollectionId === null` means "All
  // Photos" (no filter). `manualMembership` caches a manual collection's
  // image_id membership by collection id, fetched on click -- invalidated
  // by whichever collection id was actually just mutated (add/remove-from-
  // collection), never by `activeCollectionId`: those differ exactly when
  // the toolbar adds a selection to a DIFFERENT collection than the one
  // currently being viewed, and invalidating the wrong one would silently
  // leave the mutated collection's cache stale.
  let collections = $state(/** @type {import('$lib/api/catalog.js').CollectionSummary[]} */ ([]));
  let activeCollectionId = $state(/** @type {number | null} */ (null));
  let manualMembership = $state(/** @type {Map<number, Set<number>>} */ (new Map()));
  let allImageKeywords = $state(/** @type {import('$lib/api/catalog.js').ImageKeywordAssignment[]} */ ([]));
  let keywordIdsByImage = $derived(buildKeywordIdsByImage(allImageKeywords));

  async function refreshCollections() {
    collections = await listCollections();
  }

  async function loadManualMembership(/** @type {number} */ collectionId) {
    const memberIds = await listCollectionImageIds(collectionId);
    manualMembership = new Map(manualMembership).set(collectionId, new Set(memberIds));
  }

  // The image set the Library grid actually shows -- unfiltered `images`
  // for "All Photos", a manual collection's fetched membership, or a
  // smart collection's rules evaluated client-side against the
  // already-loaded catalog.
  let filteredImages = $derived.by(() => {
    if (activeCollectionId === null) return images;
    const collection = collections.find((c) => c.id === activeCollectionId);
    if (!collection) return images;
    if (collection.is_smart) {
      const rules = collection.rules ?? [];
      return images.filter((img) => matchesRules(img, rules, keywordIdsByImage));
    }
    const memberIds = manualMembership.get(activeCollectionId);
    if (!memberIds) return []; // membership not fetched yet
    return images.filter((img) => memberIds.has(img.image_id));
  });

  let activeCollection = $derived(collections.find((c) => c.id === activeCollectionId) ?? null);

  async function selectCollection(/** @type {number | null} */ collectionId) {
    activeCollectionId = collectionId;
    if (collectionId !== null && !manualMembership.has(collectionId)) {
      const collection = collections.find((c) => c.id === collectionId);
      if (collection && !collection.is_smart) await loadManualMembership(collectionId);
    }
  }

  let developVersionId = $state(/** @type {number | null} */ (null));
  let developImagePath = $state("");
  /** @type {import('$lib/api/develop.js').EditStack} */
  let editStack = $state({ schema_version: 1, ops: [] });
  let exposure = $derived(opValue(editStack, "exposure", 0));
  let contrast = $derived(opValue(editStack, "contrast", 0));
  let saturation = $derived(opValue(editStack, "saturation", 0));

  // What Export would act on right now: the open Develop image, or every
  // selected Library image (M2 Slice 3 batch export -- the frontend-only
  // follow-up M1 Slice 5's export_batch was explicitly built to accept).
  let selectedImage = $derived(images.find((img) => img.version_id === selectedId) ?? null);
  let selectedImages = $derived(images.filter((img) => selectedIds.has(img.version_id)));
  let currentExportItems = $derived.by(() => {
    if (activeModule === "develop" && developVersionId !== null) {
      return [{ path: developImagePath, version_id: developVersionId }];
    }
    if (selectedImages.length > 0) {
      return selectedImages.map((img) => ({ path: img.path, version_id: img.version_id }));
    }
    return selectedImage ? [{ path: selectedImage.path, version_id: selectedImage.version_id }] : [];
  });
  let exportItems = $state(/** @type {{ path: string, version_id: number }[] | null} */ (null));

  // Who a newly-typed keyword in MetadataPanel gets assigned to (M2 Slice
  // 4): the whole current Library selection when there is one, else just
  // the anchor image -- unconditional on "the acted-on cell is part of
  // the selection" (unlike targetVersionIds below) since there's no
  // per-cell click event here, just "apply to whatever's selected".
  let keywordTargetImageIds = $derived(
    selectedImages.length > 0
      ? selectedImages.map((img) => img.image_id)
      : selectedImage
        ? [selectedImage.image_id]
        : [],
  );

  // Persistence is debounced (not written on every slider tick) so a drag
  // doesn't flood the catalog with writes -- flushed immediately whenever
  // navigation could otherwise lose the pending change (UX-DESIGN.md §5's
  // "coalesced/debounced slider events" rule, applied to catalog writes
  // rather than the WebGPU frame loop).
  let persistTimer = /** @type {ReturnType<typeof setTimeout> | null} */ (null);
  // Tracks an in-flight (already-fired, not-yet-resolved) save separately
  // from the debounce timer -- a flush can be triggered again (e.g. by the
  // close handler below) while a previous flush's write is still in
  // flight, and callers need to be able to wait for *that* too, not just
  // "is a timer currently pending".
  let pendingSave = /** @type {Promise<void> | null} */ (null);

  // M2 Slice 2: same shape as pendingSave above, but for the IPTC fields'
  // save-on-blur writes -- tracked separately since it's a different
  // in-flight write than the Develop edit stack's, and the close handler
  // below needs to wait on whichever (or both) are actually pending.
  let pendingIptcSave = /** @type {Promise<void> | null} */ (null);

  // M1 Slice 6: this used to NOT return setEditStack's promise, so every
  // `await flushEditStack()` call site (switchModule, openDevelop, Export)
  // resolved on the next microtask regardless of whether the write had
  // actually reached Rust/SQLite yet -- silently fire-and-forget. Fixed to
  // return the real promise; this is the fix the window-close flush below
  // actually depends on to mean anything.
  function flushEditStack() {
    if (persistTimer !== null) {
      clearTimeout(persistTimer);
      persistTimer = null;
      if (developVersionId !== null) {
        const versionId = developVersionId;
        const stack = editStack;
        pendingSave = setEditStack(versionId, stack).finally(() => {
          pendingSave = null;
        });
      }
    }
    return pendingSave ?? Promise.resolve();
  }

  async function refresh() {
    images = await listImages();
  }

  // Thumbnail generation for a freshly-imported JPEG (and the RAW backstop
  // path) runs as a fire-and-forget background pass on the Rust side (see
  // import.rs's generate_missing_thumbnails) -- it is NOT part of the
  // import command's own response, so the one-shot refresh() right after
  // import can only ever show the pre-generation state (thumbnail_path:
  // NULL). Nothing else ever tells this component to look again, so
  // without this, a just-imported photo's grid cell stays a blank
  // placeholder for the rest of the session, even once the backend has
  // long since finished. Bounded polling (not an open-ended interval) so a
  // permanently-stuck thumbnail (a real decode failure) doesn't poll
  // forever -- it just stops trying and leaves the placeholder, which is
  // the correct outcome in that case.
  let pollingThumbnails = false;
  async function pollUntilThumbnailsReady() {
    if (pollingThumbnails) return;
    pollingThumbnails = true;
    try {
      const maxAttempts = 10;
      const intervalMs = 1500;
      for (let attempt = 0; attempt < maxAttempts; attempt++) {
        if (!images.some((img) => img.thumbnail_path === null)) return;
        await new Promise((resolve) => setTimeout(resolve, intervalMs));
        await refresh();
      }
    } finally {
      pollingThumbnails = false;
    }
  }

  /** @type {string[] | null} */
  let supportedExtensions = $state(null);

  async function runImport(/** @type {() => Promise<import('$lib/api/catalog.js').ImportSummary | null>} */ doImport) {
    importing = true;
    statusMessage = "";
    try {
      const summary = await doImport();
      if (!summary) return; // user cancelled the dialog
      statusMessage = `Imported ${summary.imported}, ${summary.skipped_duplicates} already in library, ${summary.failed} failed`;
      await refresh();
      pollUntilThumbnailsReady();
    } catch (/** @type {any} */ e) {
      statusMessage = `Import failed: ${e}`;
    } finally {
      importing = false;
    }
  }

  function handleImportFolder() {
    return runImport(async () => {
      const dir = await open({ directory: true, multiple: false });
      return dir ? importFolder(/** @type {string} */ (dir)) : null;
    });
  }

  async function handleImportFiles() {
    // M2 Slice 1: a separate entry point from folder import -- Tauri's
    // dialog plugin has independent `directory`/`multiple` flags, no mode
    // that lets one native dialog pick either files or a folder.
    if (!supportedExtensions) supportedExtensions = await getSupportedExtensions();
    return runImport(async () => {
      const paths = await open({
        multiple: true,
        filters: [{ name: "Photos", extensions: /** @type {string[]} */ (supportedExtensions) }],
      });
      return paths ? importFiles(/** @type {string[]} */ (paths)) : null;
    });
  }

  // Optimistic local update (UX-DESIGN.md §5) so culling feels instant --
  // the write still goes to the real catalog, this just avoids waiting on
  // a round trip + full refetch before the UI reflects the change.
  function patchLocal(/** @type {number} */ versionId, /** @type {Partial<import('$lib/api/catalog.js').ImageSummary>} */ patch) {
    images = images.map((img) => (img.version_id === versionId ? { ...img, ...patch } : img));
  }

  // Batch rate/flag/color-label (MILESTONES.md M2 scope, deferred from
  // Slice 3's multi-select work): Lightroom-style -- acting on a cell that
  // is part of an active multi-selection applies to the whole selection,
  // not just that one cell. Acting on a cell OUTSIDE the current
  // selection (or when only one image is selected) stays single-target,
  // unaffected by an unrelated selection elsewhere -- matches the mental
  // model that the star/flag/color-dot buttons are just this practice's
  // culling controls, only shown in a bulk light when your selection
  // literally includes the cell you clicked.
  function targetVersionIds(/** @type {number} */ versionId) {
    return selectedIds.size > 1 && selectedIds.has(versionId) ? [...selectedIds] : [versionId];
  }

  async function handleRatingChange(/** @type {number} */ versionId, /** @type {number} */ rating) {
    const targets = targetVersionIds(versionId);
    for (const id of targets) patchLocal(id, { rating });
    await Promise.all(targets.map((id) => setRating(id, rating)));
  }

  async function handleFlagChange(/** @type {number} */ versionId, /** @type {string} */ flag) {
    const targets = targetVersionIds(versionId);
    for (const id of targets) patchLocal(id, { flag });
    await Promise.all(targets.map((id) => setFlag(id, flag)));
  }

  async function handleColorLabelChange(/** @type {number} */ versionId, /** @type {string} */ colorLabel) {
    const targets = targetVersionIds(versionId);
    for (const id of targets) patchLocal(id, { color_label: colorLabel });
    await Promise.all(targets.map((id) => setColorLabel(id, colorLabel)));
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
  function handleSelect(/** @type {number} */ versionId, /** @type {MouseEvent=} */ event) {
    if (event?.shiftKey && selectedId !== null) {
      const anchorIndex = filteredImages.findIndex((img) => img.version_id === selectedId);
      const clickedIndex = filteredImages.findIndex((img) => img.version_id === versionId);
      if (anchorIndex !== -1 && clickedIndex !== -1) {
        const [from, to] = anchorIndex <= clickedIndex ? [anchorIndex, clickedIndex] : [clickedIndex, anchorIndex];
        selectedIds = new Set(filteredImages.slice(from, to + 1).map((img) => img.version_id));
        return; // anchor stays put, Lightroom/Finder-style
      }
      // Stale anchor (e.g. it was just removed, or is outside the current
      // filter): fall through to plain select.
    }
    if (event?.metaKey || event?.ctrlKey) {
      const next = new Set(selectedIds);
      if (next.has(versionId)) {
        next.delete(versionId);
        if (selectedId === versionId) {
          selectedId = next.size > 0 ? [...next][next.size - 1] : null;
        }
      } else {
        next.add(versionId);
        selectedId = versionId;
      }
      selectedIds = next;
      return;
    }
    selectedId = versionId;
    selectedIds = new Set([versionId]);
  }

  // Non-destructive removal (M2 Slice 3): catalog rows + app-owned derived
  // files only -- the backend never touches source files. `await` the
  // command BEFORE filtering local state: the other order would let an
  // in-flight pollUntilThumbnailsReady refresh() momentarily resurrect the
  // removed rows in the UI.
  async function handleRemoveConfirmed() {
    confirmingRemoval = false;
    // Symmetry with the close handler: force any in-progress IPTC edit's
    // blur-save to fire before the rows it targets can disappear.
    /** @type {HTMLElement | null} */ (document.activeElement)?.blur();

    const imageIds = [...new Set(selectedImages.map((img) => img.image_id))];
    if (imageIds.length === 0) return;
    try {
      await removeImages(imageIds);
    } catch (/** @type {any} */ e) {
      statusMessage = `Remove failed: ${e}`;
      return;
    }
    const removedVersionIds = new Set(selectedImages.map((img) => img.version_id));
    images = images.filter((img) => !removedVersionIds.has(img.version_id));
    statusMessage = `Removed ${imageIds.length} photo${imageIds.length === 1 ? "" : "s"} from catalog`;
    selectedId = null;
    selectedIds = new Set();
    // If the image open in Develop was just removed, clear that state too --
    // otherwise the develop branch keeps rendering a deleted image, and a
    // pending debounced edit-stack save would fire a pointless IPC call
    // against the deleted version.
    if (developVersionId !== null && removedVersionIds.has(developVersionId)) {
      if (persistTimer !== null) {
        clearTimeout(persistTimer);
        persistTimer = null;
      }
      developVersionId = null;
      developImagePath = "";
    }
  }

  // Collections (M2 Slice 5). Rename/edit-existing-smart-collection-rules
  // UI is deliberately deferred (matching this codebase's precedent for
  // `add_image_with_metadata`/`add_edit_stack` -- a lower-level building
  // block kept ready without a UI trigger yet): the rail's "+" only ever
  // creates fresh collections; changing an existing one means delete and
  // recreate for now.
  let creatingCollection = $state(false);
  let creatingSmartCollection = $state(false);
  let creatingCollectionWithImages = $state(false);
  let pendingAddToCollectionImageIds = $state(/** @type {number[]} */ ([]));
  let manualCollections = $derived(collections.filter((c) => !c.is_smart));

  async function handleCreateCollection(/** @type {string} */ name) {
    creatingCollection = false;
    await createCollection(name);
    await refreshCollections();
  }

  async function handleCreateSmartCollection(
    /** @type {string} */ name,
    /** @type {import('$lib/api/catalog.js').CollectionRule[]} */ rules,
  ) {
    creatingSmartCollection = false;
    await createSmartCollection(name, rules);
    await refreshCollections();
  }

  async function handleDeleteCollection(/** @type {number} */ collectionId, /** @type {MouseEvent} */ event) {
    event.stopPropagation(); // don't also trigger selectCollection
    await deleteCollection(collectionId);
    if (activeCollectionId === collectionId) activeCollectionId = null;
    await refreshCollections();
  }

  // "Add to Collection…" toolbar picker, from a multi-selection.
  async function handleAddToCollectionSelect(/** @type {string} */ value) {
    if (!value) return;
    const imageIds = [...new Set(selectedImages.map((img) => img.image_id))];
    if (imageIds.length === 0) return;
    if (value === "__new__") {
      pendingAddToCollectionImageIds = imageIds;
      creatingCollectionWithImages = true;
      return;
    }
    const collectionId = Number(value);
    await addImagesToCollection(collectionId, imageIds);
    // Invalidate by the collection id that was actually just mutated, not
    // by activeCollectionId -- those differ when adding to a DIFFERENT
    // collection than the one currently being viewed, and invalidating
    // the wrong one would silently leave the mutated one's cache stale.
    if (manualMembership.has(collectionId)) await loadManualMembership(collectionId);
    await refreshCollections();
    statusMessage = `Added ${imageIds.length} photo${imageIds.length === 1 ? "" : "s"} to collection`;
  }

  async function handleCreateCollectionWithImages(/** @type {string} */ name) {
    creatingCollectionWithImages = false;
    const imageIds = pendingAddToCollectionImageIds;
    pendingAddToCollectionImageIds = [];
    await createCollectionWithImages(name, imageIds);
    await refreshCollections();
    statusMessage = `Added ${imageIds.length} photo${imageIds.length === 1 ? "" : "s"} to "${name}"`;
  }

  async function handleRemoveFromCollection() {
    if (activeCollectionId === null) return;
    const imageIds = [...new Set(selectedImages.map((img) => img.image_id))];
    if (imageIds.length === 0) return;
    await removeImagesFromCollection(activeCollectionId, imageIds);
    await loadManualMembership(activeCollectionId); // mutated === active here, still the right id
    await refreshCollections();
    const removedVersionIds = new Set(selectedImages.map((img) => img.version_id));
    selectedIds = new Set([...selectedIds].filter((id) => !removedVersionIds.has(id)));
    if (selectedId !== null && removedVersionIds.has(selectedId)) selectedId = null;
    statusMessage = `Removed ${imageIds.length} photo${imageIds.length === 1 ? "" : "s"} from collection`;
  }

  function handleLibraryKeydown(/** @type {KeyboardEvent} */ e) {
    if (e.key !== "Delete" && e.key !== "Backspace") return;
    if (activeModule !== "library") return;
    if (confirmingRemoval || exportItems !== null) return;
    if (selectedIds.size === 0) return;
    // Backspace is a typing key in MetadataPanel's fields -- never treat
    // it as "remove photos" while an editable element has focus.
    const target = e.target;
    if (target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement) return;
    e.preventDefault();
    confirmingRemoval = true;
  }

  // M2 Slice 2: IPTC fields save on blur (MetadataPanel), not debounced --
  // each is a single discrete edit rather than a slider drag, so there's no
  // flood of writes to coalesce. Still tracked via pendingIptcSave so the
  // close handler can wait for an in-flight write the same way it already
  // does for the Develop edit stack.
  function handleCaptionChange(/** @type {number} */ versionId, /** @type {string} */ caption) {
    patchLocal(versionId, { caption });
    pendingIptcSave = setCaption(versionId, caption).finally(() => {
      pendingIptcSave = null;
    });
  }

  function handleCopyrightChange(/** @type {number} */ imageId, /** @type {string} */ copyright) {
    images = images.map((img) => (img.image_id === imageId ? { ...img, copyright } : img));
    pendingIptcSave = setCopyright(imageId, copyright).finally(() => {
      pendingIptcSave = null;
    });
  }

  function handleContactChange(/** @type {number} */ imageId, /** @type {string} */ contact) {
    images = images.map((img) => (img.image_id === imageId ? { ...img, contact } : img));
    pendingIptcSave = setContact(imageId, contact).finally(() => {
      pendingIptcSave = null;
    });
  }

  async function openDevelop(/** @type {number} */ versionId) {
    flushEditStack();
    const image = images.find((img) => img.version_id === versionId);
    if (!image) return;
    developVersionId = versionId;
    developImagePath = image.path;
    editStack = await getEditStack(versionId);
    activeModule = "develop";
  }

  function switchModule(/** @type {string} */ target) {
    if (activeModule === "develop" && target !== "develop") flushEditStack();
    activeModule = target;
  }

  function handleAdjustmentChange(/** @type {string} */ opName, /** @type {number} */ value) {
    editStack = upsertOp(editStack, opName, value);
    if (persistTimer) clearTimeout(persistTimer);
    persistTimer = setTimeout(flushEditStack, 250);
  }

  function handleExportClick() {
    // If a slider was just dragged, the debounced save may not have
    // landed yet -- flush it first so Export reads the value currently
    // on screen, not the last-persisted one.
    if (activeModule === "develop") flushEditStack();
    // null stays the "closed" sentinel -- never open with an empty list.
    exportItems = currentExportItems.length > 0 ? currentExportItems : null;
  }

  onMount(() => {
    // Also covers the startup catch-up pass (preview_cache::pregenerate_missing
    // / import::generate_missing_thumbnails, both run once in lib.rs's
    // .setup()): this refresh() races against that pass the same way an
    // import's own refresh() races against its own background trigger.
    refresh().then(pollUntilThumbnailsReady);
    refreshCollections();
    listAllImageKeywords().then((assignments) => (allImageKeywords = assignments));

    // M1 Slice 6 (crash-safety): flush a pending debounced edit before the
    // window actually closes, so quitting right after a slider drag can't
    // lose it. Only intervenes when something is actually pending -- the
    // common case (nothing to flush) closes immediately, no added latency.
    // This protects a *graceful* quit only (close-button click, or another
    // OS "please close" request that routes through the same
    // closeRequested pipeline `.close()` itself uses, per Tauri's own
    // docs) -- it cannot help against SIGKILL/a hard crash, which bypasses
    // every in-process handler. Whether macOS Cmd+Q routes through this
    // same path is unverified in this environment.
    let unlistenClose = /** @type {(() => void) | undefined} */ (undefined);
    getCurrentWindow()
      .onCloseRequested(async (event) => {
        // M2 Slice 2: an IPTC field saves on blur, so a value typed but not
        // yet blurred (e.g. the user clicks the window's close button while
        // still focused in the Caption textarea) needs to be forced to save
        // before the pending-work check below -- otherwise it's silently
        // lost, the same class of bug fixed for the Develop edit stack.
        /** @type {HTMLElement | null} */ (document.activeElement)?.blur();
        if (persistTimer === null && pendingSave === null && pendingIptcSave === null) return;
        event.preventDefault();
        await Promise.all([flushEditStack(), pendingIptcSave ?? Promise.resolve()]);
        await getCurrentWindow().destroy();
      })
      .then((fn) => {
        unlistenClose = fn;
      });

    return () => unlistenClose?.();
  });
</script>

<svelte:window onkeydown={handleLibraryKeydown} />

<div class="app">
  <div class="titlebar">
    <div class="module-switch">
      <button class:active={activeModule === "library"} onclick={() => switchModule("library")}>
        Library
      </button>
      <button class:active={activeModule === "develop"} onclick={() => switchModule("develop")}>
        Develop
      </button>
    </div>
    <div class="spacer"></div>
    {#if activeModule === "library" && activeCollectionId !== null && activeCollection && !activeCollection.is_smart}
      <button class="remove-btn" onclick={handleRemoveFromCollection} disabled={selectedIds.size === 0}>
        Remove from Collection{selectedIds.size > 1 ? ` (${selectedIds.size})` : ""}
      </button>
    {/if}
    <select
      class="add-to-collection-select"
      value=""
      disabled={activeModule !== "library" || selectedIds.size === 0}
      onchange={(e) => {
        handleAddToCollectionSelect(e.currentTarget.value);
        e.currentTarget.value = "";
      }}
    >
      <option value="" disabled>Add to Collection…</option>
      {#each manualCollections as collection (collection.id)}
        <option value={collection.id}>{collection.name}</option>
      {/each}
      <option value="__new__">New Collection…</option>
    </select>
    <button
      class="remove-btn"
      onclick={() => (confirmingRemoval = true)}
      disabled={activeModule !== "library" || selectedIds.size === 0}
    >
      Remove{selectedIds.size > 1 ? ` (${selectedIds.size})` : ""}
    </button>
    <button class="export-btn" onclick={handleExportClick} disabled={currentExportItems.length === 0}>
      Export{currentExportItems.length > 1 ? ` (${currentExportItems.length})` : ""}…
    </button>
    <button class="import-btn secondary" onclick={handleImportFiles} disabled={importing}>
      {importing ? "Importing…" : "Import Files…"}
    </button>
    <button class="import-btn" onclick={handleImportFolder} disabled={importing}>
      {importing ? "Importing…" : "Import Folder…"}
    </button>
  </div>

  <ExportDialog items={exportItems} onClose={() => (exportItems = null)} />

  <ConfirmDialog
    open={confirmingRemoval}
    title="Remove from catalog"
    message={`Remove ${selectedIds.size} photo${selectedIds.size === 1 ? "" : "s"} from the catalog? Source files stay on disk; edits, ratings, and metadata stored in the catalog are discarded.`}
    confirmLabel="Remove"
    onConfirm={handleRemoveConfirmed}
    onCancel={() => (confirmingRemoval = false)}
  />

  <TextPromptDialog
    open={creatingCollection}
    title="New Collection"
    label="Name"
    placeholder="e.g. Portfolio"
    confirmLabel="Create"
    onConfirm={handleCreateCollection}
    onCancel={() => (creatingCollection = false)}
  />

  <TextPromptDialog
    open={creatingCollectionWithImages}
    title="New Collection"
    label="Name"
    placeholder="e.g. Portfolio"
    confirmLabel="Create"
    onConfirm={handleCreateCollectionWithImages}
    onCancel={() => {
      creatingCollectionWithImages = false;
      pendingAddToCollectionImageIds = [];
    }}
  />

  <SmartCollectionDialog
    open={creatingSmartCollection}
    title="New Smart Collection"
    confirmLabel="Create"
    onConfirm={handleCreateSmartCollection}
    onCancel={() => (creatingSmartCollection = false)}
  />

  {#if statusMessage}
    <div class="status">{statusMessage}</div>
  {/if}

  {#if activeModule === "library"}
    <div class="body">
      <div class="rail">
        <div class="section-label">Folders</div>
        <button type="button" class="tree-item" class:active={activeCollectionId === null} onclick={() => selectCollection(null)}>
          All Photos
          <span class="count">{images.length}</span>
        </button>

        <div class="collections-header">
          <span class="section-label">Collections</span>
          <span class="collections-actions">
            <button type="button" class="rail-action" title="New Collection" onclick={() => (creatingCollection = true)}>+</button>
            <button type="button" class="rail-action" title="New Smart Collection" onclick={() => (creatingSmartCollection = true)}>⚡+</button>
          </span>
        </div>
        {#each collections as collection (collection.id)}
          <div class="tree-item collection-item" class:active={activeCollectionId === collection.id}>
            <button type="button" class="tree-item-main" onclick={() => selectCollection(collection.id)}>
              {#if collection.is_smart}<span class="smart-icon" title="Smart Collection">⚡</span>{/if}
              <span class="tree-item-name">{collection.name}</span>
              <span class="count">
                {collection.is_smart
                  ? images.filter((img) => matchesRules(img, collection.rules ?? [], keywordIdsByImage)).length
                  : (collection.count ?? 0)}
              </span>
            </button>
            <button
              type="button"
              class="tree-item-delete"
              aria-label="Delete collection {collection.name}"
              onclick={(e) => handleDeleteCollection(collection.id, e)}
            >×</button>
          </div>
        {/each}
      </div>

      {#if images.length === 0}
        <div class="empty">
          <p>No photos yet.</p>
          <div class="empty-actions">
            <button onclick={handleImportFolder} disabled={importing}>Import a folder…</button>
            <button class="secondary" onclick={handleImportFiles} disabled={importing}>Import files…</button>
          </div>
        </div>
      {:else if filteredImages.length === 0}
        <div class="empty">
          <p>No photos in this collection.</p>
        </div>
      {:else}
        <LibraryGrid
          images={filteredImages}
          {selectedIds}
          onSelect={handleSelect}
          onOpen={openDevelop}
          onRatingChange={handleRatingChange}
          onFlagChange={handleFlagChange}
          onColorLabelChange={handleColorLabelChange}
        />
      {/if}

      <MetadataPanel
        image={selectedImage}
        targetImageIds={keywordTargetImageIds}
        onCaptionChange={(caption) => selectedId !== null && handleCaptionChange(selectedId, caption)}
        onCopyrightChange={(copyright) =>
          selectedImage && handleCopyrightChange(selectedImage.image_id, copyright)}
        onContactChange={(contact) =>
          selectedImage && handleContactChange(selectedImage.image_id, contact)}
        onKeywordAssigned={(name, count) =>
          (statusMessage = `Added "${name}" to ${count} photo${count === 1 ? "" : "s"}`)}
      />
    </div>
  {:else if developImagePath}
    <div class="develop-body">
      <DevelopCanvas imagePath={developImagePath} {exposure} {contrast} {saturation} />
      <DevelopPanel
        {exposure}
        {contrast}
        {saturation}
        onExposureChange={(v) => handleAdjustmentChange("exposure", v)}
        onContrastChange={(v) => handleAdjustmentChange("contrast", v)}
        onSaturationChange={(v) => handleAdjustmentChange("saturation", v)}
      />
    </div>
  {:else}
    <div class="placeholder">Double-click a photo in Library to open it here.</div>
  {/if}
</div>

<style>
  .app {
    display: flex;
    flex-direction: column;
    height: 100vh;
  }
  .titlebar {
    display: flex;
    align-items: center;
    gap: 14px;
    height: 42px;
    flex: none;
    padding: 0 14px;
    background: var(--bg-app);
    border-bottom: 1px solid var(--border-subtle);
  }
  .module-switch {
    display: flex;
    background: var(--bg-panel);
    border: 1px solid var(--border-subtle);
    border-radius: 6px;
    padding: 2px;
    gap: 2px;
  }
  .module-switch button {
    all: unset;
    cursor: pointer;
    padding: 5px 14px;
    font-size: 11.5px;
    font-weight: 600;
    border-radius: 4px;
    color: var(--text-secondary);
  }
  .module-switch button.active {
    background: var(--bg-panel-raised);
    color: var(--text-primary);
  }
  .spacer {
    flex: 1;
  }
  .import-btn {
    all: unset;
    cursor: pointer;
    padding: 6px 14px;
    font-size: 11.5px;
    font-weight: 600;
    border-radius: 6px;
    background: var(--accent-soft);
    color: var(--accent-strong);
    border: 1px solid var(--accent);
  }
  .import-btn:disabled {
    opacity: 0.6;
    cursor: default;
  }
  .import-btn.secondary {
    background: transparent;
    color: var(--text-secondary);
    border: 1px solid var(--border-strong);
  }
  .export-btn,
  .remove-btn {
    all: unset;
    cursor: pointer;
    padding: 6px 14px;
    font-size: 11.5px;
    font-weight: 600;
    border-radius: 6px;
    color: var(--text-secondary);
    border: 1px solid var(--border-strong);
  }
  .remove-btn:not(:disabled):hover {
    color: var(--label-red);
    border-color: var(--label-red);
  }
  .export-btn:disabled,
  .remove-btn:disabled {
    opacity: 0.6;
    cursor: default;
  }
  .status {
    flex: none;
    padding: 6px 14px;
    font-size: 11.5px;
    font-family: var(--font-mono);
    color: var(--text-secondary);
    background: var(--bg-panel);
    border-bottom: 1px solid var(--border-subtle);
  }
  .body,
  .develop-body {
    flex: 1;
    display: flex;
    min-height: 0;
  }
  .rail {
    width: 200px;
    flex: none;
    background: var(--bg-panel);
    border-right: 1px solid var(--border-subtle);
    padding: 14px 10px;
    /* M2 Slice 5: the rail was a fixed 2-line static block until now --
       a variable-length Collections list needs to scroll instead of
       spilling past the box's bottom edge. */
    overflow-y: auto;
    overflow-x: hidden;
  }
  .section-label {
    font-family: var(--font-mono);
    font-size: 10px;
    letter-spacing: 0.09em;
    text-transform: uppercase;
    color: var(--text-tertiary);
    padding: 4px;
    font-weight: 600;
  }
  /* .tree-item is a <button> now (was a plain <div>) so "All Photos" and
     collections are real click targets -- reset button chrome so it
     still reads as the same flat row it always has. */
  .tree-item {
    all: unset;
    box-sizing: border-box;
    width: 100%;
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 5px 7px;
    border-radius: var(--radius-s);
    color: var(--text-secondary);
    font-size: 12px;
    cursor: pointer;
  }
  .tree-item.active {
    background: var(--accent-soft);
    color: var(--accent-strong);
  }
  .tree-item .count {
    margin-left: auto;
    font-family: var(--font-mono);
    font-size: 10.5px;
    color: var(--text-tertiary);
  }
  .collections-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding-right: 2px;
  }
  .collections-actions {
    display: flex;
    gap: 2px;
  }
  .rail-action {
    all: unset;
    cursor: pointer;
    padding: 2px 5px;
    font-size: 11px;
    border-radius: var(--radius-s);
    color: var(--text-tertiary);
  }
  .rail-action:hover {
    color: var(--accent-strong);
    background: var(--accent-soft);
  }
  .collection-item {
    display: flex;
    align-items: center;
    border-radius: var(--radius-s);
  }
  .collection-item.active {
    background: var(--accent-soft);
  }
  .collection-item .tree-item-main {
    flex: 1;
    min-width: 0;
  }
  .tree-item-main {
    all: unset;
    box-sizing: border-box;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 5px 7px;
    color: var(--text-secondary);
    font-size: 12px;
    cursor: pointer;
  }
  .collection-item.active .tree-item-main {
    color: var(--accent-strong);
  }
  .tree-item-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .smart-icon {
    flex: none;
    font-size: 10px;
  }
  .tree-item-delete {
    all: unset;
    cursor: pointer;
    flex: none;
    padding: 0 7px 0 2px;
    color: var(--text-tertiary);
    opacity: 0;
  }
  .collection-item:hover .tree-item-delete {
    opacity: 1;
  }
  .tree-item-delete:hover {
    color: var(--label-red);
  }
  .add-to-collection-select {
    all: unset;
    box-sizing: border-box;
    cursor: pointer;
    padding: 6px 10px;
    font-size: 11.5px;
    font-weight: 600;
    border-radius: 6px;
    color: var(--text-secondary);
    border: 1px solid var(--border-strong);
  }
  .add-to-collection-select:disabled {
    opacity: 0.6;
    cursor: default;
  }
  .empty {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 12px;
    color: var(--text-secondary);
  }
  .empty-actions {
    display: flex;
    gap: 8px;
  }
  .empty button {
    all: unset;
    cursor: pointer;
    padding: 8px 16px;
    font-size: 12px;
    font-weight: 600;
    border-radius: 6px;
    background: var(--accent-soft);
    color: var(--accent-strong);
    border: 1px solid var(--accent);
  }
  .empty button.secondary {
    background: transparent;
    color: var(--text-secondary);
    border: 1px solid var(--border-strong);
  }
  .placeholder {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-tertiary);
    font-family: var(--font-mono);
    font-size: 12px;
  }
</style>
