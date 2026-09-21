// Import, merge and thumbnail workflows (RFC-0009 P4c, moved out of +page.svelte's script): the
// shared import runner (catalog scan -> thumbnails -> optional face detection, with its progress and
// prompts held in the `importFlow` and `faces` stores), the folder/files/drop entry points, HDR and
// panorama merge, per-photo thumbnail regeneration, and the app-startup thumbnail catch-up poll.

import { importFlow } from "$lib/state/importFlow.svelte.js";
import { shell } from "$lib/state/shell.svelte.js";
import { refresh, prioritizeThumbnail, handleBatchThumbnailsComplete } from "$lib/actions/libraryActions.js";
import {
  backfillMissingThumbnails,
  importFolder,
  getSupportedExtensions,
  importFiles,
  mergeHdrBracket,
  mergePanorama,
} from "$lib/api/catalog.js";
import { faces } from "$lib/state/faces.svelte.js";
import { detectFacesForImportBatch } from "$lib/api/faces.js";
import { refreshPeople } from "$lib/actions/faceActions.js";
import { open } from "@tauri-apps/plugin-dialog";
import { selection } from "$lib/state/selection.svelte.js";
import { library } from "$lib/state/library.svelte.js";
import { queueThumbnailRegeneration } from "$lib/thumbnailBatchQueue.js";

// App-startup catch-up ONLY (see the onMount call in +page.svelte) -- lib.rs's
// .setup() runs its own fire-and-forget, non-progress-reporting
// generate_missing_thumbnails pass once at launch, independent of
// anything this component drives. Calling backfillMissingThumbnails()
// here too would start a SECOND, fully redundant full-catalog scan
// racing the first (both would see the same "missing" candidates before
// either finishes writing), so this stays a plain bounded poll that
// just waits for .setup()'s own pass to catch up and re-refresh()es --
// no progress bar, since this isn't a user-initiated action. Bounded
// (not an open-ended interval) so a permanently-stuck thumbnail (a real
// decode failure) doesn't poll forever -- it just stops trying and
// leaves the placeholder, which is the correct outcome in that case.
let pollingThumbnailsOnStartup = false;

export async function runImport(/** @type {() => Promise<import('$lib/api/catalog.js').ImportSummary | null>} */ doImport) {
  importFlow.importing = true;
  importFlow.catalogProgress = null;
  importFlow.thumbnailProgress = null;
  importFlow.faceDetectionProgress = null;
  importFlow.phase = "cataloging";
  shell.notify("");
  try {
    const summary = await doImport();
    if (!summary) return; // user cancelled the dialog
    shell.notify(`Imported ${summary.imported}, ${summary.skipped_duplicates} already in library, ${summary.failed} failed`);
    await refresh();
    // Cataloging (importProgress, tracked above) is only half of "import
    // done" from the user's perspective -- a freshly-imported JPEG has
    // no thumbnail yet at this point (see import.rs's own comment on
    // why). Awaiting this (rather than the old fire-and-forget +
    // untracked polling) means the progress bar stays up, with real
    // per-image feedback, until the Library grid genuinely has nothing
    // left to backfill (scoped to THIS import's own batch, not the
    // whole catalog -- see backfillMissingThumbnails's own doc comment).
    // importPhase switches the progress bar's label over to the
    // thumbnail count -- importProgress itself is left as whatever it
    // last was (100%), not cleared, so there's no momentary "0 / 0"
    // flash between the two phases.
    importFlow.phase = "thumbnails";
    await backfillMissingThumbnails(summary.import_batch);
    await refresh();
    // Face detection (M5 Slice 6 follow-up; opt-in per the 2026-09-17
    // Library-integration redesign -- see RFC-0005 §7): used to run
    // silently and unconditionally, which surprised users who didn't
    // want the one-time model download or the extra wait. Now a THIRD
    // visible phase of the same progress bar, but only if the user says
    // yes to `faces.promptDetectionOnImport`. Its own failure (e.g. no
    // network for the one-time model download) is caught separately and
    // does NOT fail the whole import -- cataloging + thumbnails already
    // succeeded, and a photo whose detection pass fails this way can
    // still be scanned later via the "Face"/"Detect Faces" actions.
    if (summary.imported > 0 && (await faces.promptDetectionOnImport(summary.imported))) {
      importFlow.phase = "faces";
      try {
        await detectFacesForImportBatch(summary.import_batch);
        await refreshPeople();
      } catch (/** @type {any} */ e) {
        shell.notify(`${shell.statusMessage} (face detection failed: ${e})`);
      }
    }
  } catch (/** @type {any} */ e) {
    shell.notify(`Import failed: ${e}`);
  } finally {
    importFlow.importing = false;
    importFlow.catalogProgress = null;
    importFlow.thumbnailProgress = null;
    importFlow.faceDetectionProgress = null;
  }
}

export function handleImportFolder() {
  return runImport(async () => {
    const dir = await open({ directory: true, multiple: false });
    return dir ? importFolder(/** @type {string} */ (dir)) : null;
  });
}

