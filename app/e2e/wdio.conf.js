import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));

// Debug binary built with `npm run tauri build -- --debug --no-bundle
// --features wdio-webdriver --config src-tauri/tauri.e2e.conf.json` (see
// package.json's "test:e2e" script, which runs that build first). The
// wdio-webdriver feature/capability pair is what embeds the WebDriver
// server this config talks to -- a plain `npm run tauri dev` build does
// not have it and cannot be driven by this suite.
const appBinaryPath = path.resolve(__dirname, "../src-tauri/target/debug/emulsion");

export const config = {
  runner: "local",
  specs: ["./specs/**/*.e2e.js"],
  maxInstances: 1,

  services: [
    [
      "@wdio/tauri-service",
      {
        appBinaryPath,
        driverProvider: "embedded",
        startTimeout: 60000,
      },
    ],
  ],

  capabilities: [
    {
      browserName: "tauri",
      "tauri:options": {
        application: appBinaryPath,
      },
    },
  ],

  logLevel: "warn",
  bail: 0,
  waitforTimeout: 10000,
  connectionRetryTimeout: 90000,
  connectionRetryCount: 3,
  // panel-resize.e2e.js occasionally sees a synthetic drag's pointer
  // events silently have no effect on CI's macOS runner -- confirmed via
  // a per-test mocha retry (this.retries()) that it's NOT a rare,
  // independent miss: once one call in a session misses, every retry of
  // the same test in that same session fails identically, pointing at
  // some accumulated per-session state rather than a one-off timing
  // fluke. A per-test retry that reuses the same session can't recover
  // from that; retrying the whole spec file gets a genuinely fresh
  // session/webview instead.
  specFileRetries: 2,

  framework: "mocha",
  mochaOpts: {
    ui: "bdd",
    // @wdio/tauri-service's own internal window-focus bookkeeping
    // (getWindowStates/ensureActiveWindowFocus) polls for the *other*
    // Tauri testing plugin (tauri-plugin-wdio, not installed here -- see
    // golden-path.e2e.js's file comment) before every single command,
    // retrying for 5s each time it's absent. That's a real per-command tax
    // in this setup, not a hang, but it adds up across a multi-step test.
    timeout: 120000,
  },

  reporters: ["spec"],
};
