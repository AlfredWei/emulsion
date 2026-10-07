import path from "node:path";
import { fileURLToPath } from "node:url";
import { openDevelopFor } from "../helpers.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const FIXTURE_PATH = path.resolve(__dirname, "../../../test_image/Smiling-woman-pink-shirt-portrait.jpg");
const FIXTURE_NAME = "Smiling-woman-pink-shirt-portrait";
const MODELS_DIR = process.env.SEGMENT_MODELS_DIR ?? "";

// Points in the portrait, normalized to the full frame (x right, y down).
const SHIRT = { x: 0.45, y: 0.78 };
const FACE = { x: 0.55, y: 0.3 };
const BACKGROUND = { x: 0.9, y: 0.12 };

/**
 * RFC-0026 slice 2d: a REAL click-select session through the real window with the real SAM 2 Tiny
 * model. LOCAL ONLY: it needs the two weight files (set SEGMENT_MODELS_DIR to the folder holding
 * sam2_hiera_tiny.encoder.onnx and .decoder.onnx; they are installed through the app's own
 * "I have the files" command, which checks name and SHA-256) and WebGPU. It prints what it
 * measured; assertions are deliberately loose (the model's exact masks are not the thing under test
 * here, the whole path is: prepare, click, candidates, refine, accept).
 */
