import net from "node:net";
import path from "node:path";

// Config for lane runs (.github/workflows/lanes.yml, scripts/lanes/). It drives any drivable binary, a sandboxed .app
// or a plain debug build, named by APP_BIN. LANE_OUT, when set, receives a screenshot after every test and at the end.
// The embedded WebDriver listens on one fixed port, so a run refuses to start while another app holds it.
// Specs are chosen with --spec. There are no spec retries, so a lane reports what happened on its first run.
const appBinaryPath = process.env.APP_BIN;
if (!appBinaryPath) throw new Error("APP_BIN must name the app executable to drive");

const laneOut = process.env.LANE_OUT;
const slug = (file) => path.basename(file ?? "spec").replace(/\.e2e\.js$/, "");
let tests = 0;

const embeddedPort = 4445;
const portTaken = () =>
  new Promise((resolve) => {
    const socket = net.connect(embeddedPort, "127.0.0.1");
    socket.once("connect", () => (socket.destroy(), resolve(true)));
    socket.once("error", () => resolve(false));
  });

export const config = {
  runner: "local",
  specs: ["./specs/sandbox-smoke.e2e.js"],
  maxInstances: 1,

  services: [["@wdio/tauri-service", { appBinaryPath, driverProvider: "embedded", startTimeout: 90000 }]],
  capabilities: [{ browserName: "tauri", "tauri:options": { application: appBinaryPath } }],

  logLevel: "warn",
  waitforTimeout: 10000,
  connectionRetryTimeout: 90000,
  connectionRetryCount: 3,
  specFileRetries: 0,

  framework: "mocha",
  mochaOpts: { ui: "bdd", timeout: 600000 },
  reporters: ["spec"],

  onPrepare: async () => {
    if (await portTaken()) {
      console.error(`port ${embeddedPort} is taken by another app with the embedded WebDriver, and this run would drive that app instead of its own`);
      process.exit(1);
    }
  },
  afterTest: async (test, _context, { passed }) => {
    if (!laneOut) return;
    tests += 1;
    await browser.saveScreenshot(path.join(laneOut, `${slug(test.file)}-${tests}${passed ? "" : "-failed"}.png`));
  },
  after: async (_result, _capabilities, specs) => {
    if (!laneOut) return;
    await browser.saveScreenshot(path.join(laneOut, `${slug(specs[0])}.png`));
  },
};
