<script>
  import { PAPER_SIZES } from "$lib/api/print.js";

  /**
   * @type {{
   *   itemCount: number,
   *   template: "single" | "contact-sheet",
   *   onTemplateChange: (v: "single" | "contact-sheet") => void,
   *   fitMode: "fit" | "fill",
   *   onFitModeChange: (v: "fit" | "fill") => void,
   *   rows: number,
   *   cols: number,
   *   onRowsChange: (v: number) => void,
   *   onColsChange: (v: number) => void,
   *   cellSpacing: number,
   *   onCellSpacingChange: (v: number) => void,
   *   paperSize: string,
   *   onPaperSizeChange: (v: string) => void,
   *   orientation: "portrait" | "landscape",
   *   onOrientationChange: (v: "portrait" | "landscape") => void,
   *   margins: {top: number, right: number, bottom: number, left: number},
   *   onMarginChange: (side: "top" | "right" | "bottom" | "left", v: number) => void,
   *   colorManaged: boolean,
   *   onColorManagedChange: (v: boolean) => void,
   *   profileTarget: "srgb" | "adobe-rgb" | "prophoto-rgb" | "custom",
   *   onProfileTargetChange: (v: string) => void,
   *   customProfilePath: string | null,
   *   onChooseCustomProfile: () => void,
   *   intent: "perceptual" | "relative" | "saturation" | "absolute",
   *   onIntentChange: (v: string) => void,
   *   printing: boolean,
   *   onPrint: () => void,
   * }}
   */
  let {
    itemCount,
    template,
    onTemplateChange,
    fitMode,
    onFitModeChange,
    rows,
    cols,
    onRowsChange,
    onColsChange,
    cellSpacing,
    onCellSpacingChange,
    paperSize,
    onPaperSizeChange,
    orientation,
    onOrientationChange,
    margins,
    onMarginChange,
    colorManaged,
    onColorManagedChange,
    profileTarget,
    onProfileTargetChange,
    customProfilePath,
    onChooseCustomProfile,
    intent,
    onIntentChange,
    printing,
    onPrint,
  } = $props();
</script>

