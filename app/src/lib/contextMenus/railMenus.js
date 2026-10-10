// Right-click menus of the rail rows (RFC-0028 §3.3, slice 2a): a collection, a person, and -- in Develop's
// left rail -- a snapshot, a preset and the Presets list itself. Pure functions of the row and injected
// commands (the real ones are in lib/actions/contextMenuActions.js, each an existing action), like the photo menu.

import { SEPARATOR, tidy } from "./model.js";

/**
 * @typedef {Object} CollectionMenuCommands
 * @property {() => void} open
 * @property {() => void} selectPhotos
 * @property {() => void} remove
 */

/**
 * @param {{ name: string, isSmart: boolean, count: number }} collection
 * @param {CollectionMenuCommands} cmd
 * @returns {import('./model.js').MenuEntry[]}
 */
export function buildCollectionMenu({ name, isSmart, count }, cmd) {
  return tidy([
    { id: "open", label: "Show Photos", run: cmd.open },
    {
      id: "select-photos",
      label: "Select All Photos in It",
      disabled: count === 0,
      reason: count === 0 ? "This collection has no photos" : undefined,
      run: cmd.selectPhotos,
    },
    SEPARATOR,
    {
      id: "delete-collection",
      label: isSmart ? "Delete Smart Collection…" : "Delete Collection…",
      danger: true,
      confirm: {
        title: isSmart ? "Delete Smart Collection" : "Delete Collection",
        message: isSmart
          ? `Delete the smart collection “${name}”? Its photos are not affected.`
          : `Delete the collection “${name}”? The photos stay in the catalog; only the grouping is removed.`,
        confirmLabel: "Delete",
      },
      run: cmd.remove,
    },
  ]);
}

/**
 * @typedef {Object} PersonMenuCommands
 * @property {() => void} showPhotos
 * @property {() => void} rename
 */

/**
 * @param {{ name: string | null, photoCount: number }} person
 * @param {PersonMenuCommands} cmd
 * @returns {import('./model.js').MenuEntry[]}
 */
export function buildPersonMenu({ name, photoCount }, cmd) {
  return tidy([
    { id: "show-photos", label: "Show Photos", disabled: photoCount === 0, reason: photoCount === 0 ? "No photos" : undefined, run: cmd.showPhotos },
    { id: "rename-person", label: name ? "Rename…" : "Name This Person…", run: cmd.rename },
  ]);
}

/**
 * @typedef {Object} SnapshotMenuCommands
 * @property {() => void} restore
 * @property {() => void} remove
 */

/**
 * @param {{ name: string }} snapshot
 * @param {SnapshotMenuCommands} cmd
 * @returns {import('./model.js').MenuEntry[]}
 */
export function buildSnapshotMenu({ name }, cmd) {
  return tidy([
    { id: "restore-snapshot", label: "Restore This Snapshot", run: cmd.restore },
    SEPARATOR,
    {
      id: "delete-snapshot",
      label: "Delete Snapshot…",
      danger: true,
      confirm: { title: "Delete Snapshot", message: `Delete the snapshot “${name}”? This cannot be undone.`, confirmLabel: "Delete" },
      run: cmd.remove,
    },
  ]);
}

/**
 * @typedef {Object} PresetMenuCommands
 * @property {() => void} apply
 * @property {() => void} exportPreset
 * @property {() => void} remove  opens the existing delete confirmation, so no second one here
 */

/**
 * @param {{ name: string }} _preset
 * @param {PresetMenuCommands} cmd
 * @returns {import('./model.js').MenuEntry[]}
 */
export function buildPresetMenu(_preset, cmd) {
  return tidy([
    { id: "apply-preset", label: "Apply to This Photo", run: cmd.apply },
    { id: "export-preset", label: "Export…", run: cmd.exportPreset },
    SEPARATOR,
    { id: "delete-preset", label: "Delete Preset…", danger: true, run: cmd.remove },
  ]);
}

/**
 * @typedef {Object} PresetListMenuCommands
 * @property {() => void} saveCurrent
 * @property {() => void} importPresets
 */

/** @param {PresetListMenuCommands} cmd @returns {import('./model.js').MenuEntry[]} */
export function buildPresetListMenu(cmd) {
  return tidy([
    { id: "save-preset", label: "Save Current as Preset…", run: cmd.saveCurrent },
    { id: "import-preset", label: "Import…", run: cmd.importPresets },
  ]);
}
