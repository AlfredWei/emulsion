<script>
  import { convertFileSrc } from "@tauri-apps/api/core";
  import { regionFractions, centreFromClick } from "$lib/enlarge.js";

  /**
   * The Preview of Enlarge (AI) (RFC-0029 §3.4): the same rectangle of the photo as a plain resize on the left and,
   * on the right, the model blended over it at the chosen Strength (`plain + (model - plain) * strength`, which is
   * alpha compositing, so the slider needs no new run). Shown at 1:1 because the point is to judge invented detail
   * at pixel level; Fit shows the whole rectangle. The navigator is the photo's thumbnail with the rectangle drawn
   * on it; a click there asks for a preview somewhere else.
   * @type {{
   *   preview: import('$lib/api/upscale.js').UpscalePreview,
   *   strength: number,
   *   sourceWidth: number,
   *   sourceHeight: number,
   *   thumbnail: string | null,
   *   busy: boolean,
   *   onMove: (centre: [number, number]) => void,
   *   onClose: () => void,
   * }}
   */
  let { preview, strength, sourceWidth, sourceHeight, thumbnail, busy, onMove, onClose } = $props();

  let split = $state(50);
  let fit = $state(false);
  let rect = $derived(regionFractions(preview.region, sourceWidth, sourceHeight));

  /** @param {MouseEvent} e */
  function navigate(e) {
    const box = /** @type {HTMLElement} */ (e.currentTarget).getBoundingClientRect();
    onMove(centreFromClick(e.clientX - box.left, e.clientY - box.top, box.width, box.height));
  }
</script>

<div class="compare" data-testid="enlarge-compare" class:busy>
  <div class="stage" class:fit style={fit ? `aspect-ratio: ${preview.width} / ${preview.height}` : ""}>
    <img class="img" src={convertFileSrc(preview.before_path)} alt="Plain resize" draggable="false" />
    <img
      class="img"
      src={convertFileSrc(preview.after_path)}
      alt="Enlarged with AI at strength {strength}"
      draggable="false"
      style="opacity: {strength / 100}; clip-path: inset(0 0 0 {split}%)"
    />
    <span class="tag left">Plain resize</span>
    <span class="tag right">AI, strength {strength}</span>
    <div class="divider" style="left: {split}%"></div>
  </div>
  <input class="slider" type="range" min="0" max="100" step="1" bind:value={split} aria-label="Plain and AI divider" />
  <div class="foot">
    {#if thumbnail}
      <button class="nav" type="button" title="Click to move the preview region" aria-label="Move the preview region" onclick={navigate}>
        <img src={convertFileSrc(thumbnail)} alt="" draggable="false" />
        <i style="left: {rect.left * 100}%; top: {rect.top * 100}%; width: {Math.max(rect.width * 100, 2)}%; height: {Math.max(rect.height * 100, 2)}%"></i>
      </button>
    {/if}
    <span class="cap">
      {preview.width}×{preview.height} px at 1:1, {Math.max(1, Math.round(preview.ms / 1000))} s. The right side is the model blended with the plain resize at the chosen Strength.
    </span>
    <span class="actions">
      <button class="link-btn" type="button" onclick={() => (fit = !fit)}>{fit ? "1:1" : "Fit"}</button>
      <button class="link-btn" type="button" onclick={onClose}>Close</button>
    </span>
  </div>
</div>

<style>
  .compare {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .compare.busy {
    opacity: 0.6;
  }
  .stage {
    position: relative;
    height: 220px;
    overflow: hidden;
    background: var(--bg-canvas, #111);
    border: 1px solid var(--border-subtle);
    border-radius: 4px;
  }
  .stage.fit {
    height: auto;
    width: 100%;
  }
  .img {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: none;
    object-position: center;
    user-select: none;
  }
  .stage.fit .img {
    object-fit: contain;
  }
  .tag {
    position: absolute;
    top: 4px;
    padding: 1px 5px;
    font-size: 9.5px;
    border-radius: 3px;
    background: rgba(0, 0, 0, 0.55);
    color: #fff;
    pointer-events: none;
  }
  .tag.left {
    left: 4px;
  }
  .tag.right {
    right: 4px;
  }
  .divider {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 1px;
    background: rgba(255, 255, 255, 0.85);
    pointer-events: none;
  }
  .slider {
    width: 100%;
    margin: 0;
    accent-color: var(--accent);
  }
  .foot {
    display: flex;
    align-items: flex-start;
    gap: 8px;
  }
  .nav {
    all: unset;
    position: relative;
    flex: none;
    width: 64px;
    cursor: crosshair;
    border: 1px solid var(--border-subtle);
    border-radius: 3px;
    overflow: hidden;
  }
  .nav:focus-visible {
    outline: 2px solid var(--accent);
  }
  .nav img {
    display: block;
    width: 100%;
    height: auto;
  }
  .nav i {
    position: absolute;
    border: 1px solid #fff;
    box-shadow: 0 0 0 1px rgba(0, 0, 0, 0.7);
    background: rgba(255, 255, 255, 0.15);
    box-sizing: border-box;
    pointer-events: none;
  }
  .cap {
    flex: 1;
    font-size: 10px;
    line-height: 1.35;
    color: var(--text-tertiary);
  }
  .actions {
    display: flex;
    flex-direction: column;
    gap: 4px;
    align-items: flex-end;
  }
  .link-btn {
    appearance: none;
    background: none;
    border: none;
    padding: 0;
    font: 10.5px var(--font-ui);
    color: var(--accent-strong);
    cursor: pointer;
  }
  .link-btn:hover {
    text-decoration: underline;
  }
</style>
