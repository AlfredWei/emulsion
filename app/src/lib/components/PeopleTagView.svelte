<script>
  /**
   * Tag Faces view (M5 Slice 6, RFC-0005 §3.6) -- one photo's detected
   * faces overlaid on its real decoded preview, with a click-to-tag
   * popover (search existing people or add a new one) and a basic
   * correction menu (rename / reassign / not-a-face), matching the
   * reviewed mockup (docs/ux/mockups/people-face-tagging-mockup.html).
   * Selection-dumb like every other view component here: IPC lives in
   * +page.svelte, this only renders `faces`/`people` and reports actions
   * back through callback props.
   *
   * `previewUrl` must be the DECODED preview (getDevelopPreview), not the
   * thumbnail -- `bbox_x/y/w/h` are fractions of the full, undistorted
   * decode, and only that preserves the source's own aspect ratio the way
   * a cropped thumbnail might not; positioning face boxes by plain CSS
   * percentages over it is what keeps them aligned regardless of display
   * size.
   * @type {{
   *   image: import('$lib/api/catalog.js').ImageSummary | null,
   *   previewUrl: string | null,
   *   faces: import('$lib/api/faces.js').FaceRow[],
   *   people: import('$lib/api/faces.js').PersonRow[],
   *   onTagFace: (faceId: number, personId: number) => void,
   *   onCreateAndTagFace: (faceId: number, name: string) => void,
   *   onRenamePerson: (personId: number, name: string | null) => void,
   *   onSetFaceExcluded: (faceId: number, excluded: boolean) => void,
   * }}
   */
  let { image, previewUrl, faces, people, onTagFace, onCreateAndTagFace, onRenamePerson, onSetFaceExcluded } = $props();

  let openPopoverFaceId = $state(/** @type {number | null} */ (null));
  let openMenuFaceId = $state(/** @type {number | null} */ (null));
  let popoverQuery = $state("");
  let renamingFaceId = $state(/** @type {number | null} */ (null));
  let renameValue = $state("");

  let suggestions = $derived.by(() => {
    const q = popoverQuery.trim().toLowerCase();
    const named = people.filter((p) => p.name);
    return q ? named.filter((p) => /** @type {string} */ (p.name).toLowerCase().includes(q)) : named;
  });

  function closeFloaters() {
    openPopoverFaceId = null;
    openMenuFaceId = null;
    renamingFaceId = null;
  }

  function togglePopover(/** @type {number} */ faceId) {
    openMenuFaceId = null;
    popoverQuery = "";
    openPopoverFaceId = openPopoverFaceId === faceId ? null : faceId;
  }

  function toggleMenu(/** @type {number} */ faceId) {
    openPopoverFaceId = null;
    openMenuFaceId = openMenuFaceId === faceId ? null : faceId;
  }

  function pickSuggestion(/** @type {number} */ faceId, /** @type {import('$lib/api/faces.js').PersonRow} */ person) {
    onTagFace(faceId, person.id);
    closeFloaters();
  }

  function submitNewPerson(/** @type {number} */ faceId) {
    const name = popoverQuery.trim();
    if (!name) return;
    onCreateAndTagFace(faceId, name);
    closeFloaters();
  }

  function startRenaming(/** @type {import('$lib/api/faces.js').FaceRow} */ face) {
    if (face.person_id === null) return;
    openMenuFaceId = null;
    renamingFaceId = face.id;
    renameValue = face.person_name ?? "";
  }

  function commitRename(/** @type {import('$lib/api/faces.js').FaceRow} */ face) {
    if (renamingFaceId !== face.id || face.person_id === null) return;
    renamingFaceId = null;
    onRenamePerson(face.person_id, renameValue.trim() || null);
  }

  /** @param {HTMLInputElement} node */
  function focusAndSelect(node) {
    node.focus();
    node.select();
  }
</script>

<svelte:window
  onclick={closeFloaters}
  onkeydown={(e) => (openPopoverFaceId !== null || openMenuFaceId !== null) && e.key === "Escape" && closeFloaters()}
/>

