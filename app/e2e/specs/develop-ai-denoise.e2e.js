import path from "node:path";
import { fileURLToPath } from "node:url";
import { openDevelopFor } from "../helpers.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const FIXTURE_PATH = path.resolve(__dirname, "../../../test_image/Field-corn-Liechtenstein-landscape.jpg");
const FIXTURE_NAME = "Field-corn-Liechtenstein-landscape";

/**
 * RFC-0027 slice 3c: AI Denoise in the real window, on the paths that need neither the 117 MB model nor
 * the network nor WebGPU (the real-model run is manual, see PROGRESS.md):
 *
 * 1. The commands behave without the model: status with size and host, a wrong file rejected without
 *    touching the cache, no kept copy for an unknown photo, a job refused while the model is missing.
 * 2. The Noise Reduction panel shows the AI Denoise block ("Not denoised yet", Amount disabled), asking to
 *    denoise opens the consent dialog naming the host and size, nothing is downloaded until the user agrees,
 *    and Cancel closes it and leaves the edit stack untouched.
 */
describe("Develop AI Denoise", function () {
  this.timeout(180000);

  let versionId;
  let modelReady = false;

  const invoke = (/** @type {string} */ cmd, /** @type {any} */ args = {}) =>
    browser.execute(
      async (c, a) => {
        try {
          return { ok: await window.__TAURI__.core.invoke(c, a) };
        } catch (e) {
          return { err: String(e) };
        }
      },
      cmd,
      args,
    );

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

  before(async function () {
    await browser.setTimeout({ script: 90000 });
    modelReady = (await invoke("denoise_model_status")).ok?.state === "ready";
    await browser.execute(async (p) => window.__TAURI__.core.invoke("import_files", { paths: [p] }), FIXTURE_PATH);
    await browser.refresh();
    const match = await browser.execute(async (p) => {
      const images = await window.__TAURI__.core.invoke("list_images");
      return images.find((img) => img.path === p) ?? null;
    }, FIXTURE_PATH);
    expect(match).not.toBeNull();
    versionId = match.version_id;
  });

  it("reports the model status with the download size and the host", async () => {
    const r = await invoke("denoise_model_status");
    expect(r.err).toBeUndefined();
    expect(["ready", "missing"]).toContain(r.ok.state);
    expect(r.ok.host).toBe("github.com");
    if (r.ok.state === "missing") expect(r.ok.download_bytes).toBe(117316644);
  });

  it("rejects a file that is not the model, without touching the cache", async () => {
    const before = await invoke("denoise_model_status");
    const r = await invoke("denoise_import_model", { path: "/definitely/not/a/model.onnx" });
    expect(r.err).toBeDefined();
    expect(await invoke("denoise_model_status")).toEqual(before);
  });

  it("has no kept copy or preview blend for a photo that was never denoised", async () => {
    expect((await invoke("denoise_cache_info", { contentHash: "0".repeat(64) })).ok).toBeNull();
    expect((await invoke("get_denoised_develop_preview", { path: FIXTURE_PATH, contentHash: "0".repeat(64), amount: 60, full: false })).ok).toBeNull();
    expect((await invoke("denoise_cancel")).err).toBeUndefined(); // nothing runs: a no-op
    expect((await invoke("denoise_remove_cache", { contentHash: "0".repeat(64) })).err).toBeUndefined();
  });

  it("refuses a job while the model is missing", async function () {
    if (modelReady) this.skip(); // a developer machine that has the model installed
    const r = await invoke("denoise_run", { path: FIXTURE_PATH, contentHash: "0".repeat(64), cropCentre: null });
    expect(r.err).toContain("not installed");
  });

  it("shows the AI Denoise block, and asking to denoise without the model asks first", async function () {
    if (modelReady) this.skip();
    await browser.execute((vid) => window.__TAURI__.core.invoke("set_edit_stack", { versionId: vid, stack: { schema_version: 1, ops: [] } }), versionId);
    await browser.refresh();
    await openDevelopFor(FIXTURE_NAME);
    await (await $("#exposure")).waitForExist({ timeout: 20000 });

    await domClick("summary", "Noise Reduction");
    await waitForDom(() => !!document.querySelector('[data-testid="ai-denoise"]'), "AI Denoise block");
    await waitForDom(() => /Not denoised yet/.test(document.querySelector('[data-testid="ai-denoise-state"]')?.textContent ?? ""), "state line");
    expect(await browser.execute(() => /** @type {HTMLInputElement} */ (document.querySelector("#ai-denoise-amount")).disabled)).toBe(true);

    await domClick('[data-testid="ai-denoise-whole"]');
    await waitForDom(() => !!document.querySelector('[data-testid="denoise-model-dialog"]'), "consent dialog");
    const text = await browser.execute(() => document.querySelector('[data-testid="denoise-model-dialog"]')?.textContent ?? "");
    expect(text).toContain("github.com");
    expect(text).toMatch(/117 MB/);
    expect(text).toContain("Nothing about your photos is sent");

    await domClick('[data-testid="denoise-model-dialog"] button', "^Cancel$");
    await waitForDom(() => !document.querySelector('[data-testid="denoise-model-dialog"]'), "dialog closed");
    expect((await invoke("denoise_model_status")).ok.state).toBe("missing"); // nothing was downloaded
    const stack = await browser.execute((vid) => window.__TAURI__.core.invoke("get_edit_stack", { versionId: vid }), versionId);
    expect(stack.ops).toEqual([]);
  });
});
