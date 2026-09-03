import path from "node:path";
import { fileURLToPath } from "node:url";
import { openDevelopFor } from "../helpers.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const FIXTURE_PATH = path.resolve(__dirname, "../../../test_image/Field-corn-Liechtenstein-landscape.jpg");
const FIXTURE_NAME = "Field-corn-Liechtenstein-landscape";

// Out of 255. Covers independent 8-bit quantization on each side (PNG
// encode on the CPU path, rgba8unorm/histogram-texture encode on the GPU
// path) -- a real algorithmic divergence (wrong constant, mis-ported
// formula) would be expected to produce errors far larger than this at the
// adjustment magnitudes used below. If CI ever shows flakiness here,
// widening to 5-6 is the first thing to try before suspecting a real
// regression.
const TOLERANCE = 4;

// A wider bound for HSL/Split Toning specifically -- both read a pixel's
// HUE, which (unlike every other op here) makes their output sensitive to
// exactly which sub-pixel location got sampled, not just to 8-bit
// quantization. `fs_lens_correct`/`fs_perspective` resample via genuine
// bilinear `textureSample` on every render, even with zero correction
// dialed in (see GREEN_PATCH's own doc comment) -- CPU's exact
// nearest-pixel read and GPU's floating-point-reconstructed texel-center
// UV are equal in exact arithmetic but not bit-for-bit, so even a
// deliberately-flattened patch (down to an 11x11-window stdev of ~3-6, the
// flattest naturally-occurring spot this fixture has near a green hue) still
// blends in a sliver of its immediate neighborhood. This is the same
// documented tradeoff `fs_perspective`'s own doc comment already accepts
// for out-of-bounds sampling (interactive preview vs. exact CPU export) --
// widening the bound here rather than chasing an even-flatter patch that
// doesn't exist in this fixture. Still far tighter than a real wrong-
// constant/mis-ported-formula bug would produce at these op magnitudes.
const HUE_SENSITIVE_TOLERANCE = 20;

// Normalized (u, v) image-fraction coordinates of large, genuinely flat
// regions of the fixture -- picked by DIRECT PIXEL-NEIGHBORHOOD INSPECTION
// (an 11x11-window per-channel stdev check), not just by eye, after an
// earlier candidate (a cornfield-interior point that visually looked like
// "flat green field" but was actually individual corn stalks/leaves/shadow
// gaps, with a per-channel stdev of ~30-40 in a 11x11 window) produced a
// reproducible ~30-40/255 false-positive divergence. Root cause: the GPU
// path's `fs_lens_correct`/`fs_perspective` passes do genuine bilinear
// `textureSample` resampling on EVERY render, even with zero lens/
// perspective correction dialed in (there's no "skip this pass" branch --
// see gradeBindGroup's own doc comment in DevelopCanvas.svelte for why
// fs_grade reads perspectiveCorrectedTex, not the raw uploaded texture,
// unconditionally) -- so any fraction-of-a-texel coordinate difference from
// the CPU path's exact nearest-pixel read blends in whatever the immediate
// neighborhood looks like. At a genuinely flat patch that blend is a no-op;
// at a busy one it isn't. Flatness, not "looks like grass", is what this
// coordinate selection is actually for.
const ROAD_PATCH = { u: 0.15, v: 0.96 }; // flat gray asphalt, lower-left
// A shaded cornfield-row gap, not the sunlit foliage: hue~96deg (dead
// center of the green HSL band) and dark/unsaturated-but-past-chroma-fade
// enough for a meaningful green-band test, while staying well clear of the
// exposure+contrast+WB stack's highlight-clip boundary that an earlier,
// brighter green candidate hit (see this file's own header comment) -- at
// full white-balance-and-exposure-boosted brightness, the near-max=1 tie
// between R and G reopens the exact same small-residual sensitivity this
// patch was chosen to avoid.
const GREEN_PATCH = { u: 0.967, v: 0.855 };

