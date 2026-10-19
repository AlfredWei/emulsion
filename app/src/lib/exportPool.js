// Runs the Export dialog's one-photo-per-call exports several at a time (RFC-0030 slice F). The backend decides how
// many actually render at once (cores and memory, `export::ExportBudget`), so the pool only has to keep that many
// calls in flight; results come back in the order of the items, whatever order they finish in.

/** How many calls to keep in flight: enough to feed the backend's limit (at most 8), never more than the photos.
 * @param {number} itemCount */
export function exportConcurrency(itemCount) {
  return Math.max(1, Math.min(8, itemCount));
}

/**
 * @template T, R
 * @param {T[]} items
 * @param {number} concurrency
 * @param {(item: T, index: number) => Promise<R>} run  one photo; must not reject (turn a failure into a result)
 * @param {{ shouldStop?: () => boolean, onStart?: (item: T, index: number) => void, onDone?: (item: T, index: number, result: R) => void }} [hooks]
 *   `shouldStop` is asked before each photo starts: once it says yes no new photo starts, those running finish.
 * @returns {Promise<{ results: (R | undefined)[], started: number }>} `results[i]` is undefined for a photo that never started
 */
export async function runPool(items, concurrency, run, hooks = {}) {
  /** @type {(R | undefined)[]} */
  const results = new Array(items.length).fill(undefined);
  let next = 0;
  let started = 0;
  async function worker() {
    for (;;) {
      if (hooks.shouldStop?.()) return;
      const i = next++;
      if (i >= items.length) return;
      started++;
      hooks.onStart?.(items[i], i);
      const result = await run(items[i], i);
      results[i] = result;
      hooks.onDone?.(items[i], i, result);
    }
  }
  await Promise.all(Array.from({ length: Math.max(1, Math.min(concurrency, items.length)) }, worker));
  return { results, started };
}
