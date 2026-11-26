import path from "node:path";
import { fileURLToPath } from "node:url";
import { ensureFixture, openMapView, screenshot, startTileServer, tryInvoke, waitForMap, writeEvidence } from "../laneMap.js";

// Lane spec for the online map default (D17): opens the map once and records whether it drew tiles or showed the
// card that turns it on. It runs on trunk too, which has no card. Driven by wdio.sandbox.conf.js with APP_BIN.
//   EXPECT_MAP=tiles|card   fail unless the first open shows that
//   MAP_TURN_ON=1           then press the card's button and wait for tiles
//   LANE_TILE_PORT=<port>   serve tiles from this computer on that port; with MAP_SEED_TILES=1 also save that server
//                           as the custom tile address before the map opens
//   MAP_SHOT=<name>         screenshot and evidence file name (default map-first)

const FIXTURE = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../../../test_image/Cavenagh-Bridge-Singapore-panorama-frame-a.jpg");
const shot = process.env.MAP_SHOT ?? "map-first";
const tilePort = Number(process.env.LANE_TILE_PORT) || 0;
const drew = (/** @type {{ tilesLoaded: number }} */ s) => s.tilesLoaded > 0;

describe("Map, first open", () => {
  /** @type {{ requests: string[], close: () => void } | null} */
  let tiles = null;
  const evidence = /** @type {Record<string, unknown>} */ ({ binary: process.env.APP_BIN });

  before(async () => {
    await browser.setTimeout({ script: 120000 });
    if (tilePort) tiles = await startTileServer(tilePort);
    await ensureFixture(FIXTURE);
    if (tilePort && process.env.MAP_SEED_TILES === "1") {
      evidence.seed = await tryInvoke("set_map_endpoints", {
        tileUrl: `http://localhost:${tilePort}/{z}/{x}/{y}.png`,
        attribution: "Lane tile server",
        nominatimUrl: null,
      });
    }
    evidence.mapSettings = await tryInvoke("get_map_settings");
  });

  after(() => {
    evidence.localTileRequests = tiles?.requests.length ?? null;
    writeEvidence(`${shot}.txt`, evidence);
    tiles?.close();
  });

  it("opens the map and shows either tiles or the card", async () => {
    await openMapView();
    const state = await waitForMap((s) => s.card || drew(s));
    const shows = state.card ? "card" : drew(state) ? "tiles" : "nothing";
    evidence.first = { ...state, shows };
    await screenshot(shot);
    if (process.env.EXPECT_MAP) expect(shows).toBe(process.env.EXPECT_MAP);
  });

  it("turns the map on from the card when asked", async function () {
    if (process.env.MAP_TURN_ON !== "1") this.skip();
    const clicked = await browser.execute(() => {
      const button = /** @type {HTMLButtonElement | null} */ (document.querySelector('[data-testid="online-map-turn-on"]'));
      button?.click();
      return !!button;
    });
    expect(clicked).toBe(true);
    const state = await waitForMap(drew);
    evidence.afterTurnOn = { ...state, stored: await tryInvoke("get_map_settings") };
    await screenshot(`${shot}-on`);
    expect(state.card).toBe(false);
    expect(drew(state)).toBe(true);
    if (tiles) expect(tiles.requests.length).toBeGreaterThan(0);
  });
});
