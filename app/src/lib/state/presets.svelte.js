// Preset list and the Develop-side dialog flags (RFC-0009 §3.3, P6a): the catalog-wide preset list
// plus the confirm/prompt dialogs and in-flight guards for preset apply, snapshot creation, reset
// and copy/paste settings. Holds state only; refreshing the list and applying/creating/deleting
// presets write the edit stack and history too, so they are actions (lib/actions).

export class PresetsStore {
  // Presets (M3): global, catalog-wide, same "fetch once at startup, keep
  // in sync locally" shape as the library store's `collections` -- NOT re-fetched per
  // image the way history/snapshots are, since presets have no relation
  // to whichever photo happens to be open.
  list = $state(/** @type {import('$lib/api/develop.js').PresetEntry[]} */ ([]));
  // Presets (M3): same TextPromptDialog/ConfirmDialog reuse as Collections/
  // Snapshots -- no new dialog components needed.
  creatingPreset = $state(false);
  confirmingDeletePresetId = $state(/** @type {number | null} */ (null));
  // Guards the Library "Apply Preset to Selection" dropdown while a batch
  // apply is in flight -- narrow but real mitigation for the one residual
  // race a design review flagged: double-clicking into Develop on one of
  // the targeted images before its own invoke() in the batch has resolved.
  applyingPreset = $state(false);
  // History/Snapshots (M3): naming a new snapshot uses the same generic
  // TextPromptDialog "New Collection" already uses -- no dedicated dialog
  // needed for one text field.
  creatingSnapshot = $state(false);
  // Develop panel "Reset": reverts every adjustment AND mask on the current
  // photo back to default in one shot, gated behind a confirmation (see
  // confirmingReset/the ConfirmDialog) since it's destructive and
  // can't be undone. Same immediate-flush shape as handleMaskDeleted
  // -- a confirmed destructive action should persist right away, not risk
  // being lost to the usual 250ms slider debounce.
  confirmingReset = $state(false);
  copySettingsDialogOpen = $state(false);
  // Guards "Paste Settings to Selection" while a batch paste is in
  // flight, same narrow race-mitigation purpose as applyingPreset.
  pastingSettingsToSelection = $state(false);
}

export function createPresetsStore() {
  return new PresetsStore();
}

export const presets = createPresetsStore();
