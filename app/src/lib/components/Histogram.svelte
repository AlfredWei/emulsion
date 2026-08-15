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

  /**
   * @type {{
   *   data: {r: Uint32Array, g: Uint32Array, b: Uint32Array} | null,
   * }}
   */
  let { data } = $props();

  const BUCKETS = 256;
  const VIEW_W = 256;
  const VIEW_H = 72;

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
</script>

<div class="histogram" class:empty={!data}>
  {#if data}
    <svg viewBox="0 0 {VIEW_W} {VIEW_H}" preserveAspectRatio="none" aria-label="Histogram">
      <polygon points={rPath} class="ch ch-r" />
      <polygon points={gPath} class="ch ch-g" />
      <polygon points={bPath} class="ch ch-b" />
    </svg>
  {/if}
</div>

<style>
  .histogram {
    height: 72px;
    margin: 10px 10px 4px;
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
</style>
