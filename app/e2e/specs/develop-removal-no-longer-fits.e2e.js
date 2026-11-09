import path from "node:path";
import os from "node:os";
import { fileURLToPath } from "node:url";
import { copyFileSync, existsSync, mkdirSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { openDevelopFor } from "../helpers.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const PHOTO = path.resolve(__dirname, "../../../test_image/Szechenyi-Chain-Bridge-Budapest-night.jpg"); // 1280 x 715
const PHOTO_NAME = path.basename(PHOTO, path.extname(PHOTO));
const MODEL = process.env.INPAINT_MODEL ?? "";
const IDENTIFIER = "dev.alfredwei.emulsion";

/**
 * RFC-0032 §20 in the real window: a removal made on a decode of another shape than the open photograph is called out
 * ("no longer fits") and Undo last takes it out. LOCAL ONLY (macOS): needs the LaMa file (INPAINT_MODEL=<lama_fp32.onnx>) and
 * WebGPU. The page cannot change a patch's stored size, so the spec does what a replaced file would have done to the data:
 * it rewrites the patch row's recorded decode size (`src_w`, `src_h`) in the catalog file with sqlite3, then reopens the photo.
 * It does NOT replace the file: the app only reads a photograph's size and content hash at import, so a file overwritten
 * on disk is not noticed by Develop at all (see RFC-0032 §23).
 */
describe("Develop: a removal that no longer fits the photograph is called out", function () {
  this.timeout(600000);

  const invoke = (/** @type {string} */ cmd, /** @type {any} */ args) => browser.execute((c, a) => window.__TAURI__.core.invoke(c, a), cmd, args ?? {});
  const text = (/** @type {string} */ sel) => browser.execute((s) => document.querySelector(s)?.textContent?.trim() ?? "", sel);
  const exists = (/** @type {string} */ sel) => browser.execute((s) => !!document.querySelector(s), sel);
  const click = (/** @type {string} */ sel) =>
    browser.execute((s) => {
      const el = document.querySelector(s);
      if (!el) throw new Error(`nothing to click: ${s}`);
      /** @type {HTMLElement} */ (el).click();
    }, sel);
  const stackRemovals = async () => ((await invoke("get_edit_stack", { versionId })).ops.find((/** @type {any} */ o) => o.op === "remove")?.items ?? []).map((/** @type {any} */ i) => i.patch);
  const SHOTS = process.env.SHOTS_DIR ?? "";
  const shot = async (/** @type {string} */ name) => {
    if (SHOTS) await browser.saveScreenshot(path.join(SHOTS, `${name}.png`));
  };
  let versionId = 0;

  async function waitFor(/** @type {() => Promise<any>} */ fn, /** @type {string} */ what, ms = 60000) {
    const deadline = Date.now() + ms;
    let last;
    while (Date.now() < deadline) {
      last = await fn();
      if (last) return last;
      await new Promise((r) => setTimeout(r, 200));
    }
    throw new Error(`${what}: not within ${ms} ms; last = ${JSON.stringify(last)}`);
  }

  async function reopen() {
    await browser.refresh();
    await openDevelopFor(PHOTO_NAME);
    await (await $("#exposure")).waitForExist({ timeout: 30000 });
  }

  before(async function () {
    if (!MODEL || !existsSync(MODEL) || process.platform !== "darwin") return this.skip();
    const models = path.join(os.homedir(), "Library", "Application Support", IDENTIFIER, "inpaint_models");
    mkdirSync(models, { recursive: true });
    copyFileSync(MODEL, path.join(models, "lama_fp32.onnx"));
    await browser.setTimeout({ script: 300000 });
    await invoke("import_files", { paths: [PHOTO] });
    versionId = (await invoke("list_images")).find((/** @type {any} */ i) => i.path === PHOTO).version_id;
    await invoke("set_edit_stack", { versionId, stack: { schema_version: 1, ops: [] }, label: null });
    await reopen();
  });

  it("a removal on the photograph it was made on is fine: no call-out", async () => {
    // A real removal through the model (the same call the Remove tool's button makes) on a small box.
    await click('[data-testid="remove-tool"]');
    await waitFor(() => exists('[data-testid="remove-layer"]'), "the layer");
    await browser.execute(() => {
      const layer = /** @type {HTMLElement} */ (document.querySelector('[data-testid="remove-layer"]'));
      const r = layer.getBoundingClientRect();
      const ev = (/** @type {string} */ type, /** @type {number} */ fx, /** @type {number} */ fy) =>
        layer.dispatchEvent(new PointerEvent(type, { clientX: r.left + r.width * fx, clientY: r.top + r.height * fy, pointerId: 1, button: 0, bubbles: true, cancelable: true }));
      ev("pointerdown", 0.5, 0.5);
      ev("pointermove", 0.52, 0.5);
      ev("pointermove", 0.52, 0.53);
      ev("pointerup", 0.52, 0.53);
    });
    await waitFor(async () => (await text('[data-testid="remove-status"]')) === "1 mark", "the mark");
    await click('[data-testid="remove-run"]');
    await waitFor(async () => (await stackRemovals()).length === 1, "the removal in the stack", 180000);
    expect(await exists('[data-testid="remove-problem"]')).toBe(false);
    const [patch] = await stackRemovals();
    const [row] = await invoke("removal_info", { versionId, patchIds: [patch] });
    expect([row.src_w, row.src_h]).toEqual([1280, 715]); // the decode the fill was made on
    await shot("1-before-replace");
  });

  it("when the recorded decode has another shape, the removal says it no longer fits and Undo last takes it out", async () => {
    const [patch] = await stackRemovals();
    await click('[data-testid="remove-tool"]'); // leave the tool, then change the data under it
    const db = path.join(os.homedir(), "Library", "Application Support", IDENTIFIER, "catalog.sqlite");
    execFileSync("sqlite3", [db, `PRAGMA busy_timeout=10000; UPDATE removal_patches SET src_w = 715, src_h = 1280 WHERE id = ${patch};`]);
    await reopen();
    await click('[data-testid="remove-tool"]');
    await waitFor(() => exists(`[data-testid="remove-problem"][data-patch="${patch}"]`), "the call-out");
    expect(await text(`[data-testid="remove-problem"][data-patch="${patch}"]`)).toMatch(/no longer fits/i);
    await shot("2-no-longer-fits");
    await click('[data-testid="remove-undo"]');
    await waitFor(async () => (await stackRemovals()).length === 0, "the removal to be taken out");
    await waitFor(async () => !(await exists('[data-testid="remove-problem"]')), "the call-out to go");
  });
});
