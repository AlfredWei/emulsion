<script>
  import { OVERLAY_CAPABLE_MASK_OPS, MAX_MASKS, MAX_MODIFIERS, COMBINE_MODES, isModifiableOp, listModifiers } from "$lib/api/develop.js";
  import { nudgeValue } from "$lib/stepMath.js";

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
   *   isResamplingColor: boolean,
   *   onResampleColor: () => void,
   *   selectedShapeId?: string | null,
   *   onSelectShape?: (id: string | null) => void,
   *   onShapeChange?: (id: string, patch: Record<string, unknown>) => void,
   *   onShapeCombine?: (id: string, combine: import('$lib/api/develop.js').Combine) => void,
   *   onRemoveShape?: (id: string) => void,
   *   onAddShape?: (combine: import('$lib/api/develop.js').Combine, shapeOp: any) => void,
   *   shapeBlockedReason?: (shapeOp: string) => string | null,
   *   armedShape?: { combine: import('$lib/api/develop.js').Combine, tool: string } | null,
   *   modifierCount?: number,
   *   brushLayers?: number,
   *   onRefineSubject?: () => void,
   * }}
   */
  let {
    mask,
    onChange,
    onDelete,
    onClose,
    showMaskOverlay,
    onShowOverlayChange,
    isResamplingColor,
    onResampleColor,
    selectedShapeId = null,
    onSelectShape = () => {},
    onShapeChange = () => {},
    onShapeCombine = () => {},
    onRemoveShape = () => {},
    onAddShape = () => {},
    shapeBlockedReason = () => null,
    armedShape = null,
    modifierCount = 0,
    brushLayers = 0,
    onRefineSubject = () => {},
  } = $props();

  // Composable masking (RFC-0025): the mask's adjustments always live on the mask itself; the
  // shape controls below (Feather, ranges, colour, Invert) edit the SELECTED shape -- the base
  // shape (the mask's own fields) or one of its modifiers.
  const SHAPE_NAMES = /** @type {Record<string, string>} */ ({
    linear_gradient_mask: "Linear gradient",
    radial_gradient_mask: "Radial",
    brush_mask: "Brush",
    luminance_range_mask: "Luminance range",
    color_range_mask: "Colour range",
    segment_mask: "Subject",
  });
  const SHAPE_ICONS = /** @type {Record<string, string>} */ ({
    linear_gradient_mask: "▭",
    radial_gradient_mask: "◔",
    brush_mask: "✎",
    luminance_range_mask: "◐",
    color_range_mask: "◍",
    segment_mask: "◉",
  });
  const COMBINE_LABELS = /** @type {Record<string, string>} */ ({ add: "+ Add", subtract: "− Subtract", intersect: "∩ Intersect" });
  const ARM_HINTS = /** @type {Record<string, string>} */ ({
    linear_gradient: "Drag on the image to place the gradient.",
    radial_gradient: "Drag on the image to place the ellipse.",
    brush: "Paint on the image to draw the shape.",
    color_range: "Click a colour on the image.",
    segment: "Click the subject on the image (Alt-click removes). Enter keeps it.",
  });
  let shapes = $derived(listModifiers(mask));
  let editShape = $derived(shapes.find((x) => x.id === selectedShapeId) ?? null);
  /** Fields of the shape being edited (the mask itself for the base shape). */
  let edit = $derived(/** @type {any} */ (editShape ? editShape.shape : mask));
  /** @param {Record<string, unknown>} patch */
  function onEdit(patch) {
    if (editShape) onShapeChange(editShape.id, patch);
    else onChange(patch);
  }
  let menuOpen = $state(false);
  /** @type {HTMLButtonElement | null} */
  let addBtn = $state(null);
  let addMode = $state(/** @type {import('$lib/api/develop.js').Combine} */ ("subtract"));
  const SHAPE_ORDER = ["radial_gradient_mask", "linear_gradient_mask", "brush_mask", "luminance_range_mask", "color_range_mask", "segment_mask"];

  // A REAL per-kind branch, not a free ride -- unlike linear vs. radial
  // (where every field below is common to both kinds), brush masks have
  // no mask-level `feather` (softness is baked per-dab at paint time from
  // the tool's own Feather/Size/Flow settings, see MaskToolStrip.svelte),
  // and luminance-range/color-range masks have a DIFFERENT `feather`
  // meaning (a band width around two edges, or a transition band beyond a
  // color-distance threshold, not one boundary) shown via their own
  // dedicated blocks below -- so the shared Feather row is hidden for all
  // three. Every kind gets its own explicit branch before the linear
  // fallback (not appended after it) -- this file's own established
  // discipline: an untyped fallback assuming "unrecognized = linear" is a
  // real bug class already shipped and fixed once elsewhere in this
  // codebase (DevelopCanvas.svelte's mask-packing loop).
  let title = $derived(
    mask.op === "radial_gradient_mask"
      ? "Radial Gradient"
      : mask.op === "brush_mask"
        ? "Brush"
        : mask.op === "luminance_range_mask"
          ? "Luminance Range"
          : mask.op === "color_range_mask"
            ? "Color Range"
            : mask.op === "segment_mask"
              ? "Subject"
              : mask.op === "spot_mask"
                ? "Spot Removal"
                : mask.op === "red_eye_mask"
                  ? "Red Eye Correction"
                  : "Linear Gradient",
  );
