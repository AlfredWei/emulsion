import path from "node:path";
import { fileURLToPath } from "node:url";
import { openDevelopFor } from "../helpers.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const testImage = (/** @type {string} */ f) => path.resolve(__dirname, "../../../test_image", f);
const SOURCE = { file: "Cavenagh-Bridge-Singapore-panorama-frame-a.jpg", name: "Cavenagh-Bridge-Singapore-panorama-frame-a" };
const TARGET = { file: "Cavenagh-Bridge-Singapore-panorama-frame-b.jpg", name: "Cavenagh-Bridge-Singapore-panorama-frame-b" };
// Never selected: proves an edit does not reach a photo outside the selection.
const OUTSIDER = { file: "Field-corn-Liechtenstein-landscape.jpg", name: "Field-corn-Liechtenstein-landscape" };

/**
 * RFC-0028 slice 4: Auto Sync in the real window. The gestures are dispatched as DOM events (as in
 * develop-multiselect.e2e.js); the slider is moved the way develop-cpu-gpu-parity.e2e.js does. Nothing here needs the
 * GPU: it checks the switch, the titlebar chip and what lands in the catalog.
 */
describe("Develop Auto Sync", function () {
  this.timeout(360000);

  const ids = /** @type {Record<string, number>} */ ({});

  const stackOf = (/** @type {number} */ versionId) =>
    browser.execute((vid) => window.__TAURI__.core.invoke("get_edit_stack", { versionId: vid }), versionId);
  const historyOf = (/** @type {number} */ versionId) =>
    browser.execute((vid) => window.__TAURI__.core.invoke("get_history", { versionId: vid }), versionId);
  const setStack = (/** @type {number} */ versionId, /** @type {any} */ stack) =>
    browser.execute((vid, s) => window.__TAURI__.core.invoke("set_edit_stack", { versionId: vid, stack: s, label: null }), versionId, stack);
  const opOf = (/** @type {any} */ s, /** @type {string} */ name) => s.ops.find((/** @type {any} */ o) => o.op === name)?.value;
  const clickCell = (/** @type {number} */ versionId, /** @type {object} */ mods = {}) =>
    browser.execute(
      (vid, m) =>
        document
          .querySelector(`.filmstrip [data-ctx-photo="${vid}"]`)
          .dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true, view: window, ...m })),
      versionId,
      mods,
    );
  const selectedCount = () => browser.execute(() => document.querySelectorAll(".filmstrip .cell.selected").length);
  const waitForSelected = (/** @type {number} */ n) =>
    browser.waitUntil(async () => (await selectedCount()) === n, { timeout: 20000, timeoutMsg: `the filmstrip never showed ${n} selected` });
  const switchState = () =>
    browser.execute(() => {
      const el = document.querySelector(".auto-sync-switch");
      return el ? { on: el.getAttribute("aria-checked") === "true", disabled: /** @type {HTMLButtonElement} */ (el).disabled } : null;
    });
  const chipText = () => browser.execute(() => document.querySelector(".auto-sync-chip")?.textContent?.trim() ?? null);
  const clickSwitch = () => browser.execute(() => /** @type {HTMLElement} */ (document.querySelector(".auto-sync-switch")).click());
  const moveExposure = (/** @type {number} */ value) =>
    browser.execute((v) => {
      const el = document.getElementById("exposure");
      const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, "value").set;
      setter.call(el, String(v));
      el.dispatchEvent(new Event("input", { bubbles: true }));
    }, value);

  before(async () => {
    await browser.setTimeout({ script: 90000 });
    await browser.execute(async (paths) => window.__TAURI__.core.invoke("import_files", { paths }), [SOURCE, TARGET, OUTSIDER].map((p) => testImage(p.file)));
    await browser.refresh();
    const found = await browser.execute(
      async (names) => {
        const images = await window.__TAURI__.core.invoke("list_images");
        return names.map((n) => images.find((i) => i.path.replace(/\\/g, "/").includes(`/${n}.`)).version_id);
      },
      [SOURCE.name, TARGET.name, OUTSIDER.name],
    );
    [ids.source, ids.target, ids.outsider] = found;
    await setStack(ids.source, { schema_version: 1, ops: [] });
    await setStack(ids.target, { schema_version: 1, ops: [{ op: "contrast", value: 20 }, { op: "crop", x: 0.1, y: 0.1, width: 0.5, height: 0.5, angle: 0 }] });
    await setStack(ids.outsider, { schema_version: 1, ops: [{ op: "contrast", value: 5 }] });
    await browser.refresh();
    await openDevelopFor(TARGET.name);
    await (await $("#exposure")).waitForExist({ timeout: 30000 });
  });

  after(async () => {
    try {
      for (const id of Object.values(ids)) await setStack(id, { schema_version: 1, ops: [] });
    } catch {
      /* best effort */
    }
  });

  it("the switch is inert with one photo selected and no chip is shown", async () => {
    await waitForSelected(1);
    expect(await switchState()).toEqual({ on: false, disabled: true });
    expect(await chipText()).toBeNull();
  });

  it("with two photos selected the switch turns on and the titlebar shows 'Auto Sync: 2 photos'", async () => {
    await clickCell(ids.source, { metaKey: true, ctrlKey: true });
    await waitForSelected(2);
    await browser.waitUntil(async () => (await switchState())?.disabled === false, { timeout: 10000, timeoutMsg: "the switch stayed inert" });
    await clickSwitch();
    await browser.waitUntil(async () => (await chipText()) === "Auto Sync: 2 photos", { timeout: 10000, timeoutMsg: "no titlebar chip" });
    expect((await switchState())?.on).toBe(true);
  });

  it("an exposure edit of the active photo reaches the other selected photo only: its own ops stay, an outsider is untouched", async () => {
    const before = await historyOf(ids.target);
    await moveExposure(1.25);
    await browser.waitUntil(async () => opOf(await stackOf(ids.target), "exposure") === 1.25, {
      timeout: 20000,
      timeoutMsg: "the other selected photo never received the exposure",
    });
    const target = await stackOf(ids.target);
    expect(opOf(target, "contrast")).toBe(20); // its own other ops stay
    expect(target.ops.some((/** @type {any} */ o) => o.op === "crop")).toBe(true); // per-photo ops are never touched
    expect(opOf(await stackOf(ids.source), "exposure")).toBe(1.25); // the open photo's own edit is unaffected
    expect(opOf(await stackOf(ids.outsider), "exposure")).toBeUndefined();
    const after = await historyOf(ids.target);
    // The first edit a photograph gets also records an "Original" row (what its undo goes back to).
    const added = after.filter((/** @type {any} */ h) => h.label !== "Original" && h.id > (before.length ? before[before.length - 1].id : 0));
    expect(added.map((/** @type {any} */ h) => h.label)).toEqual(["Auto Sync: Exposure"]);
  });

  it("a second edit syncs only the value that changed", async () => {
    await setStack(ids.target, { schema_version: 1, ops: [{ op: "exposure", value: 1.25 }, { op: "contrast", value: 33 }] });
    await moveExposure(-0.5);
    await browser.waitUntil(async () => opOf(await stackOf(ids.target), "exposure") === -0.5, { timeout: 20000, timeoutMsg: "the second edit never synced" });
    expect(opOf(await stackOf(ids.target), "contrast")).toBe(33); // the source's own contrast (none) was not copied
  });

  it("the chip turns it off, and an edit afterwards stays on the open photo", async () => {
    await browser.execute(() => /** @type {HTMLElement} */ (document.querySelector(".auto-sync-chip")).click());
    await browser.waitUntil(async () => (await chipText()) === null, { timeout: 10000, timeoutMsg: "the chip stayed" });
    expect((await switchState())?.on).toBe(false);
    await moveExposure(0.75);
    await browser.waitUntil(async () => opOf(await stackOf(ids.source), "exposure") === 0.75, { timeout: 20000 });
    await browser.pause(1000);
    expect(opOf(await stackOf(ids.target), "exposure")).toBe(-0.5);
  });

  it("it switches itself off when the selection drops to one photo", async () => {
    await clickSwitch();
    await browser.waitUntil(async () => (await chipText()) === "Auto Sync: 2 photos", { timeout: 10000 });
    await clickCell(ids.source); // a plain click collapses the selection
    await waitForSelected(1);
    await browser.waitUntil(async () => (await chipText()) === null, { timeout: 10000, timeoutMsg: "the chip stayed after the selection collapsed" });
    expect(await switchState()).toEqual({ on: false, disabled: true });
  });

  it("keyboard: the switch takes focus, and the page leaves Space alone so the button can activate (no pan, no preventDefault)", async () => {
    await clickCell(ids.target, { metaKey: true, ctrlKey: true });
    await waitForSelected(2);
    await browser.waitUntil(async () => (await switchState())?.disabled === false, { timeout: 10000 });
    await browser.execute(() => {
      // Registered after the app's own window listener, so it sees whether that one cancelled the key.
      window.__spaceDown = null;
      window.addEventListener("keydown", (e) => {
        if (e.key === " ") window.__spaceDown = { prevented: e.defaultPrevented };
      });
      /** @type {HTMLElement} */ (document.querySelector(".auto-sync-switch")).focus();
    });
    expect(await browser.execute(() => document.activeElement?.classList.contains("auto-sync-switch"))).toBe(true);
    await browser.keys(" ");
    await browser.waitUntil(() => browser.execute(() => window.__spaceDown !== null), { timeout: 10000, timeoutMsg: "the Space keydown never arrived" });
    // WebDriver's key events are untrusted, so the browser itself does not click the button for them: what can be
    // checked is that the app did not take the key (before this fix it cancelled every Space and started a pan).
    expect(await browser.execute(() => window.__spaceDown.prevented)).toBe(false);
  });
});
