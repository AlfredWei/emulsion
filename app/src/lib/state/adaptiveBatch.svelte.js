// The Library's "Apply Adaptive Preset to Selection" run (RFC-0035 B3). One dialog with three phases: ask Amount
// ("confirm"), the photos one by one ("running"), and the report ("report"). View state only; nothing is persisted.

export class AdaptiveBatchStore {
  open = $state(false);
  /** @type {"confirm" | "running" | "report"} */
  phase = $state("confirm");
  presetId = $state("");
  amount = $state(100);
  /** Version ids of the photos this run was asked to touch. */
  targets = $state(/** @type {number[]} */ ([]));
  done = $state(0);
  cancelRequested = $state(false);
  /** @type {import('$lib/adaptivePresets.js').BatchRow[]} */
  rows = $state([]);
}

export const adaptiveBatch = new AdaptiveBatchStore();
