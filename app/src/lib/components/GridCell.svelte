<script>
  import { convertFileSrc } from "@tauri-apps/api/core";

  /**
   * The MouseEvent reaches onSelect so multi-select modifiers (Cmd/Ctrl,
   * Shift) can be read by the page-level selection logic -- the keyboard
   * Enter/Space path passes no event, which means "plain select" there.
   * @type {{
   *   image: import('$lib/api/catalog.js').ImageSummary,
   *   selected: boolean,
   *   onSelect: (event?: MouseEvent) => void,
   *   onOpen: () => void,
   *   onRatingChange: (rating: number) => void,
   *   onFlagChange: (flag: string) => void,
   *   onColorLabelChange: (colorLabel: string) => void,
   * }}
   */
  let { image, selected, onSelect, onOpen, onRatingChange, onFlagChange, onColorLabelChange } =
    $props();

  const COLOR_CYCLE = ["none", "red", "yellow", "green", "blue", "purple"];

  let thumbSrc = $derived(image.thumbnail_path ? convertFileSrc(image.thumbnail_path) : null);

  function toggleFlag(/** @type {MouseEvent} */ e, /** @type {string} */ target) {
    e.stopPropagation();
    onFlagChange(image.flag === target ? "none" : target);
  }

  function setRating(/** @type {MouseEvent} */ e, /** @type {number} */ n) {
    e.stopPropagation();
    // clicking the currently-set star again clears the rating, matching
    // Lightroom's own star-rating toggle behavior
    onRatingChange(image.rating === n ? 0 : n);
  }

  function cycleColorLabel(/** @type {MouseEvent} */ e) {
    e.stopPropagation();
    const i = COLOR_CYCLE.indexOf(image.color_label);
    const next = COLOR_CYCLE[(Math.max(i, 0) + 1) % COLOR_CYCLE.length];
    onColorLabelChange(next);
  }
</script>

<div
  class="cell"
  class:selected
  role="button"
  tabindex="0"
  onclick={(e) => onSelect(e)}
  ondblclick={onOpen}
  onkeydown={(e) => (e.key === "Enter" || e.key === " ") && onSelect()}
>
  {#if thumbSrc}
    <img class="thumb" src={thumbSrc} alt="" loading="lazy" />
  {:else}
    <div class="thumb placeholder" aria-hidden="true"></div>
  {/if}

  <div class="badge-row">
    <div class="stars">
      {#each [1, 2, 3, 4, 5] as n (n)}
        <button
          class="star"
          class:on={n <= image.rating}
          type="button"
          title="Rate {n} star{n === 1 ? '' : 's'} ({n})"
          onclick={(e) => setRating(e, n)}
        >★</button>
      {/each}
    </div>
    <button
      class="flag"
      class:pick={image.flag === "pick"}
      class:reject={image.flag === "reject"}
      type="button"
      title="Pick (P)"
      onclick={(e) => toggleFlag(e, "pick")}
    >✓</button>
    <button
      class="flag"
      class:reject={image.flag === "reject"}
      type="button"
      title="Reject (X)"
      onclick={(e) => toggleFlag(e, "reject")}
    >✕</button>
    <button
      class="color-dot"
      style={image.color_label !== "none" ? `background: var(--label-${image.color_label})` : ""}
      type="button"
      title="Color label (6-9 red/yellow/green/blue)"
      onclick={cycleColorLabel}
    ></button>
  </div>
</div>

<style>
  .cell {
    all: unset;
    position: relative;
    display: block;
    aspect-ratio: 3 / 2;
    border-radius: var(--radius-s);
    overflow: hidden;
    border: 1px solid transparent;
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.5);
    cursor: pointer;
  }
  .cell.selected {
    border-color: var(--accent);
    box-shadow: 0 0 0 1px var(--accent);
  }
  .cell:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .thumb {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .thumb.placeholder {
    background: var(--bg-panel-raised);
  }
  .badge-row {
    position: absolute;
    left: 5px;
    right: 5px;
    bottom: 5px;
    display: flex;
    align-items: center;
    gap: 6px;
    font-family: var(--font-mono);
    font-size: 11px;
    color: #fff;
    opacity: 0;
    transition: opacity 0.12s ease;
  }
  .cell:hover .badge-row,
  .cell.selected .badge-row {
    opacity: 1;
  }
  .stars {
    display: flex;
    gap: 1px;
    text-shadow: 0 1px 2px rgba(0, 0, 0, 0.7);
  }
  .star {
    all: unset;
    opacity: 0.35;
    cursor: pointer;
    line-height: 1;
  }
  .star.on {
    opacity: 1;
    color: var(--accent-strong);
  }
  .flag {
    all: unset;
    cursor: pointer;
    opacity: 0.6;
    text-shadow: 0 1px 2px rgba(0, 0, 0, 0.7);
    line-height: 1;
  }
  .flag.pick {
    opacity: 1;
    color: var(--label-green);
  }
  .flag.reject {
    opacity: 1;
    color: var(--label-red);
  }
  .color-dot {
    all: unset;
    cursor: pointer;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: rgba(255, 255, 255, 0.25);
    box-shadow: 0 0 0 1px rgba(0, 0, 0, 0.4);
    margin-left: auto;
  }
</style>
