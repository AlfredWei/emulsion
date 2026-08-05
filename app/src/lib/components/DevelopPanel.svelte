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
   *   onToneCurveChange: (points: {x: number, y: number}[]) => void,
   *   hslBands: Readonly<Record<string, {hue: number, saturation: number, luminance: number}>>,
   *   onHslBandChange: (bandName: string, patch: Partial<{hue: number, saturation: number, luminance: number}>) => void,
   *   splitToning: {shadows: {hue: number, saturation: number}, highlights: {hue: number, saturation: number}, balance: number},
   *   onSplitToningZoneChange: (zone: "shadows" | "highlights", patch: Partial<{hue: number, saturation: number}>) => void,
   *   onSplitToningBalanceChange: (balance: number) => void,
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
  } = $props();

  const STATIC_SECTIONS = [
    { title: "Detail", note: "Sharpening · noise reduction" },
    { title: "Effects", note: "Grain · vignette" },
    { title: "Lens Corrections", note: "Profile · chromatic aberration" },
  ];

  function bandLabel(/** @type {string} */ name) {
    return name[0].toUpperCase() + name.slice(1);
  }
</script>

<div class="panel">
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
      <ToneCurveEditor points={toneCurvePoints} onChange={onToneCurveChange} />
    </div>
  </details>

  <details class="section">
    <summary>HSL / Color</summary>
    <div class="sub-body">
      {#each HSL_BAND_NAMES as bandName, i (bandName)}
        {@const band = hslBands[bandName] ?? IDENTITY_HSL_BANDS[bandName]}
        <div class="hsl-band">
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
</style>
