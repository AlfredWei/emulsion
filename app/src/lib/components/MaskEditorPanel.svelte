<script>
  import { OVERLAY_CAPABLE_MASK_OPS } from "$lib/api/develop.js";

  /**
   * Floating panel for the currently-selected mask (M3 Slice 5) -- not a
   * `DevelopPanel.svelte` section, deliberately: every section there is a
   * static, singleton-per-image control, while masks are multi-instance
   * and geometrically anchored to the canvas. Fixed-position anchoring
   * (like `DevelopCanvas`'s own zoom-badge corner treatment), not tracked
   * per-mask on-canvas position -- avoids following a diagonal line's
   * location on screen.
   * @type {{
   *   mask: import('$lib/api/develop.js').Mask,
   *   onChange: (patch: Partial<import('$lib/api/develop.js').Mask>) => void,
   *   onDelete: () => void,
   *   onClose: () => void,
   *   showMaskOverlay: boolean,
   *   onShowOverlayChange: (value: boolean) => void,
   * }}
   */
  let { mask, onChange, onDelete, onClose, showMaskOverlay, onShowOverlayChange } = $props();

  // A REAL per-kind branch, not a free ride -- unlike linear vs. radial
  // (where every field below is common to both kinds), brush masks have
  // no mask-level `feather` (softness is baked per-dab at paint time from
  // the tool's own Feather/Size/Flow settings, see MaskToolStrip.svelte),
  // and luminance-range masks have a DIFFERENT `feather` meaning (a band
  // width around two edges, not one boundary) shown via its own Min/Max/
  // Feather block below -- so the shared Feather row is hidden for both.
  let title = $derived(
    mask.op === "radial_gradient_mask"
      ? "Radial Gradient"
      : mask.op === "brush_mask"
        ? "Brush"
        : mask.op === "luminance_range_mask"
          ? "Luminance Range"
          : "Linear Gradient",
  );
</script>

<div class="panel" role="dialog" aria-label="{title} adjustments">
  <div class="header">
    <span class="title">{title}</span>
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
  {#if mask.op !== "brush_mask" && mask.op !== "luminance_range_mask"}
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
  {/if}

  {#if mask.op === "luminance_range_mask"}
    <!-- Min/Max/Feather, not the shared Feather row above -- this kind's
         `feather` means a band WIDTH around two edges, a different
         concept from every other kind's single-boundary feather. The
         gradient-bar track background (below, in <style>) is a cheap
         stand-in for a real luminance histogram -- not pixel-accurate,
         but closes the "what does 30 even mean" gap a bare 0-100 slider
         would otherwise leave, without building histogram UI from scratch. -->
    <div class="row">
      <label for="mask-range-min">Min</label>
      <input
        id="mask-range-min"
        class="luma-slider"
        type="range"
        min="0"
        max="100"
        step="1"
        value={mask.rangeMin}
        oninput={(e) => onChange({ rangeMin: Number(e.currentTarget.value) })}
      />
      <span class="val">{mask.rangeMin}</span>
    </div>
    <div class="row">
      <label for="mask-range-max">Max</label>
      <input
        id="mask-range-max"
        class="luma-slider"
        type="range"
        min="0"
        max="100"
        step="1"
        value={mask.rangeMax}
        oninput={(e) => onChange({ rangeMax: Number(e.currentTarget.value) })}
      />
      <span class="val">{mask.rangeMax}</span>
    </div>
    <div class="row">
      <label for="mask-range-feather">Feather</label>
      <input
        id="mask-range-feather"
        type="range"
        min="0"
        max="100"
        step="1"
        value={mask.feather}
        oninput={(e) => onChange({ feather: Number(e.currentTarget.value) })}
      />
      <span class="val">{mask.feather}</span>
    </div>
  {/if}

  {#if OVERLAY_CAPABLE_MASK_OPS.includes(mask.op)}
    <!-- Soft colored overlay showing exactly what's selected -- brush and
         luminance-range masks are otherwise invisible until a nonzero
         adjustment is set (unlike linear/radial, which always show a
         dashed outline). Not a mask data field (unlike every row above,
         which funnels through onChange), so it gets its own separate prop
         pair, matching how onDelete/onClose are already separate from
         onChange in this same component. Also toggleable via the "O"
         hotkey while Develop is open and this mask is selected -- see
         +page.svelte. -->
    <label class="invert-row">
      <input type="checkbox" checked={showMaskOverlay} onchange={(e) => onShowOverlayChange(e.currentTarget.checked)} />
      <span>Show Overlay (O)</span>
    </label>
  {/if}

  <label class="invert-row">
    <input type="checkbox" checked={mask.invert} onchange={(e) => onChange({ invert: e.currentTarget.checked })} />
    <span>Invert</span>
  </label>

  <button class="delete" type="button" onclick={onDelete}>Delete {title}</button>
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
  /* Cheap stand-in for a real luminance histogram (see the luminance-range
     Min/Max markup above) -- a plain black-to-white track background so
     the slider's own position at least visually maps to "how bright". */
  .row input[type="range"].luma-slider {
    background: linear-gradient(to right, #000, #fff);
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
