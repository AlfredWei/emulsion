<script>
  import { matchesRules } from "$lib/collectionRules.js";

  /**
   * The Catalog/Folders/Collections navigation rail, shared between
   * Library and People (M5 Slice 6 follow-up) so both modules browse the
   * same "All Photos / Last Import / a folder / a collection" source --
   * previously Library-only, which meant switching to People lost any
   * folder/collection you'd navigated to and dropped you back to
   * literally everything. Extracted verbatim out of +page.svelte's own
   * former inline rail markup; all the underlying state
   * (activeCollectionId/activeFolderKey/showLastImportOnly) still lives in
   * +page.svelte and is genuinely SHARED (not per-module-copied), so
   * picking a folder in one module and switching to the other keeps it
   * selected -- that's what "consistent with Library/Develop" means here,
   * not a second independent copy of the same UI.
   * @type {{
   *   images: import('$lib/api/catalog.js').ImageSummary[],
   *   activeCollectionId: number | null,
   *   activeFolderKey: string | null,
   *   showLastImportOnly: boolean,
   *   lastImportBatchId: number | null,
   *   folderEntries: { key: string, count: number }[],
   *   collections: import('$lib/api/catalog.js').CollectionSummary[],
   *   keywordIdsByImage: Map<number, Set<number>>,
   *   onSelectAllPhotos: () => void,
   *   onSelectLastImport: () => void,
   *   onSelectFolder: (key: string) => void,
   *   onSelectCollection: (id: number) => void,
   *   onDeleteCollection: (id: number, event: MouseEvent) => void,
   *   onCreateCollection: () => void,
   *   onCreateSmartCollection: () => void,
   * }}
   */
  let {
    images,
    activeCollectionId,
    activeFolderKey,
    showLastImportOnly,
    lastImportBatchId,
    folderEntries,
    collections,
    keywordIdsByImage,
    onSelectAllPhotos,
    onSelectLastImport,
    onSelectFolder,
    onSelectCollection,
    onDeleteCollection,
    onCreateCollection,
    onCreateSmartCollection,
  } = $props();
</script>

<div class="rail">
  <div class="section-label">Catalog</div>
  <button
    type="button"
    class="tree-item"
    class:active={activeCollectionId === null && activeFolderKey === null && !showLastImportOnly}
    onclick={onSelectAllPhotos}
  >
    All Photos
    <span class="count">{images.length}</span>
  </button>
  <button type="button" class="tree-item" class:active={showLastImportOnly} onclick={onSelectLastImport}>
    Last Import
    <span class="count">{lastImportBatchId === null ? 0 : images.filter((img) => img.import_batch === lastImportBatchId).length}</span>
  </button>

  {#if folderEntries.length > 0}
    <div class="section-label folders-label">Folders</div>
    {#each folderEntries as folder (folder.key)}
      <button
        type="button"
        class="tree-item"
        class:active={activeFolderKey === folder.key}
        onclick={() => onSelectFolder(folder.key)}
        title={folder.key}
      >
        <span class="tree-item-name">{folder.key}</span>
        <span class="count">{folder.count}</span>
      </button>
    {/each}
  {/if}

  <div class="collections-header">
    <span class="section-label">Collections</span>
    <span class="collections-actions">
      <button type="button" class="rail-action" title="New Collection" onclick={onCreateCollection}>+</button>
      <button type="button" class="rail-action" title="New Smart Collection" onclick={onCreateSmartCollection}>⚡+</button>
    </span>
  </div>
  {#each collections as collection (collection.id)}
    <div class="tree-item collection-item" class:active={activeCollectionId === collection.id}>
      <button type="button" class="tree-item-main" onclick={() => onSelectCollection(collection.id)}>
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
        onclick={(e) => onDeleteCollection(collection.id, e)}
      >×</button>
    </div>
  {/each}
</div>

<style>
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
  .folders-label {
    margin-top: 10px;
  }
  .collections-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding-right: 2px;
    margin-top: 10px;
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
</style>
