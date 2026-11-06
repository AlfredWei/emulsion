import path from "node:path";
import { fileURLToPath } from "node:url";
import { openDevelopFor } from "../helpers.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const FIXTURE_PATH = path.resolve(__dirname, "../../../test_image/Smiling-woman-pink-shirt-portrait.jpg");
const FIXTURE_NAME = "Smiling-woman-pink-shirt-portrait";
const BASELINE = { schema_version: 1, ops: [{ op: "contrast", value: 7 }, { op: "exposure", value: 0.5 }] };

/**
 * The History list stays linear when an edit returns to content an earlier row already holds: edit to A, edit back to
 * the starting value, click "Original" in the History panel, edit to A again. The list must be `Original, A`, not
 * `Original, A, <back>, A` (the cursor used to be found by content, which matched the wrong row). Needs no GPU.
 */
describe("Develop: History stays linear when an edit revisits earlier content", function () {
  this.timeout(120000);
  const invoke = (/** @type {string} */ cmd, /** @type {any} */ args) => browser.execute((c, a) => window.__TAURI__.core.invoke(c, a), cmd, args);
  const exposureOf = (/** @type {any} */ s) => s.ops.find((/** @type {any} */ o) => o.op === "exposure")?.value;
  const labels = async () => (await invoke("get_history", { versionId })).map((/** @type {any} */ r) => r.label);
  let versionId = 0;

  async function waitFor(/** @type {() => Promise<any>} */ fn, /** @type {string} */ what, ms = 20000) {
    const deadline = Date.now() + ms;
    let last;
    while (Date.now() < deadline) {
      last = await fn();
      if (last) return last;
      await new Promise((r) => setTimeout(r, 150));
    }
    throw new Error(`${what}: not within ${ms} ms; last = ${JSON.stringify(last)}`);
  }
  async function setExposure(/** @type {string} */ v) {
    await browser.execute((val) => {
      const el = /** @type {HTMLInputElement} */ (document.getElementById("exposure"));
      const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, "value")?.set;
      setter?.call(el, val);
      el.dispatchEvent(new Event("input", { bubbles: true }));
    }, v);
    await waitFor(async () => exposureOf(await invoke("get_edit_stack", { versionId })) === Number(v), `exposure ${v} to be stored`);
  }

  before(async () => {
    await browser.setTimeout({ script: 90000 });
    await invoke("import_files", { paths: [FIXTURE_PATH] });
    const images = await invoke("list_images", {});
    versionId = images.find((/** @type {any} */ i) => i.path === FIXTURE_PATH).version_id;
    await invoke("set_edit_stack", { versionId, stack: BASELINE, label: null });
    await browser.refresh();
    await openDevelopFor(FIXTURE_NAME);
    await (await $("#exposure")).waitForExist({ timeout: 20000 });
  });

  it("edit to A, edit back, click Original, edit to A: the list is Original, Exposure", async () => {
    await setExposure("0.9");
    await waitFor(async () => (await labels()).length === 2, "the first two history rows");
    await setExposure("0.5");
    await waitFor(async () => (await labels()).length === 3, "the edit back to be recorded");
    // The precondition for the old bug: the live stack is exactly the starting content again.
    expect(await invoke("get_edit_stack", { versionId })).toEqual(BASELINE);

    // Click the "Original" row in the History panel, as a user does.
    await waitFor(
      () => browser.execute(() => Array.from(document.querySelectorAll(".entry-label")).map((e) => e.textContent?.trim()).length === 3),
      "the History panel to list three rows",
    );
    // `click()` inside the page: a returned DOM element does not survive `browser.execute` in this embedding.
    const clicked = await browser.execute(() => {
      const row = /** @type {HTMLElement | undefined} */ (
        Array.from(document.querySelectorAll(".entry")).find((e) => e.querySelector(".entry-label")?.textContent?.trim() === "Original")
      );
      row?.click();
      return !!row;
    });
    expect(clicked).toBe(true);
    await waitFor(async () => exposureOf(await invoke("get_edit_stack", { versionId })) === 0.5, "Original to be restored");

    await setExposure("0.9");
    await waitFor(async () => (await labels()).length <= 2, `the abandoned rows to be dropped (history: ${JSON.stringify(await labels())})`);
    expect(await labels()).toEqual(["Original", "Exposure"]);
    const shown = await browser.execute(() => Array.from(document.querySelectorAll(".entry-label")).map((e) => e.textContent?.trim()));
    expect(shown).toEqual(["Original", "Exposure"]);
  });
});
