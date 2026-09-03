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
  // events silently have no effect (or a transient mid-reflow width) on
  // CI's macOS runner -- confirmed across many CI runs that every attempt
  // tends to have at least one miss (7/7 full-session attempts each failed
  // at least one of the 4 dragHandle calls with specFileRetries: 6), too
  // consistent to be independent bad luck. The dispatched PointerEvents
  // were missing `isPrimary`/`pointerType` (defaults to false/"" without
  // them, unlike a real mouse pointer), which WebKit's setPointerCapture
  // handling (handlePanelResizePointerDown/Up, +page.svelte) may not treat
  // consistently for a non-primary synthetic pointer -- fixed by setting
  // both explicitly in panel-resize.e2e.js, which measurably helped (CI
  // runs went from 0/N clean attempts to a mix of pass/fail per test) but,
  // confirmed on this project's first real post-billing-fix CI run
  // (2026-09-03), did NOT fully close the gap on its own: all 4 attempts
  // at specFileRetries: 3 still had at least one of the two tests fail,
  // each landing on a width that matched neither the pre-drag value nor
  // any legal clamped/dragged outcome -- consistent with the dragHandle
  // poll (panel-resize.e2e.js) occasionally reading a mid-reflow width,
  // now also hardened to require two consecutive matching reads. NOT a
  // case for switching to WebdriverIO's native pointer actions instead of
  // dispatchEvent: helpers.js documents the opposite finding already
  // (real WebDriver clicks are the *unreliable* mechanism in this
  // embedded-WebDriver + WKWebView combination, dispatchEvent is the
  // established workaround). Back to the original, empirically-needed
  // retry count as a safety net now that we have direct evidence 3 isn't
  // consistently enough.
  specFileRetries: 6,

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
