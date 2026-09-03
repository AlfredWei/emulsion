import path from "node:path";
import { fileURLToPath } from "node:url";
import { openDevelopFor } from "../helpers.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
// Reuses develop-gpu-fallback.e2e.js's own fixture -- that spec only ever
// drives it with `navigator.gpu` monkeypatched OUT (forcing the CPU
// fallback path), so it never exercises a real GPU render on this image,
// and sharing a fixture file (rather than claiming a fifth) doesn't create
// any edit-stack collision risk beyond what this project's own
// reset-to-a-known-baseline precedent already handles.
const FIXTURE_PATH = path.resolve(__dirname, "../../../test_image/Smiling-woman-pink-shirt-portrait.jpg");
const FIXTURE_NAME = "Smiling-woman-pink-shirt-portrait";

// PRD §9 / RFC-0001 §8 / ADR-0004: Develop-panel slider feedback ≤100ms on
// 5-year-old mid-range hardware. This dev machine is current-generation,
// not 5-year-old mid-range -- comfortably passing here is expected and
// necessary, but not sufficient, evidence for the PRD's real target
// hardware. Treat a pass here as "the architecture has headroom on fast
// hardware", not as final confirmation of the PRD target itself.
const BUDGET_MS = 100;

/**
 * M5 Slice 3 (GPU performance validation): this app's core architectural
 * bet (ADR-0004: "decode once in Rust, edit reactively via in-webview
 * WebGPU", chosen specifically to keep the interactive edit loop OFF the
 * cross-process IPC path a Rust-rendered-frame-per-edit design would need)
 * has never actually been measured against the PRD's own ≤100ms slider-
 * feedback target -- M5 Slice 1/2 both built safety nets (CPU fallback,
 * CPU/GPU parity) around this render path without measuring its own speed.
 * This is that measurement, using DevelopCanvas.svelte's own
 * `recordRenderLatency` instrumentation (`window.__developRenderPerf`) --
 * real GPU work, real `device.queue.onSubmittedWorkDone()` completion, not
 * a JS-return-time proxy that would miss GPU-queue backpressure.
 *
 * Catalog-size independence (the other half of MILESTONES.md's M5 exit
 * criterion, "...on a 50k-image catalog...") is deliberately NOT tested
 * here -- the interactive render loop never touches the catalog (per
 * ADR-0004's whole point), so catalog size is architecturally irrelevant
 * to it. That criterion's real risk is the SQLite persistence path
 * (`record_edit_stack`/`list_images` at scale), which
 * `catalog.rs`'s own `catalog_scales_to_50k_images` test measures directly
 * against a real 50k-row in-memory catalog -- no GUI needed, and much
 * faster than seeding 50k rows into this suite's real, reused app-data
 * catalog would be.
 */
