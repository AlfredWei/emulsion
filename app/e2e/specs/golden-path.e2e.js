import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const FIXTURE_PATH = path.resolve(__dirname, "../../../test_image/Field-corn-Liechtenstein-landscape.jpg");
const FIXTURE_NAME = "Field-corn-Liechtenstein-landscape";

/**
 * The app's own catalog data dir (and its SQLite catalog) is not reset
 * between local runs of this suite, so this test is written to be safe to
 * re-run against an accumulated catalog: it looks up the fixture by name
 * rather than assuming the grid is otherwise empty, and tolerates the
 * "already in library" duplicate-import outcome. CI runners are always
 * fresh, so this never comes up there.
 *
 * Native OS file/folder pickers (Import Files…, Export's destination
 * picker) are outside the webview and cannot be driven by WebDriver.
 * Intercepting them by monkey-patching `window.__TAURI_INTERNALS__.invoke`
 * (the transport `@tauri-apps/plugin-dialog`'s `open()` calls under the
 * hood) does not work -- Tauri v2 defines that property (and the object
 * itself on `window`) as non-configurable, so both a direct reassignment
 * and a `Proxy` substitution silently no-op. Properly mocking it requires
 * the separate `tauri-plugin-wdio` plugin + its `@wdio/tauri-plugin`
 * frontend import, which is out of scope for this slice (only
 * `tauri-plugin-wdio-webdriver`, for the embedded WebDriver server itself,
 * was planned). Instead, this spec calls the exact same Tauri commands the
 * dialog-gated buttons would eventually call, directly via
 * `window.__TAURI__.core.invoke` (exposed because `withGlobalTauri` is on
 * in the e2e build config) -- real backend commands, same catalog writes,
 * same file I/O, just without the OS chrome in the middle. Every other
 * interaction (opening Develop, editing Exposure, the Export button
 * enabling) still drives the real UI.
 */
function findFixtureCell() {
  return $(`//div[contains(concat(' ', @class, ' '), ' cell ')][.//span[contains(@class, 'file-name') and contains(text(), '${FIXTURE_NAME}')]]`);
}

/** A WebdriverIO `.click()` (real pointer-down/up actions) intermittently
 * doesn't register in this embedded-WebDriver + WKWebView combination.
 * Dispatching the DOM method directly is what actually reaches Svelte's
 * `onclick` handlers reliably here. */
async function clickEl(elOrPromise) {
  const el = await elOrPromise;
  await browser.execute((e) => e.click(), el);
}

describe("Golden path: Import -> Library -> Develop -> Export", () => {
  let exportDir;

  before(async () => {
    exportDir = fs.mkdtempSync(path.join(os.tmpdir(), "emulsion-e2e-"));
    // The WebDriver protocol's default script timeout (30s) is tight for
    // an `execute()` call that awaits a real backend command -- list_images
    // over an accumulated catalog, and especially export_images, which
    // does a full decode + re-encode + file write.
    await browser.setTimeout({ script: 90000 });
  });

  it("imports a fixture photo and shows it in the Library grid", async () => {
    await browser.execute(
      async (fixturePath) => window.__TAURI__.core.invoke("import_files", { paths: [fixturePath] }),
      FIXTURE_PATH,
    );
    // The app only re-reads the catalog in response to its own UI-driven
    // import flow (or on load) -- this import happened outside that flow,
    // so a reload is needed for the Library grid to pick it up.
    await browser.refresh();

    const cell = await findFixtureCell();
    await cell.waitForExist({ timeout: 20000 });
  });

  it("opens Develop, adjusts Exposure, and reflects the new value", async () => {
    // A real WebdriverIO `.doubleClick()` (two synthetic clicks) doesn't
    // reliably land inside the WebKit/WKWebView native double-click
    // timing window, so the app's `ondblclick` handler never fires even
    // though the two underlying clicks do (visible only as the cell
    // becoming selected). Dispatching a real `dblclick` DOM event directly
    // sidesteps that timing dependency entirely. This opens Library's
    // Loupe view (single-image), not Develop itself -- Loupe's own
    // "Develop →" button (LibraryImageViewer.svelte) is the actual entry
    // point into the Develop module.
    const cell = await findFixtureCell();
    await browser.execute((el) => {
      el.dispatchEvent(new MouseEvent("dblclick", { bubbles: true, cancelable: true, view: window }));
    }, cell);

    const developButton = await $(".hud-develop-btn");
    await developButton.waitForExist({ timeout: 10000 });
    await clickEl(developButton);

    const exposureInput = await $("#exposure");
    await exposureInput.waitForExist({ timeout: 20000 });

    // Keyboard-driven nudging (Home/ArrowRight on a focused range input)
    // doesn't reach the input at all in this embedded-WebDriver +
    // WKWebView combination -- clicking the element doesn't reliably hand
    // it real OS-level keyboard focus here. Setting the value via the
    // input's native property setter and dispatching a real `input` event
    // is what Svelte's own `oninput` handler listens for either way, and
    // sidesteps the focus dependency entirely.
    const before = Number(await exposureInput.getValue());
    const target = before >= 4 ? before - 2 : before + 2; // stay within [-5, 5]
    await browser.execute(
      (el, value) => {
        const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, "value").set;
        setter.call(el, String(value));
        el.dispatchEvent(new Event("input", { bubbles: true }));
      },
      exposureInput,
      target,
    );
    const after = Number(await exposureInput.getValue());

    expect(after).toBe(target);
    expect(after).not.toBe(before);
  });

  it("exports the develop image and writes a real file to disk", async () => {
    // Proves the Export button's own enablement logic (currentExportItems
    // derived from the open Develop image) via the real UI, even though
    // the destination-folder step inside the modal can't be driven (native
    // dialog -- see the file-level comment above).
    const exportButton = await $(".export-btn");
    await exportButton.waitForEnabled({ timeout: 10000 });
    await clickEl(exportButton);

    const dialog = await $(".dialog");
    await dialog.waitForExist({ timeout: 10000 });

    const versionId = await browser.execute(async (fixturePath) => {
      const images = await window.__TAURI__.core.invoke("list_images");
      const match = images.find((img) => img.path === fixturePath);
      return match ? match.version_id : null;
    }, FIXTURE_PATH);
    expect(versionId).not.toBeNull();

    const [result] = await browser.execute(
      async (fixturePath, id, destinationDir) => {
        return window.__TAURI__.core.invoke("export_images", {
          items: [{ path: fixturePath, version_id: id }],
          options: { destination_dir: destinationDir, long_edge: null, quality: 90 },
        });
      },
      FIXTURE_PATH,
      versionId,
      exportDir,
    );
    expect(result.error).toBeFalsy();

    const outputPath = path.join(exportDir, `${FIXTURE_NAME}.jpg`);
    expect(fs.existsSync(outputPath)).toBe(true);
  });
});
