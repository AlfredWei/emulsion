<script>
  import { MAX_MASKS } from "$lib/api/develop.js";

  /**
   * Contextual tool strip above the Filmstrip in Develop, matching
   * UX-DESIGN.md's described layout ("a contextual tool strip above [the
   * filmstrip] for crop/mask/heal tools when active... designed into this
   * layout now"). M3 Slice 5: one tool (Linear Gradient) plus a simple
   * list of placed masks -- click-to-select is simpler and more robust
   * here than hit-testing a click near a diagonal line on the canvas.
   * @type {{
   *   activeTool: string | null,
   *   masks: import('$lib/api/develop.js').LinearGradientMask[],
   *   selectedMaskId: string | null,
   *   onToolToggle: (tool: string) => void,
   *   onMaskSelect: (id: string) => void,
   * }}
   */
  let { activeTool, masks, selectedMaskId, onToolToggle, onMaskSelect } = $props();

  let atCap = $derived(masks.length >= MAX_MASKS);
</script>

<div class="strip">
  <button
    class="tool"
    class:active={activeTool === "linear_gradient"}
    type="button"
    disabled={atCap && activeTool !== "linear_gradient"}
    title={atCap ? `Maximum ${MAX_MASKS} masks reached` : "Linear Gradient"}
    onclick={() => onToolToggle("linear_gradient")}
  >Linear Gradient</button>

  {#if masks.length > 0}
    <div class="divider"></div>
    <div class="mask-list">
      {#each masks as mask, i (mask.id)}
        <button
          class="mask-chip"
          class:selected={mask.id === selectedMaskId}
          type="button"
          onclick={() => onMaskSelect(mask.id)}
        >Gradient {i + 1}</button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .strip {
    flex: none;
    display: flex;
    align-items: center;
    gap: 10px;
    height: 34px;
    padding: 0 14px;
    background: var(--bg-panel);
    border-top: 1px solid var(--border-subtle);
    border-bottom: 1px solid var(--border-subtle);
  }
  .tool {
    all: unset;
    cursor: pointer;
    padding: 4px 10px;
    font-size: 11px;
    font-weight: 600;
    border-radius: var(--radius-s);
    color: var(--text-secondary);
    border: 1px solid var(--border-strong);
  }
  .tool.active {
    background: var(--accent-soft);
    color: var(--accent-strong);
    border-color: var(--accent);
  }
  .tool:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .divider {
    width: 1px;
    height: 16px;
    background: var(--border-subtle);
  }
  .mask-list {
    display: flex;
    align-items: center;
    gap: 6px;
    overflow-x: auto;
  }
  .mask-chip {
    all: unset;
    cursor: pointer;
    flex: none;
    padding: 3px 9px;
    font-size: 10.5px;
    font-family: var(--font-mono);
    border-radius: var(--radius-s);
    color: var(--text-tertiary);
    border: 1px solid var(--border-subtle);
  }
  .mask-chip.selected {
    color: var(--accent-strong);
    border-color: var(--accent);
    background: var(--accent-soft);
  }
</style>
