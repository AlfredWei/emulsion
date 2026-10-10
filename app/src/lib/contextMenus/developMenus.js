// Right-click menus of the Develop surfaces (RFC-0028 §3.3, slice 2b): a panel header, the canvas, the selected
// mask's panel and one of its shape rows. Pure builders over injected commands, like the others.

import { SEPARATOR, tidy } from "./model.js";

/**
 * @typedef {Object} PanelMenuCommands
 * @property {() => void} toggleVisibility
 * @property {() => void} reset
 * @property {() => void} solo
 * @property {() => void} showAll
 */

/**
 * @param {{ title: string, hidden: boolean, othersVisible: boolean, anyHidden: boolean }} panel
 * @param {PanelMenuCommands} cmd
 * @returns {import('./model.js').MenuEntry[]}
 */
export function buildPanelHeaderMenu({ title, hidden, othersVisible, anyHidden }, cmd) {
  const soloUseless = !othersVisible && !hidden;
  return tidy([
    { id: "toggle-panel", label: hidden ? `Show ${title}` : `Hide ${title}`, run: cmd.toggleVisibility },
    // Undoable from History, so no confirmation (the panel's own reset button does not ask either).
    { id: "reset-panel", label: `Reset ${title}`, run: cmd.reset },
    SEPARATOR,
    { id: "solo-panel", label: `Solo ${title}`, disabled: soloUseless, reason: soloUseless ? "It is the only visible panel" : undefined, run: cmd.solo },
    { id: "show-all-panels", label: "Show All Panels", disabled: !anyHidden, reason: anyHidden ? undefined : "No panel is hidden", run: cmd.showAll },
  ]);
}

/**
 * @typedef {Object} CanvasMenuCommands
 * @property {() => void} undo
 * @property {() => void} redo
 * @property {() => void} toggleBeforeAfter
 * @property {() => void} zoomFit
 * @property {() => void} zoomActual
 * @property {() => void} toggleClipping
 * @property {() => void} toggleMaskOverlay
 * @property {() => void} copySettings
 * @property {() => void} pasteSettings
 * @property {() => void} resetSettings  opens the existing reset confirmation
 */

/**
 * `undoLabel` / `redoLabel` are the history entries those would step over (null: nothing to undo / redo).
 * @param {{ undoLabel: string | null, redoLabel: string | null, showOriginal: boolean, clipping: boolean, maskOverlay: boolean, hasMasks: boolean, hasCopiedSettings: boolean }} state
 * @param {CanvasMenuCommands} cmd
 * @returns {import('./model.js').MenuEntry[]}
 */
export function buildCanvasMenu({ undoLabel, redoLabel, showOriginal, clipping, maskOverlay, hasMasks, hasCopiedSettings }, cmd) {
  return tidy([
    { id: "undo", label: undoLabel ? `Undo ${undoLabel}` : "Undo", disabled: !undoLabel, reason: undoLabel ? undefined : "Nothing to undo", run: cmd.undo },
    { id: "redo", label: redoLabel ? `Redo ${redoLabel}` : "Redo", disabled: !redoLabel, reason: redoLabel ? undefined : "Nothing to redo", run: cmd.redo },
    SEPARATOR,
    { id: "before-after", label: "Show Original (Before)", checked: showOriginal, run: cmd.toggleBeforeAfter },
    {
      id: "zoom",
      label: "Zoom",
      children: [
        { id: "zoom-fit", label: "Fit", run: cmd.zoomFit },
        { id: "zoom-100", label: "100%", run: cmd.zoomActual },
      ],
    },
    { id: "clipping", label: "Show Clipping", checked: clipping, run: cmd.toggleClipping },
    {
      id: "mask-overlay",
      label: "Show Mask Overlay",
      checked: maskOverlay,
      disabled: !hasMasks,
      reason: hasMasks ? undefined : "This photo has no masks",
      run: cmd.toggleMaskOverlay,
    },
    SEPARATOR,
    { id: "copy-settings", label: "Copy Settings…", run: cmd.copySettings },
    { id: "paste-settings", label: "Paste Settings", disabled: !hasCopiedSettings, reason: hasCopiedSettings ? undefined : "Copy settings from a photo first", run: cmd.pasteSettings },
    { id: "reset-settings", label: "Reset Settings…", danger: true, run: cmd.resetSettings },
  ]);
}

/**
 * @typedef {Object} MaskMenuCommands
 * @property {() => void} toggleInvert
 * @property {() => void} toggleOverlay
 * @property {() => void} close
 * @property {() => void} remove
 */

/**
 * The selected mask's floating panel. `canInvert` is false for spot and red-eye masks (the panel has no Invert
 * for them), `overlayCapable` mirrors when the panel offers Show Overlay.
 * @param {{ title: string, canInvert: boolean, inverted: boolean, overlayCapable: boolean, overlayOn: boolean }} mask
 * @param {MaskMenuCommands} cmd
 * @returns {import('./model.js').MenuEntry[]}
 */
export function buildMaskMenu({ title, canInvert, inverted, overlayCapable, overlayOn }, cmd) {
  return tidy([
    canInvert && { id: "invert-mask", label: "Invert", checked: inverted, run: cmd.toggleInvert },
    overlayCapable && { id: "mask-overlay", label: "Show Overlay", checked: overlayOn, run: cmd.toggleOverlay },
    { id: "close-mask", label: "Close Mask Panel", run: cmd.close },
    SEPARATOR,
    {
      id: "delete-mask",
      label: `Delete ${title}…`,
      danger: true,
      confirm: { title: `Delete ${title}`, message: `Delete this ${title.toLowerCase()}? You can undo it from History.`, confirmLabel: "Delete" },
      run: cmd.remove,
    },
  ]);
}

/**
 * @typedef {Object} ShapeMenuCommands
 * @property {() => void} select
 * @property {() => void} remove
 */

/**
 * One added shape of the selected mask (the base shape cannot be removed, so it has no menu).
 * @param {{ name: string, selected: boolean }} shape
 * @param {ShapeMenuCommands} cmd
 * @returns {import('./model.js').MenuEntry[]}
 */
export function buildShapeMenu({ name, selected }, cmd) {
  return tidy([
    { id: "select-shape", label: "Edit This Shape", disabled: selected, reason: selected ? "Already being edited" : undefined, run: cmd.select },
    SEPARATOR,
    {
      id: "remove-shape",
      label: "Remove Shape…",
      danger: true,
      confirm: { title: "Remove Shape", message: `Remove the ${name.toLowerCase()} shape from this mask? You can undo it from History.`, confirmLabel: "Remove" },
      run: cmd.remove,
    },
  ]);
}
