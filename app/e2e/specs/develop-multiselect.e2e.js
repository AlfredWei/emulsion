import path from "node:path";
import { fileURLToPath } from "node:url";
import { openDevelopFor } from "../helpers.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
// Two fixtures: the source is also open in develop-context-menu.e2e.js (which only toggles panel visibility and
// resets its stack), the target is used by no other spec. Both stacks are restored in `after`.
const SOURCE_PATH = path.resolve(__dirname, "../../../test_image/Cavenagh-Bridge-Singapore-panorama-frame-a.jpg");
const SOURCE_NAME = "Cavenagh-Bridge-Singapore-panorama-frame-a";
const TARGET_PATH = path.resolve(__dirname, "../../../test_image/Cavenagh-Bridge-Singapore-panorama-frame-b.jpg");
const TARGET_NAME = "Cavenagh-Bridge-Singapore-panorama-frame-b";

/**
 * RFC-0028 slice 3: Develop multi-select and Sync Settings, in the real window. The gestures are dispatched as DOM
 * events (a native Cmd-click cannot be sent through this WebDriver). Nothing here needs the GPU: it checks the
 * filmstrip's selection state, the footer button, the group dialog and what lands in the catalog.
 */
describe("Develop multi-select and Sync Settings", function () {
  this.timeout(360000);

  let sourceId;
  let targetId;

  const stackOf = (/** @type {number} */ versionId) =>
    browser.execute((vid) => window.__TAURI__.core.invoke("get_edit_stack", { versionId: vid }), versionId);
  const historyOf = (/** @type {number} */ versionId) =>
    browser.execute((vid) => window.__TAURI__.core.invoke("get_history", { versionId: vid }), versionId);
  const setStack = (/** @type {number} */ versionId, /** @type {any} */ stack) =>
    browser.execute((vid, s) => window.__TAURI__.core.invoke("set_edit_stack", { versionId: vid, stack: s, label: null }), versionId, stack);
  const exposureOf = (/** @type {any} */ s) => s.ops.find((/** @type {any} */ o) => o.op === "exposure")?.value;
  /** Click on a filmstrip cell with the given modifiers (Cmd and Ctrl both, so the same spec runs on every platform). */
  const clickCell = (/** @type {number} */ versionId, /** @type {object} */ mods = {}) =>
    browser.execute(
      (vid, m) =>
        document
          .querySelector(`.filmstrip [data-ctx-photo="${vid}"]`)
          .dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true, view: window, ...m })),
      versionId,
      mods,
    );
  /** The filmstrip's frames: which cells are selected, and which of those is the active (accent) one. */
  const frames = () =>
    browser.execute(() => ({
      selected: Array.from(document.querySelectorAll(".filmstrip .cell.selected")).map((c) => Number(c.getAttribute("data-ctx-photo"))).sort((a, b) => a - b),
      neutral: Array.from(document.querySelectorAll(".filmstrip .cell.selected.secondary")).map((c) => Number(c.getAttribute("data-ctx-photo"))),
    }));
  const waitForFrames = (/** @type {(f: any) => boolean} */ pred, /** @type {string} */ what) =>
    browser.waitUntil(async () => pred(await frames()), { timeout: 20000, timeoutMsg: `${what}: the filmstrip never matched` });
  const syncButton = () => browser.execute(() => !!Array.from(document.querySelectorAll(".panel-footer button")).find((b) => /Sync Settings/.test(b.textContent ?? "")));
  const menuOpen = () => browser.execute(() => !!document.querySelector('[data-testid="context-menu"]'));
  const dismissMenu = () => browser.execute(() => window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true })));

  async function ensureOnlyActive(/** @type {number} */ activeId) {
    await clickCell(activeId);
    await waitForFrames((f) => f.selected.length === 1 && f.selected[0] === activeId, "collapse to the active photo");
  }

  before(async () => {
    await browser.setTimeout({ script: 90000 });
    await browser.execute(async (paths) => window.__TAURI__.core.invoke("import_files", { paths }), [SOURCE_PATH, TARGET_PATH]);
    await browser.refresh();
    [sourceId, targetId] = await browser.execute(
      async (names) => {
        const images = await window.__TAURI__.core.invoke("list_images");
        return names.map((n) => images.find((i) => i.path.replace(/\\/g, "/").endsWith(`/${n}.jpg`)).version_id);
      },
      [SOURCE_NAME, TARGET_NAME],
    );
    await setStack(sourceId, { schema_version: 1, ops: [{ op: "exposure", value: 1.5 }] });
    await setStack(targetId, { schema_version: 1, ops: [{ op: "contrast", value: 20 }] });
    await browser.refresh();
    await openDevelopFor(TARGET_NAME);
    await (await $("#exposure")).waitForExist({ timeout: 30000 });
  });

  after(async () => {
    try {
      await setStack(sourceId, { schema_version: 1, ops: [] });
      await setStack(targetId, { schema_version: 1, ops: [] });
    } catch {
      /* best effort */
    }
  });

  it("Cmd/Ctrl-click adds a photo and makes it active; the active frame differs from the other selected one", async () => {
    await waitForFrames((f) => f.selected.length === 1 && f.selected[0] === targetId, "start from the open photo alone");
    expect(await syncButton()).toBe(false);

    await clickCell(sourceId, { metaKey: true, ctrlKey: true });
    await waitForFrames((f) => f.selected.length === 2, "two selected");
    expect((await frames()).neutral).toEqual([targetId]); // the source is now active (accent), the target neutral
    await browser.waitUntil(async () => syncButton(), { timeout: 10000, timeoutMsg: "the footer never offered Sync Settings" });
    // The canvas follows the active photo.
    await (await $("#exposure")).waitForExist({ timeout: 30000 });
  });

  it("the right-click menu on a selected photo acts on the selection and offers Sync Settings", async () => {
    await browser.execute((vid) => {
      const el = document.querySelector(`.filmstrip [data-ctx-photo="${vid}"]`);
      const box = el.getBoundingClientRect();
      el.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: box.left + 10, clientY: box.top - 4 }));
    }, targetId);
    await browser.waitUntil(menuOpen, { timeout: 10000, timeoutMsg: "the menu never opened" });
    expect((await frames()).selected.length).toBe(2); // right-click inside the selection leaves it alone
    const header = await browser.execute(() => document.querySelector('[data-testid="context-menu"] [data-menu-header], [data-testid="context-menu"] .header')?.textContent ?? "");
    expect(header).toMatch(/2 photos/);
    await browser.execute(() => document.querySelector('[data-menu-id="develop-settings"]').dispatchEvent(new PointerEvent("pointerenter", { bubbles: false })));
    await browser.waitUntil(() => browser.execute(() => !!document.querySelector('[data-menu-id="sync-settings"]')), { timeout: 10000, timeoutMsg: "no Sync Settings row" });
    await dismissMenu();
    await browser.waitUntil(async () => !(await menuOpen()), { timeout: 10000 });
  });

  it("Sync Settings applies the chosen groups of the active photo to the other selected one, as one 'Sync Settings' entry", async () => {
    const targetBefore = await historyOf(targetId);
    await browser.execute(() => {
      const b = Array.from(document.querySelectorAll(".panel-footer button")).find((x) => /Sync Settings/.test(x.textContent ?? ""));
      /** @type {HTMLElement} */ (b).click();
    });
    await browser.waitUntil(() => browser.execute(() => !!document.querySelector('[role="dialog"][aria-label="Sync Settings"]')), {
      timeout: 10000,
      timeoutMsg: "the Sync Settings dialog never opened",
    });
    // Everything is ticked by default; confirm.
    await browser.execute(() => {
      const b = Array.from(document.querySelectorAll('[role="dialog"][aria-label="Sync Settings"] button.primary'))[0];
      /** @type {HTMLElement} */ (b).click();
    });
    await browser.waitUntil(
      async () => exposureOf(await stackOf(targetId)) === 1.5,
      { timeout: 20000, timeoutMsg: "the target never received the source's exposure" },
    );
    const target = await stackOf(targetId);
    expect(target.ops.find((/** @type {any} */ o) => o.op === "contrast")?.value).toBe(20); // its own other ops stay (merge, not replace)
    expect(exposureOf(await stackOf(sourceId))).toBe(1.5); // the source is untouched
    const targetAfter = await historyOf(targetId);
    // Exactly one new entry (the catalog may drop a stale redo branch, so compare ids rather than lengths).
    const newest = targetAfter[targetAfter.length - 1];
    expect(newest.label).toBe("Sync Settings");
    expect(newest.id).toBeGreaterThan(targetBefore.length ? targetBefore[targetBefore.length - 1].id : 0);
    expect(targetAfter.filter((/** @type {any} */ h) => h.id > (targetBefore.length ? targetBefore[targetBefore.length - 1].id : 0)).length).toBe(1);
  });

  it("Cmd/Ctrl-A selects the whole filmstrip and a plain click on the active photo collapses it", async () => {
    await browser.execute(() =>
      window.dispatchEvent(new KeyboardEvent("keydown", { key: "a", metaKey: true, ctrlKey: true, bubbles: true, cancelable: true })),
    );
    const total = await browser.execute(() => document.querySelectorAll(".filmstrip .cell").length);
    await waitForFrames((f) => f.selected.length === total, "select all");
    await ensureOnlyActive(sourceId);
    expect(await syncButton()).toBe(false);
  });

  it("an arrow key leaves a single selection", async () => {
    await clickCell(targetId, { metaKey: true, ctrlKey: true });
    await waitForFrames((f) => f.selected.length === 2, "two selected again");
    // An arrow with no neighbour does nothing (as in Library), so step towards the side that has one.
    const arrow = await browser.execute((vid) => {
      const cells = Array.from(document.querySelectorAll(".filmstrip .cell"));
      const at = cells.findIndex((c) => c.getAttribute("data-ctx-photo") === String(vid));
      return at > 0 ? "ArrowLeft" : "ArrowRight";
    }, targetId);
    await browser.execute((k) => window.dispatchEvent(new KeyboardEvent("keydown", { key: k, bubbles: true, cancelable: true })), arrow);
    await waitForFrames((f) => f.selected.length === 1, "an arrow key collapses the selection");
  });
});
