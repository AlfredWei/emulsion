<script>
  /**
   * Enlarge (AI) (RFC-0029 §3.2): scale, Strength, the numbers for what it will make, and where it goes. Nothing is
   * written until Enlarge is pressed; the original is never changed. The form is the `upscale` store's, so the
   * choices are the same next time. The first use asks to download the model (ModelDownloadDialog, mounted beside).
   */
  import { upscale } from "$lib/state/upscale.svelte.js";
  import { closeEnlargeDialog, refreshEnlargePlan, handleEnlarge } from "$lib/actions/enlargeActions.js";
  import { targetOf, planRows, veryLargeWarning, defaultStem, folderOf, MAX_SIDE } from "$lib/enlarge.js";

  let photo = $derived(upscale.target);
  let target = $derived(targetOf(upscale.settings));
  let warning = $derived(upscale.plan ? veryLargeWarning(upscale.plan) : "");
  let modelMissing = $derived(upscale.modelStatus?.state === "missing");
  let canEnlarge = $derived(upscale.plan !== null && upscale.job === null);

  /** @param {string} value */
  function chooseScale(value) {
    upscale.settings.scale = /** @type {"2" | "4" | "long_edge"} */ (value);
    refreshEnlargePlan();
  }
</script>

<svelte:window onkeydown={(e) => photo !== null && upscale.modelDialog === null && e.key === "Escape" && closeEnlargeDialog()} />

