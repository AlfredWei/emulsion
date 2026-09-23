<script>
  import { onMount } from "svelte";
  import {
    assignKeywordPath,
    removeKeywordFromImage,
    getImageKeywords,
    listKeywords,
    setGeoLocation,
  } from "$lib/api/catalog.js";
  import { geocodeSearch, setGeoLocationBatch, getMapSettings, reverseGeocode } from "$lib/api/map.js";
  import { revealInFileManager } from "$lib/api/system.js";
  import LibraryHistogram from "$lib/components/LibraryHistogram.svelte";

  /**
   * `targetImageIds` is who a newly-typed keyword gets assigned to --
   * the whole current Library selection when there is one, matching the
   * batch rate/flag/color-label behavior. The chip list below always
   * reflects `image` alone (the anchor), matching the existing IPTC
   * caption/copyright/contact precedent -- only *assignment* batches.
   * @type {{
   *   image: import('$lib/api/catalog.js').ImageSummary | null,
   *   targetImageIds: number[],
   *   selectedCount?: number,
   *   onRatingChange?: (rating: number) => void,
   *   onFlagChange?: (flag: string) => void,
   *   onColorLabelChange?: (colorLabel: string) => void,
   *   onCaptionChange: (caption: string) => void,
   *   onCopyrightChange: (copyright: string) => void,
   *   onContactChange: (contact: string) => void,
   *   onKeywordAssigned: (name: string, imageCount: number) => void,
   *   onGeoLocationChange?: (lat: number | null, lon: number | null, alt: number | null) => void,
   *   onGeoLocationApplied?: (imageIds: number[], lat: number, lon: number) => void,
   *   faces?: import('$lib/api/faces.js').FaceRow[],
   *   people?: import('$lib/api/faces.js').PersonRow[],
   *   detectingFaces?: boolean,
   *   faceDetectionProgress?: { current: number, total: number } | null,
   *   hoveredFaceId?: number | null,
   *   onHoverFace?: (faceId: number | null) => void,
   *   onDetectFaces?: () => void,
   *   onTagFace?: (faceId: number, personId: number) => void,
   *   onCreateAndTagFace?: (faceId: number, name: string) => void,
   *   onRenamePerson?: (personId: number, name: string | null) => void,
   *   onSetFaceExcluded?: (faceId: number, excluded: boolean) => void,
   * }}
   */
  let {
    image,
    targetImageIds,
    selectedCount = 1,
    onRatingChange,
    onFlagChange,
    onColorLabelChange,
    onCaptionChange,
    onCopyrightChange,
    onContactChange,
    onKeywordAssigned,
    onGeoLocationChange,
    onGeoLocationApplied,
    faces = [],
    people = [],
    detectingFaces = false,
    faceDetectionProgress = null,
    hoveredFaceId = null,
    onHoverFace,
    onDetectFaces,
    onTagFace,
    onCreateAndTagFace,
    onRenamePerson,
    onSetFaceExcluded,
  } = $props();

  let filename = $derived(image ? image.path.split(/[/\\]/).pop() || image.path : "—");
  let dirPath = $derived(
    image
      ? image.path.slice(0, Math.max(0, image.path.length - filename.length)).replace(/[/\\]$/, "")
      : "—",
  );

  let fileSizeFormatted = $derived.by(() => {
    if (!image?.file_size) return "—";
    const mb = image.file_size / (1024 * 1024);
    if (mb >= 1) return `${mb.toFixed(2)} MB`;
    const kb = image.file_size / 1024;
    return `${kb.toFixed(1)} KB`;
  });

  let dimensionsLine = $derived.by(() => {
    if (!image?.width || !image?.height) return "—";
    const mp = ((image.width * image.height) / 1000000).toFixed(1);
    return `${image.width} × ${image.height} (${mp} MP)`;
  });

  let cameraLine = $derived(
    image ? [image.camera_make, image.camera_model].filter(Boolean).join(" ") || "—" : "—",
  );
  let lensLine = $derived(image?.lens_model || "—");

  function formatShutter(/** @type {number} */ seconds) {
    if (seconds >= 1) return `${seconds}s`;
    const denominator = Math.round(1 / seconds);
    return `1/${denominator}s`;
  }

  let exposureLine = $derived.by(() => {
    if (!image) return "—";
    const parts = [];
    if (image.shutter_speed) parts.push(formatShutter(image.shutter_speed));
    if (image.aperture) parts.push(`f/${image.aperture.toFixed(1)}`);
    if (image.iso) parts.push(`ISO ${image.iso}`);
    return parts.length > 0 ? parts.join(" · ") : "—";
  });

  let exposureBiasLine = $derived.by(() => {
    if (image?.exposure_bias === undefined || image?.exposure_bias === null) return "—";
    const val = image.exposure_bias;
    return `${val >= 0 ? "+" : ""}${val.toFixed(2)} EV`;
  });

  let focalLengthLine = $derived(image?.focal_length ? `${Math.round(image.focal_length)} mm` : "—");
  let meteringLine = $derived(image?.metering_mode || "—");
  let flashLine = $derived(image?.flash || "—");

  let capturedLine = $derived(
    image?.captured_at ? image.captured_at.replace("T", " ").slice(0, 16) : "—",
  );

  // GPS / Geo Location State
  let editingGps = $state(false);
  let latInput = $state("");
  let lonInput = $state("");
  let altInput = $state("");

  // Reverse geocoding (RFC-0007 §3.4, slice 3b): "Look up place name" sends this photo's own
  // coordinates to the selected provider, only when clicked -- never automatically. The result is
  // shown here, not written anywhere, and is cleared whenever the selection or the coordinates
  // change so a stale label can never look like it still describes the current location.
  let reverseLookup = $state(/** @type {string | null} */ (null));
  let reverseLoading = $state(false);
  let reverseError = $state("");

  $effect(() => {
    if (image) {
      latInput = image.latitude != null ? image.latitude.toString() : "";
      lonInput = image.longitude != null ? image.longitude.toString() : "";
      altInput = image.altitude != null ? image.altitude.toString() : "";
      editingGps = false;
    }
    reverseLookup = null;
    reverseError = "";
  });

  async function handleReverseLookup() {
    if (!image || image.latitude == null || image.longitude == null || reverseLoading) return;
    reverseLoading = true;
    reverseError = "";
    reverseLookup = null;
    try {
      const label = await reverseGeocode(image.latitude, image.longitude);
      reverseLookup = label ?? "No place name found for these coordinates.";
    } catch (/** @type {any} */ e) {
      reverseError = String(e);
    } finally {
      reverseLoading = false;
    }
  }

  let hasGps = $derived(image?.latitude != null && image?.longitude != null);
  let gpsCoordsDisplay = $derived.by(() => {
    if (!image || image.latitude == null || image.longitude == null) return "—";
    const latStr = `${Math.abs(image.latitude).toFixed(5)}° ${image.latitude >= 0 ? "N" : "S"}`;
    const lonStr = `${Math.abs(image.longitude).toFixed(5)}° ${image.longitude >= 0 ? "E" : "W"}`;
    const altStr = image.altitude != null ? ` · ${Math.round(image.altitude)}m` : "";
    return `${latStr}, ${lonStr}${altStr}`;
  });

  let mapUrl = $derived.by(() => {
    if (!image || image.latitude == null || image.longitude == null) return "";
    return `https://www.openstreetmap.org/?mlat=${image.latitude}&mlon=${image.longitude}#map=16/${image.latitude}/${image.longitude}`;
  });

  async function handleSaveGps() {
    if (!image) return;
    const lat = latInput.trim() ? parseFloat(latInput.trim()) : null;
    const lon = lonInput.trim() ? parseFloat(lonInput.trim()) : null;
    const alt = altInput.trim() ? parseFloat(altInput.trim()) : null;
    await setGeoLocation(image.image_id, lat, lon, alt);
    if (image) {
      image.latitude = lat;
      image.longitude = lon;
      image.altitude = alt;
    }
    editingGps = false;
    onGeoLocationChange?.(lat, lon, alt);
  }

  // Place search (M5.5, RFC-0007). Applies to the whole current selection
  // (`targetImageIds`), not just the anchor -- the results header says how
  // many photos a click will affect.
  let placeQuery = $state("");
  let placeResults = $state(/** @type {import('$lib/api/map.js').GeocodeCandidate[] | null} */ (null));
  let placeError = $state("");
  let placeSearching = $state(false);
  let mapProvider = $state(/** @type {import('$lib/api/map.js').GeocodeProvider} */ ("osm"));
  let googleKeyPresent = $state(false);
  // OpenStreetMap needs no setup; Google only works once a key is saved.
  let searchAvailable = $derived(mapProvider === "osm" || googleKeyPresent);

  async function refreshMapSettings() {
    try {
      const m = await getMapSettings();
      mapProvider = m.provider;
      googleKeyPresent = m.has_google_key;
    } catch {
      // Not running under Tauri (e.g. plain vite dev): leave defaults.
    }
  }
  // Re-checked per selected image so a provider/key change in Settings
  // shows up without reloading the app.
  $effect(() => {
    void image;
    refreshMapSettings();
  });

  async function handlePlaceSearch() {
    if (!placeQuery.trim() || placeSearching) return;
    placeSearching = true;
    placeError = "";
    placeResults = null;
    try {
      placeResults = await geocodeSearch(placeQuery);
    } catch (/** @type {any} */ e) {
      placeError = String(e);
    } finally {
      placeSearching = false;
    }
  }

  async function handleApplyPlace(/** @type {import('$lib/api/map.js').GeocodeCandidate} */ place) {
    const ids = [...targetImageIds];
    if (ids.length === 0) return;
    placeError = "";
    try {
      await setGeoLocationBatch(ids, place.latitude, place.longitude);
      if (image && ids.includes(image.image_id)) {
        image.latitude = place.latitude;
        image.longitude = place.longitude;
      }
      onGeoLocationApplied?.(ids, place.latitude, place.longitude);
      placeResults = null;
      placeQuery = "";
    } catch (/** @type {any} */ e) {
      placeError = String(e);
    }
  }

  function handleReveal() {
    if (image) revealInFileManager(image.path);
  }

  // Keywording (M2 Slice 4)
  let allKeywords = $state(/** @type {import('$lib/api/catalog.js').KeywordNode[]} */ ([]));
  let imageKeywords = $state(/** @type {import('$lib/api/catalog.js').KeywordRef[]} */ ([]));
  let keywordInput = $state("");
  let suggestionsOpen = $state(false);

  onMount(async () => {
    allKeywords = await listKeywords();
  });

  let keywordSuggestions = $derived.by(() => {
    const byId = new Map(allKeywords.map((k) => [k.id, k]));
    return allKeywords.map((node) => {
      const segments = [node.name];
      let current = node;
      while (current.parent_id !== null) {
        const parent = byId.get(current.parent_id);
        if (!parent) break;
        segments.unshift(parent.name);
        current = parent;
      }
      return segments.join("/");
    });
  });

  let filteredSuggestions = $derived.by(() => {
    const query = keywordInput.trim().toLowerCase();
    if (!query) return [];
    return keywordSuggestions.filter((path) => path.toLowerCase().includes(query)).slice(0, 8);
  });

  function pickSuggestion(/** @type {string} */ path) {
    keywordInput = path;
    suggestionsOpen = false;
    handleAssignKeyword();
  }

  $effect(() => {
    const targetId = image?.image_id ?? null;
    if (targetId === null) {
      imageKeywords = [];
      return;
    }
    getImageKeywords(targetId).then((keywords) => {
      if (image?.image_id === targetId) imageKeywords = keywords;
    });
  });

  async function handleAssignKeyword() {
    const segments = keywordInput
      .split("/")
      .map((s) => s.trim())
      .filter(Boolean);
    if (segments.length === 0 || targetImageIds.length === 0) return;
    await assignKeywordPath(targetImageIds, segments);
    keywordInput = "";
    suggestionsOpen = false;
    allKeywords = await listKeywords();
    onKeywordAssigned(segments[segments.length - 1], targetImageIds.length);
    if (image) imageKeywords = await getImageKeywords(image.image_id);
  }

  async function handleRemoveKeyword(/** @type {number} */ keywordId) {
    if (!image) return;
    await removeKeywordFromImage(image.image_id, keywordId);
    imageKeywords = await getImageKeywords(image.image_id);
  }

  // People/Faces (Library-integration redesign, 2026-09-17 -- see RFC-0005
  // §7): this section replaces the standalone People tab's Tag Faces
  // canvas side panel (PeopleTagView.svelte, now removed) with the same
  // tag/rename/reassign/"not a face" interactions, just relocated here so
  // detected people show up alongside the rest of this photo's metadata.
  // IPC itself stays in +page.svelte (onTagFace/onCreateAndTagFace/
  // onRenamePerson/onSetFaceExcluded/onDetectFaces are all callback props)
  // -- this component only owns the popover/menu's own open/closed UI
  // state, matching PeopleTagView's original split.
  let openPopoverFaceId = $state(/** @type {number | null} */ (null));
  let openMenuFaceId = $state(/** @type {number | null} */ (null));
  let popoverQuery = $state("");
  let renamingFaceId = $state(/** @type {number | null} */ (null));
  let renameValue = $state("");

  let faceSuggestions = $derived.by(() => {
    const q = popoverQuery.trim().toLowerCase();
    const named = people.filter((p) => p.name);
    return q ? named.filter((p) => /** @type {string} */ (p.name).toLowerCase().includes(q)) : named;
  });

  function closeFaceFloaters() {
    openPopoverFaceId = null;
    openMenuFaceId = null;
    renamingFaceId = null;
  }

  function toggleFacePopover(/** @type {number} */ faceId) {
    openMenuFaceId = null;
    popoverQuery = "";
    openPopoverFaceId = openPopoverFaceId === faceId ? null : faceId;
  }

  function toggleFaceMenu(/** @type {number} */ faceId) {
    openPopoverFaceId = null;
    openMenuFaceId = openMenuFaceId === faceId ? null : faceId;
  }

  function pickFaceSuggestion(/** @type {number} */ faceId, /** @type {import('$lib/api/faces.js').PersonRow} */ person) {
    onTagFace?.(faceId, person.id);
    closeFaceFloaters();
  }

  function submitNewFacePerson(/** @type {number} */ faceId) {
    const name = popoverQuery.trim();
    if (!name) return;
    onCreateAndTagFace?.(faceId, name);
    closeFaceFloaters();
  }

  function startRenamingFace(/** @type {import('$lib/api/faces.js').FaceRow} */ face) {
    if (face.person_id === null) return;
    openMenuFaceId = null;
    renamingFaceId = face.id;
    renameValue = face.person_name ?? "";
  }

  function commitFaceRename(/** @type {import('$lib/api/faces.js').FaceRow} */ face) {
    if (renamingFaceId !== face.id || face.person_id === null) return;
    renamingFaceId = null;
    onRenamePerson?.(face.person_id, renameValue.trim() || null);
  }

  /** @param {HTMLInputElement} node */
  function focusAndSelectFace(node) {
    node.focus();
    node.select();
  }
