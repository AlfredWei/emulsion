import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { ensureFixture, mapState, openMapView, screenshot, waitForMap, writeEvidence } from "../laneMap.js";

// Lane spec for the online map default in a sandboxed release bundle (lanes.yml builds it for `sandbox-*` specs on
// macOS). It opens the map once, which must show the card, then lists the app's sockets every second for a minute.
// Passes when no socket of the app or its helper connects to an address off this computer. Driven by
// wdio.sandbox.conf.js with APP_BIN.

const FIXTURE = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../../../test_image/Cavenagh-Bridge-Singapore-panorama-frame-a.jpg");
const SAMPLES = 60;
const run = (/** @type {string} */ cmd, /** @type {string[]} */ args) => {
  try {
    return execFileSync(cmd, args, { encoding: "utf8" });
  } catch (e) {
    return /** @type {{ stdout?: string }} */ (e).stdout ?? "";
  }
};
const appPids = () => run("pgrep", ["-f", `${process.env.APP_BIN}|emulsion-ai`]).split("\n").filter(Boolean);
const remote = (/** @type {string} */ line) => /->(?!127\.0\.0\.1|\[::1\]|localhost)/.test(line);

describe("Sandboxed release build, first minute", () => {
  before(async () => {
    await browser.setTimeout({ script: 120000 });
    // The map is reachable only with a photo in the catalog, and the sandbox reads only inside its container.
    const bundleId = process.env.LANE_BUNDLE_ID;
    if (!bundleId) throw new Error("LANE_BUNDLE_ID must name the sandboxed app's bundle identifier");
    const inside = path.join(os.homedir(), "Library", "Containers", bundleId, "Data", "Documents", "lane-fixtures");
    fs.mkdirSync(inside, { recursive: true });
    fs.copyFileSync(FIXTURE, path.join(inside, path.basename(FIXTURE)));
    await ensureFixture(path.join(inside, path.basename(FIXTURE)));
  });

  it("shows the map card and makes no outbound connection", async () => {
    await openMapView();
    const state = await waitForMap((s) => s.card || s.tilesLoaded > 0);
    await screenshot("sandbox-map-first");
    expect(state.card).toBe(true);

    /** @type {string[]} */
    const lines = [];
    const started = new Date();
    for (let i = 1; i <= SAMPLES; i++) {
      const pids = appPids();
      lines.push(`t=${i} pids=${pids.join(",")}`);
      if (pids.length) lines.push(...run("lsof", ["-nP", "-i", "-a", "-p", pids.join(",")]).split("\n").slice(1).filter(Boolean).map((l) => `  ${l}`));
      await browser.pause(1000);
    }
    const outbound = lines.filter((l) => l.startsWith("  ") && remote(l));
    const networkLog = run("/usr/bin/log", [
      "show",
      "--start",
      started.toISOString().replace("T", " ").slice(0, 19),
      "--predicate",
      'process == "emulsion" AND (subsystem == "com.apple.network" OR subsystem == "com.apple.CFNetwork")',
      "--style",
      "compact",
    ])
      .split("\n")
      .filter((l) => /nw_connection|connect|tcp|https?:\/\//i.test(l));
    writeEvidence(
      "no-network.txt",
      [
        `samples: ${SAMPLES} at 1 s from ${started.toISOString()}`,
        `map after the minute: ${JSON.stringify(await mapState())}`,
        `outbound connections: ${outbound.length ? "\n" + outbound.join("\n") : "none"}`,
        `unified log network lines for the app: ${networkLog.length ? "\n" + networkLog.slice(0, 50).join("\n") : "none"}`,
        "",
        ...lines,
      ].join("\n") + "\n",
    );
    expect(outbound).toEqual([]);
  });
});
