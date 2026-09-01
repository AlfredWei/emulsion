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
  // events silently have no effect on CI's macOS runner -- confirmed
  // across many CI runs that it's a real per-call miss rate (rough
  // estimate ~20-25% per dragHandle call, ~4 calls per full pass), not
  // tied to a fixed position or to accumulated per-session state: the
  // very first drag in a brand-new session has failed on some runs, and
  // a same-session mocha retry (this.retries()) failed identically on
  // every attempt on others, ruling both of those theories out. Root
  // cause not pinned down (plausibly WebKit's setPointerCapture handling
  // of fully synthetic, non-trusted PointerEvents under this runner's
  // load -- see the wdio-webdriver ensureActiveWindowFocus tax below);
  // a real WebDriver-native pointer action (rather than
  // element.dispatchEvent) is the likely actual fix and hasn't been
  // tried. specFileRetries gives each attempt a fresh session, unlike a
  // same-session retry, and 6 retries (7 attempts total) pushes the
  // chance of every attempt hitting a miss down to low single digits
  // given the per-call rate above -- a statistical stopgap, not a fix.
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
