<script>
  import { OP_GROUPS } from "$lib/api/develop.js";

  /**
   * Copy Settings (M4.5): checkbox group picker over `OP_GROUPS`, feeding
   * `copySettingsOps` on confirm. Same open/Escape/overlay shape as
   * SmartCollectionDialog/ConfirmDialog -- no new dialog chrome invented.
   * @type {{
   *   open: boolean,
   *   onConfirm: (selectedGroupIds: string[]) => void,
   *   onCancel: () => void,
   * }}
   */
  let { open, onConfirm, onCancel } = $props();

  let selected = $state(/** @type {Set<string>} */ (new Set()));

  // Reset to "everything checked" each time the dialog opens, matching
  // Lightroom's own Copy Settings default.
  $effect(() => {
    if (!open) return;
    selected = new Set(OP_GROUPS.map((g) => g.id));
  });

  function toggle(/** @type {string} */ groupId) {
    const next = new Set(selected);
    if (next.has(groupId)) next.delete(groupId);
    else next.add(groupId);
    selected = next;
  }

  function selectAll() {
    selected = new Set(OP_GROUPS.map((g) => g.id));
  }

  function selectNone() {
    selected = new Set();
  }

  function submit() {
    if (selected.size === 0) return;
    onConfirm([...selected]);
  }
</script>

<svelte:window onkeydown={(e) => open && e.key === "Escape" && onCancel()} />

{#if open}
  <div class="overlay">
    <div class="dialog" role="dialog" aria-modal="true" aria-label="Copy Settings">
      <h2>Copy Settings</h2>

      <div class="field">
        <div class="field-header">
          <span class="field-label">Settings to copy</span>
          <div class="bulk-actions">
            <button type="button" onclick={selectAll}>Select All</button>
            <button type="button" onclick={selectNone}>Select None</button>
          </div>
        </div>
        <div class="groups">
          {#each OP_GROUPS as group (group.id)}
            <label class="group-row">
              <input type="checkbox" checked={selected.has(group.id)} onchange={() => toggle(group.id)} />
              {group.label}
            </label>
          {/each}
        </div>
      </div>

      <div class="actions">
        <button class="secondary" type="button" onclick={onCancel}>Cancel</button>
        <button class="primary" type="button" onclick={submit} disabled={selected.size === 0}>Copy</button>
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
    width: 340px;
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
    gap: 6px;
  }
  .field-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .field-label {
    font-size: 10.5px;
    color: var(--text-tertiary);
  }
  .bulk-actions {
    display: flex;
    gap: 10px;
  }
  .bulk-actions button {
    all: unset;
    cursor: pointer;
    font-size: 10.5px;
    color: var(--accent-strong);
  }
  .groups {
    display: flex;
    flex-direction: column;
    gap: 2px;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-s);
    padding: 6px 8px;
  }
  .group-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 0;
    font-size: 12px;
    color: var(--text-primary);
    cursor: pointer;
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
