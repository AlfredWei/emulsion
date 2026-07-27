<script>
  /**
   * @type {{
   *   image: import('$lib/api/catalog.js').ImageSummary | null,
   *   onCaptionChange: (caption: string) => void,
   *   onCopyrightChange: (copyright: string) => void,
   *   onContactChange: (contact: string) => void,
   * }}
   */
  let { image, onCaptionChange, onCopyrightChange, onContactChange } = $props();

  let cameraLine = $derived(
    image ? [image.camera_make, image.camera_model].filter(Boolean).join(" ") || "—" : "—",
  );
  let lensLine = $derived(image?.lens_model || "—");

  // "1/125 · f/8 · ISO 100" -- matches the reviewed mockup's combined
  // Exposure row. shutter_speed is stored in seconds; sub-1s values
  // display as a fraction (the common convention), 1s+ as "Ns".
  function formatShutter(/** @type {number} */ seconds) {
    if (seconds >= 1) return `${seconds}s`;
    const denominator = Math.round(1 / seconds);
    return `1/${denominator}`;
  }
  let exposureLine = $derived.by(() => {
    if (!image) return "—";
    const parts = [];
    if (image.shutter_speed) parts.push(formatShutter(image.shutter_speed));
    if (image.aperture) parts.push(`f/${image.aperture.toFixed(1)}`);
    if (image.iso) parts.push(`ISO ${image.iso}`);
    return parts.length > 0 ? parts.join(" · ") : "—";
  });

  // captured_at is stored as "YYYY-MM-DDTHH:MM:SS[+offset]" -- display as
  // "YYYY-MM-DD HH:MM", matching the mockup's format.
  let capturedLine = $derived(
    image?.captured_at ? image.captured_at.replace("T", " ").slice(0, 16) : "—",
  );
</script>

<div class="panel">
  {#if !image}
    <p class="empty">Select a photo to see its details.</p>
  {:else}
    <div class="section-label">Metadata</div>
    <div class="row"><span class="row-label">Camera</span><span>{cameraLine}</span></div>
    <div class="row"><span class="row-label">Lens</span><span>{lensLine}</span></div>
    <div class="row"><span class="row-label">Exposure</span><span class="mono">{exposureLine}</span></div>
    <div class="row"><span class="row-label">Captured</span><span class="mono">{capturedLine}</span></div>

    <div class="section-label">Info</div>
    <div class="field">
      <label for="md-caption">Caption</label>
      <textarea
        id="md-caption"
        rows="2"
        value={image.caption ?? ""}
        onblur={(e) => onCaptionChange(e.currentTarget.value)}
      ></textarea>
    </div>
    <div class="field">
      <label for="md-copyright">Copyright</label>
      <input
        id="md-copyright"
        type="text"
        value={image.copyright ?? ""}
        onblur={(e) => onCopyrightChange(e.currentTarget.value)}
      />
    </div>
    <div class="field">
      <label for="md-contact">Contact</label>
      <input
        id="md-contact"
        type="text"
        value={image.contact ?? ""}
        onblur={(e) => onContactChange(e.currentTarget.value)}
      />
    </div>
  {/if}
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
  .empty {
    color: var(--text-tertiary);
    font-size: 12px;
    padding: 4px;
  }
  .section-label {
    font-family: var(--font-mono);
    font-size: 10px;
    letter-spacing: 0.09em;
    text-transform: uppercase;
    color: var(--text-tertiary);
    padding: 10px 4px 6px;
    font-weight: 600;
  }
  .row {
    display: flex;
    align-items: baseline;
    gap: 8px;
    padding: 4px 4px;
    font-size: 12px;
  }
  .row-label {
    width: 62px;
    flex: none;
    color: var(--text-secondary);
  }
  .row span {
    color: var(--text-primary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .mono {
    font-family: var(--font-mono);
    font-variant-numeric: tabular-nums;
    font-size: 11px;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 5px 4px;
  }
  .field label {
    font-size: 10.5px;
    color: var(--text-tertiary);
  }
  .field input,
  .field textarea {
    box-sizing: border-box;
    width: 100%;
    padding: 6px 8px;
    font-size: 12px;
    font-family: inherit;
    color: var(--text-primary);
    background: var(--bg-panel-raised);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-s);
    resize: vertical;
  }
  .field input:focus,
  .field textarea:focus {
    outline: 2px solid var(--accent);
    outline-offset: -1px;
  }
</style>
