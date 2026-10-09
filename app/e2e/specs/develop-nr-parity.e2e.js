import path from "node:path";
import { fileURLToPath } from "node:url";
import { openDevelopFor } from "../helpers.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
// A committed photo with real noise (low-light, see PROGRESS.md's fixture
// notes); its own fixture, like every other Develop spec.
const FIXTURE_PATH = path.resolve(__dirname, "../../../test_image/Szechenyi-Chain-Bridge-Budapest-night.jpg");
const FIXTURE_NAME = "Szechenyi-Chain-Bridge-Budapest-night";

const BLOCK = 7; // pixels per side of the sampled block
// CPU/GPU agreement on how much NR scales the block's noise (on / off).
const RATIO_TOLERANCE = 0.1;

/**
 * Backlog item 15 follow-up: Luminance / Color Noise Reduction were retuned
 * (PR #219) in both the Rust and WGSL constants, but the CPU/GPU parity spec
 * had no NR scenario, so "preview (GPU) and export (CPU) agree" was only
 * checked by reading the shader.
 *
 * Why a block's noise (standard deviation) and not single pixels like
 * develop-cpu-gpu-parity.e2e.js: that spec samples a FLAT patch, where a
 * pixel-or-two offset between the GPU hover readback and the CPU preview
 * grid is harmless. On a noisy photo the same offset lands on a different
 * noise sample, and even with NR off the two disagree by tens of levels
 * (found the hard way: the first version of this spec failed its own
 * NR-off baseline). The spread inside a block barely depends on that offset
 * and is exactly what NR changes.
 *
 * Per scenario: pick the block where the CPU render's NR reduces the noise
 * most (so the comparison is discriminating), then drive the REAL NR sliders
 * (slider -> op -> shader, so the wiring is covered too) and check on the
 * live GPU render that NR scales that block's noise by the same factor the
 * CPU's does (noise with NR on / noise with NR off). A ratio, not absolute
 * values: even with NR off the GPU readback's block spread differs from the
 * CPU's by ~20% (same 1280x715 size, so a sampling-position offset, not a
 * resolution difference), while NR's relative effect is what must agree.
 *
 * Develop is opened ONCE: opening it takes ~80 s in a debug build and each
 * WebDriver request times out at 90 s, so reopening per scenario was both
 * slow and flaky.
 *
 * Needs WebGPU for the live render, so it skips on a runner that has none
 * (CI's), like develop-cpu-gpu-parity.e2e.js; run it locally.
 */
