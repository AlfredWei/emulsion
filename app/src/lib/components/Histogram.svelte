<script>
  // Develop histogram: overlaid R/G/B channel distribution of the
  // CURRENTLY GRADED output (every adjustment applied, not the source
  // file) -- fed live from DevelopCanvas.svelte's own GPU readback (see
  // that file's `readHistogramIfIdle`/`onHistogramUpdate`), which reuses
  // fs_final's existing pipeline/bind group a second time into a small
  // fixed-size target rather than reading back the full-resolution
  // canvas, so this component only ever sees a 256-bucket-per-channel
  // statistical sample -- never raw per-pixel data.
  //
  // Log-scaled bar heights (not linear) -- a real, near-universal
  // convention in every photo editor's histogram, including Lightroom's
  // own: a handful of near-solid-color buckets (e.g. a large sky) would
  // otherwise dwarf every other bucket at linear scale, flattening the
  // rest of the tonal range into invisibility.
  //
  // Channels are drawn with `mix-blend-mode: screen` and pure R/G/B fills
  // deliberately NOT this app's own theme accent colors -- the additive
  // overlap (red+green -> yellow, all three -> white) is what makes a
  // multi-channel histogram legible at a glance, and that convention is
  // independent of the surrounding UI's own color scheme, the same way a
  // color picker's own hue wheel never adapts to the app's theme either.

  import { computeHistogramStats } from "$lib/histogramMath.js";

  /**
   * @type {{
   *   data: {r: Uint32Array, g: Uint32Array, b: Uint32Array} | null,
   *   showClippingOverlay?: boolean,
   *   onToggleClippingOverlay?: () => void,
   *   hoverPixel?: {r: number, g: number, b: number} | null,
   * }}
   */
  let { data, showClippingOverlay = false, onToggleClippingOverlay, hoverPixel = null } = $props();

  const BUCKETS = 256;
  const VIEW_W = 256;
  const VIEW_H = 140;

  // Tone-zone dividers: static (no slider-linkage, purely visual)
  // shadow/darks/lights/highlight boundary markers at the same quarter
  // points Lightroom's own histogram uses -- 25/50/75% of the tonal
  // range, not tied to any of this app's own actual shadow/highlight
  // sliders (which use a different, non-quartile range internally).
  const TONE_ZONE_X = [0.25, 0.5, 0.75].map((f) => f * VIEW_W);

  // A pixel counts as "clipped" once it's within ~1/255 of pure
  // black/white -- matching the same CLIP_EPS threshold DevelopCanvas.svelte's
  // own WGSL clipping-overlay uses (fs_final's Clipping-gated blend), so
  // the corner triangles agree with what the overlay itself would paint.
  const CLIP_BUCKET_MARGIN = 1;

  /** @returns {string} an SVG polygon `points` string tracing the
   * log-scaled outline of one channel, closed along the bottom edge. */
  function channelPath(/** @type {Uint32Array} */ counts, /** @type {number} */ maxLog) {
    if (maxLog <= 0) return `0,${VIEW_H} ${VIEW_W},${VIEW_H}`;
    const points = [];
    for (let i = 0; i < BUCKETS; i++) {
      const x = (i / (BUCKETS - 1)) * VIEW_W;
      const h = (Math.log1p(counts[i]) / maxLog) * VIEW_H;
      points.push(`${x.toFixed(1)},${(VIEW_H - h).toFixed(1)}`);
    }
    return `0,${VIEW_H} ${points.join(" ")} ${VIEW_W},${VIEW_H}`;
  }

  let maxLog = $derived.by(() => {
    if (!data) return 0;
    let max = 0;
    for (let i = 0; i < BUCKETS; i++) {
      max = Math.max(max, data.r[i], data.g[i], data.b[i]);
    }
    return Math.log1p(max);
  });

  let rPath = $derived(data ? channelPath(data.r, maxLog) : "");
  let gPath = $derived(data ? channelPath(data.g, maxLog) : "");
  let bPath = $derived(data ? channelPath(data.b, maxLog) : "");

  /** Sums counts across all three channels within the first/last
   * `CLIP_BUCKET_MARGIN` buckets, so a warning triangle lights up even
   * if clipping shows in only one channel (e.g. a pure-red highlight). */
  function hasClipping(/** @type {Uint32Array[]} */ channels, /** @type {number[]} */ buckets) {
    for (const counts of channels) {
      for (const i of buckets) {
        if (counts[i] > 0) return true;
      }
    }
    return false;
  }

  let shadowClipped = $derived(
    data ? hasClipping([data.r, data.g, data.b], Array.from({ length: CLIP_BUCKET_MARGIN }, (_, i) => i)) : false,
  );
  let highlightClipped = $derived(
    data
      ? hasClipping(
          [data.r, data.g, data.b],
          Array.from({ length: CLIP_BUCKET_MARGIN }, (_, i) => 255 - i),
        )
      : false,
  );

  let stats = $derived(data ? computeHistogramStats(data) : null);

  /** @param {number} v */
  function pct(v) {
    return `${Math.round((v / 255) * 100)}%`;
  }
</script>

