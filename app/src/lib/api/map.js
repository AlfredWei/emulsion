// Map & geolocation Tauri commands (M5.5, RFC-0007). The backend makes the
// search request; a Google API key never reaches the frontend -- the UI only
// ever learns whether one is saved.

import { invoke } from "@tauri-apps/api/core";

/**
 * @typedef {Object} GeocodeCandidate
 * @property {string} label
 * @property {number} latitude
 * @property {number} longitude
 */

/** @returns {Promise<GeocodeCandidate[]>} */
export function geocodeSearch(/** @type {string} */ query) {
  return invoke("geocode_search", { query });
}

/**
 * Applies one location to every listed image in a single transaction;
 * altitude is left as-is. Resolves to how many images were updated.
 * @returns {Promise<number>}
 */
export function setGeoLocationBatch(
  /** @type {number[]} */ imageIds,
  /** @type {number} */ latitude,
  /** @type {number} */ longitude,
) {
  return invoke("set_geo_location_batch", { imageIds, latitude, longitude });
}

/**
 * @typedef {"osm" | "google"} GeocodeProvider
 */

/**
 * @typedef {Object} MapSettings
 * @property {GeocodeProvider} provider
 * @property {boolean} has_google_key
 */

/** @returns {Promise<MapSettings>} */
export function getMapSettings() {
  return invoke("get_map_settings");
}

export function setGeocodeProvider(/** @type {GeocodeProvider} */ provider) {
  return invoke("set_geocode_provider", { provider });
}

/** Google key only. A blank or null key removes the stored one. */
export function setMapsApiKey(/** @type {string | null} */ key) {
  return invoke("set_maps_api_key", { key });
}