/**
 * M5 Slice 2: an automated regression net for the fact that this app has
 * TWO independent implementations of the same "apply an edit stack to
 * pixels" math -- develop_engine.rs (Rust CPU, used by export/thumbnails/
 * the M5 Slice 1 GPU-fallback path) and DevelopCanvas.svelte's WGSL shader
 * (the interactive GPU path). Both are, by design, faithful hand-ports of
 * the same formulas (confirmed by direct comparison across several op
 * families before writing this spec) -- but the only previous safety net
 * was `m1-slice3-smoke/+page.svelte`, a manual dev page never run by any
 * test suite, covering ~40 lines of a shader that has since grown to
 * 1,620. This replaces "hand-reading + doc comments" with a real,
 * automated, every-run check, reusing existing infrastructure only:
 *
 * - The CPU answer comes from `preview_edit_stack` (already-existing
 *   Tauri command, M4.5 Slice 5) -- rendering the REAL live edit stack
 *   (fetched fresh via `get_edit_stack`, never hand-built JSON) to a
 *   draft-resolution PNG, read back via fetch+createImageBitmap+
 *   OffscreenCanvas (same technique `m1-slice3-smoke` and
 *   DevelopCanvas.svelte's own eyedropper source-sampling already use).
 * - The GPU answer comes from the existing eyedropper/hover-pixel
 *   readback (`reportHoverPixel`, DevelopCanvas.svelte) -- it requires no
 *   active tool and fires on any bare pointermove, already wired to
 *   Histogram.svelte's always-mounted `.hover-rgb` readout.
 * - Coordinates line up exactly because `preview_edit_stack`'s CPU render
 *   and the interactive canvas's draft-tier GPU texture both derive from
 *   the SAME `ensure_develop_preview_for_hash` decode at the same
 *   DEVELOP_PREVIEW_MAX_DIMENSION cap -- as long as a scenario has no
 *   crop op (none here do) a normalized (u, v) addresses the same source
 *   pixel on both sides. The GPU path's own geometry-correction passes
 *   (lens/perspective) still run their real bilinear resample even at
 *   zero correction, so "same pixel" only holds exactly at genuinely flat
 *   patches -- see GREEN_PATCH's own doc comment above for why that,
 *   not visual appearance, is what patch selection is actually about.
 *
 * Deliberately excludes Dehaze (develop_engine.rs's own comments document
 * its transmission-recovery as a named, accepted approximation between
 * the two sides -- asserting tight parity on it here would be a false
 * failure, not a real regression) and Spot-heal (a masked/sampled op, not
 * part of "apply an edit stack to pixels" grading at all).
 *
 * Scenarios accumulate (each `it` stacks on the previous edit stack)
 * rather than resetting between each -- both sides always read the exact
 * same live edit stack immediately before comparing, so parity holds
 * regardless of stack depth, and a realistic multi-op stack is a better
 * regression net than isolated single-op stacks.
 */
