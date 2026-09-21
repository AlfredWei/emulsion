import { describe, it, expect } from "vitest";
import { geolocatedPoints, projectToPixels, boundsOfPoints, clusterPoints, clustersInBounds, MAX_MERCATOR_LAT } from "./mapClusters.js";

const img = (/** @type {number} */ id, /** @type {any} */ lat, /** @type {any} */ lng, /** @type {number} */ version = id * 10) =>
  /** @type {any} */ ({ image_id: id, version_id: version, latitude: lat, longitude: lng });
const pt = (/** @type {number} */ id, /** @type {number} */ lat, /** @type {number} */ lng) => ({ imageId: id, versionId: id * 10, lat, lng });

describe("geolocatedPoints", () => {
  it("keeps only photos with a real coordinate pair", () => {
    const points = geolocatedPoints([
      img(1, 48.85, 2.35),
      img(2, null, null),
      img(3, 10, null),
      img(4, undefined, undefined),
      img(5, NaN, 5),
      img(6, 91, 0),
      img(7, 0, 181),
      img(8, -90, 180), // the extremes are valid
      img(9, 0, 0), // real (if suspicious) coordinates are kept
    ]);
    expect(points.map((p) => p.imageId)).toEqual([1, 8, 9]);
    expect(points[0]).toEqual({ imageId: 1, versionId: 10, lat: 48.85, lng: 2.35 });
  });

  it("counts photos, not versions: virtual copies of one photo collapse to the first seen", () => {
    const points = geolocatedPoints([img(1, 10, 20, 11), img(1, 10, 20, 12), img(2, 30, 40)]);
    expect(points.map((p) => [p.imageId, p.versionId])).toEqual([
      [1, 11],
      [2, 20],
    ]);
  });

  it("is empty for no images", () => {
    expect(geolocatedPoints([])).toEqual([]);
  });
});

describe("projectToPixels", () => {
  it("puts the world's centre at half the world size, and grows by 2^zoom", () => {
    expect(projectToPixels(0, 0, 0)).toEqual({ x: 128, y: 128 });
    expect(projectToPixels(0, 0, 3)).toEqual({ x: 1024, y: 1024 });
  });

  it("x follows longitude linearly; north is up (smaller y)", () => {
    expect(projectToPixels(0, -180, 0).x).toBe(0);
    expect(projectToPixels(0, 180, 0).x).toBe(256);
    expect(projectToPixels(60, 0, 2).y).toBeLessThan(projectToPixels(-60, 0, 2).y);
  });

  it("clamps the poles to the Mercator limit instead of returning infinity/NaN", () => {
    const north = projectToPixels(90, 0, 0);
    const south = projectToPixels(-90, 0, 0);
    expect(Number.isFinite(north.y)).toBe(true);
    expect(north.y).toBeCloseTo(0, 3);
    expect(south.y).toBeCloseTo(256, 3);
    expect(projectToPixels(MAX_MERCATOR_LAT, 0, 0).y).toBeCloseTo(north.y, 6);
  });
});

describe("boundsOfPoints", () => {
  it("is null for no points and the enclosing box otherwise", () => {
    expect(boundsOfPoints([])).toBeNull();
    expect(boundsOfPoints([pt(1, 10, 20), pt(2, -5, 40), pt(3, 7, -30)])).toEqual({ south: -5, west: -30, north: 10, east: 40 });
    expect(boundsOfPoints([pt(1, 3, 4)])).toEqual({ south: 3, west: 4, north: 3, east: 4 });
  });
});

