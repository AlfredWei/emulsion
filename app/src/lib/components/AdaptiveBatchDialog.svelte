<script>
  // RFC-0035 B3: the Library's "Apply Adaptive Preset to Selection". Three phases in one dialog: Amount, progress, then the
  // report that names every photo (applied, to check, nothing found, kept because the user changed the mask, failed).
  import { adaptiveBatch } from "$lib/state/adaptiveBatch.svelte.js";
  import { ADAPTIVE_PRESETS, describeReport } from "$lib/adaptivePresets.js";
  import { handleRunAdaptiveBatch, handleCloseAdaptiveBatch, handleCancelAdaptiveBatch, handleSelectBatchRows } from "$lib/actions/adaptiveBatchActions.js";

  const preset = $derived(ADAPTIVE_PRESETS.find((p) => p.id === adaptiveBatch.presetId) ?? null);
  const count = $derived(adaptiveBatch.targets.length);
  const rows = $derived(adaptiveBatch.rows);
  const by = $derived({
    applied: rows.filter((r) => r.status === "applied"),
    check: rows.filter((r) => r.status === "check"),
    nothing: rows.filter((r) => r.status === "nothing"),
    kept: rows.filter((r) => r.status === "kept"),
    failed: rows.filter((r) => r.status === "failed"),
  });
  const notReached = $derived(count - rows.length);

  /** @type {{ key: "check" | "nothing" | "kept" | "failed", title: string, note: string }[]} */
  const GROUPS = [
    { key: "check", title: "Applied, check these", note: "The selection is flagged: open each and look at the highlighted region." },
    { key: "nothing", title: "Nothing found", note: "Nothing was added to these." },
    { key: "kept", title: "Kept as they were", note: "You changed the mask this preset wrote earlier, so it was left alone. Re-apply in Develop to replace it." },
    { key: "failed", title: "Failed", note: "" },
  ];
</script>

<svelte:window onkeydown={(e) => adaptiveBatch.open && e.key === "Escape" && handleCloseAdaptiveBatch()} />

{#if adaptiveBatch.open && preset}
  <div class="overlay">
    <div class="dialog" role="dialog" aria-modal="true" aria-label="Apply {preset.name} to the selection">
      <h2>{preset.name}</h2>

      {#if adaptiveBatch.phase === "confirm"}
        <p class="lead">Finds the {preset.rules.map((r) => r.region).join(" and ")} in each of {count} photo{count === 1 ? "" : "s"} and adds a mask for it. Each photo gets its own History row and its own Undo.</p>
        <label class="amount">
          Amount <input type="range" min="0" max="100" step="5" bind:value={adaptiveBatch.amount} aria-label="Batch adaptive preset amount" />
          <span>{adaptiveBatch.amount} %</span>
        </label>
        <div class="actions">
          <button class="secondary" type="button" onclick={handleCloseAdaptiveBatch}>Cancel</button>
          <button class="primary adaptive-batch-run" type="button" onclick={handleRunAdaptiveBatch}>Apply to {count}</button>
        </div>
      {:else if adaptiveBatch.phase === "running"}
        <p class="lead" aria-live="polite">Looking at photo {Math.min(adaptiveBatch.done + 1, count)} of {count}…</p>
        <progress max={count} value={adaptiveBatch.done}></progress>
        <div class="actions">
          <button class="secondary" type="button" onclick={handleCancelAdaptiveBatch}>{adaptiveBatch.cancelRequested ? "Stopping…" : "Stop after this photo"}</button>
        </div>
      {:else}
        <p class="lead adaptive-batch-summary">
          {by.applied.length + by.check.length} of {count} photo{count === 1 ? "" : "s"} got a mask{by.check.length > 0 ? `, ${by.check.length} to check` : ""}.
          {#if notReached > 0}{notReached} not reached (stopped).{/if}
        </p>
        <div class="report">
          {#each GROUPS as g (g.key)}
            {#if by[g.key].length > 0}
              <section class="group adaptive-batch-{g.key}">
                <div class="group-head">
                  <span class="gt">{g.title} ({by[g.key].length})</span>
                  <button type="button" class="link" onclick={() => handleSelectBatchRows([g.key])}>Select these</button>
                </div>
                {#if g.note}<p class="note">{g.note}</p>{/if}
                <ul>
                  {#each by[g.key] as r (r.versionId)}
                    <li><span class="fname">{r.name}</span> <span class="why">{r.error ?? describeReport(r.report)}</span></li>
                  {/each}
                </ul>
              </section>
            {/if}
          {/each}
          {#if by.applied.length > 0}
            <p class="note adaptive-batch-applied">{by.applied.length} applied cleanly.</p>
          {/if}
        </div>
        <div class="actions">
          <button class="primary" type="button" onclick={handleCloseAdaptiveBatch}>Done</button>
        </div>
      {/if}
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
    width: 420px;
    max-height: 80vh;
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
  .lead,
  .note {
    margin: 0;
    font-size: 11.5px;
    color: var(--text-secondary);
  }
  .note {
    font-size: 10.5px;
    color: var(--text-tertiary);
  }
  .amount {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 11.5px;
    color: var(--text-secondary);
  }
  .amount input {
    flex: 1;
  }
  progress {
    width: 100%;
  }
  .report {
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-height: 0;
  }
  .group-head {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
  }
  .gt {
    font-size: 11.5px;
    font-weight: 600;
    color: var(--text-primary);
  }
  .link {
    all: unset;
    cursor: pointer;
    font-size: 10.5px;
    color: var(--accent-strong);
  }
  ul {
    margin: 4px 0 0;
    padding: 0;
    list-style: none;
    font-size: 11px;
  }
  li {
    padding: 2px 0;
    color: var(--text-primary);
  }
  .why {
    color: var(--text-tertiary);
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
  .secondary {
    color: var(--text-secondary);
    border: 1px solid var(--border-strong);
  }
  .primary {
    background: var(--accent-soft);
    color: var(--accent-strong);
    border: 1px solid var(--accent);
  }
</style>
