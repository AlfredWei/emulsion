// Soft-proof workflow (RFC-0009 P6b, moved out of +page.svelte's script): choosing a custom ICC
// profile file, which also switches the proof target to it.

import { open } from "@tauri-apps/plugin-dialog";
import { softProof } from "$lib/state/softProof.svelte.js";

/** Mirrors the existing single-file picker precedent (`handleImportPresetRequest` in
 * presetActions.js), just with an ICC/ICM extension filter instead of JSON. Selecting a
 * new custom profile also switches `softProof.target` to "custom" -- picking
 * a file only to leave a different profile active would be confusing. */
export async function handleChooseCustomProfile() {
  const path = await open({ multiple: false, filters: [{ name: "ICC Profile", extensions: ["icc", "icm"] }] });
  if (!path || Array.isArray(path)) return;
  softProof.customProfilePath = path;
  softProof.target = "custom";
}
