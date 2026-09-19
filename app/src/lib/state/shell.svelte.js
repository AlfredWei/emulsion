// App-shell state (RFC-0009 §3.3, P3): which module is showing, the status line, the settings
// dialog, the resizable Develop rails, and the user's keyboard bindings.
//
// One instance for the (single-window) app, plus `createShellStore()` so tests get a fresh one.
// The singleton reads two persisted UI preferences (panel widths, shortcuts) from localStorage
// when it is created; both loaders fall back to defaults if storage is missing or unreadable.
// It does nothing else at import time.

import { getStoredShortcuts } from "$lib/shortcuts.js";
import {
  getStoredPanelWidths,
  saveStoredPanelWidths,
  clamp as clampPanelWidth,
  HISTORY_PANEL_MIN_WIDTH,
  HISTORY_PANEL_MAX_WIDTH,
  DEVELOP_PANEL_MIN_WIDTH,
  DEVELOP_PANEL_MAX_WIDTH,
} from "$lib/panelLayout.js";

/**
 * @typedef {{ which: "history" | "develop", startX: number, startWidth: number } | null} PanelResizeState
 * @typedef {Object} ShellInitial
 * @property {{ history: number, develop: number }} panelWidths
 * @property {Record<string, string>} shortcuts
 */

export class ShellStore {
  /** "library" | "develop" | "print" | "people" -- written only by the navigation orchestrator
   * (openDevelop/switchModule) and the native-menu/keyboard handlers. */
  activeModule = $state("library");
  statusMessage = $state("");
  /** General Settings dialog: app-level, not module-scoped, so it is not gated on activeModule. */
  settingsOpen = $state(false);
  /** M4.5: Develop's left (History) and right (adjustments) rail widths, drag-resizable and
   * persisted across sessions -- see panelLayout.js. */
  panelWidths = $state(/** @type {{ history: number, develop: number }} */ ({ history: 0, develop: 0 }));
  panelResizeState = $state(/** @type {PanelResizeState} */ (null));
  shortcuts = $state(/** @type {Record<string, string>} */ ({}));

  /** @param {ShellInitial} initial */
  constructor(initial) {
    this.panelWidths = initial.panelWidths;
    this.shortcuts = initial.shortcuts;
  }

  /** Replaces every direct `statusMessage = ...` write the page used to make. */
  notify(/** @type {string} */ message) {
    this.statusMessage = message;
  }

  // M4.5: drag-resize for Develop's History (left) and adjustments (right)
  // rails -- same pointerdown/pointermove/pointerup + setPointerCapture
  // skeleton as DevelopCanvas.svelte's crop-handle dragging (the
  // try/catch there is for the same reason: setPointerCapture can throw
  // and must not abort the drag-state assignment).
  // Arrow fields, not methods, so they can be handed to `onpointermove={...}` unbound.
  handlePanelResizePointerDown = (/** @type {PointerEvent} */ e, /** @type {"history" | "develop"} */ which) => {
    e.preventDefault();
    this.panelResizeState = { which, startX: e.clientX, startWidth: this.panelWidths[which] };
    try {
      /** @type {HTMLElement} */ (e.currentTarget).setPointerCapture(e.pointerId);
    } catch {
      // Non-fatal -- see DevelopCanvas's crop-handle drag for why.
    }
  };

  handlePanelResizePointerMove = (/** @type {PointerEvent} */ e) => {
    if (!this.panelResizeState) return;
    const { which, startX, startWidth } = this.panelResizeState;
    // History sits on the left (dragging right grows it); the adjustments
    // panel sits on the right (dragging right shrinks it) -- opposite sign.
    const dx = e.clientX - startX;
    const delta = which === "history" ? dx : -dx;
    const [min, max] =
      which === "history" ? [HISTORY_PANEL_MIN_WIDTH, HISTORY_PANEL_MAX_WIDTH] : [DEVELOP_PANEL_MIN_WIDTH, DEVELOP_PANEL_MAX_WIDTH];
    this.panelWidths = { ...this.panelWidths, [which]: clampPanelWidth(startWidth + delta, min, max) };
  };

  handlePanelResizePointerUp = (/** @type {PointerEvent} */ e) => {
    try {
      /** @type {HTMLElement} */ (e.currentTarget).releasePointerCapture(e.pointerId);
    } catch {
      // Non-fatal -- see DevelopCanvas's crop-handle drag for why.
    }
    if (this.panelResizeState) saveStoredPanelWidths(this.panelWidths);
    this.panelResizeState = null;
  };
}

/** @param {ShellInitial} initial */
export function createShellStore(initial) {
  return new ShellStore(initial);
}

export const shell = createShellStore({ panelWidths: getStoredPanelWidths(), shortcuts: getStoredShortcuts() });
