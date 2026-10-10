import path from "node:path";
import { fileURLToPath } from "node:url";
import { openDevelopFor } from "../helpers.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
// A fixture no other spec uses, so the panel visibility this spec toggles never leaks into another spec's photo.
const FIXTURE_PATH = path.resolve(__dirname, "../../../test_image/Cavenagh-Bridge-Singapore-panorama-frame-a.jpg");
const FIXTURE_NAME = "Cavenagh-Bridge-Singapore-panorama-frame-a";

/**
 * RFC-0028 slice 2b: the Develop panel header and canvas menus, in the real window. Driven through the DOM like
 * context-menu.e2e.js (a native right-click cannot be sent through this WebDriver). Only the menus and the edit
 * stack's panel visibility are checked, which need no GPU.
 */
describe("Develop context menus", function () {
  this.timeout(180000);

  const menuOpen = () => browser.execute(() => !!document.querySelector('[data-testid="context-menu"]'));
  const rowIds = () =>
    browser.execute(() => Array.from(document.querySelectorAll('[data-testid="context-menu"] [data-menu-id]')).map((e) => e.getAttribute("data-menu-id")));
  const rightClick = (/** @type {string} */ selector) =>
    browser.execute((sel) => {
      const el = document.querySelector(sel);
      const box = el.getBoundingClientRect();
      el.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: box.left + 12, clientY: box.top + 8 }));
    }, selector);
  const clickRow = (/** @type {string} */ id) =>
    browser.execute((menuId) => /** @type {HTMLElement} */ (document.querySelector(`[data-menu-id="${menuId}"]`)).click(), id);
  const hoverRow = (/** @type {string} */ id) =>
    browser.execute((menuId) => document.querySelector(`[data-menu-id="${menuId}"]`).dispatchEvent(new PointerEvent("pointerenter", { bubbles: false })), id);
  const key = (/** @type {string} */ k) => browser.execute((kk) => window.dispatchEvent(new KeyboardEvent("keydown", { key: kk, bubbles: true, cancelable: true })), k);
  /** Which panels are hidden, by id (the `.panel-hidden` class on each section). */
  const hiddenPanels = () =>
    browser.execute(() =>
      Array.from(document.querySelectorAll("details.section.panel-hidden > summary[data-ctx-panel]")).map((s) => s.getAttribute("data-ctx-panel")),
    );
  const panelCount = () => browser.execute(() => document.querySelectorAll("summary[data-ctx-panel]").length);

  before(async () => {
    await browser.setTimeout({ script: 90000 });
    await browser.execute(async (p) => window.__TAURI__.core.invoke("import_files", { paths: [p] }), FIXTURE_PATH);
    await browser.refresh();
    await openDevelopFor(FIXTURE_NAME);
    await (await $("#exposure")).waitForExist({ timeout: 30000 });
  });

  after(async () => {
    // Leave the photo with every panel visible again.
    await rightClick('summary[data-ctx-panel="vignette"]');
    if (await menuOpen()) {
      await clickRow("show-all-panels");
    }
  });

  it("a panel header's menu hides it, and Show All brings every panel back", async () => {
    await rightClick('summary[data-ctx-panel="vignette"]');
    await browser.waitUntil(menuOpen, { timeout: 10000, timeoutMsg: "the panel header menu never opened" });
    expect(await rowIds()).toEqual(["toggle-panel", "reset-panel", "solo-panel", "show-all-panels"]);
    const label = await browser.execute(() => document.querySelector('[data-menu-id="toggle-panel"] .label').textContent);
    expect(label).toBe("Hide Vignette");
    await clickRow("toggle-panel");
    await browser.waitUntil(async () => (await hiddenPanels()).includes("vignette"), { timeout: 10000, timeoutMsg: "Vignette was not hidden" });
    expect(await menuOpen()).toBe(false);

    await rightClick('summary[data-ctx-panel="vignette"]');
    await browser.waitUntil(menuOpen, { timeout: 10000 });
    expect(await browser.execute(() => document.querySelector('[data-menu-id="toggle-panel"] .label').textContent)).toBe("Show Vignette");
    await clickRow("show-all-panels");
    await browser.waitUntil(async () => (await hiddenPanels()).length === 0, { timeout: 10000, timeoutMsg: "Show All did not show every panel" });
  });

  it("Solo shows only that panel", async () => {
    await rightClick('summary[data-ctx-panel="grain"]');
    await browser.waitUntil(menuOpen, { timeout: 10000 });
    await clickRow("solo-panel");
    const total = await panelCount();
    await browser.waitUntil(async () => (await hiddenPanels()).length === total - 1, { timeout: 10000, timeoutMsg: "Solo did not hide the other panels" });
    expect(await hiddenPanels()).not.toContain("grain");

    await rightClick('summary[data-ctx-panel="grain"]');
    await browser.waitUntil(menuOpen, { timeout: 10000 });
    await clickRow("show-all-panels");
    await browser.waitUntil(async () => (await hiddenPanels()).length === 0, { timeout: 10000 });
  });

  it("the canvas menu has the view toggles, a Zoom submenu and the settings rows, and Reset Settings asks first", async () => {
    await rightClick("[data-ctx-canvas]");
    await browser.waitUntil(menuOpen, { timeout: 10000, timeoutMsg: "the canvas menu never opened" });
    expect(await rowIds()).toEqual(expect.arrayContaining(["undo", "redo", "before-after", "zoom", "clipping", "mask-overlay", "copy-settings", "paste-settings", "reset-settings"]));
    await hoverRow("zoom");
    await browser.waitUntil(() => browser.execute(() => !!document.querySelector('[data-menu-id="zoom-fit"]')), { timeout: 5000, timeoutMsg: "the Zoom submenu never opened" });
    await key("Escape");
    await browser.waitUntil(async () => !(await menuOpen()), { timeout: 5000 });

    await rightClick("[data-ctx-canvas]");
    await browser.waitUntil(menuOpen, { timeout: 10000 });
    await clickRow("reset-settings");
    await browser.waitUntil(() => browser.execute(() => !!document.querySelector('[role="dialog"]')), { timeout: 5000, timeoutMsg: "no reset confirmation" });
    await browser.execute(() => {
      const cancel = Array.from(document.querySelectorAll('[role="dialog"] button')).find((b) => /cancel/i.test(b.textContent ?? ""));
      cancel.click();
    });
    await browser.waitUntil(() => browser.execute(() => !document.querySelector('[role="dialog"]')), { timeout: 5000 });
  });
});
