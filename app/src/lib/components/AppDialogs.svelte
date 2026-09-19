<script>
  import SettingsDialog from "$lib/components/SettingsDialog.svelte";
  import ExportDialog from "$lib/components/ExportDialog.svelte";
  import ConfirmDialog from "$lib/components/ConfirmDialog.svelte";
  import TextPromptDialog from "$lib/components/TextPromptDialog.svelte";
  import SmartCollectionDialog from "$lib/components/SmartCollectionDialog.svelte";
  import CopySettingsDialog from "$lib/components/CopySettingsDialog.svelte";
  import BackupPromptDialog from "$lib/components/BackupPromptDialog.svelte";

  /**
   * AppDialogs: every app-level modal (settings, export, confirmations, name prompts, backup prompt)
   * in one place. Stateless: open flags arrive as props, confirm/cancel leave as callbacks
   * (RFC-0009 P1).
   * @type {{
   *   settingsOpen: boolean,
   *   exportItems: { path: string, version_id: number }[] | null,
   *   copySettingsDialogOpen: boolean,
   *   confirmingFaceDetectionOnImport: boolean,
   *   pendingImportBatchSize: number,
   *   confirmingRemoval: boolean,
   *   selectedIds: Set<number>,
   *   confirmingReset: boolean,
   *   creatingCollection: boolean,
   *   creatingCollectionWithImages: boolean,
   *   creatingSmartCollection: boolean,
   *   creatingSnapshot: boolean,
   *   creatingPreset: boolean,
   *   confirmingDeletePresetId: number | string | null,
   *   backupPromptSettings: import('$lib/api/backup.js').BackupSettings | null,
   *   backupPromptOpen: boolean,
   *   onCloseSettings: () => void,
   *   onCloseExport: () => void,
   *   onCancelCopySettings: () => void,
   *   onCancelRemoval: () => void,
   *   onCancelReset: () => void,
   *   onCancelCreateCollection: () => void,
   *   onCancelCreateCollectionWithImages: () => void,
   *   onCancelCreateSmartCollection: () => void,
   *   onCancelCreateSnapshot: () => void,
   *   onCancelCreatePreset: () => void,
   *   onCancelDeletePreset: () => void,
   *   handleCopySettingsConfirmed: (selectedGroupIds: string[]) => void,
   *   handleFaceDetectionPromptConfirm: () => void,
   *   handleFaceDetectionPromptCancel: () => void,
   *   handleRemoveConfirmed: () => void,
   *   handleResetEditStack: () => void,
   *   handleCreateCollection: (name: string) => void,
   *   handleCreateCollectionWithImages: (name: string) => void,
   *   handleCreateSmartCollection: (name: string, rules: import('$lib/api/catalog.js').CollectionRule[]) => void,
   *   handleCreateSnapshotConfirmed: (name: string) => void,
   *   handleCreatePresetConfirmed: (name: string) => void,
   *   handleDeletePresetConfirmed: () => void,
   *   handleBackupDone: (settings: import('$lib/api/backup.js').BackupSettings) => void,
   *   handleBackupSkip: (settings: import('$lib/api/backup.js').BackupSettings) => void,
   * }}
   */
  let {
    settingsOpen,
    exportItems,
    copySettingsDialogOpen,
    confirmingFaceDetectionOnImport,
    pendingImportBatchSize,
    confirmingRemoval,
    selectedIds,
    confirmingReset,
    creatingCollection,
    creatingCollectionWithImages,
    creatingSmartCollection,
    creatingSnapshot,
    creatingPreset,
    confirmingDeletePresetId,
    backupPromptSettings,
    backupPromptOpen,
    onCloseSettings,
    onCloseExport,
    onCancelCopySettings,
    onCancelRemoval,
    onCancelReset,
    onCancelCreateCollection,
    onCancelCreateCollectionWithImages,
    onCancelCreateSmartCollection,
    onCancelCreateSnapshot,
    onCancelCreatePreset,
    onCancelDeletePreset,
    handleCopySettingsConfirmed,
    handleFaceDetectionPromptConfirm,
    handleFaceDetectionPromptCancel,
    handleRemoveConfirmed,
    handleResetEditStack,
    handleCreateCollection,
    handleCreateCollectionWithImages,
    handleCreateSmartCollection,
    handleCreateSnapshotConfirmed,
    handleCreatePresetConfirmed,
    handleDeletePresetConfirmed,
    handleBackupDone,
    handleBackupSkip,
  } = $props();
