<script>
  /**
   * People grid (M5 Slice 6, RFC-0005 §3.6) -- one card per person, ordered
   * however `list_people` already ordered them (busiest first). Same
   * selection-dumb prop contract as LibraryGrid/Filmstrip: all IPC
   * (listPeople/renamePerson/preview decoding) lives in +page.svelte, this
   * only renders `people` and reports a rename back through `onRename`.
   *
   * `avatarUrls` maps a person's `cover_image_path` (NOT person id --
   * several people can share a cover photo) to an already-decoded preview
   * URL; the crop itself (source photo -> just this face's bbox) is a
   * pure CSS `background-position`/`background-size` trick computed here,
   * not a server-side crop -- there's no per-face thumbnail cache, and
   * this needs no new backend endpoint to look right.
   * @type {{
   *   people: import('$lib/api/faces.js').PersonRow[],
   *   avatarUrls: Record<string, string | null>,
   *   onRename: (personId: number, name: string | null) => void,
   * }}
   */
  let { people, avatarUrls, onRename } = $props();

  const AVATAR_SIZE = 86;

  let editingId = $state(/** @type {number | null} */ (null));
  let editingValue = $state("");

  function startEditing(/** @type {import('$lib/api/faces.js').PersonRow} */ person) {
    editingId = person.id;
    editingValue = person.name ?? "";
  }

  function commitEditing() {
    if (editingId === null) return;
    const id = editingId;
    const trimmed = editingValue.trim();
    editingId = null;
    onRename(id, trimmed || null);
  }

  function cancelEditing() {
    editingId = null;
  }

  /** @param {HTMLInputElement} node */
  function focusAndSelect(node) {
    node.focus();
    node.select();
  }

  /** Crops `avatarUrls[person.cover_image_path]` down to just
   * `cover_bbox_*` and scales that crop to fill a fixed `AVATAR_SIZE`
   * circle, entirely via `background-size`/`background-position` in
   * pixels -- independent of the decoded preview's own resolution, so no
   * width/height lookup is needed here at all. Empty string (no inline
   * style) when there's nothing to crop yet -- caller falls back to an
   * initial-letter placeholder. */
  function avatarStyle(/** @type {import('$lib/api/faces.js').PersonRow} */ person) {
    const url = person.cover_image_path ? avatarUrls[person.cover_image_path] : null;
    const { cover_bbox_x: bx, cover_bbox_y: by, cover_bbox_w: bw, cover_bbox_h: bh } = person;
    if (!url || !bw || !bh) return "";
    const dispW = AVATAR_SIZE / bw;
    const dispH = AVATAR_SIZE / bh;
    const posX = -(bx ?? 0) * dispW;
    const posY = -(by ?? 0) * dispH;
    return `background-image:url("${url}"); background-size:${dispW}px ${dispH}px; background-position:${posX}px ${posY}px;`;
  }
</script>

<div class="people-grid-scroll">
  <div class="person-grid">
    {#each people as person (person.id)}
      {@const style = avatarStyle(person)}
      <div class="person-card" class:unnamed={!person.name}>
        <div class="person-avatar" {style}>
          {#if !style}
            <div class="avatar-fallback" aria-hidden="true">{(person.name ?? "?").charAt(0).toUpperCase()}</div>
          {/if}
        </div>
        {#if editingId === person.id}
          <input
            class="person-name-input"
            type="text"
            bind:value={editingValue}
            use:focusAndSelect
            onblur={commitEditing}
            onkeydown={(/** @type {KeyboardEvent} */ e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                commitEditing();
              } else if (e.key === "Escape") {
                e.preventDefault();
                cancelEditing();
              }
            }}
          />
        {:else}
          <button type="button" class="person-name" onclick={() => startEditing(person)}>
            {person.name ?? `Person ${person.id}`}
          </button>
        {/if}
        <div class="person-count">{person.photo_count} photo{person.photo_count === 1 ? "" : "s"}</div>
      </div>
    {:else}
      <div class="empty">No people yet -- import some photos, then use "Find People".</div>
    {/each}
  </div>
</div>

<style>
  .people-grid-scroll {
    flex: 1;
    overflow-y: auto;
    padding: 18px;
    min-height: 0;
  }
  .person-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(108px, 1fr));
    gap: 20px 12px;
  }
  .empty {
    grid-column: 1 / -1;
    color: var(--text-tertiary);
    font-size: 12px;
    padding: 24px 4px;
  }
  .person-card {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
  }
  .person-avatar {
    width: 86px;
    height: 86px;
    border-radius: 50%;
    overflow: hidden;
    position: relative;
    border: 2px solid transparent;
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.5);
    background-color: var(--bg-panel-raised);
    background-repeat: no-repeat;
  }
  .person-card.unnamed .person-avatar {
    border-style: dashed;
    border-color: var(--border-strong);
  }
  .avatar-fallback {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 28px;
    font-weight: 600;
    color: var(--text-tertiary);
  }
  .person-name,
  .person-name-input {
    all: unset;
    font-size: 12px;
    font-weight: 600;
    color: var(--text-primary);
    max-width: 100px;
    text-align: center;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    cursor: text;
    border-radius: var(--radius-s);
    padding: 1px 4px;
    box-sizing: border-box;
  }
  .person-name:hover {
    background: var(--bg-hover);
  }
  .person-name-input {
    outline: 1px solid var(--accent);
    background: var(--bg-panel-raised);
    white-space: normal;
  }
  .person-card.unnamed .person-name {
    color: var(--text-tertiary);
    font-weight: 500;
    font-style: italic;
  }
  .person-count {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--text-tertiary);
  }
</style>
