import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

const { open, save, getPrintReadyImages, exportPrintPdf } = vi.hoisted(() => ({
  open: vi.fn(),
  save: vi.fn(),
  getPrintReadyImages: vi.fn(),
  exportPrintPdf: vi.fn(),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open, save }));
vi.mock("@tauri-apps/api/core", () => ({ convertFileSrc: (/** @type {string} */ p) => `asset://${p}` }));
vi.mock("$lib/api/print.js", async (importActual) => ({
  ...(await importActual()),
  getPrintReadyImages,
  exportPrintPdf,
}));

import { handlePrint, handleExportPdf, handleChoosePrintCustomProfile } from "./printActions.js";
import { print } from "$lib/state/print.svelte.js";
import { shell } from "$lib/state/shell.svelte.js";

// The OS print dialog: `window.print()` in the app; Node has no `window`, so stub one.
const windowPrint = vi.fn();

const ITEMS = [
  { path: "/a.jpg", version_id: 1 },
  { path: "/b.jpg", version_id: 2 },
];

beforeEach(() => {
  vi.clearAllMocks();
  windowPrint.mockClear();
  vi.stubGlobal("window", { print: windowPrint });
  Object.assign(print, {
    items: [],
    template: "single",
    fitMode: "fit",
    rows: 2,
    cols: 2,
    cellSpacing: 0.1,
    paperSize: "letter",
    orientation: "portrait",
    margins: { top: 0.5, right: 0.5, bottom: 0.5, left: 0.5 },
    colorManaged: false,
    profileTarget: "srgb",
    customProfilePath: null,
    intent: "relative",
    printing: false,
    exportingPdf: false,
    readyUrls: {},
  });
  shell.statusMessage = "";
});
afterEach(() => vi.unstubAllGlobals());

describe("handleChoosePrintCustomProfile", () => {
  it("stores the chosen profile and switches the target to custom", async () => {
    open.mockResolvedValue("/icc/mine.icc");
    await handleChoosePrintCustomProfile();
    expect(print.customProfilePath).toBe("/icc/mine.icc");
    expect(print.profileTarget).toBe("custom");
  });

  it("does nothing when the picker is cancelled or returns several paths", async () => {
    for (const result of [null, ["/a.icc", "/b.icc"]]) {
      open.mockResolvedValue(result);
      await handleChoosePrintCustomProfile();
    }
    expect(print.customProfilePath).toBeNull();
    expect(print.profileTarget).toBe("srgb");
  });
});

describe("handlePrint", () => {
  it("does nothing with no items, or while a print is already running", async () => {
    await handlePrint();
    print.items = ITEMS;
    print.printing = true;
    await handlePrint();
    expect(getPrintReadyImages).not.toHaveBeenCalled();
  });

  it("swaps print-ready rasters in, opens the OS dialog, and clears the busy flag", async () => {
    print.items = ITEMS;
    getPrintReadyImages.mockResolvedValue([
      { version_id: 1, path: "/out/1.png" },
      { version_id: 2, path: "/out/2.png" },
    ]);
    await handlePrint();
    expect(getPrintReadyImages).toHaveBeenCalledWith([1, 2], { profile: null });
    expect(print.readyUrls).toEqual({ 1: "asset:///out/1.png", 2: "asset:///out/2.png" });
    expect(windowPrint).toHaveBeenCalledTimes(1);
    expect(print.printing).toBe(false);
    expect(shell.statusMessage).toBe("");
  });

  it("sends the colour-managed profile only when colour management is on", async () => {
    print.items = ITEMS;
    print.colorManaged = true;
    print.profileTarget = "custom";
    print.customProfilePath = "/icc/p.icc";
    print.intent = "perceptual";
    getPrintReadyImages.mockResolvedValue([{ version_id: 1, path: "/o.png" }]);
    await handlePrint();
    expect(getPrintReadyImages).toHaveBeenCalledWith([1, 2], {
      profile: { target: "custom", custom_profile_path: "/icc/p.icc", intent: "perceptual", gamut_warning: false },
    });
  });

  it("with some failures: reports the first, still prints the ones that worked", async () => {
    print.items = ITEMS;
    getPrintReadyImages.mockResolvedValue([
      { version_id: 1, path: "/out/1.png" },
      { version_id: 2, error: "decode failed" },
    ]);
    await handlePrint();
    expect(shell.statusMessage).toBe("Print: 1 photo(s) could not be prepared (decode failed)");
    expect(print.readyUrls).toEqual({ 1: "asset:///out/1.png" });
    expect(windowPrint).toHaveBeenCalledTimes(1);
  });

  it("when every photo fails: reports it and does not open the print dialog", async () => {
    print.items = ITEMS;
    getPrintReadyImages.mockResolvedValue([{ version_id: 1 }, { version_id: 2, error: "boom" }]);
    await handlePrint();
    expect(shell.statusMessage).toBe("Print: 2 photo(s) could not be prepared (unknown error)");
    expect(windowPrint).not.toHaveBeenCalled();
    expect(print.printing).toBe(false);
  });

  it("an exception is reported and still clears the busy flag", async () => {
    print.items = ITEMS;
    getPrintReadyImages.mockRejectedValue("ipc down");
    await handlePrint();
    expect(shell.statusMessage).toBe("Print failed: ipc down");
    expect(print.printing).toBe(false);
    expect(windowPrint).not.toHaveBeenCalled();
  });

  it("merges into existing ready URLs instead of discarding them", async () => {
    print.items = ITEMS;
    print.readyUrls = { 9: "asset:///old.png" };
    getPrintReadyImages.mockResolvedValue([{ version_id: 1, path: "/1.png" }]);
    await handlePrint();
    expect(print.readyUrls).toEqual({ 9: "asset:///old.png", 1: "asset:///1.png" });
  });
});

