// Thin wrapper over @tauri-apps/plugin-opener (M4 Library depth slice) --
// same "keep raw plugin/command names out of components" shape as
// catalog.js's own doc comment. `opener:default`'s permission set (see
// src-tauri/capabilities/default.json) already includes
// `allow-reveal-item-in-dir`, so no new Rust command or capability grant
// was needed for this -- the plugin's own cross-platform implementation
// (Finder on macOS, Explorer on Windows, the default file manager on
// Linux) is used directly.

import { revealItemInDir } from "@tauri-apps/plugin-opener";

/** Opens the OS file manager with `path` selected. @returns {Promise<void>} */
export function revealInFileManager(/** @type {string} */ path) {
  return revealItemInDir(path);
}
