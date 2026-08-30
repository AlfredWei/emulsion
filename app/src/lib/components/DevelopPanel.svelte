<script>
  import ToneCurveEditor from "$lib/components/ToneCurveEditor.svelte";
  import Histogram from "$lib/components/Histogram.svelte";
  import {
    HSL_BAND_NAMES,
    HSL_BAND_CENTERS_DEG,
    IDENTITY_HSL_BANDS,
    WB_PRESETS,
  } from "$lib/api/develop.js";

  /**
   * @type {{
   *   histogramData: {r: Uint32Array, g: Uint32Array, b: Uint32Array} | null,
   *   showClippingOverlay?: boolean,
   *   onToggleClippingOverlay?: () => void,
   *   hoverPixel?: {r: number, g: number, b: number} | null,
   *   exposure: number,
   *   contrast: number,
   *   saturation: number,
   *   temperature?: number,
   *   tint?: number,
   *   highlights?: number,
   *   shadows?: number,
   *   whites?: number,
   *   blacks?: number,
   *   onExposureChange: (value: number) => void,
   *   onContrastChange: (value: number) => void,
   *   onSaturationChange: (value: number) => void,
   *   onTemperatureChange?: (value: number) => void,
   *   onTintChange?: (value: number) => void,
   *   onHighlightsChange?: (value: number) => void,
   *   onShadowsChange?: (value: number) => void,
   *   onWhitesChange?: (value: number) => void,
   *   onBlacksChange?: (value: number) => void,
   *   onAutoWhiteBalance?: () => void,
   *   onAutoTone?: () => void,
   *   onWbPresetChange?: (presetKey: string) => void,
   *   toneCurvePoints: readonly {x: number, y: number}[],
   *   onToneCurveChange: (points: readonly {x: number, y: number}[]) => void,
   *   hslBands: Readonly<Record<string, {hue: number, saturation: number, luminance: number}>>,
   *   onHslBandChange: (bandName: string, patch: Partial<{hue: number, saturation: number, luminance: number}>) => void,
   *   splitToning: {shadows: {hue: number, saturation: number}, highlights: {hue: number, saturation: number}, balance: number},
   *   onSplitToningZoneChange: (zone: "shadows" | "highlights", patch: Partial<{hue: number, saturation: number}>) => void,
   *   onSplitToningBalanceChange: (balance: number) => void,
   *   highlightedHslBand: string | null,
   *   isEyedropperActive: (target: "split_toning_shadows" | "split_toning_highlights" | "hsl_band" | "tone_curve_point" | "white_balance") => boolean,
   *   onEyedropperToggle: (target: "split_toning_shadows" | "split_toning_highlights" | "hsl_band" | "tone_curve_point" | "white_balance") => void,
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
   *   lensCorrection: {
   *     profile_enabled: boolean,
   *     distortion_amount: number,
   *     vignette_amount: number,
   *     ca_amount: number,
   *     manual_distortion: number,
   *     manual_ca: number,
   *     profile: import('$lib/api/develop.js').LensProfileMatch | null,
   *   },
   *   onLensCorrectionChange: (patch: Partial<{profile_enabled: boolean, distortion_amount: number, vignette_amount: number, ca_amount: number, manual_distortion: number, manual_ca: number}>) => void,
   *   perspective: {vertical: number, horizontal: number, rotate: number, aspect: number, scale: number},
   *   onPerspectiveChange: (patch: Partial<{vertical: number, horizontal: number, rotate: number, aspect: number, scale: number}>) => void,
   *   grain: {amount: number, size: number, roughness: number},
   *   onGrainChange: (patch: Partial<{amount: number, size: number, roughness: number}>) => void,
   *   sharpen: {amount: number, radius: number, detail: number, masking: number},
   *   onSharpenChange: (patch: Partial<{amount: number, radius: number, detail: number, masking: number}>) => void,
   *   lumaNR: {amount: number, detail: number, contrast: number},
   *   onLumaNRChange: (patch: Partial<{amount: number, detail: number, contrast: number}>) => void,
   *   colorNR: {amount: number, detail: number},
   *   onColorNRChange: (patch: Partial<{amount: number, detail: number}>) => void,
   *   presets: import('$lib/api/develop.js').PresetEntry[],
   *   onApplyPreset: (presetId: number) => void,
   *   onSaveCurrentAsPresetRequest: () => void,
   *   onExportPreset: (presetId: number) => void,
   *   onDeletePresetRequest: (presetId: number) => void,
   *   onImportPresetRequest: () => void,
   *   softProofEnabled?: boolean,
   *   softProofTarget?: "srgb" | "adobe-rgb" | "prophoto-rgb" | "custom",
   *   softProofCustomProfilePath?: string | null,
   *   softProofIntent?: "perceptual" | "relative" | "saturation" | "absolute",
   *   softProofGamutWarning?: boolean,
   *   onSoftProofEnabledChange?: (value: boolean) => void,
   *   onSoftProofTargetChange?: (value: string) => void,
   *   onSoftProofIntentChange?: (value: string) => void,
   *   onSoftProofGamutWarningChange?: (value: boolean) => void,
   *   onChooseCustomProfile?: () => void,
   *   onCopySettingsRequest: () => void,
   *   canPasteSettings: boolean,
   *   onPasteSettingsRequest: () => void,
   * }}
   */
  let {
    histogramData,
    showClippingOverlay = false,
    onToggleClippingOverlay,
    hoverPixel = null,
    exposure,
    contrast,
    saturation,
    temperature = 0,
    tint = 0,
    highlights = 0,
    shadows = 0,
    whites = 0,
    blacks = 0,
    onExposureChange,
    onContrastChange,
    onSaturationChange,
    onTemperatureChange,
    onTintChange,
    onHighlightsChange,
    onShadowsChange,
    onWhitesChange,
    onBlacksChange,
    onAutoWhiteBalance,
    onAutoTone,
    onWbPresetChange,
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
    lensCorrection,
    onLensCorrectionChange,
    perspective,
    onPerspectiveChange,
    grain,
    onGrainChange,
    sharpen,
    onSharpenChange,
    lumaNR,
    onLumaNRChange,
    colorNR,
    onColorNRChange,
    presets,
    onApplyPreset,
    onSaveCurrentAsPresetRequest,
    onExportPreset,
    onDeletePresetRequest,
    onImportPresetRequest,
    softProofEnabled = false,
    softProofTarget = "srgb",
    softProofCustomProfilePath = null,
    softProofIntent = "relative",
    softProofGamutWarning = false,
    onSoftProofEnabledChange,
    onSoftProofTargetChange,
    onSoftProofIntentChange,
    onSoftProofGamutWarningChange,
    onChooseCustomProfile,
    onCopySettingsRequest,
    canPasteSettings,
    onPasteSettingsRequest,
  } = $props();

  // HSL band-jump eyedropper: scroll the identified band into view whenever
  // it changes. Meaningful (not decorative) because .panel is genuinely
  // overflow-y:auto and a band near the bottom (magenta) can sit outside
  // the visible scroll area when the section first opens.
  $effect(() => {
    if (highlightedHslBand) {
      document
        .querySelector(`[data-band="${highlightedHslBand}"]`)
        ?.scrollIntoView({ behavior: "smooth", block: "center" });
    }
  });

  const STATIC_SECTIONS = [];

  function bandLabel(/** @type {string} */ name) {
    return name[0].toUpperCase() + name.slice(1);
  }

  /**
   * @typedef {"split_toning_shadows" | "split_toning_highlights" | "hsl_band" | "tone_curve_point" | "white_balance"} EyedropperTarget
   */
