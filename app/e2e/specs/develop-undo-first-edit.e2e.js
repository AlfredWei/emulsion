import path from "node:path";
import { fileURLToPath } from "node:url";
import { openDevelopFor } from "../helpers.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const FIXTURE_PATH = path.resolve(__dirname, "../../../test_image/Smiling-woman-pink-shirt-portrait.jpg");
const FIXTURE_NAME = "Smiling-woman-pink-shirt-portrait";
// A stack no earlier run left a history row for, so the first edit below is "the first edit": the live stack matches no row.
const BASELINE = { schema_version: 1, ops: [{ op: "contrast", value: 7 }, { op: "exposure", value: 0.1234 }] };

/**
 * The first edit made to a photograph can be undone: the write that records it also records the stack it started from
 * as an "Original" row (before, Cmd+Z did nothing until a second edit existed). Needs no GPU.
 */
describe("Develop: the first edit can be undone", function () {
  this.timeout(120000);
  const invoke = (/** @type {string} */ cmd, /** @type {any} */ args) => browser.execute((c, a) => window.__TAURI__.core.invoke(c, a), cmd, args);
  const key = (/** @type {string} */ k, /** @type {any} */ mods = {}) =>
    browser.execute((kk, m) => window.dispatchEvent(new KeyboardEvent("keydown", { key: kk, bubbles: true, cancelable: true, ...m })), k, mods);
  const exposureOf = (/** @type {any} */ s) => s.ops.find((/** @type {any} */ o) => o.op === "exposure")?.value;
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

  it("the first edit records an Original row, and Cmd+Z / Cmd+Shift+Z go back and forth across it", async () => {
    await browser.execute(() => {
      const el = /** @type {HTMLInputElement} */ (document.getElementById("exposure"));
      const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, "value")?.set;
      setter?.call(el, "0.9");
      el.dispatchEvent(new Event("input", { bubbles: true }));
    });
    await waitFor(async () => exposureOf(await invoke("get_edit_stack", { versionId })) === 0.9, "the edit to be stored");
    const rows = await waitFor(async () => {
      const h = await invoke("get_history", { versionId });
      return h.length >= 2 ? h : null;
    }, "the history rows");
    expect(rows.map((/** @type {any} */ r) => r.label)).toEqual(["Original", "Exposure"]);
    // The History panel lists it.
    const labels = await browser.execute(() => Array.from(document.querySelectorAll(".entry-label")).map((e) => e.textContent?.trim()));
    expect(labels).toContain("Original");

    await key("z", { metaKey: true, ctrlKey: true });
    await waitFor(async () => exposureOf(await invoke("get_edit_stack", { versionId })) === 0.1234, "the undo to restore the starting stack");
    expect(await invoke("get_edit_stack", { versionId })).toEqual(BASELINE);

    await key("z", { metaKey: true, ctrlKey: true, shiftKey: true });
    await waitFor(async () => exposureOf(await invoke("get_edit_stack", { versionId })) === 0.9, "the redo to bring the edit back");
  });
});
