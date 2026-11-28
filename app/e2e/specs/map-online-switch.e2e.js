import path from "node:path";
import { fileURLToPath } from "node:url";
import { findCellByNameAnywhere, clickEl } from "../helpers.js";
import {
  closeSettings,
  ensureFixture,
  mapState,
  openMapSettings,
  openMapView,
  screenshot,
  startRequestLog,
  tryInvoke,
  waitForMap,
  writeEvidence,
} from "../laneMap.js";

// Lane spec for the online map switch (D17) on a fresh release catalog, where it starts off. One app session walks the
// card, a refused place search, turning the map on, a real place search, turning it off in Settings, and turning it on
// again so a relaunch can check that the choice persists. Driven by wdio.sandbox.conf.js with APP_BIN.
//   LANE_PROXY_PORT=<port>  the app was launched with HTTPS_PROXY=http://127.0.0.1:<port>; this spec listens there and
//                           records every host the backend reaches

const FIXTURE_NAME = "Cavenagh-Bridge-Singapore-panorama-frame-a";
const FIXTURE = path.resolve(path.dirname(fileURLToPath(import.meta.url)), `../../../test_image/${FIXTURE_NAME}.jpg`);
const proxyPort = Number(process.env.LANE_PROXY_PORT) || 0;
const drew = (/** @type {{ tilesLoaded: number }} */ s) => s.tilesLoaded > 0;

const clickTestId = (/** @type {string} */ id) =>
  browser.execute((i) => {
    const el = /** @type {HTMLElement | null} */ (document.querySelector(`[data-testid="${i}"]`));
    el?.click();
    return !!el;
  }, id);

const setOnlineCheckbox = (/** @type {boolean} */ on) =>
  browser.execute((want) => {
    const box = /** @type {HTMLInputElement | null} */ (document.querySelector('[data-testid="online-map-setting"]'));
    if (box && box.checked !== want) box.click();
    return box?.checked ?? null;
  }, on);

describe("Map, the online switch", () => {
  /** @type {{ hosts: string[], close: () => void } | null} */
  let proxy = null;

  before(async () => {
    await browser.setTimeout({ script: 120000 });
    if (proxyPort) proxy = await startRequestLog(proxyPort);
    await ensureFixture(FIXTURE);
  });

  after(() => proxy?.close());

  it("shows the card on a fresh release catalog", async () => {
    await openMapView();
    const state = await waitForMap((s) => s.card || drew(s));
    await screenshot("switch-1-off");
    writeEvidence("switch-1-off.txt", state);
    expect(state.card).toBe(true);
    expect(state.tileRequests).toEqual([]);
  });

  it("refuses place search and reverse lookup while off, without a request", async () => {
    const search = await tryInvoke("geocode_search", { query: "Singapore" });
    const reverse = await tryInvoke("reverse_geocode", { latitude: 1.2869, longitude: 103.8513 });
    const evidence = { search, reverse, backendHosts: proxy ? [...proxy.hosts] : "no proxy", page: await mapState() };
    writeEvidence("search-off.txt", evidence);
    expect(search.ok).toBe(false);
    expect(search.reason).toContain("are off");
    expect(reverse.ok).toBe(false);
    expect(reverse.reason).toContain("are off");
    if (proxy) expect(proxy.hosts).toEqual([]);
    expect(evidence.page.tileRequests).toEqual([]);
  });

  it("turns the map on from the card", async () => {
    await browser.execute(() => /** @type {HTMLElement | null} */ (document.querySelector('[data-testid="online-map-turn-on"]'))?.focus());
    await screenshot("switch-2-turn-on");
    expect(await clickTestId("online-map-turn-on")).toBe(true);
    const state = await waitForMap(drew);
    await screenshot("switch-3-on");
    writeEvidence("switch-3-on.txt", { ...state, stored: await tryInvoke("get_map_settings") });
    expect(state.card).toBe(false);
    expect(drew(state)).toBe(true);
  });

  it("finds Singapore with place search once on", async () => {
    await browser.execute(() => /** @type {HTMLElement | null} */ (document.querySelector('button[title^="Grid"]'))?.click());
    await clickEl(findCellByNameAnywhere(FIXTURE_NAME));
    await browser.execute(() => {
      const input = /** @type {HTMLInputElement | null} */ (document.querySelector('input[aria-label="Search place or address"]'));
      if (!input) return;
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")?.set?.call(input, "Singapore");
      input.dispatchEvent(new Event("input", { bubbles: true }));
      input.form?.requestSubmit();
    });
    const deadline = Date.now() + 30000;
    let results = /** @type {string[]} */ ([]);
    let failure = /** @type {string | null} */ (null);
    while (results.length === 0 && !failure && Date.now() < deadline) {
      await browser.pause(250);
      ({ results, failure } = await browser.execute(() => ({
        results: Array.from(document.querySelectorAll(".place-results li")).map((li) => li.textContent?.trim() ?? ""),
        failure: document.querySelector(".place-error")?.textContent ?? null,
      })));
    }
    await browser.execute(() => document.querySelector(".place-search")?.scrollIntoView({ block: "start" }));
    await screenshot("search-on");
    writeEvidence("search-on.txt", { results, failure, backendHosts: proxy ? [...proxy.hosts] : "no proxy" });
    expect(failure).toBeNull();
    expect(results.length).toBeGreaterThan(0);
    if (proxy) expect(proxy.hosts.some((h) => h.startsWith("nominatim.openstreetmap.org"))).toBe(true);
  });

  it("brings the card back when Settings turns it off, and requests no more tiles", async () => {
    await openMapView();
    await waitForMap(drew);
    await openMapSettings();
    await screenshot("switch-4-settings-on");
    expect(await setOnlineCheckbox(false)).toBe(false);
    await closeSettings();
    const off = await waitForMap((s) => s.card);
    const before = off.tileRequests.length;
    await browser.pause(5000);
    const later = await mapState();
    await screenshot("map-off-again");
    writeEvidence("map-off-again.txt", { off, tileRequestsAtOff: before, tileRequestsFiveSecondsLater: later.tileRequests.length, later });
    expect(off.card).toBe(true);
    expect(off.leaflet).toBe(false);
    expect(later.tileRequests.length).toBe(before);
  });

  it("turns it on again in Settings and keeps it for the next launch", async () => {
    await openMapSettings();
    expect(await setOnlineCheckbox(true)).toBe(true);
    await closeSettings();
    const state = await waitForMap(drew);
    const stored = await tryInvoke("get_map_settings");
    writeEvidence("switch-5-on-again.txt", { ...state, stored });
    expect(drew(state)).toBe(true);
    expect(stored.ok && stored.value.online_map_enabled).toBe(true);
  });
});
