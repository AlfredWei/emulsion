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
  // Each test that needs a mask reopens Develop (50-75 s in a debug build), so give the suite room.
  this.timeout(360000);

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

  let versionId;
  let gpuAvailable = false;

  const getEditStack = () => browser.execute((vid) => window.__TAURI__.core.invoke("get_edit_stack", { versionId: vid }), versionId);
  /** The stack is flushed on a debounce; poll until `pred` holds. */
  async function waitForStack(/** @type {(s: any) => boolean} */ pred, /** @type {string} */ what) {
    const deadline = Date.now() + 8000;
    let last = null;
    while (Date.now() < deadline) {
      last = await getEditStack();
      if (pred(last)) return last;
      await new Promise((resolve) => setTimeout(resolve, 100));
    }
    throw new Error(`${what}: persisted stack never matched within 8s; last = ${JSON.stringify(last)}`);
  }
  const countMasks = (/** @type {any} */ s) => s.ops.filter((/** @type {any} */ o) => String(o.op).endsWith("_mask")).length;
  const domClick = async (/** @type {string} */ selector, /** @type {RegExp | null} */ text = null) => {
    const ok = await browser.execute(
      (sel, src) => {
        const re = src ? new RegExp(src, "i") : null;
        const el = Array.from(document.querySelectorAll(sel)).find((e) => !re || re.test(e.textContent ?? ""));
        if (!el) return false;
        /** @type {HTMLElement} */ (el).click();
        return true;
      },
      selector,
      text ? text.source : null,
    );
    if (!ok) throw new Error(`nothing to click for ${selector}${text ? ` / ${text}` : ""}`);
  };
  const waitForDom = async (/** @type {() => boolean} */ pred, /** @type {string} */ what) => {
    const deadline = Date.now() + 15000;
    while (Date.now() < deadline) {
      if (await browser.execute(pred)) return;
      await new Promise((resolve) => setTimeout(resolve, 150));
    }
    throw new Error(`${what}: never appeared within 15s`);
  };
  /** Starts the next test from a photo with no masks, with Develop freshly open. */
  async function openWithEmptyStack() {
    await browser.execute((vid) => window.__TAURI__.core.invoke("set_edit_stack", { versionId: vid, stack: { schema_version: 1, ops: [] } }), versionId);
    await browser.refresh();
    await openDevelopFor(FIXTURE_NAME);
    await (await $("#exposure")).waitForExist({ timeout: 30000 });
  }

  before(async () => {
    await browser.setTimeout({ script: 90000 });
    gpuAvailable = await browser.execute(async () => {
      if (!navigator.gpu) return false;
      try {
        return !!(await navigator.gpu.requestAdapter());
      } catch {
        return false;
      }
    });
    await browser.execute(async (p) => window.__TAURI__.core.invoke("import_files", { paths: [p] }), FIXTURE_PATH);
    await browser.refresh();
    versionId = await browser.execute(async (name) => {
      const images = await window.__TAURI__.core.invoke("list_images");
      return images.find((i) => i.path.replace(/\\/g, "/").endsWith(`/${name}.jpg`)).version_id;
    }, FIXTURE_NAME);
    await openDevelopFor(FIXTURE_NAME);
    await (await $("#exposure")).waitForExist({ timeout: 30000 });
  });

  after(async () => {
    // Leave the photo with every panel visible again (best effort: a failed test may leave Develop closed).
    try {
      if (await browser.execute(() => !!document.querySelector('summary[data-ctx-panel="vignette"]'))) {
        await rightClick('summary[data-ctx-panel="vignette"]');
        if (await menuOpen()) await clickRow("show-all-panels");
      }
    } catch {
      /* nothing to restore */
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

  // The mask tools are disabled while WebGPU is unavailable (develop-gpu-fallback.e2e.js), as on CI runners,
  // so the tests that need a mask skip there and run locally.
  it("the mask panel's menu inverts, hides the overlay, and Delete asks first", async function () {
    if (!gpuAvailable) this.skip();
    await openWithEmptyStack();
    await waitForDom(() => !!document.querySelector('[aria-label="Luminance Range"]'), "tool strip");
    await domClick('[aria-label="Luminance Range"]');
    await waitForDom(() => !!document.querySelector('[role="dialog"][aria-label$="adjustments"]'), "mask panel");
    const created = await waitForStack((s) => countMasks(s) === 1, "mask created");
    const maskId = created.ops.find((/** @type {any} */ o) => o.op === "luminance_range_mask").id;

    await rightClick("[data-ctx-mask]");
    await browser.waitUntil(menuOpen, { timeout: 10000, timeoutMsg: "the mask panel menu never opened" });
    expect(await rowIds()).toEqual(["invert-mask", "mask-overlay", "close-mask", "delete-mask"]);
    expect(await browser.execute(() => document.querySelector('[data-menu-id="delete-mask"] .label').textContent)).toBe("Delete Luminance Range…");

    await clickRow("invert-mask");
    await waitForStack((s) => s.ops.find((/** @type {any} */ o) => o.id === maskId)?.invert === true, "mask inverted");
    // The panel's own checkbox agrees, and the menu now shows it checked.
    expect(await browser.execute(() => Array.from(document.querySelectorAll(".invert-row input")).some((i) => /** @type {HTMLInputElement} */ (i).checked))).toBe(true);
    await rightClick("[data-ctx-mask]");
    await browser.waitUntil(menuOpen, { timeout: 10000 });
    expect(await browser.execute(() => document.querySelector('[data-menu-id="invert-mask"]').getAttribute("aria-checked"))).toBe("true");

    // Delete asks; Cancel keeps the mask, Delete removes it.
    await clickRow("delete-mask");
    await waitForDom(() => !!document.querySelector('[role="dialog"]:not([aria-label$="adjustments"])'), "delete confirmation");
    await browser.execute(() => {
      const d = document.querySelector('[role="dialog"]:not([aria-label$="adjustments"])');
      Array.from(d.querySelectorAll("button")).find((b) => /cancel/i.test(b.textContent ?? "")).click();
    });
    expect(countMasks(await getEditStack())).toBe(1);
    await rightClick("[data-ctx-mask]");
    await browser.waitUntil(menuOpen, { timeout: 10000 });
    await clickRow("delete-mask");
    await waitForDom(() => !!document.querySelector('[role="dialog"]:not([aria-label$="adjustments"])'), "delete confirmation");
    await browser.execute(() => {
      const d = document.querySelector('[role="dialog"]:not([aria-label$="adjustments"])');
      Array.from(d.querySelectorAll("button")).find((b) => /^delete$/i.test((b.textContent ?? "").trim())).click();
    });
    await waitForStack((s) => countMasks(s) === 0, "mask deleted");
  });

  it("a shape row's menu removes that shape after asking", async function () {
    if (!gpuAvailable) this.skip();
    await openWithEmptyStack();
    await waitForDom(() => !!document.querySelector('[aria-label="Luminance Range"]'), "tool strip");
    await domClick('[aria-label="Luminance Range"]');
    await waitForDom(() => !!document.querySelector('[role="dialog"][aria-label$="adjustments"]'), "mask panel");
    const created = await waitForStack((s) => countMasks(s) === 1, "mask created");
    const maskId = created.ops.find((/** @type {any} */ o) => o.op === "luminance_range_mask").id;
    await domClick(".addbtn .add");
    await domClick(".addbtn .seg button", /subtract/i);
    await domClick('.addbtn [role="menuitem"]', /luminance/i);
    await waitForStack((s) => (s.ops.find((/** @type {any} */ o) => o.id === maskId)?.modifiers ?? []).length === 1, "shape added");

    await rightClick("[data-ctx-shape]");
    await browser.waitUntil(menuOpen, { timeout: 10000, timeoutMsg: "the shape menu never opened" });
    expect(await rowIds()).toEqual(["select-shape", "remove-shape"]);
    await clickRow("remove-shape");
    await waitForDom(() => !!document.querySelector('[role="dialog"]:not([aria-label$="adjustments"])'), "remove confirmation");
    await browser.execute(() => {
      const d = document.querySelector('[role="dialog"]:not([aria-label$="adjustments"])');
      Array.from(d.querySelectorAll("button")).find((b) => /^remove$/i.test((b.textContent ?? "").trim())).click();
    });
    await waitForStack((s) => s.ops.find((/** @type {any} */ o) => o.id === maskId)?.modifiers === undefined, "shape removed");
  });

  it("a right-click does not start a mask on the canvas, while a primary-button drag does", async function () {
    if (!gpuAvailable) this.skip();
    await openWithEmptyStack();
    await waitForDom(() => !!document.querySelector('[aria-label="Radial Gradient"]'), "tool strip");
    await domClick('[aria-label="Radial Gradient"]');
    const gesture = (/** @type {number} */ button) =>
      browser.execute((b) => {
        const canvas = document.querySelector(".canvas-wrap canvas");
        const r = canvas.getBoundingClientRect();
        const at = (/** @type {number} */ fx, /** @type {number} */ fy) => ({
          bubbles: true, cancelable: true, isPrimary: true, pointerId: 1, pointerType: "mouse",
          button: b, buttons: b === 0 ? 1 : 2, clientX: r.left + r.width * fx, clientY: r.top + r.height * fy,
        });
        canvas.dispatchEvent(new PointerEvent("pointerdown", at(0.4, 0.4)));
        canvas.dispatchEvent(new PointerEvent("pointermove", { ...at(0.6, 0.6), button: -1 }));
        canvas.dispatchEvent(new PointerEvent("pointerup", at(0.6, 0.6)));
      }, button);

    await gesture(2);
    await browser.pause(1500); // longer than the edit-stack flush debounce
    expect(countMasks(await getEditStack())).toBe(0);

    await gesture(0); // the positive control: the same gesture with the primary button does create one
    await waitForStack((s) => countMasks(s) === 1, "primary-button drag created a mask (harness control)");
  });
});