<div class="panel">
  <div class="panel-header">
    <span class="panel-title">PRINT</span>
  </div>

  <details class="section" open>
    <summary>Layout</summary>
    <div class="sub-body">
      <div class="row">
        <label for="print-template">Template</label>
        <select
          id="print-template"
          class="select-input"
          value={template}
          onchange={(e) => onTemplateChange(/** @type {"single" | "contact-sheet"} */ (e.currentTarget.value))}
        >
          <option value="single">Single Image</option>
          <option value="contact-sheet">Contact Sheet</option>
        </select>
      </div>
      {#if template === "single"}
        <div class="row">
          <label for="print-fit-mode">Fit</label>
          <select
            id="print-fit-mode"
            class="select-input"
            value={fitMode}
            onchange={(e) => onFitModeChange(/** @type {"fit" | "fill"} */ (e.currentTarget.value))}
          >
            <option value="fit">Fit Within Margins</option>
            <option value="fill">Fill Margins</option>
          </select>
        </div>
      {:else}
        <div class="row">
          <label for="print-rows">Rows</label>
          <input
            id="print-rows"
            class="num-input"
            type="number"
            min="1"
            max="10"
            value={rows}
            onchange={(e) => onRowsChange(Math.max(1, Number(e.currentTarget.value) || 1))}
          />
        </div>
        <div class="row">
          <label for="print-cols">Columns</label>
          <input
            id="print-cols"
            class="num-input"
            type="number"
            min="1"
            max="10"
            value={cols}
            onchange={(e) => onColsChange(Math.max(1, Number(e.currentTarget.value) || 1))}
          />
        </div>
        <div class="row">
          <label for="print-cell-spacing">Spacing (in)</label>
          <input
            id="print-cell-spacing"
            class="num-input"
            type="number"
            min="0"
            max="2"
            step="0.05"
            value={cellSpacing}
            onchange={(e) => onCellSpacingChange(Math.max(0, Number(e.currentTarget.value) || 0))}
          />
        </div>
        <p class="preset-note">{itemCount} photo{itemCount === 1 ? "" : "s"} in this print job, grid order.</p>
      {/if}
    </div>
  </details>

  <details class="section" open>
    <summary>Page Setup</summary>
    <div class="sub-body">
      <div class="row">
        <label for="print-paper-size">Paper Size</label>
        <select
          id="print-paper-size"
          class="select-input"
          value={paperSize}
          onchange={(e) => onPaperSizeChange(e.currentTarget.value)}
        >
          {#each Object.entries(PAPER_SIZES) as [key, size] (key)}
            <option value={key}>{size.name}</option>
          {/each}
        </select>
      </div>
      <div class="row">
        <label for="print-orientation">Orientation</label>
        <select
          id="print-orientation"
          class="select-input"
          value={orientation}
          onchange={(e) => onOrientationChange(/** @type {"portrait" | "landscape"} */ (e.currentTarget.value))}
        >
          <option value="portrait">Portrait</option>
          <option value="landscape">Landscape</option>
        </select>
      </div>
      <div class="row">
        <label for="print-margin-top">Margins (in)</label>
        <input
          id="print-margin-top"
          class="num-input margin-input"
          type="number"
          min="0"
          max="4"
          step="0.1"
          value={margins.top}
          title="Top"
          onchange={(e) => onMarginChange("top", Math.max(0, Number(e.currentTarget.value) || 0))}
        />
        <input
          class="num-input margin-input"
          type="number"
          min="0"
          max="4"
          step="0.1"
          value={margins.right}
          title="Right"
          onchange={(e) => onMarginChange("right", Math.max(0, Number(e.currentTarget.value) || 0))}
        />
        <input
          class="num-input margin-input"
          type="number"
          min="0"
          max="4"
          step="0.1"
          value={margins.bottom}
          title="Bottom"
          onchange={(e) => onMarginChange("bottom", Math.max(0, Number(e.currentTarget.value) || 0))}
        />
        <input
          class="num-input margin-input"
          type="number"
          min="0"
          max="4"
          step="0.1"
          value={margins.left}
          title="Left"
          onchange={(e) => onMarginChange("left", Math.max(0, Number(e.currentTarget.value) || 0))}
        />
      </div>
      <p class="preset-note">
        This drives the on-screen layout. The OS print dialog is authoritative for the paper actually loaded in your
        printer — keep the two consistent.
      </p>
    </div>
  </details>

  <details class="section" open>
    <summary>Color Management</summary>
    <div class="sub-body">
      <label class="checkbox-row">
        <input type="checkbox" checked={colorManaged} onchange={(e) => onColorManagedChange(e.currentTarget.checked)} />
        Managed by Printer Profile
      </label>
      {#if colorManaged}
        <div class="row">
          <label for="print-profile-target">Profile</label>
          <select
            id="print-profile-target"
            class="select-input"
            value={profileTarget}
            onchange={(e) => {
              const value = e.currentTarget.value;
              if (value === "custom") {
                onChooseCustomProfile();
              } else {
                onProfileTargetChange(value);
              }
            }}
          >
            <option value="srgb">sRGB</option>
            <option value="adobe-rgb">Adobe RGB</option>
            <option value="prophoto-rgb">ProPhoto RGB</option>
            <option value="custom">Custom Profile…</option>
          </select>
        </div>
        {#if profileTarget === "custom"}
          <div class="row">
            <label for="print-custom-profile-path">File</label>
            <button
              id="print-custom-profile-path"
              type="button"
              class="preset-action-btn"
              onclick={onChooseCustomProfile}
              title={customProfilePath ?? "No profile chosen"}
            >
              {customProfilePath ? customProfilePath.split(/[\\/]/).pop() : "Choose File…"}
            </button>
          </div>
        {/if}
        <div class="row">
          <label for="print-intent">Rendering Intent</label>
          <select
            id="print-intent"
            class="select-input"
            value={intent}
            onchange={(e) => onIntentChange(e.currentTarget.value)}
          >
            <option value="perceptual">Perceptual</option>
            <option value="relative">Relative Colorimetric</option>
            <option value="saturation">Saturation</option>
            <option value="absolute">Absolute Colorimetric</option>
          </select>
        </div>
      {:else}
        <p class="preset-note">
          Your printer's own driver handles color conversion — the usual default, and the simplest choice for most
          printers.
        </p>
      {/if}
    </div>
  </details>

  <div class="print-actions">
    <button class="print-btn" type="button" disabled={itemCount === 0 || printing} onclick={onPrint}>
      {printing ? "Preparing…" : "Print…"}
    </button>
  </div>
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
  .section {
    border-bottom: 1px solid var(--border-subtle);
  }
  .section:last-of-type {
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
  .checkbox-row {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 4px 8px;
    font-size: 11px;
    color: var(--text-secondary);
    cursor: pointer;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 5px 4px;
  }
  .row label {
    width: 84px;
    font-size: 11px;
    color: var(--text-secondary);
    flex: none;
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
  .num-input {
    all: unset;
    box-sizing: border-box;
    flex: 1;
    min-width: 0;
    background: var(--bg-panel-raised);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-s);
    color: var(--text-primary);
    font-size: 11px;
    font-family: inherit;
    padding: 3px 6px;
    text-align: right;
  }
  .num-input:focus {
    border-color: var(--accent);
  }
  .margin-input {
    flex: 1;
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
  .preset-note {
    margin: 8px 4px 0;
    font-size: 10.5px;
    line-height: 1.4;
    color: var(--text-tertiary);
    font-style: italic;
  }
  .print-actions {
    padding: 14px 4px 4px;
  }
  .print-btn {
    all: unset;
    box-sizing: border-box;
    width: 100%;
    cursor: pointer;
    padding: 8px;
    font-size: 12px;
    font-weight: 600;
    text-align: center;
    border-radius: var(--radius-s);
    color: var(--accent-on);
    background: var(--accent);
  }
  .print-btn:disabled {
    cursor: default;
    opacity: 0.5;
  }
</style>
