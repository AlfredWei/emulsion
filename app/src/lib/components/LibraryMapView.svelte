<script>
  import { onMount } from "svelte";
  import { convertFileSrc } from "@tauri-apps/api/core";
  import { geolocatedPoints, clusterPoints, boundsOfPoints, clustersInBounds } from "$lib/mapClusters.js";
  import { openExternalUrl } from "$lib/api/system.js";
  import { IMAGE_IDS_DRAG_MIME } from "$lib/dragTransfer.js";

  /**
   * LibraryMapView: the world map of the photos that have GPS coordinates (M5.5, RFC-0007 §3.3). Pins are
   * grouped into clusters by `mapClusters.js`; clicking one hands its photos' `image_id`s to `onSelectCluster`,
   * which scopes Library to them and shows the grid.
   *
   * Leaflet and its stylesheet are imported when this view first mounts -- never at app start -- so no tile is
   * requested until the person actually opens the map (RFC-0007 §7: the only network access in the app, and
   * only for map tiles). Tiles come from OpenStreetMap's public server; the attribution is required by its
   * tile usage policy.
   * A pin shows a thumbnail once one is available -- the same lazy `ensureThumbnail` fetch Grid/Loupe/Filmstrip
   * use, requested here for whichever photo represents a drawn pin/cluster (`onNeedThumbnail`, at most once per
   * photo per time the map is open). Until then, or for a photo Grid never had reason to generate a thumbnail
   * for, the pin falls back to the plain dot/count it always had. A cluster shows a fanned stack of up to
   * three of its own photos rather than one photo standing in for the group, so "several photos here" reads
   * at a glance at any zoom -- the same shape at every scale, since `clusterPoints` itself is what changes
   * per zoom, not this rendering.
   *
   * Placing (RFC-0007 slice 3) is drag-and-drop only: the Filmstrip -- the one photo strip still visible
   * while the map is the active Library view (Grid, otherwise the natural drag source, is not, since the two
   * are mutually exclusive Library view modes) -- makes its cells draggable; dragging one over the map shows a
   * preview pin exactly under the pointer, at the point that photo's location would become, and dropping it
   * calls `onAssignLocation` for that point immediately. There's no separate confirm step: dropping is
   * confirming, the same as dragging a file onto a folder.
   * @type {{
   *   images: import('$lib/api/catalog.js').ImageSummary[],
   *   onSelectCluster: (imageIds: number[]) => void,
   *   onAssignLocation: (imageIds: number[], lat: number, lng: number) => Promise<boolean>,
   *   onNeedThumbnail?: (versionId: number) => void,
   * }}
   */
  let { images, onSelectCluster, onAssignLocation, onNeedThumbnail } = $props();

  const TILE_URL = "https://tile.openstreetmap.org/{z}/{x}/{y}.png";
  const COPYRIGHT_URL = "https://www.openstreetmap.org/copyright";
  /** Extra viewport fraction to draw around the visible area, so panning does not show markers popping in. */
  const VIEWPORT_PAD = 0.25;

  let container = /** @type {HTMLDivElement | undefined} */ (undefined);
  /** @type {typeof import('leaflet') | null} */
  let leaflet = null;
  /** @type {import('leaflet').Map | null} */
  let map = null;
  /** @type {import('leaflet').LayerGroup | null} */
  let markers = null;
  let ready = $state(false);
  let loadError = $state(false);
  /** Set once the map has been framed around the pins, so later image changes do not yank the view. */
  let framed = false;

  /** Highlighted while a Filmstrip drag is over the map, so there's feedback before the drop lands. */
  let dragOver = $state(false);
  /** Where a live drag is, in `.map-canvas` pixels -- drives the preview pin under the pointer so the
   * exact drop point is visible before letting go. `null` whenever nothing is being dragged over the map. */
  let dragPreview = $state(/** @type {{ x: number, y: number } | null} */ (null));

  let points = $derived(geolocatedPoints(images));
  let hiddenCount = $derived(new Set(images.map((i) => i.image_id)).size - points.length);
  /** First-seen `ImageSummary` per `image_id` -- same de-duplication `geolocatedPoints` uses, so a
   * cluster's `imageIds[0]` (also first-seen) looks up the same photo its point came from. */
  let imagesById = $derived.by(() => {
    /** @type {Map<number, import('$lib/api/catalog.js').ImageSummary>} */
    const map = new Map();
    for (const img of images) if (!map.has(img.image_id)) map.set(img.image_id, img);
    return map;
  });

  /** @type {ReturnType<typeof clusterPoints>} */
  let clusters = [];
  /** Photos whose thumbnail has already been requested this time the map is open, so panning back
   * and forth over the same pins doesn't re-fire `ensureThumbnail` for one still being generated. */
  const requestedThumbnails = /** @type {Set<number>} */ (new Set());

  function recluster() {
    if (!map) return;
    clusters = clusterPoints(points, map.getZoom());
    draw();
  }

  /** @param {import('$lib/api/catalog.js').ImageSummary} rep */
  function requestThumbnail(rep) {
    if (requestedThumbnails.has(rep.image_id)) return;
    requestedThumbnails.add(rep.image_id);
    onNeedThumbnail?.(rep.version_id);
  }

  /** @param {string | null} url */
  function bgStyle(url) {
    return url ? ` style="background-image:url(&quot;${url.replace(/"/g, "&quot;")}&quot;)"` : "";
  }

  /** A representative photo's thumbnail, requesting it if it doesn't exist yet. */
  function thumbUrlFor(/** @type {number} */ imageId) {
    const rep = imagesById.get(imageId);
    const url = rep?.thumbnail_path ? convertFileSrc(rep.thumbnail_path) : null;
    if (rep && !url) requestThumbnail(rep);
    return url;
  }

  const STACK_LAYERS = ["back2", "back1", "front"];

  function draw() {
    if (!map || !markers || !leaflet) return;
    const L = leaflet;
    const b = map.getBounds().pad(VIEWPORT_PAD);
    markers.clearLayers();
    for (const c of clustersInBounds(clusters, { south: b.getSouth(), west: b.getWest(), north: b.getNorth(), east: b.getEast() })) {
      const single = c.count === 1;
      /** @type {number} */
      let size;
      /** @type {string} */
      let html;
      if (single) {
        const thumbUrl = thumbUrlFor(c.imageIds[0]);
        size = thumbUrl ? 26 : 14;
        html = thumbUrl ? `<div class="map-pin photo"${bgStyle(thumbUrl)}></div>` : `<div class="map-pin single"></div>`;
      } else {
        // Up to 3 of the cluster's own photos, fanned like a hand of cards -- distinct photos where
        // thumbnails already exist, filling in as more arrive (each layer requests its own).
        const repIds = c.imageIds.slice(0, 3);
        const layers = STACK_LAYERS.slice(STACK_LAYERS.length - repIds.length);
        const cards = repIds.map((id, i) => `<div class="map-stack-card ${layers[i]}"${bgStyle(thumbUrlFor(id))}></div>`).join("");
        html = `<div class="map-pin-stack">${cards}<span class="map-pin-count">${c.count}</span></div>`;
        size = 40;
      }
      const marker = L.marker([c.lat, c.lng], {
        icon: L.divIcon({ className: "", html, iconSize: [size, size] }),
        title: single ? "1 photo" : `${c.count} photos`,
        keyboard: true,
      });
      marker.on("click", () => onSelectCluster(c.imageIds));
      markers.addLayer(marker);
    }
  }

  /** @param {DataTransfer | null} dt */
  function draggedImageIds(dt) {
    if (!dt) return [];
    try {
      const ids = JSON.parse(dt.getData(IMAGE_IDS_DRAG_MIME) || "[]");
      return Array.isArray(ids) ? ids.filter((id) => typeof id === "number") : [];
    } catch {
      return [];
    }
  }

  function handleDragOver(/** @type {DragEvent} */ event) {
    // Only claims drags carrying our own MIME (Filmstrip cells); stopping propagation keeps
    // LibraryModule's own "drop files to import" overlay (a dragover/drop pair one level up, on
    // `.library-body`) from also lighting up underneath. A real OS file drag never has this MIME, so
    // it's left alone here and still bubbles up to that import handling.
    if (!event.dataTransfer?.types.includes(IMAGE_IDS_DRAG_MIME)) return;
    event.preventDefault();
    event.stopPropagation();
    if (event.dataTransfer) event.dataTransfer.dropEffect = "copy";
    dragOver = true;
    if (container) {
      const rect = container.getBoundingClientRect();
      dragPreview = { x: event.clientX - rect.left, y: event.clientY - rect.top };
    }
  }

  function handleDragLeave(/** @type {DragEvent} */ event) {
    // Leaflet's own panes are children of .map-view and fire their own dragleave as the pointer
    // crosses them; only clear the highlight once the pointer has actually left the whole map.
    const to = /** @type {Node | null} */ (event.relatedTarget);
    if (event.currentTarget instanceof HTMLElement && to && event.currentTarget.contains(to)) return;
    dragOver = false;
    dragPreview = null;
  }

  async function handleDrop(/** @type {DragEvent} */ event) {
    const ids = draggedImageIds(event.dataTransfer);
    dragOver = false;
    dragPreview = null;
    if (ids.length === 0 || !map || !leaflet || !container) return;
    event.preventDefault();
    event.stopPropagation();
    const rect = container.getBoundingClientRect();
    const at = map.containerPointToLatLng(leaflet.point(event.clientX - rect.left, event.clientY - rect.top));
    await onAssignLocation(ids, at.lat, at.lng);
  }

  function frameAllPins() {
    if (!map || !leaflet || framed) return;
    const box = boundsOfPoints(points);
    if (!box) return;
    framed = true;
    map.fitBounds(
      [
        [box.south, box.west],
        [box.north, box.east],
      ],
      { padding: [40, 40], maxZoom: 15 },
    );
  }

  onMount(() => {
    let disposed = false;
    (async () => {
      try {
        const [mod] = await Promise.all([import("leaflet"), import("leaflet/dist/leaflet.css")]);
        if (disposed || !container) return;
        const L = /** @type {typeof import('leaflet')} */ (mod.default ?? mod);
        leaflet = L;
        map = L.map(container, { worldCopyJump: false, minZoom: 2, zoomControl: true, attributionControl: true });
        map.attributionControl.setPrefix(false);
        L.tileLayer(TILE_URL, { maxZoom: 19, attribution: `© <a href="${COPYRIGHT_URL}">OpenStreetMap</a> contributors` }).addTo(map);
        markers = L.layerGroup().addTo(map);
        map.setView([20, 0], 2);
        container.addEventListener("click", handleAttributionClick);
        map.on("zoomend", recluster);
        map.on("moveend", draw);
        ready = true;
      } catch (err) {
        console.error("Map failed to load", err);
        loadError = true;
      }
    })();
    return () => {
      disposed = true;
      container?.removeEventListener("click", handleAttributionClick);
      map?.remove();
      map = null;
      markers = null;
      leaflet = null;
    };
  });

  // Re-cluster whenever the pins change (filters, edits to a photo's GPS) once the map exists.
  $effect(() => {
    void points;
    if (!ready) return;
    frameAllPins();
    recluster();
  });

  /** The attribution link would navigate the app window; send it to the system browser instead. */
  function handleAttributionClick(/** @type {MouseEvent} */ event) {
    const link = /** @type {HTMLElement} */ (event.target).closest?.("a[href^='http']");
    if (!link) return;
    event.preventDefault();
    openExternalUrl(/** @type {HTMLAnchorElement} */ (link).href);
  }
