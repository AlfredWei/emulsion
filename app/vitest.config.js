import { defineConfig } from "vitest/config";

// Deliberately separate from vite.config.js: that config's sveltekit()
// plugin and Tauri-specific dev-server settings (a fixed, strict 1420
// port) have nothing to do with running plain unit tests against pure JS
// modules like $lib/cropMath.js, and reusing it here would risk Vitest
// picking up server config it doesn't need. No jsdom/browser environment
// configured either -- every test target so far is DOM-free by design
// (see cropMath.js's own module doc comment for why).
export default defineConfig({
  resolve: {
    alias: {
      $lib: new URL("./src/lib", import.meta.url).pathname,
    },
  },
  test: {
    include: ["src/**/*.test.js"],
  },
});
