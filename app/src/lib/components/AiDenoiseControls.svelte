<script>
  import AiDenoiseCompare from "$lib/components/AiDenoiseCompare.svelte";
  import { nudgeValue } from "$lib/stepMath.js";

  /**
   * The AI Denoise block of the Noise Reduction panel (RFC-0027 §3.3-3.4). Stateless: the store's view
   * arrives as props, choices leave as callbacks. Amount only mixes the kept denoised copy in; the model runs
   * only from the two buttons, and a whole-photo run is always an explicit, cancellable job.
   * @type {{
   *   amount: number,
   *   hasResult: boolean,
   *   model: string | null,
   *   size: { width: number, height: number } | null,
   *   job: { kind: "crop" | "whole", done: number, total: number, label: string } | null,
   *   jobHere: boolean,
   *   crop: { before: string, after: string, width: number, height: number, ms: number } | null,
   *   error: string,
   *   gpuUnavailable: boolean,
   *   onAmountChange: (patch: { amount: number }) => void,
   *   onPreviewCrop: () => void,
   *   onDenoiseWhole: () => void,
   *   onCancel: () => void,
   *   onRemove: () => void,
   *   onDismissCrop: () => void,
   * }}
   */
  let { amount, hasResult, model, size, job, jobHere, crop, error, gpuUnavailable, onAmountChange, onPreviewCrop, onDenoiseWhole, onCancel, onRemove, onDismissCrop } = $props();

  /** @param {string | null} id */
  function modelName(id) {
    return id === "nafnet_sidd_w32" ? "NAFNet (SIDD)" : (id ?? "model");
  }

  let stateText = $derived.by(() => {
    if (job && jobHere) {
      const part = job.total > 0 ? ` ${job.done} of ${job.total} tiles` : "";
      return job.kind === "crop" ? `Previewing a crop…${part}` : `Denoising…${part}`;
    }
    if (job) return `Denoising ${job.label} in the background…`;
    if (hasResult) return `Denoised with ${modelName(model)}${size ? `, ${size.width}×${size.height}` : ""}.`;
    return "Not denoised yet.";
  });

  let percent = $derived(job && job.total > 0 ? Math.round((job.done / job.total) * 100) : 0);
  let confirmingRemove = $state(false);
</script>