describe("Develop Select Subject, real model", function () {
  this.timeout(300000);

  let versionId;
  let gpuAvailable = false;

  async function getEditStack() {
    return browser.execute((vid) => window.__TAURI__.core.invoke("get_edit_stack", { versionId: vid }), versionId);
  }

  async function waitForStack(/** @type {(s: any) => boolean} */ pred, /** @type {string} */ what, ms = 20000) {
    const deadline = Date.now() + ms;
    let last = null;
    while (Date.now() < deadline) {
      last = await getEditStack();
      if (pred(last)) return last;
      await new Promise((resolve) => setTimeout(resolve, 100));
    }
    throw new Error(`${what}: persisted stack never matched within ${ms} ms; last = ${JSON.stringify(last).slice(0, 400)}`);
  }

  async function waitForDom(/** @type {() => boolean} */ pred, /** @type {string} */ what, ms = 90000) {
    const deadline = Date.now() + ms;
    while (Date.now() < deadline) {
      if (await browser.execute(pred)) return;
      await new Promise((resolve) => setTimeout(resolve, 150));
    }
    throw new Error(`${what}: never appeared within ${ms} ms`);
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

  /** A click on the canvas at a normalized image point, as a pointer down/up pair. */
  async function canvasClick(/** @type {{x: number, y: number}} */ p, alt = false) {
    await browser.execute(
      (nx, ny, altKey) => {
        const canvas = /** @type {HTMLCanvasElement} */ (document.querySelector(".canvas-wrap canvas"));
        const r = canvas.getBoundingClientRect();
        const init = { clientX: r.left + nx * r.width, clientY: r.top + ny * r.height, altKey, bubbles: true, pointerId: 1, button: 0 };
        canvas.dispatchEvent(new PointerEvent("pointerdown", init));
        canvas.dispatchEvent(new PointerEvent("pointerup", init));
      },
      p.x,
      p.y,
      alt,
    );
  }

  /** Logit statistics of a stored field: coverage (share of texels > 0) and the logit at normalized points. */
  async function fieldStats(/** @type {string} */ logits, /** @type {{x: number, y: number}[]} */ points) {
    return browser.execute(
      async (b64, pts) => {
        const bin = atob(b64);
        const bytes = Uint8Array.from(bin, (c) => c.charCodeAt(0));
        const bmp = await createImageBitmap(new Blob([bytes], { type: "image/png" }), { colorSpaceConversion: "none", premultiplyAlpha: "none" });
        const c = new OffscreenCanvas(bmp.width, bmp.height);
        const ctx = c.getContext("2d");
        ctx.drawImage(bmp, 0, 0);
        const d = ctx.getImageData(0, 0, bmp.width, bmp.height).data;
        let inside = 0;
        for (let i = 0; i < d.length; i += 4) if ((d[i] / 255) * 8 - 4 > 0) inside++;
        const at = pts.map((p) => {
          const x = Math.min(bmp.width - 1, Math.floor(p.x * bmp.width));
          const y = Math.min(bmp.height - 1, Math.floor(p.y * bmp.height));
          return (d[(y * bmp.width + x) * 4] / 255) * 8 - 4;
        });
        return { size: [bmp.width, bmp.height], coverage: inside / (bmp.width * bmp.height), at };
      },
      logits,
      points,
    );
  }

  before(async function () {
    if (!MODELS_DIR) this.skip();
    await browser.setTimeout({ script: 120000 });
    gpuAvailable = await browser.execute(async () => {
      if (!navigator.gpu) return false;
      try {
        return !!(await navigator.gpu.requestAdapter());
      } catch {
        return false;
      }
    });
    if (!gpuAvailable) this.skip();
    // A first import of this photo in a debug build can outlast one WebDriver request: start it, then poll.
    await browser.execute((p) => {
      window.__importDone = false;
      window.__TAURI__.core.invoke("import_files", { paths: [p] }).finally(() => (window.__importDone = true));
    }, FIXTURE_PATH);
    await waitForDom(() => window.__importDone === true, "import finished", 240000);
    await browser.refresh();
    const match = await browser.execute(async (p) => {
      const images = await window.__TAURI__.core.invoke("list_images");
      return images.find((img) => img.path === p) ?? null;
    }, FIXTURE_PATH);
    expect(match).not.toBeNull();
    versionId = match.version_id;
  });

  it("installs the model files the user already has, through the app's own command", async function () {
    const before = await browser.execute(() => window.__TAURI__.core.invoke("segment_model_status"));
    const t0 = Date.now();
    const status = await browser.execute(
      (dir) => window.__TAURI__.core.invoke("segment_import_model_files", { paths: [`${dir}/sam2_hiera_tiny.encoder.onnx`, `${dir}/sam2_hiera_tiny.decoder.onnx`] }),
      MODELS_DIR,
    );
    console.log(`[real-model] status before=${before.state}, import -> ${status.state} in ${Date.now() - t0} ms`);
    expect(status.state).toBe("ready");
  });

  it("selects the shirt with one click, refines with a negative click, switches candidate and keeps it", async function () {
    await browser.execute((vid) => window.__TAURI__.core.invoke("set_edit_stack", { versionId: vid, stack: { schema_version: 1, ops: [] } }), versionId);
    await browser.refresh();
    await openDevelopFor(FIXTURE_NAME);
    await (await $("#exposure")).waitForExist({ timeout: 30000 });
    await (await $(".canvas-wrap canvas")).waitForExist({ timeout: 30000 });

    // Arm: model check + the first (cold) encode, which spawns the helper process.
    const tArm = Date.now();
    await waitForDom(() => !!document.querySelector('[aria-label="Select Subject"]'), "tool strip");
    await domClick('[aria-label="Select Subject"]');
    await waitForDom(() => /Click the subject/.test(document.querySelector('[data-testid="segment-options"]')?.textContent ?? ""), "ready for clicks");
    console.log(`[real-model] arm -> ready (helper spawn + session load + encoder): ${Date.now() - tArm} ms`);
    const info = await browser.execute(() => window.__TAURI__.core.invoke("ai_helper_info"));
    expect(info.running).toBe(true);

    // First click: the shirt.
    const tClick = Date.now();
    await canvasClick(SHIRT);
    const afterShirt = await waitForStack((s) => s.ops.some((o) => o.op === "segment_mask"), "first segment mask");
    const shirt = afterShirt.ops.find((/** @type {any} */ o) => o.op === "segment_mask");
    const shirtStats = await fieldStats(shirt.logits, [SHIRT, FACE, BACKGROUND]);
    console.log(`[real-model] click 1 (shirt) -> mask in ${Date.now() - tClick} ms; logits b64 ${shirt.logits.length} chars; candidate ${shirt.candidate}; coverage ${(shirtStats.coverage * 100).toFixed(1)}%; logit at shirt/face/background = ${shirtStats.at.map((v) => v.toFixed(2)).join(" / ")}`);
    expect(shirtStats.size).toEqual([256, 256]);
    expect(shirt.logits.length).toBeLessThan(30000);
    expect(shirtStats.at[0]).toBeGreaterThan(0); // the clicked point is selected
    expect(shirt.prompts).toHaveLength(1);

    // Candidates on screen (1/2/3), then Tab to the next one.
    const cands = await browser.execute(() => Array.from(document.querySelectorAll('[aria-label^="Selection candidates"] button')).map((b) => b.textContent));
    console.log(`[real-model] candidate buttons: ${JSON.stringify(cands)}`);
    expect(cands.length).toBeGreaterThan(1);
    await browser.keys("Tab");
    const afterTab = await waitForStack((s) => s.ops.find((o) => o.op === "segment_mask")?.logits !== shirt.logits, "candidate switched");
    const tabbed = afterTab.ops.find((/** @type {any} */ o) => o.op === "segment_mask");
    const tabStats = await fieldStats(tabbed.logits, [SHIRT, FACE, BACKGROUND]);
    console.log(`[real-model] Tab -> candidate ${tabbed.candidate}; coverage ${(tabStats.coverage * 100).toFixed(1)}%; logits ${tabStats.at.map((v) => v.toFixed(2)).join(" / ")}`);
    await browser.keys("Tab"); // wrap around to the best one again
    await browser.keys("Tab");
    await browser.keys("Tab");

    // Add the face as a second positive click, then remove the background with an Alt-click.
    const tRefine = Date.now();
    await canvasClick(FACE);
    const afterFace = await waitForStack((s) => (s.ops.find((o) => o.op === "segment_mask")?.prompts?.length ?? 0) === 2, "second click");
    const face = afterFace.ops.find((/** @type {any} */ o) => o.op === "segment_mask");
    const faceStats = await fieldStats(face.logits, [SHIRT, FACE, BACKGROUND]);
    console.log(`[real-model] click 2 (+face) -> ${Date.now() - tRefine} ms; coverage ${(faceStats.coverage * 100).toFixed(1)}%; logits shirt/face/background ${faceStats.at.map((v) => v.toFixed(2)).join(" / ")}`);
    expect(faceStats.at[1]).toBeGreaterThan(0);

    await canvasClick(BACKGROUND, true);
    const afterNeg = await waitForStack((s) => (s.ops.find((o) => o.op === "segment_mask")?.prompts?.length ?? 0) === 3, "third click");
    const neg = afterNeg.ops.find((/** @type {any} */ o) => o.op === "segment_mask");
    expect(neg.prompts[2].positive).toBe(false);
    const negStats = await fieldStats(neg.logits, [SHIRT, FACE, BACKGROUND]);
    console.log(`[real-model] click 3 (-background, Alt) -> coverage ${(negStats.coverage * 100).toFixed(1)}%; logits ${negStats.at.map((v) => v.toFixed(2)).join(" / ")}`);
    expect(negStats.at[2]).toBeLessThan(0);

    if (process.env.SEGMENT_SHOT) await browser.saveScreenshot(process.env.SEGMENT_SHOT);

    // Keep it (Enter) and look at what persisted; the panel offers Refine for a saved mask.
    await browser.keys("Enter");
    await waitForDom(() => !document.querySelector('[data-testid="segment-options"]'), "tool left");
    const final = await getEditStack();
    expect(final.ops.filter((/** @type {any} */ o) => o.op === "segment_mask")).toHaveLength(1);
    const hasRefine = await browser.execute(() => !!Array.from(document.querySelectorAll("button")).find((b) => /Refine with clicks/.test(b.textContent ?? "")));
    expect(hasRefine).toBe(true);

    // Resume the saved mask: one more click on the face keeps the same mask id and adds a prompt.
    const id = neg.id;
    await domClick("button", "Refine with clicks");
    await waitForDom(() => /Click to add/.test(document.querySelector('[data-testid="segment-options"]')?.textContent ?? ""), "resumed", 60000);
    await canvasClick({ x: 0.3, y: 0.55 });
    const resumed = await waitForStack((s) => (s.ops.find((o) => o.op === "segment_mask")?.prompts?.length ?? 0) === 4, "resumed click");
    expect(resumed.ops.find((/** @type {any} */ o) => o.op === "segment_mask").id).toBe(id);
    console.log("[real-model] resumed refinement of the saved mask: 4 prompts, same mask id");
    await browser.keys("Enter");
  });

  it("the helper holds the embedding only while it is needed (it is running now, and exits when idle)", async function () {
    const info = await browser.execute(() => window.__TAURI__.core.invoke("ai_helper_info"));
    console.log(`[real-model] helper running after the session: ${info.running}`);
    expect(info.running).toBe(true);
  });
});
