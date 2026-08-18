<script>
  import { MAX_MASKS } from "$lib/api/develop.js";

  /**
   * Contextual tool strip above the Filmstrip in Develop, matching
   * UX-DESIGN.md's described layout ("a contextual tool strip above [the
   * filmstrip] for crop/mask/heal tools when active... designed into this
   * layout now"). Linear Gradient, Radial Gradient, Brush, Luminance
   * Range, and Color Range tools, plus a simple list of placed masks --
   * click-to-select is simpler and more robust here than hit-testing a
   * click near a diagonal line/ellipse/painted region/pixel-value on the
   * canvas. Luminance Range has no geometry, so it doesn't route through
   * `activeTool` at all -- `onCreateLuminanceRange` creates the mask
   * directly on click (see +page.svelte). Color Range DOES route through
   * `activeTool` (unlike Luminance Range) -- it genuinely needs a canvas
   * click's coordinates to sample a pixel color, so DevelopCanvas.svelte's
   * own pointer handlers create and commit the mask, same funnel as
   * linear/radial/brush.
   * @type {{
   *   activeTool: string | null,
   *   masks: import('$lib/api/develop.js').Mask[],
   *   selectedMaskId: string | null,
   *   brushSize: number,
   *   brushHardness: number,
   *   brushFlow: number,
   *   spotBrushSize: number,
   *   eraseMode: boolean,
   *   maskOverlaysVisible: boolean,
   *   onToolToggle: (tool: string) => void,
   *   onMaskSelect: (id: string) => void,
   *   onBrushSizeChange: (value: number) => void,
   *   onBrushHardnessChange: (value: number) => void,
   *   onBrushFlowChange: (value: number) => void,
   *   onSpotBrushSizeChange: (value: number) => void,
   *   onEraseToggle: () => void,
   *   onNewBrush: () => void,
   *   onNewSpot: () => void,
   *   onToggleMaskOverlaysVisible: () => void,
   *   onCreateLuminanceRange: () => void,
   *   crop: {x: number, y: number, width: number, height: number, angle: number},
   *   cropAspectLock: number | null,
   *   onCropAspectPreset: (ratio: number | null) => void,
   *   onCropAngleChange: (angle: number) => void,
   *   onCropReset: () => void,
   * }}
   */
  let {
    activeTool,
    masks,
    selectedMaskId,
    brushSize,
    brushHardness,
    brushFlow,
    spotBrushSize,
    eraseMode,
    maskOverlaysVisible,
    onToolToggle,
    onMaskSelect,
    onBrushSizeChange,
    onBrushHardnessChange,
    onBrushFlowChange,
    onSpotBrushSizeChange,
    onEraseToggle,
    onNewBrush,
    onNewSpot,
    onToggleMaskOverlaysVisible,
    onCreateLuminanceRange,
    crop,
    cropAspectLock,
    onCropAspectPreset,
    onCropAngleChange,
    onCropReset,
  } = $props();

  // Crop & Straighten (M3): a fixed set of common ratios -- "Original"
  // (locking to the source photo's own native aspect) is a real,
  // deliberately DEFERRED option, named here rather than silently
  // omitted: this component has no access to the source image's own
  // dimensions (DevelopCanvas.svelte tracks that internally), and
  // plumbing it through just for this one preset wasn't judged worth the
  // extra prop given the other six ratios already cover the common cases.
  const CROP_ASPECT_PRESETS = [
    { label: "Free", ratio: null },
    { label: "1:1", ratio: 1 },
    { label: "16:9", ratio: 16 / 9 },
    { label: "3:2", ratio: 3 / 2 },
    { label: "4:3", ratio: 4 / 3 },
    { label: "5:4", ratio: 5 / 4 },
  ];

  let atCap = $derived(masks.length >= MAX_MASKS);

  // Per-kind chip numbering ("Gradient 1", "Radial 1", "Brush 1", "Range
  // 1", "Color 1") -- five independent counters, not one shared index,
  // matching how Lightroom's own mask/history list names each tool
  // instance by its own type. Each kind gets its own explicit branch
  // before the linear-gradient fallback -- this file's own established
  // discipline (see DevelopCanvas.svelte's packing-loop comment) is that
  // an untyped fallback assuming "unrecognized = linear" is a real bug
  // class already shipped and fixed once, not a style nit.
  let chipLabels = $derived.by(() => {
    let gradientCount = 0;
    let radialCount = 0;
    let brushCount = 0;
    let rangeCount = 0;
    let colorCount = 0;
    let spotCount = 0;
    return new Map(
      masks.map((mask) => {
        if (mask.op === "radial_gradient_mask") return [mask.id, `Radial ${++radialCount}`];
        if (mask.op === "brush_mask") return [mask.id, `Brush ${++brushCount}`];
        if (mask.op === "luminance_range_mask") return [mask.id, `Range ${++rangeCount}`];
        if (mask.op === "color_range_mask") return [mask.id, `Color ${++colorCount}`];
        if (mask.op === "spot_mask") return [mask.id, `Spot ${++spotCount}`];
        return [mask.id, `Gradient ${++gradientCount}`];
      }),
    );
  });
