// Native OS menu bar actions (RFC-0009 P2, moved out of +page.svelte's script).
//
// `createMenuHandler(ctx)` returns the handler for the `"menu-action"` event. `ctx` is the
// page's live view of the state and callbacks it touches (see MenuContext).

/**
 * @typedef {Object} MenuContext
 * @property {string} activeModule
 * @property {boolean} settingsOpen
 * @property {"grid" | "loupe" | "compare" | "survey" | "map"} libraryViewMode
 * @property {boolean} showOriginal
 * @property {() => unknown} handleImportFolder
 * @property {() => Promise<void>} handleImportFiles
 * @property {() => Promise<void>} handleExportClick
 * @property {() => Promise<void>} handleExportPdf
 * @property {() => void} handleUndo
 * @property {() => void} handleRedo
 * @property {() => void} handleCopySettingsRequest
 * @property {() => Promise<void>} handlePasteSettings
 * @property {() => void} handleSelectAll
 * @property {() => void} handleDeselectAll
 * @property {(target: string) => Promise<void>} switchModule
 * @property {() => void} handleToggleClippingOverlay
 */

/** @param {MenuContext} ctx */
export function createMenuHandler(ctx) {
  /** Native OS menu bar (M4.5 Slice 4, see build_menu in lib.rs): a menu
   * click reaches here as a `"menu-action"` event carrying the clicked
   * item's id, and every case below just calls the exact same handler an
   * equivalent in-UI button already calls -- no new behavior, only a new
   * entry point. Module-scoped actions (Select All, the Develop-only
   * toggles) no-op outside their module rather than acting on
   * invisible/irrelevant state, matching how the in-UI controls they
   * mirror are only ever rendered in the first place. */
  function handleMenuAction(/** @type {string} */ action) {
    switch (action) {
      case "import_folder":
        ctx.handleImportFolder();
        break;
      case "import_files":
        ctx.handleImportFiles();
        break;
      case "export":
        ctx.handleExportClick();
        break;
      case "export_pdf":
        ctx.handleExportPdf();
        break;
      case "preferences":
        ctx.settingsOpen = true;
        break;
      case "undo":
        if (ctx.activeModule === "develop") ctx.handleUndo();
        break;
      case "redo":
        if (ctx.activeModule === "develop") ctx.handleRedo();
        break;
      case "copy_settings":
        ctx.handleCopySettingsRequest();
        break;
      case "paste_settings":
        ctx.handlePasteSettings();
        break;
      case "select_all":
        if (ctx.activeModule === "library") ctx.handleSelectAll();
        break;
      case "deselect_all":
        if (ctx.activeModule === "library") ctx.handleDeselectAll();
        break;
      case "view_library":
        ctx.switchModule("library");
        break;
      case "view_develop":
        ctx.switchModule("develop");
        break;
      case "view_print":
        ctx.switchModule("print");
        break;
      case "view_grid":
        ctx.switchModule("library");
        ctx.libraryViewMode = "grid";
        break;
      case "view_loupe":
        ctx.switchModule("library");
        ctx.libraryViewMode = "loupe";
        break;
      case "view_compare":
        ctx.switchModule("library");
        ctx.libraryViewMode = "compare";
        break;
      case "view_survey":
        ctx.switchModule("library");
        ctx.libraryViewMode = "survey";
        break;
      case "toggle_clipping":
        if (ctx.activeModule === "develop") ctx.handleToggleClippingOverlay();
        break;
      case "toggle_before_after":
        if (ctx.activeModule === "develop") ctx.showOriginal = !ctx.showOriginal;
        break;
    }
  }

  return handleMenuAction;
}