{#if photo}
  <div class="overlay">
    <div class="dialog" role="dialog" aria-modal="true" aria-label="Enlarge (AI)" data-testid="enlarge-dialog">
      <h2>Enlarge (AI)</h2>
      <p class="source" title={photo.path}>{photo.path.split(/[\\/]/).pop()}</p>

      <fieldset class="group">
        <legend class="label">Scale</legend>
        <div class="seg" role="radiogroup" aria-label="Scale">
          {#each [["2", "2×"], ["4", "4×"], ["long_edge", "Long edge"]] as [value, text] (value)}
            <label class:on={upscale.settings.scale === value}>
              <input type="radio" name="enlarge-scale" {value} checked={upscale.settings.scale === value} onchange={() => chooseScale(value)} />
              {text}
            </label>
          {/each}
        </div>
        {#if upscale.settings.scale === "long_edge"}
          <div class="edge">
            <input
              type="text"
              inputmode="numeric"
              aria-label="Long edge in pixels"
              placeholder="e.g. 8000"
              bind:value={upscale.settings.longEdge}
              oninput={refreshEnlargePlan}
              data-testid="enlarge-long-edge"
            />
            <span class="unit">px on the long side · JPEG holds at most {MAX_SIDE.toLocaleString("en-US")} px on a side</span>
          </div>
        {/if}
      </fieldset>

      <div class="row">
        <label class="label" for="enlarge-strength">Strength</label>
        <div class="strength">
          <input id="enlarge-strength" type="range" min="1" max="100" step="1" bind:value={upscale.settings.strength} />
          <output for="enlarge-strength">{upscale.settings.strength}</output>
        </div>
        <p class="hint">
          Low values stay close to a plain resize, 100 is the model alone. The model invents the detail it adds;
          lower the strength if fine regular patterns look wrong (look at the railings).
        </p>
      </div>

      <div class="summary" aria-live="polite" data-testid="enlarge-summary">
        {#if upscale.plan}
          {#each planRows(upscale.plan) as row (row.label)}
            <div class="line"><span class="k">{row.label}</span><span class="v">{row.value}</span></div>
          {/each}
        {:else if upscale.planError}
          <p class="err" role="alert">{upscale.planError}</p>
        {:else if target === null}
          <p class="muted">Enter the long edge you want, in pixels.</p>
        {:else}
          <p class="muted">Working out the size…</p>
        {/if}
      </div>
      {#if warning}
        <p class="warn" role="alert" data-testid="enlarge-very-large">⚠ {warning}</p>
      {/if}

      <fieldset class="group">
        <legend class="label">Save as</legend>
        <div class="seg" role="radiogroup" aria-label="JPEG quality">
          {#each [[95, "JPEG 95"], [100, "JPEG 100"]] as [value, text] (value)}
            <label class:on={upscale.settings.quality === value}>
              <input
                type="radio"
                name="enlarge-quality"
                {value}
                checked={upscale.settings.quality === value}
                onchange={() => {
                  upscale.settings.quality = /** @type {95 | 100} */ (value);
                  refreshEnlargePlan();
                }}
              />
              {text}
            </label>
          {/each}
        </div>
        <input
          type="text"
          aria-label="File name"
          placeholder={defaultStem(photo.path, target)}
          bind:value={upscale.fileName}
          data-testid="enlarge-name"
        />
        <p class="hint">Saved next to the original ({folderOf(photo.path).split(/[\\/]/).pop()}), added to the catalog and linked to it.</p>
      </fieldset>

      <label class="check">
        <input type="checkbox" bind:checked={upscale.settings.addToCollections} />
        Add to the same collections as the original
      </label>
      <label class="check">
        <input type="checkbox" bind:checked={upscale.settings.metadataNote} />
        Note “enlarged with AI” in the file's metadata
      </label>

      {#if modelMissing}
        <p class="notice" data-testid="enlarge-model-missing">
          The enlargement model is not installed. It is a one-time download of about
          {Math.round((upscale.modelStatus?.download_bytes ?? 0) / 1_000_000)} MB; you will be asked before anything is fetched.
        </p>
      {/if}
      {#if upscale.error}<p class="err" role="alert">{upscale.error}</p>{/if}

      <p class="hint foot">Nothing is written until you press Enlarge. The original is never changed.</p>
      <div class="actions">
        <button class="secondary" type="button" onclick={closeEnlargeDialog}>Cancel</button>
        <button class="primary" type="button" onclick={handleEnlarge} disabled={!canEnlarge} data-testid="enlarge-go">
          {modelMissing ? "Download model and enlarge" : "Enlarge"}
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
    width: 380px;
    max-height: calc(100vh - 40px);
    overflow-y: auto;
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
  .source {
    margin: -6px 0 0;
    font-size: 11px;
    color: var(--text-tertiary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .row,
  .group {
    all: unset;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .label {
    font-size: 11px;
    color: var(--text-secondary);
    padding: 0;
  }
  .seg {
    display: flex;
    gap: 0;
  }
  .seg label {
    flex: 1;
    text-align: center;
    padding: 5px 8px;
    font-size: 11.5px;
    color: var(--text-secondary);
    background: var(--bg-panel-raised);
    border: 1px solid var(--border-subtle);
    cursor: pointer;
  }
  .seg label:first-child {
    border-radius: var(--radius-s) 0 0 var(--radius-s);
  }
  .seg label:last-child {
    border-radius: 0 var(--radius-s) var(--radius-s) 0;
  }
  .seg label.on {
    background: var(--accent-soft);
    color: var(--accent-strong);
    border-color: var(--accent);
  }
  .seg input {
    position: absolute;
    opacity: 0;
    pointer-events: none;
  }
  .seg label:focus-within {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }
  input[type="text"] {
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
  .edge {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .unit {
    font-size: 10.5px;
    color: var(--text-tertiary);
  }
  .strength {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .strength input {
    flex: 1;
    accent-color: var(--accent);
  }
  .strength output {
    width: 28px;
    text-align: right;
    font-size: 11.5px;
    font-family: var(--font-mono);
    color: var(--text-primary);
  }
  .hint {
    margin: 0;
    font-size: 10.5px;
    line-height: 1.4;
    color: var(--text-tertiary);
  }
  .hint.foot {
    margin-top: 2px;
  }
  .summary {
    padding: 8px 10px;
    background: var(--bg-panel-raised);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-s);
    display: flex;
    flex-direction: column;
    gap: 3px;
    min-height: 54px;
    justify-content: center;
  }
  .line {
    display: flex;
    gap: 8px;
    font-size: 11.5px;
  }
  .k {
    width: 48px;
    flex: none;
    color: var(--text-secondary);
  }
  .v {
    color: var(--text-primary);
  }
  .muted {
    margin: 0;
    font-size: 11px;
    color: var(--text-tertiary);
  }
  .err {
    margin: 0;
    font-size: 11px;
    color: var(--danger, #e5484d);
  }
  .warn {
    margin: 0;
    padding: 6px 8px;
    font-size: 11px;
    line-height: 1.4;
    color: var(--text-primary);
    border: 1px solid var(--border-strong);
    border-radius: 6px;
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
  .check {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    color: var(--text-secondary);
    cursor: pointer;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
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
  .actions .primary {
    background: var(--accent-soft);
    color: var(--accent-strong);
    border: 1px solid var(--accent);
  }
  .actions .secondary {
    color: var(--text-secondary);
    border: 1px solid var(--border-subtle);
  }
</style>
