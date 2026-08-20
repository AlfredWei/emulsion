<script>
  import { open } from "@tauri-apps/plugin-dialog";
  import { getBackupSettings, updateBackupSettings, performCatalogBackup } from "$lib/api/backup.js";
  import {
    getStoredShortcuts,
    saveStoredShortcuts,
    resetStoredShortcuts,
    SHORTCUT_DEFINITIONS,
    formatKeyDisplay,
  } from "$lib/shortcuts.js";

  /**
   * General app-wide Settings/Preferences dialog with Backup and Shortcuts tabs.
   * @type {{ open: boolean, onClose: () => void }}
   */
  let { open: isOpen, onClose } = $props();

  let activeTab = $state(/** @type {"backup" | "shortcuts"} */ ("shortcuts"));

  // Backup settings state
  let settings = $state(/** @type {import('$lib/api/backup.js').BackupSettings | null} */ (null));
  let backingUp = $state(false);
  let backupError = $state("");

  // Shortcuts settings state
  let shortcuts = $state(getStoredShortcuts());
  let recordingId = $state(/** @type {string | null} */ (null));
  let shortcutConflictMessage = $state("");

  $effect(() => {
    if (isOpen) {
      backupError = "";
      shortcutConflictMessage = "";
      recordingId = null;
      shortcuts = getStoredShortcuts();
      getBackupSettings().then((s) => (settings = s));
    }
  });

  /** @param {Partial<import('$lib/api/backup.js').BackupSettings>} patch */
  function saveBackupSettings(patch) {
    if (!settings) return;
    settings = { ...settings, ...patch };
    updateBackupSettings(settings).catch(() => {});
  }

  async function pickFolder() {
    const dir = await open({ directory: true, multiple: false });
    if (dir) saveBackupSettings({ folder: /** @type {string} */ (dir) });
  }

  async function handleBackUpNow() {
    if (!settings?.folder) {
      backupError = "Choose a backup folder first.";
      return;
    }
    backingUp = true;
    backupError = "";
    try {
      const outcome = await performCatalogBackup(settings.folder, settings.check_integrity, settings.optimize);
      saveBackupSettings({ last_backup_at: outcome.performed_at });
    } catch (/** @type {any} */ e) {
      backupError = `Backup failed: ${e}`;
    } finally {
      backingUp = false;
    }
  }

  function handleStartRecording(/** @type {string} */ id) {
    recordingId = id;
    shortcutConflictMessage = "";
  }

  function handleRecordKey(/** @type {KeyboardEvent} */ e) {
    if (!recordingId) return;
    e.preventDefault();
    e.stopPropagation();

    if (e.key === "Escape") {
      recordingId = null;
      return;
    }

    // Ignore standalone modifier keys
    if (["Shift", "Control", "Alt", "Meta"].includes(e.key)) {
      return;
    }

    let key = e.key;
    if (key.length === 1) {
      key = key.toLowerCase();
    }

    // Check for conflict
    const conflictEntry = Object.entries(shortcuts).find(
      ([id, k]) => id !== recordingId && k.toLowerCase() === key.toLowerCase(),
    );

    const next = { ...shortcuts, [recordingId]: key };
    shortcuts = next;
    saveStoredShortcuts(next);

    if (conflictEntry) {
      const def = SHORTCUT_DEFINITIONS.find((d) => d.id === conflictEntry[0]);
      shortcutConflictMessage = `Note: "${key.toUpperCase()}" was previously assigned to "${def?.label ?? conflictEntry[0]}"`;
    } else {
      shortcutConflictMessage = "";
    }

    recordingId = null;
  }

  function handleResetAllShortcuts() {
    shortcuts = resetStoredShortcuts();
    shortcutConflictMessage = "Shortcuts reset to defaults.";
    recordingId = null;
  }

  let groupedShortcuts = $derived.by(() => {
    /** @type {Record<string, typeof SHORTCUT_DEFINITIONS>} */
    const groups = {};
    for (const def of SHORTCUT_DEFINITIONS) {
      if (!groups[def.category]) groups[def.category] = [];
      groups[def.category].push(def);
    }
    return groups;
  });
</script>

<svelte:window
  onkeydown={(e) => {
    if (!isOpen) return;
    if (recordingId) {
      handleRecordKey(e);
      return;
    }
    if (e.key === "Escape") onClose();
  }}
