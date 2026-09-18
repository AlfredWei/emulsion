/**
 * Persisted collapsed/expanded state for the Library rail's Folders,
 * Collections, and People sections -- pure UI/window-chrome preference,
 * not catalog data, so this follows panelLayout.js's localStorage
 * precedent rather than the SQLite `settings` table BackupSettings uses.
 */

const STORAGE_KEY = "emulsion_catalog_rail_sections_v1";

/** @typedef {"folders" | "collections" | "people"} RailSectionId */

/** @returns {Record<RailSectionId, boolean>} true = expanded */
export function getStoredRailSections() {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw) {
      const parsed = JSON.parse(raw);
      return {
        folders: parsed.folders !== false,
        collections: parsed.collections !== false,
        people: parsed.people !== false,
      };
    }
  } catch {
    // ignore parsing failure, fall back to defaults
  }
  return { folders: true, collections: true, people: true };
}

/** @param {Record<RailSectionId, boolean>} sections */
export function saveStoredRailSections(sections) {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(sections));
  } catch {
    // ignore
  }
}
