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

/** Minimum pointer travel, in pixels, before a Filmstrip cell's pointerdown becomes a photo drag
 * rather than the plain click that selects/opens it (see `photoDrag` below). */
const PHOTO_DRAG_THRESHOLD = 4;

/**
 * @typedef {{ which: "history" | "develop", startX: number, startWidth: number } | null} PanelResizeState
 * @typedef {{ imageIds: number[], pointer: { x: number, y: number } } | null} PhotoDragState
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
  /** A photo drag toward the world map (M5.5), non-null only once the pointer has actually moved past
   * `PHOTO_DRAG_THRESHOLD` -- see the pointer handlers below for why this can't be native HTML5
   * drag-and-drop. `pointer` is viewport (`clientX`/`clientY`) coordinates; a consumer with its own
   * bounding rect (`LibraryMapView`) converts that itself. Lives here, not on `library` or a
   * Filmstrip prop, because it's a plain cross-component pointer-interaction state, exactly like
   * `panelResizeState` above -- Develop's own Filmstrip renders cells with the same drag handlers but
   * nothing ever reacts to this while Develop is open, since there is no map to drop them on. */
  photoDrag = $state(/** @type {PhotoDragState} */ (null));
  /** @type {{ pointerId: number, startX: number, startY: number, imageIds: number[], el: HTMLElement } | null} */
  #photoDragCandidate = null;

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

  // Dragging a Filmstrip photo onto the world map (M5.5, RFC-0007): plain pointer events, not native
  // HTML5 drag-and-drop. Tauri's window-level drag-drop interception (`onDragDropEvent` in
  // appEvents.js, needed for real OS file imports since a webview's own File API can't give a real
  // path on every platform) takes over the window's native drag handling and, in practice, stops the
  // browser's own `dragstart`/`dragover`/`drop` DOM events from firing at all for a drag that never
  // leaves the page -- so an HTML5-DnD version of this looked fine outside Tauri (no such
  // interception there) and did nothing inside it. Pointer events sidestep that path entirely, the
  // same reason DevelopCanvas's crop handles and the panel resize above already use them.
  //
  // Arms on pointerdown, but does nothing yet -- `#photoDragCandidate` only remembers where the
  // gesture started, so a plain click (no real movement) still just runs the cell's own
  // onclick/ondblclick untouched. `photoDrag` only becomes non-null once the pointer has actually
  // moved past `PHOTO_DRAG_THRESHOLD`, which is also when pointer capture is taken (so the rest of
  // the gesture keeps reporting to the originating cell no matter where the pointer physically is).
  handlePhotoDragPointerDown = (/** @type {PointerEvent} */ e, /** @type {number[]} */ imageIds) => {
    if (e.button !== 0) return;
    this.#photoDragCandidate = {
      pointerId: e.pointerId,
      startX: e.clientX,
      startY: e.clientY,
      imageIds,
      el: /** @type {HTMLElement} */ (e.currentTarget),
    };
  };

  handlePhotoDragPointerMove = (/** @type {PointerEvent} */ e) => {
    const c = this.#photoDragCandidate;
    if (!c || e.pointerId !== c.pointerId) return;
    if (!this.photoDrag) {
      if (Math.hypot(e.clientX - c.startX, e.clientY - c.startY) < PHOTO_DRAG_THRESHOLD) return;
      try {
        c.el.setPointerCapture(c.pointerId);
      } catch {
        // Non-fatal -- see DevelopCanvas's crop-handle drag for why.
      }
      this.photoDrag = { imageIds: c.imageIds, pointer: { x: e.clientX, y: e.clientY } };
      return;
    }
    this.photoDrag = { ...this.photoDrag, pointer: { x: e.clientX, y: e.clientY } };
  };

  /** Just ends the gesture -- whether it was "dropped" on anything is for a consumer (`LibraryMapView`)
   * to decide, reactively, from `photoDrag` going back to `null` while its own last-known hover state
   * said the pointer was over it. */
  handlePhotoDragPointerUp = (/** @type {PointerEvent} */ e) => {
    const c = this.#photoDragCandidate;
    if (!c || e.pointerId !== c.pointerId) return; // an unrelated pointer's up leaves the real one alone
    if (this.photoDrag) {
      try {
        c.el.releasePointerCapture(c.pointerId);
      } catch {
        // Non-fatal, as above.
      }
    }
    this.#photoDragCandidate = null;
    this.photoDrag = null;
  };
}

/** @param {ShellInitial} initial */
export function createShellStore(initial) {
  return new ShellStore(initial);
}

export const shell = createShellStore({ panelWidths: getStoredPanelWidths(), shortcuts: getStoredShortcuts() });
