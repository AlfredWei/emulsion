import { describe, it, expect, beforeEach, vi } from "vitest";
import { getStoredFilmstripHeight, saveStoredFilmstripHeight, FILMSTRIP_MIN_HEIGHT, FILMSTRIP_MAX_HEIGHT, FILMSTRIP_DEFAULT_HEIGHT } from "./panelLayout.js";

/** Vitest runs in node, which has no localStorage: a map-backed stand-in for these tests. */
function stubStorage() {
  /** @type {Map<string, string>} */
  const m = new Map();
  vi.stubGlobal("localStorage", { getItem: (/** @type {string} */ k) => m.get(k) ?? null, setItem: (/** @type {string} */ k, /** @type {string} */ v) => void m.set(k, String(v)), clear: () => m.clear() });
  return m;
}

describe("the stored filmstrip height", () => {
  beforeEach(() => void stubStorage());

  it("is the default when nothing is stored or the value is junk", () => {
    expect(getStoredFilmstripHeight()).toBe(FILMSTRIP_DEFAULT_HEIGHT);
    localStorage.setItem("emulsion_filmstrip_height_v1", "not a number");
    expect(getStoredFilmstripHeight()).toBe(FILMSTRIP_DEFAULT_HEIGHT);
  });

  it("round-trips, and a stored value outside the range is clamped", () => {
    saveStoredFilmstripHeight(150);
    expect(getStoredFilmstripHeight()).toBe(150);
    localStorage.setItem("emulsion_filmstrip_height_v1", "9999");
    expect(getStoredFilmstripHeight()).toBe(FILMSTRIP_MAX_HEIGHT);
    localStorage.setItem("emulsion_filmstrip_height_v1", "10");
    expect(getStoredFilmstripHeight()).toBe(FILMSTRIP_MIN_HEIGHT);
  });
});
