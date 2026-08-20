/**
 * Shortcuts configuration & persistence module.
 * Provides default shortcuts, category grouping, and localStorage persistence.
 */

export const DEFAULT_SHORTCUTS = {
  // Navigation
  nextImage: "ArrowRight",
  prevImage: "ArrowLeft",
  gridDown: "ArrowDown",
  gridUp: "ArrowUp",

  // View Modes
  viewGrid: "g",
  viewLoupe: "e",
  viewCompare: "c",
  viewSurvey: "n",
  viewDevelop: "d",
  toggleView: " ",

  // Rating & Culling
  rate0: "0",
  rate1: "1",
  rate2: "2",
  rate3: "3",
  rate4: "4",
  rate5: "5",
  flagPick: "p",
  flagReject: "x",
  flagUnflag: "u",
  colorRed: "6",
  colorYellow: "7",
  colorGreen: "8",
  colorBlue: "9",

  // Develop
  toggleOriginal: "\\",
  toggleMaskOverlay: "o",
  toggleMaskChrome: "h",
};

/**
 * @typedef {Object} ShortcutDefinition
 * @property {string} id
 * @property {string} label
 * @property {string} category
 * @property {string} defaultKey
 */

/** @type {ShortcutDefinition[]} */
export const SHORTCUT_DEFINITIONS = [
  // Navigation
  { id: "nextImage", label: "Next Photo", category: "Navigation", defaultKey: "ArrowRight" },
  { id: "prevImage", label: "Previous Photo", category: "Navigation", defaultKey: "ArrowLeft" },
  { id: "gridDown", label: "Step Down (Grid)", category: "Navigation", defaultKey: "ArrowDown" },
  { id: "gridUp", label: "Step Up (Grid)", category: "Navigation", defaultKey: "ArrowUp" },

  // View Modes
  { id: "viewGrid", label: "Grid View", category: "View Modes", defaultKey: "g" },
  { id: "viewLoupe", label: "Loupe / Single View", category: "View Modes", defaultKey: "e" },
  { id: "viewCompare", label: "Compare View", category: "View Modes", defaultKey: "c" },
  { id: "viewSurvey", label: "Survey View", category: "View Modes", defaultKey: "n" },
  { id: "viewDevelop", label: "Develop Module", category: "View Modes", defaultKey: "d" },
  { id: "toggleView", label: "Toggle Grid / Loupe (or Fit Zoom)", category: "View Modes", defaultKey: "Space" },

  // Culling
  { id: "rate0", label: "Clear Rating (0 Star)", category: "Rating & Culling", defaultKey: "0" },
  { id: "rate1", label: "Set 1 Star", category: "Rating & Culling", defaultKey: "1" },
  { id: "rate2", label: "Set 2 Stars", category: "Rating & Culling", defaultKey: "2" },
  { id: "rate3", label: "Set 3 Stars", category: "Rating & Culling", defaultKey: "3" },
  { id: "rate4", label: "Set 4 Stars", category: "Rating & Culling", defaultKey: "4" },
  { id: "rate5", label: "Set 5 Stars", category: "Rating & Culling", defaultKey: "5" },
  { id: "flagPick", label: "Pick Flag (Toggle)", category: "Rating & Culling", defaultKey: "p" },
  { id: "flagReject", label: "Reject Flag (Toggle)", category: "Rating & Culling", defaultKey: "x" },
  { id: "flagUnflag", label: "Unflag Photo", category: "Rating & Culling", defaultKey: "u" },
  { id: "colorRed", label: "Red Label", category: "Rating & Culling", defaultKey: "6" },
  { id: "colorYellow", label: "Yellow Label", category: "Rating & Culling", defaultKey: "7" },
  { id: "colorGreen", label: "Green Label", category: "Rating & Culling", defaultKey: "8" },
  { id: "colorBlue", label: "Blue Label", category: "Rating & Culling", defaultKey: "9" },

  // Develop
  { id: "toggleOriginal", label: "Before / After Toggle", category: "Develop", defaultKey: "\\" },
  { id: "toggleMaskOverlay", label: "Mask Overlay Toggle", category: "Develop", defaultKey: "o" },
  { id: "toggleMaskChrome", label: "Hide / Show Mask Pins", category: "Develop", defaultKey: "h" },
];

const STORAGE_KEY = "emulsion_shortcuts_v1";

/**
 * Load user configured shortcuts from localStorage with fallback to defaults.
 * @returns {Record<string, string>}
 */
export function getStoredShortcuts() {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw) {
      return { ...DEFAULT_SHORTCUTS, ...JSON.parse(raw) };
    }
  } catch {
    // ignore parsing failure
  }
  return { ...DEFAULT_SHORTCUTS };
}

/**
 * Save user configured shortcuts to localStorage.
 * @param {Record<string, string>} shortcuts
 */
export function saveStoredShortcuts(shortcuts) {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(shortcuts));
    window.dispatchEvent(new CustomEvent("shortcuts-updated", { detail: shortcuts }));
  } catch {
    // ignore
  }
}

/**
 * Reset all shortcuts to system defaults.
 * @returns {Record<string, string>}
 */
export function resetStoredShortcuts() {
  try {
    localStorage.removeItem(STORAGE_KEY);
    window.dispatchEvent(new CustomEvent("shortcuts-updated", { detail: DEFAULT_SHORTCUTS }));
  } catch {
    // ignore
  }
  return { ...DEFAULT_SHORTCUTS };
}

/**
 * Format key string nicely for UI display.
 * @param {string} key
 * @returns {string}
 */
export function formatKeyDisplay(key) {
  if (!key) return "—";
  if (key === " ") return "Space";
  if (key === "ArrowRight") return "→";
  if (key === "ArrowLeft") return "←";
  if (key === "ArrowUp") return "↑";
  if (key === "ArrowDown") return "↓";
  if (key === "Escape") return "Esc";
  if (key === "Delete") return "Del";
  if (key === "Backspace") return "⌫";
  if (key === "Enter") return "↵";
  return key.toUpperCase();
}
