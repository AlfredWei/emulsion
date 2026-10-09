// The right-click menu of the empty part of the Library grid (RFC-0028 §3.3).

import { SEPARATOR, tidy } from "./model.js";

/**
 * @typedef {Object} GridMenuCommands
 * @property {() => void} selectAll
 * @property {() => void} deselectAll
 * @property {() => void} importFolder
 * @property {() => void} importFiles
 */

/**
 * @param {{ photoCount: number, selectedCount: number }} input
 * @param {GridMenuCommands} cmd
 * @returns {import('./model.js').MenuEntry[]}
 */
export function buildGridBackgroundMenu({ photoCount, selectedCount }, cmd) {
  return tidy([
    { id: "select-all", label: "Select All", disabled: photoCount === 0, reason: photoCount === 0 ? "No photos here" : undefined, run: cmd.selectAll },
    { id: "deselect-all", label: "Deselect All", disabled: selectedCount === 0, reason: selectedCount === 0 ? "Nothing is selected" : undefined, run: cmd.deselectAll },
    SEPARATOR,
    { id: "import-folder", label: "Import Folder…", run: cmd.importFolder },
    { id: "import-files", label: "Import Files…", run: cmd.importFiles },
  ]);
}
