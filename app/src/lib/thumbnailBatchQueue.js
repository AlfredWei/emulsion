// Thumbnail batch update queue: coalesces multiple regeneration requests
// into a single atomic update across all components consuming thumbnails.
//
// Why batching matters: without this, rapid edits (slider drags, undo/redo,
// preset application) trigger individual regenerateThumbnail() calls that each
// update local state independently. Components consuming thumbnails see
// staggered updates, causing visible refresh delays and inconsistency between
// grid/filmstrip/metadata panels showing the same image.
//
// With batching: multiple regeneration requests are coalesced into a single
// Promise.all() operation that updates all thumbnails at once, so all
// components see a synchronized refresh.

import { regenerateThumbnail } from './api/develop.js';

/** @type {Set<number>} */
let pendingVersionIds = new Set();

/** @type {ReturnType<typeof setTimeout> | null} */
let debounceTimer = null;

const DEBOUNCE_DELAY_MS = 150;

/**
 * Register a version ID for thumbnail regeneration. Multiple calls to this
 * function with the same version ID (within the debounce window) only trigger
 * ONE regeneration, not duplicates.
 *
 * @param {number} versionId
 * @param {(results: Map<number, string | null>) => void} onBatchComplete
 *   Called once when the batch completes. Map keys are version IDs,
 *   values are the new thumbnail_path (or null if regeneration failed).
 */
export function queueThumbnailRegeneration(versionId, onBatchComplete) {
  if (!versionId || versionId === null) return;

  pendingVersionIds.add(versionId);

  if (debounceTimer !== null) {
    clearTimeout(debounceTimer);
  }

  debounceTimer = setTimeout(() => {
    debounceTimer = null;
    processBatch(onBatchComplete);
  }, DEBOUNCE_DELAY_MS);
}

/**
 * Force immediate processing of any pending thumbnails (don't wait for debounce).
 * Used when switching away from Develop or closing the app.
 *
 * @param {(results: Map<number, string | null>) => void} onBatchComplete
 */
export function flushThumbnailBatch(onBatchComplete) {
  if (debounceTimer !== null) {
    clearTimeout(debounceTimer);
    debounceTimer = null;
  }

  if (pendingVersionIds.size > 0) {
    processBatch(onBatchComplete);
  }
}

/**
 * Internal: execute the batch regeneration.
 *
 * @param {(results: Map<number, string | null>) => void} onBatchComplete
 */
async function processBatch(onBatchComplete) {
  const versionsToRegen = Array.from(pendingVersionIds);
  pendingVersionIds.clear();

  if (versionsToRegen.length === 0) return;

  try {
    const promises = versionsToRegen.map((vid) =>
      regenerateThumbnail(vid)
        .then((path) => ({ versionId: vid, path }))
        .catch(() => ({ versionId: vid, path: null }))
    );

    const results = await Promise.all(promises);
    const resultMap = new Map(results.map((r) => [r.versionId, r.path]));

    onBatchComplete(resultMap);
  } catch {
    // Best-effort; stale grid thumbnails aren't worth surfacing errors for
  }
}

/**
 * Return the current pending batch size (for testing/debugging).
 * @returns {number}
 */
export function getPendingBatchSize() {
  return pendingVersionIds.size;
}

/**
 * Clear any pending regenerations and cancel the debounce (for testing).
 */
export function clearPendingBatch() {
  if (debounceTimer !== null) {
    clearTimeout(debounceTimer);
    debounceTimer = null;
  }
  pendingVersionIds.clear();
}