describe("clusterPoints", () => {
  it("is empty for no points", () => {
    expect(clusterPoints([], 5)).toEqual([]);
  });

  it("merges points that project into the same cell and keeps far ones apart", () => {
    // Paris and a point ~1 km away, plus Tokyo.
    const points = [pt(1, 48.8566, 2.3522), pt(2, 48.8600, 2.3600), pt(3, 35.68, 139.69)];
    const clusters = clusterPoints(points, 6);
    expect(clusters.map((c) => c.imageIds)).toEqual([[1, 2], [3]]);
    expect(clusters.map((c) => c.count)).toEqual([2, 1]);
  });

  it("splits a cluster as the zoom increases", () => {
    const points = [pt(1, 48.8566, 2.3522), pt(2, 48.8600, 2.3600)];
    expect(clusterPoints(points, 6)).toHaveLength(1);
    expect(clusterPoints(points, 17)).toHaveLength(2);
  });

  it("never loses or duplicates a photo, at any zoom", () => {
    const points = Array.from({ length: 200 }, (_, i) => pt(i + 1, ((i * 37) % 170) - 85, ((i * 91) % 350) - 175));
    let previous = 0;
    for (const zoom of [0, 1, 2, 4, 6, 9, 12, 16]) {
      const clusters = clusterPoints(points, zoom);
      expect(clusters.reduce((n, c) => n + c.count, 0)).toBe(200);
      expect(clusters.flatMap((c) => c.imageIds).sort((a, b) => a - b)).toEqual(points.map((p) => p.imageId));
      expect(clusters.length).toBeGreaterThanOrEqual(previous); // zooming in never merges clusters
      previous = clusters.length;
    }
  });

  it("positions a cluster at the mean of its points and bounds it by them", () => {
    const [c] = clusterPoints([pt(1, 10, 20), pt(2, 12, 22)], 0);
    expect(c.lat).toBe(11);
    expect(c.lng).toBe(21);
    expect(c.bounds).toEqual({ south: 10, west: 20, north: 12, east: 22 });
    expect(c.count).toBe(2);
  });

  it("uses both axes: the same longitude at very different latitudes stays apart", () => {
    expect(clusterPoints([pt(1, 10, 20), pt(2, 60, 20)], 3).map((c) => c.imageIds)).toEqual([[1], [2]]);
  });

  it("default cells are 64 screen pixels: points ~200 px apart are separate clusters", () => {
    // At zoom 4 the world is 4096 px wide; x = 1000 and x = 1200 px.
    const lngAt = (/** @type {number} */ x) => (x / 4096) * 360 - 180;
    expect(clusterPoints([pt(1, 0, lngAt(1000)), pt(2, 0, lngAt(1200))], 4)).toHaveLength(2);
  });

  it("honours the cell size: a bigger cell merges more", () => {
    const points = [pt(1, 0, 0), pt(2, 0, 3)];
    expect(clusterPoints(points, 4, 8)).toHaveLength(2);
    expect(clusterPoints(points, 4, 512)).toHaveLength(1);
  });

  it("is deterministic and in first-seen order, with image ids in input order", () => {
    const points = [pt(5, 12.34, 56.78), pt(3, -33.3, 111.1), pt(9, 12.3401, 56.7801), pt(1, -33.3001, 111.1001)];
    const a = clusterPoints(points, 10);
    expect(a).toEqual(clusterPoints(points, 10));
    expect(a.map((c) => c.imageIds)).toEqual([[5, 9], [3, 1]]);
  });

  it("handles coordinates at the poles and the dateline without NaN", () => {
    const clusters = clusterPoints([pt(1, 90, 180), pt(2, -90, -180)], 3);
    for (const c of clusters) {
      expect(Number.isFinite(c.lat)).toBe(true);
      expect(Number.isFinite(c.lng)).toBe(true);
    }
    expect(clusters.reduce((n, c) => n + c.count, 0)).toBe(2);
  });
});

describe("clustersInBounds", () => {
  const cluster = (/** @type {number} */ lat, /** @type {number} */ lng) =>
    /** @type {any} */ ({ lat, lng, count: 1, imageIds: [1], bounds: { south: lat, west: lng, north: lat, east: lng } });
  const box = { south: 0, west: 0, north: 10, east: 20 };

  it("keeps clusters inside the box and drops the rest, preserving order", () => {
    const inside = [cluster(5, 5), cluster(9, 19)];
    const outside = [cluster(-1, 5), cluster(11, 5), cluster(5, -1), cluster(5, 21)];
    expect(clustersInBounds([inside[0], ...outside, inside[1]], box)).toEqual(inside);
  });

  it("includes clusters exactly on an edge", () => {
    const edges = [cluster(0, 0), cluster(10, 20), cluster(0, 20), cluster(10, 0)];
    expect(clustersInBounds(edges, box)).toEqual(edges);
  });

  it("returns an empty list for no clusters", () => {
    expect(clustersInBounds([], box)).toEqual([]);
  });
});
