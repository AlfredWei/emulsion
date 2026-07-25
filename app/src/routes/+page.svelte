<script>
  import "$lib/styles/tokens.css";
  import { open } from "@tauri-apps/plugin-dialog";
  import { onMount } from "svelte";
  import LibraryGrid from "$lib/components/LibraryGrid.svelte";
  import {
    importFolder,
    listImages,
    setRating,
    setFlag,
    setColorLabel,
  } from "$lib/api/catalog.js";

  /** @type {import('$lib/api/catalog.js').ImageSummary[]} */
  let images = $state([]);
  let selectedId = $state(/** @type {number | null} */ (null));
  let activeModule = $state("library"); // "library" | "develop"
  let importing = $state(false);
  let statusMessage = $state("");

  async function refresh() {
    images = await listImages();
  }

  async function handleImport() {
    const dir = await open({ directory: true, multiple: false });
    if (!dir) return;

    importing = true;
    statusMessage = "";
    try {
      const summary = await importFolder(/** @type {string} */ (dir));
      statusMessage = `Imported ${summary.imported}, ${summary.skipped_duplicates} already in library, ${summary.failed} failed`;
      await refresh();
    } catch (/** @type {any} */ e) {
      statusMessage = `Import failed: ${e}`;
    } finally {
      importing = false;
    }
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

  onMount(() => {
    refresh();
  });
</script>

<div class="app">
  <div class="titlebar">
    <div class="module-switch">
      <button class:active={activeModule === "library"} onclick={() => (activeModule = "library")}>
        Library
      </button>
      <button class:active={activeModule === "develop"} onclick={() => (activeModule = "develop")}>
        Develop
      </button>
    </div>
    <div class="spacer"></div>
    <button class="import-btn" onclick={handleImport} disabled={importing}>
      {importing ? "Importing…" : "Import…"}
    </button>
  </div>

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
          <button onclick={handleImport} disabled={importing}>Import a folder…</button>
        </div>
      {:else}
        <LibraryGrid
          {images}
          {selectedId}
          onSelect={(id) => (selectedId = id)}
          onRatingChange={handleRatingChange}
          onFlagChange={handleFlagChange}
          onColorLabelChange={handleColorLabelChange}
        />
      {/if}
    </div>
  {:else}
    <div class="placeholder">Develop — coming in M1 Slice 3</div>
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
  .status {
    flex: none;
    padding: 6px 14px;
    font-size: 11.5px;
    font-family: var(--font-mono);
    color: var(--text-secondary);
    background: var(--bg-panel);
    border-bottom: 1px solid var(--border-subtle);
  }
  .body {
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
