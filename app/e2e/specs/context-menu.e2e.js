import path from "node:path";
import { fileURLToPath } from "node:url";
import { findCellByNameAnywhere } from "../helpers.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const FIXTURE_PATH = path.resolve(__dirname, "../../../test_image/Field-corn-Liechtenstein-landscape.jpg");
const FIXTURE_NAME = "Field-corn-Liechtenstein-landscape";

/**
 * RFC-0028 slice 1: the in-app context menu on a Library photo. CI-safe (no WebGPU, no model). Everything is
 * driven through the DOM (`browser.execute`) like the other specs: a real native right-click cannot be sent
 * through this WebDriver, so a `contextmenu` event is dispatched on the cell, which is what the app listens
 * for. The real gesture stays a manual check.
 */
describe("Library photo context menu", function () {
  this.timeout(180000);
  let versionId;

  const menuOpen = () => browser.execute(() => !!document.querySelector('[data-testid="context-menu"]'));
  const rowIds = () =>
    browser.execute(() => Array.from(document.querySelectorAll('[data-testid="context-menu"] [data-menu-id]')).map((e) => e.getAttribute("data-menu-id")));
  const cellState = () =>
    browser.execute(
      async (vid) => {
        const images = await window.__TAURI__.core.invoke("list_images");
        const i = images.find((x) => x.version_id === vid);
        return { rating: i.rating, flag: i.flag };
      },
      versionId,
    );

  /** Dispatches a right-click on the fixture's cell. */
  const rightClickCell = () =>
    browser.execute((name) => {
      const label = Array.from(document.querySelectorAll(".file-name")).find((el) => el.textContent?.includes(name));
      const cell = label.closest("[data-ctx-photo]");
      const box = cell.getBoundingClientRect();
      cell.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: box.left + 20, clientY: box.top + 20 }));
    }, FIXTURE_NAME);

  /** Hovers a row (opening its submenu) the way the pointer does. */
  const hoverRow = (/** @type {string} */ id) =>
    browser.execute((menuId) => {
      const row = document.querySelector(`[data-menu-id="${menuId}"]`);
      row.dispatchEvent(new PointerEvent("pointerenter", { bubbles: false }));
    }, id);
  const clickRow = (/** @type {string} */ id) =>
    browser.execute((menuId) => /** @type {HTMLElement} */ (document.querySelector(`[data-menu-id="${menuId}"]`)).click(), id);
  const key = (/** @type {string} */ k, /** @type {object} */ extra = {}) =>
    browser.execute(
      (kk, ex) => window.dispatchEvent(new KeyboardEvent("keydown", { key: kk, bubbles: true, cancelable: true, ...ex })),
      k,
      extra,
    );

  const pageState = () =>
    browser.execute(() => ({
      href: location.href,
      hasGrid: !!document.querySelector("[data-ctx-grid]"),
      cells: document.querySelectorAll("[data-ctx-photo]").length,
      names: Array.from(document.querySelectorAll(".file-name")).map((e) => e.textContent).slice(0, 5),
      dialog: !!document.querySelector('[role="dialog"]'),
      body: document.body.innerText.replace(/\s+/g, " ").slice(0, 300),
    }));
  afterEach(async function () {
    if (this.currentTest?.state === "failed") console.log("PAGE-STATE after failure:", JSON.stringify(await pageState().catch((e) => String(e))));
  });

  before(async () => {
    await browser.setTimeout({ script: 90000 });
    await browser.execute(async (p) => window.__TAURI__.core.invoke("import_files", { paths: [p] }), FIXTURE_PATH);
    await browser.refresh();
    await findCellByNameAnywhere(FIXTURE_NAME, { timeout: 60000 });
    versionId = await browser.execute(async (name) => {
      const images = await window.__TAURI__.core.invoke("list_images");
      return images.find((i) => i.path.replace(/\\/g, "/").endsWith(`/${name}.jpg`)).version_id; // Windows paths use backslashes
    }, FIXTURE_NAME);
    // A known starting point (the shared dev catalog keeps whatever earlier runs left).
    await browser.execute(async (vid) => {
      await window.__TAURI__.core.invoke("set_rating", { versionId: vid, rating: 0 });
      await window.__TAURI__.core.invoke("set_flag", { versionId: vid, flag: "none" });
    }, versionId);
    await browser.refresh();
    await findCellByNameAnywhere(FIXTURE_NAME, { timeout: 60000 });
  });

  it("right-click on a photo opens the photo menu with the common operations", async () => {
    await rightClickCell();
    await browser.waitUntil(menuOpen, { timeout: 10000, timeoutMsg: "the context menu never opened" });
    const ids = await rowIds();
    expect(ids).toEqual(
      expect.arrayContaining(["open-develop", "open-loupe", "rating", "flag", "color-label", "add-to-collection", "develop-settings", "export", "reveal", "copy-path", "detect-faces", "remove-from-catalog"]),
    );
    // The webview's own menu is suppressed on the photo: the event was cancelled by the app.
    const prevented = await browser.execute(() => {
      const cell = document.querySelector("[data-ctx-photo]");
      const ev = new MouseEvent("contextmenu", { bubbles: true, cancelable: true });
      cell.dispatchEvent(ev);
      return ev.defaultPrevented;
    });
    expect(prevented).toBe(true);
    await key("Escape");
    await browser.waitUntil(async () => !(await menuOpen()), { timeout: 5000 });
  });

  it("Rating > 4 stars sets the rating of the right-clicked photo", async () => {
    await rightClickCell();
    await browser.waitUntil(menuOpen, { timeout: 10000 });
    await hoverRow("rating");
    await browser.waitUntil(() => browser.execute(() => !!document.querySelector('[data-menu-id="rating-4"]')), { timeout: 5000, timeoutMsg: "the Rating submenu never opened" });
    await clickRow("rating-4");
    await browser.waitUntil(async () => (await cellState()).rating === 4, { timeout: 10000, timeoutMsg: "rating was not saved" });
    expect(await menuOpen()).toBe(false);
  });

  it("the current rating is checked the next time the menu opens", async () => {
    await rightClickCell();
    await browser.waitUntil(menuOpen, { timeout: 10000 });
    await hoverRow("rating");
    await browser.waitUntil(() => browser.execute(() => !!document.querySelector('[data-menu-id="rating-4"]')), { timeout: 5000 });
    const checked = await browser.execute(() => document.querySelector('[data-menu-id="rating-4"]').getAttribute("aria-checked"));
    expect(checked).toBe("true");
    await key("Escape");
  });

  it("keyboard: Shift+F10 on the focused photo opens the menu, arrows reach Flag > Pick, Enter applies it, focus returns to the photo", async () => {
    await browser.execute((name) => {
      const label = Array.from(document.querySelectorAll(".file-name")).find((el) => el.textContent?.includes(name));
      label.closest("[data-ctx-photo]").focus();
    }, FIXTURE_NAME);
    const trace = [];
    const step = async (label, fn) => {
      await fn();
      trace.push(`${label}:${await browser.execute(() => `${document.querySelector("[data-ctx-grid]") ? "grid" : "NO-GRID"}/${document.querySelector('[data-testid="context-menu"]') ? "menu" : "no-menu"}`)}`);
    };
    await step("F10", () => key("F10", { shiftKey: true }));
    await browser.waitUntil(menuOpen, { timeout: 10000, timeoutMsg: "Shift+F10 did not open the menu" });
    // Highlight starts on the first row; walk down to Flag with the keys (Open in Develop, Open in Loupe, Rating, Flag).
    for (let i = 0; i < 3; i++) await step(`Down${i}`, () => key("ArrowDown"));
    const onFlag = await browser.execute(() => document.querySelector('[data-testid="context-menu"] .hl')?.getAttribute("data-menu-id"));
    expect(onFlag).toBe("flag");
    await step("Right", () => key("ArrowRight"));
    await step("Enter", () => key("Enter")); // the submenu's first row: Pick
    console.log("KEY-TRACE:", trace.join(" "));
    await browser.waitUntil(async () => (await cellState()).flag === "pick", { timeout: 10000, timeoutMsg: "flag was not saved" });
    expect(await menuOpen()).toBe(false);
    await browser
      .waitUntil(() => browser.execute(() => !!document.activeElement?.closest("[data-ctx-photo]")), { timeout: 3000 })
      .catch(() => {});
    const snapshot = await browser.execute(() => ({
      focusedIsPhoto: !!document.activeElement?.closest("[data-ctx-photo]"),
      active: document.activeElement?.tagName + "." + document.activeElement?.className,
      hasGrid: !!document.querySelector("[data-ctx-grid]"),
      cells: document.querySelectorAll("[data-ctx-photo]").length,
      body: document.body.innerText.slice(0, 200),
    }));
    console.log("PAGE-STATE after keyboard test:", JSON.stringify(await pageState()));
    expect(JSON.stringify(snapshot)).toContain('"focusedIsPhoto":true');
  });

  it("a letter typed while the menu is open does not reach the page's shortcuts", async () => {
    await rightClickCell();
    await browser.waitUntil(menuOpen, { timeout: 10000 });
    await key("x"); // x = Reject while no menu is open
    await browser.pause(400);
    expect((await cellState()).flag).toBe("pick");
    await key("Escape");
  });

  it("the grid's empty area has its own menu, and a click elsewhere closes the menu", async () => {
    await browser.execute(() => {
      const grid = document.querySelector("[data-ctx-grid]");
      grid.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 5, clientY: 5 }));
    });
    await browser.waitUntil(menuOpen, { timeout: 10000, timeoutMsg: "the empty-area menu never opened" });
    const opened = await rowIds();
    expect(opened).toEqual(["select-all", "deselect-all", "import-folder", "import-files"]);
    await browser.execute(() => document.body.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true })));
    await browser.waitUntil(async () => !(await menuOpen()), { timeout: 5000, timeoutMsg: "an outside click did not close the menu" });
  });

  it("Remove from Catalog hands over to the existing confirmation dialog (and Cancel keeps the photo)", async () => {
    await rightClickCell();
    await browser.waitUntil(menuOpen, { timeout: 10000 });
    await clickRow("remove-from-catalog");
    await browser.waitUntil(() => browser.execute(() => !!document.querySelector('[role="dialog"]')), { timeout: 5000, timeoutMsg: "no confirmation dialog" });
    await browser.execute(() => {
      const cancel = Array.from(document.querySelectorAll('[role="dialog"] button')).find((b) => /cancel/i.test(b.textContent ?? ""));
      cancel.click();
    });
    const still = await browser.execute(async (vid) => (await window.__TAURI__.core.invoke("list_images")).some((i) => i.version_id === vid), versionId);
    expect(still).toBe(true);
  });
});
