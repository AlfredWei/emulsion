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

const FILMSTRIP_KEY = "emulsion_filmstrip_height_v1";
/** The filmstrip's own height in px (cells plus its 10 px padding top and bottom): the old fixed strip was 72. */
export const FILMSTRIP_MIN_HEIGHT = 72;
export const FILMSTRIP_MAX_HEIGHT = 260;
export const FILMSTRIP_DEFAULT_HEIGHT = 72;
/** Vertical padding inside the strip (top plus bottom); a cell is the strip's height minus this. */
export const FILMSTRIP_PADDING = 20;

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

/** The user's chosen filmstrip height (Library and Develop share one strip height), or the default.
 * @returns {number} */
export function getStoredFilmstripHeight() {
  try {
    const raw = Number(localStorage.getItem(FILMSTRIP_KEY));
    if (raw) return clamp(raw, FILMSTRIP_MIN_HEIGHT, FILMSTRIP_MAX_HEIGHT);
  } catch {
    // ignore, fall back to the default
  }
  return FILMSTRIP_DEFAULT_HEIGHT;
}

/** @param {number} height */
export function saveStoredFilmstripHeight(height) {
  try {
    localStorage.setItem(FILMSTRIP_KEY, String(height));
  } catch {
    // ignore
  }
}