</script>

<!-- Step-nudge (up/down) buttons, same as DevelopPanel.svelte's own
     snippet of the same name (M4.5 Slice 7) -- duplicated rather than
     shared across the two components, since they don't otherwise share
     any markup or a parent module. -->
{#snippet stepButtons(
  /** @type {number} */ value,
  /** @type {number} */ step,
  /** @type {number} */ min,
  /** @type {number} */ max,
  /** @type {(value: number) => void} */ onChange,
)}
  <div class="step-buttons">
    <button
      type="button"
      class="step-btn"
      tabindex="-1"
      aria-label="Increase"
      disabled={value >= max}
      onclick={() => onChange(nudgeValue(value, 1, step, min, max))}
    >▲</button>
    <button
      type="button"
      class="step-btn"
      tabindex="-1"
      aria-label="Decrease"
      disabled={value <= min}
      onclick={() => onChange(nudgeValue(value, -1, step, min, max))}
    >▼</button>
  </div>
{/snippet}

<div class="panel" role="dialog" aria-label="{title} adjustments">
  <div class="header">
    <span class="title">{title}</span>
    <button class="close" type="button" title="Deselect" onclick={onClose}>×</button>
  </div>

  {#if mask.op === "spot_mask"}
    <!-- No Exposure/Contrast/Saturation for a spot mask -- it copies pixel
         CONTENT, it doesn't gate a parametric adjustment (see SpotMask's
         own doc comment in develop.js). Mode replaces those rows instead:
         Clone is a direct offset sample, Heal additionally shifts the
         sampled patch to match the destination area's local mean color
         (a simplified stand-in for true seamless blending, computed
         server-side in develop_engine.rs's compute_heal_shift). -->
    <div class="row">
      <label for="mask-spot-mode">Mode</label>
      <div class="mode-toggle" id="mask-spot-mode">
        <button
          type="button"
          class:active={mask.mode === "heal"}
          onclick={() => onChange({ mode: "heal" })}
        >Heal</button>
        <button
          type="button"
          class:active={mask.mode === "clone"}
          onclick={() => onChange({ mode: "clone" })}
        >Clone</button>
      </div>
    </div>
  {:else if mask.op !== "red_eye_mask"}
    <!-- Red eye also skips this shared block -- like spot, it has no
         exposure/contrast/saturation channel (see RedEyeMask's own JSDoc
         typedef in develop.js); its own Pupil Size/Darken controls live in
         a dedicated block below, same pattern as luminance/color range's
         own dedicated blocks. -->
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
      {@render stepButtons(mask.exposure, 0.05, -5, 5, (v) => onChange({ exposure: v }))}
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
      {@render stepButtons(mask.contrast, 1, -100, 100, (v) => onChange({ contrast: v }))}
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
      {@render stepButtons(mask.saturation, 1, -100, 100, (v) => onChange({ saturation: v }))}
    </div>
  {/if}
  {#if isModifiableOp(mask.op)}
    <!-- Composable masking (RFC-0025): what selects the pixels. The base shape first, then each
         modifier in evaluation order; the rows pick which shape the controls below edit. -->
    <div class="sep"></div>
    <div class="sec"><span>Shapes</span><span class="n">{modifierCount} / {MAX_MODIFIERS} · brush {brushLayers} / {MAX_MASKS}</span></div>
    <div class="shapes">
      <div class="shape" class:sel={editShape === null}>
        <button type="button" class="pick" onclick={() => onSelectShape(null)}>
          <span class="ico">{SHAPE_ICONS[mask.op]}</span><span class="nm">{SHAPE_NAMES[mask.op]}</span>
        </button>
        <span class="base">base</span>
      </div>
      {#each shapes as shape (shape.id)}
        <div class="shape" class:sel={shape.id === selectedShapeId}>
          <button type="button" class="pick" onclick={() => onSelectShape(shape.id)}>
            <span class="ico">{SHAPE_ICONS[shape.shape.op]}</span><span class="nm">{SHAPE_NAMES[shape.shape.op]}</span>
          </button>
          <select
            class="combine {shape.combine}"
            aria-label="Combine mode"
            value={shape.combine}
            onchange={(e) => onShapeCombine(shape.id, /** @type {any} */ (e.currentTarget.value))}
          >
            {#each COMBINE_MODES as m (m)}<option value={m}>{COMBINE_LABELS[m]}</option>{/each}
          </select>
          <button type="button" class="x" title="Remove shape" aria-label="Remove shape" onclick={() => {
              onRemoveShape(shape.id);
              // The row (and this button) unmount; keep keyboard focus inside the panel.
              addBtn?.focus();
            }}>×</button>
        </div>
      {/each}
    </div>
    {#if armedShape}
      <div class="hint">{COMBINE_LABELS[armedShape.combine]}: {ARM_HINTS[armedShape.tool] ?? ""}</div>
    {/if}
    <!-- Esc closes the menu and returns focus to its button; tabbing out of it closes it too. -->
    <div
      class="addbtn"
      role="presentation"
      onkeydown={(e) => {
        if (e.key === "Escape" && menuOpen) {
          e.stopPropagation();
          menuOpen = false;
          /** @type {HTMLElement | null} */ (e.currentTarget.querySelector(".add"))?.focus();
        }
      }}
      onfocusout={(e) => {
        if (menuOpen && !e.currentTarget.contains(/** @type {Node | null} */ (e.relatedTarget))) menuOpen = false;
      }}
    >
      <button type="button" class="add" bind:this={addBtn} aria-expanded={menuOpen} onclick={() => (menuOpen = !menuOpen)}>＋ Add shape ▾</button>
      {#if menuOpen}
        <div class="menu" role="menu">
          <div class="seg">
            {#each COMBINE_MODES as m (m)}
              <button type="button" class:on={addMode === m} onclick={() => (addMode = m)}>{COMBINE_LABELS[m]}</button>
            {/each}
          </div>
          {#each SHAPE_ORDER as op (op)}
            {@const why = shapeBlockedReason(op)}
            <button
              type="button"
              role="menuitem"
              class="opt"
              disabled={why !== null}
              title={why ?? ""}
              onclick={() => {
                menuOpen = false;
                // The menu unmounts with focus inside it; hand focus back so keyboard users keep their place.
                addBtn?.focus();
                onAddShape(addMode, op);
              }}
            >{SHAPE_NAMES[op]}{#if why}<span class="why">{why}</span>{/if}</button>
          {/each}
        </div>
      {/if}
    </div>
    {#if shapes.length > 0}
      <div class="sec"><span>Editing: {SHAPE_NAMES[edit.op]}</span></div>
    {/if}
  {/if}
  {#if edit.op !== "brush_mask" && edit.op !== "luminance_range_mask" && edit.op !== "color_range_mask"}
    <div class="row">
      <label for="mask-feather">Feather</label>
      <input
        id="mask-feather"
        type="range"
        min="0"
        max="100"
        step="1"
        value={edit.feather}
        oninput={(e) => onEdit({ feather: Number(e.currentTarget.value) })}
      />
      <span class="val">{edit.feather}</span>
      {@render stepButtons(edit.feather, 1, 0, 100, (v) => onEdit({ feather: v }))}
    </div>
  {/if}
  {#if edit.op === "segment_mask"}
    <!-- Grow/Shrink moves the selection edge outwards / inwards without touching the stored
         logits (RFC-0026 §3.2); Feather above widens the edge ramp. -->
    <div class="row">
      <label for="mask-grow">Grow</label>
      <input
        id="mask-grow"
        type="range"
        min="-100"
        max="100"
        step="1"
        value={edit.grow}
        oninput={(e) => onEdit({ grow: Number(e.currentTarget.value) })}
      />
      <span class="val">{edit.grow >= 0 ? "+" : ""}{edit.grow}</span>
      {@render stepButtons(edit.grow, 1, -100, 100, (v) => onEdit({ grow: v }))}
    </div>
    {#if Array.isArray(edit.prompts) && edit.prompts.length > 0}
      <!-- Resumes the saved clicks: the next click adds to / removes from this selection. -->
      <button type="button" class="refine" title="Click the image to add to or remove from this selection" onclick={onRefineSubject}>Refine with clicks…</button>
    {/if}
  {/if}

  {#if edit.op === "luminance_range_mask"}
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
        value={edit.rangeMin}
        oninput={(e) => onEdit({ rangeMin: Number(e.currentTarget.value) })}
      />
      <span class="val">{edit.rangeMin}</span>
      {@render stepButtons(edit.rangeMin, 1, 0, 100, (v) => onEdit({ rangeMin: v }))}
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
        value={edit.rangeMax}
        oninput={(e) => onEdit({ rangeMax: Number(e.currentTarget.value) })}
      />
      <span class="val">{edit.rangeMax}</span>
      {@render stepButtons(edit.rangeMax, 1, 0, 100, (v) => onEdit({ rangeMax: v }))}
    </div>
    <div class="row">
      <label for="mask-range-feather">Feather</label>
      <input
        id="mask-range-feather"
        type="range"
        min="0"
        max="100"
        step="1"
        value={edit.feather}
        oninput={(e) => onEdit({ feather: Number(e.currentTarget.value) })}
      />
      <span class="val">{edit.feather}</span>
      {@render stepButtons(edit.feather, 1, 0, 100, (v) => onEdit({ feather: v }))}
    </div>
  {/if}

  {#if edit.op === "color_range_mask"}
    <!-- Swatch + Range/Feather, not the shared Feather row above -- same
         reasoning as luminance range's own dedicated block: this kind's
         `feather` means a transition band beyond a color-distance
         threshold, not one boundary. The swatch itself stays a
         non-interactive preview; the eyedropper button next to it re-uses
         DevelopCanvas's existing click-to-sample gesture (the same one
         that creates a NEW color-range mask) but targets THIS mask's
         `refColor` instead -- see +page.svelte's colorRangeResampleTarget
         wiring. -->
    <div class="row">
      <label for="mask-color-swatch">Color</label>
      <div
        id="mask-color-swatch"
        class="color-swatch"
        style="background: rgb({Math.round(edit.refColor.r * 255)}, {Math.round(edit.refColor.g * 255)}, {Math.round(edit.refColor.b * 255)})"
      ></div>
      <button
        class="resample"
        class:active={isResamplingColor}
        type="button"
        title={isResamplingColor ? "Click a point on the image to sample its color" : "Re-sample color"}
        aria-label="Re-sample color"
        onclick={onResampleColor}
      >
        <svg viewBox="0 0 16 16" width="14" height="14" fill="none" aria-hidden="true">
          <line x1="14" y1="2.3" x2="10.7" y2="5.6" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" />
          <line x1="10.5" y1="5.4" x2="4.3" y2="11.6" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" />
          <path d="M4.3 11.6 L3 14 L5.4 12.7 Z" fill="currentColor" />
        </svg>
      </button>
    </div>
    <div class="row">
      <label for="mask-color-range">Range</label>
      <input
        id="mask-color-range"
        type="range"
        min="0"
        max="100"
        step="1"
        value={edit.range}
        oninput={(e) => onEdit({ range: Number(e.currentTarget.value) })}
      />
      <span class="val">{edit.range}</span>
      {@render stepButtons(edit.range, 1, 0, 100, (v) => onEdit({ range: v }))}
    </div>
    <div class="row">
      <label for="mask-color-feather">Feather</label>
      <input
        id="mask-color-feather"
        type="range"
        min="0"
        max="100"
        step="1"
        value={edit.feather}
        oninput={(e) => onEdit({ feather: Number(e.currentTarget.value) })}
      />
      <span class="val">{edit.feather}</span>
      {@render stepButtons(edit.feather, 1, 0, 100, (v) => onEdit({ feather: v }))}
    </div>
  {/if}

  {#if mask.op === "red_eye_mask"}
    <!-- Pupil Size/Darken, not the shared Exposure/Contrast/Saturation
         block above (skipped for this kind, see its own comment there) --
         this kind's effect is fixed (redness-selective desaturate+darken),
         these two sliders are its only tunable parameters. -->
    <div class="row">
      <label for="mask-red-eye-pupil-size">Pupil Size</label>
      <input
        id="mask-red-eye-pupil-size"
        type="range"
        min="0"
        max="100"
        step="1"
        value={mask.pupilSize}
        oninput={(e) => onChange({ pupilSize: Number(e.currentTarget.value) })}
      />
      <span class="val">{mask.pupilSize}</span>
      {@render stepButtons(mask.pupilSize, 1, 0, 100, (v) => onChange({ pupilSize: v }))}
    </div>
    <div class="row">
      <label for="mask-red-eye-darken">Darken</label>
      <input
        id="mask-red-eye-darken"
        type="range"
        min="0"
        max="100"
        step="1"
        value={mask.darken}
        oninput={(e) => onChange({ darken: Number(e.currentTarget.value) })}
      />
      <span class="val">{mask.darken}</span>
      {@render stepButtons(mask.darken, 1, 0, 100, (v) => onChange({ darken: v }))}
    </div>
  {/if}

  {#if OVERLAY_CAPABLE_MASK_OPS.includes(mask.op) || shapes.length > 0}
    <!-- Soft colored overlay showing exactly what's selected -- brush,
         luminance-range, and color-range masks are otherwise invisible
         until a nonzero adjustment is set (unlike linear/radial, which
         always show a dashed outline). Not a mask data field (unlike every row above,
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

  {#if mask.op !== "spot_mask" && mask.op !== "red_eye_mask"}
    <!-- No Invert for a spot mask: replacing pixel content everywhere
         EXCEPT a small circle would never be a sensible spot-removal
         operation (see spot_mask_weight's own doc comment in
         develop_engine.rs), unlike every other kind's region-gated
         adjustment, which inverting meaningfully flips. Red eye has no
         `invert` field at all (see RedEyeMask's own JSDoc typedef) --
         correcting everywhere EXCEPT the oval is nonsensical for the same
         reason. -->
    <label class="invert-row">
      <input type="checkbox" checked={edit.invert} onchange={(e) => onEdit({ invert: e.currentTarget.checked })} />
      <span>Invert</span>
    </label>
  {/if}

  <button class="delete" type="button" onclick={onDelete}>Delete {title}</button>
</div>

<style>
  .panel {
    position: absolute;
    top: 14px;
    left: 14px;
    width: 210px;
    /* 16 shapes make the panel taller than a laptop-height canvas: scroll inside it instead of
       overflowing past the canvas. */
    max-height: calc(100% - 28px);
    overflow-y: auto;
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
  /* The sampled reference color for a color-range mask -- non-interactive,
     see the color-range block's own comment for why re-sampling isn't
     supported here. */
  .color-swatch {
    flex: 1;
    height: 16px;
    border-radius: var(--radius-s);
    border: 1px solid var(--border-strong);
  }
  .resample {
    all: unset;
    cursor: pointer;
    flex: none;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 20px;
    color: var(--text-tertiary);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-s);
  }
  .resample:hover {
    color: var(--text-primary);
  }
  .resample.active {
    color: var(--accent-strong);
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .mode-toggle {
    flex: 1;
    display: flex;
    gap: 4px;
  }
  .mode-toggle button {
    all: unset;
    flex: 1;
    text-align: center;
    cursor: pointer;
    padding: 4px 0;
    font-size: 11px;
    color: var(--text-secondary);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-s);
  }
  .mode-toggle button.active {
    color: var(--accent-strong);
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .row .val {
    width: 32px;
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
    color: var(--text-tertiary);
    opacity: 0.3;
    cursor: default;
  }
  .refine {
    all: unset;
    cursor: pointer;
    align-self: flex-start;
    margin: 0 0 6px;
    padding: 3px 8px;
    font-size: 11px;
    color: var(--text-secondary);
    border: 1px solid var(--border-strong);
    border-radius: 6px;
  }
  .refine:hover {
    color: var(--text-primary);
    border-color: var(--accent);
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
  .sep {
    height: 1px;
    background: var(--border-subtle);
    margin: 0 -2px;
  }
  .sec {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    font-size: 10px;
    letter-spacing: 0.05em;
    text-transform: uppercase;
    color: var(--text-tertiary);
    font-weight: 600;
  }
  .sec .n {
    font-family: var(--font-mono);
    letter-spacing: 0;
    text-transform: none;
    font-weight: 400;
  }
  .shapes {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin: 0 -4px;
  }
  .shape {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 2px 4px;
    border-radius: var(--radius-s);
    border: 1px solid transparent;
    font-size: 11px;
  }
  .shape:hover {
    background: var(--bg-hover);
  }
  .shape.sel {
    background: var(--accent-soft);
    border-color: var(--accent);
  }
  .shape .pick {
    all: unset;
    display: flex;
    align-items: center;
    gap: 5px;
    flex: 1;
    min-width: 0;
    cursor: pointer;
    padding: 2px 0;
  }
  .shape .pick:focus-visible {
    outline: 1px solid var(--accent);
  }
  .shape .ico {
    width: 16px;
    text-align: center;
    color: var(--text-tertiary);
    font-size: 10px;
  }
  .shape .nm {
    flex: 1;
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .shape .base {
    font-size: 10px;
    color: var(--text-tertiary);
  }
  .shape .combine {
    appearance: none;
    background: var(--bg-panel-raised);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-s);
    color: var(--text-primary);
    font: 10px var(--font-ui);
    padding: 1px 4px;
    cursor: pointer;
  }
  .shape .combine.add {
    color: #8fd19e;
  }
  .shape .combine.subtract {
    color: #e88;
  }
  .shape .combine.intersect {
    color: #8ab4f0;
  }
  .shape .x {
    all: unset;
    cursor: pointer;
    color: var(--text-tertiary);
    padding: 0 3px;
    font-size: 12px;
  }
  .shape .x:hover {
    color: var(--text-primary);
  }
  .hint {
    font-size: 10px;
    line-height: 1.35;
    color: var(--text-tertiary);
  }
  .addbtn {
    position: relative;
  }
  .addbtn .add {
    width: 100%;
    appearance: none;
    background: var(--bg-panel-raised);
    border: 1px dashed var(--border-strong);
    border-radius: var(--radius-s);
    color: var(--text-secondary);
    font: 11px var(--font-ui);
    padding: 5px;
    cursor: pointer;
  }
  .addbtn .add:hover {
    color: var(--accent-strong);
    border-color: var(--accent);
  }
  .menu {
    position: absolute;
    left: 0;
    right: 0;
    top: 30px;
    background: var(--bg-panel-raised);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-m);
    box-shadow: var(--shadow-soft);
    padding: 4px 0;
    z-index: 5;
    font-size: 11px;
    display: flex;
    flex-direction: column;
  }
  .menu .seg {
    display: flex;
    gap: 3px;
    padding: 4px 6px 6px;
    border-bottom: 1px solid var(--border-subtle);
    margin-bottom: 3px;
  }
  .menu .seg button {
    flex: 1;
    appearance: none;
    background: var(--bg-panel);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-s);
    color: var(--text-secondary);
    font: 10px var(--font-ui);
    padding: 3px 0;
    cursor: pointer;
  }
  .menu .seg button.on {
    border-color: var(--accent);
    color: var(--accent-strong);
    background: var(--accent-soft);
  }
  .menu .opt {
    all: unset;
    padding: 4px 10px;
    cursor: pointer;
    color: var(--text-primary);
  }
  .menu .opt:hover:not(:disabled),
  .menu .opt:focus-visible {
    background: var(--accent-soft);
    color: var(--accent-strong);
  }
  .menu .opt:disabled {
    color: var(--text-tertiary);
    cursor: not-allowed;
  }
  .menu .why {
    font-size: 9.5px;
    margin-left: 6px;
  }
</style>