/>

{#if isOpen}
  <div class="overlay">
    <div class="dialog" role="dialog" aria-modal="true" aria-label="Settings">
      <div class="dialog-header">
        <h2>Settings</h2>
        <div class="tabs-nav" role="tablist">
          <button
            type="button"
            class="tab-btn"
            class:active={activeTab === "shortcuts"}
            onclick={() => (activeTab = "shortcuts")}
          >
            Keyboard Shortcuts
          </button>
          <button
            type="button"
            class="tab-btn"
            class:active={activeTab === "backup"}
            onclick={() => (activeTab = "backup")}
          >
            Backup
          </button>
        </div>
      </div>

      {#if activeTab === "shortcuts"}
        <div class="shortcuts-panel">
          {#if shortcutConflictMessage}
            <div class="conflict-banner">{shortcutConflictMessage}</div>
          {/if}

          <div class="shortcuts-scroll">
            {#each Object.entries(groupedShortcuts) as [category, items] (category)}
              <div class="shortcut-group">
                <div class="group-title">{category}</div>
                <div class="group-items">
                  {#each items as item (item.id)}
                    {@const currentKey = shortcuts[item.id] ?? item.defaultKey}
                    {@const isRec = recordingId === item.id}
                    <div class="shortcut-row">
                      <span class="shortcut-label">{item.label}</span>
                      <button
                        type="button"
                        class="key-badge"
                        class:recording={isRec}
                        title="Click to change shortcut"
                        onclick={() => handleStartRecording(item.id)}
                      >
                        {isRec ? "Press key…" : formatKeyDisplay(currentKey)}
                      </button>
                    </div>
                  {/each}
                </div>
              </div>
            {/each}
          </div>

          <div class="shortcuts-footer">
            <button
              type="button"
              class="reset-shortcuts-btn"
              onclick={handleResetAllShortcuts}
            >
              Reset to Defaults
            </button>
          </div>
        </div>
      {:else if activeTab === "backup" && settings}
        <section class="backup-section">
          <div class="row">
            <label class="label" for="settings-backup-frequency">Frequency</label>
            <select
              id="settings-backup-frequency"
              value={settings.frequency}
              onchange={(e) => saveBackupSettings({ frequency: e.currentTarget.value })}
            >
              <option value="every_time">Every time</option>
              <option value="daily">Once a day</option>
              <option value="weekly">Once a week</option>
              <option value="monthly">Once a month</option>
              <option value="never">Never</option>
            </select>
          </div>

          <div class="row">
            <span class="label">Backup folder</span>
            <button class="folder-btn" type="button" onclick={pickFolder}>
              {settings.folder ?? "Choose folder…"}
            </button>
          </div>

          <label class="checkbox-row">
            <input
              type="checkbox"
              checked={settings.check_integrity}
              onchange={(e) => saveBackupSettings({ check_integrity: e.currentTarget.checked })}
            />
            <span>Test integrity before backing up</span>
          </label>
          <label class="checkbox-row">
            <input
              type="checkbox"
              checked={settings.optimize}
              onchange={(e) => saveBackupSettings({ optimize: e.currentTarget.checked })}
            />
            <span>Optimize catalog (vacuum/compact)</span>
          </label>

          <div class="backup-now-row">
            <button class="secondary" type="button" onclick={handleBackUpNow} disabled={backingUp || !settings.folder}>
              {backingUp ? "Backing up…" : "Back Up Now"}
            </button>
            {#if settings.last_backup_at}
              <span class="last-backup">Last backup: {settings.last_backup_at}</span>
            {/if}
          </div>

          {#if backupError}
            <div class="status">{backupError}</div>
          {/if}
        </section>
      {/if}

      <div class="actions">
        <button class="primary" type="button" onclick={onClose}>Close</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.65);
    backdrop-filter: blur(4px);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 100;
  }
  .dialog {
    width: 480px;
    max-height: 80vh;
    background: var(--bg-panel);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-m);
    box-shadow: var(--shadow-soft);
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .dialog-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding-bottom: 8px;
    border-bottom: 1px solid var(--border-subtle);
  }
  h2 {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
    color: var(--text-primary);
  }
  .tabs-nav {
    display: flex;
    background: var(--bg-panel-raised);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-s);
    padding: 2px;
    gap: 2px;
  }
  .tab-btn {
    all: unset;
    cursor: pointer;
    font-family: var(--font-mono);
    font-size: 11px;
    padding: 3px 10px;
    border-radius: calc(var(--radius-s) - 1px);
    color: var(--text-secondary);
    transition: all 0.1s ease;
  }
  .tab-btn:hover {
    color: var(--text-primary);
  }
  .tab-btn.active {
    background: var(--accent);
    color: #fff;
    font-weight: 600;
  }
  .shortcuts-panel {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-height: 0;
    flex: 1;
  }
  .conflict-banner {
    background: rgba(234, 179, 8, 0.15);
    border: 1px solid rgba(234, 179, 8, 0.4);
    color: var(--label-yellow);
    font-size: 11px;
    font-family: var(--font-mono);
    padding: 6px 10px;
    border-radius: var(--radius-s);
  }
  .shortcuts-scroll {
    max-height: 380px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding-right: 4px;
  }
  .shortcut-group {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .group-title {
    font-family: var(--font-mono);
    font-size: 10px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--text-tertiary);
    padding: 4px 2px;
  }
  .group-items {
    background: var(--bg-panel-raised);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-s);
    overflow: hidden;
  }
  .shortcut-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 6px 10px;
    border-bottom: 1px solid var(--border-subtle);
    font-size: 11.5px;
  }
  .shortcut-row:last-child {
    border-bottom: none;
  }
  .shortcut-label {
    color: var(--text-secondary);
  }
  .key-badge {
    all: unset;
    cursor: pointer;
    min-width: 32px;
    padding: 2px 8px;
    text-align: center;
    background: var(--bg-panel);
    border: 1px solid var(--border-strong);
    border-radius: 4px;
    font-family: var(--font-mono);
    font-size: 11px;
    font-weight: 600;
    color: var(--text-primary);
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.4);
    transition: all 0.1s ease;
  }
  .key-badge:hover {
    border-color: var(--accent);
    color: var(--accent-strong);
    transform: scale(1.05);
  }
  .key-badge.recording {
    background: var(--accent);
    border-color: #fff;
    color: #fff;
    animation: pulse 1s infinite alternate;
  }
  @keyframes pulse {
    from {
      opacity: 0.8;
    }
    to {
      opacity: 1;
    }
  }
  .shortcuts-footer {
    display: flex;
    justify-content: flex-start;
    padding-top: 4px;
  }
  .reset-shortcuts-btn {
    all: unset;
    cursor: pointer;
    font-family: var(--font-mono);
    font-size: 10.5px;
    color: var(--text-tertiary);
    padding: 2px 4px;
  }
  .reset-shortcuts-btn:hover {
    color: var(--label-red);
    text-decoration: underline;
  }
  .backup-section {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding-top: 4px;
  }
  .row {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .label {
    font-size: 11px;
    color: var(--text-secondary);
  }
  select,
  .folder-btn {
    all: unset;
    box-sizing: border-box;
    width: 100%;
    padding: 6px 8px;
    font-size: 12px;
    font-family: inherit;
    color: var(--text-primary);
    background: var(--bg-panel-raised);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-s);
  }
  .folder-btn {
    cursor: pointer;
    color: var(--text-secondary);
  }
  .checkbox-row {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 11.5px;
    color: var(--text-primary);
    cursor: pointer;
  }
  .backup-now-row {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-top: 2px;
  }
  .last-backup {
    font-size: 10.5px;
    font-family: var(--font-mono);
    color: var(--text-tertiary);
  }
  .status {
    font-size: 11px;
    font-family: var(--font-mono);
    color: var(--text-secondary);
    word-break: break-word;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 4px;
    padding-top: 8px;
    border-top: 1px solid var(--border-subtle);
  }
  .actions button,
  .backup-now-row button {
    all: unset;
    cursor: pointer;
    padding: 6px 14px;
    font-size: 11.5px;
    font-weight: 600;
    border-radius: 6px;
  }
  .actions button:disabled,
  .backup-now-row button:disabled {
    opacity: 0.6;
    cursor: default;
  }
  .primary {
    background: var(--accent-soft);
    color: var(--accent-strong);
    border: 1px solid var(--accent);
  }
  .secondary {
    color: var(--text-secondary);
    border: 1px solid var(--border-strong);
  }
</style>