describe("Develop CPU/GPU parity", () => {
  let versionId;
  let contentHash;
  let canvasEl;

  before(async function () {
    await browser.setTimeout({ script: 90000 });

    // This spec's whole premise is comparing a LIVE GPU render against the
    // CPU one -- meaningless on a runner where WebGPU isn't available, since
    // DevelopCanvas.svelte then takes M5 Slice 1's CPU-fallback path (see
    // develop-gpu-fallback.e2e.js) and there's no GPU render to read a
    // hover-pixel from at all. Confirmed empirically on GitHub Actions'
    // windows-latest runner: `navigator.gpu` init fails there for real (not
    // forced, unlike develop-gpu-fallback.e2e.js's own monkeypatch), which
    // otherwise showed up as every scenario below throwing "GPU hover-pixel
    // readback never stabilized" after a full 15s poll timeout each --
    // replicating `initGpu`'s own adapter-acquisition check directly here,
    // before importing the fixture, skips fast instead of failing slow.
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
    contentHash = match.content_hash;

    // Like develop-step-nudge.e2e.js's own reset-to-a-known-baseline
    // precedent (same class of hazard, its own comment documents the
    // identical root cause): this fixture's catalog row is NOT reset
    // between local runs, so a scenario below could otherwise be setting
    // a value that's ALREADY present from an earlier run -- our own
    // "wait for the persisted stack to actually change" checks
    // (waitForEditStackFlush) would then see no change and time out, a
    // false negative in the harness, not a real app bug. Resetting to a
    // known-empty stack directly via `set_edit_stack` (bypassing the UI
    // reset button/dialog, which isn't needed just to establish a clean
    // starting point) makes every subsequent "did this specific edit
    // actually flush" check meaningful.
    //
    // MUST happen before openDevelopFor: Develop loads the catalog's
    // persisted stack into the page's in-memory `editStack` once, when
    // the view opens. A reset issued AFTER that only overwrites the DB
    // row -- the live in-memory state (and everything it flushes back on
    // the next edit) stays on the pre-reset value, silently clobbering
    // this reset the moment any scenario below makes an edit. Confirmed
    // empirically: an earlier version of this hook reset after opening
    // Develop and every scenario ended up compounding onto ~12 stale ops
    // (including some, e.g. Highlights/Shadows/Whites, this spec never
    // touches) left over from unrelated manual testing of this fixture.
    await browser.execute(
      (vid) => window.__TAURI__.core.invoke("set_edit_stack", { versionId: vid, stack: { schema_version: 1, ops: [] } }),
      versionId,
    );

    await openDevelopFor(FIXTURE_NAME);

    const exposureInput = await $("#exposure");
    await exposureInput.waitForExist({ timeout: 20000 });

    canvasEl = await $(".canvas-wrap canvas");
    await canvasEl.waitForExist({ timeout: 20000 });
  });

  /** Sets a range/number input's value via the native property setter +
   * a real `input` event -- keyboard-driven nudging doesn't reliably reach
   * these inputs in this embedded-WebDriver + WKWebView combination (see
   * golden-path.e2e.js's own comment), and this is what Svelte's
   * `oninput` listens for either way. Works regardless of whether the
   * input's own `<details>` section is currently expanded (confirmed by
   * develop-step-nudge.e2e.js's own precedent for the HSL section). */
  async function getEditStack() {
    return browser.execute(
      (vid) => window.__TAURI__.core.invoke("get_edit_stack", { versionId: vid }),
      versionId,
    );
  }

  /** `handleAdjustmentChange` (+page.svelte) writes the in-memory
   * `editStack` synchronously -- which is what actually drives the GPU
   * canvas's reactive re-render -- but persists it to the SQLite catalog
   * (what `get_edit_stack` reads) via a 250ms-DEBOUNCED `scheduleFlush`.
   * Calling `get_edit_stack` immediately after dispatching an edit races
   * that debounce and can read the STALE, pre-edit stack (confirmed
   * empirically: an early version of this spec did exactly that and got
   * polar-opposite CPU/GPU readings -- not a real rendering divergence,
   * just comparing two different edit stacks). Poll for the catalog to
   * actually change rather than guessing a fixed delay, matching this
   * project's own established "wait for actual change, not a fixed
   * sleep" e2e discipline. */
  async function waitForEditStackFlush(/** @type {string} */ beforeJson) {
    const deadline = Date.now() + 8000;
    while (Date.now() < deadline) {
      const after = await getEditStack();
      if (JSON.stringify(after) !== beforeJson) return after;
      await new Promise((resolve) => setTimeout(resolve, 100));
    }
    throw new Error("edit stack's debounced catalog flush never completed within 8s");
  }

  async function setSliderValue(/** @type {string} */ id, /** @type {number} */ value) {
    const before = JSON.stringify(await getEditStack());
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
    await waitForEditStackFlush(before);
  }

  /** Adds a Tone Curve point via a single click on the graph's background
   * rect, at normalized curve-space (nx, ny) -- mirrors
   * ToneCurveEditor.svelte's own `normalizedFromEvent` math (y inverted:
   * curve-space y grows upward, screen y grows downward). A plain click,
   * not a drag, avoiding any pointer-capture drag fragility. */
  async function addToneCurvePoint(/** @type {number} */ nx, /** @type {number} */ ny) {
    const before = JSON.stringify(await getEditStack());
    const bg = await $("svg.curve rect.bg");
    await browser.execute(
      (el, x, y) => {
        const rect = el.getBoundingClientRect();
        const clientX = rect.left + x * rect.width;
        const clientY = rect.top + (1 - y) * rect.height;
        el.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true, clientX, clientY }));
      },
      bg,
      nx,
      ny,
    );
    await waitForEditStackFlush(before);
  }

  /** Reads a pixel out of a CPU-rendered preview PNG, inside the real
   * webview (fetch + createImageBitmap + OffscreenCanvas.getImageData) --
   * the same technique m1-slice3-smoke and DevelopCanvas.svelte's own
   * eyedropper source-sampling already use. No new Node dependency. */
  async function readCpuPixel(/** @type {string} */ previewPath, /** @type {number} */ u, /** @type {number} */ v) {
    return browser.execute(
      async (p, nx, ny) => {
        const url = window.__TAURI__.core.convertFileSrc(p);
        const resp = await fetch(url);
        const bitmap = await createImageBitmap(await resp.blob());
        const canvas = new OffscreenCanvas(bitmap.width, bitmap.height);
        const ctx = canvas.getContext("2d");
        ctx.drawImage(bitmap, 0, 0);
        const px = Math.min(bitmap.width - 1, Math.floor(nx * bitmap.width));
        const py = Math.min(bitmap.height - 1, Math.floor(ny * bitmap.height));
        const d = ctx.getImageData(px, py, 1, 1).data;
        return { r: d[0], g: d[1], b: d[2], width: bitmap.width, height: bitmap.height };
      },
      previewPath,
      u,
      v,
    );
  }

  /** Reads the live WebGPU-rendered pixel under a synthetic hover, via the
   * existing eyedropper/hover-pixel mechanism (reportHoverPixel,
   * DevelopCanvas.svelte -- requires no active tool, fires on any bare
   * pointermove not mid-drag). The underlying histogram-texture readback
   * this reuses is async and refreshed once per idle frame, so this
   * dispatches+polls in a single browser.execute (matching
   * panel-resize.e2e.js's own established reasoning for why that pays the
   * per-command window-focus tax once, not once per poll iteration) until
   * two consecutive reads agree. */
  async function readGpuPixel(/** @type {number} */ u, /** @type {number} */ v) {
    return browser.execute(
      async (canvas, nx, ny) => {
        const rect = canvas.getBoundingClientRect();
        const clientX = rect.left + nx * rect.width;
        const clientY = rect.top + ny * rect.height;
        const dispatch = () => {
          canvas.dispatchEvent(
            new PointerEvent("pointermove", {
              bubbles: true,
              cancelable: true,
              clientX,
              clientY,
              isPrimary: true,
              pointerType: "mouse",
            }),
          );
        };
        const readText = () => document.querySelector(".hover-rgb")?.textContent ?? null;

        let prev = null;
        const deadline = Date.now() + 15000;
        while (Date.now() < deadline) {
          dispatch();
          await new Promise((resolve) => setTimeout(resolve, 150));
          const text = readText();
          if (text && text === prev) {
            const m = /R(\d+)\s*G(\d+)\s*B(\d+)/.exec(text);
            if (m) return { r: Number(m[1]), g: Number(m[2]), b: Number(m[3]) };
          }
          prev = text;
        }
        return null;
      },
      canvasEl,
      u,
      v,
    );
  }

  /** Fetches the REAL live edit stack (never hand-built JSON) and renders
   * it via the CPU path, then compares against the GPU-rendered value at
   * the same normalized coordinate. */
  async function assertParityAt(
    /** @type {{u: number, v: number}} */ patch,
    /** @type {string} */ label,
    /** @type {number} */ tolerance = TOLERANCE,
  ) {
    const stack = await browser.execute(
      (vid) => window.__TAURI__.core.invoke("get_edit_stack", { versionId: vid }),
      versionId,
    );
    const preview = await browser.execute(
      (p, hash, s) => window.__TAURI__.core.invoke("preview_edit_stack", { path: p, contentHash: hash, stack: s }),
      FIXTURE_PATH,
      contentHash,
      stack,
    );

    const cpu = await readCpuPixel(preview.path, patch.u, patch.v);
    const gpu = await readGpuPixel(patch.u, patch.v);

    // WebdriverIO's `expect()` (expect-webdriverio), unlike Vitest's,
    // doesn't accept a second "message" argument -- plain throws give a
    // more useful failure description than its default output would here.
    if (gpu === null) {
      throw new Error(`${label}: GPU hover-pixel readback never stabilized`);
    }
    for (const ch of /** @type {const} */ (["r", "g", "b"])) {
      const diff = Math.abs(cpu[ch] - gpu[ch]);
      if (diff > tolerance) {
        throw new Error(`${label} channel ${ch}: CPU=${cpu[ch]} GPU=${gpu[ch]} diff=${diff} exceeds tolerance ${tolerance} | full cpu=${JSON.stringify(cpu)} gpu=${JSON.stringify(gpu)}`);
      }
    }
  }

  it("Exposure: CPU and GPU renders agree", async () => {
    await setSliderValue("exposure", 1.5);
    await assertParityAt(ROAD_PATCH, "Exposure");
  });

  it("Contrast: CPU and GPU renders agree", async () => {
    await setSliderValue("contrast", 30);
    await assertParityAt(ROAD_PATCH, "Contrast");
  });

  it("White balance (temperature + tint): CPU and GPU renders agree", async () => {
    await setSliderValue("temperature", 40);
    await setSliderValue("tint", -15);
    await assertParityAt(ROAD_PATCH, "White balance");
  });

  it("HSL green-band saturation: CPU and GPU renders agree", async () => {
    await setSliderValue("hsl-green-sat", 60);
    await assertParityAt(GREEN_PATCH, "HSL green saturation", HUE_SENSITIVE_TOLERANCE);
  });

  it("Tone curve midtone lift: CPU and GPU renders agree", async () => {
    await addToneCurvePoint(0.5, 0.75);
    await assertParityAt(ROAD_PATCH, "Tone curve");
  });

  it("Split toning (shadows hue + saturation): CPU and GPU renders agree", async () => {
    await setSliderValue("st-shadow-hue", 30);
    await setSliderValue("st-shadow-sat", 50);
    await assertParityAt(GREEN_PATCH, "Split toning", HUE_SENSITIVE_TOLERANCE);
  });
});