export async function handleImportFiles() {
  // M2 Slice 1: a separate entry point from folder import -- Tauri's
  // dialog plugin has independent `directory`/`multiple` flags, no mode
  // that lets one native dialog pick either files or a folder.
  if (!importFlow.supportedExtensions) importFlow.supportedExtensions = await getSupportedExtensions();
  return runImport(async () => {
    const paths = await open({
      multiple: true,
      filters: [{ name: "Photos", extensions: /** @type {string[]} */ (importFlow.supportedExtensions) }],
    });
    return paths ? importFiles(/** @type {string[]} */ (paths)) : null;
  });
}

export function handleDropImport(/** @type {string[]} */ paths) {
  if (!paths || paths.length === 0) return;
  return runImport(async () => {
    return importFiles(paths);
  });
}

/** Merges the current Library selection (2+ RAW photos, in whatever
 * order `selectedImages` iterates -- see mergeHdrBracket's own doc
 * comment for why `hdr_merge`'s alignment reference is chosen by EV,
 * not by this order, so exact click order doesn't matter here) into
 * one new image, added to the catalog as its own row. Same
 * await-then-refresh-then-poll shape `runImport` already established
 * for a freshly-imported image, since the merge result is cataloged
 * exactly like a JPEG import (thumbnail filled in later by the same
 * background pass). Deduped by image_id first, same reasoning as
 * `handleRemoveConfirmed`'s own dedupe: a virtual copy's version_id is
 * a distinct selection entry but not a distinct source photo. */
export async function handleMergeHdrBracket() {
  const imageIds = [...new Set(selection.selectedImages.map((img) => img.image_id))];
  if (imageIds.length < 2) return;
  importFlow.mergingHdr = true;
  importFlow.hdrMergeProgress = null;
  shell.notify("");
  try {
    const resultImageId = await mergeHdrBracket(imageIds);
    await refresh();
    const merged = library.images.find((img) => img.image_id === resultImageId);
    if (merged) {
      selection.selectedId = merged.version_id;
      selection.selectedIds = new Set([merged.version_id]);
      // A merge result isn't tagged with an import_batch (it's not from
      // import_paths_with_progress), so backfillMissingThumbnails'
      // batch-scoping doesn't apply here -- prioritizeThumbnail's
      // single-image ensure_thumbnail path is the right tool for
      // exactly one new image anyway, same as opening Loupe/Develop.
      // Fire-and-forget, same as the old blind-poll helper this
      // replaces -- doesn't block this function's own status/selection
      // update on the merge result's thumbnail.
      prioritizeThumbnail(merged.version_id);
    }
    shell.notify(`Merged ${imageIds.length} photos into one HDR image`);
  } catch (/** @type {any} */ e) {
    shell.notify(`HDR merge failed: ${e}`);
  } finally {
    importFlow.mergingHdr = false;
    importFlow.hdrMergeProgress = null;
  }
}

// Panorama merge (M5, RFC-0004). Same guard/status/refresh shape as
// handleMergeHdrBracket above -- the only real difference is no
// RAW-only client pre-check (any format works for a stitch) and the
// dedupe-by-image_id reasoning still applies unchanged.
/** Stitches the current Library selection (2+ photos, in whatever
 * order the user selected them -- unlike HDR merge, that order DOES
 * matter here: adjacent selections are assumed to overlap, see
 * mergePanorama's own doc comment) into one new wide composite,
 * cataloged exactly like an HDR merge result. */
export async function handleMergePanorama() {
  const imageIds = [...new Set(selection.selectedImages.map((img) => img.image_id))];
  if (imageIds.length < 2) return;
  importFlow.mergingPanorama = true;
  shell.notify("");
  try {
    const resultImageId = await mergePanorama(imageIds);
    await refresh();
    const merged = library.images.find((img) => img.image_id === resultImageId);
    if (merged) {
      selection.selectedId = merged.version_id;
      selection.selectedIds = new Set([merged.version_id]);
      prioritizeThumbnail(merged.version_id);
    }
    shell.notify(`Stitched ${imageIds.length} photos into one panorama`);
  } catch (/** @type {any} */ e) {
    shell.notify(`Panorama merge failed: ${e}`);
  } finally {
    importFlow.mergingPanorama = false;
  }
}

// 250ms debounce inside flushEditStack + 150ms debounce in batch queue (two
// layers) prevents rapid edits/slider drags from hammering the backend:
// first layer settles pending edits, second layer coalesces multiple regen
// requests into a single atomic update. The batch system ensures all
// components see thumbnails refresh together.
export function regenerateThumbnailFor(/** @type {number | null} */ versionId) {
  if (versionId === null) return;
  queueThumbnailRegeneration(versionId, handleBatchThumbnailsComplete);
}

export async function pollUntilThumbnailsReadyOnStartup() {
  if (pollingThumbnailsOnStartup) return;
  pollingThumbnailsOnStartup = true;
  try {
    const maxAttempts = 10;
    const intervalMs = 1500;
    for (let attempt = 0; attempt < maxAttempts; attempt++) {
      if (!library.images.some((img) => img.thumbnail_path === null)) return;
      await new Promise((resolve) => setTimeout(resolve, intervalMs));
      await refresh();
    }
  } finally {
    pollingThumbnailsOnStartup = false;
  }
}
