// Global keyboard shortcuts (RFC-0009 P2, moved out of +page.svelte's script).
//
// `createKeyboardHandlers(ctx)` returns the window keydown/keyup handlers. `ctx` is the
// page's live view of the state and callbacks they touch (see KeyboardContext); nothing here
// imports a store or a component, so the handlers are unit-testable with a plain fake object
// (keyboard.test.js).

import { OVERLAY_CAPABLE_MASK_OPS } from "$lib/api/develop.js";

/**
 * @typedef {Object} KeyboardContext
 * @property {string} activeModule
 * @property {{ path: string, version_id: number }[] | null} exportItems
 * @property {boolean} settingsOpen
 * @property {boolean} backupPromptOpen
 * @property {boolean} creatingSnapshot
 * @property {boolean} creatingPreset
 * @property {number | string | null} confirmingDeletePresetId
 * @property {boolean} confirmingRemoval
 * @property {boolean} creatingCollection
 * @property {boolean} creatingSmartCollection
 * @property {boolean} creatingCollectionWithImages
 * @property {Record<string, string>} shortcuts
 * @property {"grid" | "loupe" | "compare" | "survey" | "map"} libraryViewMode
 * @property {boolean} spacePanning
 * @property {{ op?: string } | null | undefined} selectedMask
 * @property {boolean} showMaskOverlay
 * @property {boolean} maskOverlaysVisible
 * @property {boolean} showOriginal
 * @property {Set<number>} selectedIds
 * @property {number | null} selectedId
 * @property {{ version_id: number }[]} filteredImages
 * @property {{ flag?: string, color_label?: string } | null | undefined} selectedImage
 * @property {() => void} handleUndo
 * @property {() => void} handleRedo
 * @property {(extend?: boolean) => void} selectNextImage
 * @property {(extend?: boolean) => void} selectPrevImage
 * @property {(step: number, extend?: boolean) => void} selectGridStep
 * @property {() => void} showMapView
 * @property {(target: string) => Promise<void>} switchModule
 * @property {(versionId: number) => Promise<void>} openDevelop
 * @property {() => void} handleSelectAll
 * @property {() => void} handleDeselectAll
 * @property {() => void} handleCompareNextCandidate
 * @property {() => void} handleComparePrevCandidate
 * @property {(versionId: number | null | undefined, rating: number) => Promise<void>} handleRatingChange
 * @property {(versionId: number | null | undefined, flag: string) => Promise<void>} handleFlagChange
 * @property {(versionId: number | null | undefined, colorLabel: string) => Promise<void>} handleColorLabelChange
 */

