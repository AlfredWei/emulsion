import path from "node:path";
import { fileURLToPath } from "node:url";
import { openDevelopFor } from "../helpers.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const FIXTURE_PATH = path.resolve(__dirname, "../../../test_image/Field-corn-Liechtenstein-landscape.jpg");
const FIXTURE_NAME = "Field-corn-Liechtenstein-landscape";

// Out of 255; same bound and reasoning as develop-cpu-gpu-parity.e2e.js.
const TOLERANCE = 4;

// The flat gray asphalt patch that spec picked by direct neighbourhood
// inspection (normalized u, v). Every weight below is positional, so the
// patch only needs to be flat for the CPU/GPU resample to agree.
const ROAD_PATCH = { u: 0.15, v: 0.96 };

/**
 * RFC-0025 (composable masking): the modifiers fold, end to end in the real
 * app. Two scenarios:
 *
 * 1. CPU/GPU parity for a linear mask carrying all three combine modes. At
 *    ROAD_PATCH the weights are (hand-derived from the shared formulas):
 *    base 0.5; subtract a y-gradient worth 0.5 -> 0.25; add an x-gradient
 *    worth 0.25 -> 0.4375; intersect a soft inverted radial worth 0.75 ->
 *    0.328. Mid-range weights make this a strong witness: a swapped combine,
 *    a dropped modifier or a wrong fold order moves the pixel by tens of
 *    levels at +2 EV. The rgb-reading kinds (luminance/colour range) and the
 *    brush shape are covered by the Rust tests and the real-GPU probe
 *    (docs/rfc/RFC-0025-appendix/gpu_mask_probe.js), not here.
 * 2. The Shapes UI: add a subtract luminance-range shape, change it to
 *    intersect, remove it, checking the persisted edit stack each time.
 *
 * Both need WebGPU (the mask tools are disabled without it), so both skip on
 * a runner that has none -- CI's runners have none, so run them locally.
 *
 * Both start from a known stack written with `set_edit_stack` BEFORE Develop
 * opens (see develop-cpu-gpu-parity.e2e.js for why the order matters).
 */
