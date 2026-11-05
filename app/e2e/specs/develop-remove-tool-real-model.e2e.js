import path from "node:path";
import os from "node:os";
import { fileURLToPath } from "node:url";
import { copyFileSync, existsSync, mkdirSync } from "node:fs";
import { openDevelopFor } from "../helpers.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const FIXTURE_PATH = process.env.INPAINT_IMAGE ?? path.resolve(__dirname, "../../../test_image/Szechenyi-Chain-Bridge-Budapest-night.jpg");
const FIXTURE_NAME = path.basename(FIXTURE_PATH, path.extname(FIXTURE_PATH));
const MODEL = process.env.INPAINT_MODEL ?? "";
/** Folder with sam2_hiera_tiny.encoder.onnx and .decoder.onnx (optional): without it the click-select test is skipped. */
const SEGMENT_DIR = process.env.SEGMENT_MODELS_DIR ?? "";
/** Where the click-select test clicks, as "x,y" fractions of the photograph (the object to take out). */
const SELECT_AT = (process.env.SELECT_AT ?? "0.5,0.5").split(",").map(Number);
const IDENTIFIER = "dev.alfredwei.emulsion";

/**
 * RFC-0032 slice E: a REAL removal driven through the Remove tool's own controls. LOCAL ONLY (macOS): needs the 208 MB
 * LaMa file (INPAINT_MODEL=<lama_fp32.onnx>, copied into the app's `inpaint_models` folder) and WebGPU. Marks are made
 * with synthetic pointer events on the marking layer (WebDriver pointer actions deliver only a legacy mousedown).
 */
