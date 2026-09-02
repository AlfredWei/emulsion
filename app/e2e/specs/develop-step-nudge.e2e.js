import path from "node:path";
import { fileURLToPath } from "node:url";
import { openDevelopFor } from "../helpers.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
// A third, distinct fixture from golden-path's and panel-resize's own, for
// the same reason both of those already document: keeps this spec's own
// edit-stack history isolated from a repeated local run's accumulated
// catalog.
const FIXTURE_PATH = path.resolve(__dirname, "../../../test_image/Red-eye-flash.jpeg");
const FIXTURE_NAME = "Red-eye-flash";

/**
 * M4.5 Slice 7: every Develop slider row grew a pair of up/down step-nudge
 * buttons (DevelopPanel.svelte's `stepButtons` snippet) for fine
 * single-step adjustment beyond drag precision. This drives the real
 * buttons via a real DOM click (not a synthetic pointer-event dispatch --
 * a plain `<button>` click is not one of this codebase's own documented
 * WebKit/embedded-WebDriver quirks, unlike the range-input keyboard-focus
 * and dblclick cases golden-path.e2e.js already works around).
 */
describe("Develop slider step-nudge buttons", () => {
  before(async () => {
    await browser.setTimeout({ script: 90000 });
    await browser.execute(
      async (fixturePath) => window.__TAURI__.core.invoke("import_files", { paths: [fixturePath] }),
      FIXTURE_PATH,
    );
    await browser.refresh();
    await openDevelopFor(FIXTURE_NAME);
    const exposureInput = await $("#exposure");
    await exposureInput.waitForExist({ timeout: 20000 });

    // Like golden-path.e2e.js's own catalog, this fixture's edit stack is
    // not reset between local runs -- a repeat run could start from
    // whatever a PRIOR run's "clamps at max" test left Exposure at (e.g.
    // already pinned to +5), which would make a plain "before + 0.05"
    // expectation wrong. Force a known, comfortably-mid-range baseline
    // before every test in this file relies on relative nudges.
    await browser.execute(() => {
      for (const [id, value] of [["exposure", "0"], ["contrast", "0"]]) {
        const el = document.getElementById(id);
        const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, "value").set;
        setter.call(el, value);
        el.dispatchEvent(new Event("input", { bubbles: true }));
      }
    });
  });

  /** Reads `#<id>`'s current numeric value via its native property (same
   * source `oninput`'s own handler reads from), not the rounded `.val`
   * display text. */
  async function sliderValue(/** @type {string} */ id) {
    return browser.execute((elId) => Number(document.getElementById(elId).value), id);
  }

  /** Clicks the increase (`dir=1`) or decrease (`dir=-1`) step button in
   * `#<id>`'s own `.row`. */
  async function clickStep(/** @type {string} */ id, /** @type {1 | -1} */ dir) {
    await browser.execute(
      (elId, label) => {
        const row = document.getElementById(elId).closest(".row");
        row.querySelector(`.step-btn[aria-label="${label}"]`).click();
      },
      id,
      dir === 1 ? "Increase" : "Decrease",
    );
  }

  it("nudges Exposure up and down by its own fractional step (0.05), without float drift", async () => {
    const before = await sliderValue("exposure");
    await clickStep("exposure", 1);
    expect(await sliderValue("exposure")).toBe(Math.round((before + 0.05) * 100) / 100);
    await clickStep("exposure", -1);
    await clickStep("exposure", -1);
    expect(await sliderValue("exposure")).toBe(Math.round((before - 0.05) * 100) / 100);
  });

  it("nudges Contrast up and down by its own whole-number step (1)", async () => {
    const before = await sliderValue("contrast");
    await clickStep("contrast", 1);
    expect(await sliderValue("contrast")).toBe(before + 1);
    await clickStep("contrast", -1);
    expect(await sliderValue("contrast")).toBe(before);
  });

  it("clamps at Exposure's max (+5) and disables the Increase button there", async () => {
    await browser.execute(() => {
      const el = document.getElementById("exposure");
      const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, "value").set;
      setter.call(el, "5");
      el.dispatchEvent(new Event("input", { bubbles: true }));
    });
    await clickStep("exposure", 1);
    expect(await sliderValue("exposure")).toBe(5);
    const disabled = await browser.execute(() => {
      const row = document.getElementById("exposure").closest(".row");
      return row.querySelector('.step-btn[aria-label="Increase"]').disabled;
    });
    expect(disabled).toBe(true);
  });
});