describe("Develop GPU render latency", () => {
  let versionId;

  before(async function () {
    await browser.setTimeout({ script: 90000 });

    // Same reasoning as develop-cpu-gpu-parity.e2e.js's own guard: this
    // spec's whole premise is measuring a REAL GPU render, meaningless on
    // a runner where WebGPU isn't available (DevelopCanvas.svelte then
    // takes the CPU-fallback path, which never touches
    // `recordRenderLatency` at all).
    const gpuAvailable = await browser.execute(async () => {
      if (!navigator.gpu) return false;
      try {
        const adapter = await navigator.gpu.requestAdapter();
        return !!adapter;
      } catch {
        return false;
      }
    });
    if (!gpuAvailable) {
      this.skip();
    }

    await browser.execute(
      async (fixturePath) => window.__TAURI__.core.invoke("import_files", { paths: [fixturePath] }),
      FIXTURE_PATH,
    );
    await browser.refresh();

    const match = await browser.execute(async (fixturePath) => {
      const images = await window.__TAURI__.core.invoke("list_images");
      return images.find((img) => img.path === fixturePath) ?? null;
    }, FIXTURE_PATH);
    expect(match).not.toBeNull();
    versionId = match.version_id;

    // Reset to a known-empty stack BEFORE opening Develop -- see
    // develop-cpu-gpu-parity.e2e.js's own comment for why the ordering
    // matters (Develop loads the persisted stack into memory once, on
    // open; resetting after would be silently clobbered by the first
    // edit's flush). Not needed for correctness of the latency numbers
    // themselves, but keeps each local run's accumulated-ops baseline
    // comparable run to run.
    await browser.execute(
      (vid) => window.__TAURI__.core.invoke("set_edit_stack", { versionId: vid, stack: { schema_version: 1, ops: [] } }),
      versionId,
    );

    await openDevelopFor(FIXTURE_NAME);
    const exposureInput = await $("#exposure");
    await exposureInput.waitForExist({ timeout: 20000 });
  });

  /** Sets a range input via the native property setter + a real `input`
   * event -- same technique as develop-step-nudge.e2e.js /
   * develop-cpu-gpu-parity.e2e.js, the one that reliably reaches Svelte's
   * `oninput` in this embedded-WebDriver + WKWebView combination. Waits
   * for `window.__developRenderPerf` to actually grow by one entry rather
   * than a fixed delay -- `recordRenderLatency`'s own
   * `onSubmittedWorkDone()` wait is real async GPU-queue time, not
   * instant. */
  async function nudgeSliderAndMeasure(/** @type {string} */ id, /** @type {number} */ value) {
    const before = await browser.execute(() => (window.__developRenderPerf ?? []).length);
    await browser.execute(
      (elId, v) => {
        const el = document.getElementById(elId);
        const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, "value").set;
        setter.call(el, String(v));
        el.dispatchEvent(new Event("input", { bubbles: true }));
      },
      id,
      value,
    );
    const deadline = Date.now() + 10000;
    while (Date.now() < deadline) {
      const len = await browser.execute(() => (window.__developRenderPerf ?? []).length);
      if (len > before) {
        return browser.execute((i) => window.__developRenderPerf[i], before);
      }
      await new Promise((resolve) => setTimeout(resolve, 50));
    }
    throw new Error(`render-latency sample for #${id}=${value} never appeared within 10s`);
  }

  // Scenarios accumulate onto one growing edit stack (matching
  // develop-cpu-gpu-parity.e2e.js's own reasoning: a realistic multi-op
  // stack is a more honest measurement than isolated single-op ones), and
  // are deliberately chosen to span this pipeline's cost range -- from a
  // cheap global-only grade pass (Exposure) up to the separable H+V blur
  // passes (Sharpen, Luminance NR) that are this shader's most expensive
  // per-pixel work.
  const SCENARIOS = [
    ["exposure", 1.5, "Exposure (global grade only)"],
    ["contrast", 30, "Contrast (global grade only)"],
    ["hsl-green-sat", 60, "HSL green-band saturation"],
    ["sharpen-amount", 60, "Sharpen (separable H+V blur pass)"],
    ["luma-nr-amount", 40, "Luminance noise reduction (separable H+V blur pass)"],
  ];

  it(`every scenario's GPU render completes within the ${BUDGET_MS}ms PRD budget`, async () => {
    /** @type {{ label: string, ms: number }[]} */
    const results = [];
    for (const [id, value, label] of SCENARIOS) {
      const sample = await nudgeSliderAndMeasure(id, value);
      results.push({ label, ms: sample.ms });
    }

    // Logged unconditionally (not just on failure) -- these are the real
    // numbers this slice exists to produce, and e2e output is the only
    // place they're captured.
    // eslint-disable-next-line no-console
    console.log("Develop GPU render latency (ms):", results.map((r) => `${r.label}=${r.ms.toFixed(2)}`).join(", "));

    const over = results.filter((r) => r.ms >= BUDGET_MS);
    if (over.length > 0) {
      throw new Error(
        `${over.length} scenario(s) exceeded the ${BUDGET_MS}ms budget: ${over.map((r) => `${r.label}=${r.ms.toFixed(2)}ms`).join(", ")}`,
      );
    }
  });
});
