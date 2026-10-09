<script>
  import { convertFileSrc } from "@tauri-apps/api/core";

  /**
   * The before / after of a crop preview (RFC-0027 §3.4): the same rectangle of the photo before and after
   * the model, split at a draggable divider. Shown at 1:1 by default, because the point is to judge noise
   * and texture at pixel level, which a fitted view hides; "Fit" shows the whole rectangle.
   * @type {{
   *   before: string,
   *   after: string,
   *   width: number,
   *   height: number,
   *   seconds: number,
   *   onClose: () => void,
   * }}
   */
  let { before, after, width, height, seconds, onClose } = $props();

  let split = $state(50);
  let fit = $state(false);
</script>

<div class="compare" data-testid="ai-denoise-compare">
  <div class="stage" class:fit style={fit ? `aspect-ratio: ${width} / ${height}` : ""}>
    <img class="img after" src={convertFileSrc(after)} alt="After denoise" draggable="false" />
    <img class="img before" src={convertFileSrc(before)} alt="Before denoise" draggable="false" style="clip-path: inset(0 {100 - split}% 0 0)" />
    <span class="tag left">Before</span>
    <span class="tag right">After</span>
    <div class="divider" style="left: {split}%"></div>
  </div>
  <input class="slider" type="range" min="0" max="100" step="1" bind:value={split} aria-label="Before and after divider" />
  <div class="foot">
    <span class="cap">{width}×{height} crop, full strength, {seconds} s</span>
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
    padding: 4px 4px 6px;
  }
  .stage {
    position: relative;
    height: 200px;
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
  }
  .foot {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
  }
  .cap {
    font-size: 10px;
    color: var(--text-tertiary);
  }
  .actions {
    display: flex;
    gap: 10px;
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
