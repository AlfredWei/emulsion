<script>
  /**
   * Generic single-text-field dialog (M2 Slice 5) -- ConfirmDialog is
   * confirm/cancel only, no input; this is its name-a-thing sibling, same
   * overlay/dialog shell and `open &&`-gated Escape idiom. Used for "New
   * Collection" naming.
   * @type {{
   *   open: boolean,
   *   title: string,
   *   label: string,
   *   placeholder?: string,
   *   confirmLabel: string,
   *   onConfirm: (value: string) => void,
   *   onCancel: () => void,
   * }}
   */
  let { open, title, label, placeholder = "", confirmLabel, onConfirm, onCancel } = $props();

  let value = $state("");

  // Reset the field each time the dialog opens, not just on first mount.
  $effect(() => {
    if (open) value = "";
  });

  function submit() {
    const trimmed = value.trim();
    if (!trimmed) return;
    onConfirm(trimmed);
  }
</script>

<svelte:window onkeydown={(e) => open && e.key === "Escape" && onCancel()} />

{#if open}
  <div class="overlay">
    <div class="dialog" role="dialog" aria-modal="true" aria-label={title}>
      <h2>{title}</h2>
      <div class="field">
        <label for="text-prompt-input">{label}</label>
        <input
          id="text-prompt-input"
          type="text"
          {placeholder}
          bind:value
          onkeydown={(e) => e.key === "Enter" && submit()}
        />
      </div>
      <div class="actions">
        <button class="secondary" type="button" onclick={onCancel}>Cancel</button>
        <button class="primary" type="button" onclick={submit} disabled={!value.trim()}>{confirmLabel}</button>
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
  .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .field label {
    font-size: 10.5px;
    color: var(--text-tertiary);
  }
  .field input {
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
  .field input:focus {
    outline: 2px solid var(--accent);
    outline-offset: -1px;
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