</script>

<svelte:window
  onclick={closeFaceFloaters}
  onkeydown={(e) => (openPopoverFaceId !== null || openMenuFaceId !== null) && e.key === "Escape" && closeFaceFloaters()}
/>

<div class="panel">
  {#if !image}
    <p class="empty">Select a photo to see its details.</p>
  {:else}
    <LibraryHistogram thumbnailPath={image.thumbnail_path} />

    <!-- Rating & Flag (Culling) -->
    <div class="section-header-row">
      <div class="section-label">Rating & Flag</div>
      {#if selectedCount > 1}
        <span class="multi-select-badge">{selectedCount} selected</span>
      {/if}
    </div>
    <div class="culling-block">
      <div class="culling-row stars-row">
        <span class="culling-label">Rating</span>
        <div class="culling-stars">
          {#each [1, 2, 3, 4, 5] as n (n)}
            <button
              type="button"
              class="star-btn"
              class:on={image.rating >= n}
              title="Rate {n} star{n === 1 ? '' : 's'} ({n})"
              onclick={() => onRatingChange?.(image.rating === n ? 0 : n)}
            >★</button>
          {/each}
          {#if image.rating > 0}
            <button
              type="button"
              class="clear-rating-btn"
              title="Clear rating (0)"
              onclick={() => onRatingChange?.(0)}
            >0</button>
          {/if}
        </div>
      </div>
      <div class="culling-row">
        <span class="culling-label">Flag</span>
        <div class="culling-flags">
          <button
            type="button"
            class="flag-pill flag-pick"
            class:active={image.flag === "pick"}
            title="Pick (P)"
            aria-label="Pick flag"
            onclick={() => onFlagChange?.(image.flag === "pick" ? "none" : "pick")}
          >
            <svg viewBox="0 0 16 16" width="12" height="12" fill="currentColor" aria-hidden="true">
              <path d="M3 2v12h1.5V9h7l-1.5-3.5L11.5 2H3z" />
            </svg>
          </button>
          <button
            type="button"
            class="flag-pill flag-unflag"
            class:active={image.flag === "none" || !image.flag}
            title="Unflag (U)"
            aria-label="Unflag"
            onclick={() => onFlagChange?.("none")}
          >
            <svg viewBox="0 0 16 16" width="12" height="12" fill="none" stroke="currentColor" stroke-width="1.4" aria-hidden="true">
              <path d="M3 2v12h1.5V9h6.5l-1.2-3.5L11 2H3z" stroke-linejoin="round" />
            </svg>
          </button>
          <button
            type="button"
            class="flag-pill flag-reject"
            class:active={image.flag === "reject"}
            title="Reject (X)"
            aria-label="Reject flag"
            onclick={() => onFlagChange?.(image.flag === "reject" ? "none" : "reject")}
          >
            <svg viewBox="0 0 16 16" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" aria-hidden="true">
              <path d="M4 4l8 8M12 4l-8 8" />
            </svg>
          </button>
        </div>
      </div>
      <div class="culling-row">
        <span class="culling-label">Color</span>
        <div class="culling-colors">
          {#each ["red", "yellow", "green", "blue", "purple"] as color (color)}
            <button
              type="button"
              class="color-dot-btn"
              class:active={image.color_label === color}
              style="background: var(--label-{color})"
              title="{color.charAt(0).toUpperCase() + color.slice(1)}"
              onclick={() => onColorLabelChange?.(image.color_label === color ? "none" : color)}
            ></button>
          {/each}
          {#if image.color_label && image.color_label !== "none"}
            <button
              type="button"
              class="clear-color-btn"
              title="Clear color label"
              onclick={() => onColorLabelChange?.("none")}
            >×</button>
          {/if}
        </div>
      </div>
    </div>

    <!-- File Information -->
    <div class="section-label">File</div>
    <div class="row has-tooltip">
      <span class="row-label">Name</span>
      <span class="val truncate font-mono">{filename}</span>
      <div class="tooltip-bubble">{filename}&#10;{image.path}</div>
    </div>
    <div class="row has-tooltip">
      <span class="row-label">Folder</span>
      <span class="val truncate font-mono">{dirPath}</span>
      <div class="tooltip-bubble">{dirPath}</div>
    </div>
    <div class="file-action-row">
      <button type="button" class="reveal-link" onclick={handleReveal}>📁 Reveal in File Manager ↗</button>
    </div>
    <div class="row">
      <span class="row-label">Size</span>
      <span class="val font-mono">{fileSizeFormatted}</span>
    </div>
    {#if dimensionsLine !== "—"}
      <div class="row">
        <span class="row-label">Dimensions</span>
        <span class="val font-mono">{dimensionsLine}</span>
      </div>
    {/if}

    <!-- Camera & Lens -->
    <div class="section-label">Camera & Lens</div>
    <div class="row has-tooltip">
      <span class="row-label">Camera</span>
      <span class="val truncate">{cameraLine}</span>
      {#if cameraLine !== "—"}
        <div class="tooltip-bubble">{cameraLine}</div>
      {/if}
    </div>
    <div class="row has-tooltip">
      <span class="row-label">Lens</span>
      <span class="val truncate">{lensLine}</span>
      {#if lensLine !== "—"}
        <div class="tooltip-bubble">{lensLine}</div>
      {/if}
    </div>

    <!-- Shooting & Exposure -->
    <div class="section-label">Shooting</div>
    <div class="row">
      <span class="row-label">Exposure</span>
      <span class="val font-mono">{exposureLine}</span>
    </div>
    {#if exposureBiasLine !== "—"}
      <div class="row">
        <span class="row-label">Bias</span>
        <span class="val font-mono">{exposureBiasLine}</span>
      </div>
    {/if}
    {#if focalLengthLine !== "—"}
      <div class="row">
        <span class="row-label">Focal L.</span>
        <span class="val font-mono">{focalLengthLine}</span>
      </div>
    {/if}
    {#if meteringLine !== "—"}
      <div class="row">
        <span class="row-label">Metering</span>
        <span class="val">{meteringLine}</span>
      </div>
    {/if}
    {#if flashLine !== "—"}
      <div class="row">
        <span class="row-label">Flash</span>
        <span class="val">{flashLine}</span>
      </div>
    {/if}
    <div class="row">
      <span class="row-label">Captured</span>
      <span class="val font-mono">{capturedLine}</span>
    </div>

    <!-- Location (GPS) -->
    <div class="section-header-row">
      <div class="section-label">Location (GPS)</div>
      <button
        class="action-link-btn"
        type="button"
        onclick={() => (editingGps = !editingGps)}
      >
        {editingGps ? "Cancel" : hasGps ? "Edit" : "+ Add GPS"}
      </button>
    </div>

    {#if searchAvailable}
      <form class="place-search" onsubmit={(e) => { e.preventDefault(); handlePlaceSearch(); }}>
        <input
          type="text"
          placeholder="Search place or address…"
          aria-label="Search place or address"
          bind:value={placeQuery}
        />
        <button type="submit" disabled={placeSearching || !placeQuery.trim()}>
          {placeSearching ? "…" : "Search"}
        </button>
      </form>
      {#if placeError}
        <div class="place-msg place-error">{placeError}</div>
      {:else if placeResults !== null}
        {#if placeResults.length === 0}
          <div class="place-msg">No places found.</div>
        {:else}
          <div class="place-msg">
            Apply to {targetImageIds.length} photo{targetImageIds.length === 1 ? "" : "s"}:
          </div>
          <ul class="place-results">
            {#each placeResults as place (place.label + place.latitude + place.longitude)}
              <li>
                <button type="button" onclick={() => handleApplyPlace(place)} title={place.label}>
                  {place.label}
                </button>
              </li>
            {/each}
          </ul>
        {/if}
      {/if}
      {#if mapProvider === "osm"}
        <div class="place-msg">Search data © OpenStreetMap contributors</div>
      {/if}
    {:else}
      <div class="place-msg">Google search needs an API key — add one in Settings → Map, or switch to OpenStreetMap.</div>
    {/if}

    {#if editingGps}
      <div class="gps-edit-form">
        <div class="gps-field">
          <label for="gps-lat">Latitude</label>
          <input id="gps-lat" type="text" placeholder="e.g. 25.03396" bind:value={latInput} />
        </div>
        <div class="gps-field">
          <label for="gps-lon">Longitude</label>
          <input id="gps-lon" type="text" placeholder="e.g. 121.56446" bind:value={lonInput} />
        </div>
        <div class="gps-field">
          <label for="gps-alt">Altitude (m)</label>
          <input id="gps-alt" type="text" placeholder="e.g. 15.0" bind:value={altInput} />
        </div>
        <button class="save-gps-btn" type="button" onclick={handleSaveGps}>Save Coordinates</button>
      </div>
    {:else if hasGps}
      <div class="row has-tooltip">
        <span class="row-label">Coords</span>
        <span class="val font-mono truncate">{gpsCoordsDisplay}</span>
        <div class="tooltip-bubble">{gpsCoordsDisplay}</div>
      </div>
      <div class="gps-action-row">
        <a class="map-link" href={mapUrl} target="_blank" rel="noreferrer">
          🗺 View on OpenStreetMap ↗
        </a>
        <button class="action-link-btn" type="button" onclick={handleReverseLookup} disabled={reverseLoading}>
          {reverseLoading ? "Looking up…" : "Look up place name"}
        </button>
      </div>
      {#if reverseError}
        <div class="place-msg place-error">{reverseError}</div>
      {:else if reverseLookup !== null}
        <div class="place-msg" title={reverseLookup}>{reverseLookup}</div>
      {/if}
    {:else}
      <div class="empty-hint-row">No GPS data</div>
    {/if}

    <!-- People/Faces (Library-integration redesign -- RFC-0005 §7) -->
    <div class="section-header-row">
      <div class="section-label">People</div>
      {#if selectedCount <= 1}
        <button
          class="action-link-btn"
          type="button"
          onclick={() => onDetectFaces?.()}
          disabled={detectingFaces}
        >
          {detectingFaces ? "Detecting…" : image.faces_scanned ? "Re-scan" : "😊 Face"}
        </button>
      {/if}
    </div>
    {#if selectedCount > 1}
      <div class="empty-hint-row">Select a single photo to see its detected people.</div>
    {:else if detectingFaces}
      <div class="face-detect-progress">
        {#if faceDetectionProgress}
          <progress value={faceDetectionProgress.current} max={faceDetectionProgress.total}></progress>
          <span>{faceDetectionProgress.current} / {faceDetectionProgress.total}</span>
        {:else}
          <span>Detecting faces…</span>
        {/if}
      </div>
    {:else if faces.length === 0}
      <div class="empty-hint-row">
        {image.faces_scanned ? "No faces detected in this photo." : "Not yet scanned — click “Face” above to detect."}
      </div>
    {:else}
      <div class="face-list">
        {#each faces as face (face.id)}
          {@const unnamed = face.person_id === null || !face.person_name}
          <div
            class="face-list-item"
            class:hovered={hoveredFaceId === face.id}
            role="presentation"
            onmouseenter={() => onHoverFace?.(face.id)}
            onmouseleave={() => onHoverFace?.(null)}
          >
            <span class="mini-avatar" aria-hidden="true"></span>
            <div class="info">
              {#if renamingFaceId === face.id}
                <input
                  class="face-rename-input"
                  type="text"
                  bind:value={renameValue}
                  use:focusAndSelectFace
                  onclick={(/** @type {MouseEvent} */ e) => e.stopPropagation()}
                  onblur={() => commitFaceRename(face)}
                  onkeydown={(/** @type {KeyboardEvent} */ e) => {
                    if (e.key === "Enter") {
                      e.preventDefault();
                      commitFaceRename(face);
                    } else if (e.key === "Escape") {
                      e.preventDefault();
                      renamingFaceId = null;
                    }
                  }}
                />
              {:else}
                <button
                  type="button"
                  class="nm-btn"
                  class:unnamed
                  onclick={(e) => {
                    e.stopPropagation();
                    toggleFacePopover(face.id);
                  }}
                >
                  {face.person_id === null ? "Who is this?" : (face.person_name ?? `Person ${face.person_id}`)}
                </button>
              {/if}
              <div class="sub">
                {face.person_id === null ? "Detected, not yet tagged" : face.person_name ? "Tagged" : "Auto-clustered, unnamed"}
              </div>

              {#if openPopoverFaceId === face.id}
                <div class="tag-popover" role="presentation" onclick={(/** @type {MouseEvent} */ e) => e.stopPropagation()}>
                  <input
                    type="text"
                    placeholder="Search people or add new…"
                    bind:value={popoverQuery}
                    use:focusAndSelectFace
                    onkeydown={(/** @type {KeyboardEvent} */ e) => e.key === "Enter" && submitNewFacePerson(face.id)}
                  />
                  <div class="results">
                    {#each faceSuggestions as person (person.id)}
                      <button type="button" class="suggestion" onclick={() => pickFaceSuggestion(face.id, person)}>
                        <span class="suggestion-name">{person.name}</span>
                        <span class="cnt">{person.photo_count}</span>
                      </button>
                    {/each}
                    <button type="button" class="suggestion new-person" onclick={() => submitNewFacePerson(face.id)}>
                      + New person{popoverQuery.trim() ? ` "${popoverQuery.trim()}"` : "…"}
                    </button>
                  </div>
                </div>
              {/if}
            </div>
            <button
              type="button"
              class="face-more-btn"
              aria-label="Face options"
              onclick={(e) => {
                e.stopPropagation();
                toggleFaceMenu(face.id);
              }}
            >⋯</button>
            {#if openMenuFaceId === face.id}
              <div class="face-menu" role="presentation" onclick={(/** @type {MouseEvent} */ e) => e.stopPropagation()}>
                {#if unnamed}
                  <button
                    type="button"
                    onclick={() => {
                      openMenuFaceId = null;
                      toggleFacePopover(face.id);
                    }}
                  >Tag this face…</button>
                {:else}
                  <button type="button" onclick={() => startRenamingFace(face)}>Rename…</button>
                  <button
                    type="button"
                    onclick={() => {
                      openMenuFaceId = null;
                      toggleFacePopover(face.id);
                    }}
                  >Reassign to…</button>
                {/if}
                <button
                  type="button"
                  class="danger"
                  onclick={() => {
                    closeFaceFloaters();
                    onSetFaceExcluded?.(face.id, true);
                  }}
                >Not a face</button>
              </div>
            {/if}
          </div>
        {/each}
      </div>
    {/if}

    <!-- Keywords -->
    <div class="section-label">Keywords</div>
    <div class="keyword-chips">
      {#each imageKeywords as keyword (keyword.id)}
        <span class="chip" title={keyword.path}>
          {keyword.name}
          <button
            type="button"
            class="chip-remove"
            aria-label="Remove keyword {keyword.name}"
            onclick={() => handleRemoveKeyword(keyword.id)}
          >×</button>
        </span>
      {:else}
        <span class="empty-hint">No keywords yet</span>
      {/each}
    </div>
    <div class="field keyword-input-field">
      <input
        id="md-keyword-input"
        type="text"
        placeholder="Add a keyword (nature/birds/owl)…"
        aria-label="Add a keyword"
        bind:value={keywordInput}
        onfocus={() => (suggestionsOpen = true)}
        onblur={() => (suggestionsOpen = false)}
        oninput={() => (suggestionsOpen = true)}
        onkeydown={(e) => e.key === "Enter" && (e.preventDefault(), handleAssignKeyword())}
      />
      {#if suggestionsOpen && filteredSuggestions.length > 0}
        <ul class="suggestions">
          {#each filteredSuggestions as path (path)}
            <li>
              <button
                type="button"
                onmousedown={(e) => e.preventDefault()}
                onclick={() => pickSuggestion(path)}
              >
                {path}
              </button>
            </li>
          {/each}
        </ul>
      {/if}
    </div>

    <!-- IPTC Info -->
    <div class="section-label">IPTC Info</div>
    <div class="field">
      <label for="md-caption">Caption</label>
      <textarea
        id="md-caption"
        rows="2"
        value={image.caption ?? ""}
        onblur={(e) => onCaptionChange(e.currentTarget.value)}
      ></textarea>
    </div>
    <div class="field">
      <label for="md-copyright">Copyright</label>
      <input
        id="md-copyright"
        type="text"
        value={image.copyright ?? ""}
        onblur={(e) => onCopyrightChange(e.currentTarget.value)}
      />
    </div>
    <div class="field">
      <label for="md-contact">Contact</label>
      <input
        id="md-contact"
        type="text"
        value={image.contact ?? ""}
        onblur={(e) => onContactChange(e.currentTarget.value)}
      />
    </div>
  {/if}
</div>

<style>
  .panel {
    width: 250px;
    flex: none;
    background: var(--bg-panel);
    border-left: 1px solid var(--border-subtle);
    overflow-y: auto;
    overflow-x: hidden;
    padding: 14px 12px;
  }
  .empty {
    color: var(--text-tertiary);
    font-size: 12px;
    padding: 4px;
  }
  .section-label {
    font-family: var(--font-mono);
    font-size: 10px;
    letter-spacing: 0.09em;
    text-transform: uppercase;
    color: var(--text-tertiary);
    padding: 10px 4px 6px;
    font-weight: 600;
  }
  .section-header-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding-right: 4px;
  }
  .section-header-row .section-label {
    padding-right: 0;
  }
  .multi-select-badge {
    font-size: 10px;
    font-family: var(--font-mono);
    color: var(--accent-strong);
    background: var(--accent-soft);
    padding: 2px 6px;
    border-radius: var(--radius-s);
    border: 1px solid var(--accent-border);
    font-weight: 500;
  }
  .culling-block {
    background: var(--bg-panel-raised);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-s);
    padding: 8px;
    margin: 2px 4px 10px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .culling-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
  }
  .culling-label {
    font-size: 10.5px;
    color: var(--text-secondary);
    width: 44px;
    flex: none;
  }
  .culling-stars {
    display: flex;
    align-items: center;
    gap: 3px;
  }
  .star-btn {
    all: unset;
    cursor: pointer;
    font-size: 14px;
    line-height: 1;
    color: var(--text-tertiary);
    opacity: 0.5;
    transition: transform 0.1s ease, color 0.1s ease, opacity 0.1s ease;
    padding: 1px 2px;
  }
  .star-btn:hover {
    transform: scale(1.15);
    opacity: 1;
    color: var(--accent-strong);
  }
  .star-btn.on {
    opacity: 1;
    color: var(--accent-strong);
  }
  .clear-rating-btn {
    all: unset;
    cursor: pointer;
    font-size: 9.5px;
    font-family: var(--font-mono);
    color: var(--text-tertiary);
    background: var(--bg-panel);
    border: 1px solid var(--border-subtle);
    border-radius: 50%;
    width: 14px;
    height: 14px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    margin-left: 3px;
  }
  .clear-rating-btn:hover {
    color: var(--label-red);
    border-color: var(--label-red);
  }
  .culling-flags {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .flag-pill {
    all: unset;
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 20px;
    border-radius: var(--radius-s);
    background: var(--bg-panel);
    border: 1px solid var(--border-subtle);
    color: var(--text-secondary);
    transition: all 0.1s ease;
  }
  .flag-pill:hover {
    color: var(--text-primary);
    border-color: var(--border-strong);
  }
  .flag-pick.active {
    background: rgba(34, 197, 94, 0.18);
    color: var(--label-green);
    border-color: rgba(34, 197, 94, 0.4);
    font-weight: 600;
  }
  .flag-reject.active {
    background: rgba(239, 68, 68, 0.18);
    color: var(--label-red);
    border-color: rgba(239, 68, 68, 0.4);
    font-weight: 600;
  }
  .flag-unflag.active {
    background: var(--bg-app);
    color: var(--text-tertiary);
  }
  .culling-colors {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .color-dot-btn {
    all: unset;
    cursor: pointer;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    box-shadow: 0 0 0 1px rgba(0, 0, 0, 0.3);
    transition: transform 0.1s ease, box-shadow 0.1s ease;
  }
  .color-dot-btn:hover {
    transform: scale(1.25);
  }
  .color-dot-btn.active {
    box-shadow: 0 0 0 2px #fff, 0 0 0 3px var(--accent);
    transform: scale(1.2);
  }
  .clear-color-btn {
    all: unset;
    cursor: pointer;
    font-size: 11px;
    color: var(--text-tertiary);
    padding: 0 3px;
  }
  .clear-color-btn:hover {
    color: var(--label-red);
  }
  .action-link-btn {
    all: unset;
    font-size: 10.5px;
    font-family: var(--font-mono);
    color: var(--accent);
    cursor: pointer;
    padding: 2px 4px;
    border-radius: var(--radius-s);
  }
  .action-link-btn:hover {
    text-decoration: underline;
  }
  .row {
    position: relative;
    display: flex;
    align-items: baseline;
    gap: 8px;
    padding: 3.5px 4px;
    font-size: 11.5px;
  }
  .row-label {
    width: 66px;
    flex: none;
    color: var(--text-secondary);
  }
  .val {
    color: var(--text-primary);
    min-width: 0;
  }
  .truncate {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    display: block;
    flex: 1;
  }
  .font-mono {
    font-family: var(--font-mono);
    font-variant-numeric: tabular-nums;
    font-size: 11px;
  }

  /* Custom Floating Tooltip Bubble */
  .has-tooltip {
    cursor: default;
  }
  .tooltip-bubble {
    visibility: hidden;
    opacity: 0;
    position: absolute;
    bottom: calc(100% + 4px);
    left: 4px;
    z-index: 50;
    max-width: 230px;
    background: rgba(18, 18, 22, 0.95);
    color: #f1f1f5;
    padding: 5px 8px;
    border-radius: var(--radius-s);
    font-family: var(--font-mono);
    font-size: 10px;
    line-height: 1.4;
    white-space: pre-wrap;
    word-break: break-all;
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.6);
    border: 1px solid var(--border-subtle);
    pointer-events: none;
    transition: opacity 0.15s ease, transform 0.15s ease;
    transform: translateY(2px);
  }
  .has-tooltip:hover .tooltip-bubble {
    visibility: visible;
    opacity: 1;
    transform: translateY(0);
  }

  .place-search {
    display: flex;
    gap: 4px;
    padding: 2px 4px 4px;
  }
  .place-search input {
    flex: 1;
    min-width: 0;
    padding: 3px 6px;
    font-size: 11px;
    color: var(--text-primary);
    background: var(--bg-app);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-s);
    box-sizing: border-box;
  }
  .place-search button {
    all: unset;
    padding: 3px 8px;
    font-size: 11px;
    color: var(--text-primary);
    background: var(--bg-panel-raised);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-s);
    cursor: pointer;
  }
  .place-search button:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .place-msg {
    padding: 0 4px 4px;
    font-size: 10.5px;
    color: var(--text-tertiary);
    word-break: break-word;
  }
  .place-error {
    color: var(--text-secondary);
  }
  .place-results {
    list-style: none;
    margin: 0 0 6px;
    padding: 0 4px;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .place-results button {
    all: unset;
    box-sizing: border-box;
    width: 100%;
    padding: 4px 6px;
    font-size: 11px;
    color: var(--text-primary);
    background: var(--bg-panel-raised);
    border-radius: var(--radius-s);
    cursor: pointer;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .place-results button:hover {
    filter: brightness(1.15);
  }
  .gps-edit-form {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 6px 4px 8px;
    background: var(--bg-panel-raised);
    border-radius: var(--radius-s);
    margin-bottom: 6px;
  }
  .gps-field {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
  }
  .gps-field label {
    font-size: 10px;
    color: var(--text-secondary);
    font-family: var(--font-mono);
    width: 65px;
  }
  .gps-field input {
    flex: 1;
    padding: 3px 6px;
    font-size: 11px;
    font-family: var(--font-mono);
    color: var(--text-primary);
    background: var(--bg-app);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-s);
    box-sizing: border-box;
  }
  .save-gps-btn {
    all: unset;
    margin-top: 4px;
    padding: 4px 8px;
    text-align: center;
    background: var(--accent);
    color: #fff;
    font-size: 11px;
    font-family: var(--font-mono);
    border-radius: var(--radius-s);
    cursor: pointer;
    font-weight: 500;
  }
  .save-gps-btn:hover {
    filter: brightness(1.1);
  }
  .gps-action-row {
    padding: 3px 4px 6px;
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  .map-link {
    font-size: 10.5px;
    color: var(--accent);
    text-decoration: none;
    font-family: var(--font-mono);
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }
  .map-link:hover {
    text-decoration: underline;
  }
  .file-action-row {
    padding: 3px 4px 6px;
  }
  .reveal-link {
    all: unset;
    cursor: pointer;
    font-size: 10.5px;
    color: var(--accent);
    font-family: var(--font-mono);
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }
  .reveal-link:hover {
    text-decoration: underline;
  }
  .empty-hint-row {
    font-size: 11px;
    color: var(--text-tertiary);
    padding: 2px 4px 6px;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 5px 4px;
  }
  .field label {
    font-size: 10.5px;
    color: var(--text-tertiary);
  }
  .field input,
  .field textarea {
    box-sizing: border-box;
    width: 100%;
    padding: 6px 8px;
    font-size: 12px;
    font-family: inherit;
    color: var(--text-primary);
    background: var(--bg-panel-raised);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-s);
    resize: vertical;
  }
  .field input:focus,
  .field textarea:focus {
    outline: 2px solid var(--accent);
    outline-offset: -1px;
  }
  .keyword-chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    padding: 2px 4px 8px;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 3px 6px 3px 9px;
    font-size: 11px;
    color: var(--text-secondary);
    background: var(--bg-panel-raised);
    border: 1px solid var(--border-subtle);
    border-radius: 999px;
  }
  .chip-remove {
    all: unset;
    cursor: pointer;
    line-height: 1;
    padding: 0 2px;
    color: var(--text-tertiary);
  }
  .chip-remove:hover {
    color: var(--label-red);
  }
  .empty-hint {
    font-size: 11px;
    color: var(--text-tertiary);
  }
  .keyword-input-field {
    position: relative;
  }
  .suggestions {
    all: unset;
    position: absolute;
    top: calc(100% + 2px);
    left: 4px;
    right: 4px;
    z-index: 10;
    display: block;
    max-height: 160px;
    overflow-y: auto;
    background: var(--bg-panel-raised);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-s);
    box-shadow: var(--shadow-soft);
    padding: 4px;
  }
  .suggestions li {
    list-style: none;
  }
  .suggestions button {
    all: unset;
    box-sizing: border-box;
    display: block;
    width: 100%;
    padding: 6px 8px;
    font-size: 12px;
    color: var(--text-primary);
    border-radius: var(--radius-s);
    cursor: pointer;
  }
  .suggestions button:hover {
    background: var(--accent-soft);
    color: var(--accent-strong);
  }

  /* People/Faces (Library-integration redesign) */
  .face-detect-progress {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 11px;
    color: var(--text-secondary);
    padding: 2px 4px 6px;
  }
  .face-detect-progress progress {
    flex: 1;
    height: 5px;
    accent-color: var(--accent);
  }
  .face-list {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding-bottom: 4px;
  }
  .face-list-item {
    position: relative;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px;
    border-radius: var(--radius-s);
  }
  .face-list-item.hovered {
    background: var(--accent-soft);
  }
  .face-list-item .mini-avatar {
    width: 28px;
    height: 28px;
    border-radius: 50%;
    flex: none;
    background: var(--bg-panel-raised);
    box-shadow: 0 0 0 1px var(--border-subtle);
  }
  .face-list-item .info {
    flex: 1;
    min-width: 0;
    position: relative;
  }
  .nm-btn {
    all: unset;
    display: block;
    font-size: 12px;
    color: var(--text-primary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    cursor: pointer;
    max-width: 100%;
  }
  .nm-btn.unnamed {
    color: var(--accent);
  }
  .face-list-item .sub {
    font-size: 10px;
    color: var(--text-tertiary);
  }
  .face-rename-input {
    all: unset;
    display: block;
    font-size: 12px;
    color: var(--text-primary);
    background: var(--bg-panel-raised);
    outline: 1px solid var(--accent);
    border-radius: var(--radius-s);
    padding: 1px 4px;
    max-width: 100%;
    box-sizing: border-box;
  }
  .face-more-btn {
    all: unset;
    flex: none;
    width: 18px;
    height: 18px;
    border-radius: 50%;
    color: var(--text-tertiary);
    font-size: 12px;
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
  }
  .face-more-btn:hover {
    background: var(--bg-hover);
    color: var(--text-primary);
  }
  .tag-popover {
    position: absolute;
    left: 0;
    top: calc(100% + 2px);
    z-index: 10;
    width: 200px;
    background: var(--bg-panel-raised);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-m);
    box-shadow: var(--shadow-soft);
    padding: 8px;
  }
  .tag-popover input[type="text"] {
    width: 100%;
    background: var(--bg-panel);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-s);
    color: var(--text-primary);
    font-size: 12px;
    padding: 6px 8px;
    margin-bottom: 6px;
    font-family: var(--font-ui);
    box-sizing: border-box;
  }
  .tag-popover input[type="text"]:focus {
    outline: 1px solid var(--accent);
    border-color: var(--accent);
  }
  .suggestion {
    all: unset;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 5px 6px;
    border-radius: var(--radius-s);
    cursor: pointer;
    font-size: 12px;
    width: 100%;
    box-sizing: border-box;
  }
  .suggestion:hover {
    background: var(--bg-hover);
  }
  .suggestion-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .suggestion .cnt {
    margin-left: auto;
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--text-tertiary);
  }
  .new-person {
    border-top: 1px solid var(--border-subtle);
    margin-top: 4px;
    padding-top: 7px;
    color: var(--accent-strong);
    font-weight: 700;
  }
  .face-menu {
    position: absolute;
    right: 0;
    top: calc(100% + 2px);
    z-index: 10;
    background: var(--bg-panel-raised);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-m);
    box-shadow: var(--shadow-soft);
    padding: 4px;
    width: 152px;
  }
  .face-menu button {
    all: unset;
    display: block;
    width: 100%;
    padding: 6px 8px;
    font-size: 12px;
    border-radius: var(--radius-s);
    color: var(--text-secondary);
    cursor: pointer;
    box-sizing: border-box;
  }
  .face-menu button:hover {
    background: var(--bg-hover);
    color: var(--text-primary);
  }
  .face-menu button.danger:hover {
    color: var(--label-red);
  }
</style>
