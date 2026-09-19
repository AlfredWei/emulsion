import { defineConfig } from "vitest/config";
import { svelte } from "@sveltejs/vite-plugin-svelte";

/** vite-plugin-svelte compiles for the server whenever the transform is flagged `ssr`, and
 * Vitest's Node environment always flags it -- which strips `$effect`/`$effect.root` from
 * `.svelte.js` modules, so effects would silently never run under test. Forcing `ssr: false`
 * makes tests compile the same client code the app runs. */
function svelteClientForTests() {
  const force = (/** @type {any[]} */ args, /** @type {number} */ i) => {
    const a = [...args];
    a[i] = { ...(a[i] ?? {}), ssr: false };
    return a;
  };
  return svelte().map((/** @type {any} */ plugin) => {
    const wrapped = { ...plugin };
    for (const [hook, optsIndex] of /** @type {[string, number][]} */ ([["resolveId", 2], ["load", 1], ["transform", 2]])) {
      const original = plugin[hook];
      if (!original) continue;
      const fn = typeof original === "function" ? original : original.handler;
      const handler = function (/** @type {any[]} */ ...args) {
        // @ts-ignore -- `this` is Rollup's plugin context
        return fn.apply(this, force(args, optsIndex));
      };
      wrapped[hook] = typeof original === "function" ? handler : { ...original, handler };
    }
    return wrapped;
  });
}

// Deliberately separate from vite.config.js: that config's sveltekit()
// plugin and Tauri-specific dev-server settings (a fixed, strict 1420
// port) have nothing to do with running plain unit tests against pure JS
// modules like $lib/cropMath.js, and reusing it here would risk Vitest
// picking up server config it doesn't need. No jsdom/browser environment
// configured either -- every test target so far is DOM-free by design
// (see cropMath.js's own module doc comment for why).
//
// The one Svelte piece: the bare `svelte()` compiler plugin (not sveltekit()), so `.svelte.js`
// runes modules (`$state`/`$derived`/`$effect`, see lib/state/) compile under test, as *client*
// code (svelteClientForTests) against Svelte's *client* runtime (ssr.resolve.conditions) --
// otherwise reactivity would silently differ from the app (server build: `$state` is a plain
// value and effects never run). Verified by lib/state/svelteRunes.test.js.
export default defineConfig({
  plugins: [svelteClientForTests()],
  // Vitest runs modules through Vite's SSR transform, which reads ssr.resolve.conditions, not
  // resolve.conditions; without "browser" here Svelte resolves its server runtime.
  ssr: { resolve: { conditions: ["browser"] } },
  resolve: {
    alias: {
      $lib: new URL("./src/lib", import.meta.url).pathname,
    },
  },
  test: {
    include: ["src/**/*.test.js"],
  },
});
