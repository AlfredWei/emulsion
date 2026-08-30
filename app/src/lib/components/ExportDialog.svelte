<script>
  import { open } from "@tauri-apps/plugin-dialog";
  import { exportImages } from "$lib/api/export.js";
  import { openFolder } from "$lib/api/system.js";

  /**
   * Batch-capable since M2 Slice 3 -- `items` is the whole selection (or
   * the single Develop image). null = closed; callers guard against
   * opening with an empty array, so `items` is never [].
   * @type {{
   *   items: { path: string, version_id: number }[] | null,
   *   onClose: () => void,
   * }}
   */
  let { items, onClose } = $props();

  let destinationDir = $state(/** @type {string | null} */ (null));
  let longEdge = $state("");
  let quality = $state(90);
  let exporting = $state(false);
  let statusMessage = $state("");
  let revealWhenDone = $state(true);
  // M4.5: real per-file progress, not a single opaque "Exporting…" state.
  // `progressCurrent`/`progressFileName` track the item CURRENTLY being
  // exported (1-indexed, so it reads as "2 of 5" rather than "1 of 5"
  // while the second file is actually mid-export).
  let progressCurrent = $state(0);
  let progressTotal = $state(0);
  let progressFileName = $state("");

  async function pickDestination() {
    const dir = await open({ directory: true, multiple: false });
    if (dir) destinationDir = /** @type {string} */ (dir);
  }

  /** @param {string} path */
  function fileNameOf(path) {
    return path.split(/[\\/]/).pop() ?? path;
  }

  /** M4.5: exports one item per `exportImages` call instead of the whole
   * batch in a single invoke -- lets the UI update between items for real
   * progress. Sequential, not `Promise.all`, matching this app's own
   * "deliberately sequential, not a worker pool" caution elsewhere for
   * full-resolution decode work (preview_cache.rs's own doc comment) --
   * several full-res RAW decodes running concurrently would spike
   * CPU/memory for no real benefit, since Rust's underlying loop was
   * always sequential anyway. */
  async function handleExport() {
    if (!items || items.length === 0 || !destinationDir) return;
    exporting = true;
    statusMessage = "";
    progressCurrent = 0;
    progressTotal = items.length;
    const options = {
      destination_dir: destinationDir,
      long_edge: longEdge.trim() ? Number(longEdge) : null,
      quality,
    };
    const results = /** @type {import('$lib/api/export.js').ExportResult[]} */ ([]);
    try {
      for (const item of items) {
        progressCurrent += 1;
        progressFileName = fileNameOf(item.path);
        const [result] = await exportImages([{ path: item.path, version_id: item.version_id }], options);
        results.push(result);
      }
      const failed = results.filter((r) => r.error);
      if (results.length === 1) {
        // Keep the single-image message shape people already know.
        statusMessage = failed.length > 0
          ? `Export failed: ${failed[0].error}`
          : `Exported to ${results[0].output_path}`;
      } else {
        statusMessage =
          `Exported ${results.length - failed.length} of ${results.length}` +
          (failed.length > 0 ? ` — first failure: ${failed[0].error}` : "");
      }
      // Isolated from the export loop's own try/catch above -- a failure
      // opening the destination folder (e.g. a permission error) must
      // never overwrite the export's own success/failure message with a
      // misleading "Export failed", since the export itself already
      // finished by this point.
      if (revealWhenDone && failed.length < results.length) {
        try {
          await openFolder(destinationDir);
        } catch (/** @type {any} */ e) {
          statusMessage += ` (couldn't open destination folder: ${e})`;
        }
      }
    } catch (/** @type {any} */ e) {
      statusMessage = `Export failed: ${e}`;
    } finally {
      exporting = false;
    }
  }
</script>

<svelte:window onkeydown={(e) => items && e.key === "Escape" && onClose()} />

{#if items}
  <div class="overlay">
    <div class="dialog" role="dialog" aria-modal="true" aria-label="Export">
      <h2>Export{items.length > 1 ? ` ${items.length} photos` : ""}</h2>

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

      <label class="checkbox-row">
        <input type="checkbox" bind:checked={revealWhenDone} disabled={exporting} />
        Show in file manager when done
      </label>

      {#if exporting}
        <div class="progress-row">
          <progress value={progressCurrent} max={progressTotal}></progress>
          <span class="progress-label">
            Exporting {progressCurrent} of {progressTotal}{progressFileName ? ` — ${progressFileName}` : ""}
          </span>
        </div>
      {:else if statusMessage}
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
  input:not([type="checkbox"]),
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
  .checkbox-row {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    color: var(--text-secondary);
    cursor: pointer;
  }
  .progress-row {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  progress {
    width: 100%;
    height: 6px;
    accent-color: var(--accent);
  }
  .progress-label {
    font-size: 10.5px;
    color: var(--text-tertiary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
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
