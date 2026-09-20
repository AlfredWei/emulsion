// Print module state (RFC-0009 §3.3, P3b). Ephemeral view state, same "never persisted into the
// edit stack" treatment as Soft Proof's own state -- nothing here is part of a photo's saved edits.
//
// Workflows that use this state (print, export PDF, choose a profile) are in
// lib/actions/printActions.js. `items` is filled by the navigation code when entering Print.

/**
 * @typedef {{ path: string, version_id: number }} PrintItem
 * @typedef {"srgb" | "adobe-rgb" | "prophoto-rgb" | "custom"} PrintProfileTarget
 * @typedef {"perceptual" | "relative" | "saturation" | "absolute"} PrintIntent
 */

export class PrintStore {
  // `items` is a snapshot of Library's selection (or the open Develop image) taken when entering
  // Print (see switchModule), matching how Develop snapshots its own open image while Library's
  // grid is hidden.
  items = $state(/** @type {PrintItem[]} */ ([]));
  template = $state(/** @type {"single" | "contact-sheet"} */ ("single"));
  fitMode = $state(/** @type {"fit" | "fill"} */ ("fit"));
  rows = $state(2);
  cols = $state(2);
  cellSpacing = $state(0.1);
  paperSize = $state("letter");
  orientation = $state(/** @type {"portrait" | "landscape"} */ ("portrait"));
  margins = $state({ top: 0.5, right: 0.5, bottom: 0.5, left: 0.5 });
  colorManaged = $state(false);
  profileTarget = $state(/** @type {PrintProfileTarget} */ ("srgb"));
  customProfilePath = $state(/** @type {string | null} */ (null));
  intent = $state(/** @type {PrintIntent} */ ("relative"));
  printing = $state(false);
  exportingPdf = $state(false);
  // Populated by handlePrint right before window.print() -- swapped into PrintLayoutView's <img>
  // src in place of the live (lower-resolution) layout preview, so the actual OS print dialog sees
  // the real full-resolution, color-managed payload.
  readyUrls = $state(/** @type {Record<number, string>} */ ({}));

  colorManagementSettings = $derived(
    this.colorManaged
      ? { target: this.profileTarget, customProfilePath: this.customProfilePath, intent: this.intent }
      : null,
  );
}

export function createPrintStore() {
  return new PrintStore();
}

export const print = createPrintStore();
