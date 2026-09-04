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
  // panel-resize.e2e.js's CI-only 100%-reproducible failure (2026-09-03/04,
  // see PROGRESS.md's "M4.5 Slice 7" follow-up for the full writeup) turned
  // out to be neither a pointer-event delivery problem nor a mid-reflow
  // race -- both suspected by two prior investigation passes, neither
  // fixed it. Root cause: on GitHub's macOS e2e runner specifically,
  // `Element.getBoundingClientRect()` on the resized rail returns a value
  // frozen at the page's first layout and never reflects a later width
  // change, no matter how long you poll -- confirmed by instrumenting the
  // test to read the raw `style.width` DOM attribute (a plain synchronous
  // property write, unaffected) side by side with getBoundingClientRect()
  // across a real CI run: `style.width` updated correctly within the
  // first 50ms poll on every single one of 6 retries, while
  // getBoundingClientRect() never changed once across any of them, even
  // after 15s. Fixed by reading `style.width` instead (panel-resize.e2e.js)
  // -- not a timing problem retries could ever have papered over, so no
  // retry count was ever going to make this spec reliable on that runner.
  // Retries stay in place as a general CI safety net for other, unrelated
  // sources of flakiness, not because this specific failure needs them.
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
