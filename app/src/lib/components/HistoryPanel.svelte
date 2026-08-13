<script>
  /**
   * Develop-mode left rail (M3): History (click any row to jump straight
   * to that edit state -- undo/redo are just this same jump, one row up
   * or down) and Snapshots (named save points) for the currently open
   * photo. Mirrors the Library `.rail`'s own 200px/border-right/overflow-y
   * shell (see +page.svelte's own `.rail` CSS) -- Develop had no left
   * rail at all before this.
   * @type {{
   *   history: import('$lib/api/develop.js').HistoryEntry[],
   *   historyIndex: number,
   *   snapshots: import('$lib/api/develop.js').SnapshotEntry[],
   *   onJumpTo: (index: number) => void,
   *   onCreateSnapshotRequest: () => void,
   *   onRestoreSnapshot: (id: number) => void,
   *   onDeleteSnapshot: (id: number) => void,
   * }}
   */
  let { history, historyIndex, snapshots, onJumpTo, onCreateSnapshotRequest, onRestoreSnapshot, onDeleteSnapshot } =
    $props();

  // SQLite's datetime('now') is UTC with no timezone suffix -- appending
  // "Z" is what tells JS's Date parser that, rather than silently
  // misreading it as local time (the same class of mistake this project
  // avoids elsewhere by being explicit about which space a value lives
  // in, e.g. cropMath.js's normalized-vs-pixel distinction).
  function formatTime(/** @type {string} */ sqliteUtc) {
    return new Date(`${sqliteUtc}Z`).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  }
</script>

<div class="history-rail">
  <div class="section-label">History</div>
  <div class="entry-list">
    {#if history.length === 0}
      <div class="empty">No edits yet</div>
    {/if}
    {#each history as entry, index (entry.id)}
      <button
        type="button"
        class="entry"
        class:current={index === historyIndex}
        onclick={() => onJumpTo(index)}
      >
        <span class="entry-label">{entry.label}</span>
        <span class="entry-time">{formatTime(entry.created_at)}</span>
      </button>
    {/each}
  </div>

  <div class="collections-header snapshots-header">
    <div class="section-label">Snapshots</div>
    <button type="button" class="rail-action" onclick={onCreateSnapshotRequest} title="Create Snapshot">+</button>
  </div>
  <div class="entry-list">
    {#if snapshots.length === 0}
      <div class="empty">No snapshots</div>
    {/if}
    {#each snapshots as snapshot (snapshot.id)}
      <div class="snapshot-row">
        <button type="button" class="entry snapshot-entry" onclick={() => onRestoreSnapshot(snapshot.id)}>
          <span class="entry-label">{snapshot.name}</span>
          <span class="entry-time">{formatTime(snapshot.created_at)}</span>
        </button>
        <button
          type="button"
          class="rail-action delete"
          onclick={() => onDeleteSnapshot(snapshot.id)}
          title="Delete Snapshot"
        >
          ×
        </button>
      </div>
    {/each}
  </div>
</div>

<style>
  .history-rail {
    width: 200px;
    flex: none;
    background: var(--bg-panel);
    border-right: 1px solid var(--border-subtle);
    padding: 14px 10px;
    overflow-y: auto;
    overflow-x: hidden;
    display: flex;
    flex-direction: column;
    gap: 4px;
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
  .snapshots-header {
    margin-top: 10px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding-right: 2px;
  }
  .snapshots-header .section-label {
    padding: 4px 0;
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
  .rail-action.delete {
    font-size: 13px;
    line-height: 1;
  }
  .entry-list {
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .empty {
    padding: 5px 7px;
    font-size: 11.5px;
    color: var(--text-tertiary);
  }
  .entry {
    all: unset;
    box-sizing: border-box;
    width: 100%;
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 6px;
    padding: 5px 7px;
    border-radius: var(--radius-s);
    color: var(--text-secondary);
    font-size: 12px;
    cursor: pointer;
  }
  .entry:hover {
    background: var(--bg-panel-raised);
  }
  .entry.current {
    background: var(--accent-soft);
    color: var(--accent-strong);
  }
  .entry-label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .entry-time {
    flex: none;
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--text-tertiary);
  }
  .entry.current .entry-time {
    color: var(--accent-strong);
    opacity: 0.75;
  }
  .snapshot-row {
    display: flex;
    align-items: center;
    gap: 2px;
  }
  .snapshot-row .entry {
    flex: 1;
    min-width: 0;
  }
</style>
