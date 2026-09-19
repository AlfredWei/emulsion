import { it, expect } from "vitest";
import { flushSync } from "svelte";
import { createCounter, trackDouble } from "./svelteRunesFixture.svelte.js";

// Guards the vitest.config.js setup: if runes ever compile for the server (or resolve Svelte's
// server runtime), `$effect` silently never runs and store tests would pass vacuously.
it("compiles runes", () => {
  const c = createCounter();
  c.inc();
  c.inc();
  expect(c.n).toBe(2);
  expect(c.double).toBe(4);
});

it("effects registered by an install-style function track store fields", () => {
  const c = createCounter();
  const seen = /** @type {number[]} */ ([]);
  const stop = trackDouble(c, seen);
  flushSync();
  c.inc();
  flushSync();
  c.inc();
  flushSync();
  stop();
  c.inc();
  flushSync();
  expect(seen).toEqual([0, 2, 4]);
});
