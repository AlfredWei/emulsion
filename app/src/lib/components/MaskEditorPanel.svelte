<script>
  /**
   * Floating panel for the currently-selected mask (M3 Slice 5) -- not a
   * `DevelopPanel.svelte` section, deliberately: every section there is a
   * static, singleton-per-image control, while masks are multi-instance
   * and geometrically anchored to the canvas. Fixed-position anchoring
   * (like `DevelopCanvas`'s own zoom-badge corner treatment), not tracked
   * per-mask on-canvas position -- avoids following a diagonal line's
   * location on screen.
   * @type {{
   *   mask: import('$lib/api/develop.js').LinearGradientMask,
   *   onChange: (patch: Partial<import('$lib/api/develop.js').LinearGradientMask>) => void,
   *   onDelete: () => void,
   *   onClose: () => void,
   * }}
   */
  let { mask, onChange, onDelete, onClose } = $props();
</script>

<div class="panel" role="dialog" aria-label="Gradient adjustments">
  <div class="header">
    <span class="title">Linear Gradient</span>
    <button class="close" type="button" title="Deselect" onclick={onClose}>×</button>
  </div>

  <div class="row">
    <label for="mask-exposure">Exposure</label>
    <input
      id="mask-exposure"
      type="range"
      min="-5"
      max="5"
      step="0.05"
      value={mask.exposure}
      oninput={(e) => onChange({ exposure: Number(e.currentTarget.value) })}
    />
    <span class="val">{mask.exposure >= 0 ? "+" : ""}{mask.exposure.toFixed(2)}</span>
  </div>
  <div class="row">
    <label for="mask-contrast">Contrast</label>
    <input
      id="mask-contrast"
      type="range"
      min="-100"
      max="100"
      step="1"
      value={mask.contrast}
      oninput={(e) => onChange({ contrast: Number(e.currentTarget.value) })}
    />
    <span class="val">{mask.contrast >= 0 ? "+" : ""}{mask.contrast}</span>
  </div>
  <div class="row">
    <label for="mask-saturation">Saturation</label>
    <input
      id="mask-saturation"
      type="range"
      min="-100"
      max="100"
      step="1"
      value={mask.saturation}
      oninput={(e) => onChange({ saturation: Number(e.currentTarget.value) })}
    />
    <span class="val">{mask.saturation >= 0 ? "+" : ""}{mask.saturation}</span>
  </div>
  <div class="row">
    <label for="mask-feather">Feather</label>
    <input
      id="mask-feather"
      type="range"
      min="0"
      max="100"
      step="1"
      value={mask.feather}
      oninput={(e) => onChange({ feather: Number(e.currentTarget.value) })}
    />
    <span class="val">{mask.feather}</span>
  </div>

  <label class="invert-row">
    <input type="checkbox" checked={mask.invert} onchange={(e) => onChange({ invert: e.currentTarget.checked })} />
    <span>Invert</span>
  </label>

  <button class="delete" type="button" onclick={onDelete}>Delete Gradient</button>
</div>

<style>
  .panel {
    position: absolute;
    top: 14px;
    left: 14px;
    width: 210px;
    z-index: 2;
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px;
    background: var(--bg-panel);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-m);
    box-shadow: var(--shadow-soft);
  }
  .header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .title {
    font-size: 11.5px;
    font-weight: 600;
    color: var(--text-primary);
  }
  .close {
    all: unset;
    cursor: pointer;
    padding: 0 4px;
    color: var(--text-tertiary);
  }
  .close:hover {
    color: var(--text-primary);
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .row label {
    width: 62px;
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
    width: 38px;
    text-align: right;
    font-family: var(--font-mono);
    font-variant-numeric: tabular-nums;
    font-size: 10.5px;
    color: var(--text-tertiary);
  }
  .invert-row {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    color: var(--text-secondary);
    cursor: pointer;
  }
  .delete {
    all: unset;
    cursor: pointer;
    text-align: center;
    padding: 6px;
    margin-top: 2px;
    font-size: 11px;
    font-weight: 600;
    border-radius: var(--radius-s);
    color: var(--label-red);
    border: 1px solid var(--border-strong);
  }
  .delete:hover {
    border-color: var(--label-red);
  }
</style>
