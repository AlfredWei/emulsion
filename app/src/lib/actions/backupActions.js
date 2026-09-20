// Close-time catalog backup prompt handlers (RFC-0009 P3d, moved out of +page.svelte's script).
// The prompt's open/settings state and its promise bridge live in the `importFlow` store; these two
// save the user's choice and close the prompt, which lets the window-close handler continue.

import { updateBackupSettings } from "$lib/api/backup.js";
import { importFlow } from "$lib/state/importFlow.svelte.js";

/** @param {import('$lib/api/backup.js').BackupSettings} settings */
export function handleBackupDone(settings) {
  updateBackupSettings(settings).catch(() => {});
  importFlow.closeBackupPrompt();
}

/** @param {import('$lib/api/backup.js').BackupSettings} settings */
export function handleBackupSkip(settings) {
  // Resets the due-clock so skipping doesn't re-prompt on literally the
  // next close -- a deliberate simplification of Lightroom's own more
  // precise "postpone until the next real interval" semantics.
  updateBackupSettings({ ...settings, last_backup_at: new Date().toISOString() }).catch(() => {});
  importFlow.closeBackupPrompt();
}
