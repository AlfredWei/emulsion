<script>
  import { matchesRules } from "$lib/collectionRules.js";
  import { getStoredRailSections, saveStoredRailSections } from "$lib/railSections.js";

  /**
   * The Catalog/Folders/Collections/People navigation rail, shared between
   * Library and People (M5 Slice 6 follow-up) so both modules browse the
   * same "All Photos / Last Import / a folder / a collection / a person"
   * source -- previously Library-only, which meant switching to People
   * lost any folder/collection you'd navigated to and dropped you back to
   * literally everything. Extracted verbatim out of +page.svelte's own
   * former inline rail markup; all the underlying state
   * (activeCollectionId/activeFolderKey/showLastImportOnly/activePersonId)
   * still lives in +page.svelte and is genuinely SHARED (not per-module-
   * copied), so picking a folder in one module and switching to the other
   * keeps it selected -- that's what "consistent with Library/Develop"
   * means here, not a second independent copy of the same UI.
   *
   * Folders/Collections/People are each individually collapsible (2026-
   * 09-18 user request) -- collapsed state is pure UI chrome, persisted
   * via railSections.js's localStorage helper, not lifted to +page.svelte
   * since nothing outside this component needs to know it.
   *
   * The People section's avatar crop (`avatarUrls`, keyed by
   * `cover_image_path` not person id, since several people can share a
   * cover photo) reuses the exact CSS background-position/size trick the
   * pre-2026-09-17 standalone PeopleGrid used, just at rail-row scale
   * instead of a grid card -- decoding stays in +page.svelte, this only
   * crops.
   * @type {{
   *   images: import('$lib/api/catalog.js').ImageSummary[],
   *   activeCollectionId: number | null,
   *   activeFolderKey: string | null,
   *   showLastImportOnly: boolean,
   *   activePersonId: number | null,
   *   activeMapImageIds: Set<number> | null,
   *   lastImportBatchId: number | null,
   *   folderEntries: { key: string, count: number }[],
   *   collections: import('$lib/api/catalog.js').CollectionSummary[],
   *   keywordIdsByImage: Map<number, Set<number>>,
   *   people: import('$lib/api/faces.js').PersonRow[],
   *   avatarUrls: Record<string, string | null>,
   *   onSelectAllPhotos: () => void,
   *   onSelectLastImport: () => void,
   *   onSelectFolder: (key: string) => void,
   *   onSelectCollection: (id: number) => void,
   *   onDeleteCollection: (id: number, event: MouseEvent) => void,
   *   onCreateCollection: () => void,
   *   onCreateSmartCollection: () => void,
   *   onSelectPerson: (id: number) => void,
   *   onRenamePerson: (id: number, name: string | null) => void,
   * }}
   */
  let {
    images,
    activeCollectionId,
    activeFolderKey,
    showLastImportOnly,
    activePersonId,
    activeMapImageIds,
    lastImportBatchId,
    folderEntries,
    collections,
    keywordIdsByImage,
    people,
    avatarUrls,
    onSelectAllPhotos,
    onSelectLastImport,
    onSelectFolder,
    onSelectCollection,
    onDeleteCollection,
    onCreateCollection,
    onCreateSmartCollection,
    onSelectPerson,
    onRenamePerson,
  } = $props();

  let expanded = $state(getStoredRailSections());
  $effect(() => saveStoredRailSections(expanded));

  /** @param {"folders" | "collections" | "people"} section */
  function toggleSection(section) {
    expanded = { ...expanded, [section]: !expanded[section] };
  }

  const PERSON_AVATAR_SIZE = 20;

  let editingPersonId = $state(/** @type {number | null} */ (null));
  let editingPersonName = $state("");

  function startEditingPerson(/** @type {import('$lib/api/faces.js').PersonRow} */ person) {
    editingPersonId = person.id;
    editingPersonName = person.name ?? "";
  }

  function commitEditingPerson() {
    if (editingPersonId === null) return;
    const id = editingPersonId;
    const trimmed = editingPersonName.trim();
    editingPersonId = null;
    onRenamePerson(id, trimmed || null);
  }

  function cancelEditingPerson() {
    editingPersonId = null;
  }

  /** @param {HTMLInputElement} node */
  function focusAndSelect(node) {
    node.focus();
    node.select();
  }

  /** Double-click-to-filter, EXCEPT when the click landed on the name
   * label/input (that's the single-click-to-rename target) -- otherwise a
   * rapid double-click on the name itself would both open the rename
   * input on the first click and fire this row-level filter on the
   * second, since dblclick bubbles from whatever child was actually hit.
   * @param {MouseEvent} event
   * @param {number} personId */
  function handlePersonRowDblClick(event, personId) {
    const target = /** @type {HTMLElement} */ (event.target);
    if (target.closest(".person-name, .person-name-input")) return;
    onSelectPerson(personId);
  }

  /** Crops `avatarUrls[person.cover_image_path]` down to just
   * `cover_bbox_*` and scales that crop to fill a fixed
   * `PERSON_AVATAR_SIZE` circle, entirely via `background-size`/
   * `background-position` in pixels -- same math the pre-fold PeopleGrid
   * used, just at rail-row scale. Empty string (no inline style) when
   * there's nothing to crop yet -- caller falls back to an initial-letter
   * placeholder. */
  function avatarStyle(/** @type {import('$lib/api/faces.js').PersonRow} */ person) {
    const url = person.cover_image_path ? avatarUrls[person.cover_image_path] : null;
    const { cover_bbox_x: bx, cover_bbox_y: by, cover_bbox_w: bw, cover_bbox_h: bh } = person;
    if (!url || !bw || !bh) return "";
    const dispW = PERSON_AVATAR_SIZE / bw;
    const dispH = PERSON_AVATAR_SIZE / bh;
    const posX = -(bx ?? 0) * dispW;
    const posY = -(by ?? 0) * dispH;
    return `background-image:url("${url}"); background-size:${dispW}px ${dispH}px; background-position:${posX}px ${posY}px;`;
  }