</script>

<!-- Eyedropper icon, reused verbatim from MaskEditorPanel.svelte's own
     .resample button -- this codebase's established "same icon vocabulary
     across the app" practice (that button was itself reused from the
     Color Range mask tool's own eyedropper icon). -->
{#snippet eyedropperIcon()}
  <svg viewBox="0 0 16 16" width="14" height="14" fill="none" aria-hidden="true">
    <line
      x1="14"
      y1="2.3"
      x2="10.7"
      y2="5.6"
      stroke="currentColor"
      stroke-width="1.6"
      stroke-linecap="round"
    />
    <line
      x1="10.5"
      y1="5.4"
      x2="4.3"
      y2="11.6"
      stroke="currentColor"
      stroke-width="1.6"
      stroke-linecap="round"
    />
    <path d="M4.3 11.6 L3 14 L5.4 12.7 Z" fill="currentColor" />
  </svg>
{/snippet}

{#snippet autoWbIcon()}
  <svg viewBox="0 0 16 16" width="13" height="13" fill="none" aria-hidden="true">
    <!-- Magic sparkles & temperature balance -->
    <path d="M7.5 1.5 L8.5 4.5 L11.5 5.5 L8.5 6.5 L7.5 9.5 L6.5 6.5 L3.5 5.5 L6.5 4.5 Z" fill="currentColor" />
    <path d="M12 9 L12.5 10.5 L14 11 L12.5 11.5 L12 13 L11.5 11.5 L10 11 L11.5 10.5 Z" fill="currentColor" opacity="0.8" />
    <circle cx="4" cy="11.5" r="1.5" fill="currentColor" opacity="0.8" />
  </svg>
{/snippet}

{#snippet autoToneIcon()}
  <svg viewBox="0 0 16 16" width="13" height="13" fill="none" aria-hidden="true">
    <!-- Equalizer / Auto tone levels -->
    <line x1="3" y1="2" x2="3" y2="14" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" opacity="0.4" />
    <circle cx="3" cy="6" r="2" fill="currentColor" />
    <line x1="8" y1="2" x2="8" y2="14" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" opacity="0.4" />
    <circle cx="8" cy="10" r="2" fill="currentColor" />
    <line x1="13" y1="2" x2="13" y2="14" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" opacity="0.4" />
    <circle cx="13" cy="4" r="2" fill="currentColor" />
  </svg>
{/snippet}

<div class="panel">
  <div class="panel-scroll">
  <div class="panel-header">
    <span class="panel-title">Edit</span>
    <button class="reset-btn" type="button" disabled={!hasEdits} onclick={onResetRequest}>Reset</button>
  </div>
  <Histogram data={histogramData} {showClippingOverlay} {onToggleClippingOverlay} {hoverPixel} />
  <details class="section" open>
    <summary>Basic</summary>
    <div class="sub-body">
      <!-- White Balance -->
      <div class="subsection-header">
        <span class="subsection-title">White Balance</span>
        <div class="header-actions">
          <button
            class="resample"
            class:active={isEyedropperActive("white_balance")}
            type="button"
            title="White Balance Eyedropper: click a neutral tone in photo"
            aria-label="White Balance Eyedropper"
            onclick={() => onEyedropperToggle("white_balance")}
          >
            {@render eyedropperIcon()}
          </button>
          {#if onAutoWhiteBalance}
            <button
              class="auto-btn"
              type="button"
              title="Auto White Balance (AWB)"
              aria-label="Auto White Balance"
              onclick={onAutoWhiteBalance}
            >
              {@render autoWbIcon()}
            </button>
          {/if}
        </div>
      </div>

      {#if onWbPresetChange}
        <div class="row">
          <label for="wb-preset">Profile</label>
          <select
            id="wb-preset"
            class="select-input"
            onchange={(e) => onWbPresetChange(e.currentTarget.value)}
          >
            {#each Object.entries(WB_PRESETS) as [key, preset] (key)}
              <option value={key}>{preset.name}</option>
            {/each}
          </select>
        </div>
      {/if}

      <div class="row">
        <label for="temperature">Temp</label>
        <input
          id="temperature"
          type="range"
          min="-100"
          max="100"
          step="1"
          value={temperature}
          oninput={(e) => onTemperatureChange?.(Number(e.currentTarget.value))}
        />
        <span class="val">{temperature >= 0 ? "+" : ""}{temperature}</span>
      </div>

      <div class="row">
        <label for="tint">Tint</label>
        <input
          id="tint"
          type="range"
          min="-100"
          max="100"
          step="1"
          value={tint}
          oninput={(e) => onTintChange?.(Number(e.currentTarget.value))}
        />
        <span class="val">{tint >= 0 ? "+" : ""}{tint}</span>
      </div>

      <!-- Tone -->
      <div class="subsection-header" style="margin-top: 10px;">
        <span class="subsection-title">Tone</span>
        {#if onAutoTone}
          <button
            class="auto-btn"
            type="button"
            title="Auto Tone: balance exposure, contrast & dynamic range"
            aria-label="Auto Tone"
            onclick={onAutoTone}
          >
            {@render autoToneIcon()}
          </button>
        {/if}
      </div>

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
        <label for="highlights">Highlights</label>
        <input
          id="highlights"
          type="range"
          min="-100"
          max="100"
          step="1"
          value={highlights}
          oninput={(e) => onHighlightsChange?.(Number(e.currentTarget.value))}
        />
        <span class="val">{highlights >= 0 ? "+" : ""}{highlights}</span>
      </div>
      <div class="row">
        <label for="shadows">Shadows</label>
        <input
          id="shadows"
          type="range"
          min="-100"
          max="100"
          step="1"
          value={shadows}
          oninput={(e) => onShadowsChange?.(Number(e.currentTarget.value))}
        />
        <span class="val">{shadows >= 0 ? "+" : ""}{shadows}</span>
      </div>
      <div class="row">
        <label for="whites">Whites</label>
        <input
          id="whites"
          type="range"
          min="-100"
          max="100"
          step="1"
          value={whites}
          oninput={(e) => onWhitesChange?.(Number(e.currentTarget.value))}
        />
        <span class="val">{whites >= 0 ? "+" : ""}{whites}</span>
      </div>
      <div class="row">
        <label for="blacks">Blacks</label>
        <input
          id="blacks"
          type="range"
          min="-100"
          max="100"
          step="1"
          value={blacks}
          oninput={(e) => onBlacksChange?.(Number(e.currentTarget.value))}
        />
        <span class="val">{blacks >= 0 ? "+" : ""}{blacks}</span>
      </div>

      <!-- Presence -->
      <div class="subsection-header" style="margin-top: 10px;">
        <span class="subsection-title">Presence</span>
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
    <summary>Sharpening</summary>
    <div class="sub-body">
      <div class="row">
        <label for="sharpen-amount">Amount</label>
        <input
          id="sharpen-amount"
          type="range"
          min="0"
          max="100"
          step="1"
          value={sharpen.amount}
          oninput={(e) => onSharpenChange({ amount: Number(e.currentTarget.value) })}
        />
        <span class="val">{sharpen.amount}</span>
      </div>
      <div class="row">
        <label for="sharpen-radius">Radius</label>
        <input
          id="sharpen-radius"
          type="range"
          min="0"
          max="100"
          step="1"
          value={sharpen.radius}
          oninput={(e) => onSharpenChange({ radius: Number(e.currentTarget.value) })}
        />
        <span class="val">{sharpen.radius}</span>
      </div>
      <div class="row">
        <label for="sharpen-detail">Detail</label>
        <input
          id="sharpen-detail"
          type="range"
          min="0"
          max="100"
          step="1"
          value={sharpen.detail}
          oninput={(e) => onSharpenChange({ detail: Number(e.currentTarget.value) })}
        />
        <span class="val">{sharpen.detail}</span>
      </div>
      <div class="row">
        <label for="sharpen-masking">Masking</label>
        <input
          id="sharpen-masking"
          type="range"
          min="0"
          max="100"
          step="1"
          value={sharpen.masking}
          oninput={(e) => onSharpenChange({ masking: Number(e.currentTarget.value) })}
        />
        <span class="val">{sharpen.masking}</span>
      </div>
    </div>
  </details>

  <details class="section">
    <summary>Noise Reduction</summary>
    <div class="sub-body">
      <div class="subsection-label">Luminance</div>
      <div class="row">
        <label for="luma-nr-amount">Amount</label>
        <input
          id="luma-nr-amount"
          type="range"
          min="0"
          max="100"
          step="1"
          value={lumaNR.amount}
          oninput={(e) => onLumaNRChange({ amount: Number(e.currentTarget.value) })}
        />
        <span class="val">{lumaNR.amount}</span>
      </div>
      <div class="row">
        <label for="luma-nr-detail">Detail</label>
        <input
          id="luma-nr-detail"
          type="range"
          min="0"
          max="100"
          step="1"
          value={lumaNR.detail}
          oninput={(e) => onLumaNRChange({ detail: Number(e.currentTarget.value) })}
        />
        <span class="val">{lumaNR.detail}</span>
      </div>
      <div class="row">
        <label for="luma-nr-contrast">Contrast</label>
        <input
          id="luma-nr-contrast"
          type="range"
          min="0"
          max="100"
          step="1"
          value={lumaNR.contrast}
          oninput={(e) => onLumaNRChange({ contrast: Number(e.currentTarget.value) })}
        />
        <span class="val">{lumaNR.contrast}</span>
      </div>
      <div class="subsection-label">Color</div>
      <div class="row">
        <label for="color-nr-amount">Amount</label>
        <input
          id="color-nr-amount"
          type="range"
          min="0"
          max="100"
          step="1"
          value={colorNR.amount}
          oninput={(e) => onColorNRChange({ amount: Number(e.currentTarget.value) })}
        />
        <span class="val">{colorNR.amount}</span>
      </div>
      <div class="row">
        <label for="color-nr-detail">Detail</label>
        <input
          id="color-nr-detail"
          type="range"
          min="0"
          max="100"
          step="1"
          value={colorNR.detail}
          oninput={(e) => onColorNRChange({ detail: Number(e.currentTarget.value) })}
        />
        <span class="val">{colorNR.detail}</span>
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

  <details class="section">
    <summary>Presets</summary>
    <div class="sub-body">
      <div class="preset-actions">
        <button type="button" class="preset-action-btn" onclick={onSaveCurrentAsPresetRequest}>
          Save Current as Preset…
        </button>
        <button type="button" class="preset-action-btn" onclick={onImportPresetRequest}>Import…</button>
      </div>
      {#if presets.length === 0}
        <div class="static-note">No presets yet -- save the current adjustments as one, or import a file.</div>
      {:else}
        <ul class="preset-list">
          {#each presets as preset (preset.id)}
            <li class="preset-row">
              <button
                type="button"
                class="preset-name-btn"
                onclick={() => onApplyPreset(preset.id)}
                title="Apply {preset.name}"
              >
                {preset.name}
              </button>
              <div class="preset-row-actions">
                <button type="button" class="preset-icon-btn" onclick={() => onExportPreset(preset.id)} title="Export">
                  ⇩
                </button>
                <button
                  type="button"
                  class="preset-icon-btn delete"
                  onclick={() => onDeletePresetRequest(preset.id)}
                  title="Delete"
                >
                  ×
                </button>
              </div>
            </li>
          {/each}
        </ul>
      {/if}
      <!-- Known limitation (see develop.js's applyPresetOps doc comment):
           each preset op wholly REPLACES the matching op on the target
           image, so an HSL/Tone Curve/Split Toning-bearing preset zeroes
           out any of the target's own untouched adjustments in that same
           category, not just the ones the preset itself set. Surfaced
           here rather than left as a silent, unexplained-looking data
           loss the first time someone hits it. -->
      <p class="preset-note">
        Applying a preset replaces matching adjustment categories (e.g. all HSL bands) entirely -- it does not merge
        partial changes.
      </p>
    </div>
  </details>

  <details class="section">
    <summary>Lens Corrections</summary>
    <div class="sub-body">
      {#if lensCorrection.profile}
        <div class="static-note">Profile found: {lensCorrection.profile.camera} + {lensCorrection.profile.lens}</div>
      {:else}
        <div class="static-note">No matching lens profile found for this photo's camera/lens metadata.</div>
      {/if}
      <label class="checkbox-row">
        <input
          type="checkbox"
          checked={lensCorrection.profile_enabled}
          onchange={(e) => onLensCorrectionChange({ profile_enabled: e.currentTarget.checked })}
        />
        Enable Profile Corrections
      </label>
      <div class="row">
        <label for="lens-distortion-amount">Distortion</label>
        <input
          id="lens-distortion-amount"
          type="range"
          min="0"
          max="100"
          step="1"
          disabled={!lensCorrection.profile_enabled}
          value={lensCorrection.distortion_amount}
          oninput={(e) => onLensCorrectionChange({ distortion_amount: Number(e.currentTarget.value) })}
        />
        <span class="val">{lensCorrection.distortion_amount}</span>
      </div>
      <div class="row">
        <label for="lens-vignette-amount">Vignetting</label>
        <input
          id="lens-vignette-amount"
          type="range"
          min="0"
          max="100"
          step="1"
          disabled={!lensCorrection.profile_enabled}
          value={lensCorrection.vignette_amount}
          oninput={(e) => onLensCorrectionChange({ vignette_amount: Number(e.currentTarget.value) })}
        />
        <span class="val">{lensCorrection.vignette_amount}</span>
      </div>
      <div class="row">
        <label for="lens-ca-amount">Chromatic Aberration</label>
        <input
          id="lens-ca-amount"
          type="range"
          min="0"
          max="100"
          step="1"
          disabled={!lensCorrection.profile_enabled}
          value={lensCorrection.ca_amount}
          oninput={(e) => onLensCorrectionChange({ ca_amount: Number(e.currentTarget.value) })}
        />
        <span class="val">{lensCorrection.ca_amount}</span>
      </div>
      <div class="subsection-label">Manual</div>
      <div class="row">
        <label for="lens-manual-distortion">Distortion</label>
        <input
          id="lens-manual-distortion"
          type="range"
          min="-100"
          max="100"
          step="1"
          value={lensCorrection.manual_distortion}
          oninput={(e) => onLensCorrectionChange({ manual_distortion: Number(e.currentTarget.value) })}
        />
        <span class="val">{lensCorrection.manual_distortion}</span>
      </div>
      <div class="row">
        <label for="lens-manual-ca">Chromatic Aberration</label>
        <input
          id="lens-manual-ca"
          type="range"
          min="-100"
          max="100"
          step="1"
          value={lensCorrection.manual_ca}
          oninput={(e) => onLensCorrectionChange({ manual_ca: Number(e.currentTarget.value) })}
        />
        <span class="val">{lensCorrection.manual_ca}</span>
      </div>
      <!-- No manual vignette control here on purpose -- it would be the
           exact same radial-gain formula the Vignette section above
           already exposes (see develop_engine.rs's own doc comment on
           this op for the full reasoning). -->
      <p class="preset-note">For manual vignette correction, use the Vignette section above.</p>
    </div>
  </details>

  <details class="section">
    <summary>Perspective</summary>
    <div class="sub-body">
      <div class="row">
        <label for="perspective-vertical">Vertical</label>
        <input
          id="perspective-vertical"
          type="range"
          min="-100"
          max="100"
          step="1"
          value={perspective.vertical}
          oninput={(e) => onPerspectiveChange({ vertical: Number(e.currentTarget.value) })}
        />
        <span class="val">{perspective.vertical}</span>
      </div>
      <div class="row">
        <label for="perspective-horizontal">Horizontal</label>
        <input
          id="perspective-horizontal"
          type="range"
          min="-100"
          max="100"
          step="1"
          value={perspective.horizontal}
          oninput={(e) => onPerspectiveChange({ horizontal: Number(e.currentTarget.value) })}
        />
        <span class="val">{perspective.horizontal}</span>
      </div>
      <div class="row">
        <label for="perspective-rotate">Rotate</label>
        <input
          id="perspective-rotate"
          type="range"
          min="-10"
          max="10"
          step="0.1"
          value={perspective.rotate}
          oninput={(e) => onPerspectiveChange({ rotate: Number(e.currentTarget.value) })}
        />
        <span class="val">{perspective.rotate}</span>
      </div>
      <div class="row">
        <label for="perspective-aspect">Aspect</label>
        <input
          id="perspective-aspect"
          type="range"
          min="-100"
          max="100"
          step="1"
          value={perspective.aspect}
          oninput={(e) => onPerspectiveChange({ aspect: Number(e.currentTarget.value) })}
        />
        <span class="val">{perspective.aspect}</span>
      </div>
      <div class="row">
        <label for="perspective-scale">Scale</label>
        <input
          id="perspective-scale"
          type="range"
          min="50"
          max="150"
          step="1"
          value={perspective.scale}
          oninput={(e) => onPerspectiveChange({ scale: Number(e.currentTarget.value) })}
        />
        <span class="val">{perspective.scale}</span>
      </div>
      <!-- No auto-crop here on purpose -- Vertical/Horizontal/Rotate can
           reveal blank corners; the existing Crop tool (Develop's own
           tool strip) is the manual way to trim them, matching this
           slice's "manual controls only" scope (see
           develop_engine.rs's own header comment on the `perspective`
           op). -->
      <p class="preset-note">Use the Crop tool to trim any blank corners this introduces.</p>
    </div>
  </details>

  <details class="section">
    <summary>Soft Proof</summary>
    <div class="sub-body">
      <label class="checkbox-row">
        <input
          type="checkbox"
          checked={softProofEnabled}
          onchange={(e) => onSoftProofEnabledChange?.(e.currentTarget.checked)}
        />
        Enable Soft Proofing
      </label>
      <div class="row">
        <label for="soft-proof-target">Profile</label>
        <select
          id="soft-proof-target"
          class="select-input"
          value={softProofTarget}
          onchange={(e) => {
            const value = e.currentTarget.value;
            if (value === "custom") {
              onChooseCustomProfile?.();
            } else {
              onSoftProofTargetChange?.(value);
            }
          }}
        >
          <option value="srgb">sRGB</option>
          <option value="adobe-rgb">Adobe RGB</option>
          <option value="prophoto-rgb">ProPhoto RGB</option>
          <option value="custom">Custom Profile…</option>
        </select>
      </div>
      {#if softProofTarget === "custom"}
        <div class="row">
          <label for="soft-proof-custom-path">File</label>
          <button
            id="soft-proof-custom-path"
            type="button"
            class="preset-action-btn"
            onclick={onChooseCustomProfile}
            title={softProofCustomProfilePath ?? "No profile chosen"}
          >
            {softProofCustomProfilePath ? softProofCustomProfilePath.split(/[\\/]/).pop() : "Choose File…"}
          </button>
        </div>
      {/if}
      <div class="row">
        <label for="soft-proof-intent">Rendering Intent</label>
        <select
          id="soft-proof-intent"
          class="select-input"
          value={softProofIntent}
          onchange={(e) => onSoftProofIntentChange?.(e.currentTarget.value)}
        >
          <option value="perceptual">Perceptual</option>
          <option value="relative">Relative Colorimetric</option>
          <option value="saturation">Saturation</option>
          <option value="absolute">Absolute Colorimetric</option>
        </select>
      </div>
      <label class="checkbox-row">
        <input
          type="checkbox"
          checked={softProofGamutWarning}
          onchange={(e) => onSoftProofGamutWarningChange?.(e.currentTarget.checked)}
        />
        Show Gamut Warning
      </label>
      <p class="preset-note">
        Simulates how the current edit will look when rendered on the selected output profile. Out-of-gamut colors
        are approximated (or, with Gamut Warning on, flagged in gray) the way they would actually be clipped by
        that device.
      </p>
    </div>
  </details>
  </div>

  <div class="panel-footer">
    <button type="button" class="preset-action-btn" onclick={onCopySettingsRequest}>Copy Settings…</button>
    <button type="button" class="preset-action-btn" onclick={onPasteSettingsRequest} disabled={!canPasteSettings}>
      Paste Settings…
    </button>
  </div>
</div>

<style>
  .panel {
    width: 240px;
    flex: none;
    display: flex;
    flex-direction: column;
    background: var(--bg-panel);
    border-left: 1px solid var(--border-subtle);
    overflow: hidden;
  }
  /* Everything scrolls except .panel-footer below -- Copy/Paste Settings
     needs to stay reachable without scrolling past every adjustment
     section (user feedback: it was buried at the bottom of the scroll
     as a collapsible section originally). */
  .panel-scroll {
    flex: 1 1 auto;
    min-height: 0;
    overflow-y: auto;
    overflow-x: hidden;
    padding: 14px 12px;
  }
  .panel-footer {
    flex: none;
    display: flex;
    gap: 8px;
    padding: 10px 12px;
    border-top: 1px solid var(--border-subtle);
  }
  .panel-footer .preset-action-btn {
    flex: 1 1 0;
    width: auto;
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
  .subsection-label {
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--text-tertiary);
    padding: 8px 4px 2px;
  }
  .checkbox-row {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 4px 8px;
    font-size: 11px;
    color: var(--text-secondary);
    cursor: pointer;
  }
  .subsection-label:first-child {
    padding-top: 2px;
  }
  .static-note {
    color: var(--text-tertiary);
    font-size: 11px;
    padding: 4px 4px 12px;
  }
  .preset-actions {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 2px 4px 8px;
  }
  .preset-action-btn {
    all: unset;
    box-sizing: border-box;
    width: 100%;
    cursor: pointer;
    padding: 6px 8px;
    font-size: 11px;
    font-weight: 600;
    border-radius: var(--radius-s);
    color: var(--accent-strong);
    background: var(--accent-soft);
    text-align: center;
  }
  .preset-action-btn:disabled {
    cursor: default;
    color: var(--text-tertiary);
    background: transparent;
    opacity: 0.6;
  }
  .preset-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .preset-row {
    display: flex;
    align-items: center;
    gap: 2px;
  }
  .preset-name-btn {
    all: unset;
    box-sizing: border-box;
    flex: 1;
    min-width: 0;
    cursor: pointer;
    padding: 5px 4px;
    font-size: 12px;
    color: var(--text-secondary);
    border-radius: var(--radius-s);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .preset-name-btn:hover {
    background: var(--bg-panel-raised);
    color: var(--text-primary);
  }
  .preset-row-actions {
    display: flex;
    gap: 0;
    flex: none;
  }
  .preset-icon-btn {
    all: unset;
    cursor: pointer;
    padding: 3px 6px;
    font-size: 12px;
    line-height: 1;
    border-radius: var(--radius-s);
    color: var(--text-tertiary);
  }
  .preset-icon-btn:hover {
    color: var(--accent-strong);
    background: var(--accent-soft);
  }
  .preset-icon-btn.delete:hover {
    color: var(--label-red);
  }
  .preset-note {
    margin: 8px 4px 0;
    font-size: 10.5px;
    line-height: 1.4;
    color: var(--text-tertiary);
    font-style: italic;
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
  .subsection-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 6px 4px 4px;
    margin-bottom: 2px;
  }
  .subsection-title {
    font-size: 10px;
    font-weight: 600;
    font-family: var(--font-mono);
    color: var(--text-tertiary);
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }
  .header-actions {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .auto-btn {
    all: unset;
    cursor: pointer;
    flex: none;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 20px;
    color: var(--accent-strong);
    background: var(--accent-soft);
    border: 1px solid var(--accent);
    border-radius: var(--radius-s);
    transition: all 0.12s ease;
  }
  .auto-btn:hover {
    filter: brightness(1.15);
    background: var(--accent);
    color: #fff;
  }
  .select-input {
    flex: 1;
    background: var(--bg-panel-raised);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-s);
    color: var(--text-primary);
    font-size: 11px;
    font-family: inherit;
    padding: 3px 6px;
    outline: none;
  }
  .select-input:focus {
    border-color: var(--accent);
  }
</style>
