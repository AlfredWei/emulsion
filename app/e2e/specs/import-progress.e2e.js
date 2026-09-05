import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const FIXTURE_DIR = path.resolve(__dirname, "../../../test_image");

/**
 * Import's native file/folder picker can't be driven by WebDriver (see
 * golden-path.e2e.js's own file-level comment for the full explanation),
 * so this drives the same backend command the real "Import Files…" button
 * invokes, directly via `window.__TAURI__.core.invoke` -- the same
 * dialog-bypass pattern golden-path.e2e.js already uses for its own import
 * step. What's new to this spec is the assertion: that a real multi-file
 * import emits a complete, ordered `import-progress` event sequence
 * (lib.rs's import_files -> import.rs's `import_paths_with_progress`),
 * which is the data the toolbar's import progress bar renders from.
 *
 * Uses the four JPEG fixtures under test_image/ (not a single file) so
 * there's more than one event to order-check, and deliberately avoids the
 * RAW sample gated behind EMULSION_TEST_RAW_SAMPLE (see ci.yml) -- this
 * spec doesn't need a real RAW decode, just multiple candidate files.
 * Safe to re-run against an accumulated local catalog like golden-path.e2e.js:
 * a duplicate still counts as a candidate file and still gets a progress
 * event (see import.rs's own `progress_callback_fires_once_per_candidate_file_in_order`
 * unit test), so the event count is stable across repeated runs.
 */
describe("Import progress: real multi-file import emits ordered progress events", () => {
  it("emits one 'import-progress' event per candidate file, ending at (total, total)", async () => {
    const files = fs
      .readdirSync(FIXTURE_DIR)
      .filter((f) => /\.(jpe?g|dng|cr2|nef|arw)$/i.test(f))
      .map((f) => path.join(FIXTURE_DIR, f));
    expect(files.length).toBeGreaterThan(1);

    const events = await browser.execute(async (paths) => {
      const collected = [];
      const unlisten = await window.__TAURI__.event.listen("import-progress", (event) => {
        collected.push(event.payload);
      });
      await window.__TAURI__.core.invoke("import_files", { paths });
      unlisten();
      return collected;
    }, files);

    expect(events.length).toBe(files.length);
    for (let i = 0; i < events.length; i++) {
      expect(events[i]).toEqual({ current: i + 1, total: files.length });
    }
  });
});
