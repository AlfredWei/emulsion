<script>
  // RFC-0035 slice C: the Adaptive section of the History panel's Presets tab. Pick a preset, set Amount, Apply; the
  // report below says per rule what was found, flagged for a look, or not found. Applying again (Re-apply) replaces the
  // masks the preset made, and asks first when the user changed one. The new mask is opened by the apply itself
  // (adaptiveActions.js), so what was found is on screen with its overlay.
  import { describeRule } from "$lib/adaptivePresets.js";

  /**
   * @type {{
   *   presets: { id: string, name: string, rules: { region: string }[] }[],
   *   planned: { id: string, name: string, region: string, needs: string }[],
   *   applied: Record<string, { edited: boolean }>,
   *   photoKey: unknown,
   *   onApply: (presetId: string, amount: number, options: { replaceEdited: boolean }) => Promise<import('$lib/adaptivePresets.js').RuleReport[] | null>,
   *   onOpenApplied: (presetId: string) => void,
   *   onSelectItYourself: (region: string) => void,
   * }}
   */
  let { presets, planned, applied, photoKey, onApply, onOpenApplied, onSelectItYourself } = $props();

  let selectedId = $state(/** @type {string | null} */ (null));
  let amount = $state(100);
  let confirming = $state(false);
  let busy = $state(false);
  let report = $state(/** @type {import('$lib/adaptivePresets.js').RuleReport[] | null} */ (null));

  const selected = $derived(presets.find((p) => p.id === selectedId) ?? null);
  const isApplied = $derived(selected !== null && applied[selected.id] !== undefined);

  // A report belongs to one photo and one preset.
  $effect(() => {
    void photoKey;
    void selectedId;
    report = null;
    confirming = false;
  });

  /** @param {string} id */
  function choose(id) {
    selectedId = id;
    if (applied[id]) onOpenApplied(id);
  }

  /** @param {boolean} replaceEdited */
  async function run(replaceEdited) {
    if (!selected || busy) return;
    busy = true;
    confirming = false;
    try {
      report = await onApply(selected.id, amount, { replaceEdited });
    } finally {
      busy = false;
    }
  }

  function apply() {
    if (selected && applied[selected.id]?.edited) {
      confirming = true;
      return;
    }
    void run(false);
  }

  const ICON = { found: "✓", check: "!", not_found: "–", skipped: "–" };
  const added = $derived(report !== null && report.some((r) => r.action !== "skipped"));
</script>

