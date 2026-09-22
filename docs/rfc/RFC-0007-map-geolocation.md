# RFC-0007: Map & geolocation (M5.5)

- Status: Accepted (PR #133), revised 2026-09-19 — see §7
- Date: 2026-09-19
- Companion documents: [MILESTONES §M5.5](../../PRD/MILESTONES.md#m55--map--geolocation), [PRD §3](../../PRD/PRD.md#3-non-goals-permanent-not-just-later), [PROGRESS.md](../../PROGRESS.md), [ADR-0005](../adr/ADR-0005-catalog-storage.md), [ADR-0008](../adr/ADR-0008-plugin-extensibility-api-v0.md)

## 1. Problem

M5.5 asks for: search an address/place name to set a photo's (or a batch's) location, a world-map view of every geolocated photo, and location edits that round-trip to exported files as EXIF GPS. Three facts about today's code shape the design:

- **The catalog already stores GPS.** `images.latitude/longitude/altitude` exist (`catalog.rs`), populated from EXIF at import and editable one photo at a time via `set_geo_location` (M4's manual-entry fields in `MetadataPanel`). There is no schema work; every new entry path (search, pin-drop, batch) writes these same columns, which is what makes the "no divergent write path" exit criterion cheap to satisfy.
- **Export wrote no EXIF at all** when this RFC was drafted (`export.rs` encoded a bare JPEG). The "round-trips through export as EXIF GPS" criterion therefore needed new export functionality, which was split out and built first as the EXIF/IPTC writer slice (§3.6).
- **This is the first feature that talks to a third-party web service on the user's behalf.** `reqwest` is already a dependency (face-model download, ADR-0007), but that is a one-time fetch of a public file. A user-triggered geocoding call carries user-typed text to Google; the map view loads Google-served script and tiles. This needs an explicit privacy boundary (§3.4), not just an implementation.

## 2. Non-goals

- No offline geocoding or tiles (M5.5 already defers this).
- No bundled or app-owned API key. The app is local-first with no server component; it cannot hold a shared secret safely. The user supplies their own Google Maps Platform key (§3.1).
- No travel routes, timelines, or track-log (GPX) import. Deferred by M5.5.
- No writing GPS into RAW originals or sidecars. Originals stay untouched (ADR-0005/0006); location lives in the catalog and lands in *exported* files only.
- No persisted geocode cache of Google results (§3.2) — Google's terms restrict storing geocoding results beyond narrow limits; we persist only coordinates the user chose to apply.

## 3. Design

### 3.1 API key: user-supplied, stored in the local settings KV

Google Maps Platform requires a key (and a billing-enabled project). The user pastes their own key in Settings → Map. It is stored in the catalog's existing `settings` KV table (same mechanism as `cache_dir`), never in the repo, never sent anywhere except Google. Without a key, all map/search UI shows a clear "add a Google Maps API key in Settings" state and the rest of the app is unaffected; manual coordinate entry keeps working. The user guide's setup section must explain the key/billing requirement honestly.

*Alternative considered:* a keyless provider (Nominatim/OSM + Leaflet). Rejected as the default because the request specifically asked for Google Maps, but see §6: the geocoder is isolated behind a small trait so swapping provider is a contained change.

### 3.2 Geocoding: in Rust, not in the webview

Forward geocoding is a Tauri command (`geocode_search(query) -> Vec<GeocodeCandidate {label, lat, lng}>`) using the existing `reqwest`, calling Google's Geocoding API. Doing it in Rust keeps the key out of frontend code/devtools-visible network calls of arbitrary pages, gives one place to rate-limit/debounce and map errors (`ZERO_RESULTS`, `OVER_QUERY_LIMIT`, `REQUEST_DENIED`) to user-readable messages, and makes the call unit-testable against a mocked response body (parse function tested on recorded JSON; no live network in CI). The request sends only the typed query string (plus the key).

Returns up to ~5 candidates; the UI shows them and applies only the one the user picks. Results are not cached to disk (§2).

### 3.3 Map rendering: Maps JavaScript API in the webview

The world-map Library view and the pin-drop picker use the Maps JavaScript API loaded lazily (only when the user opens a map surface with a key configured). `tauri.conf.json` has `"csp": null` today, so no policy change is required to load it; if a CSP is later introduced it needs `maps.googleapis.com`/`maps.gstatic.com` allowances (noted for M8 hardening).

- **Pins & clustering:** the frontend asks the backend for `(image_id, lat, lng)` of all geolocated images (a single lean query — no thumbnails), then clusters client-side with a small in-repo grid-based clusterer keyed on zoom. No new npm dependency; the marker-clusterer library is not worth pulling in for grid clustering at catalog scale (hundreds of thousands of points cluster fine in JS at O(n) per zoom change).
- **Click → filter:** clicking a pin/cluster sets a fifth mutually-exclusive Library source (alongside folder/collection/last-import/person — same shape `activePersonId` just used), holding the set of image ids, then switches to Grid.
- **Reality check to schedule:** Tauri's webview on Windows (WebView2) and macOS (WKWebView) should both run the Maps JS API, but this has not been verified in this project. Slice 3 begins with a spike to confirm it in the built app (not just `vite dev`) before building on it, the same way M0 validated WebGPU.

### 3.4 Privacy boundary (amends the M5.5 wording)

M5.5 as written says "only the searched text leaves the device." Three surfaces are actually in play, with different exposure:

| Surface | What leaves the device |
|---|---|
| Forward search | The typed query text |
| Map view / pin-drop | Map tile & script requests (reveals the viewed region/zoom to Google). Photo pins are drawn locally; **no photo coordinates are sent** |
| Reverse geocoding (label a photo's existing coordinates) | **The photo's coordinates** — catalog-derived data, which the M5.5 wording does not permit |

Recommendation: keep reverse geocoding, but make it strictly user-initiated per action ("Look up place name" on the selected photo(s)), never automatic on import or selection, and amend M5.5's exception text to name coordinates-on-explicit-request. Reverse-geocode results are shown, not persisted (§2). If the user would rather not cross that line at all, reverse geocoding is the one scope item to cut with no other slice affected. **This is the main decision for review.**

### 3.5 Writing locations: one catalog path, batch-capable

New command `set_geo_location_batch(image_ids, lat, lng, altitude?)` executing a single SQLite transaction (all-or-nothing, matching how import/keyword batch operations already behave). The existing single-photo command stays; search, pin-drop, and manual entry all end in one of these two, so there is one write path. Altitude is left unchanged when applying a searched location (Google's geocoder doesn't supply it) rather than being zeroed — batch applies to lat/lng only unless altitude is explicitly passed.

Edits are catalog-only and therefore already non-destructive to originals. Undo is out of scope for v0: overwriting a photo's existing location cannot be reverted except by re-entering the old value (a known limitation; "undo last location assign" is a natural follow-up, not required by the exit criteria).

### 3.6 Export: EXIF GPS on output — delivered by a prerequisite slice

Per review, the EXIF/IPTC writer is built **before** this milestone as its own slice (PR #134): `metadata_writer.rs` embeds EXIF (including GPS) and IPTC in exported JPEGs, with per-export user toggles for EXIF, GPS, and IPTC. It reads coordinates from the same catalog columns every entry path here writes, so this milestone needs no export work of its own. Its tests already round-trip GPS through the import-side parser, which covers the "manual and search-assigned coordinates export identically" criterion (both are the same columns). What remains for M5.5 is only to confirm that end-to-end once search/batch assignment exists.

## 4. Slice plan

0. **Prerequisite (done, PR #134): EXIF/IPTC export writer** with per-export toggles — see §3.6.
1. **Search & assign (backend + panel):** settings key storage, `geocode_search`, `set_geo_location_batch`, Settings → Map key field, a "Set location" search box in the metadata panel operating on the selection. Parser tests on recorded Google JSON; batch transaction test; end-to-end check that an assigned location appears in an exported file.
2. **Map view:** WebView spike first (§3.3), then the world-map Library view, client-side clustering, click-to-filter.
3. **Pin-drop & reverse geocode:** drag-to-adjust picker reusing the map surface; reverse-geocode button per §3.4 (or dropped, depending on the review decision).

Slice 1 is independently shippable and doesn't need the map at all; slice 2 carries the only real technical risk.

## 5. Open questions for review

1. **Reverse geocoding (§3.4):** keep as explicit-per-action with the amended privacy wording, or cut?
2. **Slice order:** 1→2→3 as above, or map view earlier, at the cost of doing the riskiest slice before the useful-and-safe one?
3. **Provider:** Google-only behind a trait (recommended), or also ship a keyless OSM fallback so the feature works with no setup?

## 6. Consequences

- New Settings surface (key) and the first explicit third-party privacy boundary; documented in the user guide and PRD §3's exception text.
- Export gains EXIF/IPTC writing infrastructure (via the prerequisite slice) that later metadata work can extend.
- A `Geocoder` trait boundary keeps a provider swap (or offline geocoder, if ever un-deferred) local to one module.
- No ADR yet; if slice 3's WebView spike or the provider decision produces a lasting architectural choice, it gets an ADR after shipping, per this project's RFC-then-ADR practice.

## 7. Update (2026-09-19): review decisions, and what they change

Google's Geocoding/Maps APIs turned out to require a billing account with a card even inside their free monthly caps (10,000 geocodes / 10,000 map loads at the time of writing; verify against Google's current pricing page). For an app whose premise is "no account, no cloud," that is a real setup hurdle for every future user. Review answered §5 as follows:

1. **Provider — both, OpenStreetMap by default.** Nominatim (no key, no account) is the default search provider; Google is an optional upgrade the user selects in Settings → Map and supplies their own key for (better at landmark/business names). This supersedes §3.1's "Google-only": the key is now needed *only* when Google is selected. Nominatim's policy is honored in code: an identifying User-Agent and at most one request per second (a process-wide throttle). Results carry the "© OpenStreetMap contributors" attribution in the UI. The provider is a setting and a two-arm `match` in `geocode.rs`; an interface/trait is still not warranted at two providers.
2. **Reverse geocoding — keep, explicit per action, in a later slice.** Amends M5.5's privacy wording (done in MILESTONES): forward search sends the typed text; reverse geocoding sends the photo's coordinates *only when the user asks for it on that photo*, never automatically. It goes to whichever provider is selected, so its exposure is that provider's.
3. **Map view — worth doing, but a later slice, rendered with Leaflet + OSM tiles (decided below).** Search & assign ships first (this slice), then the map view, then pin-drop/reverse geocode — the plan in §4 stands with the map view explicitly not next.

**Decided (2026-09-19, accepted in review): the map view uses Leaflet + OpenStreetMap tiles, for both geocoding providers.** Stated rationale: Google requires a billing account, so the app stays on the user-friendly, no-account path. Google's map tiles cannot be used inside Leaflet under Google's terms, and supporting two renderers would double the riskiest slice, so Google's role is limited to geocoding quality. Consequences: §3.3's Maps-JS-in-WebView spike is no longer a prerequisite (Leaflet in a webview is well-trodden) and the Maps JS "key in the webview" concern disappears — a Google key never needs to reach the frontend. The world-map view is *not* Google Maps, a deliberate departure from M5.5's original "Google Maps integration" phrasing. OSM's tile policy restricts heavy use (identifying requests, no bulk pre-fetching); fine for a personal-scale viewer, revisit if usage grows. §3.3 is superseded by this paragraph.

**Slice 1 as built** follows this update: `geocode.rs` supports both providers; Settings → Map has the provider choice and the Google key field (shown only when Google is selected); the panel's search works out of the box on OSM.

**Slice 2 (map view), design notes from building it (2026-09-21).** Two departures from §3.3, both simplifications. (1) *No lean coordinates query:* every `ImageSummary` already has `latitude`/`longitude`, so pins are computed from the Library's image list in the frontend; the backend gains nothing. (2) *Pins count photos, not versions:* virtual copies share their source's location and collapse to one point. Built in two parts: **2a** the pure clustering module `lib/mapClusters.js` (Web Mercator grid, no dependency) and the fifth mutually exclusive Library source `activeMapImageIds`; **2b** Leaflet, the `map` view mode and its wiring.

**Slice 2b as built (2026-09-21).** `LibraryMapView` draws Leaflet 1.9.4 (the one new runtime dependency, plus `@types/leaflet` for the type check) over `https://tile.openstreetmap.org`. Decisions worth keeping:

- **Tiles only when the map is open.** Leaflet and its CSS are dynamically imported when the component first mounts, so nothing is requested at app start or while another view is showing. The user guide's privacy paragraph says this. `csp` is `null` in `tauri.conf.json`, so no policy change was needed for the tile host.
- **Clusters are cached per zoom and drawn per viewport.** `clusterPoints` runs on `zoomend` and whenever the pins change; on every `moveend` only the clusters inside the viewport (padded 25%, `clustersInBounds`) become markers, so a large catalog does not create thousands of DOM nodes. Markers are `divIcon`s styled with the app's accent colour, which also avoids Leaflet's default marker images (a bundler asset-path problem).
- **The map shows the source, not the selection.** Opening the map (`showMapView`) clears `activeMapImageIds`, so the pins reflect the current folder/collection and filter bar. Clicking a pin or cluster (`handleMapClusterSelect`) sets the selection and returns to Grid, where the rail's "Map selection" entry (which clears it) shows the count.
- **Attribution link.** Leaflet's attribution `<a>` would navigate the app window; a click handler routes it through `openExternalUrl` (`plugin-opener`'s `openUrl`, already granted by `opener:default`), falling back to `window.open` outside Tauri.
- **No keyboard shortcut** for the map yet (adding one means a new entry in the rebinding table); toolbar only.
- **Not done here:** slice 3 (pin-drop / drag-to-adjust, reverse geocoding), the map following the metadata panel's selection, and any offline tile cache (OSM's policy discourages bulk pre-fetching).

**Slice 3a as built (2026-09-21): placing photos on the map, plus the `M` shortcut.** Slice 3 is split; reverse geocoding (3b) is not built yet.

- **Placing lives in the map view, not a separate picker.** With photos selected, "Place N photos" enters a placing mode: a click drops a draggable pin, "Apply" saves it. If the first selected photo already has coordinates the pin starts there, so drag-to-adjust and place-from-scratch are the same flow. Targets are `selection.keywordTargetImageIds` (the whole selection, else the anchor), deduplicated.
- **One write path, as §3.5 wanted:** Apply calls the existing `set_geo_location_batch` through `handleMapAssignLocation`; no new Rust command. Locally, `applyLocationLocally` (extracted from `LibraryModule`'s inline copy, now shared with the metadata panel's search-assign) updates every version of each photo, so pins and the panel refresh without refetching. Failure leaves the pin in place and reports in the status strip.
- **`normalizeCoordinate`** wraps longitudes from Leaflet's repeated world back into +-180 and clamps latitude before saving, since the backend's `validate_coordinates` rejects out-of-range values and a pin dragged into a neighbouring copy of the world would otherwise fail.
- **Shortcut:** `viewMap` (default `M`, rebindable, merged into stored shortcut sets like any new default) calls `showMapView`. No menu item yet: the native View menu is built in Rust (`lib.rs`), a separate change.
- **Still open for 3b:** explicit per-photo reverse geocoding ("Look up place name"), which sends coordinates to the selected provider and needs a backend command, the privacy wording (§3.4), and UI for showing the result without persisting it.

**Slice 3b as built (2026-09-21), closing RFC-0007.** Reverse geocoding, the one scope item left open by §3.4/§5.

- **One new backend command, `reverse_geocode(latitude, longitude) -> Option<String>`.** `geocode::reverse` mirrors `search`'s provider match: Google's reverse endpoint returns the same `{status, results[].formatted_address/geometry}` shape as forward search, so `parse_google_response` is reused as-is (closest result's label); Nominatim's `/reverse` returns one object instead of a list, so a small dedicated parser (`parse_osm_reverse_response`) reads `display_name`, and `None`/absent means nothing nearby -- not an error, the same shape as a forward search's empty list. Both share the existing one-request-per-second throttle (`throttle_nominatim`, extracted from `search` rather than duplicated) and provider/key lookup as `geocode_search`.
- **Explicit per action, exactly as §3.4 specified.** `reverse_geocode` is never called automatically -- the frontend only calls it from a "Look up place name" click, and only for the anchor image's own already-saved coordinates. The result is shown in the metadata panel (and while placing a pin on the map) and not persisted; switching photos, editing GPS, or applying a new location all clear it so a stale label can never look current.
- **No write path.** Unlike search-assign and the map placer, this only reads; `set_geo_location_batch` is untouched.
- **RFC-0007 is done.** All three slices (search & assign, map view, pin-drop + reverse geocode) are built; M5.5's exit criteria are met.
