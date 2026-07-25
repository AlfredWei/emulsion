<script>
  /**
   * @type {{
   *   exposure: number,
   *   contrast: number,
   *   saturation: number,
   *   onExposureChange: (value: number) => void,
   *   onContrastChange: (value: number) => void,
   *   onSaturationChange: (value: number) => void,
   * }}
   */
  let { exposure, contrast, saturation, onExposureChange, onContrastChange, onSaturationChange } =
    $props();

  const STATIC_SECTIONS = [
    { title: "Tone Curve", note: "Point curve editor" },
    { title: "HSL / Color", note: "Per-channel hue · sat · lum" },
    { title: "Detail", note: "Sharpening · noise reduction" },
    { title: "Effects", note: "Grain · vignette" },
    { title: "Lens Corrections", note: "Profile · chromatic aberration" },
  ];
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
</style>
