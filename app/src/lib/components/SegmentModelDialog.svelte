<script>
  /**
   * First-use consent for Select Subject's model files (RFC-0026 §3.5): the one scoped network use
   * of an otherwise offline app. Same overlay/dialog shell and Escape idiom as ConfirmDialog.
   * Stateless: the store's dialog state arrives as props, choices leave as callbacks.
   * @type {{
   *   dialog: null | "consent" | "downloading" | "failed",
   *   host: string,
   *   downloadBytes: number,
   *   downloaded: number,
   *   total: number,
   *   error: string,
   *   onDownload: () => void,
   *   onImport: () => void,
   *   onCancel: () => void,
   *   onRetry: () => void,
   * }}
   */
  let { dialog, host, downloadBytes, downloaded, total, error, onDownload, onImport, onCancel, onRetry } = $props();

  const mb = (/** @type {number} */ b) => Math.round(b / 1_000_000);
  let percent = $derived(total > 0 ? Math.min(100, Math.round((downloaded / total) * 100)) : 0);
</script>

<svelte:window onkeydown={(e) => dialog !== null && dialog !== "downloading" && e.key === "Escape" && onCancel()} />

{#if dialog !== null}
  <div class="overlay">
    <div class="dialog" role="dialog" aria-modal="true" aria-label="Select Subject model" data-testid="segment-model-dialog">
      <h2>Select Subject needs a one-time download</h2>
      {#if dialog === "consent"}
        <p class="message">
          The selection model is about {mb(downloadBytes)} MB and is fetched once from <strong>{host}</strong>, then kept on this
          computer. Nothing about your photos is sent, and selection itself runs entirely offline.
        </p>
        {#if error}<p class="message err" role="alert">{error}</p>{/if}
        <div class="actions">
          <button class="secondary" type="button" onclick={onCancel}>Cancel</button>
          <button class="secondary" type="button" onclick={onImport} title="Choose the two .onnx model files you already have">I have the files</button>
          <button class="primary" type="button" onclick={onDownload}>Download</button>
        </div>
      {:else if dialog === "downloading"}
        <p class="message">Downloading the selection model from {host}…</p>
        <div class="bar" role="progressbar" aria-valuemin="0" aria-valuemax="100" aria-valuenow={percent}>
          <div class="fill" style="width:{percent}%"></div>
        </div>
        <p class="message small">{mb(downloaded)} of {mb(total)} MB. This cannot be cancelled once started; it can be left running.</p>
      {:else}
        <p class="message err" role="alert">{error}</p>
        <div class="actions">
          <button class="secondary" type="button" onclick={onCancel}>Close</button>
          <button class="primary" type="button" onclick={onRetry}>Try again</button>
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
    width: 360px;
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
  .message {
    margin: 0;
    font-size: 12px;
    line-height: 1.5;
    color: var(--text-secondary);
  }
  .message.small {
    font-size: 11px;
    color: var(--text-tertiary);
  }
  .message.err {
    color: var(--label-red);
    word-break: break-word;
  }
  .bar {
    height: 6px;
    border-radius: 3px;
    background: var(--border-subtle);
    overflow: hidden;
  }
  .fill {
    height: 100%;
    background: var(--accent-strong);
    transition: width 0.2s;
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
    background: rgba(var(--accent-rgb, 90, 140, 255), 0.14);
    color: var(--accent-strong);
    border: 1px solid var(--accent-strong);
  }
</style>