</script>

<SettingsDialog open={settingsOpen} onClose={onCloseSettings} />

<ExportDialog items={exportItems} onClose={onCloseExport} />

<CopySettingsDialog
  open={copySettingsDialogOpen}
  onConfirm={handleCopySettingsConfirmed}
  onCancel={onCancelCopySettings}
/>

<ConfirmDialog
  open={confirmingFaceDetectionOnImport}
  variant="primary"
  title="Detect faces?"
  message={`Run face detection on the ${pendingImportBatchSize} photo${pendingImportBatchSize === 1 ? "" : "s"} you just imported? The first run downloads a small detection model. You can always detect faces later from Library.`}
  confirmLabel="Detect Faces"
  onConfirm={handleFaceDetectionPromptConfirm}
  onCancel={handleFaceDetectionPromptCancel}
/>

<ConfirmDialog
  open={confirmingRemoval}
  title="Remove from catalog"
  message={`Remove ${selectedIds.size} photo${selectedIds.size === 1 ? "" : "s"} from the catalog? Source files stay on disk; edits, ratings, and metadata stored in the catalog are discarded.`}
  confirmLabel="Remove"
  onConfirm={handleRemoveConfirmed}
  onCancel={onCancelRemoval}
/>

<ConfirmDialog
  open={confirmingReset}
  title="Reset all edits"
  message="Revert every adjustment and mask on this photo back to default? This can't be undone."
  confirmLabel="Reset"
  onConfirm={handleResetEditStack}
  onCancel={onCancelReset}
/>

<TextPromptDialog
  open={creatingCollection}
  title="New Collection"
  label="Name"
  placeholder="e.g. Portfolio"
  confirmLabel="Create"
  onConfirm={handleCreateCollection}
  onCancel={onCancelCreateCollection}
/>

<TextPromptDialog
  open={creatingCollectionWithImages}
  title="New Collection"
  label="Name"
  placeholder="e.g. Portfolio"
  confirmLabel="Create"
  onConfirm={handleCreateCollectionWithImages}
  onCancel={onCancelCreateCollectionWithImages}
/>

<SmartCollectionDialog
  open={creatingSmartCollection}
  title="New Smart Collection"
  confirmLabel="Create"
  onConfirm={handleCreateSmartCollection}
  onCancel={onCancelCreateSmartCollection}
/>

<TextPromptDialog
  open={creatingSnapshot}
  title="New Snapshot"
  label="Name"
  placeholder="e.g. Before crop"
  confirmLabel="Create"
  onConfirm={handleCreateSnapshotConfirmed}
  onCancel={onCancelCreateSnapshot}
/>

<TextPromptDialog
  open={creatingPreset}
  title="New Preset"
  label="Name"
  placeholder="e.g. Moody B&W"
  confirmLabel="Save"
  onConfirm={handleCreatePresetConfirmed}
  onCancel={onCancelCreatePreset}
/>

<ConfirmDialog
  open={confirmingDeletePresetId !== null}
  title="Delete preset"
  message="Delete this preset? This can't be undone."
  confirmLabel="Delete"
  onConfirm={handleDeletePresetConfirmed}
  onCancel={onCancelDeletePreset}
/>

{#if backupPromptSettings}
  <BackupPromptDialog
    open={backupPromptOpen}
    settings={backupPromptSettings}
    onDone={handleBackupDone}
    onSkip={handleBackupSkip}
  />
{/if}

<style>

</style>