describe("Develop: the Remove tool, with the model", function () {
  this.timeout(600000);

  const invoke = (/** @type {string} */ cmd, /** @type {any} */ args) => browser.execute((c, a) => window.__TAURI__.core.invoke(c, a), cmd, args ?? {});
  const text = (/** @type {string} */ sel) => browser.execute((s) => document.querySelector(s)?.textContent?.trim() ?? "", sel);
  const exists = (/** @type {string} */ sel) => browser.execute((s) => !!document.querySelector(s), sel);
  const click = (/** @type {string} */ sel, /** @type {string | null} */ label = null) =>
    browser.execute(
      (s, l) => {
        const el = Array.from(document.querySelectorAll(s)).find((e) => !l || (e.textContent ?? "").trim() === l);
        if (!el) throw new Error(`nothing to click: ${s} ${l ?? ""}`);
        /** @type {HTMLElement} */ (el).click();
      },
      sel,
      label,
    );
  /** Draws along `points` (fractions of the photograph) on the marking layer. */
  const draw = (/** @type {[number, number][]} */ points) =>
    browser.execute((pts) => {
      const layer = /** @type {HTMLElement} */ (document.querySelector('[data-testid="remove-layer"]'));
      const r = layer.getBoundingClientRect();
      const ev = (/** @type {string} */ type, /** @type {[number, number]} */ [fx, fy]) => layer.dispatchEvent(new PointerEvent(type, { clientX: r.left + r.width * fx, clientY: r.top + r.height * fy, pointerId: 1, button: 0, bubbles: true, cancelable: true }));
      ev("pointerdown", pts[0]);
      for (const p of pts.slice(1)) ev("pointermove", p);
      ev("pointerup", pts[pts.length - 1]);
    }, points);
  const stackRemovals = async () => ((await invoke("get_edit_stack", { versionId })).ops.find((/** @type {any} */ o) => o.op === "remove")?.items ?? []).map((/** @type {any} */ i) => i.patch);
  const canvasRemovals = () => browser.execute(() => document.querySelector(".canvas-wrap canvas")?.getAttribute("data-removals-source") ?? "");
  let versionId = 0;
  let haveSegment = false;
  const SHOTS = process.env.SHOTS_DIR ?? "";
  const shot = async (/** @type {string} */ name) => {
    if (SHOTS) await browser.saveScreenshot(path.join(SHOTS, `${name}.png`));
  };

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

  before(async function () {
    if (!MODEL || !existsSync(MODEL) || process.platform !== "darwin") return this.skip();
    const models = path.join(os.homedir(), "Library", "Application Support", IDENTIFIER, "inpaint_models");
    mkdirSync(models, { recursive: true });
    copyFileSync(MODEL, path.join(models, "lama_fp32.onnx"));
    if (SEGMENT_DIR && existsSync(path.join(SEGMENT_DIR, "sam2_hiera_tiny.encoder.onnx"))) {
      const seg = path.join(os.homedir(), "Library", "Application Support", IDENTIFIER, "segment_models");
      mkdirSync(seg, { recursive: true });
      for (const f of ["sam2_hiera_tiny.encoder.onnx", "sam2_hiera_tiny.decoder.onnx"]) copyFileSync(path.join(SEGMENT_DIR, f), path.join(seg, f));
      haveSegment = true;
    }
    await browser.setTimeout({ script: 300000 });
    await invoke("import_files", { paths: [FIXTURE_PATH] });
    const mine = (await invoke("list_images")).find((/** @type {any} */ i) => i.path === FIXTURE_PATH);
    versionId = mine.version_id;
    await invoke("set_edit_stack", { versionId, stack: { schema_version: 1, ops: [] }, label: null });
    await browser.refresh();
    await openDevelopFor(FIXTURE_NAME);
    await (await $("#exposure")).waitForExist({ timeout: 30000 });
  });

  it("arms, finds the model, and a painted stroke becomes one mark", async () => {
    await click('[data-testid="remove-tool"]');
    await waitFor(() => exists('[data-testid="remove-layer"]'), "the marking layer");
    await waitFor(async () => (await text('[data-testid="remove-status"]')) === "Mark what to remove", "the tool to be ready");
    expect(await exists('[data-testid="remove-unavailable"]')).toBe(false);
    await draw([[0.5, 0.5], [0.51, 0.505], [0.52, 0.51], [0.53, 0.512]]);
    await waitFor(async () => (await text('[data-testid="remove-status"]')) === "1 mark", "one mark");
    expect(await browser.execute(() => document.querySelectorAll('[data-testid="remove-layer"] .mark-stroke').length)).toBe(1);
    await shot("1-marked");
  });

  it("Remove runs the model, names the patch in the stack, shows it on the canvas, and clears the mark", async () => {
    const t0 = Date.now();
    await click('[data-testid="remove-run"]');
    await waitFor(async () => /^Removed 1 thing/.test(await text('[data-testid="remove-status"]')), "the removal to finish", 180000);
    console.log(`removal through the tool took ${((Date.now() - t0) / 1000).toFixed(1)} s: ${await text('[data-testid="remove-status"]')}`);
    const patches = await waitFor(async () => {
      const p = await stackRemovals();
      return p.length ? p : null;
    }, "the remove op to be stored");
    expect(patches.length).toBe(1);
    await waitFor(async () => (await canvasRemovals()) === String(patches[0]), "the canvas source to carry the patch");
    expect(await browser.execute(() => document.querySelectorAll('[data-testid="remove-layer"] .mark-stroke').length)).toBe(0);
    const labels = await invoke("get_history", { versionId });
    expect(labels.map((/** @type {any} */ r) => r.label)).toContain("Remove object");
    await shot("2-removed");
  });

  it("a lasso is a mark too, and a second removal is added after the first", async () => {
    await click('[data-testid="remove-options"] button', "Lasso");
    await draw([[0.2, 0.2], [0.24, 0.2], [0.24, 0.25], [0.2, 0.25], [0.2, 0.21]]);
    await waitFor(async () => (await text('[data-testid="remove-status"]')) === "1 mark", "the lasso mark");
    expect(await browser.execute(() => document.querySelectorAll('[data-testid="remove-layer"] polygon').length)).toBe(1);
    await click('[data-testid="remove-run"]');
    await waitFor(async () => (await stackRemovals()).length === 2, "the second patch in the stack", 180000);
    await waitFor(async () => (await canvasRemovals()).split(",").length === 2, "the canvas to carry both");
  });

  it("a large mark asks first and Clear drops it without running", async () => {
    await draw([[0.1, 0.1], [0.9, 0.1], [0.9, 0.9], [0.1, 0.9], [0.12, 0.5]]);
    await waitFor(async () => (await text('[data-testid="remove-status"]')) === "1 mark", "the large lasso");
    await click('[data-testid="remove-run"]');
    await waitFor(async () => /large area/.test(await text('[data-testid="remove-status"]')), "the large-area question");
    expect((await stackRemovals()).length).toBe(2);
    await click('[data-testid="remove-options"] button', "Clear");
    await waitFor(async () => (await text('[data-testid="remove-status"]')) !== "" && !(await exists('[data-testid="remove-run"]')), "the marks to go");
  });

  it("Undo last takes the newest removal out; the canvas follows", async () => {
    await click('[data-testid="remove-undo"]');
    await waitFor(async () => (await stackRemovals()).length === 1, "one removal left in the stack");
    await waitFor(async () => (await canvasRemovals()).split(",").length === 1, "the canvas to follow");
  });

  it("Select: a click takes the object under it, shows it, and Remove takes it out", async function () {
    if (!haveSegment) return this.skip();
    await click('[data-testid="remove-mode-select"]');
    await waitFor(async () => (await text('[data-testid="remove-status"]')) === "Click what to remove", "click-select to be ready", 120000);
    const before = (await stackRemovals()).length;
    await draw([SELECT_AT]);
    await waitFor(async () => (await text('[data-testid="remove-status"]')) === "1 mark", "the clicked object", 120000);
    expect(await browser.execute(() => document.querySelectorAll('[data-testid="remove-layer"] image').length)).toBe(1);
    await shot("3-selected");
    // Another guess (Tab) swaps the candidate when the model offered several.
    if (await exists('[data-testid="remove-next-guess"]')) {
      await click('[data-testid="remove-next-guess"]');
      await new Promise((r) => setTimeout(r, 500));
      await shot("3b-another-guess");
      await click('[data-testid="remove-next-guess"]');
      await click('[data-testid="remove-next-guess"]');
    }
    await click('[data-testid="remove-run"]');
    await waitFor(async () => {
      if (/large area/.test(await text('[data-testid="remove-status"]'))) await click('[data-testid="remove-options"] button', "Remove anyway").catch(() => {}); // gone once pressed
      return (await stackRemovals()).length === before + 1;
    }, "the selected object's patch in the stack", 240000);
    await waitFor(async () => (await canvasRemovals()).split(",").length === before + 1, "the canvas to carry it");
    expect(await browser.execute(() => document.querySelectorAll('[data-testid="remove-layer"] image').length)).toBe(0);
    await shot("4-selected-removed");
  });

  it("leaving the tool takes the layer away", async () => {
    await click('[data-testid="remove-tool"]');
    await waitFor(async () => !(await exists('[data-testid="remove-layer"]')), "the layer to go");
  });
});