/** @param {KeyboardContext} ctx */
export function createKeyboardHandlers(ctx) {
  // M3 Slice 2: standard Lightroom Classic Library shortcuts -- 0-5 directly
  // SET star rating (0 clears, not a toggle), P/X toggle Pick/Reject on and
  // off, U always hard-clears to unflagged (a third, distinct key, not a
  // toggle of P or X), 6/7/8/9 toggle Red/Yellow/Green/Blue on and off.
  // Purple has no default key in real Lightroom, so none is bound here
  // either. Reuses handleRatingChange/handleFlagChange/handleColorLabelChange
  // with `selectedId` (the anchor) directly -- selectedId is always a
  // member of selectedIds whenever there's a real multi-selection, so their
  // existing targetVersionIds() batching applies "for free," no new
  // target-computation needed.
  const COLOR_KEYS = { 6: "red", 7: "yellow", 8: "green", 9: "blue" };

  // Comprehensive keyboard shortcut handler with custom user bindings
  function handleGlobalKeydown(/** @type {KeyboardEvent} */ e) {
    const target = e.target;
    const isTypingTarget =
      (target instanceof HTMLInputElement && target.type !== "range") ||
      target instanceof HTMLTextAreaElement ||
      target instanceof HTMLSelectElement;
    if (isTypingTarget) {
      return;
    }

    const key = e.key.toLowerCase();
    const rawKey = e.key;

    if (ctx.activeModule === "develop") {
      if (
        ctx.exportItems !== null ||
        ctx.settingsOpen ||
        ctx.backupPromptOpen ||
        ctx.creatingSnapshot ||
        ctx.creatingPreset ||
        ctx.confirmingDeletePresetId !== null
      ) {
        return;
      }
      // Undo/Redo (M3)
      if ((e.metaKey || e.ctrlKey) && !e.altKey && key === "z") {
        e.preventDefault();
        if (e.shiftKey) ctx.handleRedo();
        else ctx.handleUndo();
        return;
      }
      if (e.ctrlKey && !e.metaKey && !e.altKey && !e.shiftKey && key === "y") {
        e.preventDefault();
        ctx.handleRedo();
        return;
      }

      // Arrow navigation in Develop: navigate to previous / next photo
      if (!e.metaKey && !e.ctrlKey && !e.altKey) {
        if (rawKey === ctx.shortcuts.nextImage || rawKey === ctx.shortcuts.gridDown || rawKey === "ArrowRight") {
          e.preventDefault();
          ctx.selectNextImage(false);
          return;
        }
        if (rawKey === ctx.shortcuts.prevImage || rawKey === ctx.shortcuts.gridUp || rawKey === "ArrowLeft") {
          e.preventDefault();
          ctx.selectPrevImage(false);
          return;
        }
        if (key === ctx.shortcuts.viewGrid?.toLowerCase()) {
          e.preventDefault();
          ctx.switchModule("library");
          ctx.libraryViewMode = "grid";
          return;
        }
        if (key === ctx.shortcuts.viewLoupe?.toLowerCase()) {
          e.preventDefault();
          ctx.switchModule("library");
          ctx.libraryViewMode = "loupe";
          return;
        }
      }

      if (e.metaKey || e.ctrlKey || e.altKey) return;
      if (rawKey === " " || key === ctx.shortcuts.toggleView?.toLowerCase()) {
        e.preventDefault();
        if (!e.repeat) ctx.spacePanning = true;
        return;
      }
      if (
        key === ctx.shortcuts.toggleMaskOverlay?.toLowerCase() &&
        OVERLAY_CAPABLE_MASK_OPS.includes(ctx.selectedMask?.op ?? "")
      ) {
        e.preventDefault();
        ctx.showMaskOverlay = !ctx.showMaskOverlay;
        return;
      }
      if (key === ctx.shortcuts.toggleMaskChrome?.toLowerCase()) {
        e.preventDefault();
        ctx.maskOverlaysVisible = !ctx.maskOverlaysVisible;
        return;
      }
      if (rawKey === ctx.shortcuts.toggleOriginal || key === ctx.shortcuts.toggleOriginal?.toLowerCase()) {
        e.preventDefault();
        ctx.showOriginal = !ctx.showOriginal;
        return;
      }
      return;
    }

    if (ctx.activeModule !== "library") return;
    if (
      ctx.confirmingRemoval ||
      ctx.exportItems !== null ||
      ctx.creatingCollection ||
      ctx.creatingSmartCollection ||
      ctx.creatingCollectionWithImages ||
      ctx.settingsOpen ||
      ctx.backupPromptOpen
    ) {
      return;
    }

    // Select All: Cmd+A / Ctrl+A
    if ((e.metaKey || e.ctrlKey) && !e.altKey && key === "a") {
      e.preventDefault();
      ctx.handleSelectAll();
      return;
    }
    // Deselect All / Back to Grid: Cmd+D / Ctrl+D / Escape
    if (((e.metaKey || e.ctrlKey) && !e.altKey && key === "d") || rawKey === "Escape") {
      e.preventDefault();
      ctx.handleDeselectAll();
      return;
    }

    if (rawKey === "Delete" || rawKey === "Backspace") {
      if (ctx.selectedIds.size === 0) return;
      e.preventDefault();
      ctx.confirmingRemoval = true;
      return;
    }

    // Arrow navigation in Library
    if (rawKey === ctx.shortcuts.nextImage || rawKey === "ArrowRight") {
      e.preventDefault();
      if (ctx.libraryViewMode === "compare") {
        ctx.handleCompareNextCandidate();
      } else {
        ctx.selectNextImage(e.shiftKey);
      }
      return;
    }
    if (rawKey === ctx.shortcuts.prevImage || rawKey === "ArrowLeft") {
      e.preventDefault();
      if (ctx.libraryViewMode === "compare") {
        ctx.handleComparePrevCandidate();
      } else {
        ctx.selectPrevImage(e.shiftKey);
      }
      return;
    }
    if (rawKey === ctx.shortcuts.gridDown || rawKey === "ArrowDown") {
      e.preventDefault();
      if (ctx.libraryViewMode === "grid") {
        ctx.selectGridStep(4, e.shiftKey);
      } else {
        ctx.selectNextImage(e.shiftKey);
      }
      return;
    }
    if (rawKey === ctx.shortcuts.gridUp || rawKey === "ArrowUp") {
      e.preventDefault();
      if (ctx.libraryViewMode === "grid") {
        ctx.selectGridStep(-4, e.shiftKey);
      } else {
        ctx.selectPrevImage(e.shiftKey);
      }
      return;
    }

    if (e.metaKey || e.ctrlKey || e.altKey) return;

    // Mode hotkeys
    if (key === ctx.shortcuts.viewGrid?.toLowerCase()) {
      e.preventDefault();
      ctx.libraryViewMode = "grid";
      return;
    }
    if (key === ctx.shortcuts.viewLoupe?.toLowerCase() || rawKey === "Enter") {
      e.preventDefault();
      if (ctx.selectedId !== null) {
        ctx.libraryViewMode = "loupe";
      } else if (ctx.filteredImages.length > 0) {
        ctx.selectedId = ctx.filteredImages[0].version_id;
        ctx.selectedIds = new Set([ctx.filteredImages[0].version_id]);
        ctx.libraryViewMode = "loupe";
      }
      return;
    }
    if (key === ctx.shortcuts.viewCompare?.toLowerCase()) {
      e.preventDefault();
      ctx.libraryViewMode = "compare";
      return;
    }
    if (key === ctx.shortcuts.viewSurvey?.toLowerCase()) {
      e.preventDefault();
      ctx.libraryViewMode = "survey";
      return;
    }
    if (key === ctx.shortcuts.viewMap?.toLowerCase()) {
      e.preventDefault();
      ctx.showMapView();
      return;
    }
    if (key === ctx.shortcuts.viewDevelop?.toLowerCase()) {
      e.preventDefault();
      if (ctx.selectedId !== null) {
        ctx.openDevelop(ctx.selectedId);
      } else if (ctx.filteredImages.length > 0) {
        ctx.openDevelop(ctx.filteredImages[0].version_id);
      }
      return;
    }
    if (rawKey === " " || key === ctx.shortcuts.toggleView?.toLowerCase()) {
      e.preventDefault();
      if (ctx.libraryViewMode === "grid") {
        if (ctx.selectedId !== null) ctx.libraryViewMode = "loupe";
      } else if (ctx.libraryViewMode === "loupe") {
        ctx.libraryViewMode = "grid";
      }
      return;
    }

    // Rating shortcuts
    if (rawKey === ctx.shortcuts.rate0) {
      e.preventDefault();
      ctx.handleRatingChange(ctx.selectedId, 0);
      return;
    }
    if (rawKey === ctx.shortcuts.rate1) {
      e.preventDefault();
      ctx.handleRatingChange(ctx.selectedId, 1);
      return;
    }
    if (rawKey === ctx.shortcuts.rate2) {
      e.preventDefault();
      ctx.handleRatingChange(ctx.selectedId, 2);
      return;
    }
    if (rawKey === ctx.shortcuts.rate3) {
      e.preventDefault();
      ctx.handleRatingChange(ctx.selectedId, 3);
      return;
    }
    if (rawKey === ctx.shortcuts.rate4) {
      e.preventDefault();
      ctx.handleRatingChange(ctx.selectedId, 4);
      return;
    }
    if (rawKey === ctx.shortcuts.rate5) {
      e.preventDefault();
      ctx.handleRatingChange(ctx.selectedId, 5);
      return;
    }

    // Flag shortcuts
    if (key === ctx.shortcuts.flagPick?.toLowerCase()) {
      e.preventDefault();
      ctx.handleFlagChange(ctx.selectedId, ctx.selectedImage?.flag === "pick" ? "none" : "pick");
      return;
    }
    if (key === ctx.shortcuts.flagReject?.toLowerCase()) {
      e.preventDefault();
      ctx.handleFlagChange(ctx.selectedId, ctx.selectedImage?.flag === "reject" ? "none" : "reject");
      return;
    }
    if (key === ctx.shortcuts.flagUnflag?.toLowerCase()) {
      e.preventDefault();
      ctx.handleFlagChange(ctx.selectedId, "none");
      return;
    }

    // Color labels
    if (key === ctx.shortcuts.colorRed?.toLowerCase()) {
      e.preventDefault();
      ctx.handleColorLabelChange(ctx.selectedId, ctx.selectedImage?.color_label === "red" ? "none" : "red");
      return;
    }
    if (key === ctx.shortcuts.colorYellow?.toLowerCase()) {
      e.preventDefault();
      ctx.handleColorLabelChange(ctx.selectedId, ctx.selectedImage?.color_label === "yellow" ? "none" : "yellow");
      return;
    }
    if (key === ctx.shortcuts.colorGreen?.toLowerCase()) {
      e.preventDefault();
      ctx.handleColorLabelChange(ctx.selectedId, ctx.selectedImage?.color_label === "green" ? "none" : "green");
      return;
    }
    if (key === ctx.shortcuts.colorBlue?.toLowerCase()) {
      e.preventDefault();
      ctx.handleColorLabelChange(ctx.selectedId, ctx.selectedImage?.color_label === "blue" ? "none" : "blue");
      return;
    }
  }

  // M4 Slice 3: releases space-pan (see spacePanning's own doc comment).
  // No input-focus/dialog guards needed here, unlike handleGlobalKeydown --
  // clearing this is always safe even if focus moved somewhere else while
  // the key was held, since spacePanning can only have been set true by
  // that same keydown handler's own guarded path in the first place.
  function handleGlobalKeyup(/** @type {KeyboardEvent} */ e) {
    if (e.key === " ") ctx.spacePanning = false;
  }

  return { handleGlobalKeydown, handleGlobalKeyup };
}
