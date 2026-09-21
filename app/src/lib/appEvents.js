// App-level event wiring (RFC-0009 P7, moved out of +page.svelte's onMount): the startup refreshes,
// the window-close flush (pending edit / IPTC save, then the backup prompt), and the Tauri event
// listeners -- file drag-and-drop, native menu clicks, and the five progress streams (import,
// thumbnails, HDR merge, face detection, on-demand face scan) -- plus the `shortcuts-updated`
// window event. `installAppEvents` is called once from the page's `onMount` and returns the same
// cleanup function the inline body returned.
//
// `handleMenuAction` is the one thing it needs from the component: the native-menu handler is built
// over the page's keyboard/menu context (lib/menuActions.js), so the page passes it in.

import { getCurrentWindow } from "@tauri-apps/api/window";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { listen } from "@tauri-apps/api/event";
import { shell } from "$lib/state/shell.svelte.js";
import { faces } from "$lib/state/faces.svelte.js";
import { importFlow } from "$lib/state/importFlow.svelte.js";
import { library } from "$lib/state/library.svelte.js";
import { develop } from "$lib/state/develop.svelte.js";
import { listAllImageKeywords } from "$lib/api/catalog.js";
import { flushThumbnailBatch } from "$lib/thumbnailBatchQueue.js";
import { getBackupSettings, isBackupDue } from "$lib/api/backup.js";
import { refresh, refreshCollections, handleBatchThumbnailsComplete } from "$lib/actions/libraryActions.js";
import { handleDropImport, regenerateThumbnailFor, pollUntilThumbnailsReadyOnStartup } from "$lib/actions/importActions.js";
import { refreshPresets } from "$lib/actions/presetActions.js";

/**
 * @param {{ handleMenuAction: (action: string) => void }} ctx
 * @returns {() => void} removes every listener it registered
 */
