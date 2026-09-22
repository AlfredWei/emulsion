<script>
  import { onMount } from "svelte";
  import { convertFileSrc } from "@tauri-apps/api/core";
  import { geolocatedPoints, clusterPoints, boundsOfPoints, clustersInBounds } from "$lib/mapClusters.js";
  import { openExternalUrl } from "$lib/api/system.js";

  /**
   * LibraryMapView: the world map of the photos that have GPS coordinates (M5.5, RFC-0007 §3.3). Pins are
   * grouped into clusters by `mapClusters.js`; clicking one hands its photos' `image_id`s to `onSelectCluster`,
   * which scopes Library to them and shows the grid.
   *
   * Placing (RFC-0007 slice 3): with photos selected, "Place N photos" lets you click the map to drop a
   * draggable pin and Apply to save that location for the whole selection. When the first selected photo
   * already has a location the pin starts there, so this doubles as drag-to-adjust.
   *
   * Leaflet and its stylesheet are imported when this view first mounts -- never at app start -- so no tile is
   * requested until the person actually opens the map (RFC-0007 §7: the only network access in the app, and
   * only for map tiles). Tiles come from OpenStreetMap's public server; the attribution is required by its
   * tile usage policy.
   * A pin shows a thumbnail once one is available -- the same lazy `ensureThumbnail` fetch Grid/Loupe/Filmstrip
   * use, requested here for whichever photo represents a drawn pin/cluster (`onNeedThumbnail`, at most once per
   * photo per time the map is open). Until then, or for a photo Grid never had reason to generate a thumbnail
   * for, the pin falls back to the plain dot/count it always had.
   * @type {{
   *   images: import('$lib/api/catalog.js').ImageSummary[],
   *   selectedImageIds: number[],
   *   onSelectCluster: (imageIds: number[]) => void,
   *   onAssignLocation: (imageIds: number[], lat: number, lng: number) => Promise<boolean>,
   *   onNeedThumbnail?: (versionId: number) => void,
   * }}
   */
  let { images, selectedImageIds, onSelectCluster, onAssignLocation, onNeedThumbnail } = $props();

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

  let placing = $state(false);
  let saving = $state(false);
  /** The dropped pin's position while placing; `null` until the first click (or a starting location). */
  let draft = $state(/** @type {{ lat: number, lng: number } | null} */ (null));
  /** @type {import('leaflet').Marker | null} */
  let draftMarker = null;

  let targetIds = $derived([...new Set(selectedImageIds)]);
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

  function draw() {
    if (!map || !markers || !leaflet) return;
    const L = leaflet;
    const b = map.getBounds().pad(VIEWPORT_PAD);
    markers.clearLayers();
    for (const c of clustersInBounds(clusters, { south: b.getSouth(), west: b.getWest(), north: b.getNorth(), east: b.getEast() })) {
      const single = c.count === 1;
      const rep = imagesById.get(c.imageIds[0]);
      const thumbUrl = rep?.thumbnail_path ? convertFileSrc(rep.thumbnail_path) : null;
      if (rep && !thumbUrl) requestThumbnail(rep);
      const size = thumbUrl ? (single ? 26 : 34) : single ? 14 : 30;
      const html = thumbUrl
        ? `<div class="map-pin photo${single ? "" : " cluster"}" style="background-image:url(&quot;${thumbUrl.replace(/"/g, "&quot;")}&quot;)">${single ? "" : `<span class="map-pin-count">${c.count}</span>`}</div>`
        : `<div class="map-pin${single ? " single" : ""}">${single ? "" : c.count}</div>`;
      const marker = L.marker([c.lat, c.lng], {
        icon: L.divIcon({ className: "", html, iconSize: [size, size] }),
        title: single ? "1 photo" : `${c.count} photos`,
        keyboard: true,
      });
      marker.on("click", () => onSelectCluster(c.imageIds));
      markers.addLayer(marker);
    }
  }

  function setDraft(/** @type {number} */ lat, /** @type {number} */ lng) {
    if (!map || !leaflet) return;
    const L = leaflet;
    draft = { lat, lng };
    if (draftMarker) {
      draftMarker.setLatLng([lat, lng]);
      return;
    }
    draftMarker = L.marker([lat, lng], {
      draggable: true,
      zIndexOffset: 1000,
      icon: L.divIcon({ className: "", html: '<div class="map-pin draft"></div>', iconSize: [22, 22] }),
    }).addTo(map);
    draftMarker.on("dragend", () => {
      const at = draftMarker?.getLatLng();
      if (at) draft = { lat: at.lat, lng: at.lng };
    });
  }

  function clearDraft() {
    draftMarker?.remove();
    draftMarker = null;
    draft = null;
  }

  function startPlacing() {
    if (targetIds.length === 0) return;
    placing = true;
    // Start from the first selected photo's own location, so the pin can be nudged instead of re-placed.
    const first = images.find((img) => img.image_id === targetIds[0]);
    if (first && typeof first.latitude === "number" && typeof first.longitude === "number") setDraft(first.latitude, first.longitude);
  }

  function stopPlacing() {
    placing = false;
    clearDraft();
  }

  async function applyPlacement() {
    if (!draft || saving) return;
    saving = true;
    try {
      if (await onAssignLocation(targetIds, draft.lat, draft.lng)) stopPlacing();
    } finally {
      saving = false;
    }
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
        map.on("click", (/** @type {import('leaflet').LeafletMouseEvent} */ e) => {
          if (placing) setDraft(e.latlng.lat, e.latlng.lng);
        });
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
      draftMarker = null;
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

<div class="map-view" role="application" aria-label="Photo map">
  <div class="map-canvas" bind:this={container}></div>
  {#if ready && !loadError}
    <div class="place-bar">
      {#if !placing}
        <button type="button" disabled={targetIds.length === 0} onclick={startPlacing} title="Set the location of the selected photos by clicking the map">
          {targetIds.length === 0 ? "Select photos to place them" : `Place ${targetIds.length} photo${targetIds.length === 1 ? "" : "s"}`}
        </button>
      {:else}
        <span class="place-hint">
          {#if draft}
            {draft.lat.toFixed(5)}, {draft.lng.toFixed(5)} · drag to adjust
          {:else}
            Click the map to drop a pin for {targetIds.length} photo{targetIds.length === 1 ? "" : "s"}
          {/if}
        </span>
        <button type="button" class="primary" disabled={!draft || saving} onclick={applyPlacement}>{saving ? "Saving…" : "Apply"}</button>
        <button type="button" disabled={saving} onclick={stopPlacing}>Cancel</button>
      {/if}
    </div>
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
  .place-bar {
    position: absolute;
    z-index: 1000;
    top: 10px;
    left: 50%;
    transform: translateX(-50%);
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 8px;
    background: var(--bg-panel-raised);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-s);
    font-size: 12px;
    color: var(--text-secondary);
  }
  .place-bar button {
    font-size: 12px;
    padding: 4px 10px;
  }
  .place-bar button.primary {
    background: var(--accent);
    color: #fff;
    border-color: var(--accent);
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
  .map-view :global(.map-pin.draft) {
    background: #fff;
    border-color: var(--accent);
    cursor: grab;
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
  .map-view :global(.leaflet-container) {
    font-family: inherit;
    background: var(--bg-app);
  }
</style>
