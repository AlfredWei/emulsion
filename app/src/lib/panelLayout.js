/**
 * Persisted widths for Develop's resizable left (History) and right
 * (adjustments) rails (M4.5) -- pure UI/window-chrome preference, not
 * catalog data, so this follows shortcuts.js's localStorage precedent
 * rather than the SQLite `settings` table BackupSettings uses (that one
 * is per-catalog; panel width should stay the same across catalogs on the
 * same machine).
 */

const STORAGE_KEY = "emulsion_develop_panel_widths_v1";

export const HISTORY_PANEL_MIN_WIDTH = 160;
export const HISTORY_PANEL_MAX_WIDTH = 400;
export const HISTORY_PANEL_DEFAULT_WIDTH = 200;

export const DEVELOP_PANEL_MIN_WIDTH = 200;
export const DEVELOP_PANEL_MAX_WIDTH = 480;
export const DEVELOP_PANEL_DEFAULT_WIDTH = 240;

/** @param {number} n @param {number} min @param {number} max */
export function clamp(n, min, max) {
  return Math.min(max, Math.max(min, n));
}

/**
 * @returns {{ history: number, develop: number }}
 */
export function getStoredPanelWidths() {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw) {
      const parsed = JSON.parse(raw);
      return {
        history: clamp(Number(parsed.history) || HISTORY_PANEL_DEFAULT_WIDTH, HISTORY_PANEL_MIN_WIDTH, HISTORY_PANEL_MAX_WIDTH),
        develop: clamp(Number(parsed.develop) || DEVELOP_PANEL_DEFAULT_WIDTH, DEVELOP_PANEL_MIN_WIDTH, DEVELOP_PANEL_MAX_WIDTH),
      };
    }
  } catch {
    // ignore parsing failure, fall back to defaults
  }
  return { history: HISTORY_PANEL_DEFAULT_WIDTH, develop: DEVELOP_PANEL_DEFAULT_WIDTH };
}

/** @param {{ history: number, develop: number }} widths */
export function saveStoredPanelWidths(widths) {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(widths));
  } catch {
    // ignore
  }
}
