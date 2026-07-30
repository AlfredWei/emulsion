<script>
  import { open } from "@tauri-apps/plugin-dialog";
  import { getBackupSettings, updateBackupSettings, performCatalogBackup } from "$lib/api/backup.js";

  /**
   * General app-wide Settings/Preferences dialog (M3 Slice 1). Establishes
   * the pattern going forward: a new configurable feature adds its own
   * named <section> below rather than a bespoke one-off dialog. Only one
   * section exists today (Backup) -- no sidebar/tab navigation, that would
   * be over-building for one section.
   *
   * Unlike BackupPromptDialog (which only persists on "Back Up Now" /
   * "Skip This Time"), every field here saves immediately on change --
   * there's no dirty-state/cancel model anywhere else in this app to be
   * consistent with, and "Close" never has anything pending to lose.
   * @type {{ open: boolean, onClose: () => void }}
   */
  let { open: isOpen, onClose } = $props();

  let settings = $state(/** @type {import('$lib/api/backup.js').BackupSettings | null} */ (null));
  let backingUp = $state(false);
  let backupError = $state("");

  $effect(() => {
    if (isOpen) {
      backupError = "";
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
</script>

<svelte:window onkeydown={(e) => isOpen && e.key === "Escape" && onClose()} />

{#if isOpen}
  <div class="overlay">
    <div class="dialog" role="dialog" aria-modal="true" aria-label="Settings">
      <h2>Settings</h2>

      {#if settings}
        <section>
          <h3>Backup</h3>

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
    background: rgba(0, 0, 0, 0.5);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 100;
  }
  .dialog {
    width: 380px;
    background: var(--bg-panel);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-m);
    box-shadow: var(--shadow-soft);
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  h2 {
    margin: 0;
    font-size: 13px;
    font-weight: 600;
    color: var(--text-primary);
  }
  section {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding-top: 10px;
    border-top: 1px solid var(--border-subtle);
  }
  h3 {
    margin: 0;
    font-family: var(--font-mono);
    font-size: 10px;
    letter-spacing: 0.09em;
    text-transform: uppercase;
    color: var(--text-tertiary);
    font-weight: 600;
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
