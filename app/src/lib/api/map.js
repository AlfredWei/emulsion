// Map & geolocation Tauri commands (M5.5, RFC-0007). The API key itself
// never reaches the frontend: the backend reads it and makes the Google
// call; the UI only ever learns whether a key exists.

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

/** @returns {Promise<boolean>} */
export function hasMapsApiKey() {
  return invoke("has_maps_api_key");
}

/** A blank or null key removes the stored one. */
export function setMapsApiKey(/** @type {string | null} */ key) {
  return invoke("set_maps_api_key", { key });
}
