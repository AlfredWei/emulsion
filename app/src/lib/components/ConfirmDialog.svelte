<script>
  /**
   * Small confirm dialog (M2 Slice 3), same overlay/dialog shell and
   * `open &&`-gated Escape idiom as ExportDialog. Only one modal can be
   * open at a time in practice (the page opens one or the other), so the
   * two Escape handlers can't double-fire.
   * @type {{
   *   open: boolean,
   *   title: string,
   *   message: string,
   *   confirmLabel: string,
   *   onConfirm: () => void,
   *   onCancel: () => void,
   * }}
   */
  let { open, title, message, confirmLabel, onConfirm, onCancel } = $props();
</script>

<svelte:window onkeydown={(e) => open && e.key === "Escape" && onCancel()} />

{#if open}
  <div class="overlay">
    <div class="dialog" role="dialog" aria-modal="true" aria-label={title}>
      <h2>{title}</h2>
      <p class="message">{message}</p>
      <div class="actions">
        <button class="secondary" type="button" onclick={onCancel}>Cancel</button>
        <button class="danger" type="button" onclick={onConfirm}>{confirmLabel}</button>
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
  .danger {
    background: rgba(225, 88, 91, 0.12);
    color: var(--label-red);
    border: 1px solid var(--label-red);
  }
</style>
