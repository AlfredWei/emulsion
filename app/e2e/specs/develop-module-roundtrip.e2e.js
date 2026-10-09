import path from "node:path";
import { existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { openDevelopFor } from "../helpers.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const FIXTURE_PATH = path.resolve(__dirname, "../../../test_image/Szechenyi-Chain-Bridge-Budapest-night.jpg");
const FIXTURE_NAME = "Szechenyi-Chain-Bridge-Budapest-night";

/**
 * User report (2026-10-09): after switching Library -> Develop, the histogram stays empty and Develop's
 * controls stop working. This spec goes Develop -> Library -> Develop and checks that the histogram fills in
 * again and that a slider still changes it. Needs WebGPU (skips otherwise).
 */
const ROUNDS = Number(process.env.ROUNDTRIP_ROUNDS ?? 3);

describe("Develop after a Library round trip", function () {
  this.timeout(300000);
  let gpuAvailable = false;

  async function waitForDom(/** @type {() => boolean} */ pred, /** @type {string} */ what, ms = 60000) {
    const deadline = Date.now() + ms;
    while (Date.now() < deadline) {
      if (await browser.execute(pred)) return;
      await new Promise((resolve) => setTimeout(resolve, 200));
    }
    throw new Error(`${what}: never appeared within ${ms} ms`);
  }

  const histogramSignature = () =>
    browser.execute(() => Array.from(document.querySelectorAll(".histogram svg polygon.ch")).map((p) => p.getAttribute("points")).join("|"));

  async function titlebar(/** @type {string} */ label) {
    const ok = await browser.execute((l) => {
      const b = Array.from(document.querySelectorAll(".module-switch button")).find((e) => (e.textContent ?? "").trim() === l);
      if (!b) return false;
      /** @type {HTMLElement} */ (b).click();
      return true;
    }, label);
    if (!ok) throw new Error(`no title bar button ${label}`);
  }

  before(async function () {
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
    await browser.execute(async (p) => window.__TAURI__.core.invoke("import_files", { paths: [p] }), FIXTURE_PATH);
    await browser.refresh();
    // Start from no edits, so each slider value below is a real change (the app uses the local catalog).
    await browser.execute(async (name) => {
      const match = (await window.__TAURI__.core.invoke("list_images")).find((i) => i.path.endsWith(`/${name}`));
      await window.__TAURI__.core.invoke("set_edit_stack", { versionId: match.version_id, stack: { schema_version: 1, ops: [] } });
    }, path.basename(FIXTURE_PATH));
    await browser.refresh();
    await openDevelopFor(FIXTURE_NAME);
    await (await $("#exposure")).waitForExist({ timeout: 90000 });
  });

  it("has a histogram on first open", async () => {
    await waitForDom(() => !!document.querySelector(".histogram svg polygon.ch"), "histogram", 90000);
  });

  for (let round = 1; round <= ROUNDS; round++) {
    it(`fills the histogram again after Library and back (round ${round})`, async () => {
      await titlebar("Library");
      await waitForDom(() => !document.querySelector("#exposure"), "Library shown");
      await titlebar("Develop");
      await (await $("#exposure")).waitForExist({ timeout: 90000 });
      await waitForDom(() => !!document.querySelector(".histogram svg polygon.ch"), "histogram after return", 30000);
    });

    it(`a slider still changes the histogram (round ${round})`, async () => {
      const before = await histogramSignature();
      await browser.execute((v) => {
        const el = /** @type {HTMLInputElement} */ (document.querySelector("#exposure"));
        el.value = String(v);
        el.dispatchEvent(new Event("input", { bubbles: true }));
      }, (round % 2 === 0 ? -1 : 1) * (0.5 + round * 0.05));
      const deadline = Date.now() + 15000;
      let after = before;
      while (Date.now() < deadline && after === before) {
        await new Promise((r) => setTimeout(r, 250));
        after = await histogramSignature();
      }
      expect(after).not.toBe(before);
    });
  }
});

/**
 * The same round trip, and the \\ (Before / After) key, on a photo whose AI Denoise is applied (real model; local
 * only: it needs the model installed, a noisy photo and WebGPU). Reports from 2026-10-09: with denoise applied
 * "Original" looked the same as "Edited", and the histogram / controls broke after Library and back.
 */
describe("Develop with AI Denoise applied", function () {
  this.timeout(600000);
  const DN_PATH = path.resolve(__dirname, "../../../test_image/nind-iso6400/chapel_ISO6400_2000.jpg");
  const DN_NAME = "chapel_ISO6400_2000";
  let versionId;

  async function waitForDom(/** @type {() => boolean} */ pred, /** @type {string} */ what, ms = 60000) {
    const deadline = Date.now() + ms;
    while (Date.now() < deadline) {
      if (await browser.execute(pred)) return;
      await new Promise((resolve) => setTimeout(resolve, 200));
    }
    throw new Error(`${what}: never appeared within ${ms} ms`);
  }
  const shown = () => browser.execute(() => document.querySelector(".canvas-wrap canvas")?.getAttribute("data-denoise-source") ?? "");
  const sig = () => browser.execute(() => Array.from(document.querySelectorAll(".histogram svg polygon.ch")).map((p) => p.getAttribute("points")).join("|"));
  const pressBackslash = () =>
    browser.execute(() => {
      window.dispatchEvent(new KeyboardEvent("keydown", { key: "\\", bubbles: true, cancelable: true }));
    });
  async function titlebar(/** @type {string} */ label) {
    await browser.execute((l) => {
      Array.from(document.querySelectorAll(".module-switch button")).find((e) => (e.textContent ?? "").trim() === l)?.click();
    }, label);
  }

  before(async function () {
    if (!existsSync(DN_PATH)) this.skip();
    await browser.setTimeout({ script: 120000 });
    const ok = await browser.execute(async () => {
      if (!navigator.gpu) return false;
      try {
        if (!(await navigator.gpu.requestAdapter())) return false;
      } catch {
        return false;
      }
      return (await window.__TAURI__.core.invoke("denoise_model_status")).state === "ready";
    });
    if (!ok) this.skip();
    await browser.execute((p) => {
      window.__importDone = false;
      window.__TAURI__.core.invoke("import_files", { paths: [p] }).finally(() => (window.__importDone = true));
    }, DN_PATH);
    await waitForDom(() => window.__importDone === true, "import finished", 240000);
    await browser.refresh();
    const match = await browser.execute(async (name) => (await window.__TAURI__.core.invoke("list_images")).find((i) => i.path.endsWith(`/${name}`)) ?? null, path.basename(DN_PATH));
    versionId = match.version_id;
    await browser.execute((h) => window.__TAURI__.core.invoke("denoise_remove_cache", { contentHash: h }), match.content_hash);
    await browser.execute((vid) => window.__TAURI__.core.invoke("set_edit_stack", { versionId: vid, stack: { schema_version: 1, ops: [] } }), versionId);
    await browser.refresh();
    await openDevelopFor(DN_NAME);
    await (await $("#exposure")).waitForExist({ timeout: 90000 });
    await browser.execute(() => Array.from(document.querySelectorAll("summary")).find((e) => /Noise Reduction/i.test(e.textContent ?? ""))?.click());
    await waitForDom(() => !!document.querySelector('[data-testid="ai-denoise"]'), "AI Denoise block");
    await browser.execute(() => document.querySelector('[data-testid="ai-denoise-whole"]').click());
    await waitForDom(() => /Denoised with/.test(document.querySelector('[data-testid="ai-denoise-state"]')?.textContent ?? ""), "job finished", 400000);
    await waitForDom(() => document.querySelector(".canvas-wrap canvas")?.getAttribute("data-denoise-source") === "70", "mix shown", 60000);
  });

  it("Before / After shows the plain photo, then the mix again", async () => {
    await pressBackslash();
    await waitForDom(() => document.querySelector(".canvas-wrap canvas")?.getAttribute("data-denoise-source") === "0", "original = plain source", 15000);
    await pressBackslash();
    await waitForDom(() => document.querySelector(".canvas-wrap canvas")?.getAttribute("data-denoise-source") === "70", "edited = mix", 15000);
  });

  for (let round = 1; round <= 2; round++) {
    it(`histogram and controls work after Library and back (round ${round})`, async () => {
      await titlebar("Library");
      await waitForDom(() => !document.querySelector("#exposure"), "Library shown");
      await titlebar("Develop");
      await (await $("#exposure")).waitForExist({ timeout: 90000 });
      await waitForDom(() => !!document.querySelector(".histogram svg polygon.ch"), "histogram after return", 30000);
      expect(await shown()).toBe("70");
      const before = await sig();
      await browser.execute((v) => {
        const el = document.querySelector("#exposure");
        el.value = String(v);
        el.dispatchEvent(new Event("input", { bubbles: true }));
      }, round === 1 ? 1.5 : -1.5);
      const deadline = Date.now() + 15000;
      let after = before;
      while (Date.now() < deadline && after === before) {
        await new Promise((r) => setTimeout(r, 250));
        after = await sig();
      }
      expect(after).not.toBe(before);
      await pressBackslash();
      await waitForDom(() => document.querySelector(".canvas-wrap canvas")?.getAttribute("data-denoise-source") === "0", "original after return", 15000);
      await pressBackslash();
      await waitForDom(() => document.querySelector(".canvas-wrap canvas")?.getAttribute("data-denoise-source") === "70", "edited after return", 15000);
    });
  }
});

/** Switching Library / Develop while a whole-photo denoise job is running (the job carries on). */
describe("Develop after a Library round trip made while a denoise job runs", function () {
  this.timeout(600000);
  const DN_PATH = path.resolve(__dirname, "../../../test_image/nind-iso6400/chapel_ISO6400_2000.jpg");
  const DN_NAME = "chapel_ISO6400_2000";

  async function waitForDom(/** @type {() => boolean} */ pred, /** @type {string} */ what, ms = 60000) {
    const deadline = Date.now() + ms;
    while (Date.now() < deadline) {
      if (await browser.execute(pred)) return;
      await new Promise((resolve) => setTimeout(resolve, 200));
    }
    throw new Error(`${what}: never appeared within ${ms} ms`);
  }
  const sig = () => browser.execute(() => Array.from(document.querySelectorAll(".histogram svg polygon.ch")).map((p) => p.getAttribute("points")).join("|"));
  async function titlebar(/** @type {string} */ label) {
    await browser.execute((l) => {
      Array.from(document.querySelectorAll(".module-switch button")).find((e) => (e.textContent ?? "").trim() === l)?.click();
    }, label);
  }
  async function checkAlive(/** @type {number} */ value) {
    await (await $("#exposure")).waitForExist({ timeout: 90000 });
    await waitForDom(() => !!document.querySelector(".histogram svg polygon.ch"), "histogram", 40000);
    const before = await sig();
    await browser.execute((v) => {
      const el = document.querySelector("#exposure");
      el.value = String(v);
      el.dispatchEvent(new Event("input", { bubbles: true }));
    }, value);
    const deadline = Date.now() + 15000;
    let after = before;
    while (Date.now() < deadline && after === before) {
      await new Promise((r) => setTimeout(r, 250));
      after = await sig();
    }
    expect(after).not.toBe(before);
  }

  before(async function () {
    if (!existsSync(DN_PATH)) this.skip();
    await browser.setTimeout({ script: 120000 });
    const ok = await browser.execute(async () => {
      if (!navigator.gpu || !(await navigator.gpu.requestAdapter())) return false;
      return (await window.__TAURI__.core.invoke("denoise_model_status")).state === "ready";
    });
    if (!ok) this.skip();
    await browser.refresh();
    const match = await browser.execute(async (name) => (await window.__TAURI__.core.invoke("list_images")).find((i) => i.path.endsWith(`/${name}`)) ?? null, path.basename(DN_PATH));
    await browser.execute((h) => window.__TAURI__.core.invoke("denoise_remove_cache", { contentHash: h }), match.content_hash);
    await browser.execute((vid) => window.__TAURI__.core.invoke("set_edit_stack", { versionId: vid, stack: { schema_version: 1, ops: [] } }), match.version_id);
    await browser.refresh();
    await openDevelopFor(DN_NAME);
    await (await $("#exposure")).waitForExist({ timeout: 90000 });
    await browser.execute(() => Array.from(document.querySelectorAll("summary")).find((e) => /Noise Reduction/i.test(e.textContent ?? ""))?.click());
    await waitForDom(() => !!document.querySelector('[data-testid="ai-denoise"]'), "AI Denoise block");
    await browser.execute(() => document.querySelector('[data-testid="ai-denoise-whole"]').click());
    await waitForDom(() => !!document.querySelector('[data-testid="ai-denoise-progress"]'), "job running");
  });

  it("works after Library and back while the job runs, and again when it has finished", async () => {
    await titlebar("Library");
    await waitForDom(() => !document.querySelector("#exposure"), "Library shown");
    await new Promise((r) => setTimeout(r, 3000));
    await titlebar("Develop");
    await checkAlive(0.9);
    // The job finishes while Develop is open (it may have finished meanwhile; then this is already true).
    await waitForDom(() => /Denoised with/.test(document.querySelector('[data-testid="ai-denoise-state"]')?.textContent ?? ""), "job finished", 400000);
    await checkAlive(-0.9);
    await titlebar("Library");
    await waitForDom(() => !document.querySelector("#exposure"), "Library shown again");
    await titlebar("Develop");
    await checkAlive(0.4);
  });
});