</script>

<div
  class="map-view"
  class:drag-over={dragOver}
  role="application"
  aria-label="Photo map"
  ondragover={handleDragOver}
  ondragleave={handleDragLeave}
  ondrop={handleDrop}
>
  <div class="map-canvas" bind:this={container}></div>
  {#if dragOver && dragPreview}
    <div class="drop-preview" style="left:{dragPreview.x}px; top:{dragPreview.y}px;"></div>
  {:else if ready && !loadError}
    <div class="drag-hint">Drag a photo from the filmstrip onto the map to set its location</div>
  {/if}
  {#if loadError}
    <div class="map-note">The map could not be loaded.</div>
  {:else if ready && points.length === 0}
    <div class="map-note">None of these photos have a location. Add GPS coordinates in the metadata panel.</div>
  {:else if ready && hiddenCount > 0}
    <div class="map-badge">{hiddenCount} without location</div>
  {/if}
</div>

<style>
  .map-view {
    position: relative;
    flex: 1;
    min-height: 0;
    background: var(--bg-app);
  }
  .map-canvas {
    position: absolute;
    inset: 0;
  }
  .map-note,
  .map-badge {
    position: absolute;
    z-index: 1000;
    background: var(--bg-panel-raised);
    color: var(--text-secondary);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-s);
    font-size: 12px;
    pointer-events: none;
  }
  .drag-hint {
    position: absolute;
    z-index: 1000;
    top: 10px;
    left: 50%;
    transform: translateX(-50%);
    padding: 5px 10px;
    background: var(--bg-panel-raised);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-s);
    font-size: 12px;
    color: var(--text-secondary);
    pointer-events: none;
  }
  .map-note {
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    padding: 10px 14px;
    max-width: 320px;
    text-align: center;
  }
  .map-badge {
    left: 10px;
    bottom: 24px;
    padding: 4px 8px;
  }
  .map-view.drag-over .map-canvas {
    outline: 3px solid var(--accent);
    outline-offset: -3px;
  }
  /* Follows the pointer while a photo is dragged over the map -- its center is exactly the point that
   * would be saved on drop, computed the same way (`container`-relative pixels). */
  .drop-preview {
    position: absolute;
    z-index: 1001;
    width: 22px;
    height: 22px;
    margin: -11px 0 0 -11px;
    border-radius: 50%;
    background: rgba(255, 255, 255, 0.3);
    border: 2px solid var(--accent);
    box-shadow: 0 0 0 2px rgba(0, 0, 0, 0.45);
    pointer-events: none;
  }
  /* Markers are created by Leaflet outside this component's markup, so they need :global. */
  .map-view :global(.map-pin) {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
    width: 100%;
    height: 100%;
    box-sizing: border-box;
    border-radius: 50%;
    background: var(--accent);
    color: #fff;
    border: 2px solid #fff;
    box-shadow: 0 1px 4px rgba(0, 0, 0, 0.5);
    font-size: 11px;
    font-weight: 600;
    cursor: pointer;
  }
  .map-view :global(.map-pin.single) {
    border-width: 2px;
  }
  /* A pin with a loaded thumbnail: the photo fills the circle instead of the plain dot/count. */
  .map-view :global(.map-pin.photo) {
    background-color: var(--bg-panel-raised);
    background-size: cover;
    background-position: center;
  }
  .map-view :global(.map-pin-count) {
    position: absolute;
    right: -4px;
    bottom: -4px;
    min-width: 16px;
    height: 16px;
    padding: 0 3px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 999px;
    background: var(--accent);
    color: #fff;
    font-size: 10px;
    font-weight: 700;
    border: 1.5px solid #fff;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.5);
  }
  /* A cluster: up to 3 of its own photos fanned like a hand of cards, back layers rotated/offset and
   * dimmed for depth, so it reads as "several photos" rather than one photo standing in for the group. */
  .map-view :global(.map-pin-stack) {
    position: relative;
    width: 100%;
    height: 100%;
  }
  .map-view :global(.map-stack-card) {
    position: absolute;
    left: 50%;
    top: 50%;
    width: 24px;
    height: 24px;
    margin: -12px 0 0 -12px;
    box-sizing: border-box;
    border-radius: 5px;
    background-color: var(--bg-panel-raised);
    background-size: cover;
    background-position: center;
    border: 2px solid #fff;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.5);
    cursor: pointer;
  }
  .map-view :global(.map-stack-card.back2) {
    z-index: 1;
    transform: rotate(-12deg) translate(-6px, -5px);
    filter: brightness(0.82);
  }
  .map-view :global(.map-stack-card.back1) {
    z-index: 2;
    transform: rotate(8deg) translate(5px, -3px);
    filter: brightness(0.9);
  }
  .map-view :global(.map-stack-card.front) {
    z-index: 3;
    background-color: var(--accent);
  }
  .map-view :global(.leaflet-container) {
    font-family: inherit;
    background: var(--bg-app);
  }
</style>
