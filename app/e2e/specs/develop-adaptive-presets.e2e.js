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
  /** The rows an edit wrote: the first edit of a photograph also records an "Original" row to undo back to. */
  const editRows = (/** @type {any[]} */ rows) => rows.filter((r) => r.label !== "Original");

  async function setAmount(/** @type {number} */ v) {
    await browser.execute((val) => {
      const el = /** @type {HTMLInputElement} */ (document.querySelector('input[aria-label="Adaptive preset amount"]'));
      el.value = String(val);
      el.dispatchEvent(new Event("input", { bubbles: true }));
    }, v);
  }

  /** Picks a preset's row in the Adaptive section, then presses Apply (or Re-apply). */
  async function applyPreset(/** @type {string} */ name) {
    await domClick(".adaptive-row", name);
    await domClick(".adaptive-apply");
  }

  async function openPresetsTab() {
    await domClick(".tab-btn", "Presets");
    await waitFor(() => browser.execute(() => !!document.querySelector(".adaptive-block .adaptive-row")), "adaptive block");
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
    const before = editRows(await history(vid)).length;
    await setAmount(60);
    await applyPreset("Cool the shadows");
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
    const rows = editRows(await history(vid));
    expect(rows.length).toBe(before + 1);
    expect(rows[rows.length - 1].label).toBe("Adaptive: Cool the shadows 60 %");
    state.shadowRange = m.rangeMax;
  });

  it("applying again replaces the mask instead of stacking a second one", async function () {
    const vid = state.corn.version_id;
    await setAmount(100);
    await applyPreset("Cool the shadows");
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

  it("after Apply the new mask is already selected: panel open on Original, so the user sees what was found", async function () {
    const probe = () =>
      browser.execute(() => ({
        chips: document.querySelectorAll(".mask-chip").length,
        selected: document.querySelectorAll(".mask-chip.selected").length,
        basis: /** @type {HTMLSelectElement | null} */ (document.querySelector("#mask-range-basis"))?.value ?? null,
      }));
    await waitFor(async () => (await probe()).selected === 1, "the new mask selected without a click");
    const now = await probe();
    expect(now.chips).toBe(1);
    expect(now.basis).toBe("original");
    // Closing the panel and picking the chip again also works: a preset mask only carries the adjustments it sets,
    // and the panel used to throw on its missing exposure.
    await domClick('[role="dialog"] .close');
    await waitFor(async () => (await probe()).basis === null, "the panel closed");
    await domClick(".mask-chip");
    await waitFor(async () => (await probe()).basis === "original", "the chip reopens the panel on Original");
  });

  it("the section reports what was found, and Apply has become Re-apply", async function () {
    const text = () =>
      browser.execute(() => ({
        lines: Array.from(document.querySelectorAll(".adaptive-report .line")).map((e) => `${e.className}|${e.textContent}`),
        button: document.querySelector(".adaptive-apply")?.textContent?.trim() ?? "",
        rows: Array.from(document.querySelectorAll(".adaptive-row")).map((e) => e.textContent ?? ""),
      }));
    await waitFor(async () => (await text()).lines.length > 0, "a report line");
    const now = await text();
    expect(now.lines[0]).toMatch(/^line (found|check)/);
    expect(now.lines[0]).toContain("Shadows");
    expect(now.button).toBe("Re-apply");
    // Only presets that work are listed: no greyed-out placeholders for ones that are not built.
    expect(now.rows.join(" ")).not.toContain("Needs a click and GPU acceleration");
    expect(now.rows.some((r) => /Cool the shadows/.test(r))).toBe(true);
  });

  it("Re-apply asks before replacing a mask the user changed; Keep mine keeps it, Replace restores the preset", async function () {
    const vid = state.corn.version_id;
    const mine = async () => masksOf(await stackOf(vid)).filter((/** @type {any} */ o) => o.op === "luminance_range_mask");
    const confirmShown = () => browser.execute(() => !!document.querySelector(".adaptive-confirm"));
    // Choosing the row opens its mask (the panel), so the user's change goes into the preset's own mask.
    await domClick(".adaptive-row", "Cool the shadows");
    await waitFor(async () => browser.execute(() => !!document.querySelector("#mask-exposure")), "the mask panel");
    await browser.execute(() => {
      const el = /** @type {HTMLInputElement} */ (document.querySelector("#mask-exposure"));
      el.value = "0.5";
      el.dispatchEvent(new Event("input", { bubbles: true }));
    });
    await waitFor(async () => (await mine())[0].exposure === 0.5, "the user's exposure persisted");
    await domClick(".adaptive-apply");
    await waitFor(confirmShown, "the confirm");
    await domClick(".adaptive-keep");
    await waitFor(async () => !(await confirmShown()), "the confirm closed");
    expect(await mine()).toHaveLength(1);
    expect((await mine())[0].exposure).toBe(0.5);
    await domClick(".adaptive-apply");
    await waitFor(confirmShown, "the confirm again");
    await domClick(".adaptive-replace");
    await waitFor(async () => {
      const m = await mine();
      return m.length === 1 && m[0].exposure === 0 ? m : null;
    }, "the preset's own mask is back (exposure 0)");
  });

  it("the panel's Measured on switch changes a luminance mask's basis (RFC-0036)", async function () {
    const vid = state.corn.version_id;
    const lumas = async () => masksOf(await stackOf(vid)).filter((/** @type {any} */ o) => o.op === "luminance_range_mask");
    const shadowId = (await lumas())[0].id;
    // With GPU acceleration the tool strip draws a mask of the user's own (the path develop-mask-shapes uses). Without it
    // (the CI Windows runner has none) every mask tool is disabled, so the switch is tried on the shadows mask itself.
    const toolDisabled = await browser.execute(() => /** @type {HTMLButtonElement | null} */ (document.querySelector('[aria-label="Luminance Range"]'))?.disabled ?? true);
    if (!toolDisabled) await domClick('[aria-label="Luminance Range"]');
    else await domClick(".mask-chip");
    await waitFor(async () => browser.execute(() => !!document.querySelector("#mask-range-basis")), "the Measured on row");
    const shown = () => browser.execute(() => /** @type {HTMLSelectElement} */ (document.querySelector("#mask-range-basis")).value);
    const choose = (/** @type {string} */ v) =>
      browser.execute((value) => {
        const sel = /** @type {HTMLSelectElement} */ (document.querySelector("#mask-range-basis"));
        sel.value = value;
        sel.dispatchEvent(new Event("change", { bubbles: true }));
      }, v);
    const mine = async () => (await lumas()).find((/** @type {any} */ o) => (toolDisabled ? o.id === shadowId : o.id !== shadowId));
    await waitFor(async () => !!(await mine()), "the mask persisted");
    if (toolDisabled) {
      expect(await shown()).toBe("original");
    } else {
      expect(await shown()).toBe("edited");
      expect((await mine()).basis).toBeUndefined();
    }
    await choose("original");
    await waitFor(async () => (await mine()).basis === "original", "switched to Original");
    await choose("edited");
    await waitFor(async () => (await mine()).basis === "edited", "switched to As edited");
  });

  it("Deepen sky adds a segment mask flagged for a check, and its raster is a valid 256 x 256 field", async function () {
    const vid = state.corn.version_id;
    await setAmount(100);
    await applyPreset("Deepen sky");
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

  it("Add a person yourself: the first click joins the flagged people mask as an added shape, keeps it and its adjustments, and Esc takes it off (local: needs SEGMENT_MODELS_DIR, WebGPU and ADAPTIVE_CHECK_PHOTO)", async function () {
    this.timeout(300000);
    // A flagged (`check`) result is what offers the button, and the repository's own photographs give none: point
    // ADAPTIVE_CHECK_PHOTO at a photograph of several people (Lift people finds some by shape, which is always `check`).
    const modelsDir = process.env.SEGMENT_MODELS_DIR ?? "";
    const photoPath = process.env.ADAPTIVE_CHECK_PHOTO ?? "";
    if (!modelsDir || !photoPath) this.skip();
    const gpu = await browser.execute(async () => {
      try {
        return !!navigator.gpu && !!(await navigator.gpu.requestAdapter());
      } catch {
        return false;
      }
    });
    if (!gpu) this.skip();
    const status = await invoke("segment_import_model_files", { paths: [`${modelsDir}/sam2_hiera_tiny.encoder.onnx`, `${modelsDir}/sam2_hiera_tiny.decoder.onnx`] });
    expect(status.state).toBe("ready");
    const photo = await importFixture(photoPath);
    await browser.refresh();
    await openDevelopFor(path.basename(photoPath).replace(/\.[^.]+$/, ""));
    await (await $("#exposure")).waitForExist({ timeout: 30000 });
    await openPresetsTab();
    await setAmount(100);
    await applyPreset("Lift people");
    const line = await waitFor(() => browser.execute(() => document.querySelector(".adaptive-report .line")?.textContent ?? ""), "a report line", 120000);
    console.log(`SELECT_YOURSELF_E2E ${path.basename(photoPath)}: ${line.trim()}`);
    await waitFor(() => browser.execute(() => !!document.querySelector(".adaptive-yourself")), "the Select it yourself button (the result must be flagged)", 10000);
    const vid = photo.version_id;
    const flagged = masksOf(await stackOf(vid)).find((/** @type {any} */ m) => m.source?.preset === "lift-people");
    expect(flagged).toBeTruthy();
    const rowsBefore = await history(vid);
    // After Apply the new mask is selected and its panel covers the presets list, so the button is also in the panel's note.
    await waitFor(() => browser.execute(() => !!document.querySelector(".check-note + .check-yourself")), "the Select it yourself button in the mask panel", 10000);
    await domClick(".check-yourself");
    await waitFor(() => browser.execute(() => /Click the subject/.test(document.querySelector('[data-testid="segment-options"]')?.textContent ?? "")), "ready for clicks", 120000);
    await browser.execute(() => {
      const canvas = /** @type {HTMLCanvasElement} */ (document.querySelector(".canvas-wrap canvas"));
      const r = canvas.getBoundingClientRect();
      const init = { clientX: r.left + 0.5 * r.width, clientY: r.top + 0.5 * r.height, bubbles: true, pointerId: 1, button: 0 };
      canvas.dispatchEvent(new PointerEvent("pointerdown", init));
      canvas.dispatchEvent(new PointerEvent("pointerup", init));
    });
    // A person mask is several people, so the click is ADDED to them (a shape of the same mask) instead of replacing them all.
    const stack = await waitFor(async () => {
      const s = await stackOf(vid);
      const ms = masksOf(s);
      return ms.length === 1 && ms[0].id === flagged.id && (ms[0].modifiers ?? []).length === 1 ? s : null;
    }, "the click was added to the flagged mask as a shape", 60000);
    const mine = masksOf(stack)[0];
    console.log(`SELECT_YOURSELF_E2E exposure=${mine.exposure} modifiers=${JSON.stringify((mine.modifiers ?? []).map((/** @type {any} */ m) => [m.combine, m.shape.op]))}`);
    expect(mine.modifiers[0].combine).toBe("add");
    expect(mine.modifiers[0].shape.op).toBe("segment_mask");
    expect(mine.logits).toBe(flagged.logits); // the people the model found are untouched
    expect(mine.exposure).toBeCloseTo(flagged.exposure, 5);
    expect(mine.source).toBeTruthy(); // still the preset's mask (now edited, so Re-apply asks before replacing it)
    const rowsAfter = await history(vid);
    console.log(`SELECT_YOURSELF_E2E history rows ${rowsBefore.length} -> ${rowsAfter.length}: ${rowsAfter.slice(rowsBefore.length).map((/** @type {any} */ r) => r.label).join(" | ")}`);
    expect(rowsAfter.length).toBe(rowsBefore.length + 1);
    // Esc takes the added shape off again and leaves the flagged mask as it was.
    await browser.keys(["Escape"]);
    const back = await waitFor(async () => {
      const m = masksOf(await stackOf(vid))[0];
      return m && (m.modifiers ?? []).length === 0 ? m : null;
    }, "Esc removed the added shape", 30000);
    expect(back.id).toBe(flagged.id);
    expect(back.logits).toBe(flagged.logits);
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