<div class="dn" data-testid="ai-denoise">
  <p class="state" data-testid="ai-denoise-state">{stateText}</p>

  <div class="row" class:dim={!hasResult}>
    <label for="ai-denoise-amount">Amount</label>
    <input
      id="ai-denoise-amount"
      type="range"
      min="0"
      max="100"
      step="1"
      disabled={!hasResult}
      value={amount}
      oninput={(e) => onAmountChange({ amount: Number(e.currentTarget.value) })}
    />
    <span class="val">{amount}</span>
    <div class="step-buttons">
      <button type="button" class="step-btn" tabindex="-1" aria-label="Increase AI Denoise amount" disabled={!hasResult || amount >= 100} onclick={() => onAmountChange({ amount: nudgeValue(amount, 1, 1, 0, 100) })}>▲</button>
      <button type="button" class="step-btn" tabindex="-1" aria-label="Decrease AI Denoise amount" disabled={!hasResult || amount <= 0} onclick={() => onAmountChange({ amount: nudgeValue(amount, -1, 1, 0, 100) })}>▼</button>
    </div>
  </div>

  {#if job}
    <div class="bar" role="progressbar" aria-valuemin="0" aria-valuemax="100" aria-valuenow={percent} data-testid="ai-denoise-progress">
      <div class="fill" style="width:{percent}%"></div>
    </div>
    <div class="buttons">
      <button class="btn" type="button" onclick={onCancel} data-testid="ai-denoise-cancel">Cancel</button>
    </div>
  {:else}
    <div class="buttons">
      <button class="btn" type="button" onclick={onPreviewCrop} data-testid="ai-denoise-preview" title="Denoise a rectangle in the middle of the photo to judge the result before the whole photo is done">Preview on a crop</button>
      <button class="btn primary" type="button" onclick={onDenoiseWhole} data-testid="ai-denoise-whole" title="Runs the model over the whole photo on this computer; you can keep editing while it works">{hasResult ? "Denoise again" : "Denoise whole photo"}</button>
    </div>
  {/if}

  {#if error}<p class="error" role="alert" data-testid="ai-denoise-error">{error}</p>{/if}

  {#if crop}
    <AiDenoiseCompare before={crop.before} after={crop.after} width={crop.width} height={crop.height} seconds={Math.max(1, Math.round(crop.ms / 1000))} onClose={onDismissCrop} />
  {/if}

  {#if hasResult && !job}
    <div class="remove">
      {#if confirmingRemove}
        <span class="note">Delete the denoised copy of this photo?</span>
        <button class="link-btn" type="button" onclick={() => { confirmingRemove = false; onRemove(); }} data-testid="ai-denoise-remove-confirm">Delete</button>
        <button class="link-btn" type="button" onclick={() => (confirmingRemove = false)}>Keep</button>
      {:else}
        <button class="link-btn" type="button" onclick={() => (confirmingRemove = true)} data-testid="ai-denoise-remove">Remove denoised copy</button>
      {/if}
    </div>
  {/if}

  <p class="note">
    The model runs on this computer and never while you drag Amount. It works before everything else and combines with Luminance and Color below.
    {#if gpuUnavailable}This preview needs WebGPU to show it; export still applies it.{/if}
  </p>
</div>

<style>
  .dn {
    padding: 0 0 4px;
  }
  .state {
    margin: 0;
    padding: 2px 4px 4px;
    font-size: 11px;
    color: var(--text-secondary);
  }
  .note {
    margin: 0;
    padding: 4px 4px 2px;
    font-size: 10.5px;
    line-height: 1.35;
    color: var(--text-tertiary);
  }
  .error {
    margin: 0;
    padding: 4px;
    font-size: 11px;
    line-height: 1.4;
    color: var(--label-red);
    word-break: break-word;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 5px 4px;
  }
  .row.dim label,
  .row.dim .val,
  .row.dim input[type="range"] {
    opacity: 0.45;
  }
  .row label {
    width: 66px;
    font-size: 11px;
    color: var(--text-secondary);
    flex: none;
  }
  .row input[type="range"] {
    flex: 1;
    min-width: 0;
    appearance: none;
    -webkit-appearance: none;
    height: 3px;
    background: var(--border-strong);
    border-radius: 2px;
    outline: none;
  }
  .row input[type="range"]::-webkit-slider-thumb {
    -webkit-appearance: none;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: var(--accent-strong);
    border: 2px solid var(--bg-panel);
    cursor: pointer;
  }
  .row .val {
    width: 36px;
    text-align: right;
    font-family: var(--font-mono);
    font-variant-numeric: tabular-nums;
    font-size: 10.5px;
    color: var(--text-tertiary);
  }
  .step-buttons {
    display: flex;
    flex-direction: column;
    flex: none;
    width: 12px;
  }
  .step-btn {
    appearance: none;
    -webkit-appearance: none;
    border: none;
    background: transparent;
    color: var(--text-tertiary);
    font-size: 7px;
    line-height: 1;
    padding: 1px 0;
    cursor: pointer;
  }
  .step-btn:hover:not(:disabled) {
    color: var(--accent-strong);
  }
  .step-btn:disabled {
    opacity: 0.3;
    cursor: default;
  }
  .buttons {
    display: flex;
    gap: 6px;
    padding: 4px 4px 2px;
  }
  .btn {
    all: unset;
    cursor: pointer;
    flex: 1;
    text-align: center;
    padding: 5px 6px;
    font-size: 11px;
    font-weight: 600;
    border-radius: 6px;
    color: var(--text-secondary);
    border: 1px solid var(--border-strong);
  }
  .btn:hover {
    border-color: var(--accent-strong);
  }
  .btn.primary {
    background: rgba(var(--accent-rgb, 90, 140, 255), 0.14);
    color: var(--accent-strong);
    border-color: var(--accent-strong);
  }
  .bar {
    height: 6px;
    margin: 4px;
    border-radius: 3px;
    background: var(--border-subtle);
    overflow: hidden;
  }
  .fill {
    height: 100%;
    background: var(--accent-strong);
    transition: width 0.2s;
  }
  .remove {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 2px 4px;
  }
  .remove .note {
    padding: 0;
  }
  .link-btn {
    appearance: none;
    background: none;
    border: none;
    padding: 0;
    font: 10.5px var(--font-ui);
    color: var(--accent-strong);
    cursor: pointer;
  }
  .link-btn:hover {
    text-decoration: underline;
  }
</style>
