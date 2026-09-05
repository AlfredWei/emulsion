import fs from "node:fs";
import os from "node:os";
import path from "node:path";

/**
 * Covers Settings > Storage (storage.rs): get_storage_info/set_cache_dir
 * and the SettingsDialog UI that surfaces them. The actual "Choose
 * Folder…" button can't be driven end-to-end -- it opens a native OS
 * picker, which WebDriver can't reach (same limitation golden-path.e2e.js
 * documents at length for Import/Export) -- so this calls set_cache_dir
 * directly via invoke, exactly like a real folder pick would, and
 * verifies the UI separately by just opening Settings and reading what
 * it renders from a real get_storage_info response.
 *
 * IMPORTANT: this test moves the real shared dev catalog's actual
 * thumbnails/previews to a temp directory and back. The `after` hook is
 * the only thing standing between a clean run and every other manual
 * session/e2e spec finding its thumbnails missing -- it unconditionally
 * resets cache_dir to the default (None) no matter how the test body
 * exits, not just on success.
 */
describe("Settings > Storage: cache location", () => {
  let tmpDir;
  let originalInfo;

  before(async () => {
    await browser.setTimeout({ script: 30000 });
    tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "emulsion-e2e-storage-"));
    originalInfo = await browser.execute(() => window.__TAURI__.core.invoke("get_storage_info"));
  });

  after(async () => {
    // Unconditional reset -- see file comment. Best-effort: this is
    // cleanup, not an assertion, so it must never throw and mask the
    // real test outcome.
    try {
      await browser.execute(() => window.__TAURI__.core.invoke("set_cache_dir", { newDir: null }));
    } catch {
      // ignore
    }
    fs.rmSync(tmpDir, { recursive: true, force: true });
  });

  it("get_storage_info reports the real default location and non-negative usage", () => {
    expect(originalInfo.cache_dir).toBeNull();
    expect(typeof originalInfo.effective_dir).toBe("string");
    expect(originalInfo.effective_dir.length).toBeGreaterThan(0);
    expect(originalInfo.thumbnails_bytes).toBeGreaterThanOrEqual(0);
    expect(originalInfo.previews_bytes).toBeGreaterThanOrEqual(0);
  });

  it("set_cache_dir moves real files to the new location and back", async () => {
    const moved = await browser.execute(
      (dir) => window.__TAURI__.core.invoke("set_cache_dir", { newDir: dir }),
      tmpDir,
    );
    expect(moved.cache_dir).toBe(tmpDir);
    expect(moved.effective_dir).toBe(tmpDir);
    // Whatever usage the default location had before, it now shows up
    // under the new one -- a real move, not just a pointer update.
    expect(moved.thumbnails_bytes).toBe(originalInfo.thumbnails_bytes);
    expect(moved.previews_bytes).toBe(originalInfo.previews_bytes);

    const restored = await browser.execute(() => window.__TAURI__.core.invoke("set_cache_dir", { newDir: null }));
    expect(restored.cache_dir).toBeNull();
    expect(restored.effective_dir).toBe(originalInfo.effective_dir);
    expect(restored.thumbnails_bytes).toBe(originalInfo.thumbnails_bytes);
    expect(restored.previews_bytes).toBe(originalInfo.previews_bytes);
  });

  it("Settings dialog's Storage tab renders the real effective directory", async () => {
    // Preferences… is on the app menu (not drivable), but +page.svelte
    // also exposes it as a plain command via the "menu-action" event --
    // dispatch that directly, same "skip the OS chrome, drive the
    // resulting command" approach golden-path.e2e.js uses for imports.
    // Buttons are found/clicked via a real DOM query + dispatched click
    // (not a WebdriverIO `$` text selector), matching hdr-merge.e2e.js's
    // established pattern -- see helpers.js's findCellByNameAnywhere for
    // why: every `$`/`elementClick` command pays a real, flat ~5s tax in
    // this environment, so plain `execute()` is both more reliable and
    // far cheaper here.
    await browser.execute(() => window.__TAURI__.event.emit("menu-action", "preferences"));
    await browser.pause(300); // let the dialog itself mount

    await browser.execute(() => {
      const storageTab = Array.from(document.querySelectorAll(".tab-btn")).find((b) => b.textContent === "Storage");
      storageTab?.click();
    });

    // Polled via execute() rather than a single fixed pause -- the tab
    // switch itself is instant, but the panel only renders once its own
    // getStorageInfo() call resolves (a real, if fast, IPC round trip).
    let folderBtnText = null;
    for (let i = 0; i < 20 && folderBtnText === null; i++) {
      folderBtnText = await browser.execute(() => document.querySelector(".folder-btn")?.textContent ?? null);
      if (folderBtnText === null) await browser.pause(200);
    }
    expect(folderBtnText).not.toBeNull();
    expect(/** @type {string} */ (folderBtnText).length).toBeGreaterThan(0);

    await browser.execute(() => {
      const closeBtn = Array.from(document.querySelectorAll("button")).find((b) => b.textContent === "Close");
      closeBtn?.click();
    });
  });
});
