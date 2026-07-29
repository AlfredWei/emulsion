// Thin wrapper around the catalog-backup Tauri commands (PRD §7.6, M2 final
// slice, see app/src-tauri/src/lib.rs) -- keeps raw command-name strings out
// of components, matching develop.js/export.js's precedent of one small
// module per concern rather than piling everything into catalog.js.

import { invoke } from "@tauri-apps/api/core";

/**
 * @typedef {Object} BackupSettings
 * @property {string} frequency "never" | "every_time" | "daily" | "weekly" | "monthly"
 * @property {string | null} folder
 * @property {boolean} check_integrity
 * @property {boolean} optimize
 * @property {string | null} last_backup_at
 */

/**
 * @typedef {Object} BackupOutcome
 * @property {string} path
 * @property {string} performed_at
 */

/** @returns {Promise<BackupSettings>} */
export function getBackupSettings() {
  return invoke("get_backup_settings");
}

/** @returns {Promise<void>} */
export function updateBackupSettings(/** @type {BackupSettings} */ settings) {
  return invoke("update_backup_settings", { settings });
}

/** @returns {Promise<BackupOutcome>} */
export function performCatalogBackup(
  /** @type {string} */ folder,
  /** @type {boolean} */ checkIntegrity,
  /** @type {boolean} */ optimize,
) {
  return invoke("perform_catalog_backup", { folder, checkIntegrity, optimize });
}

/** Pure due-ness check over already-fetched settings -- no IPC, safe to
 * call synchronously from the close handler right after an `await
 * getBackupSettings()`. `never` is never due; a catalog that's never been
 * backed up is always due (this is what makes the very first close prompt
 * the user through initial setup); `every_time` is always due; otherwise
 * compares elapsed days against the chosen interval.
 * @returns {boolean} */
export function isBackupDue(/** @type {BackupSettings} */ settings) {
  if (settings.frequency === "never") return false;
  if (!settings.last_backup_at) return true;
  if (settings.frequency === "every_time") return true;
  const thresholds = { daily: 1, weekly: 7, monthly: 30 };
  const thresholdDays = thresholds[/** @type {"daily" | "weekly" | "monthly"} */ (settings.frequency)];
  if (thresholdDays === undefined) return false;
  const elapsedMs = Date.now() - parseUtcTimestamp(settings.last_backup_at).getTime();
  return elapsedMs / (1000 * 60 * 60 * 24) >= thresholdDays;
}

/** `last_backup_at` arrives in one of two shapes: SQLite's `datetime('now')`
 * ("YYYY-MM-DD HH:MM:SS", UTC, no timezone marker -- the format
 * `perform_catalog_backup` stores) or a full ISO string with a trailing
 * "Z" (what the "Skip This Time" path stores locally, see +page.svelte's
 * `handleBackupSkip`). A bare space-separated string with no timezone
 * marker is parsed as *local* time by `Date`, not UTC, which would silently
 * skew due-ness by the user's UTC offset -- normalize to real ISO 8601
 * first so both shapes parse as the UTC instant they actually represent. */
function parseUtcTimestamp(/** @type {string} */ raw) {
  const hasTimezone = /[Zz]|[+-]\d{2}:?\d{2}$/.test(raw);
  return new Date(hasTimezone ? raw : raw.replace(" ", "T") + "Z");
}
