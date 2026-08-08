<script>
  import ToneCurveEditor from "$lib/components/ToneCurveEditor.svelte";
  import { HSL_BAND_NAMES, HSL_BAND_CENTERS_DEG, IDENTITY_HSL_BANDS } from "$lib/api/develop.js";

  /**
   * @type {{
   *   exposure: number,
   *   contrast: number,
   *   saturation: number,
   *   onExposureChange: (value: number) => void,
   *   onContrastChange: (value: number) => void,
   *   onSaturationChange: (value: number) => void,
   *   toneCurvePoints: readonly {x: number, y: number}[],
   *   onToneCurveChange: (points: readonly {x: number, y: number}[]) => void,
   *   hslBands: Readonly<Record<string, {hue: number, saturation: number, luminance: number}>>,
   *   onHslBandChange: (bandName: string, patch: Partial<{hue: number, saturation: number, luminance: number}>) => void,
   *   splitToning: {shadows: {hue: number, saturation: number}, highlights: {hue: number, saturation: number}, balance: number},
   *   onSplitToningZoneChange: (zone: "shadows" | "highlights", patch: Partial<{hue: number, saturation: number}>) => void,
   *   onSplitToningBalanceChange: (balance: number) => void,
   *   highlightedHslBand: string | null,
   *   isEyedropperActive: (target: "split_toning_shadows" | "split_toning_highlights" | "hsl_band" | "tone_curve_point") => boolean,
   *   onEyedropperToggle: (target: "split_toning_shadows" | "split_toning_highlights" | "hsl_band" | "tone_curve_point") => void,
   *   hasEdits: boolean,
   *   onResetRequest: () => void,
   *   dehaze: number,
   *   onDehazeChange: (value: number) => void,
   *   texture: number,
   *   onTextureChange: (value: number) => void,
   *   clarity: number,
   *   onClarityChange: (value: number) => void,
   *   vignette: {amount: number, midpoint: number, feather: number},
   *   onVignetteChange: (patch: Partial<{amount: number, midpoint: number, feather: number}>) => void,
   *   grain: {amount: number, size: number, roughness: number},
   *   onGrainChange: (patch: Partial<{amount: number, size: number, roughness: number}>) => void,
   * }}
   */
  let {
    exposure,
    contrast,
    saturation,
    onExposureChange,
    onContrastChange,
    onSaturationChange,
    toneCurvePoints,
    onToneCurveChange,
    hslBands,
    onHslBandChange,
    splitToning,
    onSplitToningZoneChange,
    onSplitToningBalanceChange,
    highlightedHslBand,
    isEyedropperActive,
    onEyedropperToggle,
    hasEdits,
    onResetRequest,
    dehaze,
    onDehazeChange,
    texture,
    onTextureChange,
    clarity,
    onClarityChange,
    vignette,
    onVignetteChange,
    grain,
    onGrainChange,
  } = $props();

  // HSL band-jump eyedropper: scroll the identified band into view whenever
  // it changes. Meaningful (not decorative) because .panel is genuinely
  // overflow-y:auto and a band near the bottom (magenta) can sit outside
  // the visible scroll area when the section first opens.
  $effect(() => {
    if (highlightedHslBand) {
      document.querySelector(`[data-band="${highlightedHslBand}"]`)?.scrollIntoView({ behavior: "smooth", block: "center" });
    }
  });

  const STATIC_SECTIONS = [
    { title: "Detail", note: "Sharpening · noise reduction" },
    { title: "Lens Corrections", note: "Profile · chromatic aberration" },
  ];

  function bandLabel(/** @type {string} */ name) {
    return name[0].toUpperCase() + name.slice(1);
  }

  /**
   * @typedef {"split_toning_shadows" | "split_toning_highlights" | "hsl_band" | "tone_curve_point"} EyedropperTarget
   */
</script>