<div class="body-grid">
  <div class="main">
    <div class="tagcanvas-wrap">
      {#if !image}
        <div class="empty-hint">Pick a photo from the filmstrip below to tag its faces.</div>
      {:else if !previewUrl}
        <div class="empty-hint">Loading…</div>
      {:else}
        <div class="tagphoto">
          <img class="tagphoto-img" src={previewUrl} alt="" />
          {#each faces as face (face.id)}
            {@const untagged = face.person_id === null}
            {@const label = untagged ? null : (face.person_name ?? `Person ${face.person_id}`)}
            <div
              class="face-box"
              class:untagged
              class:open={openPopoverFaceId === face.id || openMenuFaceId === face.id}
              style={`left:${face.bbox_x * 100}%;top:${face.bbox_y * 100}%;width:${face.bbox_w * 100}%;height:${face.bbox_h * 100}%`}
            >
              <button
                type="button"
                class="more-btn"
                onclick={(e) => {
                  e.stopPropagation();
                  toggleMenu(face.id);
                }}
              >⋯</button>
              {#if renamingFaceId === face.id}
                <input
                  class="tag-pill-input"
                  type="text"
                  bind:value={renameValue}
                  use:focusAndSelect
                  onclick={(/** @type {MouseEvent} */ e) => e.stopPropagation()}
                  onblur={() => commitRename(face)}
                  onkeydown={(/** @type {KeyboardEvent} */ e) => {
                    if (e.key === "Enter") {
                      e.preventDefault();
                      commitRename(face);
                    } else if (e.key === "Escape") {
                      e.preventDefault();
                      renamingFaceId = null;
                    }
                  }}
                />
              {:else}
                <button
                  type="button"
                  class="tag-pill"
                  class:untagged
                  onclick={(e) => {
                    e.stopPropagation();
                    if (untagged) togglePopover(face.id);
                  }}
                >
                  {#if untagged}
                    + Who is this?
                  {:else}
                    <span class="dot"></span>{label}
                  {/if}
                </button>
              {/if}

              {#if openPopoverFaceId === face.id}
                <div class="tag-popover" role="presentation" onclick={(/** @type {MouseEvent} */ e) => e.stopPropagation()}>
                  <input
                    type="text"
                    placeholder="Search people or add new…"
                    bind:value={popoverQuery}
                    use:focusAndSelect
                    onkeydown={(/** @type {KeyboardEvent} */ e) => e.key === "Enter" && submitNewPerson(face.id)}
                  />
                  <div class="results">
                    {#each suggestions as person (person.id)}
                      <button type="button" class="suggestion" onclick={() => pickSuggestion(face.id, person)}>
                        <span class="mini-avatar" aria-hidden="true"></span>
                        <span class="suggestion-name">{person.name}</span>
                        <span class="cnt">{person.photo_count}</span>
                      </button>
                    {/each}
                    <button type="button" class="suggestion new-person" onclick={() => submitNewPerson(face.id)}>
                      + New person{popoverQuery.trim() ? ` "${popoverQuery.trim()}"` : "…"}
                    </button>
                  </div>
                </div>
              {/if}

              {#if openMenuFaceId === face.id}
                <div class="face-menu" role="presentation" onclick={(/** @type {MouseEvent} */ e) => e.stopPropagation()}>
                  {#if untagged}
                    <button
                      type="button"
                      onclick={() => {
                        openMenuFaceId = null;
                        togglePopover(face.id);
                      }}
                    >Tag this face…</button>
                  {:else}
                    <button type="button" onclick={() => startRenaming(face)}>Rename…</button>
                    <button
                      type="button"
                      onclick={() => {
                        openMenuFaceId = null;
                        togglePopover(face.id);
                      }}
                    >Reassign to…</button>
                  {/if}
                  <button
                    type="button"
                    class="danger"
                    onclick={() => {
                      closeFloaters();
                      onSetFaceExcluded(face.id, true);
                    }}
                  >Not a face</button>
                </div>
              {/if}
            </div>
          {/each}
        </div>
      {/if}
    </div>
  </div>

  <div class="panel">
    <div class="section-label">Faces in this photo</div>
    {#each faces as face (face.id)}
      {@const unnamed = face.person_id === null || !face.person_name}
      <div class="face-list-item">
        <span class="mini-avatar" aria-hidden="true"></span>
        <div class="info">
          <div class="nm" class:unnamed>
            {face.person_id === null ? "Who is this?" : (face.person_name ?? `Person ${face.person_id}`)}
          </div>
          <div class="sub">
            {face.person_id === null ? "Detected, not yet tagged" : face.person_name ? "Tagged" : "Auto-clustered, unnamed"}
          </div>
        </div>
      </div>
    {:else}
      <div class="panel-empty">
        {image ? "No faces detected in this photo." : "Pick a photo to see its faces."}
      </div>
    {/each}
    <div class="panel-note">
      Faces marked "Not a face" are kept (not deleted) but hidden here and excluded from clustering.
    </div>
  </div>
</div>

<style>
  .body-grid {
    flex: 1;
    display: grid;
    grid-template-columns: 1fr 240px;
    min-height: 0;
  }
  .panel {
    background: var(--bg-panel);
    overflow-y: auto;
    border-left: 1px solid var(--border-subtle);
    padding: 14px 12px;
  }
  .main {
    background: var(--bg-canvas);
    position: relative;
    overflow: auto;
    display: flex;
    flex-direction: column;
  }
  .section-label {
    font-family: var(--font-mono);
    font-size: 10px;
    letter-spacing: 0.09em;
    text-transform: uppercase;
    color: var(--text-tertiary);
    padding: 10px 4px 6px;
    font-weight: 600;
  }
  .tagcanvas-wrap {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 24px;
    position: relative;
  }
  .empty-hint {
    color: var(--text-tertiary);
    font-size: 13px;
  }
  .tagphoto {
    position: relative;
    max-width: 100%;
    max-height: 100%;
    border-radius: 2px;
    box-shadow: 0 20px 50px -14px rgba(0, 0, 0, 0.7);
    line-height: 0;
  }
  .tagphoto-img {
    display: block;
    max-width: 100%;
    max-height: calc(100vh - 260px);
    border-radius: 2px;
  }
  .face-box {
    all: unset;
    position: absolute;
    border: 2px solid var(--accent);
    border-radius: 6px;
    cursor: default;
    z-index: 2;
    box-sizing: border-box;
  }
  .face-box.untagged {
    border-style: dashed;
    border-color: var(--text-secondary);
  }
  .face-box.untagged:hover,
  .face-box.open {
    border-color: var(--accent-strong);
  }
  .more-btn {
    all: unset;
    position: absolute;
    top: -11px;
    right: -11px;
    width: 18px;
    height: 18px;
    border-radius: 50%;
    background: var(--bg-panel-raised);
    border: 1px solid var(--border-strong);
    color: var(--text-secondary);
    font-size: 11px;
    display: flex;
    align-items: center;
    justify-content: center;
    opacity: 0;
    transition: opacity 0.12s;
    cursor: pointer;
    box-sizing: border-box;
  }
  .face-box:hover .more-btn,
  .face-box.open .more-btn {
    opacity: 1;
  }
  .tag-pill,
  .tag-pill-input {
    all: unset;
    position: absolute;
    left: 0;
    top: calc(100% + 6px);
    white-space: nowrap;
    background: rgba(20, 18, 16, 0.85);
    color: var(--text-primary);
    font-size: 10.5px;
    font-weight: 600;
    padding: 3px 8px;
    border-radius: 99px;
    display: flex;
    align-items: center;
    gap: 5px;
    cursor: pointer;
    box-sizing: border-box;
  }
  .tag-pill.untagged {
    color: var(--text-secondary);
    font-weight: 500;
    background: rgba(20, 18, 16, 0.65);
    border: 1px dashed var(--border-strong);
    cursor: pointer;
  }
  .tag-pill-input {
    outline: 1px solid var(--accent);
    background: var(--bg-panel-raised);
    cursor: text;
    min-width: 60px;
  }
  .dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--accent-strong);
    flex: none;
  }
  .tag-popover {
    position: absolute;
    left: 0;
    top: calc(100% + 30px);
    z-index: 6;
    width: 208px;
    background: var(--bg-panel-raised);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-m);
    box-shadow: var(--shadow-soft);
    padding: 8px;
  }
  .tag-popover input[type="text"] {
    width: 100%;
    background: var(--bg-panel);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-s);
    color: var(--text-primary);
    font-size: 12px;
    padding: 6px 8px;
    margin-bottom: 6px;
    font-family: var(--font-ui);
    box-sizing: border-box;
  }
  .tag-popover input[type="text"]:focus {
    outline: 1px solid var(--accent);
    border-color: var(--accent);
  }
  .suggestion {
    all: unset;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 5px 6px;
    border-radius: var(--radius-s);
    cursor: pointer;
    font-size: 12px;
    width: 100%;
    box-sizing: border-box;
  }
  .suggestion:hover {
    background: var(--bg-hover);
  }
  .mini-avatar {
    width: 20px;
    height: 20px;
    border-radius: 50%;
    flex: none;
    background: var(--bg-panel-raised);
  }
  .suggestion-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .cnt {
    margin-left: auto;
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--text-tertiary);
  }
  .new-person {
    border-top: 1px solid var(--border-subtle);
    margin-top: 4px;
    padding-top: 7px;
    color: var(--accent-strong);
    font-weight: 700;
  }
  .face-menu {
    position: absolute;
    right: -11px;
    top: 18px;
    z-index: 6;
    background: var(--bg-panel-raised);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-m);
    box-shadow: var(--shadow-soft);
    padding: 4px;
    width: 152px;
  }
  .face-menu button {
    all: unset;
    display: block;
    width: 100%;
    padding: 6px 8px;
    font-size: 12px;
    border-radius: var(--radius-s);
    color: var(--text-secondary);
    cursor: pointer;
    box-sizing: border-box;
  }
  .face-menu button:hover {
    background: var(--bg-hover);
    color: var(--text-primary);
  }
  .face-menu button.danger:hover {
    color: var(--label-red);
  }
  .face-list-item {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 7px 6px;
    border-radius: var(--radius-s);
  }
  .face-list-item .mini-avatar {
    width: 32px;
    height: 32px;
    box-shadow: 0 0 0 1px var(--border-subtle);
  }
  .face-list-item .info {
    flex: 1;
    min-width: 0;
  }
  .face-list-item .info .nm {
    font-size: 12px;
    color: var(--text-primary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .face-list-item .info .nm.unnamed {
    color: var(--text-tertiary);
    font-style: italic;
  }
  .face-list-item .info .sub {
    font-size: 10px;
    color: var(--text-tertiary);
  }
  .panel-empty {
    color: var(--text-tertiary);
    font-size: 11.5px;
    padding: 8px 6px;
  }
  .panel-note {
    font-size: 10.5px;
    color: var(--text-tertiary);
    line-height: 1.5;
    padding: 10px 6px 4px;
    border-top: 1px solid var(--border-subtle);
    margin-top: 8px;
  }
</style>
