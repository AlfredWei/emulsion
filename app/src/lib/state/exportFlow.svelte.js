// Export state (RFC-0009 §3.3, P6c): what an Export or Print would act on right now, and the
// Export dialog's own open/closed value. `currentItems` reads the shell (active module), the
// develop store (the open image) and the selection, so the store takes all three as constructor
// arguments (store DAG: exportFlow -> shell, develop, selection); tests build isolated ones.
//
// Clicking Export (which flushes the pending edit first) and entering Print (which snapshots
// `currentItems` once) are in lib/actions/navigation.js.

import { shell } from "./shell.svelte.js";
import { develop } from "./develop.svelte.js";
import { selection } from "./selection.svelte.js";

/** @typedef {{ path: string, version_id: number }} ExportItem */

export class ExportFlowStore {
  /**
   * @param {import('./shell.svelte.js').ShellStore} shell
   * @param {import('./develop.svelte.js').DevelopStore} develop
   * @param {import('./selection.svelte.js').SelectionStore} selection
   */
  constructor(shell, develop, selection) {
    // Set before any derived below is first read (deriveds are lazy).
    this.shell = shell;
    this.develop = develop;
    this.selection = selection;
  }

  // What Export would act on right now: the open Develop image, or every
  // selected Library image (M2 Slice 3 batch export -- the frontend-only
  // follow-up M1 Slice 5's export_batch was explicitly built to accept).
  currentItems = $derived.by(() => {
    if (this.shell.activeModule === "develop" && this.develop.versionId !== null) {
      return [{ path: this.develop.imagePath, version_id: this.develop.versionId }];
    }
    if (this.selection.selectedImages.length > 0) {
      return this.selection.selectedImages.map((img) => ({ path: img.path, version_id: img.version_id }));
    }
    return this.selection.selectedImage ? [{ path: this.selection.selectedImage.path, version_id: this.selection.selectedImage.version_id }] : [];
  });

  /** The Export dialog's items; `null` is the "closed" sentinel -- never open with an empty list. */
  items = $state(/** @type {ExportItem[] | null} */ (null));
}

/**
 * @param {import('./shell.svelte.js').ShellStore} s
 * @param {import('./develop.svelte.js').DevelopStore} d
 * @param {import('./selection.svelte.js').SelectionStore} sel
 */
export function createExportFlowStore(s, d, sel) {
  return new ExportFlowStore(s, d, sel);
}

export const exportFlow = createExportFlowStore(shell, develop, selection);
