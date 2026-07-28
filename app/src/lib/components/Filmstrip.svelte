<script>
  import { convertFileSrc } from "@tauri-apps/api/core";

  /**
   * Persistent filmstrip, shared between Library and Develop
   * (docs/ux/UX-DESIGN.md §2/§4) -- shows the current filtered image set
   * so context isn't lost switching between culling and editing. Same
   * selection-dumb prop contract as LibraryGrid.svelte; the two call
   * sites (+page.svelte) supply different onSelect/onOpen behavior
   * (select vs. switch-the-open-Develop-image) rather than this
   * component knowing which module it's in.
   *
   * Cells are thumbnail-only, no flag/star/color-label badge row --
   * matches the reviewed mockup's own explicit simplification for
   * filmstrip cells specifically (distinct from GridCell.svelte, which
   * keeps its badge row for the main grid).
   * @type {{
   *   images: import('$lib/api/catalog.js').ImageSummary[],
   *   selectedIds: Set<number>,
   *   onSelect: (versionId: number, event?: MouseEvent) => void,
   *   onOpen: (versionId: number) => void,
   * }}
   */
  let { images, selectedIds, onSelect, onOpen } = $props();
</script>

<div class="filmstrip">
  {#each images as image (image.version_id)}
    {@const thumbSrc = image.thumbnail_path ? convertFileSrc(image.thumbnail_path) : null}
    <div
      class="cell"
      class:selected={selectedIds.has(image.version_id)}
      role="button"
      tabindex="0"
      onclick={(e) => onSelect(image.version_id, e)}
      ondblclick={() => onOpen(image.version_id)}
      onkeydown={(e) => (e.key === "Enter" || e.key === " ") && onSelect(image.version_id)}
    >
      {#if thumbSrc}
        <img class="thumb" src={thumbSrc} alt="" loading="lazy" />
      {:else}
        <div class="thumb placeholder" aria-hidden="true"></div>
      {/if}
    </div>
  {/each}
</div>

<style>
  .filmstrip {
    flex: none;
    display: flex;
    gap: 7px;
    padding: 10px 14px;
    background: var(--bg-app);
    border-top: 1px solid var(--border-subtle);
    overflow-x: auto;
    overflow-y: hidden;
  }
  .cell {
    all: unset;
    position: relative;
    flex: none;
    width: 76px;
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
</style>
