// Print workflows (RFC-0009 P3b, moved out of +page.svelte's script): choosing a print ICC
// profile, sending the job to the OS print dialog, and exporting a PDF. They read and write the
// `print` store and report through `shell.notify`; nothing here is component state.

import { open, save } from "@tauri-apps/plugin-dialog";
import { print } from "$lib/state/print.svelte.js";
import { getPrintReadyImages, PAPER_SIZES, exportPrintPdf } from "$lib/api/print.js";
import { convertFileSrc } from "@tauri-apps/api/core";
import { shell } from "$lib/state/shell.svelte.js";
import { tick } from "svelte";

/** Same file-picker precedent as Soft Proof's `handleChooseCustomProfile` (still in
 * +page.svelte), kept separate since it targets Print's own (not Soft Proof's) state. */
export async function handleChoosePrintCustomProfile() {
  const path = await open({ multiple: false, filters: [{ name: "ICC Profile", extensions: ["icc", "icm"] }] });
  if (!path || Array.isArray(path)) return;
  print.customProfilePath = path;
  print.profileTarget = "custom";
}

/** Generates the full-resolution, color-managed print payload for every
 * item in this print job, swaps it into the layout view, then triggers
 * the OS-native print dialog. Awaiting `tick()` before `window.print()`
 * gives the swapped `<img src>`s a chance to actually load -- the OS
 * dialog reads whatever's currently painted, not a promise. */
export async function handlePrint() {
  if (print.items.length === 0 || print.printing) return;
  print.printing = true;
  try {
    const results = await getPrintReadyImages(
      print.items.map((item) => item.version_id),
      {
        profile: print.colorManaged
          ? {
              target: print.profileTarget,
              custom_profile_path: print.customProfilePath,
              intent: print.intent,
              gamut_warning: false,
            }
          : null,
      },
    );
    const next = { ...print.readyUrls };
    const failures = [];
    for (const result of results) {
      if (result.path) {
        next[result.version_id] = convertFileSrc(result.path);
      } else {
        failures.push(result.error ?? "unknown error");
      }
    }
    print.readyUrls = next;
    if (failures.length > 0) {
      shell.notify(`Print: ${failures.length} photo(s) could not be prepared (${failures[0]})`);
      if (failures.length === results.length) return;
    }
    await tick();
    window.print();
  } catch (e) {
    shell.notify(`Print failed: ${e}`);
  } finally {
    print.printing = false;
  }
}

/** "Export as PDF" -- a direct alternative to handlePrint's window.print()
 * flow: picks a destination via the same save-dialog convention as
 * Export/preset-export, then asks the Rust side to compose a real PDF
 * from the print-ready rasters (same generation/caching path handlePrint
 * uses via get_print_ready_images), skipping the OS print dialog. */
export async function handleExportPdf() {
  if (print.items.length === 0 || print.exportingPdf) return;
  const destinationPath = await save({
    filters: [{ name: "PDF", extensions: ["pdf"] }],
    defaultPath: "print.pdf",
  });
  if (!destinationPath) return;
  print.exportingPdf = true;
  try {
    const paper = PAPER_SIZES[/** @type {keyof typeof PAPER_SIZES} */ (print.paperSize)] ?? PAPER_SIZES.letter;
    const pageWidthIn = print.orientation === "landscape" ? paper.heightIn : paper.widthIn;
    const pageHeightIn = print.orientation === "landscape" ? paper.widthIn : paper.heightIn;
    await exportPrintPdf({
      version_ids: print.items.map((item) => item.version_id),
      destination_path: destinationPath,
      layout: {
        template: print.template,
        fit_mode: print.fitMode,
        rows: print.rows,
        cols: print.cols,
        cell_spacing_in: print.cellSpacing,
      },
      page: {
        width_in: pageWidthIn,
        height_in: pageHeightIn,
        margin_top_in: print.margins.top,
        margin_right_in: print.margins.right,
        margin_bottom_in: print.margins.bottom,
        margin_left_in: print.margins.left,
      },
      color_management: {
        profile: print.colorManaged
          ? {
              target: print.profileTarget,
              custom_profile_path: print.customProfilePath,
              intent: print.intent,
              gamut_warning: false,
            }
          : null,
      },
    });
    shell.notify(`Exported PDF to ${destinationPath}`);
  } catch (e) {
    shell.notify(`PDF export failed: ${e}`);
  } finally {
    print.exportingPdf = false;
  }
}
