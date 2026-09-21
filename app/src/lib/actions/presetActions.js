// Preset and copy/paste-settings workflows (RFC-0009 P6b, moved out of +page.svelte's script):
// the preset list (refresh, create, import/export, delete), applying a preset or the copied
// settings to the open image or the Library selection, the Develop dialog confirms that go with
// them, and the preset hover preview. They combine `presets`, `develop`, `selection` and `library`.

import { presets } from "$lib/state/presets.svelte.js";
import { listPresets, createPreset, presetEligibleOps, applyPresetOps, exportPresetFile, importPresetFile, deletePreset, getEditStack, setEditStack, regenerateThumbnail, copySettingsOps, previewEditStack } from "$lib/api/develop.js";
import { handleCreateSnapshot } from "$lib/actions/developActions.js";
import { develop } from "$lib/state/develop.svelte.js";
import { regenerateThumbnailFor } from "$lib/actions/importActions.js";
import { save, open } from "@tauri-apps/plugin-dialog";
import { shell } from "$lib/state/shell.svelte.js";
import { selection } from "$lib/state/selection.svelte.js";
import { patchLocal } from "$lib/actions/libraryActions.js";

export async function refreshPresets() {
  presets.list = await listPresets();
}

export function handleCreateSnapshotConfirmed(/** @type {string} */ name) {
  presets.creatingSnapshot = false;
  handleCreateSnapshot(name);
}

export function handleSaveCurrentAsPresetRequest() {
  presets.creatingPreset = true;
}

export async function handleCreatePresetConfirmed(/** @type {string} */ name) {
  presets.creatingPreset = false;
  const preset = await createPreset(name, presetEligibleOps(develop.editStack));
  presets.list = [...presets.list, preset];
}

/** Applying a preset to the currently open Develop image is an
 * immediate, discrete action (like Reset/mask-delete), not a debounced
 * slider drag -- flushes right away under its own label. */
export async function handleApplyPreset(/** @type {number} */ presetId) {
  develop.clearPreview();
  if (develop.versionId === null) return;
  const preset = presets.list.find((p) => p.id === presetId);
  if (!preset) return;
  const versionId = develop.versionId;
  develop.editStack = applyPresetOps(develop.editStack, preset.edit_stack);
  // Same immediate-regen pattern restoreTo/handleRestoreSnapshot already
  // follow -- this is a jump-to-a-different-look commit, not a slider
  // drag, so the Library grid thumbnail shouldn't have to wait for the
  // "leaving Develop" checkpoint (switchModule/openDevelop) to catch up.
  // Awaited first, same unawaited-dependent-IPC-calls hazard openDevelop's
  // own flush/regen pair guards against (regenerateThumbnailFor re-reads
  // the edit stack fresh from the catalog, so it must not race the write
  // it's meant to reflect).
  await develop.flushEditStack(`Apply Preset: ${preset.name}`);
  regenerateThumbnailFor(versionId);
}

export async function handleExportPreset(/** @type {number} */ presetId) {
  const preset = presets.list.find((p) => p.id === presetId);
  if (!preset) return;
  const path = await save({
    defaultPath: `${preset.name}.json`,
    filters: [{ name: "Preset", extensions: ["json"] }],
  });
  if (!path) return; // user cancelled
  try {
    await exportPresetFile(preset.name, preset.edit_stack, path);
    shell.notify(`Exported "${preset.name}"`);
  } catch (/** @type {any} */ e) {
    shell.notify(`Export preset failed: ${e}`);
  }
}

export async function handleImportPresetRequest() {
  const path = await open({ multiple: false, filters: [{ name: "Preset", extensions: ["json"] }] });
  if (!path || Array.isArray(path)) return;
  try {
    const raw = await importPresetFile(path);
    // Defensive re-filter -- a hand-edited or foreign file could
    // contain a crop/mask op that would otherwise sail straight
    // through undetected (see importPresetFile's own doc comment).
    const filtered = presetEligibleOps({ schema_version: raw.schema_version, ops: raw.ops });
    const preset = await createPreset(raw.name, filtered);
    presets.list = [...presets.list, preset];
    shell.notify(`Imported "${raw.name}"`);
  } catch (/** @type {any} */ e) {
    shell.notify(`Import preset failed: ${e}`);
  }
}

export function handleDeletePresetRequest(/** @type {number} */ presetId) {
  presets.confirmingDeletePresetId = presetId;
}

export async function handleDeletePresetConfirmed() {
  if (presets.confirmingDeletePresetId === null) return;
  const presetId = presets.confirmingDeletePresetId;
  presets.confirmingDeletePresetId = null;
  await deletePreset(presetId);
  presets.list = presets.list.filter((p) => p.id !== presetId);
}

