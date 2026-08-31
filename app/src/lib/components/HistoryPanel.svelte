<script>
  /**
   * Develop-mode left rail (M3): History (click any row to jump straight
   * to that edit state -- undo/redo are just this same jump, one row up
   * or down), Snapshots (named save points), and Presets (M4.5:
   * relocated here from the right-hand DevelopPanel accordion, to sit
   * alongside the other "jump to a different look" actions) for the
   * currently open photo. Mirrors the Library `.rail`'s own
   * 200px/border-right/overflow-y shell (see +page.svelte's own `.rail`
   * CSS) -- Develop had no left rail at all before M3.
   *
   * Layout (M4.5): a small preview thumbnail is pinned at the very top,
   * above a History/Presets tab strip (Snapshots lives inside the
   * History tab, alongside its own "jump to a different state"
   * sibling) -- only the tab's own content scrolls underneath, same
   * fixed-header/scrolling-body split `DevelopPanel.svelte`'s own footer
   * already established.
   *
   * Hovering any History/Snapshot/Preset row fires the matching
   * `onPeek*` callback so the parent can render that entry's resulting
   * look into `previewUrl`, shown in the thumbnail box above --
   * `onPeekEnd` on mouseleave clears it. This component has no opinion
   * on how the preview image itself is produced (it doesn't hold or
   * fetch any EditStack or drive any canvas); it only reports hover
   * enter/leave and displays whatever URL it's given.
   * @type {{
   *   history: import('$lib/api/develop.js').HistoryEntry[],
   *   historyIndex: number,
   *   snapshots: import('$lib/api/develop.js').SnapshotEntry[],
   *   onJumpTo: (index: number) => void,
   *   onCreateSnapshotRequest: () => void,
   *   onRestoreSnapshot: (id: number) => void,
   *   onDeleteSnapshot: (id: number) => void,
   *   presets: import('$lib/api/develop.js').PresetEntry[],
   *   onApplyPreset: (presetId: number) => void,
   *   onSaveCurrentAsPresetRequest: () => void,
   *   onExportPreset: (presetId: number) => void,
   *   onDeletePresetRequest: (presetId: number) => void,
   *   onImportPresetRequest: () => void,
   *   onPeekHistory: (index: number) => void,
   *   onPeekSnapshot: (id: number) => void,
   *   onPeekPreset: (id: number) => void,
   *   onPeekEnd: () => void,
   *   previewUrl: string | null,
   *   width?: number,
   * }}
   */
  let {
    history,
    historyIndex,
    snapshots,
    onJumpTo,
    onCreateSnapshotRequest,
    onRestoreSnapshot,
    onDeleteSnapshot,
    presets,
    onApplyPreset,
    onSaveCurrentAsPresetRequest,
    onExportPreset,
    onDeletePresetRequest,
    onImportPresetRequest,
    onPeekHistory,
    onPeekSnapshot,
    onPeekPreset,
    onPeekEnd,
    previewUrl,
    width = 200,
  } = $props();

  let activeTab = $state(/** @type {"history" | "presets"} */ ("history"));

  // SQLite's datetime('now') is UTC with no timezone suffix -- appending
  // "Z" is what tells JS's Date parser that, rather than silently
  // misreading it as local time (the same class of mistake this project
  // avoids elsewhere by being explicit about which space a value lives
  // in, e.g. cropMath.js's normalized-vs-pixel distinction).
  function formatTime(/** @type {string} */ sqliteUtc) {
    return new Date(`${sqliteUtc}Z`).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  }
</script>

