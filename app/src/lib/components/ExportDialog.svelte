<script>
  import { open } from "@tauri-apps/plugin-dialog";
  import { exportImages, listExportPlugins } from "$lib/api/export.js";
  import { openFolder } from "$lib/api/system.js";
  import { denoiseMissingForExport } from "$lib/api/denoise.js";
  import { denoise } from "$lib/state/denoise.svelte.js";
  import { handleDenoiseBatch, handleDenoiseBatchStop } from "$lib/actions/denoiseActions.js";

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
  // M5, RFC-0006: the export-plugin hook. "" means "no plugin selected" --
  // <select> values are always strings, so plugin ids (numbers) are only
  // parsed back out at the point handleExport builds `options`.
  let plugins = $state(/** @type {import('$lib/api/export.js').ExportPlugin[]} */ ([]));
  let pluginId = $state("");
  // Metadata embedding is the user's call per export. EXIF (camera settings,
  // capture time, GPS) and IPTC (caption, copyright, contact, keywords) are
  // independent; GPS is a sub-choice of EXIF since location is the field
  // people most often want to strip before sharing.
  let writeExif = $state(true);
  let writeIptc = $state(true);
  let writeGps = $state(true);

  // Loads once per dialog open, not once per component mount -- `items`
  // flips from null to non-null every time the dialog is (re)opened, and a
  // plugin could have been added/edited/deleted in Settings since the last
  // time this dialog was open.
  $effect(() => {
    if (items) {
      listExportPlugins().then((p) => (plugins = p));
    }
  });

  // RFC-0027 §3.6: export never runs the model, so a photo whose edit asks for AI Denoise but that was never
  // denoised would export without it. Say so before the user clicks Export, with the names.
  let missingDenoise = $state(/** @type {{ version_id: number, name: string, path: string, content_hash: string }[]} */ ([]));
  let denoiseNote = $state("");
  function refreshMissingDenoise() {
    const list = items;
    if (!list) return Promise.resolve();
    return denoiseMissingForExport(list.map((i) => i.version_id))
      .then((m) => {
        if (items === list) missingDenoise = m;
      })
      .catch(() => {});
  }
  $effect(() => {
    missingDenoise = [];
    denoiseNote = "";
    if (items) refreshMissingDenoise();
  });

  /** "Denoise these first": the user's explicit go-ahead for the model jobs (RFC-0027 §3.6). */
  async function denoiseFirst() {
    denoiseNote = "";
    const outcome = await handleDenoiseBatch(missingDenoise);
    if (outcome === "no-model") {
      denoiseNote = "The denoise model is not installed yet. Open a photo in Develop and choose Denoise whole photo once to install it.";
    } else if (outcome === "busy") {
      denoiseNote = "Another denoise job is running; try again when it has finished.";
    } else if (outcome === "failed") {
      denoiseNote = "A denoise job failed; the rest were not run.";
    }
    await refreshMissingDenoise();
  }

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
      plugin_id: pluginId ? Number(pluginId) : null,
      metadata: { exif: writeExif, iptc: writeIptc, gps: writeExif && writeGps },
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
      // A plugin's own spawn() failure is reported separately from export
      // failure (RFC-0006 §3.3) -- surfaced here as an appended note, never
      // in place of the export's own success message.
      const pluginFailed = results.filter((r) => r.plugin_error);
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
      const metadataFailed = results.filter((r) => r.metadata_warning);
      if (metadataFailed.length > 0) {
        statusMessage += ` (metadata not written for ${metadataFailed.length}: ${metadataFailed[0].metadata_warning})`;
      }
      if (pluginFailed.length > 0) {
        statusMessage += ` (plugin failed to run: ${pluginFailed[0].plugin_error})`;
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

      {#if missingDenoise.length > 0}
        <p class="notice" role="note" data-testid="export-denoise-missing">
          {missingDenoise.length === 1 ? "1 photo has" : `${missingDenoise.length} photos have`} AI Denoise set but no denoised copy
          ({missingDenoise.slice(0, 3).map((m) => m.name).join(", ")}{missingDenoise.length > 3 ? ", …" : ""}), so
          {missingDenoise.length === 1 ? "it" : "they"} will export without it. Export never runs the model by itself.
          {#if denoise.batch}
            <span data-testid="export-denoise-progress">
              Denoising {denoise.batch.index} of {denoise.batch.total}{denoise.job && denoise.job.total > 0 ? `, tile ${denoise.job.done} of ${denoise.job.total}` : ""}…
            </span>
            <button class="secondary" type="button" onclick={handleDenoiseBatchStop} data-testid="export-denoise-stop">Stop</button>
          {:else}
            <button class="secondary" type="button" onclick={denoiseFirst} disabled={exporting || denoise.job !== null} data-testid="export-denoise-first">
              Denoise {missingDenoise.length === 1 ? "it" : "these"} first
            </button>
          {/if}
          {#if denoiseNote}<span data-testid="export-denoise-note"> {denoiseNote}</span>{/if}
        </p>
      {/if}

      {#if plugins.length > 0}
        <div class="row">
          <label class="label" for="export-plugin">Run after export</label>
          <select id="export-plugin" bind:value={pluginId} disabled={exporting}>
            <option value="">None</option>
            {#each plugins as plugin (plugin.id)}
              <option value={String(plugin.id)}>{plugin.name}</option>
            {/each}
          </select>
        </div>
      {/if}

      <fieldset class="metadata" disabled={exporting}>
        <legend class="label">Metadata</legend>
        <label class="checkbox-row">
          <input type="checkbox" bind:checked={writeExif} />
          Write EXIF (camera, exposure, capture time)
        </label>
        <label class="checkbox-row sub">
          <input type="checkbox" bind:checked={writeGps} disabled={!writeExif} />
          Include GPS location
        </label>
        <label class="checkbox-row">
          <input type="checkbox" bind:checked={writeIptc} />
          Write IPTC (caption, copyright, contact, keywords)
        </label>
      </fieldset>

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
        <button class="secondary" type="button" onclick={onClose} disabled={exporting || denoise.batch !== null}>Close</button>
        <button
          class="primary"
          type="button"
          onclick={handleExport}
          disabled={exporting || !destinationDir || denoise.batch !== null}
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
  input:disabled,
  select:disabled {
    opacity: 0.6;
  }
  .notice {
    margin: 0;
    padding: 6px 8px;
    font-size: 11px;
    line-height: 1.4;
    color: var(--text-secondary);
    border: 1px solid var(--border-strong);
    border-radius: 6px;
  }
  .status {
    font-size: 11px;
    font-family: var(--font-mono);
    color: var(--text-secondary);
    word-break: break-all;
  }
  .metadata {
    all: unset;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .metadata:disabled {
    opacity: 0.6;
  }
  .checkbox-row.sub {
    margin-left: 18px;
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