<div class="histogram-panel">
  <div class="histogram" class:empty={!data}>
    {#if data}
      <svg viewBox="0 0 {VIEW_W} {VIEW_H}" preserveAspectRatio="none" aria-label="Histogram">
        {#each TONE_ZONE_X as x (x)}
          <line x1={x} y1="0" x2={x} y2={VIEW_H} class="tone-zone" />
        {/each}
        <polygon points={rPath} class="ch ch-r" />
        <polygon points={gPath} class="ch ch-g" />
        <polygon points={bPath} class="ch ch-b" />
      </svg>
      <button
        type="button"
        class="clip-warning clip-shadow"
        class:active={shadowClipped}
        class:toggled={showClippingOverlay}
        disabled={!onToggleClippingOverlay}
        onclick={() => onToggleClippingOverlay?.()}
        title={onToggleClippingOverlay
          ? showClippingOverlay
            ? "Hide shadow clipping overlay"
            : "Show shadow clipping overlay"
          : "Shadow clipping"}
        aria-label="Toggle shadow clipping overlay"
        aria-pressed={showClippingOverlay}
      >
        <svg viewBox="0 0 10 10"><polygon points="0,0 10,0 0,10" /></svg>
      </button>
      <button
        type="button"
        class="clip-warning clip-highlight"
        class:active={highlightClipped}
        class:toggled={showClippingOverlay}
        disabled={!onToggleClippingOverlay}
        onclick={() => onToggleClippingOverlay?.()}
        title={onToggleClippingOverlay
          ? showClippingOverlay
            ? "Hide highlight clipping overlay"
            : "Show highlight clipping overlay"
          : "Highlight clipping"}
        aria-label="Toggle highlight clipping overlay"
        aria-pressed={showClippingOverlay}
      >
        <svg viewBox="0 0 10 10"><polygon points="10,0 10,10 0,0" /></svg>
      </button>
    {/if}
  </div>
  {#if data && stats}
    <div class="info-row">
      <span class="stat" title="Tonal range (min-max)">{pct(stats.min)}–{pct(stats.max)}</span>
      <span class="stat" title="Mean brightness">avg {pct(stats.mean)}</span>
      {#if hoverPixel}
        <span class="hover-rgb">
          <span class="swatch" style="background: rgb({hoverPixel.r}, {hoverPixel.g}, {hoverPixel.b})"></span>
          R{hoverPixel.r} G{hoverPixel.g} B{hoverPixel.b}
        </span>
      {/if}
    </div>
  {/if}
</div>

<style>
  .histogram-panel {
    margin: 10px 10px 4px;
  }
  .histogram {
    position: relative;
    height: 140px;
    border-radius: var(--radius-s, 4px);
    border: 1px solid var(--border-subtle);
    background: var(--bg-panel-raised);
    overflow: hidden;
  }
  .histogram.empty {
    background: repeating-linear-gradient(
      45deg,
      var(--bg-panel-raised),
      var(--bg-panel-raised) 6px,
      var(--bg-panel) 6px,
      var(--bg-panel) 12px
    );
  }
  svg {
    display: block;
    width: 100%;
    height: 100%;
    mix-blend-mode: normal;
  }
  .ch {
    mix-blend-mode: screen;
  }
  .ch-r {
    fill: rgba(255, 64, 64, 0.85);
  }
  .ch-g {
    fill: rgba(64, 255, 64, 0.85);
  }
  .ch-b {
    fill: rgba(64, 128, 255, 0.85);
  }
  .tone-zone {
    stroke: rgba(255, 255, 255, 0.14);
    stroke-width: 1;
    vector-effect: non-scaling-stroke;
  }

  .clip-warning {
    position: absolute;
    top: 0;
    width: 16px;
    height: 16px;
    padding: 0;
    border: none;
    background: none;
    cursor: pointer;
    opacity: 0.28;
    transition: opacity 0.12s ease;
  }
  .clip-warning.active {
    opacity: 0.85;
  }
  .clip-warning:hover {
    opacity: 1;
  }
  .clip-warning:disabled {
    cursor: default;
  }
  .clip-warning:disabled:hover {
    opacity: 0.28;
  }
  .clip-warning:disabled.active:hover {
    opacity: 0.85;
  }
  .clip-warning.toggled {
    opacity: 1;
    outline: 1px solid rgba(255, 255, 255, 0.4);
    outline-offset: -1px;
  }
  .clip-warning svg {
    width: 100%;
    height: 100%;
    mix-blend-mode: normal;
  }
  .clip-shadow {
    left: 0;
  }
  .clip-shadow polygon {
    fill: #4a90ff;
  }
  .clip-highlight {
    right: 0;
  }
  .clip-highlight polygon {
    fill: #ff4a4a;
  }

  .info-row {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-top: 4px;
    padding: 0 2px;
    font-size: 11px;
    font-variant-numeric: tabular-nums;
    color: var(--text-secondary, rgba(255, 255, 255, 0.6));
  }
  .hover-rgb {
    display: flex;
    align-items: center;
    gap: 4px;
    margin-left: auto;
  }
  .swatch {
    display: inline-block;
    width: 10px;
    height: 10px;
    border-radius: 2px;
    border: 1px solid var(--border-subtle);
  }
</style>
