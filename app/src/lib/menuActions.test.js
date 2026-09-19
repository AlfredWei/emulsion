import { describe, it, expect, vi } from "vitest";
import { createMenuHandler } from "./menuActions.js";

function makeCtx(over = {}) {
  return {
    activeModule: "library",
    settingsOpen: false,
    libraryViewMode: /** @type {"grid" | "loupe" | "compare" | "survey"} */ ("grid"),
    showOriginal: false,
    handleImportFolder: vi.fn(),
    handleImportFiles: vi.fn(async () => {}),
    handleExportClick: vi.fn(async () => {}),
    handleExportPdf: vi.fn(async () => {}),
    handleUndo: vi.fn(),
    handleRedo: vi.fn(),
    handleCopySettingsRequest: vi.fn(),
    handlePasteSettings: vi.fn(async () => {}),
    handleSelectAll: vi.fn(),
    handleDeselectAll: vi.fn(),
    switchModule: vi.fn(async () => {}),
    handleToggleClippingOverlay: vi.fn(),
    ...over,
  };
}

describe("menu actions", () => {
  it("import/export/settings items call the same handler as their in-UI buttons, in any module", () => {
    for (const activeModule of ["library", "develop", "print"]) {
      const ctx = makeCtx({ activeModule });
      const menu = createMenuHandler(ctx);
      menu("import_folder");
      menu("import_files");
      menu("export");
      menu("export_pdf");
      menu("copy_settings");
      menu("paste_settings");
      menu("preferences");
      expect(ctx.handleImportFolder).toHaveBeenCalledTimes(1);
      expect(ctx.handleImportFiles).toHaveBeenCalledTimes(1);
      expect(ctx.handleExportClick).toHaveBeenCalledTimes(1);
      expect(ctx.handleExportPdf).toHaveBeenCalledTimes(1);
      expect(ctx.handleCopySettingsRequest).toHaveBeenCalledTimes(1);
      expect(ctx.handlePasteSettings).toHaveBeenCalledTimes(1);
      expect(ctx.settingsOpen).toBe(true);
    }
  });

  it("module switches", () => {
    const ctx = makeCtx();
    const menu = createMenuHandler(ctx);
    menu("view_library");
    menu("view_develop");
    menu("view_print");
    expect(ctx.switchModule.mock.calls.map((c) => c[0])).toEqual(["library", "develop", "print"]);
  });

  it("view modes switch to Library first, then set the mode", () => {
    for (const [action, mode] of [["view_grid", "grid"], ["view_loupe", "loupe"], ["view_compare", "compare"], ["view_survey", "survey"]]) {
      const ctx = makeCtx({ activeModule: "develop" });
      createMenuHandler(ctx)(action);
      expect(ctx.switchModule).toHaveBeenCalledWith("library");
      expect(ctx.libraryViewMode).toBe(mode);
    }
  });

  it("Develop-only actions no-op elsewhere", () => {
    const lib = makeCtx();
    const menu = createMenuHandler(lib);
    for (const a of ["undo", "redo", "toggle_clipping", "toggle_before_after"]) menu(a);
    expect(lib.handleUndo).not.toHaveBeenCalled();
    expect(lib.handleRedo).not.toHaveBeenCalled();
    expect(lib.handleToggleClippingOverlay).not.toHaveBeenCalled();
    expect(lib.showOriginal).toBe(false);

    const dev = makeCtx({ activeModule: "develop" });
    const devMenu = createMenuHandler(dev);
    devMenu("undo");
    devMenu("redo");
    devMenu("toggle_clipping");
    devMenu("toggle_before_after");
    expect(dev.handleUndo).toHaveBeenCalledTimes(1);
    expect(dev.handleRedo).toHaveBeenCalledTimes(1);
    expect(dev.handleToggleClippingOverlay).toHaveBeenCalledTimes(1);
    expect(dev.showOriginal).toBe(true);
  });

  it("Library-only selection actions no-op elsewhere", () => {
    const dev = makeCtx({ activeModule: "develop" });
    const devMenu = createMenuHandler(dev);
    devMenu("select_all");
    devMenu("deselect_all");
    expect(dev.handleSelectAll).not.toHaveBeenCalled();
    expect(dev.handleDeselectAll).not.toHaveBeenCalled();

    const lib = makeCtx();
    const menu = createMenuHandler(lib);
    menu("select_all");
    menu("deselect_all");
    expect(lib.handleSelectAll).toHaveBeenCalledTimes(1);
    expect(lib.handleDeselectAll).toHaveBeenCalledTimes(1);
  });

  it("an unknown action does nothing", () => {
    const ctx = makeCtx();
    expect(() => createMenuHandler(ctx)("definitely_not_an_action")).not.toThrow();
    expect(ctx.switchModule).not.toHaveBeenCalled();
  });
});
