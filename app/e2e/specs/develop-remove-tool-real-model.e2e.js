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

  it("a listed removal other than the newest can be deleted; the canvas follows", async () => {
    // One removal is left after Undo last: add a second, then delete the FIRST one by its chip.
    const [first] = await stackRemovals();
    await draw([[0.7, 0.7], [0.74, 0.7], [0.74, 0.75], [0.7, 0.75], [0.7, 0.71]]);
    await waitFor(async () => (await text('[data-testid="remove-status"]')) === "1 mark", "the lasso mark");
    await click('[data-testid="remove-run"]');
    await waitFor(async () => (await stackRemovals()).length === 2, "the second patch in the stack", 180000);
    const [, second] = await stackRemovals();
    await waitFor(async () => (await exists('[data-testid="remove-delete"]')), "the removal chips to show");
    await shot("3a-chips");
    await click(`[data-testid="remove-delete"][data-patch="${first}"]`);
    await waitFor(async () => (await stackRemovals()).join() === String(second), "only the newer removal to be left in the stack");
    await waitFor(async () => (await canvasRemovals()) === String(second), "the canvas to carry only the newer one");
    const labels = (await invoke("get_history", { versionId })).map((/** @type {any} */ r) => r.label);
    expect(labels[labels.length - 1]).toBe("Delete removal");
    expect(await exists('[data-testid="remove-delete"]')).toBe(false); // one left: no chips
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
    // Put the stack back as it was, so the steps that follow see the same state with or without the click-select models.
    await click('[data-testid="remove-undo"]');
    await waitFor(async () => (await stackRemovals()).length === before, "the selected object's removal to be taken out again");
    await waitFor(async () => (await canvasRemovals()).split(",").filter(Boolean).length === before, "the canvas to follow");
  });

  it("Select: Heal instead heals the selected object from its cut, not the model", async function () {
    if (!haveSegment) return this.skip();
    const removalsBefore = await stackRemovals();
    const spotsBefore = (await invoke("get_edit_stack", { versionId })).ops.filter((/** @type {any} */ o) => o.op === "spot_mask").length;
    await draw([SELECT_AT]);
    await waitFor(async () => (await text('[data-testid="remove-status"]')) === "1 mark", "the clicked object", 120000);
    await click('[data-testid="remove-heal"]');
    const spot = await waitFor(async () => {
      const spots = (await invoke("get_edit_stack", { versionId })).ops.filter((/** @type {any} */ o) => o.op === "spot_mask");
      return spots.length === spotsBefore + 1 ? spots[spots.length - 1] : null;
    }, "a heal mask from the selected object");
    expect(spot.mode).toBe("heal");
    expect(spot.dabs.length).toBeGreaterThan(2);
    // The dabs sit on the object that was clicked: some dab is near the click.
    expect(spot.dabs.some((/** @type {any} */ d) => Math.hypot(d.x - SELECT_AT[0], d.y - SELECT_AT[1]) < 0.15)).toBe(true);
    expect(await stackRemovals()).toEqual(removalsBefore); // the model did not run
    await waitFor(async () => !(await exists('[data-testid="remove-layer"]')), "the tool to be left");
    await click('[data-testid="remove-tool"]'); // arm it again for the steps that follow
    await waitFor(() => exists('[data-testid="remove-layer"]'), "the layer to come back");
    await shot("4b-selected-healed");
  });

  it("leaving the tool takes the layer away", async () => {
    await click('[data-testid="remove-tool"]');
    await waitFor(async () => !(await exists('[data-testid="remove-layer"]')), "the layer to go");
  });
  it("Heal instead turns the marks into a selected Spot Removal (heal) mask, not a model removal", async () => {
    const removalsBefore = await stackRemovals();
    const spotsBefore = (await invoke("get_edit_stack", { versionId })).ops.filter((/** @type {any} */ o) => o.op === "spot_mask").length;
    await click('[data-testid="remove-tool"]');
    await waitFor(() => exists('[data-testid="remove-layer"]'), "the layer to come back");
    await click('[data-testid="remove-options"] button', "Brush");
    await draw([[0.3, 0.8], [0.34, 0.8], [0.38, 0.8]]);
    await waitFor(async () => (await text('[data-testid="remove-status"]')) === "1 mark", "the stroke mark");
    await click('[data-testid="remove-heal"]');
    const spot = await waitFor(async () => {
      const spots = (await invoke("get_edit_stack", { versionId })).ops.filter((/** @type {any} */ o) => o.op === "spot_mask");
      return spots.length === spotsBefore + 1 ? spots[spots.length - 1] : null;
    }, "a new spot mask in the stack");
    expect(spot.mode).toBe("heal");
    expect(spot.dabs.length).toBeGreaterThan(5);
    expect(spot.dabs[0].x).toBeCloseTo(0.3, 1);
    expect(spot.sourceOffset.dx).not.toBe(0);
    expect(await stackRemovals()).toEqual(removalsBefore); // the model did not run
    await waitFor(async () => !(await exists('[data-testid="remove-layer"]')), "the tool to be left");
    const labels = await waitFor(async () => {
      const l = (await invoke("get_history", { versionId })).map((/** @type {any} */ r) => r.label);
      return l[l.length - 1] === "Heal instead" ? l : null;
    }, "the Heal instead history row");
    expect(labels).toContain("Heal instead");
    await shot("5-heal-instead");
  });

  it("Heal on a finished removal swaps it for a heal mask over its hole, in one edit", async () => {
    const [patch] = await stackRemovals();
    expect(patch).toBeTruthy();
    const stored = await invoke("removal_hole", { versionId, patchId: patch });
    console.log(`[measure] stored hole: ${Math.floor((stored.png.length * 3) / 4)} bytes of PNG, box origin (${stored.x}, ${stored.y}) in a ${stored.src_w}x${stored.src_h} decode`);
    const spotsBefore = (await invoke("get_edit_stack", { versionId })).ops.filter((/** @type {any} */ o) => o.op === "spot_mask").length;
    await click('[data-testid="remove-tool"]');
    await waitFor(() => exists(`[data-testid="remove-heal-removal"][data-patch="${patch}"]`), "the Heal chip for the removal (it has a stored hole)");
    await click(`[data-testid="remove-heal-removal"][data-patch="${patch}"]`);
    await waitFor(async () => (await stackRemovals()).length === 0, "the removal to leave the stack");
    const stack = await invoke("get_edit_stack", { versionId });
    const spots = stack.ops.filter((/** @type {any} */ o) => o.op === "spot_mask");
    expect(spots.length).toBe(spotsBefore + 1);
    const spot = spots[spots.length - 1];
    expect(spot.mode).toBe("heal");
    // The second removal was the lasso around (0.70..0.74, 0.70..0.75): the dabs sit on it, a little over for the grown hole.
    expect(spot.dabs.length).toBeGreaterThan(2);
    for (const d of spot.dabs) {
      expect(d.x).toBeGreaterThan(0.66);
      expect(d.x).toBeLessThan(0.78);
      expect(d.y).toBeGreaterThan(0.66);
      expect(d.y).toBeLessThan(0.79);
    }
    const labels = await waitFor(async () => {
      const l = (await invoke("get_history", { versionId })).map((/** @type {any} */ r) => r.label);
      return l[l.length - 1] === "Heal instead" ? l : null;
    }, "the Heal instead history row");
    expect(labels.filter((/** @type {string} */ l) => l === "Heal instead").length).toBeGreaterThanOrEqual(2);
    await waitFor(async () => (await canvasRemovals()) === "", "the canvas to drop the removal");
    await shot("6-heal-finished-removal");
    await click('[data-testid="remove-tool"]'); // the tool stays armed after a Heal on a chip: leave it
    await waitFor(async () => !(await exists('[data-testid="remove-layer"]')), "the tool to be left");
  });

  it("Try again redoes a removal made with an older engine from its stored hole, in place, in one edit", async () => {
    // A removal with the installed engine has no Try again (the result would be identical): make one, then say it came from an older model.
    await click('[data-testid="remove-tool"]');
    await waitFor(() => exists('[data-testid="remove-layer"]'), "the layer");
    await click('[data-testid="remove-options"] button', "Lasso");
    await draw([[0.15, 0.15], [0.2, 0.15], [0.2, 0.2], [0.15, 0.2], [0.15, 0.16]]);
    await waitFor(async () => (await text('[data-testid="remove-status"]')) === "1 mark", "the lasso mark");
    await click('[data-testid="remove-run"]');
    await waitFor(async () => (await stackRemovals()).length === 1, "the removal in the stack", 180000);
    expect(await exists('[data-testid="remove-redo"]')).toBe(false);
    const [old] = await stackRemovals();
    const stack = await invoke("get_edit_stack", { versionId });
    const op = stack.ops.find((/** @type {any} */ o) => o.op === "remove");
    op.items[0].engine = "lama-old";
    await invoke("set_edit_stack", { versionId, stack, label: "Pretend an older engine" });
    await browser.refresh();
    await openDevelopFor(FIXTURE_NAME);
    await (await $("#exposure")).waitForExist({ timeout: 30000 });
    await click('[data-testid="remove-tool"]');
    await waitFor(() => exists(`[data-testid="remove-redo"][data-patch="${old}"]`), "the Try again chip");
    await click(`[data-testid="remove-redo"][data-patch="${old}"]`);
    await waitFor(async () => {
      const [now] = await stackRemovals();
      return now !== undefined && now !== old;
    }, "the removal to be redone", 180000);
    const after = (await invoke("get_edit_stack", { versionId })).ops.find((/** @type {any} */ o) => o.op === "remove").items;
    expect(after.length).toBe(1);
    expect(after[0].engine).toBe("lama-512-places2");
    expect(after[0].patch).not.toBe(old);
    const rows = await invoke("removal_info", { versionId, patchIds: [old, after[0].patch] });
    expect(rows.map((/** @type {any} */ r) => [r.patch_id, r.has_hole])).toEqual([[old, true], [after[0].patch, true]]); // both rows stay, with their holes
    const labels = (await invoke("get_history", { versionId })).map((/** @type {any} */ r) => r.label);
    expect(labels[labels.length - 1]).toBe("Redo removal");
    await waitFor(async () => (await canvasRemovals()) === String(after[0].patch), "the canvas to carry the new patch");
    expect(await exists('[data-testid="remove-redo"]')).toBe(false); // the engine matches now
    await shot("7-try-again");
  });

  it("a removal whose stored fill is missing is called out, and Undo last takes it out", async () => {
    const [real] = await stackRemovals();
    const stack = await invoke("get_edit_stack", { versionId });
    stack.ops.find((/** @type {any} */ o) => o.op === "remove").items.push({ patch: 999999, engine: "lama-512-places2" });
    await invoke("set_edit_stack", { versionId, stack, label: "Pretend a missing patch" });
    await browser.refresh();
    await openDevelopFor(FIXTURE_NAME);
    await (await $("#exposure")).waitForExist({ timeout: 30000 });
    await click('[data-testid="remove-tool"]');
    await waitFor(() => exists('[data-testid="remove-problem"][data-patch="999999"]'), "the missing removal to be called out");
    expect(await text('[data-testid="remove-problem"][data-patch="999999"]')).toMatch(/Removal 2 is missing/);
    expect(await exists(`[data-testid="remove-problem"][data-patch="${real}"]`)).toBe(false); // the real one is fine
    await shot("8-missing-patch");
    await click('[data-testid="remove-undo"]'); // the newest is the missing one
    await waitFor(async () => (await stackRemovals()).join() === String(real), "only the real removal to be left");
    await waitFor(async () => !(await exists('[data-testid="remove-problem"]')), "the call-out to go");
    await click('[data-testid="remove-tool"]');
  });
});
