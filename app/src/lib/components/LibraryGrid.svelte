<script>
  // Hand-rolled virtualized grid (see docs/ux/UX-DESIGN.md §5: grid DOM
  // cost must stay independent of catalog size, e.g. a 50k-image catalog
  // shouldn't cost more DOM than a 500-image one). Not pulling in a
  // virtualization library: this is a uniform grid (fixed aspect-ratio
  // cells), simple enough to compute the visible row range directly from
  // scroll position without one.

  import GridCell from "./GridCell.svelte";

  /**
   * @type {{
   *   images: import('$lib/api/catalog.js').ImageSummary[],
   *   selectedId: number | null,
   *   onSelect: (versionId: number) => void,
   *   onOpen: (versionId: number) => void,
   *   onRatingChange: (versionId: number, rating: number) => void,
   *   onFlagChange: (versionId: number, flag: string) => void,
   *   onColorLabelChange: (versionId: number, colorLabel: string) => void,
   * }}
   */
  let { images, selectedId, onSelect, onOpen, onRatingChange, onFlagChange, onColorLabelChange } =
    $props();

  const CELL_MIN_WIDTH = 140;
  const GAP = 10;
  const ROW_BUFFER = 3; // extra rows rendered above/below the viewport

  let containerEl = $state(/** @type {HTMLDivElement | null} */ (null));
  let containerWidth = $state(0);
  let containerHeight = $state(0);
  let scrollTop = $state(0);

  let columns = $derived(Math.max(1, Math.floor((containerWidth + GAP) / (CELL_MIN_WIDTH + GAP))));
  let cellWidth = $derived(columns > 0 ? (containerWidth - GAP * (columns - 1)) / columns : CELL_MIN_WIDTH);
  let rowHeight = $derived(cellWidth * (2 / 3) + GAP);
  let totalRows = $derived(Math.ceil(images.length / columns));
  let totalHeight = $derived(totalRows * rowHeight);

  let startRow = $derived(Math.max(0, Math.floor(scrollTop / rowHeight) - ROW_BUFFER));
  let endRow = $derived(
    Math.min(totalRows, Math.ceil((scrollTop + containerHeight) / rowHeight) + ROW_BUFFER),
  );

  let visibleImages = $derived(images.slice(startRow * columns, endRow * columns));
  let offsetTop = $derived(startRow * rowHeight);

  function handleScroll() {
    if (containerEl) scrollTop = containerEl.scrollTop;
  }
</script>

<div
  class="grid-scroll"
  bind:this={containerEl}
  bind:clientWidth={containerWidth}
  bind:clientHeight={containerHeight}
  onscroll={handleScroll}
>
  <div class="phantom" style="height: {totalHeight}px">
    <div
      class="visible-rows"
      style="top: {offsetTop}px; display: grid; grid-template-columns: repeat({columns}, 1fr); gap: {GAP}px;"
    >
      {#each visibleImages as image (image.version_id)}
        <GridCell
          {image}
          selected={image.version_id === selectedId}
          onSelect={() => onSelect(image.version_id)}
          onOpen={() => onOpen(image.version_id)}
          onRatingChange={(rating) => onRatingChange(image.version_id, rating)}
          onFlagChange={(flag) => onFlagChange(image.version_id, flag)}
          onColorLabelChange={(colorLabel) => onColorLabelChange(image.version_id, colorLabel)}
        />
      {/each}
    </div>
  </div>
</div>

<style>
  .grid-scroll {
    flex: 1;
    overflow-y: auto;
    padding: 14px;
    min-height: 0;
  }
  .phantom {
    position: relative;
    width: 100%;
  }
  .visible-rows {
    position: absolute;
    left: 0;
    right: 0;
  }
</style>
