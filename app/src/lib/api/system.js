import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";

/** Opens the OS file manager with `path` selected. The Rust side does it, so the webview's capability grants no
 * file-system access to the opener plugin. @returns {Promise<void>} */
export function revealInFileManager(/** @type {string} */ path) {
  return invoke("reveal_in_file_manager", { path });
}

/** Opens `path` (a directory) in the OS file manager, for Export's "reveal destination folder when done".
 * @returns {Promise<void>} */
export function openFolder(/** @type {string} */ path) {
  return invoke("open_folder", { path });
}

/** Opens an http(s) link in the default browser -- for links inside webview content (the map's
 * attribution), which would otherwise navigate the app window itself away. The capability grants
 * `allow-open-url` for web URLs only. Outside Tauri (plain dev server) falls back to `window.open`.
 * @returns {Promise<void>} */
export async function openExternalUrl(/** @type {string} */ url) {
  try {
    await openUrl(url);
  } catch {
    window.open(url, "_blank", "noopener");
  }
}

/** Opens the bundled THIRD_PARTY_NOTICES.md (the licences of the AI models that ship with the app) in the OS's
 * default viewer. The Rust side finds the file -- next to the app in an installed build, the checkout's copy in a
 * development build -- and opens it itself, so the webview's `allow-open-path` scope is not involved.
 * @returns {Promise<void>} */
export function openThirdPartyNotices() {
  return invoke("open_third_party_notices");
}

/** Opens the bundled THIRD_PARTY_LIBRARIES.md (the licences of the code libraries Emulsion is built from, and where
 * the full texts of the ones with conditions are) in the OS's default viewer. Opened the same way as the notices.
 * @returns {Promise<void>} */
export function openThirdPartyLibraries() {
  return invoke("open_third_party_libraries");
}

/** @type {Promise<import('$lib/modelEdition.js').EditionInfo> | null} */
let edition = null;

/** The running edition: its name, the model families it leaves out (RFC-0038 slice 4, ADR-0012), its channel and what
 * the channel contains, and the features compiled out of the binary (ADR-0011). Fixed for the run, so it is asked once;
 * a failed call is asked again next time.
 * @returns {Promise<import('$lib/modelEdition.js').EditionInfo>} */
export function getModelEdition() {
  edition ??= invoke("model_edition").catch((e) => {
    edition = null;
    throw e;
  });
  return edition;
}
