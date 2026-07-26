<script>
  import { open } from "@tauri-apps/plugin-dialog";
  import { exportImages } from "$lib/api/export.js";

  /**
   * @type {{
   *   item: { path: string, version_id: number } | null,
   *   onClose: () => void,
   * }}
   */
  let { item, onClose } = $props();

  let destinationDir = $state(/** @type {string | null} */ (null));
  let longEdge = $state("");
  let quality = $state(90);
  let exporting = $state(false);
  let statusMessage = $state("");

  async function pickDestination() {
    const dir = await open({ directory: true, multiple: false });
    if (dir) destinationDir = /** @type {string} */ (dir);
  }

  async function handleExport() {
    if (!item || !destinationDir) return;
    exporting = true;
    statusMessage = "";
    try {
      const [result] = await exportImages(
        [{ path: item.path, version_id: item.version_id }],
        {
          destination_dir: destinationDir,
          long_edge: longEdge.trim() ? Number(longEdge) : null,
          quality,
        },
      );
      statusMessage = result.error
        ? `Export failed: ${result.error}`
        : `Exported to ${result.output_path}`;
    } catch (/** @type {any} */ e) {
      statusMessage = `Export failed: ${e}`;
    } finally {
      exporting = false;
    }
  }
</script>

<svelte:window onkeydown={(e) => item && e.key === "Escape" && onClose()} />

{#if item}
  <div class="overlay">
    <div class="dialog" role="dialog" aria-modal="true" aria-label="Export">
      <h2>Export</h2>

      <div class="row">
        <span class="label">Destination</span>
        <button class="folder-btn" type="button" onclick={pickDestination} disabled={exporting}>
          {destinationDir ?? "Choose folder…"}
        </button>
      </div>

      <div class="row">
        <label class="label" for="export-long-edge">Long edge (px)</label>
        <input
          id="export-long-edge"
          type="number"
          min="1"
          placeholder="Original size"
          bind:value={longEdge}
          disabled={exporting}
        />
      </div>

      <div class="row">
        <label class="label" for="export-quality">Quality</label>
        <input
          id="export-quality"
          type="number"
          min="1"
          max="100"
          bind:value={quality}
          disabled={exporting}
        />
      </div>

      {#if statusMessage}
        <div class="status">{statusMessage}</div>
      {/if}

      <div class="actions">
        <button class="secondary" type="button" onclick={onClose} disabled={exporting}>Close</button>
        <button
          class="primary"
          type="button"
          onclick={handleExport}
          disabled={exporting || !destinationDir}
        >
          {exporting ? "Exporting…" : "Export"}
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
    width: 320px;
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
    margin: 0 0 4px;
    font-size: 13px;
    font-weight: 600;
    color: var(--text-primary);
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
  input,
  .folder-btn {
    all: unset;
    box-sizing: border-box;
    width: 100%;
    padding: 6px 8px;
    font-size: 12px;
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
  input:disabled {
    opacity: 0.6;
  }
  .status {
    font-size: 11px;
    font-family: var(--font-mono);
    color: var(--text-secondary);
    word-break: break-all;
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
