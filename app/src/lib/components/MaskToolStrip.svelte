<script>
  import { MAX_MASKS } from "$lib/api/develop.js";

  /**
   * Contextual tool strip above the Filmstrip in Develop, matching
   * UX-DESIGN.md's described layout ("a contextual tool strip above [the
   * filmstrip] for crop/mask/heal tools when active... designed into this
   * layout now"). M3 Slice 5/6/7: Linear Gradient, Radial Gradient, and
   * Brush tools, plus a simple list of placed masks -- click-to-select is
   * simpler and more robust here than hit-testing a click near a diagonal
   * line/ellipse/painted region on the canvas.
   * @type {{
   *   activeTool: string | null,
   *   masks: import('$lib/api/develop.js').Mask[],
   *   selectedMaskId: string | null,
   *   brushSize: number,
   *   brushHardness: number,
   *   brushFlow: number,
   *   eraseMode: boolean,
   *   onToolToggle: (tool: string) => void,
   *   onMaskSelect: (id: string) => void,
   *   onBrushSizeChange: (value: number) => void,
   *   onBrushHardnessChange: (value: number) => void,
   *   onBrushFlowChange: (value: number) => void,
   *   onEraseToggle: () => void,
   *   onNewBrush: () => void,
   * }}
   */
  let {
    activeTool,
    masks,
    selectedMaskId,
    brushSize,
    brushHardness,
    brushFlow,
    eraseMode,
    onToolToggle,
    onMaskSelect,
    onBrushSizeChange,
    onBrushHardnessChange,
    onBrushFlowChange,
    onEraseToggle,
    onNewBrush,
  } = $props();

  let atCap = $derived(masks.length >= MAX_MASKS);

  // Per-kind chip numbering ("Gradient 1", "Radial 1", "Brush 1") -- three
  // independent counters, not one shared index, matching how Lightroom's
  // own mask/history list names each tool instance by its own type.
  let chipLabels = $derived.by(() => {
    let gradientCount = 0;
    let radialCount = 0;
    let brushCount = 0;
    return new Map(
      masks.map((mask) => {
        if (mask.op === "radial_gradient_mask") return [mask.id, `Radial ${++radialCount}`];
        if (mask.op === "brush_mask") return [mask.id, `Brush ${++brushCount}`];
        return [mask.id, `Gradient ${++gradientCount}`];
      }),
    );
  });
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
  <button
    class="tool"
    class:active={activeTool === "radial_gradient"}
    type="button"
    disabled={atCap && activeTool !== "radial_gradient"}
    title={atCap ? `Maximum ${MAX_MASKS} masks reached` : "Radial Gradient"}
    onclick={() => onToolToggle("radial_gradient")}
  >Radial Gradient</button>
  <button
    class="tool"
    class:active={activeTool === "brush"}
    type="button"
    disabled={atCap && activeTool !== "brush"}
    title={atCap ? `Maximum ${MAX_MASKS} masks reached` : "Brush"}
    onclick={() => onToolToggle("brush")}
  >Brush</button>

  {#if activeTool === "brush"}
    <div class="divider"></div>
    <div class="brush-options">
      <label class="brush-option">
        <span>Size</span>
        <input
          type="range"
          min="0.01"
          max="0.3"
          step="0.005"
          value={brushSize}
          oninput={(e) => onBrushSizeChange(Number(e.currentTarget.value))}
        />
      </label>
      <label class="brush-option">
        <!-- Feather (shown) is the inverse of the stored `hardness` (0
             feather = a hard edge = hardness 100; 100 feather = fully
             soft = hardness 0) -- matches this app's existing gradient
             masks' own "Feather" naming/convention, kept consistent here
             even though the underlying Dab field is `hardness`. -->
        <span>Feather</span>
        <input
          type="range"
          min="0"
          max="100"
          step="1"
          value={100 - brushHardness}
          oninput={(e) => onBrushHardnessChange(100 - Number(e.currentTarget.value))}
        />
      </label>
      <label class="brush-option">
        <span>Flow</span>
        <input
          type="range"
          min="0.05"
          max="1"
          step="0.05"
          value={brushFlow}
          oninput={(e) => onBrushFlowChange(Number(e.currentTarget.value))}
        />
      </label>
      <button
        class="tool small"
        class:active={eraseMode}
        type="button"
        title="Erase (paint removes coverage instead of adding it)"
        onclick={onEraseToggle}
      >Erase</button>
      <button class="tool small" type="button" title="Start a new brush mask on the next stroke" onclick={onNewBrush}>New Brush</button>
    </div>
  {/if}

  {#if masks.length > 0}
    <div class="divider"></div>
    <div class="mask-list">
      {#each masks as mask (mask.id)}
        <button
          class="mask-chip"
          class:selected={mask.id === selectedMaskId}
          type="button"
          onclick={() => onMaskSelect(mask.id)}
        >{chipLabels.get(mask.id)}</button>
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
  .brush-options {
    display: flex;
    align-items: center;
    gap: 10px;
    flex: none;
  }
  .brush-option {
    display: flex;
    align-items: center;
    gap: 5px;
    cursor: default;
  }
  .brush-option span {
    font-size: 10.5px;
    color: var(--text-tertiary);
    flex: none;
  }
  .brush-option input[type="range"] {
    width: 60px;
    appearance: none;
    -webkit-appearance: none;
    height: 3px;
    background: var(--border-strong);
    border-radius: 2px;
    outline: none;
  }
  .brush-option input[type="range"]::-webkit-slider-thumb {
    -webkit-appearance: none;
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: var(--accent-strong);
    border: 2px solid var(--bg-panel);
    cursor: pointer;
  }
  .tool.small {
    padding: 3px 8px;
    font-size: 10.5px;
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
