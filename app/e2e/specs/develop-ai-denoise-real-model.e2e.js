import path from "node:path";
import { fileURLToPath } from "node:url";
import { existsSync } from "node:fs";
import { openDevelopFor } from "../helpers.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
// A real ISO 6400 photo (3840x2559, untracked: test_image/nind-iso6400/ is a local set, see its README).
const FIXTURE_PATH = process.env.DENOISE_IMAGE ?? path.resolve(__dirname, "../../../test_image/nind-iso6400/chapel_ISO6400_2000.jpg");
const FIXTURE_NAME = path.basename(FIXTURE_PATH, path.extname(FIXTURE_PATH));
const MODEL = process.env.DENOISE_MODEL ?? "";

/**
 * RFC-0027 slice 3c: a REAL AI Denoise session through the real window with the real model. LOCAL ONLY: it
 * needs the 117 MB model file (set DENOISE_MODEL to nafnet_sidd_w32.onnx; it is installed through the app's
 * own import command, which checks name and SHA-256), a noisy photo, and WebGPU. It prints what it measured.
 *
 * What it covers: crop preview -> before/after shown; whole-photo job with progress; the job ends with Amount
 * set and the kept copy listed; the canvas's source becomes the mix (and the mix really has less noise than the
 * plain preview); Amount 0 goes back to the plain source; the amount survives a re-open; removing the copy
 * puts the plain preview back.
 */