export function installAppEvents({ handleMenuAction }) {
  // Also covers the startup catch-up pass (preview_cache::pregenerate_missing
  // / import::generate_missing_thumbnails, both run once in lib.rs's
  // .setup()): this refresh() races against that pass the same way an
  // import's own refresh() races against its own background trigger.
  refresh().then(pollUntilThumbnailsReadyOnStartup);
  refreshCollections();
  refreshPresets();
  listAllImageKeywords().then((assignments) => (library.allImageKeywords = assignments));

  // M1 Slice 6 (crash-safety): flush a pending debounced edit before the
  // window actually closes, so quitting right after a slider drag can't
  // lose it. Only intervenes when something is actually pending -- the
  // common case (nothing to flush) closes immediately, no added latency.
  // This protects a *graceful* quit only (close-button click, or another
  // OS "please close" request that routes through the same
  // closeRequested pipeline `.close()` itself uses, per Tauri's own
  // docs) -- it cannot help against SIGKILL/a hard crash, which bypasses
  // every in-process handler. Whether macOS Cmd+Q routes through this
  // same path is unverified in this environment.
  //
  // Catalog backup (PRD §7.6): settings are re-fetched fresh here, not
  // snapshotted at startup -- Tauri's own `onCloseRequested` docs show
  // `event.preventDefault()` being called *after* an `await` as the
  // canonical pattern (the wrapper awaits the whole handler before ever
  // checking `isPreventDefault()`), so there's no staleness risk to
  // engineer around by pre-fetching.
  let unlistenClose = /** @type {(() => void) | undefined} */ (undefined);
  getCurrentWindow()
    .onCloseRequested(async (event) => {
      // M3 Slice 1: force-close Settings unconditionally before any
      // backup-prompt logic below runs. SettingsDialog and
      // BackupPromptDialog share the same fixed-inset/z-index overlay
      // shell -- if both were left open at once, whichever is later in
      // DOM order would silently swallow all clicks, and the close-prompt
      // underneath could get stuck uninteractive after event.preventDefault()
      // already fired. This removes the ambiguity outright rather than
      // relying on template order as an implicit invariant.
      shell.settingsOpen = false;

      // M2 Slice 2: an IPTC field saves on blur, so a value typed but not
      // yet blurred (e.g. the user clicks the window's close button while
      // still focused in the Caption textarea) needs to be forced to save
      // before the pending-work check below -- otherwise it's silently
      // lost, the same class of bug fixed for the Develop edit stack.
      /** @type {HTMLElement | null} */ (document.activeElement)?.blur();

      const editPending = develop.hasPendingWork;
      let backupSettings = /** @type {import('$lib/api/backup.js').BackupSettings | null} */ (null);
      try {
        backupSettings = await getBackupSettings();
      } catch {
        // Treat an unreadable settings fetch as "not due" -- a backup
        // check must never block an otherwise-clean quit.
      }
      const backupDue = backupSettings !== null && isBackupDue(backupSettings);

      if (!editPending && !backupDue) return;
      event.preventDefault();

      const wasEditPending = develop.hasPendingEdit;
      await develop.flushPending();
      // Fire-and-forget, deliberately NOT awaited: a thumbnail regen
      // abandoned by a force-quit mid-encode is a stale-until-next-flush
      // grid thumbnail, strictly lower stakes than the lost-edit bug M1
      // Slice 6 actually fixed for the edit-stack flush -- direct
      // precedent already established for generate_missing_thumbnails.
      // Blocking app quit on this would be a real regression. Flush the
      // batch queue to ensure pending regens start immediately, but don't
      // wait for them to complete.
      if (wasEditPending) {
        regenerateThumbnailFor(develop.versionId);
        flushThumbnailBatch(handleBatchThumbnailsComplete);
      }

      // Always resolves -- "Skip This Time" is always available even if
      // "Back Up Now" fails, so this can never trap the user unable to quit.
      if (backupDue && backupSettings !== null) await importFlow.showBackupPromptAndWait(backupSettings);

      await getCurrentWindow().destroy();
    })
    .then((fn) => {
      unlistenClose = fn;
    });

  let unlistenDragDrop = /** @type {(() => void) | undefined} */ (undefined);
  try {
    getCurrentWebview()
      .onDragDropEvent((event) => {
        if (event.payload.type === "enter" || event.payload.type === "over") {
          if (shell.activeModule === "library") importFlow.isDraggingFiles = true;
        } else if (event.payload.type === "leave") {
          importFlow.isDraggingFiles = false;
        } else if (event.payload.type === "drop") {
          importFlow.isDraggingFiles = false;
          if (event.payload.paths && event.payload.paths.length > 0) {
            handleDropImport(event.payload.paths);
          }
        }
      })
      .then((fn) => {
        unlistenDragDrop = fn;
      });
  } catch {
    // ignore outside Tauri
  }

  const onShortcutsUpdated = (/** @type {any} */ event) => {
    if (event.detail) shell.shortcuts = event.detail;
  };
  window.addEventListener("shortcuts-updated", onShortcutsUpdated);

  // Native OS menu bar (M4.5 Slice 4): lib.rs's `on_menu_event` re-emits
  // every click as a plain `"menu-action"` event, same
  // try/catch-outside-Tauri precedent as the drag-drop listener above.
  let unlistenMenu = /** @type {(() => void) | undefined} */ (undefined);
  try {
    listen("menu-action", (/** @type {{ payload: string }} */ event) => {
      handleMenuAction(event.payload);
    }).then((fn) => {
      unlistenMenu = fn;
    });
  } catch {
    // ignore outside Tauri
  }

  // Import progress bar: import_folder/import_files (lib.rs) emit this
  // once per file as they walk a folder or file list -- see import.rs's
  // `import_paths_with_progress`. Same try/catch-outside-Tauri precedent
  // as the listeners above.
  let unlistenImportProgress = /** @type {(() => void) | undefined} */ (undefined);
  try {
    listen("import-progress", (/** @type {{ payload: { current: number, total: number } }} */ event) => {
      importFlow.catalogProgress = event.payload;
    }).then((fn) => {
      unlistenImportProgress = fn;
    });
  } catch {
    // ignore outside Tauri
  }

  // Thumbnail-backfill progress bar, second phase of the same bar
  // (runImport awaits backfillMissingThumbnails right after import
  // itself resolves) -- lib.rs's backfill_missing_thumbnails emits this
  // once per image via generate_missing_thumbnails_with_progress.
  let unlistenThumbnailProgress = /** @type {(() => void) | undefined} */ (undefined);
  try {
    listen("thumbnail-progress", (/** @type {{ payload: { current: number, total: number } }} */ event) => {
      importFlow.thumbnailProgress = event.payload;
    }).then((fn) => {
      unlistenThumbnailProgress = fn;
    });
  } catch {
    // ignore outside Tauri
  }

  // HDR merge progress bar: lib.rs's merge_hdr_bracket emits this once
  // per pipeline step (see hdrMergeProgress's own doc comment above).
  let unlistenHdrMergeProgress = /** @type {(() => void) | undefined} */ (undefined);
  try {
    listen("hdr-merge-progress", (/** @type {{ payload: { current: number, total: number } }} */ event) => {
      importFlow.hdrMergeProgress = event.payload;
    }).then((fn) => {
      unlistenHdrMergeProgress = fn;
    });
  } catch {
    // ignore outside Tauri
  }

  // Face-detection progress bar, third phase of the same import bar
  // (runImport now awaits detectFacesForImportBatch right after
  // thumbnail backfill resolves, same "stay visible until genuinely
  // done" treatment as thumbnails got) -- lib.rs's
  // detect_faces_for_import_batch emits this once per image.
  let unlistenFaceDetectionProgress = /** @type {(() => void) | undefined} */ (undefined);
  try {
    listen("face-detection-progress", (/** @type {{ payload: { current: number, total: number } }} */ event) => {
      importFlow.faceDetectionProgress = event.payload;
    }).then((fn) => {
      unlistenFaceDetectionProgress = fn;
    });
  } catch {
    // ignore outside Tauri
  }

  // On-demand detection's own progress (MetadataPanel's "Face" button,
  // Library's multi-select/folder batch actions) -- a distinct event
  // from import's own "face-detection-progress" above so the two
  // progress displays can never cross-talk (see runFaceDetection's own
  // doc comment).
  let unlistenFaceScanProgress = /** @type {(() => void) | undefined} */ (undefined);
  try {
    listen("face-detection-scan-progress", (/** @type {{ payload: { current: number, total: number } }} */ event) => {
      faces.scanProgress = event.payload;
    }).then((fn) => {
      unlistenFaceScanProgress = fn;
    });
  } catch {
    // ignore outside Tauri
  }

  return () => {
    unlistenClose?.();
    unlistenDragDrop?.();
    unlistenMenu?.();
    unlistenImportProgress?.();
    unlistenThumbnailProgress?.();
    unlistenHdrMergeProgress?.();
    unlistenFaceDetectionProgress?.();
    unlistenFaceScanProgress?.();
    window.removeEventListener("shortcuts-updated", onShortcutsUpdated);
  };
}