</script>

<div class="strip">
  <!-- Icon buttons, not text -- the strip was running out of horizontal
       room as mask kinds accumulated (5 tools + brush options + a
       variable-length mask-chip list all share this one row). `title`
       (native tooltip) + `aria-label` (accessible name) together replace
       the visible label that used to carry both jobs at once. -->
  <button
    class="tool icon"
    class:active={activeTool === "crop"}
    type="button"
    title="Crop & Straighten"
    aria-label="Crop & Straighten"
    onclick={() => onToolToggle("crop")}
  >
    <svg viewBox="0 0 16 16" width="16" height="16" fill="none" aria-hidden="true">
      <path d="M4 1 V12 H15" stroke="currentColor" stroke-width="1.3" />
      <path d="M12 4 V15 H1" stroke="currentColor" stroke-width="1.3" />
    </svg>
  </button>
  <div class="divider"></div>
  <button
    class="tool icon"
    class:active={activeTool === "linear_gradient"}
    type="button"
    disabled={atCap && activeTool !== "linear_gradient"}
    title={atCap ? `Maximum ${MAX_MASKS} masks reached` : "Linear Gradient"}
    aria-label="Linear Gradient"
    onclick={() => onToolToggle("linear_gradient")}
  >
    <svg viewBox="0 0 16 16" width="16" height="16" fill="none" aria-hidden="true">
      <circle cx="3.2" cy="12.5" r="1.5" fill="currentColor" />
      <line x1="4" y1="11.5" x2="12" y2="3.5" stroke="currentColor" stroke-width="1.3" />
      <circle cx="12.8" cy="2.7" r="1.5" fill="currentColor" />
    </svg>
  </button>
  <button
    class="tool icon"
    class:active={activeTool === "radial_gradient"}
    type="button"
    disabled={atCap && activeTool !== "radial_gradient"}
    title={atCap ? `Maximum ${MAX_MASKS} masks reached` : "Radial Gradient"}
    aria-label="Radial Gradient"
    onclick={() => onToolToggle("radial_gradient")}
  >
    <svg viewBox="0 0 16 16" width="16" height="16" fill="none" aria-hidden="true">
      <circle cx="8" cy="8" r="6" stroke="currentColor" stroke-width="1" stroke-dasharray="2 2" />
      <circle cx="8" cy="8" r="2.6" stroke="currentColor" stroke-width="1.4" />
    </svg>
  </button>
  <button
    class="tool icon"
    class:active={activeTool === "brush"}
    type="button"
    disabled={atCap && activeTool !== "brush"}
    title={atCap ? `Maximum ${MAX_MASKS} masks reached` : "Brush"}
    aria-label="Brush"
    onclick={() => onToolToggle("brush")}
  >
    <svg viewBox="0 0 16 16" width="16" height="16" fill="none" aria-hidden="true">
      <rect x="9" y="1" width="2.6" height="8" rx="1.1" stroke="currentColor" stroke-width="1" transform="rotate(40 10.3 5)" />
      <circle cx="4" cy="12.6" r="1.8" fill="currentColor" />
    </svg>
  </button>
  <button
    class="tool icon"
    type="button"
    disabled={atCap}
    title={atCap ? `Maximum ${MAX_MASKS} masks reached` : "Luminance Range -- creates immediately, no canvas interaction needed (this mask has no geometry, real Lightroom's own behavior for this kind)"}
    aria-label="Luminance Range"
    onclick={onCreateLuminanceRange}
  >
    <svg viewBox="0 0 16 16" width="16" height="16" fill="none" aria-hidden="true">
      <defs>
        <linearGradient id="mts-luma-grad" x1="0" y1="0" x2="1" y2="0">
          <stop offset="0" stop-color="#000" />
          <stop offset="1" stop-color="#fff" />
        </linearGradient>
      </defs>
      <rect x="1.5" y="6.2" width="13" height="3.6" rx="1" fill="url(#mts-luma-grad)" stroke="currentColor" stroke-width="0.75" />
      <line x1="6" y1="4.3" x2="6" y2="11.7" stroke="currentColor" stroke-width="1.1" />
      <line x1="10" y1="4.3" x2="10" y2="11.7" stroke="currentColor" stroke-width="1.1" />
    </svg>
  </button>
  <button
    class="tool icon"
    class:active={activeTool === "color_range"}
    type="button"
    disabled={atCap && activeTool !== "color_range"}
    title={atCap ? `Maximum ${MAX_MASKS} masks reached` : "Color Range -- click a point on the image to sample its color and create a mask selecting similar colors"}
    aria-label="Color Range"
    onclick={() => onToolToggle("color_range")}
  >
    <svg viewBox="0 0 16 16" width="16" height="16" fill="none" aria-hidden="true">
      <line x1="14" y1="2.3" x2="10.7" y2="5.6" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" />
      <line x1="10.5" y1="5.4" x2="4.3" y2="11.6" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" />
      <path d="M4.3 11.6 L3 14 L5.4 12.7 Z" fill="currentColor" />
    </svg>
  </button>
  <button
    class="tool icon"
    class:active={activeTool === "spot"}
    type="button"
    disabled={atCap && activeTool !== "spot"}
    title={atCap ? `Maximum ${MAX_MASKS} masks reached` : "Spot Removal (Healing/Clone) -- click-drag to paint over a blemish"}
    aria-label="Spot Removal"
    onclick={() => onToolToggle("spot")}
  >
    <svg viewBox="0 0 16 16" width="16" height="16" fill="none" aria-hidden="true">
      <circle cx="9.5" cy="6.5" r="4" stroke="currentColor" stroke-width="1.3" />
      <circle cx="3.6" cy="12.4" r="1.6" stroke="currentColor" stroke-width="1.1" stroke-dasharray="1.6 1.4" />
    </svg>
  </button>

  <div class="divider"></div>
  <!-- Always visible, not gated by activeTool -- toggling it is a review
       action independent of which mask tool (if any) is currently active,
       so it needs to stay reachable the whole time. Mirrors the H hotkey
       wired in +page.svelte. -->
  <button
    class="tool icon"
    class:active={!maskOverlaysVisible}
    type="button"
    title={maskOverlaysVisible ? "Hide mask overlays (H) -- so edit pins/handles don't block reviewing the result" : "Show mask overlays (H)"}
    aria-label={maskOverlaysVisible ? "Hide mask overlays" : "Show mask overlays"}
    onclick={onToggleMaskOverlaysVisible}
  >
    {#if maskOverlaysVisible}
      <svg viewBox="0 0 16 16" width="16" height="16" fill="none" aria-hidden="true">
        <path d="M1 8 C3 4, 13 4, 15 8 C13 12, 3 12, 1 8 Z" stroke="currentColor" stroke-width="1.3" />
        <circle cx="8" cy="8" r="2" stroke="currentColor" stroke-width="1.3" />
      </svg>
    {:else}
      <svg viewBox="0 0 16 16" width="16" height="16" fill="none" aria-hidden="true">
        <path d="M1 8 C3 4, 13 4, 15 8 C13 12, 3 12, 1 8 Z" stroke="currentColor" stroke-width="1.3" />
        <circle cx="8" cy="8" r="2" stroke="currentColor" stroke-width="1.3" />
        <line x1="2" y1="14" x2="14" y2="2" stroke="currentColor" stroke-width="1.3" />
      </svg>
    {/if}
  </button>

  {#if activeTool === "crop"}
    <div class="divider"></div>
    <div class="brush-options">
      {#each CROP_ASPECT_PRESETS as preset (preset.label)}
        <button
          class="tool small"
          class:active={preset.ratio === cropAspectLock}
          type="button"
          title="Aspect ratio: {preset.label}"
          onclick={() => onCropAspectPreset(preset.ratio)}
        >{preset.label}</button>
      {/each}
      <label class="brush-option">
        <span>Angle</span>
        <input
          type="range"
          min="-45"
          max="45"
          step="0.1"
          value={crop.angle}
          oninput={(e) => onCropAngleChange(Number(e.currentTarget.value))}
        />
      </label>
      <span class="crop-angle-value">{crop.angle.toFixed(1)}°</span>
      <button class="tool small" type="button" title="Reset crop to full frame" onclick={onCropReset}>Reset</button>
      <!-- Crop is already persisted live as it's dragged (same debounced
           edit-stack write every other op uses) -- this button doesn't
           "save" anything that wasn't already saved. What it actually
           does is deselect the tool, which is what reveals the committed,
           rotated/clipped CSS preview (see DevelopCanvas.svelte's own
           showCommittedCropPreview doc comment) -- named "Done" rather
           than left implicit (re-clicking the crop tool icon toggles the
           same way) because a user reported not knowing how to "apply"
           the crop at all. -->
      <button class="tool small done" type="button" title="Done editing crop" onclick={() => onToolToggle("crop")}>Done</button>
    </div>
  {/if}

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

  {#if activeTool === "spot"}
    <!-- M4 Slice 2 (brush-like spot removal): just a Size slider + "New
         Spot" -- no Hardness/Flow, since a spot mask's edge softness is a
         single mask-level Feather (edited in MaskEditorPanel, same as
         before) rather than baked in per-dab, and spot removal has no
         "build up opacity" flow concept the way adjustment brushing does.
         Mirrors the Brush block's own layout above. -->
    <div class="divider"></div>
    <div class="brush-options">
      <label class="brush-option">
        <span>Size</span>
        <input
          type="range"
          min="0.005"
          max="0.15"
          step="0.0025"
          value={spotBrushSize}
          oninput={(e) => onSpotBrushSizeChange(Number(e.currentTarget.value))}
        />
      </label>
      <button class="tool small" type="button" title="Start a new spot mask on the next stroke" onclick={onNewSpot}>New Spot</button>
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
  .tool.icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 26px;
    padding: 0;
  }
  .tool.icon svg {
    display: block;
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
  .crop-angle-value {
    font-family: var(--font-mono);
    font-variant-numeric: tabular-nums;
    font-size: 10.5px;
    color: var(--text-tertiary);
    flex: none;
    min-width: 34px;
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
  .tool.small.done {
    margin-left: auto;
    background: var(--accent-strong);
    color: var(--bg-panel);
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
