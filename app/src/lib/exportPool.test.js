import { describe, it, expect } from "vitest";
import { exportConcurrency, runPool } from "./exportPool.js";

const tick = (/** @type {number} */ ms) => new Promise((r) => setTimeout(r, ms));

describe("exportConcurrency", () => {
  it("is the number of photos up to eight, never below one", () => {
    expect(exportConcurrency(0)).toBe(1);
    expect(exportConcurrency(1)).toBe(1);
    expect(exportConcurrency(5)).toBe(5);
    expect(exportConcurrency(500)).toBe(8);
  });
});

describe("runPool", () => {
  it("returns the results in item order whatever order they finish in", async () => {
    const { results, started } = await runPool([30, 5, 15, 1], 4, async (ms) => {
      await tick(ms);
      return `done-${ms}`;
    });
    expect(results).toEqual(["done-30", "done-5", "done-15", "done-1"]);
    expect(started).toBe(4);
  });

  it("never has more than `concurrency` photos in flight", async () => {
    let running = 0;
    let peak = 0;
    await runPool(Array.from({ length: 12 }, (_, i) => i), 3, async () => {
      running++;
      peak = Math.max(peak, running);
      await tick(5);
      running--;
      return 1;
    });
    expect(peak).toBe(3);
  });

  it("reports each start and each completion", async () => {
    const started = /** @type {number[]} */ ([]);
    const done = /** @type {number[]} */ ([]);
    await runPool(["a", "b", "c"], 2, async (x) => x.toUpperCase(), { onStart: (_, i) => started.push(i), onDone: (_, i) => done.push(i) });
    expect(started.sort()).toEqual([0, 1, 2]);
    expect(done.sort()).toEqual([0, 1, 2]);
  });

  it("stops starting new photos once asked, lets the running ones finish, and leaves the rest undefined", async () => {
    let stop = false;
    const { results, started } = await runPool([1, 2, 3, 4, 5, 6], 2, async (n) => {
      if (n === 2) stop = true;
      await tick(5);
      return n * 10;
    }, { shouldStop: () => stop });
    expect(started).toBeGreaterThanOrEqual(2);
    expect(started).toBeLessThan(6);
    expect(results.slice(0, started).every((r) => r !== undefined)).toBe(true);
    expect(results.slice(started).every((r) => r === undefined)).toBe(true);
  });

  it("handles an empty list", async () => {
    expect(await runPool([], 4, async () => 1)).toEqual({ results: [], started: 0 });
  });
});
