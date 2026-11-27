import fs from "node:fs";
import http from "node:http";
import net from "node:net";
import os from "node:os";
import path from "node:path";

// Shared by the map lane specs (map-online-first, map-online-switch). They run through wdio.sandbox.conf.js against a
// release or debug binary named by APP_BIN, and write their evidence to LANE_OUT.

export const laneOut = process.env.LANE_OUT ?? fs.mkdtempSync(path.join(os.tmpdir(), "map-lane-"));
fs.mkdirSync(laneOut, { recursive: true });

export const writeEvidence = (/** @type {string} */ name, /** @type {unknown} */ value) =>
  fs.writeFileSync(path.join(laneOut, name), typeof value === "string" ? value : JSON.stringify(value, null, 2) + "\n");

export const screenshot = (/** @type {string} */ name) => browser.saveScreenshot(path.join(laneOut, `${name}.png`));

export const invoke = (/** @type {string} */ cmd, /** @type {Record<string, unknown>} */ args = {}) =>
  browser.execute(async (c, a) => window.__TAURI__.core.invoke(c, a), cmd, args);

/**
 * Like `invoke`, but resolves to `{ ok, value }` or `{ ok: false, reason }` instead of throwing. The key is not
 * `error`: the WebDriver client reports a script result that holds an `error` string as a failed script (measured).
 */
export const tryInvoke = (/** @type {string} */ cmd, /** @type {Record<string, unknown>} */ args = {}) =>
  browser.execute(
    async (c, a) => {
      try {
        return { ok: true, value: await window.__TAURI__.core.invoke(c, a) };
      } catch (e) {
        return { ok: false, reason: String(e) };
      }
    },
    cmd,
    args,
  );

/** A tile server on this computer: each tile is a labelled green square, and every request is recorded. */
export function startTileServer(/** @type {number} */ port) {
  /** @type {string[]} */
  const requests = [];
  const server = http.createServer((req, res) => {
    requests.push(req.url ?? "");
    const label = (req.url ?? "").replace(/\.png.*$/, "");
    res.writeHead(200, { "Content-Type": "image/svg+xml", "Cache-Control": "no-store" });
    res.end(
      `<svg xmlns="http://www.w3.org/2000/svg" width="256" height="256"><rect width="256" height="256" fill="#2e7d5b" stroke="#ffffff"/>` +
        `<text x="12" y="32" font-size="18" fill="#ffffff" font-family="sans-serif">lane tile ${label}</text></svg>`,
    );
  });
  return new Promise((resolve) => server.listen(port, "127.0.0.1", () => resolve({ requests, close: () => server.close() })));
}

/**
 * A forwarding proxy that records every host the app's backend reaches through it. The app reads HTTPS_PROXY when it
 * starts, so the lane sets HTTPS_PROXY=http://127.0.0.1:<port> before wdio launches the app. The webview does not use it;
 * its tile requests are read from the page's resource timing instead (`mapState`).
 */
export function startRequestLog(/** @type {number} */ port) {
  /** @type {string[]} */
  const hosts = [];
  const server = http.createServer((req, res) => {
    hosts.push(req.headers.host ?? req.url ?? "");
    res.writeHead(502).end();
  });
  server.on("connect", (req, client, head) => {
    hosts.push(req.url ?? "");
    const [host, p] = (req.url ?? "").split(":");
    const upstream = net.connect(Number(p) || 443, host, () => {
      client.write("HTTP/1.1 200 Connection Established\r\n\r\n");
      upstream.write(head);
      upstream.pipe(client);
      client.pipe(upstream);
    });
    upstream.on("error", () => client.destroy());
    client.on("error", () => upstream.destroy());
  });
  return new Promise((resolve) => server.listen(port, "127.0.0.1", () => resolve({ hosts, close: () => server.close() })));
}

/** What the map view shows now, and every tile address the page has requested since it loaded. */
export const mapState = () =>
  browser.execute(() => ({
    card: !!document.querySelector('[data-testid="online-map-card"]'),
    leaflet: !!document.querySelector(".leaflet-map-pane"),
    tilesLoaded: document.querySelectorAll(".leaflet-tile-loaded").length,
    tileRequests: performance
      .getEntriesByType("resource")
      .map((e) => e.name)
      .filter((n) => /\/\d+\/\d+\/\d+(\.png)?(\?|$)/.test(n)),
  }));

/** Polls `mapState` until `done` holds, and returns the last state either way. */
export async function waitForMap(/** @type {(s: Awaited<ReturnType<typeof mapState>>) => boolean} */ done, timeout = 30000) {
  const deadline = Date.now() + timeout;
  let state = await mapState();
  while (!done(state) && Date.now() < deadline) {
    await browser.pause(250);
    state = await mapState();
  }
  return state;
}

export async function openMapView() {
  await browser.execute(() => {
    performance.setResourceTimingBufferSize(10000);
    /** @type {HTMLElement | null} */ (document.querySelector('button[title^="Map View"]'))?.click();
  });
}

export async function openMapSettings() {
  await browser.execute(() => window.__TAURI__.event.emit("menu-action", "preferences"));
  await browser.pause(300);
  await browser.execute(() => {
    /** @type {HTMLElement | undefined} */ (Array.from(document.querySelectorAll(".tab-btn")).find((b) => b.textContent === "Map"))?.click();
  });
  await browser.pause(300);
}

export async function closeSettings() {
  await browser.execute(() => document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })));
  await browser.pause(300);
}

/** Imports the Singapore fixture once, so the map has a pin and the metadata panel has a photo. */
export async function ensureFixture(/** @type {string} */ fixturePath) {
  const images = await invoke("list_images");
  if (images.length === 0) await invoke("import_files", { paths: [fixturePath] });
}
