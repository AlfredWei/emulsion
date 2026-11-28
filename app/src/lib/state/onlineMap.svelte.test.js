import { describe, it, expect, vi, beforeEach } from "vitest";

const api = vi.hoisted(() => ({ getMapSettings: vi.fn(), setOnlineMapEnabled: vi.fn() }));
vi.mock("$lib/api/map.js", () => api);

import { OnlineMapStore } from "./onlineMap.svelte.js";

/** @param {boolean} online */
const settings = (online) => ({ online_map_enabled: online, provider: "osm", has_google_key: false, tile_url: null, attribution: null, nominatim_url: null });

beforeEach(() => {
  vi.clearAllMocks();
  api.setOnlineMapEnabled.mockResolvedValue(undefined);
});

describe("OnlineMapStore", () => {
  it("shows neither the map nor the card until the backend has answered", () => {
    expect(new OnlineMapStore().enabled).toBeNull();
  });

  it("takes the switch from the backend, which owns the per-build default", async () => {
    for (const online of [false, true]) {
      api.getMapSettings.mockResolvedValue(settings(online));
      const store = new OnlineMapStore();
      const got = await store.refresh();
      expect(store.enabled).toBe(online);
      expect(got).toEqual(settings(online));
    }
  });

  it("turning it on from the card saves the choice, then shows the map", async () => {
    api.getMapSettings.mockResolvedValue(settings(false));
    const store = new OnlineMapStore();
    await store.refresh();
    await store.set(true);
    expect(api.setOnlineMapEnabled).toHaveBeenCalledWith(true);
    expect(store.enabled).toBe(true);
    await store.set(false);
    expect(store.enabled).toBe(false);
  });

  it("keeps the card when the choice could not be saved", async () => {
    api.getMapSettings.mockResolvedValue(settings(false));
    api.setOnlineMapEnabled.mockRejectedValue(new Error("database is locked"));
    const store = new OnlineMapStore();
    await store.refresh();
    await expect(store.set(true)).rejects.toThrow("database is locked");
    expect(store.enabled).toBe(false);
  });
});
