import path from "node:path";
import { fileURLToPath } from "node:url";
import { findCellByName } from "../helpers.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const FIXTURE_A = path.resolve(__dirname, "../../../test_image/Field-corn-Liechtenstein-landscape.jpg");
const FIXTURE_B = path.resolve(__dirname, "../../../test_image/Red-eye-flash.jpeg");
const NAME_A = "Field-corn-Liechtenstein-landscape";
const NAME_B = "Red-eye-flash";

/**
 * No RAW bracket fixtures exist in this repo (test_image/ is JPEG-only,
 * same fixtures golden-path.e2e.js already uses) -- merge_hdr_bracket's
 * RAW-only validation means a real, successful merge can't be exercised
 * here, and is documented in RFC-0003 §4/PROGRESS.md as locally-
 * verified-only pending real bracket RAW files. This spec instead covers
 * the two things that ARE real regression risk for this UI-affecting
 * slice: the "Merge to HDR…" button's enabled state actually tracking
 * the Library selection count, and a real round-trip through the actual
 * Tauri command (not a mock) surfacing its RAW-only rejection as a
 * status message rather than silently doing nothing or throwing
 * unhandled.
 *
 * Locates the button by its own text content via `browser.execute`
 * rather than a WebdriverIO text selector -- same "dispatch/query via a
 * real DOM call" approach `clickEl`/`openDevelopFor` in helpers.js
 * already use, since direct WebdriverIO clicks are documented there as
 * intermittently unreliable in this embedded-WebDriver + WKWebView
 * combination.
 */
async function mergeButtonState() {
  return browser.execute(() => {
    const btn = Array.from(document.querySelectorAll("button")).find((b) => b.textContent.includes("Merge to HDR"));
    return btn ? { disabled: btn.disabled, text: btn.textContent.trim() } : null;
  });
}

async function clickMergeButton() {
  await browser.execute(() => {
    const btn = Array.from(document.querySelectorAll("button")).find((b) => b.textContent.includes("Merge to HDR"));
    btn.click();
  });
}

/** Dispatches a real `click` MouseEvent with the given modifier keys --
 * see +page.svelte's `handleSelect` for the Cmd/Ctrl-toggles-membership,
 * plain-click-replaces-selection semantics this drives. */
async function clickCell(/** @type {string} */ name, /** @type {{meta?: boolean}} */ opts = {}) {
  const cell = await findCellByName(name);
  await browser.execute(
    (el, meta) => {
      el.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true, view: window, metaKey: meta, ctrlKey: meta }));
    },
    cell,
    !!opts.meta,
  );
}

describe("HDR merge: Library selection gating + backend round-trip", () => {
  before(async () => {
    // list_images/merge_hdr_bracket's own RAW-decode-rejection path is
    // fast (fails before any real decode is attempted, per
    // merge_hdr_bracket's own doc comment), but the default WebDriver
    // script timeout (30s) is still tight for a real IPC round-trip.
    await browser.setTimeout({ script: 60000 });
    await browser.execute(
      async (paths) => window.__TAURI__.core.invoke("import_files", { paths }),
      [FIXTURE_A, FIXTURE_B],
    );
    await browser.refresh();
    await (await findCellByName(NAME_A)).waitForExist({ timeout: 20000 });
    await (await findCellByName(NAME_B)).waitForExist({ timeout: 20000 });
  });

  it("is disabled with 0 or 1 photos selected, enabled at 2+", async () => {
    // Fresh reload just happened in `before` -- selection starts empty.
    expect((await mergeButtonState()).disabled).toBe(true);

    await clickCell(NAME_A);
    expect((await mergeButtonState()).disabled).toBe(true);

    await clickCell(NAME_B, { meta: true });
    const twoSelected = await mergeButtonState();
    expect(twoSelected.disabled).toBe(false);
    expect(twoSelected.text).toContain("(2)");
  });

  it("surfaces the backend's real RAW-only rejection as a status message", async () => {
    await clickMergeButton();

    const status = await $(".status");
    await browser.waitUntil(async () => (await status.getText()).includes("HDR merge failed"), {
      timeout: 20000,
      timeoutMsg: "expected the status line to report the RAW-only rejection from merge_hdr_bracket",
    });
    expect(await status.getText()).toContain("not RAW");
  });
});
