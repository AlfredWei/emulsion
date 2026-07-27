<script>
  import "$lib/styles/tokens.css";
  import { open } from "@tauri-apps/plugin-dialog";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { onMount } from "svelte";
  import LibraryGrid from "$lib/components/LibraryGrid.svelte";
  import DevelopCanvas from "$lib/components/DevelopCanvas.svelte";
  import DevelopPanel from "$lib/components/DevelopPanel.svelte";
  import ExportDialog from "$lib/components/ExportDialog.svelte";
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
  } from "$lib/api/catalog.js";
  import { getEditStack, setEditStack, opValue, upsertOp } from "$lib/api/develop.js";

  /** @type {import('$lib/api/catalog.js').ImageSummary[]} */
  let images = $state([]);
  let selectedId = $state(/** @type {number | null} */ (null));
  let activeModule = $state("library"); // "library" | "develop"
  let importing = $state(false);
  let statusMessage = $state("");

  let developVersionId = $state(/** @type {number | null} */ (null));
  let developImagePath = $state("");
  /** @type {import('$lib/api/develop.js').EditStack} */
  let editStack = $state({ schema_version: 1, ops: [] });
  let exposure = $derived(opValue(editStack, "exposure", 0));
  let contrast = $derived(opValue(editStack, "contrast", 0));
  let saturation = $derived(opValue(editStack, "saturation", 0));

  // The image Export would act on right now: the open Develop image, or
  // the selected Library image -- whichever module is active.
  let selectedImage = $derived(images.find((img) => img.version_id === selectedId) ?? null);
  let currentExportItem = $derived(
    activeModule === "develop" && developVersionId !== null
      ? { path: developImagePath, version_id: developVersionId }
      : selectedImage
        ? { path: selectedImage.path, version_id: selectedImage.version_id }
        : null,
  );
  let exportItem = $state(/** @type {{ path: string, version_id: number } | null} */ (null));

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

  async function handleRatingChange(/** @type {number} */ versionId, /** @type {number} */ rating) {
    patchLocal(versionId, { rating });
    await setRating(versionId, rating);
  }

  async function handleFlagChange(/** @type {number} */ versionId, /** @type {string} */ flag) {
    patchLocal(versionId, { flag });
    await setFlag(versionId, flag);
  }

  async function handleColorLabelChange(/** @type {number} */ versionId, /** @type {string} */ colorLabel) {
    patchLocal(versionId, { color_label: colorLabel });
    await setColorLabel(versionId, colorLabel);
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
    exportItem = currentExportItem;
  }

  onMount(() => {
    // Also covers the startup catch-up pass (preview_cache::pregenerate_missing
    // / import::generate_missing_thumbnails, both run once in lib.rs's
    // .setup()): this refresh() races against that pass the same way an
    // import's own refresh() races against its own background trigger.
    refresh().then(pollUntilThumbnailsReady);

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
    <button class="export-btn" onclick={handleExportClick} disabled={!currentExportItem}>
      Export…
    </button>
    <button class="import-btn secondary" onclick={handleImportFiles} disabled={importing}>
      {importing ? "Importing…" : "Import Files…"}
    </button>
    <button class="import-btn" onclick={handleImportFolder} disabled={importing}>
      {importing ? "Importing…" : "Import Folder…"}
    </button>
  </div>

  <ExportDialog item={exportItem} onClose={() => (exportItem = null)} />

  {#if statusMessage}
    <div class="status">{statusMessage}</div>
  {/if}

  {#if activeModule === "library"}
    <div class="body">
      <div class="rail">
        <div class="section-label">Folders</div>
        <div class="tree-item active">
          All Photos
          <span class="count">{images.length}</span>
        </div>
      </div>

      {#if images.length === 0}
        <div class="empty">
          <p>No photos yet.</p>
          <div class="empty-actions">
            <button onclick={handleImportFolder} disabled={importing}>Import a folder…</button>
            <button class="secondary" onclick={handleImportFiles} disabled={importing}>Import files…</button>
          </div>
        </div>
      {:else}
        <LibraryGrid
          {images}
          {selectedId}
          onSelect={(id) => (selectedId = id)}
          onOpen={openDevelop}
          onRatingChange={handleRatingChange}
          onFlagChange={handleFlagChange}
          onColorLabelChange={handleColorLabelChange}
        />
      {/if}

      <MetadataPanel
        image={selectedImage}
        onCaptionChange={(caption) => selectedId !== null && handleCaptionChange(selectedId, caption)}
        onCopyrightChange={(copyright) =>
          selectedImage && handleCopyrightChange(selectedImage.image_id, copyright)}
        onContactChange={(contact) =>
          selectedImage && handleContactChange(selectedImage.image_id, contact)}
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
  .export-btn {
    all: unset;
    cursor: pointer;
    padding: 6px 14px;
    font-size: 11.5px;
    font-weight: 600;
    border-radius: 6px;
    color: var(--text-secondary);
    border: 1px solid var(--border-strong);
  }
  .export-btn:disabled {
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
  .tree-item {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 5px 7px;
    border-radius: var(--radius-s);
    color: var(--text-secondary);
    font-size: 12px;
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
