<script>
  import { open } from "@tauri-apps/plugin-dialog";
  import { performCatalogBackup } from "$lib/api/backup.js";

  /**
   * Catalog backup (PRD §7.6). One dialog serves both "configure" and
   * "perform" at once -- matches real Lightroom, which also has no
   * persistent backup-settings screen, only the on-close prompt itself.
   * The close handler in +page.svelte awaits `onDone`/`onSkip` before
   * destroying the window, so this dialog also owns the promise-bridge
   * contract: either callback must always eventually fire once the user
   * has made a choice, since "Skip This Time" is the only guaranteed way
   * out of a failed "Back Up Now" attempt.
   * @type {{
   *   open: boolean,
   *   settings: import('$lib/api/backup.js').BackupSettings,
   *   onDone: (settings: import('$lib/api/backup.js').BackupSettings) => void,
   *   onSkip: (settings: import('$lib/api/backup.js').BackupSettings) => void,
   * }}
   */
  let { open: isOpen, settings, onDone, onSkip } = $props();

  let frequency = $state("weekly");
  let folder = $state(/** @type {string | null} */ (null));
  let checkIntegrity = $state(true);
  let optimize = $state(false);
  let backingUp = $state(false);
  let errorMessage = $state("");

  // Re-sync local form state whenever a fresh `settings` snapshot arrives
  // (each close-prompt trigger re-fetches settings before opening this
  // dialog) -- otherwise a previous session's leftover local edits could
  // resurface on the next prompt.
  $effect(() => {
    if (isOpen) {
      frequency = settings.frequency;
      folder = settings.folder;
      checkIntegrity = settings.check_integrity;
      optimize = settings.optimize;
      errorMessage = "";
    }
  });

  function currentFormSettings() {
    return { frequency, folder, check_integrity: checkIntegrity, optimize, last_backup_at: settings.last_backup_at };
  }

  async function pickFolder() {
    const dir = await open({ directory: true, multiple: false });
    if (dir) folder = /** @type {string} */ (dir);
  }

  async function handleBackUpNow() {
    if (!folder) {
      errorMessage = "Choose a backup folder first.";
      return;
    }
    backingUp = true;
    errorMessage = "";
    try {
      const outcome = await performCatalogBackup(folder, checkIntegrity, optimize);
      onDone({ ...currentFormSettings(), last_backup_at: outcome.performed_at });
    } catch (/** @type {any} */ e) {
      // A backup failure must never trap the user -- they can still fix
      // the folder and retry, or click "Skip This Time" to close anyway.
      errorMessage = `Backup failed: ${e}`;
      backingUp = false;
    }
  }

  function handleSkip() {
    onSkip(currentFormSettings());
  }
</script>

{#if isOpen}
  <div class="overlay">
    <div class="dialog" role="dialog" aria-modal="true" aria-label="Back up catalog">
      <h2>Back up your catalog?</h2>
      <p class="hint">A timestamped copy of the catalog file is written to a folder you choose. Your photos are never touched.</p>

      <div class="row">
        <label class="label" for="backup-frequency">Frequency</label>
        <select id="backup-frequency" bind:value={frequency} disabled={backingUp}>
          <option value="every_time">Every time</option>
          <option value="daily">Once a day</option>
          <option value="weekly">Once a week</option>
          <option value="monthly">Once a month</option>
          <option value="never">Never</option>
        </select>
      </div>

      <div class="row">
        <span class="label">Backup folder</span>
        <button class="folder-btn" type="button" onclick={pickFolder} disabled={backingUp}>
          {folder ?? "Choose folder…"}
        </button>
      </div>

      <label class="checkbox-row">
        <input type="checkbox" bind:checked={checkIntegrity} disabled={backingUp} />
        <span>Test integrity before backing up</span>
      </label>
      <label class="checkbox-row">
        <input type="checkbox" bind:checked={optimize} disabled={backingUp} />
        <span>Optimize catalog (vacuum/compact)</span>
      </label>

      {#if errorMessage}
        <div class="status">{errorMessage}</div>
      {/if}

      <div class="actions">
        <button class="secondary" type="button" onclick={handleSkip} disabled={backingUp}>Skip This Time</button>
        <button class="primary" type="button" onclick={handleBackUpNow} disabled={backingUp || !folder}>
          {backingUp ? "Backing up…" : "Back Up Now"}
        </button>
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
    width: 340px;
    background: var(--bg-panel);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-m);
    box-shadow: var(--shadow-soft);
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  h2 {
    margin: 0;
    font-size: 13px;
    font-weight: 600;
    color: var(--text-primary);
  }
  .hint {
    margin: 0 0 4px;
    font-size: 11px;
    line-height: 1.4;
    color: var(--text-secondary);
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
  .folder-btn:disabled,
  select:disabled {
    opacity: 0.6;
  }
  .checkbox-row {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 11.5px;
    color: var(--text-primary);
    cursor: pointer;
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
    margin-top: 6px;
  }
  .actions button {
    all: unset;
    cursor: pointer;
    padding: 6px 14px;
    font-size: 11.5px;
    font-weight: 600;
    border-radius: 6px;
  }
  .actions button:disabled {
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
