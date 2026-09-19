# RFC-0007: Map & geolocation (M5.5)

- Status: Draft — for review in PR (flips to Accepted once merged)
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