describe("Develop mask shapes (composable masking)", function () {
  // Each scenario reopens Develop and renders on the CPU path several times in a debug build, which
  // alone takes 50-75 s; the suite-wide 120 s is too close when they run back to back.
  this.timeout(240000);

  let versionId;
  let contentHash;
  let gpuAvailable = false;

  const lin = (x0, y0, x1, y1) => ({
    op: "linear_gradient_mask",
    start: { x: x0, y: y0 },
    end: { x: x1, y: y1 },
    feather: 0,
    invert: false,
  });

  const baseStack = () => ({
    schema_version: 1,
    ops: [
      {
        ...lin(0, 0.5, 0.3, 0.5), // t = 0.5 at u = 0.15
        id: "e2e-base",
        exposure: 2,
        contrast: 0,
        saturation: 0,
        modifiers: [
          { id: "e2e-sub", combine: "subtract", shape: { ...lin(0.5, 0.9, 0.5, 1.02), id: "e2e-sub" } }, // t = 0.5 at v = 0.96
          { id: "e2e-add", combine: "add", shape: { ...lin(0, 0.5, 0.6, 0.5), id: "e2e-add" } }, // t = 0.25
          {
            id: "e2e-int",
            combine: "intersect",
            // d = 0.5, feather 100 -> inside weight 0.75 (invert = effect inside)
            shape: { op: "radial_gradient_mask", id: "e2e-int", center: { x: 0.25, y: 0.96 }, radiusX: 0.2, radiusY: 0.2, feather: 100, invert: true },
          },
        ],
      },
    ],
  });

  async function getEditStack() {
    return browser.execute((vid) => window.__TAURI__.core.invoke("get_edit_stack", { versionId: vid }), versionId);
  }

  /** The stack is flushed to the catalog on a debounce; poll until `pred` holds. */
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

  /** Clicks the first element matching `selector` (optionally also matching `text`), via `execute`. */
  async function domClick(/** @type {string} */ selector, /** @type {RegExp | null} */ text = null) {
    const ok = await browser.execute(
      (sel, src) => {
        const re = src ? new RegExp(src, "i") : null;
        const el = Array.from(document.querySelectorAll(sel)).find((e) => !re || re.test(e.textContent ?? ""));
        if (!el) return false;
        /** @type {HTMLElement} */ (el).click();
        return true;
      },
      selector,
      text ? text.source : null,
    );
    if (!ok) throw new Error(`nothing to click for ${selector}${text ? ` / ${text}` : ""}`);
  }

  async function waitForDom(/** @type {() => boolean} */ pred, /** @type {string} */ what) {
    const deadline = Date.now() + 15000;
    while (Date.now() < deadline) {
      if (await browser.execute(pred)) return;
      await new Promise((resolve) => setTimeout(resolve, 150));
    }
    throw new Error(`${what}: never appeared within 15s`);
  }

  async function readCpuPixel(/** @type {string} */ previewPath, /** @type {number} */ u, /** @type {number} */ v) {
    return browser.execute(
      async (p, nx, ny) => {
        const resp = await fetch(window.__TAURI__.core.convertFileSrc(p));
        const bitmap = await createImageBitmap(await resp.blob());
        const canvas = new OffscreenCanvas(bitmap.width, bitmap.height);
        const ctx = canvas.getContext("2d");
        ctx.drawImage(bitmap, 0, 0);
        const px = Math.min(bitmap.width - 1, Math.floor(nx * bitmap.width));
        const py = Math.min(bitmap.height - 1, Math.floor(ny * bitmap.height));
        const d = ctx.getImageData(px, py, 1, 1).data;
        return { r: d[0], g: d[1], b: d[2] };
      },
      previewPath,
      u,
      v,
    );
  }

  /** The live WebGPU pixel under a synthetic hover (see the parity spec's readGpuPixel). */
  async function readGpuPixel(/** @type {any} */ canvas, /** @type {number} */ u, /** @type {number} */ v) {
    return browser.execute(
      async (c, nx, ny) => {
        const rect = c.getBoundingClientRect();
        const clientX = rect.left + nx * rect.width;
        const clientY = rect.top + ny * rect.height;
        let prev = null;
        const deadline = Date.now() + 15000;
        while (Date.now() < deadline) {
          c.dispatchEvent(new PointerEvent("pointermove", { bubbles: true, cancelable: true, clientX, clientY, isPrimary: true, pointerType: "mouse" }));
          await new Promise((resolve) => setTimeout(resolve, 150));
          const text = document.querySelector(".hover-rgb")?.textContent ?? null;
          if (text && text === prev) {
            const m = /R(\d+)\s*G(\d+)\s*B(\d+)/.exec(text);
            if (m) return { r: Number(m[1]), g: Number(m[2]), b: Number(m[3]) };
          }
          prev = text;
        }
        return null;
      },
      canvas,
      u,
      v,
    );
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

    await browser.execute(async (p) => window.__TAURI__.core.invoke("import_files", { paths: [p] }), FIXTURE_PATH);
    await browser.refresh();
    const match = await browser.execute(async (p) => {
      const images = await window.__TAURI__.core.invoke("list_images");
      return images.find((img) => img.path === p) ?? null;
    }, FIXTURE_PATH);
    expect(match).not.toBeNull();
    versionId = match.version_id;
    contentHash = match.content_hash;
  });

  async function openWithStack(/** @type {any} */ stack) {
    await browser.execute((vid, s) => window.__TAURI__.core.invoke("set_edit_stack", { versionId: vid, stack: s }), versionId, stack);
    await browser.refresh();
    await openDevelopFor(FIXTURE_NAME);
    const exposureInput = await $("#exposure");
    await exposureInput.waitForExist({ timeout: 20000 });
  }

  it("CPU and GPU agree on a mask with subtract, add and intersect shapes", async function () {
    if (!gpuAvailable) this.skip();
    await openWithStack(baseStack());
    const canvasEl = await $(".canvas-wrap canvas");
    await canvasEl.waitForExist({ timeout: 20000 });

    const stack = await getEditStack();
    expect(stack.ops[0].modifiers.map((/** @type {any} */ m) => m.combine)).toEqual(["subtract", "add", "intersect"]);

    const preview = await browser.execute(
      (p, hash, s) => window.__TAURI__.core.invoke("preview_edit_stack", { path: p, contentHash: hash, stack: s }),
      FIXTURE_PATH,
      contentHash,
      stack,
    );
    const cpu = await readCpuPixel(preview.path, ROAD_PATCH.u, ROAD_PATCH.v);
    const gpu = await readGpuPixel(canvasEl, ROAD_PATCH.u, ROAD_PATCH.v);
    if (gpu === null) throw new Error("GPU hover-pixel readback never stabilized");
    for (const ch of /** @type {const} */ (["r", "g", "b"])) {
      const diff = Math.abs(cpu[ch] - gpu[ch]);
      if (diff > TOLERANCE) {
        throw new Error(`mask shapes channel ${ch}: CPU=${cpu[ch]} GPU=${gpu[ch]} diff=${diff} exceeds ${TOLERANCE} | cpu=${JSON.stringify(cpu)} gpu=${JSON.stringify(gpu)}`);
      }
    }

    // The modifiers must actually change the pixel: the same mask with the
    // modifiers stripped (weight 0.5) renders differently on the CPU path.
    const bare = baseStack();
    delete bare.ops[0].modifiers;
    const barePreview = await browser.execute(
      (p, hash, s) => window.__TAURI__.core.invoke("preview_edit_stack", { path: p, contentHash: hash, stack: s }),
      FIXTURE_PATH,
      contentHash,
      bare,
    );
    const cpuBare = await readCpuPixel(barePreview.path, ROAD_PATCH.u, ROAD_PATCH.v);
    expect(Math.abs(cpuBare.r - cpu.r) + Math.abs(cpuBare.g - cpu.g) + Math.abs(cpuBare.b - cpu.b)).toBeGreaterThan(12);
  });

  /** A 256x256 8-bit gray PNG (base64) whose byte at (x, y) is `fn(x, y)`, made in the page like
   * the helper's: the stored form of a segment mask's logit field (RFC-0026). */
  async function logitField(/** @type {(x: number, y: number) => number} */ fn) {
    const bytes = [];
    for (let y = 0; y < 256; y++) for (let x = 0; x < 256; x++) bytes.push(Math.max(0, Math.min(255, Math.round(fn(x, y)))));
    return browser.execute(async (data) => {
      const canvas = new OffscreenCanvas(256, 256);
      const ctx = canvas.getContext("2d");
      const img = ctx.createImageData(256, 256);
      for (let i = 0; i < data.length; i++) img.data.set([data[i], data[i], data[i], 255], i * 4);
      ctx.putImageData(img, 0, 0);
      const buf = new Uint8Array(await (await canvas.convertToBlob({ type: "image/png" })).arrayBuffer());
      let bin = "";
      for (let i = 0; i < buf.length; i += 0x8000) bin += String.fromCharCode(...buf.subarray(i, i + 0x8000));
      return btoa(bin);
    }, bytes);
  }

  // RFC-0026 slice 2b: the segment kind end to end. The fields are smooth ramps with the ROAD_PATCH
  // at a mid weight, so a dropped grow/feather, a mis-scaled byte-to-logit step or a missing layer
  // upload moves the pixel by tens of levels, while 1-texel sampling differences stay far below
  // TOLERANCE. Hand-derived weights at the patch: base ramp byte 128 at texel x = 38.4 (u = 0.15)
  // -> logit 0.016, + grow 20 (0.6) -> 0.616, feather 100 (h = 3.7) -> w = 0.583; the subtract
  // segment shape is byte 128 at texel y = 245.76 (v = 0.96) -> w = 0.502 -> 0.583 * (1 - 0.502).
  it("CPU and GPU agree on a segment mask with a subtract segment shape", async function () {
    if (!gpuAvailable) this.skip();
    await browser.execute((vid) => window.__TAURI__.core.invoke("set_edit_stack", { versionId: vid, stack: { schema_version: 1, ops: [] } }), versionId);
    const baseField = await logitField((x) => 128 + (x - 38.4) * 3);
    const shapeField = await logitField((_x, y) => 128 + (y - 245.76) * 3);
    const stack = {
      schema_version: 1,
      ops: [
        {
          op: "segment_mask",
          id: "e2e-seg",
          logits: baseField,
          feather: 100,
          grow: 20,
          invert: false,
          exposure: 2,
          contrast: 0,
          saturation: 0,
          modifiers: [{ id: "e2e-seg-sub", combine: "subtract", shape: { op: "segment_mask", logits: shapeField, feather: 100, grow: 0, invert: false } }],
        },
      ],
    };
    await openWithStack(stack);
    const canvasEl = await $(".canvas-wrap canvas");
    await canvasEl.waitForExist({ timeout: 20000 });

    const persisted = await getEditStack();
    expect(persisted.ops[0].op).toBe("segment_mask");
    expect(persisted.ops[0].modifiers[0].shape.logits).toBe(shapeField);

    const preview = await browser.execute(
      (p, hash, s) => window.__TAURI__.core.invoke("preview_edit_stack", { path: p, contentHash: hash, stack: s }),
      FIXTURE_PATH,
      contentHash,
      persisted,
    );
    const cpu = await readCpuPixel(preview.path, ROAD_PATCH.u, ROAD_PATCH.v);
    const gpu = await readGpuPixel(canvasEl, ROAD_PATCH.u, ROAD_PATCH.v);
    if (gpu === null) throw new Error("GPU hover-pixel readback never stabilized");
    for (const ch of /** @type {const} */ (["r", "g", "b"])) {
      const diff = Math.abs(cpu[ch] - gpu[ch]);
      if (diff > TOLERANCE) {
        throw new Error(`segment mask channel ${ch}: CPU=${cpu[ch]} GPU=${gpu[ch]} diff=${diff} exceeds ${TOLERANCE} | cpu=${JSON.stringify(cpu)} gpu=${JSON.stringify(gpu)}`);
      }
    }

    // Not a vacuous pass: the same mask without its subtract shape (weight 0.583, not 0.29) renders differently.
    const bare = JSON.parse(JSON.stringify(persisted));
    delete bare.ops[0].modifiers;
    const barePreview = await browser.execute(
      (p, hash, s) => window.__TAURI__.core.invoke("preview_edit_stack", { path: p, contentHash: hash, stack: s }),
      FIXTURE_PATH,
      contentHash,
      bare,
    );
    const cpuBare = await readCpuPixel(barePreview.path, ROAD_PATCH.u, ROAD_PATCH.v);
    expect(Math.abs(cpuBare.r - cpu.r) + Math.abs(cpuBare.g - cpu.g) + Math.abs(cpuBare.b - cpu.b)).toBeGreaterThan(12);
  });

  it("Shapes UI: add a subtract shape, change it to intersect, remove it", async function () {
    // The mask tools are disabled while WebGPU is unavailable (develop-gpu-fallback.e2e.js), as on CI runners.
    if (!gpuAvailable) this.skip();
    const stack = baseStack();
    delete stack.ops[0].modifiers; // a plain mask: the UI adds the shapes itself
    await openWithStack(stack);

    // The tool strip's Luminance Range button creates a mask at once and selects
    // it, which opens the panel (no canvas gesture needed). DOM work goes through
    // `execute` (not `$`): every `$` pays the service's 5 s window-focus tax.
    await waitForDom(() => !!document.querySelector('[aria-label="Luminance Range"]'), "tool strip");
    await domClick('[aria-label="Luminance Range"]');
    await waitForDom(() => !!document.querySelector('[role="dialog"][aria-label$="adjustments"]'), "mask panel");

    const countMasks = (/** @type {any} */ s) => s.ops.filter((/** @type {any} */ o) => String(o.op).endsWith("_mask")).length;
    const afterCreate = await waitForStack((s) => countMasks(s) === 2, "luminance mask created");
    const lumaId = afterCreate.ops.find((/** @type {any} */ o) => o.op === "luminance_range_mask").id;

    // Add shape > Subtract > Luminance range (the one shape created without a canvas gesture).
    await domClick(".addbtn .add");
    await domClick(".addbtn .seg button", /subtract/i);
    await domClick('.addbtn [role="menuitem"]', /luminance/i);
    let s = await waitForStack(
      (st) => (st.ops.find((/** @type {any} */ o) => o.id === lumaId)?.modifiers ?? []).length === 1,
      "shape added",
    );
    let mod = s.ops.find((/** @type {any} */ o) => o.id === lumaId).modifiers[0];
    expect(mod.combine).toBe("subtract");
    expect(mod.shape.op).toBe("luminance_range_mask");

    // Change the combine mode through the row's <select>.
    await browser.execute(() => {
      const sel = /** @type {HTMLSelectElement} */ (document.querySelector(".shapes select.combine"));
      sel.value = "intersect";
      sel.dispatchEvent(new Event("change", { bubbles: true }));
    });
    s = await waitForStack(
      (st) => st.ops.find((/** @type {any} */ o) => o.id === lumaId)?.modifiers?.[0]?.combine === "intersect",
      "combine changed",
    );
    expect(s.ops.find((/** @type {any} */ o) => o.id === lumaId).modifiers[0].id).toBe(mod.id);

    // Remove it: the mask keeps no `modifiers` key at all.
    await domClick('.shapes button[aria-label="Remove shape"]');
    s = await waitForStack((st) => st.ops.find((/** @type {any} */ o) => o.id === lumaId)?.modifiers === undefined, "shape removed");
    expect(Object.hasOwn(s.ops.find((/** @type {any} */ o) => o.id === lumaId), "modifiers")).toBe(false);
  });
});