<div class="adaptive-block">
  <div class="adaptive-title">Adaptive</div>
  {#each presets as p (p.id)}
    <button type="button" class="adaptive-row" class:sel={p.id === selectedId} aria-pressed={p.id === selectedId} onclick={() => choose(p.id)}
      title="Finds the {p.rules.map((r) => r.region).join(' and ')} in this photo and adds a mask for it">
      <span class="name">{p.name}</span>
      <span class="chip">{p.rules.map((r) => r.region).join(", ")}</span>
    </button>
  {/each}
  {#each planned as p (p.id)}
    <div class="adaptive-row off" title={p.needs}>
      <span class="name">{p.name}<span class="needs">{p.needs}</span></span>
      <span class="chip">{p.region}</span>
    </div>
  {/each}

  <label class="adaptive-amount">
    Amount <input type="range" min="0" max="100" step="5" bind:value={amount} aria-label="Adaptive preset amount" />
    <span>{amount} %</span>
  </label>
  <button type="button" class="adaptive-apply" onclick={apply}>{busy ? "Looking…" : isApplied ? "Re-apply" : "Apply"}</button>
  {#if selected === null}
    <p class="adaptive-hint">Pick a preset. It finds the region in this photo and adds a mask for it.</p>
  {/if}

  {#if confirming && selected}
    <div class="adaptive-confirm" role="alertdialog" aria-label="Replace the mask you changed?">
      <p class="q">Replace the mask you changed?</p>
      <p>You changed it after {selected.name} wrote it. Re-applying finds the region again and replaces your changes.</p>
      <div class="btns">
        <button type="button" class="adaptive-keep" onclick={() => (confirming = false)}>Keep mine</button>
        <button type="button" class="adaptive-replace" onclick={() => run(true)}>Replace</button>
      </div>
    </div>
  {/if}

  {#if report}
    <div class="adaptive-report" aria-live="polite">
      {#each report as r, i (i)}
        {@const d = describeRule(r)}
        <div class="line {d.kind}">
          <span class="ico" aria-hidden="true">{ICON[d.kind]}</span>
          <div>
            <div class="rname">{d.name}</div>
            <div class="rtext">{d.text}</div>
            {#if d.kind === "check" && (r.region === "sky" || r.region === "face" || r.region === "person")}
              <button type="button" class="adaptive-yourself" onclick={() => onSelectItYourself(r.region)}>Select it yourself</button>
            {/if}
          </div>
        </div>
      {/each}
      {#if !added}
        <p class="rtext">Nothing was added. Your photo is unchanged.</p>
      {/if}
    </div>
  {/if}
</div>

<style>
  .adaptive-block {
    margin-top: 12px;
    padding-top: 8px;
    border-top: 1px solid var(--border-subtle);
  }
  .adaptive-title {
    margin: 0 4px 6px;
    font-size: 11px;
    font-weight: 600;
    color: var(--text-secondary);
  }
  .adaptive-row {
    all: unset;
    box-sizing: border-box;
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    padding: 6px 8px;
    margin-bottom: 4px;
    font-size: 12px;
    color: var(--text-primary);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-s);
    cursor: pointer;
  }
  .adaptive-row:hover {
    border-color: var(--border-strong);
  }
  .adaptive-row.sel {
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .adaptive-row.off {
    cursor: default;
    opacity: 0.55;
  }
  .adaptive-row.off:hover {
    border-color: var(--border-subtle);
  }
  .name {
    flex: 1;
    min-width: 0;
  }
  .needs {
    display: block;
    font-size: 10.5px;
    color: var(--text-tertiary);
  }
  .chip {
    font-size: 10.5px;
    color: var(--text-secondary);
  }
  .adaptive-amount {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 8px 4px 6px;
    font-size: 11px;
    color: var(--text-tertiary);
  }
  .adaptive-amount input {
    flex: 1;
    min-width: 0;
  }
  .adaptive-apply {
    all: unset;
    box-sizing: border-box;
    display: block;
    width: 100%;
    padding: 5px 0;
    text-align: center;
    font-size: 12px;
    font-weight: 600;
    color: var(--text-primary);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-s);
    cursor: pointer;
  }
  .adaptive-apply:hover {
    background: var(--bg-hover, var(--accent-soft));
  }
  .adaptive-hint,
  .rtext {
    margin: 6px 4px 0;
    font-size: 10.5px;
    color: var(--text-tertiary);
  }
  .adaptive-confirm {
    margin-top: 8px;
    padding: 8px;
    font-size: 11px;
    color: var(--text-secondary);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-s);
  }
  .adaptive-confirm p {
    margin: 0 0 6px;
  }
  .adaptive-confirm .q {
    font-weight: 600;
    color: var(--text-primary);
  }
  .btns {
    display: flex;
    gap: 6px;
    justify-content: flex-end;
  }
  .btns button,
  .adaptive-yourself {
    all: unset;
    cursor: pointer;
    padding: 3px 8px;
    font-size: 11px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-s);
  }
  .adaptive-report {
    margin-top: 8px;
    padding-top: 8px;
    border-top: 1px solid var(--border-subtle);
  }
  .line {
    display: flex;
    gap: 6px;
    margin-bottom: 6px;
  }
  .ico {
    width: 14px;
    text-align: center;
    font-weight: 700;
  }
  .found .ico {
    color: var(--label-green, var(--accent));
  }
  .check .ico {
    color: var(--label-yellow, var(--text-primary));
  }
  .not_found .ico,
  .skipped .ico {
    color: var(--text-tertiary);
  }
  .rname {
    font-size: 11.5px;
    font-weight: 600;
    color: var(--text-primary);
  }
  .line .rtext {
    margin: 0 0 4px;
  }
</style>
