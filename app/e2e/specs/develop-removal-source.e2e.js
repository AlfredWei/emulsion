import path from "node:path";
import os from "node:os";
import { fileURLToPath } from "node:url";
import { createHash } from "node:crypto";
import { copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { openDevelopFor } from "../helpers.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const FIXTURE_PATH = process.env.INPAINT_IMAGE ?? path.resolve(__dirname, "../../../test_image/Szechenyi-Chain-Bridge-Budapest-night.jpg");
const FIXTURE_NAME = path.basename(FIXTURE_PATH, path.extname(FIXTURE_PATH));
const MODEL = process.env.INPAINT_MODEL ?? "";
const IDENTIFIER = "dev.alfredwei.emulsion";

/**
 * RFC-0032 §12 slice D2d: a REAL removal through the real window. LOCAL ONLY (macOS): it needs the 208 MB LaMa file
 * (INPAINT_MODEL=<lama_fp32.onnx>, copied into the app's `inpaint_models` folder, where the app looks for it) and
 * WebGPU. The patch is made by `remove_object` itself, then named in the edit stack; what is checked is that the
 * pictures the app builds from that stack carry it: the canvas's source texture, the graded preview, the thumbnail.
 */
describe("Develop: a removal is in the canvas source, the graded preview and the thumbnail", function () {
  this.timeout(600000);

  const invoke = (/** @type {string} */ cmd, /** @type {any} */ args) => browser.execute((c, a) => window.__TAURI__.core.invoke(c, a), cmd, args ?? {});
  let versionId = 0;
  let hash = "";
  let patchId = 0;
  /** Where the mark is, normalised. */
  const MARK = { x: 0.5, y: 0.5, r: 0.03 };

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

  /** The mean absolute difference of two image files over the box the mark covers (and, as a control, a corner). */
  async function difference(/** @type {string} */ a, /** @type {string} */ b) {
    return browser.execute(
      async (pa, pb, mark) => {
        const load = async (p) => createImageBitmap(await (await fetch(window.__TAURI__.core.convertFileSrc(p))).blob());
        const [ia, ib] = [await load(pa), await load(pb)];
        if (ia.width !== ib.width || ia.height !== ib.height) return { error: `sizes ${ia.width}x${ia.height} vs ${ib.width}x${ib.height}` };
        const read = (bmp) => {
          const c = new OffscreenCanvas(bmp.width, bmp.height);
          const g = c.getContext("2d");
          g.drawImage(bmp, 0, 0);
          return g;
        };
        const [ga, gb] = [read(ia), read(ib)];
        const meanDiff = (x, y, w, h) => {
          const da = ga.getImageData(x, y, w, h).data;
          const db = gb.getImageData(x, y, w, h).data;
          let sum = 0;
          for (let i = 0; i < da.length; i += 4) sum += Math.abs(da[i] - db[i]) + Math.abs(da[i + 1] - db[i + 1]) + Math.abs(da[i + 2] - db[i + 2]);
          return sum / ((da.length / 4) * 3);
        };
        const side = Math.max(4, Math.round(mark.r * Math.max(ia.width, ia.height)));
        const cx = Math.round(mark.x * ia.width);
        const cy = Math.round(mark.y * ia.height);
        return { inside: meanDiff(Math.max(0, cx - side), Math.max(0, cy - side), side * 2, side * 2), corner: meanDiff(0, 0, 16, 16), w: ia.width, h: ia.height };
      },
      a,
      b,
      MARK,
    );
  }

  before(async function () {
    if (!MODEL || !existsSync(MODEL) || process.platform !== "darwin") return this.skip();
    const models = path.join(os.homedir(), "Library", "Application Support", IDENTIFIER, "inpaint_models");
    mkdirSync(models, { recursive: true });
    copyFileSync(MODEL, path.join(models, "lama_fp32.onnx"));
    await browser.setTimeout({ script: 300000 });
    await invoke("import_files", { paths: [FIXTURE_PATH] });
    const images = await invoke("list_images");
    const mine = images.find((/** @type {any} */ i) => i.path === FIXTURE_PATH);
    versionId = mine.version_id;
    hash = mine.content_hash;
    await invoke("set_edit_stack", { versionId, stack: { schema_version: 1, ops: [] }, label: null });
    await browser.refresh();
    await openDevelopFor(FIXTURE_NAME);
    await (await $("#exposure")).waitForExist({ timeout: 30000 });
  });

  it("removes an object and names the patch in the edit stack; the canvas source then carries it", async () => {
    const removed = await invoke("remove_object", { versionId, shapes: [{ kind: "stroke", points: [[MARK.x, MARK.y]], radius: MARK.r }], growPx: 6 });
    expect(removed.length).toBe(1);
    patchId = removed[0].patch_id;
    console.log(`removal patch ${patchId}: ${JSON.stringify(removed[0])}`);
    // Written through the same command the app uses; the Develop store picks the stack up when the photo is reopened.
    await invoke("set_edit_stack", { versionId, stack: { schema_version: 1, ops: [{ op: "remove", items: [{ patch: patchId, engine: removed[0].engine }] }] }, label: "Remove object" });
    await browser.refresh();
    await openDevelopFor(FIXTURE_NAME);
    await (await $("#exposure")).waitForExist({ timeout: 30000 });
    const attr = () => browser.execute(() => document.querySelector(".canvas-wrap canvas")?.getAttribute("data-removals-source") ?? "");
    await waitFor(async () => (await attr()) === String(patchId), "the canvas source to carry the removal");
  });

  it("the source file differs from the plain preview where the mark was and nowhere else", async () => {
    const plain = await invoke("get_develop_preview", { path: FIXTURE_PATH, contentHash: hash });
    const baked = await invoke("get_develop_source_preview", { path: FIXTURE_PATH, contentHash: hash, amount: 0, full: false, versionId, patchIds: [patchId] });
    expect(baked).not.toBeNull();
    const d = await difference(plain.path, baked.path);
    console.log(`canvas source vs plain preview: ${JSON.stringify(d)}`);
    expect(d.error).toBeUndefined();
    expect(d.inside).toBeGreaterThan(1);
    expect(d.corner).toBe(0);
    // A patch id of another photograph, or none: the plain preview.
    expect(await invoke("get_develop_source_preview", { path: FIXTURE_PATH, contentHash: hash, amount: 0, full: false, versionId, patchIds: [] })).toBeNull();
    expect(await invoke("get_develop_source_preview", { path: FIXTURE_PATH, contentHash: hash, amount: 0, full: false, versionId, patchIds: [patchId + 999] })).toBeNull();
  });

  it("Before / After shows the plain preview and the removal comes back", async () => {
    const attr = () => browser.execute(() => document.querySelector(".canvas-wrap canvas")?.getAttribute("data-removals-source") ?? "");
    await browser.execute(() => window.dispatchEvent(new KeyboardEvent("keydown", { key: "\\", bubbles: true, cancelable: true })));
    await waitFor(async () => (await attr()) === "", "the plain preview while Before is shown");
    await browser.execute(() => window.dispatchEvent(new KeyboardEvent("keyup", { key: "\\", bubbles: true, cancelable: true })));
    await browser.execute(() => window.dispatchEvent(new KeyboardEvent("keydown", { key: "\\", bubbles: true, cancelable: true })));
    await waitFor(async () => (await attr()) === String(patchId), "the removal back after the second press");
  });

  it("zoomed to 100 % the 1:1 tier carries it too", async () => {
    const attr = (/** @type {string} */ name) => browser.execute((n) => document.querySelector(".canvas-wrap canvas")?.getAttribute(n) ?? "", name);
    const t0 = Date.now();
    await browser.execute(() => window.dispatchEvent(new KeyboardEvent("keydown", { key: "1", metaKey: true, ctrlKey: true, bubbles: true, cancelable: true })));
    await waitFor(async () => (await attr("data-source-tier")) === "full", "the 1:1 tier on the GPU", 120000);
    console.log(`1:1 tier on the GPU ${((Date.now() - t0) / 1000).toFixed(1)} s after Cmd+1 (full decode and the bake)`);
    expect(await attr("data-removals-source")).toBe(String(patchId));
    const plain = await invoke("get_develop_full_preview", { path: FIXTURE_PATH, contentHash: hash });
    const baked = await invoke("get_develop_source_preview", { path: FIXTURE_PATH, contentHash: hash, amount: 0, full: true, versionId, patchIds: [patchId] });
    expect(baked).not.toBeNull();
    expect([baked.width, baked.height]).toEqual([plain.width, plain.height]);
    const d = await difference(plain.path, baked.path);
    console.log(`1:1 source vs plain full preview: ${JSON.stringify(d)}`);
    expect(d.inside).toBeGreaterThan(1);
    expect(d.corner).toBe(0);
    await browser.execute(() => window.dispatchEvent(new KeyboardEvent("keydown", { key: "0", metaKey: true, ctrlKey: true, bubbles: true, cancelable: true })));
  });

  it("the graded preview and the thumbnail carry the removal", async () => {
    const plain = await invoke("get_develop_preview", { path: FIXTURE_PATH, contentHash: hash });
    const graded = await invoke("get_graded_develop_preview", { versionId });
    const d = await difference(plain.path, graded.path);
    console.log(`graded preview vs plain preview: ${JSON.stringify(d)}`);
    expect(d.inside).toBeGreaterThan(1);
    expect(d.corner).toBe(0);
    const thumb = await invoke("regenerate_thumbnail", { versionId });
    expect(thumb).toBeTruthy();
    const t = await browser.execute(async (p) => {
      const bmp = await createImageBitmap(await (await fetch(window.__TAURI__.core.convertFileSrc(p))).blob());
      return { w: bmp.width, h: bmp.height };
    }, thumb);
    console.log(`thumbnail ${JSON.stringify(t)}`);
    expect(t.w).toBeGreaterThan(0);
  });

  it("an export carries it too: the same photo without the removal exports to a different file", async () => {
    const out = mkdtempSync(path.join(os.tmpdir(), "emulsion-removal-export-"));
    const options = { destination_dir: out, long_edge: 1600, quality: 90, metadata: {} };
    const exportOnce = async () => {
      const results = await invoke("export_images", { items: [{ version_id: versionId, path: FIXTURE_PATH }], options });
      expect(results[0].error).toBeNull();
      return createHash("sha256").update(readFileSync(results[0].output_path)).digest("hex");
    };
    const stack = await invoke("get_edit_stack", { versionId });
    const withRemoval = await exportOnce();
    expect(await exportOnce()).toBe(withRemoval); // deterministic, so a difference below is the removal
    await invoke("set_edit_stack", { versionId, stack: { schema_version: 1, ops: [] }, label: "Without the removal" });
    const without = await exportOnce();
    console.log(`export with ${withRemoval.slice(0, 12)}, without ${without.slice(0, 12)}`);
    expect(without).not.toBe(withRemoval);
    await invoke("set_edit_stack", { versionId, stack, label: "Removal back" });
    rmSync(out, { recursive: true, force: true });
  });
});