describe("Develop noise reduction CPU/GPU parity", function () {
  this.timeout(300000);

  let versionId;
  let contentHash;
  let canvasEl;
  let gpuAvailable = false;

  before(async () => {
    await browser.setTimeout({ script: 120000 });
    gpuAvailable = await browser.execute(async () => {
      if (!navigator.gpu) return false;
      try {
        return !!(await navigator.gpu.requestAdapter());
      } catch {
        return false;
      }
    });
    if (!gpuAvailable) return;

    await browser.execute(async (p) => window.__TAURI__.core.invoke("import_files", { paths: [p] }), FIXTURE_PATH);
    await browser.refresh();
    // By file name, not full path: the e2e app uses the real local catalog,
    // which dedupes by content, so from a git worktree the fixture is already
    // catalogued under the main checkout's path.
    const match = await browser.execute(async (name) => {
      const images = await window.__TAURI__.core.invoke("list_images");
      return images.find((img) => img.path.endsWith(`/${name}`)) ?? null;
    }, path.basename(FIXTURE_PATH));
    expect(match).not.toBeNull();
    versionId = match.version_id;
    contentHash = match.content_hash;
  });

  const stackWith = (/** @type {any[]} */ ops) => ({ schema_version: 1, ops });

  before(async function () {
    if (!gpuAvailable) return;
    await openWithStack(stackWith([]));
  });

  /** Slider ids for each NR op's fields. */
  const SLIDERS = {
    luma_nr: { amount: "luma-nr-amount", detail: "luma-nr-detail", contrast: "luma-nr-contrast" },
    color_nr: { amount: "color-nr-amount", detail: "color-nr-detail" },
  };

  /** Sets the NR sliders to an op list's values the way a user drag does
   * (native value setter + an `input` event), or back to off with `[]`. */
  async function setNrSliders(/** @type {any[]} */ nrOps) {
    const values = { "luma-nr-amount": 0, "luma-nr-detail": 50, "luma-nr-contrast": 0, "color-nr-amount": 0, "color-nr-detail": 50 };
    for (const op of nrOps) {
      for (const [field, id] of Object.entries(SLIDERS[op.op])) values[id] = op[field];
    }
    await browser.execute((vals) => {
      const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, "value").set;
      for (const [id, v] of Object.entries(vals)) {
        const el = document.getElementById(id);
        setter.call(el, String(v));
        el.dispatchEvent(new Event("input", { bubbles: true }));
      }
    }, values);
    // Let the GPU re-render settle before reading it back: the hover readback
    // accepts two equal consecutive reads, which a stale frame could satisfy.
    await new Promise((resolve) => setTimeout(resolve, 2000));
  }

  async function openWithStack(/** @type {any} */ stack) {
    // Written BEFORE Develop opens (see develop-cpu-gpu-parity.e2e.js for why the order matters).
    await browser.execute((vid, s) => window.__TAURI__.core.invoke("set_edit_stack", { versionId: vid, stack: s }), versionId, stack);
    await browser.refresh();
    await openDevelopFor(FIXTURE_NAME);
    await (await $("#exposure")).waitForExist({ timeout: 20000 });
    canvasEl = await $(".canvas-wrap canvas");
    await canvasEl.waitForExist({ timeout: 20000 });
  }

  async function cpuPreview(/** @type {any} */ stack) {
    return browser.execute(
      (p, hash, s) => window.__TAURI__.core.invoke("preview_edit_stack", { path: p, contentHash: hash, stack: s }),
      FIXTURE_PATH,
      contentHash,
      stack,
    );
  }

  /** In the page: the block (top-left pixel x, y; image width/height) where
   * the NR-on CPU render's noise `metric` is lowest relative to NR-off, plus
   * both noise values. `metric` is "luma" (spread of luma) or "chroma"
   * (spread of R-luma and B-luma). Blocks must carry real noise to qualify. */
  async function bestBlock(/** @type {string} */ offPath, /** @type {string} */ onPath, /** @type {string} */ metric) {
    return browser.execute(
      async (a, b, m, side) => {
        const load = async (p) => {
          const bitmap = await createImageBitmap(await (await fetch(window.__TAURI__.core.convertFileSrc(p))).blob());
          const c = new OffscreenCanvas(bitmap.width, bitmap.height);
          const ctx = c.getContext("2d");
          ctx.drawImage(bitmap, 0, 0);
          return ctx.getImageData(0, 0, bitmap.width, bitmap.height);
        };
        const spread = (img, x0, y0) => {
          const vals = [];
          for (let y = y0; y < y0 + side; y++) {
            for (let x = x0; x < x0 + side; x++) {
              const i = (y * img.width + x) * 4;
              const [r, g, bl] = [img.data[i], img.data[i + 1], img.data[i + 2]];
              const l = 0.2126 * r + 0.7152 * g + 0.0722 * bl;
              if (m === "luma") vals.push(l);
              else vals.push(r - l, bl - l);
            }
          }
          const mean = vals.reduce((s, v) => s + v, 0) / vals.length;
          return Math.sqrt(vals.reduce((s, v) => s + (v - mean) ** 2, 0) / vals.length);
        };
        const off = await load(a);
        const on = await load(b);
        let best = null;
        const margin = 24;
        for (let y = margin; y < off.height - margin - side; y += 7) {
          for (let x = margin; x < off.width - margin - side; x += 7) {
            const sOff = spread(off, x, y);
            // Noisy enough to witness something, but not a real edge/texture:
            // there the spread is dominated by content, not noise.
            if (sOff < 4 || sOff > 14) continue;
            const sOn = spread(on, x, y);
            const gain = sOff - sOn;
            if (!best || gain > best.gain) best = { x, y, sOff, sOn, gain, width: off.width, height: off.height };
          }
        }
        return best;
      },
      offPath,
      onPath,
      metric,
      BLOCK,
    );
  }

  /** The same noise metric over the live GPU render, reading the block's
   * pixels through the hover readback (develop-cpu-gpu-parity.e2e.js's own
   * mechanism). One page call per ROW: a single call for the whole block can
   * outlast WebDriver's 90 s request timeout on a busy machine. */
  async function gpuBlockSpread(/** @type {any} */ block, /** @type {string} */ metric) {
    const vals = [];
    for (let dy = 0; dy < BLOCK; dy++) {
      const row = await browser.execute(
        async (canvas, blk, y, side) => {
          const rect = canvas.getBoundingClientRect();
          const readOne = async (nx, ny) => {
            const clientX = rect.left + nx * rect.width;
            const clientY = rect.top + ny * rect.height;
            let prev = null;
            const deadline = Date.now() + 15000;
            while (Date.now() < deadline) {
              canvas.dispatchEvent(
                new PointerEvent("pointermove", { bubbles: true, cancelable: true, clientX, clientY, isPrimary: true, pointerType: "mouse" }),
              );
              await new Promise((resolve) => setTimeout(resolve, 100));
              const text = document.querySelector(".hover-rgb")?.textContent ?? null;
              if (text && text === prev) {
                const mm = /R(\d+)\s*G(\d+)\s*B(\d+)/.exec(text);
                if (mm) return [Number(mm[1]), Number(mm[2]), Number(mm[3])];
              }
              prev = text;
            }
            return null;
          };
          const out = [];
          for (let dx = 0; dx < side; dx++) {
            const px = await readOne((blk.x + dx + 0.5) / blk.width, (blk.y + y + 0.5) / blk.height);
            if (!px) return null;
            out.push(px);
          }
          return out;
        },
        canvasEl,
        block,
        dy,
        BLOCK,
      );
      if (!row) return null;
      for (const px of row) {
        const l = 0.2126 * px[0] + 0.7152 * px[1] + 0.0722 * px[2];
        if (metric === "luma") vals.push(l);
        else vals.push(px[0] - l, px[2] - l);
      }
    }
    const mean = vals.reduce((s, v) => s + v, 0) / vals.length;
    return Math.sqrt(vals.reduce((s, v) => s + (v - mean) ** 2, 0) / vals.length);
  }

  async function assertNrParity(/** @type {string} */ label, /** @type {any[]} */ nrOps, /** @type {string} */ metric) {
    const offPreview = await cpuPreview(stackWith([]));
    const onPreview = await cpuPreview(stackWith(nrOps));
    const block = await bestBlock(offPreview.path, onPreview.path, metric);
    expect(block).not.toBeNull();
    // Discrimination: NR must clearly reduce the noise on the CPU render, or
    // agreeing there would prove nothing.
    expect(block.sOn).toBeLessThan(block.sOff * 0.8);

    await setNrSliders([]);
    const gpuOff = await gpuBlockSpread(block, metric);
    await setNrSliders(nrOps);
    const gpuOn = await gpuBlockSpread(block, metric);
    await setNrSliders([]);
    if (gpuOff === null || gpuOn === null) throw new Error(`${label}: GPU hover-pixel readback never stabilized`);
    const canvasSize = await browser.execute((c) => `${c.width}x${c.height}`, canvasEl);

    const cpuRatio = block.sOn / block.sOff;
    const gpuRatio = gpuOn / gpuOff;
    const summary = `${label} (${metric}) block@(${block.x},${block.y}) of ${block.width}x${block.height} (GPU canvas ${canvasSize}): CPU off=${block.sOff.toFixed(2)} on=${block.sOn.toFixed(2)} ratio=${cpuRatio.toFixed(3)} | GPU off=${gpuOff.toFixed(2)} on=${gpuOn.toFixed(2)} ratio=${gpuRatio.toFixed(3)}`;
    console.log(summary);
    if (!(gpuRatio < 0.85)) throw new Error(`GPU render did not reduce noise like the CPU's: ${summary}`);
    if (Math.abs(gpuRatio - cpuRatio) > RATIO_TOLERANCE) throw new Error(`GPU NR effect differs from CPU's: ${summary}`);
  }

  it("Luminance NR (Amount 100, Detail 50): CPU and GPU renders agree on how much it reduces a block's noise", async function () {
    if (!gpuAvailable) this.skip();
    await assertNrParity("Luminance NR", [{ op: "luma_nr", amount: 100, detail: 50, contrast: 0 }], "luma");
  });

  it("Color NR (Amount 100, Detail 50): CPU and GPU renders agree on how much it reduces a block's chroma noise", async function () {
    if (!gpuAvailable) this.skip();
    await assertNrParity("Color NR", [{ op: "color_nr", amount: 100, detail: 50 }], "chroma");
  });
});