<!-- Eyedropper icon, reused verbatim from MaskEditorPanel.svelte's own
     .resample button -- this codebase's established "same icon vocabulary
     across the app" practice (that button was itself reused from the
     Color Range mask tool's own eyedropper icon). -->
{#snippet eyedropperIcon()}
  <svg viewBox="0 0 16 16" width="14" height="14" fill="none" aria-hidden="true">
    <line x1="14" y1="2.3" x2="10.7" y2="5.6" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" />
    <line x1="10.5" y1="5.4" x2="4.3" y2="11.6" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" />
    <path d="M4.3 11.6 L3 14 L5.4 12.7 Z" fill="currentColor" />
  </svg>
{/snippet}

<div class="panel">
  <div class="panel-header">
    <span class="panel-title">Edit</span>
    <button class="reset-btn" type="button" disabled={!hasEdits} onclick={onResetRequest}>Reset</button>
  </div>
  <details class="section" open>
    <summary>Basic</summary>
    <div class="sub-body">
      <div class="row">
        <label for="exposure">Exposure</label>
        <input
          id="exposure"
          type="range"
          min="-5"
          max="5"
          step="0.05"
          value={exposure}
          oninput={(e) => onExposureChange(Number(e.currentTarget.value))}
        />
        <span class="val">{exposure >= 0 ? "+" : ""}{exposure.toFixed(2)}</span>
      </div>
      <div class="row">
        <label for="contrast">Contrast</label>
        <input
          id="contrast"
          type="range"
          min="-100"
          max="100"
          step="1"
          value={contrast}
          oninput={(e) => onContrastChange(Number(e.currentTarget.value))}
        />
        <span class="val">{contrast >= 0 ? "+" : ""}{contrast}</span>
      </div>
      <div class="row">
        <label for="saturation">Saturation</label>
        <input
          id="saturation"
          type="range"
          min="-100"
          max="100"
          step="1"
          value={saturation}
          oninput={(e) => onSaturationChange(Number(e.currentTarget.value))}
        />
        <span class="val">{saturation >= 0 ? "+" : ""}{saturation}</span>
      </div>
    </div>
  </details>

  <details class="section" open>
    <summary>Tone Curve</summary>
    <div class="sub-body">
      <div class="row eyedropper-row">
        <button
          class="resample"
          class:active={isEyedropperActive("tone_curve_point")}
          type="button"
          title="Add a curve point from a photo tone"
          aria-label="Add a curve point from a photo tone"
          onclick={() => onEyedropperToggle("tone_curve_point")}
        >
          {@render eyedropperIcon()}
        </button>
        <span>Click a tone in the photo to add a point</span>
      </div>
      <ToneCurveEditor points={toneCurvePoints} onChange={onToneCurveChange} />
    </div>
  </details>

  <details class="section">
    <summary>HSL / Color</summary>
    <div class="sub-body">
      <div class="row eyedropper-row">
        <button
          class="resample"
          class:active={isEyedropperActive("hsl_band")}
          type="button"
          title="Identify a color's HSL band"
          aria-label="Identify a color's HSL band"
          onclick={() => onEyedropperToggle("hsl_band")}
        >
          {@render eyedropperIcon()}
        </button>
        <span>Sample from photo to jump to its band</span>
      </div>
      {#each HSL_BAND_NAMES as bandName, i (bandName)}
        {@const band = hslBands[bandName] ?? IDENTITY_HSL_BANDS[bandName]}
        <div class="hsl-band" class:jump-highlight={bandName === highlightedHslBand} data-band={bandName}>
          <div class="hsl-band-label">
            <span class="swatch" style="background: hsl({HSL_BAND_CENTERS_DEG[i]}, 70%, 50%)"></span>
            <span>{bandLabel(bandName)}</span>
          </div>
          <div class="row">
            <label for="hsl-{bandName}-hue">Hue</label>
            <input
              id="hsl-{bandName}-hue"
              type="range"
              min="-100"
              max="100"
              step="1"
              value={band.hue}
              oninput={(e) => onHslBandChange(bandName, { hue: Number(e.currentTarget.value) })}
            />
            <span class="val">{band.hue >= 0 ? "+" : ""}{band.hue}</span>
          </div>
          <div class="row">
            <label for="hsl-{bandName}-sat">Sat</label>
            <input
              id="hsl-{bandName}-sat"
              type="range"
              min="-100"
              max="100"
              step="1"
              value={band.saturation}
              oninput={(e) => onHslBandChange(bandName, { saturation: Number(e.currentTarget.value) })}
            />
            <span class="val">{band.saturation >= 0 ? "+" : ""}{band.saturation}</span>
          </div>
          <div class="row">
            <label for="hsl-{bandName}-lum">Lum</label>
            <input
              id="hsl-{bandName}-lum"
              type="range"
              min="-100"
              max="100"
              step="1"
              value={band.luminance}
              oninput={(e) => onHslBandChange(bandName, { luminance: Number(e.currentTarget.value) })}
            />
            <span class="val">{band.luminance >= 0 ? "+" : ""}{band.luminance}</span>
          </div>
        </div>
      {/each}
    </div>
  </details>

  <details class="section">
    <summary>Split Toning</summary>
    <div class="sub-body">
      <div class="split-zone">
        <div class="split-zone-label">
          <span class="swatch" style="background: hsl({splitToning.shadows.hue}, {splitToning.shadows.saturation}%, 50%)"></span>
          <span>Shadows</span>
          <button
            class="resample"
            class:active={isEyedropperActive("split_toning_shadows")}
            type="button"
            title="Sample shadow tint from photo"
            aria-label="Sample shadow tint from photo"
            onclick={() => onEyedropperToggle("split_toning_shadows")}
          >
            {@render eyedropperIcon()}
          </button>
        </div>
        <div class="row">
          <label for="st-shadow-hue">Hue</label>
          <input
            id="st-shadow-hue"
            type="range"
            min="0"
            max="360"
            step="1"
            value={splitToning.shadows.hue}
            oninput={(e) => onSplitToningZoneChange("shadows", { hue: Number(e.currentTarget.value) })}
          />
          <span class="val">{splitToning.shadows.hue}</span>
        </div>
        <div class="row">
          <label for="st-shadow-sat">Sat</label>
          <input
            id="st-shadow-sat"
            type="range"
            min="0"
            max="100"
            step="1"
            value={splitToning.shadows.saturation}
            oninput={(e) => onSplitToningZoneChange("shadows", { saturation: Number(e.currentTarget.value) })}
          />
          <span class="val">{splitToning.shadows.saturation}</span>
        </div>
      </div>
      <div class="split-zone">
        <div class="split-zone-label">
          <span class="swatch" style="background: hsl({splitToning.highlights.hue}, {splitToning.highlights.saturation}%, 50%)"></span>
          <span>Highlights</span>
          <button
            class="resample"
            class:active={isEyedropperActive("split_toning_highlights")}
            type="button"
            title="Sample highlight tint from photo"
            aria-label="Sample highlight tint from photo"
            onclick={() => onEyedropperToggle("split_toning_highlights")}
          >
            {@render eyedropperIcon()}
          </button>
        </div>
        <div class="row">
          <label for="st-highlight-hue">Hue</label>
          <input
            id="st-highlight-hue"
            type="range"
            min="0"
            max="360"
            step="1"
            value={splitToning.highlights.hue}
            oninput={(e) => onSplitToningZoneChange("highlights", { hue: Number(e.currentTarget.value) })}
          />
          <span class="val">{splitToning.highlights.hue}</span>
        </div>
        <div class="row">
          <label for="st-highlight-sat">Sat</label>
          <input
            id="st-highlight-sat"
            type="range"
            min="0"
            max="100"
            step="1"
            value={splitToning.highlights.saturation}
            oninput={(e) => onSplitToningZoneChange("highlights", { saturation: Number(e.currentTarget.value) })}
          />
          <span class="val">{splitToning.highlights.saturation}</span>
        </div>
      </div>
      <div class="row">
        <label for="st-balance">Balance</label>
        <input
          id="st-balance"
          type="range"
          min="-100"
          max="100"
          step="1"
          value={splitToning.balance}
          oninput={(e) => onSplitToningBalanceChange(Number(e.currentTarget.value))}
        />
        <span class="val">{splitToning.balance >= 0 ? "+" : ""}{splitToning.balance}</span>
      </div>
    </div>
  </details>

  <details class="section">
    <summary>Texture &amp; Clarity</summary>
    <div class="sub-body">
      <div class="row">
        <label for="texture-amount">Texture</label>
        <input
          id="texture-amount"
          type="range"
          min="-100"
          max="100"
          step="1"
          value={texture}
          oninput={(e) => onTextureChange(Number(e.currentTarget.value))}
        />
        <span class="val">{texture}</span>
      </div>
      <div class="row">
        <label for="clarity-amount">Clarity</label>
        <input
          id="clarity-amount"
          type="range"
          min="-100"
          max="100"
          step="1"
          value={clarity}
          oninput={(e) => onClarityChange(Number(e.currentTarget.value))}
        />
        <span class="val">{clarity}</span>
      </div>
    </div>
  </details>

  <details class="section">
    <summary>Dehaze</summary>
    <div class="sub-body">
      <div class="row">
        <label for="dehaze-amount">Amount</label>
        <input
          id="dehaze-amount"
          type="range"
          min="0"
          max="100"
          step="1"
          value={dehaze}
          oninput={(e) => onDehazeChange(Number(e.currentTarget.value))}
        />
        <span class="val">{dehaze}</span>
      </div>
    </div>
  </details>

  <details class="section">
    <summary>Vignette</summary>
    <div class="sub-body">
      <div class="row">
        <label for="vignette-amount">Amount</label>
        <input
          id="vignette-amount"
          type="range"
          min="-100"
          max="100"
          step="1"
          value={vignette.amount}
          oninput={(e) => onVignetteChange({ amount: Number(e.currentTarget.value) })}
        />
        <span class="val">{vignette.amount}</span>
      </div>
      <div class="row">
        <label for="vignette-midpoint">Midpoint</label>
        <input
          id="vignette-midpoint"
          type="range"
          min="0"
          max="100"
          step="1"
          value={vignette.midpoint}
          oninput={(e) => onVignetteChange({ midpoint: Number(e.currentTarget.value) })}
        />
        <span class="val">{vignette.midpoint}</span>
      </div>
      <div class="row">
        <label for="vignette-feather">Feather</label>
        <input
          id="vignette-feather"
          type="range"
          min="0"
          max="100"
          step="1"
          value={vignette.feather}
          oninput={(e) => onVignetteChange({ feather: Number(e.currentTarget.value) })}
        />
        <span class="val">{vignette.feather}</span>
      </div>
    </div>
  </details>

  <details class="section">
    <summary>Grain</summary>
    <div class="sub-body">
      <div class="row">
        <label for="grain-amount">Amount</label>
        <input
          id="grain-amount"
          type="range"
          min="0"
          max="100"
          step="1"
          value={grain.amount}
          oninput={(e) => onGrainChange({ amount: Number(e.currentTarget.value) })}
        />
        <span class="val">{grain.amount}</span>
      </div>
      <div class="row">
        <label for="grain-size">Size</label>
        <input
          id="grain-size"
          type="range"
          min="0"
          max="100"
          step="1"
          value={grain.size}
          oninput={(e) => onGrainChange({ size: Number(e.currentTarget.value) })}
        />
        <span class="val">{grain.size}</span>
      </div>
      <div class="row">
        <label for="grain-roughness">Roughness</label>
        <input
          id="grain-roughness"
          type="range"
          min="0"
          max="100"
          step="1"
          value={grain.roughness}
          oninput={(e) => onGrainChange({ roughness: Number(e.currentTarget.value) })}
        />
        <span class="val">{grain.roughness}</span>
      </div>
    </div>
  </details>

  {#each STATIC_SECTIONS as section (section.title)}
    <details class="section">
      <summary>{section.title}</summary>
      <div class="sub-body static-note">{section.note}</div>
    </details>
  {/each}
</div>

<style>
  .panel {
    width: 240px;
    flex: none;
    background: var(--bg-panel);
    border-left: 1px solid var(--border-subtle);
    overflow-y: auto;
    overflow-x: hidden;
    padding: 14px 12px;
  }
  .panel-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 2px 4px 10px;
  }
  .panel-title {
    font-size: 11.5px;
    font-weight: 600;
    letter-spacing: 0.02em;
    color: var(--text-primary);
  }
  /* Same destructive-action color as MaskEditorPanel.svelte's .delete
     button and ConfirmDialog.svelte's .danger confirm button -- one
     consistent "this is destructive" visual language, not a new one. */
  .reset-btn {
    all: unset;
    cursor: pointer;
    padding: 4px 10px;
    font-size: 11px;
    font-weight: 600;
    border-radius: var(--radius-s);
    color: var(--label-red);
    border: 1px solid var(--border-strong);
  }
  .reset-btn:disabled {
    cursor: default;
    color: var(--text-tertiary);
    opacity: 0.6;
  }
  .section {
    border-bottom: 1px solid var(--border-subtle);
  }
  .section:last-child {
    border-bottom: none;
  }
  .section summary {
    list-style: none;
    cursor: pointer;
    padding: 9px 4px;
    font-size: 11.5px;
    font-weight: 600;
    letter-spacing: 0.02em;
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--text-primary);
  }
  .section summary::-webkit-details-marker {
    display: none;
  }
  .section summary::before {
    content: "›";
    display: inline-block;
    color: var(--text-tertiary);
    transition: transform 0.12s ease;
    width: 10px;
  }
  .section[open] summary::before {
    transform: rotate(90deg);
  }
  .sub-body {
    padding-bottom: 8px;
  }
  .static-note {
    color: var(--text-tertiary);
    font-size: 11px;
    padding: 4px 4px 12px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 5px 4px;
  }
  .row label {
    width: 66px;
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
  .row input[type="range"]:focus-visible::-webkit-slider-thumb {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .row .val {
    width: 42px;
    text-align: right;
    font-family: var(--font-mono);
    font-variant-numeric: tabular-nums;
    font-size: 10.5px;
    color: var(--text-tertiary);
  }
  .hsl-band {
    padding: 6px 4px 10px;
    border-bottom: 1px solid var(--border-subtle);
  }
  .hsl-band:last-child {
    border-bottom: none;
    padding-bottom: 4px;
  }
  .hsl-band-label {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 0 0 2px;
    font-size: 11px;
    font-weight: 600;
    color: var(--text-secondary);
  }
  .swatch {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    border: 1px solid var(--border-strong);
    flex: none;
  }
  .split-zone {
    padding: 6px 4px 10px;
    border-bottom: 1px solid var(--border-subtle);
  }
  .split-zone-label {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 0 0 2px;
    font-size: 11px;
    font-weight: 600;
    color: var(--text-secondary);
  }
  .split-zone-label .resample {
    margin-left: auto;
  }
  .eyedropper-row {
    padding-bottom: 8px;
    font-size: 11px;
    color: var(--text-secondary);
  }
  /* Reused verbatim from MaskEditorPanel.svelte's own .resample button. */
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
  /* Transient highlight for the HSL eyedropper's band-jump navigation --
     fades back to the normal border after the same timeout that clears
     highlightedHslBand in +page.svelte. */
  .hsl-band.jump-highlight {
    outline: 1.5px solid var(--accent);
    outline-offset: -1.5px;
    transition: outline-color 0.2s ease;
  }
</style>
