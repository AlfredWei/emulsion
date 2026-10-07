import path from "node:path";
import { fileURLToPath } from "node:url";
import { openDevelopFor } from "../helpers.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const FIXTURE_PATH = path.resolve(__dirname, "../../../test_image/Field-corn-Liechtenstein-landscape.jpg");
const FIXTURE_NAME = "Field-corn-Liechtenstein-landscape";

/**
 * RFC-0026 slice 2c: the Select Subject UI in the real window, on the paths that need neither the
 * 155 MB weights nor the network (the real-model run is manual, see PROGRESS.md):
 *
 * 1. With the model files absent, arming the tool opens the consent dialog (naming the host and the
 *    size), nothing is downloaded until the user agrees, and Cancel closes it and leaves the tool.
 * 2. A stored segment mask opens its panel with the Grow control, edits persist, and the panel's
 *    Add shape menu offers Subject.
 *
 * The mask tools are disabled without WebGPU, so both skip on a runner that has none (CI's).
 */
describe("Develop Select Subject (click-select UI)", function () {
  this.timeout(180000);

  let versionId;
  let gpuAvailable = false;
  let modelReady = false;

  async function getEditStack() {
    return browser.execute((vid) => window.__TAURI__.core.invoke("get_edit_stack", { versionId: vid }), versionId);
  }

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

  async function waitForDom(/** @type {() => boolean} */ pred, /** @type {string} */ what) {
    const deadline = Date.now() + 15000;
    while (Date.now() < deadline) {
      if (await browser.execute(pred)) return;
      await new Promise((resolve) => setTimeout(resolve, 150));
    }
    throw new Error(`${what}: never appeared within 15s`);
  }

  async function domClick(/** @type {string} */ selector, /** @type {string | null} */ text = null) {
    const ok = await browser.execute(
      (sel, src) => {
        const re = src ? new RegExp(src, "i") : null;
        const el = Array.from(document.querySelectorAll(sel)).find((e) => !re || re.test(e.textContent ?? ""));
        if (!el) return false;
        /** @type {HTMLElement} */ (el).click();
        return true;
      },
      selector,
      text,
    );
    if (!ok) throw new Error(`nothing to click for ${selector}${text ? ` / ${text}` : ""}`);
  }

  /** A 256x256 8-bit gray PNG (base64): a left-to-right ramp, like a helper's stored logit field. */
  async function rampField() {
    return browser.execute(async () => {
      const canvas = new OffscreenCanvas(256, 256);
      const ctx = canvas.getContext("2d");
      const img = ctx.createImageData(256, 256);
      for (let y = 0; y < 256; y++) for (let x = 0; x < 256; x++) img.data.set([x, x, x, 255], (y * 256 + x) * 4);
      ctx.putImageData(img, 0, 0);
      const buf = new Uint8Array(await (await canvas.convertToBlob({ type: "image/png" })).arrayBuffer());
      let bin = "";
      for (let i = 0; i < buf.length; i += 0x8000) bin += String.fromCharCode(...buf.subarray(i, i + 0x8000));
      return btoa(bin);
    });
  }

  before(async function () {
    await browser.setTimeout({ script: 90000 });
    gpuAvailable = await browser.execute(async () => {
      if (!navigator.gpu) return false;
      try {
        return !!(await navigator.gpu.requestAdapter());
      } catch {
        return false;
      }
    });
    modelReady = (await browser.execute(() => window.__TAURI__.core.invoke("segment_model_status"))).state === "ready";
    await browser.execute(async (p) => window.__TAURI__.core.invoke("import_files", { paths: [p] }), FIXTURE_PATH);
    await browser.refresh();
    const match = await browser.execute(async (p) => {
      const images = await window.__TAURI__.core.invoke("list_images");
      return images.find((img) => img.path === p) ?? null;
    }, FIXTURE_PATH);
    expect(match).not.toBeNull();
    versionId = match.version_id;
  });

  async function openWithStack(/** @type {any} */ stack) {
    await browser.execute((vid, s) => window.__TAURI__.core.invoke("set_edit_stack", { versionId: vid, stack: s }), versionId, stack);
    await browser.refresh();
    await openDevelopFor(FIXTURE_NAME);
    const exposureInput = await $("#exposure");
    await exposureInput.waitForExist({ timeout: 20000 });
  }

  it("arming the tool without the model asks first, and Cancel leaves the tool", async function () {
    if (!gpuAvailable || modelReady) this.skip(); // a developer machine that has the weights installed
    await openWithStack({ schema_version: 1, ops: [] });
    await waitForDom(() => !!document.querySelector('[aria-label="Select Subject"]'), "tool strip");
    await domClick('[aria-label="Select Subject"]');
    await waitForDom(() => !!document.querySelector('[data-testid="segment-model-dialog"]'), "consent dialog");
    const text = await browser.execute(() => document.querySelector('[data-testid="segment-model-dialog"]')?.textContent ?? "");
    expect(text).toContain("huggingface.co");
    expect(text).toMatch(/\d+ MB/);
    expect(text).toContain("Nothing about your photos is sent");
    await domClick('[data-testid="segment-model-dialog"] button', "^Cancel$");
    await waitForDom(() => !document.querySelector('[data-testid="segment-model-dialog"]'), "dialog closed");
    const stillActive = await browser.execute(() => !!document.querySelector('[aria-label="Select Subject"].active'));
    expect(stillActive).toBe(false);
    expect((await getEditStack()).ops).toEqual([]);
  });

  it("a stored segment mask opens a panel with Grow, and its edits persist", async function () {
    if (!gpuAvailable) this.skip();
    const logits = await rampField();
    await openWithStack({
      schema_version: 1,
      ops: [{ op: "segment_mask", id: "e2e-subject", logits, feather: 0, grow: 0, invert: false, exposure: 1, contrast: 0, saturation: 0 }],
    });
    await waitForDom(() => !!Array.from(document.querySelectorAll("button")).find((b) => /^Subject 1$/.test((b.textContent ?? "").trim())), "Subject chip");
    await domClick("button", "^Subject 1$");
    await waitForDom(() => !!document.querySelector('[role="dialog"][aria-label="Subject adjustments"]'), "mask panel");
    await waitForDom(() => !!document.querySelector("#mask-grow"), "Grow slider");

    await browser.execute(() => {
      const el = /** @type {HTMLInputElement} */ (document.querySelector("#mask-grow"));
      el.value = "40";
      el.dispatchEvent(new Event("input", { bubbles: true }));
    });
    const stack = await waitForStack((s) => s.ops[0]?.grow === 40, "grow persisted");
    expect(stack.ops[0].logits).toBe(logits); // Grow never touches the stored field

    await domClick("button", "Add shape");
    await waitForDom(() => !!Array.from(document.querySelectorAll('[role="menuitem"]')).find((b) => /^Subject/.test((b.textContent ?? "").trim())), "Subject in the Add shape menu");
  });
});
