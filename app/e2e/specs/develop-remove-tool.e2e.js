import path from "node:path";
import { fileURLToPath } from "node:url";
import { openDevelopFor } from "../helpers.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const FIXTURE_PATH = path.resolve(__dirname, "../../../test_image/Smiling-woman-pink-shirt-portrait.jpg");
const FIXTURE_NAME = "Smiling-woman-pink-shirt-portrait";

/**
 * RFC-0032 slice E: the Remove tool's controls and marking layer in the real window. Needs no model and no GPU work
 * beyond opening Develop: a build without the model (every CI build) must say so, arm without error, and not let the
 * user draw marks it could never act on. The real removal is `develop-remove-tool-real-model.e2e.js` (local only).
 */
describe("Develop: the Remove tool without the model", function () {
  this.timeout(120000);
  const invoke = (/** @type {string} */ cmd, /** @type {any} */ args) => browser.execute((c, a) => window.__TAURI__.core.invoke(c, a), cmd, args ?? {});
  const exists = (/** @type {string} */ sel) => browser.execute((s) => !!document.querySelector(s), sel);
  const text = (/** @type {string} */ sel) => browser.execute((s) => document.querySelector(s)?.textContent?.trim() ?? "", sel);

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
    await browser.setTimeout({ script: 60000 });
    await invoke("import_files", { paths: [FIXTURE_PATH] });
    await browser.refresh();
    await openDevelopFor(FIXTURE_NAME);
    await (await $("#exposure")).waitForExist({ timeout: 20000 });
  });

  it("the tool is in the strip, arms, and says the model is missing; the layer takes no marks", async () => {
    expect(await exists('[data-testid="remove-tool"]')).toBe(true);
    expect(await exists('[data-testid="remove-layer"]')).toBe(false);
    // Like every mask tool, Remove needs the GPU canvas (its marking layer lives in the canvas's mask overlay). CI runners
    // have none, so the canvas is in its CPU-fallback view there: the button must be present but disabled, and say why.
    await waitFor(
      () => browser.execute(() => !!document.querySelector(".cpu-fallback-badge") || !(/** @type {HTMLButtonElement} */ (document.querySelector('[data-testid="remove-tool"]')).disabled)),
      "the canvas to settle (CPU fallback, or a GPU canvas with the tool enabled)",
      60000,
    );
    const gpuCanvas = !(await exists(".cpu-fallback-badge"));
    if (!gpuCanvas) {
      const btn = await browser.execute(() => {
        const b = /** @type {HTMLButtonElement} */ (document.querySelector('[data-testid="remove-tool"]'));
        return { disabled: b.disabled, title: b.title };
      });
      expect(btn.disabled).toBe(true);
      expect(btn.title).toContain("unavailable without GPU");
      expect(await exists('[data-testid="remove-layer"]')).toBe(false);
      return;
    }
    await browser.execute(() => /** @type {HTMLElement} */ (document.querySelector('[data-testid="remove-tool"]')).click());
    expect(await waitFor(async () => (await text('[data-testid="remove-unavailable"]')) || null, "the missing-model message")).toContain("not installed");
    await waitFor(() => exists('[data-testid="remove-layer"]'), "the marking layer");
    // A drag on the layer draws nothing.
    await browser.execute(() => {
      const layer = /** @type {HTMLElement} */ (document.querySelector('[data-testid="remove-layer"]'));
      const r = layer.getBoundingClientRect();
      const at = (/** @type {number} */ fx, /** @type {number} */ fy) => ({ clientX: r.left + r.width * fx, clientY: r.top + r.height * fy, pointerId: 1, button: 0, bubbles: true, cancelable: true });
      layer.dispatchEvent(new PointerEvent("pointerdown", at(0.4, 0.4)));
      layer.dispatchEvent(new PointerEvent("pointermove", at(0.5, 0.5)));
      layer.dispatchEvent(new PointerEvent("pointerup", at(0.5, 0.5)));
    });
    expect(await exists('[data-testid="remove-run"]')).toBe(false);
    expect(await browser.execute(() => document.querySelectorAll('[data-testid="remove-layer"] .mark, [data-testid="remove-layer"] .mark-stroke').length)).toBe(0);
  });

  it("toggling the tool off takes the layer away, and the stack was not touched", async () => {
    await browser.execute(() => /** @type {HTMLElement} */ (document.querySelector('[data-testid="remove-tool"]')).click());
    await waitFor(async () => !(await exists('[data-testid="remove-layer"]')), "the layer to go");
    const images = await invoke("list_images", {});
    const versionId = images.find((/** @type {any} */ i) => i.path === FIXTURE_PATH).version_id;
    expect((await invoke("get_edit_stack", { versionId })).ops.some((/** @type {any} */ o) => o.op === "remove")).toBe(false);
  });
});