</script>

<div class="rail">
  <div class="section-label">Catalog</div>
  <button
    type="button"
    class="tree-item"
    class:active={activeCollectionId === null && activeFolderKey === null && !showLastImportOnly && activePersonId === null && activeMapImageIds === null}
    onclick={onSelectAllPhotos}
  >
    All Photos
    <span class="count">{images.length}</span>
  </button>
  <button type="button" class="tree-item" class:active={showLastImportOnly} onclick={onSelectLastImport}>
    Last Import
    <span class="count">{lastImportBatchId === null ? 0 : images.filter((img) => img.import_batch === lastImportBatchId).length}</span>
  </button>

  {#if activeMapImageIds !== null}
    <button type="button" class="tree-item active" title="Map selection - click to clear" onclick={onSelectAllPhotos}>
      Map selection
      <span class="count">{new Set(images.filter((img) => activeMapImageIds.has(img.image_id)).map((img) => img.image_id)).size}</span>
    </button>
  {/if}

  {#if folderEntries.length > 0}
    <button type="button" class="section-header folders-label" onclick={() => toggleSection("folders")}>
      <span class="chevron" class:collapsed={!expanded.folders}>▾</span>
      <span class="section-label">Folders</span>
    </button>
    {#if expanded.folders}
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
  {/if}

  <div class="collections-header">
    <button type="button" class="section-header" onclick={() => toggleSection("collections")}>
      <span class="chevron" class:collapsed={!expanded.collections}>▾</span>
      <span class="section-label">Collections</span>
    </button>
    <span class="collections-actions">
      <button type="button" class="rail-action" title="New Collection" onclick={onCreateCollection}>+</button>
      <button type="button" class="rail-action" title="New Smart Collection" onclick={onCreateSmartCollection}>⚡+</button>
    </span>
  </div>
  {#if expanded.collections}
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
  {/if}

  {#if people.length > 0}
    <button type="button" class="section-header people-label" onclick={() => toggleSection("people")}>
      <span class="chevron" class:collapsed={!expanded.people}>▾</span>
      <span class="section-label">People</span>
    </button>
    {#if expanded.people}
      {#each people as person (person.id)}
        {@const style = avatarStyle(person)}
        <div
          class="tree-item person-item"
          class:active={activePersonId === person.id}
          class:unnamed={!person.name}
          ondblclick={(e) => handlePersonRowDblClick(e, person.id)}
          role="button"
          tabindex="0"
          title="Double-click to show this person's photos in Library"
        >
          <span class="person-avatar" style={style}>
            {#if !style}
              <span class="avatar-fallback" aria-hidden="true">{(person.name ?? "?").charAt(0).toUpperCase()}</span>
            {/if}
          </span>
          {#if editingPersonId === person.id}
            <input
              class="person-name-input"
              type="text"
              bind:value={editingPersonName}
              use:focusAndSelect
              onblur={commitEditingPerson}
              onkeydown={(/** @type {KeyboardEvent} */ e) => {
                if (e.key === "Enter") {
                  e.preventDefault();
                  commitEditingPerson();
                } else if (e.key === "Escape") {
                  e.preventDefault();
                  cancelEditingPerson();
                }
              }}
            />
          {:else}
            <button type="button" class="person-name" onclick={() => startEditingPerson(person)}>
              {person.name ?? `Person ${person.id}`}
            </button>
          {/if}
          <span class="count">{person.photo_count}</span>
        </div>
      {/each}
    {/if}
  {/if}
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
  .section-header {
    all: unset;
    box-sizing: border-box;
    display: flex;
    align-items: center;
    gap: 3px;
    cursor: pointer;
    border-radius: var(--radius-s);
  }
  .section-header:hover .section-label {
    color: var(--text-secondary);
  }
  .chevron {
    font-size: 9px;
    color: var(--text-tertiary);
    transition: transform 0.1s ease;
    display: inline-block;
    width: 10px;
    text-align: center;
  }
  .chevron.collapsed {
    transform: rotate(-90deg);
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
  .people-label {
    margin-top: 10px;
  }
  .person-item {
    cursor: default;
  }
  .person-avatar {
    flex: none;
    width: 20px;
    height: 20px;
    border-radius: 50%;
    overflow: hidden;
    position: relative;
    background-color: var(--bg-panel-raised);
    background-repeat: no-repeat;
    display: block;
  }
  .person-item.unnamed .person-avatar {
    outline: 1px dashed var(--border-strong);
    outline-offset: -1px;
  }
  .avatar-fallback {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 9px;
    font-weight: 600;
    color: var(--text-tertiary);
  }
  .person-name,
  .person-name-input {
    all: unset;
    flex: 1;
    min-width: 0;
    font-size: 12px;
    color: var(--text-secondary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    cursor: text;
    border-radius: var(--radius-s);
    box-sizing: border-box;
  }
  .person-item.unnamed .person-name {
    color: var(--text-tertiary);
    font-style: italic;
  }
  .person-name:hover {
    color: var(--text-primary);
  }
  .person-name-input {
    outline: 1px solid var(--accent);
    background: var(--bg-panel-raised);
    padding: 0 2px;
  }
  .person-item.active {
    background: var(--accent-soft);
  }
  .person-item.active .person-name {
    color: var(--accent-strong);
  }
</style>
