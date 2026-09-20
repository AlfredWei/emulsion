// Import / merge / backup-prompt state (RFC-0009 §3.3, P3d): the progress and busy flags that
// StatusStrip and the toolbar buttons show while an import, an HDR/panorama merge, or the
// close-time backup prompt is running.
//
// The workflows that drive this state (`runImport`, the import/merge handlers, thumbnail
// polling) stay in the page until P4: they call `refresh()` and write the image list and the
// selection. The backup prompt's own handlers are in lib/actions/backupActions.js.

/** @typedef {{ current: number, total: number } | null} Progress */

export class ImportFlowStore {
  importing = $state(false);
  // Populated from the backend's "import-progress" event (lib.rs's import_folder/import_files)
  // while `importing` is true; null before the first event of a given import arrives (e.g. a
  // large folder still being walked) or once the cataloging phase finishes and the
  // thumbnail-backfill phase (`thumbnailProgress`) takes over.
  catalogProgress = $state(/** @type {Progress} */ (null));
  // Populated from "thumbnail-progress" (lib.rs's backfill_missing_thumbnails) while runImport
  // awaits that call -- the SECOND phase of the same progress bar, shown after the cataloging
  // phase completes. `importing` stays true through both phases so "import done" and "every
  // thumbnail ready" read as the same moment, not "done" followed by an untracked silent wait.
  thumbnailProgress = $state(/** @type {Progress} */ (null));
  // Populated from "face-detection-progress" (lib.rs's detect_faces_for_import_batch) while
  // runImport awaits that call -- the THIRD phase of the same progress bar, same "stay visible
  // until genuinely done" treatment thumbnails already got.
  faceDetectionProgress = $state(/** @type {Progress} */ (null));
  // Which of the three runImport phases the progress bar should currently describe -- switched to
  // "thumbnails" once cataloging resolves, then "faces" once thumbnail backfill resolves.
  phase = $state(/** @type {"cataloging" | "thumbnails" | "faces"} */ ("cataloging"));
  /** @type {string[] | null} */
  supportedExtensions = $state(null);
  isDraggingFiles = $state(false);

  // HDR merge (M5, RFC-0003). Guards the button/re-entrancy the same narrow way applyingPreset/
  // pastingSettingsToSelection do.
  mergingHdr = $state(false);
  // Populated from "hdr-merge-progress" (lib.rs's merge_hdr_bracket, via hdr_merge::merge_bracket's
  // own on_progress callback) while mergingHdr is true -- one step per RAW frame decoded, plus
  // align/merge/tone-map, so the multi-second pipeline shows real movement instead of looking hung.
  hdrMergeProgress = $state(/** @type {Progress} */ (null));
  // Panorama merge (M5, RFC-0004). Same guard shape as HDR merge.
  mergingPanorama = $state(false);

  // Catalog backup (PRD §7.6): the close handler needs to actually wait for the user's dialog
  // interaction before destroying the window -- a promise bridge, since every other dialog in this
  // app is fire-and-forget from its caller's perspective. The resolver always eventually fires:
  // "Skip This Time" is always available even if "Back Up Now" fails, so the promise is
  // guaranteed to settle.
  backupPromptOpen = $state(false);
  backupPromptSettings = $state(/** @type {import('$lib/api/backup.js').BackupSettings | null} */ (null));
  /** @type {(() => void) | null} */
  #resolveBackupPrompt = null;

  showBackupPromptAndWait(/** @type {import('$lib/api/backup.js').BackupSettings} */ settings) {
    return new Promise((resolve) => {
      this.backupPromptSettings = settings;
      this.#resolveBackupPrompt = () => resolve(undefined);
      this.backupPromptOpen = true;
    });
  }

  closeBackupPrompt() {
    this.backupPromptOpen = false;
    this.#resolveBackupPrompt?.();
    this.#resolveBackupPrompt = null;
  }
}

export function createImportFlowStore() {
  return new ImportFlowStore();
}

export const importFlow = createImportFlowStore();
