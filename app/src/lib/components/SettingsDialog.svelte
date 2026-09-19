<script>
  import { open } from "@tauri-apps/plugin-dialog";
  import { getBackupSettings, updateBackupSettings, performCatalogBackup } from "$lib/api/backup.js";
  import { getStorageInfo, setCacheDir } from "$lib/api/storage.js";
  import { hasMapsApiKey, setMapsApiKey } from "$lib/api/map.js";
  import {
    listExportPlugins,
    addExportPlugin,
    updateExportPlugin,
    deleteExportPlugin,
  } from "$lib/api/export.js";
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

  let activeTab = $state(/** @type {"backup" | "shortcuts" | "storage" | "exportPlugins" | "map"} */ ("shortcuts"));

  // Backup settings state
  let settings = $state(/** @type {import('$lib/api/backup.js').BackupSettings | null} */ (null));
  let backingUp = $state(false);
  let backupError = $state("");

  // Storage settings state
  let storageInfo = $state(/** @type {import('$lib/api/storage.js').StorageInfo | null} */ (null));
  let movingCache = $state(false);
  let storageError = $state("");

  // Map settings state (M5.5): the key is write-only from here -- the
  // backend never sends it back, so this field can only set or clear it.
  let mapsKeySaved = $state(false);
  let mapsKeyInput = $state("");
  let mapsError = $state("");

  async function handleSaveMapsKey() {
    mapsError = "";
    try {
      await setMapsApiKey(mapsKeyInput);
      mapsKeySaved = mapsKeyInput.trim().length > 0;
      mapsKeyInput = "";
    } catch (/** @type {any} */ e) {
      mapsError = String(e);
    }
  }

  async function handleRemoveMapsKey() {
    mapsError = "";
    try {
      await setMapsApiKey(null);
      mapsKeySaved = false;
      mapsKeyInput = "";
    } catch (/** @type {any} */ e) {
      mapsError = String(e);
    }
  }

  // Shortcuts settings state
  let shortcuts = $state(getStoredShortcuts());
  let recordingId = $state(/** @type {string | null} */ (null));
  let shortcutConflictMessage = $state("");

  // Export plugins settings state (M5, RFC-0006). `pluginForm` is null when
  // the add/edit form is hidden; `pluginForm.id === null` means "adding a
  // new plugin" rather than editing an existing one. `args` is a single
  // space-separated string, not the raw `string[]` -- the simplest input
  // this v0 UI offers, split/joined at the form boundary (see
  // savePluginForm/openEditPluginForm); an argument containing a literal
  // space isn't expressible through this field, a known, accepted v0 gap.
  let plugins = $state(/** @type {import('$lib/api/export.js').ExportPlugin[]} */ ([]));
  let pluginForm = $state(/** @type {{ id: number | null, name: string, command: string, args: string } | null} */ (null));
  let pluginError = $state("");

  $effect(() => {
    if (isOpen) {
      backupError = "";
      storageError = "";
      shortcutConflictMessage = "";
      recordingId = null;
      pluginForm = null;
      pluginError = "";
      mapsError = "";
      mapsKeyInput = "";
      shortcuts = getStoredShortcuts();
      hasMapsApiKey().then((v) => (mapsKeySaved = v));
      getBackupSettings().then((s) => (settings = s));
      getStorageInfo().then((s) => (storageInfo = s));
      listExportPlugins().then((p) => (plugins = p));
    }
  });

  function openAddPluginForm() {
    pluginForm = { id: null, name: "", command: "", args: "" };
    pluginError = "";
  }

  /** @param {import('$lib/api/export.js').ExportPlugin} plugin */
  function openEditPluginForm(plugin) {
    pluginForm = { id: plugin.id, name: plugin.name, command: plugin.command, args: plugin.args_template.join(" ") };
    pluginError = "";
  }

  async function savePluginForm() {
    if (!pluginForm) return;
    const name = pluginForm.name.trim();
    const command = pluginForm.command.trim();
    if (!name || !command) {
      pluginError = "Name and command are both required.";
      return;
    }
    const argsTemplate = pluginForm.args.split(/\s+/).filter(Boolean);
    try {
      if (pluginForm.id === null) {
        const created = await addExportPlugin(name, command, argsTemplate);
        plugins = [...plugins, created];
      } else {
        await updateExportPlugin(pluginForm.id, name, command, argsTemplate);
        plugins = plugins.map((p) => (p.id === pluginForm?.id ? { ...p, name, command, args_template: argsTemplate } : p));
      }
      pluginForm = null;
      pluginError = "";
    } catch (/** @type {any} */ e) {
      pluginError = `Couldn't save plugin: ${e}`;
    }
  }

  async function handleDeletePlugin(/** @type {number} */ id) {
    try {
      await deleteExportPlugin(id);
      plugins = plugins.filter((p) => p.id !== id);
    } catch (/** @type {any} */ e) {
      pluginError = `Couldn't delete plugin: ${e}`;
    }
  }

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

  /** Human-readable size, matching a scale a user actually cares about
   * for cache cleanup decisions (MB/GB, not raw byte counts). */
  function formatBytes(/** @type {number} */ bytes) {
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
    return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
  }

  async function handlePickCacheDir() {
    const dir = await open({ directory: true, multiple: false });
    if (!dir) return;
    movingCache = true;
    storageError = "";
    try {
      storageInfo = await setCacheDir(/** @type {string} */ (dir));
    } catch (/** @type {any} */ e) {
      storageError = `Couldn't move thumbnails/previews to that folder: ${e}`;
    } finally {
      movingCache = false;
    }
  }

  async function handleResetCacheDir() {
    movingCache = true;
    storageError = "";
    try {
      storageInfo = await setCacheDir(null);
    } catch (/** @type {any} */ e) {
      storageError = `Couldn't move thumbnails/previews back to the default location: ${e}`;
    } finally {
      movingCache = false;
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
          <button
            type="button"
            class="tab-btn"
            class:active={activeTab === "storage"}
            onclick={() => (activeTab = "storage")}
          >
            Storage
          </button>
          <button
            type="button"
            class="tab-btn"
            class:active={activeTab === "exportPlugins"}
            onclick={() => (activeTab = "exportPlugins")}
          >
            Export Plugins
          </button>
          <button
            type="button"
            class="tab-btn"
            class:active={activeTab === "map"}
            onclick={() => (activeTab = "map")}
          >
            Map
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
      {:else if activeTab === "storage" && storageInfo}
        <section class="backup-section">
          <div class="row">
            <span class="label">Thumbnails &amp; preview cache location</span>
            <button class="folder-btn" type="button" onclick={handlePickCacheDir} disabled={movingCache}>
              {storageInfo.effective_dir}
            </button>
          </div>

          <div class="storage-usage">
            <div class="storage-usage-row">
              <span>Thumbnails</span>
              <span class="storage-usage-value">{formatBytes(storageInfo.thumbnails_bytes)}</span>
            </div>
            <div class="storage-usage-row">
              <span>Preview cache</span>
              <span class="storage-usage-value">{formatBytes(storageInfo.previews_bytes)}</span>
            </div>
          </div>

          <p class="storage-note">
            Choosing a new folder moves existing thumbnails and previews there right away, so space is reclaimed on
            the old drive immediately. Both are safe to move or clear -- they're regenerated automatically as needed.
          </p>

          <div class="backup-now-row">
            {#if storageInfo.cache_dir}
              <button class="secondary" type="button" onclick={handleResetCacheDir} disabled={movingCache}>
                {movingCache ? "Moving…" : "Reset to Default Location"}
              </button>
            {:else if movingCache}
              <span class="last-backup">Moving…</span>
            {/if}
          </div>

          {#if storageError}
            <div class="status">{storageError}</div>
          {/if}
        </section>
      {:else if activeTab === "map"}
        <section class="backup-section">
          <p class="storage-note">
            Searching for a place by name or address uses Google's Geocoding API with <em>your own</em> API key
            (create one in Google Cloud Console and enable the Geocoding API; Google may require billing to be
            set up). Only the text you type into the search box, plus the key, is sent to Google — and only when
            you press Search. Catalog and photo data never leave this computer.
          </p>
          <div class="row">
            <label class="label" for="maps-api-key">Google Maps API key</label>
            <input
              id="maps-api-key"
              type="password"
              autocomplete="off"
              bind:value={mapsKeyInput}
              placeholder={mapsKeySaved ? "Key saved — paste a new one to replace it" : "Paste your API key"}
            />
          </div>
          <div class="backup-now-row">
            <button class="primary" type="button" onclick={handleSaveMapsKey} disabled={!mapsKeyInput.trim()}>
              Save Key
            </button>
            {#if mapsKeySaved}
              <button class="secondary" type="button" onclick={handleRemoveMapsKey}>Remove Key</button>
              <span class="last-backup">Key saved</span>
            {/if}
          </div>
          {#if mapsError}
            <div class="status">{mapsError}</div>
          {/if}
        </section>
      {:else if activeTab === "exportPlugins"}
        <section class="backup-section">
          <p class="storage-note">
            Run an external command after each exported file (M5 v0 -- see RFC-0006). The command runs with your own
            account's permissions, exactly as configured below -- there's no sandboxing, so only add commands you
            trust. <code>{"{path}"}</code>, <code>{"{dir}"}</code>, and <code>{"{filename}"}</code> in the arguments
            are replaced with the exported file's own path, folder, and filename.
          </p>

          {#if plugins.length > 0}
            <div class="plugin-list">
              {#each plugins as plugin (plugin.id)}
                <div class="plugin-row">
                  <div class="plugin-info">
                    <span class="plugin-name">{plugin.name}</span>
                    <span class="plugin-command">{plugin.command} {plugin.args_template.join(" ")}</span>
                  </div>
                  <div class="plugin-actions">
                    <button class="link-btn" type="button" onclick={() => openEditPluginForm(plugin)}>Edit</button>
                    <button class="link-btn danger" type="button" onclick={() => handleDeletePlugin(plugin.id)}>Delete</button>
                  </div>
                </div>
              {/each}
            </div>
          {/if}

          {#if pluginForm}
            <div class="plugin-form">
              <div class="row">
                <label class="label" for="plugin-name">Name</label>
                <input id="plugin-name" type="text" bind:value={pluginForm.name} placeholder="e.g. Upload to server" />
              </div>
              <div class="row">
                <label class="label" for="plugin-command">Command</label>
                <input id="plugin-command" type="text" bind:value={pluginForm.command} placeholder="/path/to/script" />
              </div>
              <div class="row">
                <label class="label" for="plugin-args">Arguments (space-separated)</label>
                <input id="plugin-args" type="text" bind:value={pluginForm.args} placeholder="--upload {'{path}'}" />
              </div>
              <div class="backup-now-row">
                <button class="primary" type="button" onclick={savePluginForm}>Save</button>
                <button class="secondary" type="button" onclick={() => (pluginForm = null)}>Cancel</button>
              </div>
            </div>
          {:else}
            <div class="backup-now-row">
              <button class="secondary" type="button" onclick={openAddPluginForm}>Add Plugin</button>
            </div>
          {/if}

          {#if pluginError}
            <div class="status">{pluginError}</div>
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
  input[type="text"],
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
  .storage-usage {
    display: flex;
    flex-direction: column;
    gap: 2px;
    background: var(--bg-panel-raised);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-s);
    padding: 6px 10px;
  }
  .storage-usage-row {
    display: flex;
    justify-content: space-between;
    font-size: 11.5px;
    color: var(--text-secondary);
    padding: 3px 0;
  }
  .storage-usage-value {
    font-family: var(--font-mono);
    color: var(--text-primary);
    font-weight: 600;
  }
  .storage-note {
    margin: 0;
    font-size: 10.5px;
    line-height: 1.5;
    color: var(--text-tertiary);
  }
  .storage-note code {
    font-family: var(--font-mono);
    background: var(--bg-panel-raised);
    padding: 1px 4px;
    border-radius: 3px;
  }
  .plugin-list {
    display: flex;
    flex-direction: column;
    gap: 1px;
    background: var(--bg-panel-raised);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-s);
    overflow: hidden;
  }
  .plugin-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    padding: 8px 10px;
    border-bottom: 1px solid var(--border-subtle);
  }
  .plugin-row:last-child {
    border-bottom: none;
  }
  .plugin-info {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .plugin-name {
    font-size: 12px;
    font-weight: 600;
    color: var(--text-primary);
  }
  .plugin-command {
    font-size: 10.5px;
    font-family: var(--font-mono);
    color: var(--text-tertiary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .plugin-actions {
    display: flex;
    gap: 10px;
    flex-shrink: 0;
  }
  .link-btn {
    all: unset;
    cursor: pointer;
    font-size: 11px;
    color: var(--text-secondary);
  }
  .link-btn:hover {
    color: var(--text-primary);
    text-decoration: underline;
  }
  .link-btn.danger:hover {
    color: var(--label-red);
  }
  .plugin-form {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 10px;
    background: var(--bg-panel-raised);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-s);
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
  .backup-now-row button:disabled,
  .folder-btn:disabled {
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