<div class="history-rail" style="width: {width}px">
  <div class="preview-box">
    {#if previewUrl}
      <img src={previewUrl} alt="" />
    {:else}
      <div class="preview-placeholder">Hover a row to preview</div>
    {/if}
  </div>

  <div class="tab-bar">
    <button type="button" class="tab-btn" class:active={activeTab === "history"} onclick={() => (activeTab = "history")}>
      History
    </button>
    <button type="button" class="tab-btn" class:active={activeTab === "presets"} onclick={() => (activeTab = "presets")}>
      Presets
    </button>
  </div>

  <div class="rail-scroll">
    {#if activeTab === "history"}
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
            onmouseenter={() => onPeekHistory(index)}
            onmouseleave={onPeekEnd}
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
            <button
              type="button"
              class="entry snapshot-entry"
              onclick={() => onRestoreSnapshot(snapshot.id)}
              onmouseenter={() => onPeekSnapshot(snapshot.id)}
              onmouseleave={onPeekEnd}
            >
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
    {:else}
      <div class="preset-actions">
        <button type="button" class="preset-action-btn" onclick={onSaveCurrentAsPresetRequest}>
          Save Current as Preset…
        </button>
        <button type="button" class="preset-action-btn" onclick={onImportPresetRequest}>Import…</button>
      </div>
      {#if presets.length === 0}
        <div class="empty">No presets yet</div>
      {:else}
        <ul class="preset-list">
          {#each presets as preset (preset.id)}
            <li class="preset-row">
              <button
                type="button"
                class="preset-name-btn"
                onclick={() => onApplyPreset(preset.id)}
                onmouseenter={() => onPeekPreset(preset.id)}
                onmouseleave={onPeekEnd}
                title="Apply {preset.name}"
              >
                {preset.name}
              </button>
              <div class="preset-row-actions">
                <button type="button" class="preset-icon-btn" onclick={() => onExportPreset(preset.id)} title="Export">
                  ⇩
                </button>
                <button
                  type="button"
                  class="preset-icon-btn delete"
                  onclick={() => onDeletePresetRequest(preset.id)}
                  title="Delete"
                >
                  ×
                </button>
              </div>
            </li>
          {/each}
        </ul>
        <!-- Known limitation (see develop.js's applyPresetOps doc comment):
             each preset op wholly REPLACES the matching op on the target
             image, so an HSL/Tone Curve/Split Toning-bearing preset zeroes
             out any of the target's own untouched adjustments in that same
             category, not just the ones the preset itself set. -->
        <p class="preset-note">
          Applying a preset replaces matching adjustment categories (e.g. all HSL bands) entirely -- it does not merge
          partial changes.
        </p>
      {/if}
    {/if}
  </div>
</div>

<style>
  .history-rail {
    /* width set inline from the `width` prop (M4.5: drag-resizable) */
    flex: none;
    background: var(--bg-panel);
    border-right: 1px solid var(--border-subtle);
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  .preview-box {
    flex: none;
    margin: 10px 10px 0;
    aspect-ratio: 4 / 3;
    background: #000;
    border-radius: var(--radius-s);
    overflow: hidden;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .preview-box img {
    width: 100%;
    height: 100%;
    object-fit: contain;
  }
  .preview-placeholder {
    color: var(--text-tertiary);
    font-size: 10.5px;
    text-align: center;
    padding: 8px;
  }
  .tab-bar {
    flex: none;
    display: flex;
    margin: 10px 10px 0;
    border-bottom: 1px solid var(--border-subtle);
  }
  .tab-btn {
    all: unset;
    box-sizing: border-box;
    flex: 1;
    text-align: center;
    cursor: pointer;
    padding: 7px 4px;
    font-family: var(--font-mono);
    font-size: 10px;
    letter-spacing: 0.09em;
    text-transform: uppercase;
    font-weight: 600;
    color: var(--text-tertiary);
    border-bottom: 2px solid transparent;
  }
  .tab-btn:hover {
    color: var(--text-secondary);
  }
  .tab-btn.active {
    color: var(--accent-strong);
    border-bottom-color: var(--accent-strong);
  }
  .rail-scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    overflow-x: hidden;
    padding: 10px 10px 14px;
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
  .preset-actions {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 2px 4px 8px;
  }
  .preset-action-btn {
    all: unset;
    box-sizing: border-box;
    width: 100%;
    cursor: pointer;
    padding: 6px 8px;
    font-size: 11px;
    font-weight: 600;
    border-radius: var(--radius-s);
    color: var(--accent-strong);
    background: var(--accent-soft);
    text-align: center;
  }
  .preset-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .preset-row {
    display: flex;
    align-items: center;
    gap: 2px;
  }
  .preset-name-btn {
    all: unset;
    box-sizing: border-box;
    flex: 1;
    min-width: 0;
    cursor: pointer;
    padding: 5px 4px;
    font-size: 12px;
    color: var(--text-secondary);
    border-radius: var(--radius-s);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .preset-name-btn:hover {
    background: var(--bg-panel-raised);
    color: var(--text-primary);
  }
  .preset-row-actions {
    display: flex;
    gap: 0;
    flex: none;
  }
  .preset-icon-btn {
    all: unset;
    cursor: pointer;
    padding: 3px 6px;
    font-size: 12px;
    line-height: 1;
    border-radius: var(--radius-s);
    color: var(--text-tertiary);
  }
  .preset-icon-btn:hover {
    color: var(--accent-strong);
    background: var(--accent-soft);
  }
  .preset-icon-btn.delete:hover {
    color: var(--label-red);
  }
  .preset-note {
    margin: 8px 4px 0;
    font-size: 10.5px;
    line-height: 1.4;
    color: var(--text-tertiary);
    font-style: italic;
  }
</style>
