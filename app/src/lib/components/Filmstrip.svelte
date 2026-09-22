<script>
  import { convertFileSrc } from "@tauri-apps/api/core";
  import { shell } from "$lib/state/shell.svelte.js";

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
   *
   * Cells are also drag sources for the world map (M5.5 map view, RFC-0007): this is the one photo
   * strip still visible while the Map is Library's active view (Grid isn't, so it can't be the
   * source), so dragging a filmstrip photo onto a map point is how "drag a photo onto the map" is
   * reachable at all -- `LibraryMapView` is the only current consumer of `shell.photoDrag`. Dragging
   * a cell that's part of the current multi-selection carries the whole selection (by `image_id`,
   * de-duplicating virtual copies of the same photo); dragging any other cell carries just that one
   * photo, selected or not.
   *
   * This is plain pointer events routed through `shell.photoDrag`, not native HTML5 drag-and-drop --
   * see that store's own doc comment for why (Tauri's window-level drag-drop interception, needed for
   * real OS file imports, stops the browser's native drag events from firing for a drag that never
   * leaves the page). Reading `shell` directly here, rather than adding three more callback props
   * threaded through both `+page.svelte` call sites, matches `panelResizeState`'s precedent: this is a
   * plain cross-component pointer-interaction state, and Develop's own Filmstrip renders the same
   * handlers inertly (there's no map open to drop onto, so nothing ever reacts to it there).
   * @type {{
   *   images: import('$lib/api/catalog.js').ImageSummary[],
   *   selectedIds: Set<number>,
   *   onSelect: (versionId: number, event?: MouseEvent) => void,
   *   onOpen: (versionId: number) => void,
   * }}
   */
  let { images, selectedIds, onSelect, onOpen } = $props();

  /** @param {import('$lib/api/catalog.js').ImageSummary} image */
  function dragImageIds(image) {
    return selectedIds.has(image.version_id) && selectedIds.size > 1
      ? [...new Set(images.filter((img) => selectedIds.has(img.version_id)).map((img) => img.image_id))]
      : [image.image_id];
  }
</script>

<div class="filmstrip">
  {#each images as image (image.version_id)}
    {@const thumbSrc = image.thumbnail_path ? convertFileSrc(image.thumbnail_path) : null}
    <div
      class="cell"
      class:selected={selectedIds.has(image.version_id)}
      role="button"
      tabindex="0"
      onpointerdown={(e) => shell.handlePhotoDragPointerDown(e, dragImageIds(image))}
      onpointermove={shell.handlePhotoDragPointerMove}
      onpointerup={shell.handlePhotoDragPointerUp}
      onclick={(e) => onSelect(image.version_id, e)}
      ondblclick={() => onOpen(image.version_id)}
      onkeydown={(e) => (e.key === "Enter" || e.key === " ") && onSelect(image.version_id)}
    >
      {#if thumbSrc}
        <img class="thumb" src={thumbSrc} alt="" loading="lazy" draggable="false" />
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
    touch-action: none;
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