describe("handleExportPdf", () => {
  it("does nothing with no items or while an export is running, and never asks for a path", async () => {
    await handleExportPdf();
    print.items = ITEMS;
    print.exportingPdf = true;
    await handleExportPdf();
    expect(save).not.toHaveBeenCalled();
  });

  it("a cancelled save dialog exports nothing and leaves the busy flag alone", async () => {
    print.items = ITEMS;
    save.mockResolvedValue(null);
    await handleExportPdf();
    expect(exportPrintPdf).not.toHaveBeenCalled();
    expect(print.exportingPdf).toBe(false);
  });

  it("builds the request from the layout and page settings and reports the destination", async () => {
    print.items = ITEMS;
    print.template = "contact-sheet";
    print.fitMode = "fill";
    print.rows = 3;
    print.cols = 4;
    print.cellSpacing = 0.25;
    print.margins = { top: 1, right: 2, bottom: 3, left: 4 };
    save.mockResolvedValue("/out/print.pdf");
    exportPrintPdf.mockResolvedValue(undefined);
    await handleExportPdf();
    const req = exportPrintPdf.mock.calls[0][0];
    expect(req.version_ids).toEqual([1, 2]);
    expect(req.destination_path).toBe("/out/print.pdf");
    expect(req.layout).toEqual({ template: "contact-sheet", fit_mode: "fill", rows: 3, cols: 4, cell_spacing_in: 0.25 });
    expect(req.page).toMatchObject({ margin_top_in: 1, margin_right_in: 2, margin_bottom_in: 3, margin_left_in: 4 });
    expect(req.color_management).toEqual({ profile: null });
    expect(shell.statusMessage).toBe("Exported PDF to /out/print.pdf");
    expect(print.exportingPdf).toBe(false);
  });

  it("landscape swaps the page width and height", async () => {
    print.items = ITEMS;
    save.mockResolvedValue("/out/p.pdf");
    exportPrintPdf.mockResolvedValue(undefined);
    await handleExportPdf();
    const portrait = exportPrintPdf.mock.calls[0][0].page;
    print.orientation = "landscape";
    await handleExportPdf();
    const landscape = exportPrintPdf.mock.calls[1][0].page;
    expect(portrait.width_in).toBeLessThan(portrait.height_in);
    expect(landscape.width_in).toBe(portrait.height_in);
    expect(landscape.height_in).toBe(portrait.width_in);
  });

  it("an unknown paper size falls back to Letter", async () => {
    print.items = ITEMS;
    save.mockResolvedValue("/out/p.pdf");
    exportPrintPdf.mockResolvedValue(undefined);
    await handleExportPdf();
    const letter = exportPrintPdf.mock.calls[0][0].page;
    print.paperSize = "no-such-size";
    await handleExportPdf();
    expect(exportPrintPdf.mock.calls[1][0].page.width_in).toBe(letter.width_in);
  });

  it("an export error is reported and clears the busy flag", async () => {
    print.items = ITEMS;
    save.mockResolvedValue("/out/p.pdf");
    exportPrintPdf.mockRejectedValue("disk full");
    await handleExportPdf();
    expect(shell.statusMessage).toBe("PDF export failed: disk full");
    expect(print.exportingPdf).toBe(false);
  });
});
