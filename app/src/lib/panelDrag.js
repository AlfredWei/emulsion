// Dragging a floating panel inside its container (the mask panel over the Develop canvas): the position arithmetic and
// the remembered position, kept free of the DOM so they can be tested. A position is the panel's left / top offset in
// its container, in CSS pixels; `null` means "where the stylesheet puts it".

/** @typedef {{ x: number, y: number }} PanelPosition */

/** How much of the panel's height must stay inside the container at the bottom: its header to grab, and enough of the
 * body to still be usable (the panel scrolls). */
export const MIN_VISIBLE_PX = 120;

/**
 * `pos` moved to the nearest place where the panel is inside the container horizontally and where its header is
 * inside vertically (the lower part of the body may run past the bottom: the panel scrolls).
 * @param {PanelPosition} pos
 * @param {{ w: number, h: number }} panel
 * @param {{ w: number, h: number }} container
 * @returns {PanelPosition}
 */
export function clampPanelPosition(pos, panel, container) {
  const maxX = Math.max(0, container.w - panel.w);
  const maxY = Math.max(0, container.h - Math.min(panel.h, MIN_VISIBLE_PX));
  return { x: Math.min(maxX, Math.max(0, Math.round(pos.x))), y: Math.min(maxY, Math.max(0, Math.round(pos.y))) };
}

/** @param {string} key @returns {PanelPosition | null} */
export function loadPanelPosition(key) {
  try {
    const raw = localStorage.getItem(key);
    if (raw === null) return null;
    const v = JSON.parse(raw);
    return Number.isFinite(v?.x) && Number.isFinite(v?.y) ? { x: v.x, y: v.y } : null;
  } catch {
    return null; // no storage, or something else wrote there: the default place
  }
}

/** @param {string} key @param {PanelPosition | null} pos `null` forgets the position. */
export function savePanelPosition(key, pos) {
  try {
    if (pos === null) localStorage.removeItem(key);
    else localStorage.setItem(key, JSON.stringify(pos));
  } catch {
    // not persisted; fine
  }
}