/** Library batch-apply -- version_id-targeted (NOT image_id: virtual
 * copies are separate versions with independent edit stacks, so
 * image_id would silently under-apply whenever one is selected
 * alongside its original). Each target is an independent getEditStack
 * -> merge -> setEditStack -> regenerateThumbnail round trip, same
 * non-atomic-across-the-batch shape rating/flag/color-label changes
 * already use -- a partial failure here is no worse than a partial
 * failure there. If the image currently open in Develop is among the
 * targets, its in-memory editStack is explicitly re-synced afterward
 * (see the comment below) so a later flush can't silently clobber the
 * just-applied preset with the stale pre-apply stack. */
export async function handleApplyPresetToSelection(/** @type {string} */ value) {
  if (!value) return;
  const preset = presets.list.find((p) => p.id === Number(value));
  if (!preset) return;
  const targets = [...selection.selectedIds];
  if (targets.length === 0) return;
  presets.applyingPreset = true;
  try {
    await Promise.all(
      targets.map(async (versionId) => {
        const current = await getEditStack(versionId);
        const merged = applyPresetOps(current, preset.edit_stack);
        await setEditStack(versionId, merged, `Apply Preset: ${preset.name}`);
        const path = await regenerateThumbnail(versionId);
        if (path) patchLocal(versionId, { thumbnail_path: path });
      }),
    );
    // Re-sync: developVersionId's in-memory editStack was NOT touched
    // by the loop above (it writes straight to the catalog), so if the
    // image currently open in Develop was also a batch target, refetch
    // it now -- otherwise a later flush (window close, switching
    // images) would still hold the stale pre-apply stack and silently
    // overwrite what this batch just wrote.
    if (develop.versionId !== null && targets.includes(develop.versionId)) {
      develop.editStack = await getEditStack(develop.versionId);
    }
    shell.notify(`Applied "${preset.name}" to ${targets.length} photo${targets.length === 1 ? "" : "s"}`);
  } catch (/** @type {any} */ e) {
    shell.notify(`Apply preset failed: ${e}`);
  } finally {
    presets.applyingPreset = false;
  }
}

export function handleCopySettingsRequest() {
  if (develop.versionId === null) return;
  presets.copySettingsDialogOpen = true;
}

export function handleCopySettingsConfirmed(/** @type {string[]} */ groupIds) {
  presets.copySettingsDialogOpen = false;
  develop.copiedSettings = copySettingsOps(develop.editStack, groupIds);
  shell.notify("Copied settings");
}

/** The Copy/Paste Settings buttons live at the bottom of DevelopPanel
 * (Develop-only), so paste there only ever targets the image currently
 * open in Develop -- an immediate, discrete action (like Apply
 * Preset), flushed right away rather than going through the slider
 * debounce. */
export async function handlePasteSettings() {
  if (!develop.copiedSettings || develop.versionId === null) return;
  const versionId = develop.versionId;
  develop.editStack = applyPresetOps(develop.editStack, develop.copiedSettings);
  // Same immediate-regen reasoning (and awaited-first ordering) as
  // handleApplyPreset's own comment.
  await develop.flushEditStack("Paste Settings");
  regenerateThumbnailFor(versionId);
  shell.notify("Pasted settings");
}

/** M4.5 batch apply: applies the SAME in-memory clipboard Copy
 * Settings filled (not a Preset) across every Library-selected image
 * in one action. Mirrors handleApplyPresetToSelection's exact shape --
 * frontend-orchestrated, non-atomic-across-the-batch getEditStack ->
 * applyPresetOps merge -> setEditStack -> regenerateThumbnail per
 * target, with the same re-sync-if-the-open-Develop-image-was-a-target
 * guard -- rather than inventing a second batch pattern. */
export async function handlePasteSettingsToSelection() {
  if (!develop.copiedSettings) return;
  const targets = [...selection.selectedIds];
  if (targets.length === 0) return;
  const settingsToApply = develop.copiedSettings;
  presets.pastingSettingsToSelection = true;
  try {
    await Promise.all(
      targets.map(async (versionId) => {
        const current = await getEditStack(versionId);
        const merged = applyPresetOps(current, settingsToApply);
        await setEditStack(versionId, merged, "Paste Settings");
        const path = await regenerateThumbnail(versionId);
        if (path) patchLocal(versionId, { thumbnail_path: path });
      }),
    );
    if (develop.versionId !== null && targets.includes(develop.versionId)) {
      develop.editStack = await getEditStack(develop.versionId);
    }
    shell.notify(`Pasted settings to ${targets.length} photo${targets.length === 1 ? "" : "s"}`);
  } catch (/** @type {any} */ e) {
    shell.notify(`Paste settings failed: ${e}`);
  } finally {
    presets.pastingSettingsToSelection = false;
  }
}

export function handlePeekPreset(/** @type {number} */ presetId) {
  if (develop.imagePath === null) return;
  const preset = presets.list.find((p) => p.id === presetId);
  if (!preset) return;
  const mergedStack = applyPresetOps(develop.editStack, preset.edit_stack);
  const path = develop.imagePath;
  const contentHash = develop.imageContentHash;
  develop.schedulePreview(() => previewEditStack(path, contentHash, mergedStack));
}
