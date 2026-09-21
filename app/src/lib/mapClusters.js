// World-map pins and clustering (M5.5 map view, RFC-0007 §3.3). Pure functions over plain data, so the
// map component only has to draw what these return.
//
// Photos with coordinates are already on every `ImageSummary` (`latitude`/`longitude`, filled from EXIF at
// import or edited in the metadata panel), so no extra query is needed. Clustering is a grid in Web Mercator
// pixel space, like the map's own tiles: at a given zoom, points whose projected positions fall in the same
// `cellSize`-pixel square become one cluster. It is O(n) per zoom change, and it needs no dependency.

/**
 * @typedef {import('$lib/api/catalog.js').ImageSummary} ImageSummary
 * @typedef {{ imageId: number, versionId: number, lat: number, lng: number }} MapPoint
 * @typedef {{ south: number, west: number, north: number, east: number }} MapBounds
 * @typedef {{ lat: number, lng: number, count: number, imageIds: number[], bounds: MapBounds }} MapCluster
 */

/** Web Mercator cannot show latitudes beyond this. */
export const MAX_MERCATOR_LAT = 85.0511287798;
/** Leaflet's (and OSM's) tile size in pixels at zoom 0 covering the whole world. */
const WORLD_TILE = 256;

/** True for a real coordinate pair; rejects null/NaN/out-of-range values. */
function validCoordinate(/** @type {number | null | undefined} */ lat, /** @type {number | null | undefined} */ lng) {
  return typeof lat === "number" && typeof lng === "number" && Number.isFinite(lat) && Number.isFinite(lng) && Math.abs(lat) <= 90 && Math.abs(lng) <= 180;
}

/**
 * One point per distinct photo (`image_id`) that has coordinates. Virtual copies share their source photo's
 * location, so they collapse to the first version seen; the map counts photos, not versions.
 * @param {readonly ImageSummary[]} images
 * @returns {MapPoint[]}
 */
export function geolocatedPoints(images) {
  /** @type {Set<number>} */
  const seen = new Set();
  /** @type {MapPoint[]} */
  const points = [];
  for (const img of images) {
    if (seen.has(img.image_id) || !validCoordinate(img.latitude, img.longitude)) continue;
    seen.add(img.image_id);
    points.push({ imageId: img.image_id, versionId: img.version_id, lat: /** @type {number} */ (img.latitude), lng: /** @type {number} */ (img.longitude) });
  }
  return points;
}

/** Projects to Web Mercator world pixels at `zoom` (fractional zoom allowed). */
export function projectToPixels(/** @type {number} */ lat, /** @type {number} */ lng, /** @type {number} */ zoom) {
  const size = WORLD_TILE * 2 ** zoom;
  const clamped = Math.max(-MAX_MERCATOR_LAT, Math.min(MAX_MERCATOR_LAT, lat));
  const sin = Math.sin((clamped * Math.PI) / 180);
  return {
    x: ((lng + 180) / 360) * size,
    y: (0.5 - Math.log((1 + sin) / (1 - sin)) / (4 * Math.PI)) * size,
  };
}

/** The smallest box containing every point, or `null` for none. */
export function boundsOfPoints(/** @type {readonly { lat: number, lng: number }[]} */ points) {
  if (points.length === 0) return null;
  let south = Infinity;
  let north = -Infinity;
  let west = Infinity;
  let east = -Infinity;
  for (const p of points) {
    if (p.lat < south) south = p.lat;
    if (p.lat > north) north = p.lat;
    if (p.lng < west) west = p.lng;
    if (p.lng > east) east = p.lng;
  }
  return { south, west, north, east };
}

/**
 * Groups points into clusters for the map at `zoom`. Cluster position is the mean of its points' coordinates;
 * `imageIds` keeps input order, and clusters come out in first-seen order, so the result is deterministic for
 * a given input.
 * @param {readonly MapPoint[]} points
 * @param {number} zoom map zoom level (fractional allowed)
 * @param {number} [cellSize] cluster cell edge in screen pixels
 * @returns {MapCluster[]}
 */
export function clusterPoints(points, zoom, cellSize = 64) {
  /** @type {Map<string, { latSum: number, lngSum: number, points: MapPoint[] }>} */
  const cells = new Map();
  for (const p of points) {
    const { x, y } = projectToPixels(p.lat, p.lng, zoom);
    const key = `${Math.floor(x / cellSize)}:${Math.floor(y / cellSize)}`;
    const cell = cells.get(key);
    if (cell) {
      cell.latSum += p.lat;
      cell.lngSum += p.lng;
      cell.points.push(p);
    } else {
      cells.set(key, { latSum: p.lat, lngSum: p.lng, points: [p] });
    }
  }
  return [...cells.values()].map((c) => ({
    lat: c.latSum / c.points.length,
    lng: c.lngSum / c.points.length,
    count: c.points.length,
    imageIds: c.points.map((p) => p.imageId),
    bounds: /** @type {MapBounds} */ (boundsOfPoints(c.points)),
  }));
}

/**
 * The clusters that fall inside `bounds` (edges included), so the map only draws what is on screen. A
 * cluster is kept by its own position, not its `bounds`; pad the viewport a little at the call site so
 * a cluster just off the edge does not pop in late while panning. Longitude wrapping is not handled:
 * `bounds.west <= bounds.east` is assumed.
 * @param {readonly MapCluster[]} clusters
 * @param {MapBounds} bounds
 * @returns {MapCluster[]}
 */
export function clustersInBounds(clusters, bounds) {
  return clusters.filter((c) => c.lat >= bounds.south && c.lat <= bounds.north && c.lng >= bounds.west && c.lng <= bounds.east);
}

/**
 * A coordinate the catalog will accept, from whatever the map produced. Dragging or clicking far to the
 * left or right of the world gives longitudes beyond +-180 (Leaflet repeats the world), so longitude is
 * wrapped back into range and latitude clamped; `null` for anything not finite.
 * @param {number} lat
 * @param {number} lng
 * @returns {{ lat: number, lng: number } | null}
 */
export function normalizeCoordinate(lat, lng) {
  if (!Number.isFinite(lat) || !Number.isFinite(lng)) return null;
  const wrapped = lng >= -180 && lng <= 180 ? lng : ((((lng + 180) % 360) + 360) % 360) - 180;
  return { lat: Math.max(-90, Math.min(90, lat)), lng: wrapped };
}