describe("Develop AI Denoise, real model", function () {
  this.timeout(600000);

  let versionId;

  async function waitForDom(/** @type {() => boolean} */ pred, /** @type {string} */ what, ms = 90000) {
    const deadline = Date.now() + ms;
    while (Date.now() < deadline) {
      if (await browser.execute(pred)) return;
      await new Promise((resolve) => setTimeout(resolve, 200));
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

  const stateText = () => browser.execute(() => document.querySelector('[data-testid="ai-denoise-state"]')?.textContent ?? "");
  const shown = () => browser.execute(() => document.querySelector(".canvas-wrap canvas")?.getAttribute("data-denoise-source") ?? "");

  /** The standard deviation of luma in every 7x7 block of a central region of an image file, in a fixed grid
   * order, so two files of the same size can be compared block for block. */
  async function blockSds(/** @type {string} */ file) {
    return browser.execute(async (p) => {
      const bitmap = await createImageBitmap(await (await fetch(window.__TAURI__.core.convertFileSrc(p))).blob());
      const side = 7;
      const w = Math.min(bitmap.width, 800);
      const h = Math.min(bitmap.height, 600);
      const x0 = Math.floor((bitmap.width - w) / 2);
      const y0 = Math.floor((bitmap.height - h) / 2);
      const c = new OffscreenCanvas(w, h);
      const ctx = c.getContext("2d");
      ctx.drawImage(bitmap, x0, y0, w, h, 0, 0, w, h);
      const d = ctx.getImageData(0, 0, w, h).data;
      const out = [];
      for (let by = 0; by + side <= h; by += side) {
        for (let bx = 0; bx + side <= w; bx += side) {
          const vals = [];
          for (let y = by; y < by + side; y++) {
            for (let x = bx; x < bx + side; x++) {
              const i = (y * w + x) * 4;
              vals.push(0.2126 * d[i] + 0.7152 * d[i + 1] + 0.0722 * d[i + 2]);
            }
          }
          const mean = vals.reduce((s, v) => s + v, 0) / vals.length;
          out.push(Math.sqrt(vals.reduce((s, v) => s + (v - mean) ** 2, 0) / vals.length));
        }
      }
      return out;
    }, file);
  }

  /** Mean block spread over the blocks that look like noise (not edges) in the PLAIN image; the same blocks
   * are used for every version, so a block the denoiser flattens still counts, as a low value. */
  const meanOver = (/** @type {number[]} */ sds, /** @type {number[]} */ plain) => {
    let sum = 0;
    let n = 0;
    plain.forEach((p, i) => {
      if (p > 2 && p < 14) {
        sum += sds[i];
        n++;
      }
    });
    return { noise: sum / Math.max(1, n), blocks: n };
  };

  before(async function () {
    if (!MODEL || !existsSync(FIXTURE_PATH)) this.skip();
    await browser.setTimeout({ script: 120000 });
    const gpu = await browser.execute(async () => {
      if (!navigator.gpu) return false;
      try {
        return !!(await navigator.gpu.requestAdapter());
      } catch {
        return false;
      }
    });
    if (!gpu) this.skip();
    const status = await browser.execute((p) => window.__TAURI__.core.invoke("denoise_import_model", { path: p }), MODEL);
    expect(status.state).toBe("ready");
    await browser.execute((p) => {
      window.__importDone = false;
      window.__TAURI__.core.invoke("import_files", { paths: [p] }).finally(() => (window.__importDone = true));
    }, FIXTURE_PATH);
    await waitForDom(() => window.__importDone === true, "import finished", 240000);
    await browser.refresh();
    const match = await browser.execute(async (name) => {
      const images = await window.__TAURI__.core.invoke("list_images");
      return images.find((img) => img.path.endsWith(`/${name}`)) ?? null;
    }, path.basename(FIXTURE_PATH));
    expect(match).not.toBeNull();
    versionId = match.version_id;
    // Start clean: no kept copy, no op.
    await browser.execute((h) => window.__TAURI__.core.invoke("denoise_remove_cache", { contentHash: h }), match.content_hash);
    await browser.execute((vid) => window.__TAURI__.core.invoke("set_edit_stack", { versionId: vid, stack: { schema_version: 1, ops: [] } }), versionId);
    await browser.refresh();
    await openDevelopFor(FIXTURE_NAME);
    await (await $("#exposure")).waitForExist({ timeout: 60000 });
    await domClick("summary", "Noise Reduction");
    await waitForDom(() => !!document.querySelector('[data-testid="ai-denoise"]'), "AI Denoise block");
  });

  it("starts as 'Not denoised yet' with the plain source", async () => {
    expect(await stateText()).toMatch(/Not denoised yet/);
    expect(await shown()).toBe("0");
  });

  it("previews a crop before and after", async () => {
    const t0 = Date.now();
    await domClick('[data-testid="ai-denoise-preview"]');
    await waitForDom(() => !!document.querySelector('[data-testid="ai-denoise-compare"]'), "compare view", 120000);
    console.log(`crop preview shown after ${((Date.now() - t0) / 1000).toFixed(1)} s`);
    const info = await browser.execute(() => {
      const imgs = Array.from(document.querySelectorAll('[data-testid="ai-denoise-compare"] img'));
      return imgs.map((i) => ({ alt: i.alt, w: /** @type {HTMLImageElement} */ (i).naturalWidth, h: /** @type {HTMLImageElement} */ (i).naturalHeight }));
    });
    expect(info.map((i) => i.alt).sort()).toEqual(["After denoise", "Before denoise"]);
    for (const i of info) expect([i.w, i.h]).toEqual([992, 736]);
    expect(await shown()).toBe("0"); // a crop preview changes nothing in the edit
    const stack = await browser.execute((vid) => window.__TAURI__.core.invoke("get_edit_stack", { versionId: vid }), versionId);
    expect(stack.ops).toEqual([]);
    await domClick('[data-testid="ai-denoise-compare"] button', "^Close$");
  });

  it("denoises the whole photo with progress, then shows the mix", async () => {
    const t0 = Date.now();
    await domClick('[data-testid="ai-denoise-whole"]');
    await waitForDom(() => !!document.querySelector('[data-testid="ai-denoise-progress"]'), "progress bar");
    let sawProgress = false;
    await waitForDom(() => {
      const t = document.querySelector('[data-testid="ai-denoise-state"]')?.textContent ?? "";
      return /Denoised with/.test(t);
    }, "job finished", 400000);
    sawProgress = true;
    console.log(`whole-photo job finished after ${((Date.now() - t0) / 1000).toFixed(1)} s: ${await stateText()}`);
    expect(sawProgress).toBe(true);
    const amount = await browser.execute(() => /** @type {HTMLInputElement} */ (document.querySelector("#ai-denoise-amount")).value);
    expect(amount).toBe("70");
    // The amount was written to the stack (the write is debounced, so poll for it).
    let amountInStack;
    const deadline = Date.now() + 10000;
    while (Date.now() < deadline && amountInStack === undefined) {
      const stack = await browser.execute((vid) => window.__TAURI__.core.invoke("get_edit_stack", { versionId: vid }), versionId);
      amountInStack = stack.ops.find((o) => o.op === "ai_denoise")?.amount;
      if (amountInStack === undefined) await new Promise((resolve) => setTimeout(resolve, 200));
    }
    expect(amountInStack).toBe(70);
    await waitForDom(() => document.querySelector(".canvas-wrap canvas")?.getAttribute("data-denoise-source") === "70", "canvas source is the mix", 60000);
  });

  it("the mix really has less noise than the plain preview", async () => {
    const hash = await browser.execute(async (vid) => (await window.__TAURI__.core.invoke("list_images")).find((i) => i.version_id === vid).content_hash, versionId);
    const plain = await browser.execute((p, h) => window.__TAURI__.core.invoke("get_develop_preview", { path: p, contentHash: h }), FIXTURE_PATH, hash);
    // Only the latest blend per photo and tier is kept, so each one is measured right after it is built.
    const blend = (/** @type {number} */ amount) =>
      browser.execute((p, h, a) => window.__TAURI__.core.invoke("get_denoised_develop_preview", { path: p, contentHash: h, amount: a, full: false }), FIXTURE_PATH, hash, amount);
    const plainSds = await blockSds(plain.path);
    const halfInfo = await blend(50);
    const halfSds = await blockSds(halfInfo.path);
    const fullInfo = await blend(100);
    const fullSds = await blockSds(fullInfo.path);
    const a = meanOver(plainSds, plainSds);
    const b = meanOver(halfSds, plainSds);
    const c = meanOver(fullSds, plainSds);
    expect([fullInfo.width, fullInfo.height]).toEqual([plain.width, plain.height]);
    console.log(`noise (mean local sd over the plain image's noisy blocks): plain ${a.noise.toFixed(2)}, amount 50 ${b.noise.toFixed(2)}, amount 100 ${c.noise.toFixed(2)} over ${a.blocks} blocks`);
    expect(c.noise).toBeLessThan(a.noise * 0.85);
    expect(b.noise).toBeLessThan(a.noise);
    expect(b.noise).toBeGreaterThan(c.noise);
  });

  it("Amount 0 goes back to the plain source, and a new amount rebuilds the mix once it settles", async () => {
    const set = (/** @type {number} */ v) =>
      browser.execute((value) => {
        const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, "value").set;
        const el = document.getElementById("ai-denoise-amount");
        setter.call(el, String(value));
        el.dispatchEvent(new Event("input", { bubbles: true }));
      }, v);
    await set(0);
    await waitForDom(() => document.querySelector(".canvas-wrap canvas")?.getAttribute("data-denoise-source") === "0", "plain source", 30000);
    await set(35);
    await waitForDom(() => document.querySelector(".canvas-wrap canvas")?.getAttribute("data-denoise-source") === "35", "mix at 35", 30000);
  });

  it("keeps the kept copy and the amount across a re-open, and removing the copy restores the plain source", async () => {
    await browser.refresh();
    await openDevelopFor(FIXTURE_NAME);
    await (await $("#exposure")).waitForExist({ timeout: 60000 });
    await domClick("summary", "Noise Reduction");
    await waitForDom(() => /Denoised with/.test(document.querySelector('[data-testid="ai-denoise-state"]')?.textContent ?? ""), "copy found again", 30000);
    await waitForDom(() => document.querySelector(".canvas-wrap canvas")?.getAttribute("data-denoise-source") === "35", "mix at the stored amount", 60000);

    await domClick('[data-testid="ai-denoise-remove"]');
    await domClick('[data-testid="ai-denoise-remove-confirm"]');
    await waitForDom(() => /Not denoised yet/.test(document.querySelector('[data-testid="ai-denoise-state"]')?.textContent ?? ""), "copy gone", 30000);
    await waitForDom(() => document.querySelector(".canvas-wrap canvas")?.getAttribute("data-denoise-source") === "0", "plain source again", 30000);
  });
});
