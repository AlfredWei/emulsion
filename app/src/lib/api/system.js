// Thin wrapper over @tauri-apps/plugin-opener (M4 Library depth slice) --
// same "keep raw plugin/command names out of components" shape as
// catalog.js's own doc comment. `opener:default`'s permission set (see
// src-tauri/capabilities/default.json) already includes
// `allow-reveal-item-in-dir`, so no new Rust command or capability grant
// was needed for this -- the plugin's own cross-platform implementation
// (Finder on macOS, Explorer on Windows, the default file manager on
// Linux) is used directly.

import { revealItemInDir, openPath, openUrl } from "@tauri-apps/plugin-opener";

/** Opens the OS file manager with `path` selected. @returns {Promise<void>} */
export function revealInFileManager(/** @type {string} */ path) {
  return revealItemInDir(path);
}

/** Opens `path` (a directory) in the OS file manager -- M4.5, added for
 * Export's "reveal destination folder when done". Unlike
 * `revealInFileManager`, `allow-open-path` is NOT part of `opener:default`
 * (see its own permissions/default.toml), so it's granted explicitly in
 * capabilities/default.json.
 * @returns {Promise<void>} */
export function openFolder(/** @type {string} */ path) {
  return openPath(path);
}

/** Opens an http(s) link in the default browser -- for links inside webview content (the map's
 * attribution), which would otherwise navigate the app window itself away. `opener:default` already
 * grants `allow-open-url` for web URLs. Outside Tauri (plain dev server) falls back to `window.open`.
 * @returns {Promise<void>} */
export async function openExternalUrl(/** @type {string} */ url) {
  try {
    await openUrl(url);
  } catch {
    window.open(url, "_blank", "noopener");
  }
}
