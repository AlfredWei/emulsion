import path from "node:path";
import { fileURLToPath } from "node:url";
import { openDevelopFor } from "../helpers.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const CORN = path.resolve(__dirname, "../../../test_image/Field-corn-Liechtenstein-landscape.jpg");
const CORN_NAME = "Field-corn-Liechtenstein-landscape";
const PORTRAIT = path.resolve(__dirname, "../../../test_image/Smiling-woman-pink-shirt-portrait.jpg");

/**
 * RFC-0035 B2 in the real window: the temporary "Adaptive (trying out)" block in the History panel's Presets tab resolves a
 * region in Rust (`resolve_adaptive_region`), writes the mask as one labelled flush and replaces it on a second apply.
 * Needs no GPU: it reads the persisted stack, the History rows and the CPU preview of the result. The face case skips
 * when the face models are not installed (this throwaway profile would have to download them).
 */
describe("Develop adaptive presets (RFC-0035 B2)", function () {
  this.timeout(240000);

  const state = /** @type {Record<string, any>} */ ({});

  const invoke = (/** @type {string} */ cmd, /** @type {any} */ args) =>
    browser.execute((c, a) => window.__TAURI__.core.invoke(c, a), cmd, args);

  async function importFixture(/** @type {string} */ file) {
    await invoke("import_files", { paths: [file] });
    const images = await invoke("list_images", {});
    const img = images.find((/** @type {any} */ i) => i.path === file);
    expect(img).toBeTruthy();
    return img;
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

  const stackOf = (/** @type {number} */ vid) => invoke("get_edit_stack", { versionId: vid });
  const masksOf = (/** @type {any} */ s) => s.ops.filter((/** @type {any} */ o) => /_mask$/.test(o.op));
  const history = (/** @type {number} */ vid) => invoke("get_history", { versionId: vid });

  async function setAmount(/** @type {number} */ v) {
    await browser.execute((val) => {
      const el = /** @type {HTMLInputElement} */ (document.querySelector('input[aria-label="Adaptive preset amount"]'));
      el.value = String(val);
      el.dispatchEvent(new Event("input", { bubbles: true }));
    }, v);
  }

  async function openPresetsTab() {
    await domClick(".tab-btn", "Presets");
    await waitFor(() => browser.execute(() => !!document.querySelector(".adaptive-block")), "adaptive block");
  }

  before(async function () {
    await browser.setTimeout({ script: 120000 });
    state.corn = await importFixture(CORN);
    await browser.refresh();
    await openDevelopFor(CORN_NAME);
    await (await $("#exposure")).waitForExist({ timeout: 20000 });
    await openPresetsTab();
  });

  it("Cool the shadows adds one luminance-range mask under one History row, scaled by Amount", async function () {
    const vid = state.corn.version_id;
    const before = (await history(vid)).length;
    await setAmount(60);
    await domClick(".adaptive-btn", "Cool the shadows");
    const stack = await waitFor(async () => {
      const s = await stackOf(vid);
      return masksOf(s).length === 1 ? s : null;
    }, "shadow mask persisted");
    const m = masksOf(stack)[0];
    expect(m.op).toBe("luminance_range_mask");
    expect(m.rangeMin).toBe(0);
    expect(m.temperature).toBeCloseTo(-4.8, 5);
    expect(m.saturation).toBeCloseTo(-3, 5);
    expect(m.source.preset).toBe("cool-shadows");
    const rows = await history(vid);
    expect(rows.length).toBe(before + 1);
    expect(rows[rows.length - 1].label).toBe("Adaptive: Cool the shadows 60 %");
    state.shadowRange = m.rangeMax;
  });

  it("applying again replaces the mask instead of stacking a second one", async function () {
    const vid = state.corn.version_id;
    await setAmount(100);
    await domClick(".adaptive-btn", "Cool the shadows");
    const stack = await waitFor(async () => {
      const s = await stackOf(vid);
      const m = masksOf(s)[0];
      return m && m.temperature === -8 ? s : null;
    }, "re-applied at 100 %");
    expect(masksOf(stack)).toHaveLength(1);
  });

  it("the shadows mask is stored as measured on the original (RFC-0036)", async function () {
    const m = masksOf(await stackOf(state.corn.version_id)).find((/** @type {any} */ o) => o.op === "luminance_range_mask");
    expect(m.basis).toBe("original");
  });

  it("the shadows mask is listed in the tool strip, and selecting it opens the panel on Original (RFC-0036)", async function () {
    // A preset mask carries only the adjustments the preset sets; the panel used to throw on its missing exposure.
    const probe = () =>
      browser.execute(() => ({
        chips: document.querySelectorAll(".mask-chip").length,
        selected: document.querySelectorAll(".mask-chip.selected").length,
        basis: /** @type {HTMLSelectElement | null} */ (document.querySelector("#mask-range-basis"))?.value ?? null,
      }));
    await waitFor(async () => (await probe()).chips === 1, "a chip for the resolver's mask");
    await domClick(".mask-chip");
    await waitFor(async () => (await probe()).selected === 1, "the chip selected");
    expect((await probe()).basis).toBe("original");
  });

  it("the panel's Measured on switch changes a drawn luminance mask's basis (RFC-0036)", async function () {
    const vid = state.corn.version_id;
    const lumas = async () => masksOf(await stackOf(vid)).filter((/** @type {any} */ o) => o.op === "luminance_range_mask");
    const shadowId = (await lumas())[0].id;
    // The tool strip creates a mask the user draws and selects it (the path develop-mask-shapes uses).
    await domClick('[aria-label="Luminance Range"]');
    await waitFor(async () => browser.execute(() => !!document.querySelector("#mask-range-basis")), "the Measured on row");
    const shown = () => browser.execute(() => /** @type {HTMLSelectElement} */ (document.querySelector("#mask-range-basis")).value);
    const choose = (/** @type {string} */ v) =>
      browser.execute((value) => {
        const sel = /** @type {HTMLSelectElement} */ (document.querySelector("#mask-range-basis"));
        sel.value = value;
        sel.dispatchEvent(new Event("change", { bubbles: true }));
      }, v);
    const mine = async () => (await lumas()).find((/** @type {any} */ o) => o.id !== shadowId);
    await waitFor(async () => !!(await mine()), "the drawn mask persisted");
    expect(await shown()).toBe("edited");
    expect((await mine()).basis).toBeUndefined();
    await choose("original");
    await waitFor(async () => (await mine()).basis === "original", "switched to Original");
    await choose("edited");
    await waitFor(async () => (await mine()).basis === "edited", "switched back to As edited");
  });

  it("Deepen sky adds a segment mask flagged for a check, and its raster is a valid 256 x 256 field", async function () {
    const vid = state.corn.version_id;
    await setAmount(100);
    await domClick(".adaptive-btn", "Deepen sky");
    const stack = await waitFor(async () => {
      const s = await stackOf(vid);
      return masksOf(s).some((/** @type {any} */ m) => m.op === "segment_mask") ? s : null;
    }, "sky mask persisted");
    const m = masksOf(stack).find((/** @type {any} */ x) => x.op === "segment_mask");
    expect(m.exposure).toBeCloseTo(-0.3, 5);
    expect(m.contrast).toBe(12);
    expect(m.source.preset).toBe("deepen-sky");
    const dims = await browser.execute(async (b64) => {
      const bmp = await createImageBitmap(await (await fetch(`data:image/png;base64,${b64}`)).blob());
      return [bmp.width, bmp.height];
    }, m.logits);
    expect(dims).toEqual([256, 256]);
    const status = await browser.execute(() => document.querySelector("[data-testid='status-message'], .status-message")?.textContent ?? "");
    state.skyStatus = status;
  });

  it("the CPU render of the stack differs from the unmasked one (the masks reach the pixels)", async function () {
    const vid = state.corn.version_id;
    const stack = await stackOf(vid);
    const bare = { ...stack, ops: stack.ops.filter((/** @type {any} */ o) => !/_mask$/.test(o.op)) };
    const a = await invoke("preview_edit_stack", { path: CORN, contentHash: state.corn.content_hash, stack: bare });
    const b = await invoke("preview_edit_stack", { path: CORN, contentHash: state.corn.content_hash, stack });
    console.log(`ADAPTIVE_E2E before=${a.path} after=${b.path}`);
    expect(a.path).not.toBe(b.path);
  });

  it("Brighten faces on a portrait adds a face raster (skips when the face models are absent)", async function () {
    let img;
    try {
      img = await importFixture(PORTRAIT);
      await invoke("detect_faces_for_images", { imageIds: [img.image_id] });
    } catch (/** @type {any} */ e) {
      console.log(`face detection unavailable here, skipping: ${e}`);
      this.skip();
    }
    const faces = await invoke("get_faces_for_image", { imageId: img.image_id });
    if (faces.length === 0) this.skip();
    const r = await invoke("resolve_adaptive_region", { imageId: img.image_id, path: PORTRAIT, contentHash: img.content_hash, region: "face" });
    console.log(`ADAPTIVE_E2E faces=${faces.length} outcome=${r.outcome} coverage=${r.coverage} reason=${r.reason}`);
    expect(["found", "check"]).toContain(r.outcome);
    expect(r.mask.op).toBe("segment_mask");
  });
});
