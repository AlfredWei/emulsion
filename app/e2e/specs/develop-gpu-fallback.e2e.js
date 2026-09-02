import path from "node:path";
import { fileURLToPath } from "node:url";
import { openDevelopFor } from "../helpers.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
// A fourth, distinct fixture from golden-path's/panel-resize's/
// develop-step-nudge's own, for the same catalog-isolation reason those
// specs already document.
const FIXTURE_PATH = path.resolve(__dirname, "../../../test_image/Smiling-woman-pink-shirt-portrait.jpg");
const FIXTURE_NAME = "Smiling-woman-pink-shirt-portrait";

/**
 * M5 Slice 1: when `DevelopCanvas.svelte`'s `initGpu()` fails to acquire a
 * WebGPU device (no `navigator.gpu`, no adapter, or `requestDevice()`
 * rejecting), Develop now falls back to a debounced, CPU-rendered static
 * preview instead of a permanent `status === "error"` overlay -- see
 * `gpuFallback.js`, ADR-0004's dated Update section, and
 * RFC-0002-develop-gpu-cpu-fallback.md.
 *
 * This dev/CI machine has real, working WebGPU (that's the whole reason a
 * CPU fallback needs forcing to exercise at all), so `navigator.gpu` is
 * monkeypatched out immediately after the page loads, before Develop is
 * ever opened -- `initGpu` isn't called until an image is actually opened
 * in Develop, so this is well ahead of that.
 */
describe("Develop GPU/CPU fallback", () => {
  before(async () => {
    await browser.setTimeout({ script: 90000 });
    await browser.execute(
      async (fixturePath) => window.__TAURI__.core.invoke("import_files", { paths: [fixturePath] }),
      FIXTURE_PATH,
    );
    await browser.refresh();

    // Prefer removing `navigator.gpu` outright (the "no-webgpu" branch of
    // classifyGpuFailure); if the property isn't configurable in this
    // WebKit build, fall back to making adapter acquisition fail instead
    // -- `initGpu`'s own `if (!adapter) throw ...` treats that identically
    // (see gpuFallback.js's own doc comment on why device-request-failed
    // is the safe fallback classification either way).
    await browser.execute(() => {
      try {
        Object.defineProperty(window.navigator, "gpu", { value: undefined, configurable: true });
      } catch {
        navigator.gpu.requestAdapter = () => Promise.resolve(null);
      }
    });

    await openDevelopFor(FIXTURE_NAME);
  });

  it("shows the fallback banner and a rendered image instead of erroring", async () => {
    const badge = await $(".cpu-fallback-badge");
    await badge.waitForExist({ timeout: 20000 });
    expect(await badge.getText()).toContain("GPU acceleration unavailable");

    // The debounced first render (see +page.svelte's gpuFallbackTimer
    // effect) needs a moment; poll rather than assume a fixed delay.
    const src = await browser.execute(() => {
      return new Promise((resolve) => {
        const deadline = Date.now() + 15000;
        const poll = () => {
          const img = document.querySelector(".cpu-fallback-image");
          if (img && img.src) return resolve(img.src);
          if (Date.now() > deadline) return resolve(null);
          setTimeout(poll, 200);
        };
        poll();
      });
    });
    expect(src).not.toBeNull();
    expect(src.length).toBeGreaterThan(0);

    // The real WebGPU <canvas> must be hidden, not just covered -- this is
    // "no device ever acquired", not a cosmetic overlay.
    const canvasHidden = await browser.execute(() => {
      const canvas = document.querySelector(".canvas-wrap canvas");
      return canvas ? getComputedStyle(canvas).display === "none" : null;
    });
    expect(canvasHidden).toBe(true);
  });

  it("disables mask/crop tools while GPU is unavailable", async () => {
    const disabledStates = await browser.execute(() => {
      const strip = document.querySelector(".strip");
      const labels = ["Crop & Straighten", "Linear Gradient", "Radial Gradient", "Brush", "Luminance Range", "Color Range", "Spot Removal", "Red Eye Correction"];
      return labels.map((label) => {
        const btn = strip.querySelector(`button[aria-label="${label}"]`);
        return btn ? btn.disabled : null;
      });
    });
    expect(disabledStates.every((d) => d === true)).toBe(true);
  });

  it("re-renders the fallback preview (debounced) after an Exposure edit", async () => {
    const before = await browser.execute(() => document.querySelector(".cpu-fallback-image")?.src ?? null);
    expect(before).not.toBeNull();

    await browser.execute(() => {
      const el = document.getElementById("exposure");
      const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, "value").set;
      setter.call(el, "1.5");
      el.dispatchEvent(new Event("input", { bubbles: true }));
    });

    const after = await browser.execute((prev) => {
      return new Promise((resolve) => {
        const deadline = Date.now() + 15000;
        const poll = () => {
          const img = document.querySelector(".cpu-fallback-image");
          if (img && img.src && img.src !== prev) return resolve(img.src);
          if (Date.now() > deadline) return resolve(null);
          setTimeout(poll, 200);
        };
        poll();
      });
    }, before);
    expect(after).not.toBeNull();
    expect(after).not.toBe(before);
  });
});
