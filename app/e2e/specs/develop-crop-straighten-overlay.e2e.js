import path from "node:path";
import { fileURLToPath } from "node:url";
import { openDevelopFor } from "../helpers.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
// Its own fixture, like every other Develop spec, so this spec's edit-stack
// history is isolated from the others' accumulated catalog.
const FIXTURE_PATH = path.resolve(__dirname, "../../../test_image/Smiling-woman-pink-shirt-portrait.jpg");
const FIXTURE_NAME = "Smiling-woman-pink-shirt-portrait";

// Normalized position of the red-eye mask's centre handle.
const MASK_CENTER = { x: 0.3, y: 0.4 };
const ANGLE = 20;
// Pixels; the handle is a ~12 px square, so its centre is exact to well under this.
const POSITION_TOLERANCE = 3;

/**
 * Crop & Straighten's live preview rotates the photo (the canvas) under a
 * fixed crop rect. Two reported bugs (2026-10-09) were that the overlay
 * chrome did not follow:
 *
 * 1. mask pins/lines/rings (here Red Eye) stayed at their unrotated screen
 *    position while the photo turned under them;
 * 2. the dimming outside the crop rect was four bands over the canvas's
 *    layout box, so the rotated photo's corners poking out of it stayed
 *    undimmed.
 *
 * This checks the geometry in the real window: the Red Eye centre handle
 * sits where rotating its normalized position about the canvas centre says
 * it should (and visibly not where the unrotated overlay would put it),
 * the mask rotor carries the same transform as the canvas, the dimming
 * reaches past the rotated photo's bounding box, and all of it returns to
 * the unrotated layout at angle 0.
 *
 * The mask tools are disabled while WebGPU is unavailable
 * (develop-gpu-fallback.e2e.js), but the Crop tool is not, and Red Eye is
 * only a stored op here -- so this needs WebGPU only for the preview canvas
 * to exist; it skips on a runner that has none (CI's), like the mask specs,
 * so run it locally.
 */
describe("Develop crop straighten overlay", function () {
  this.timeout(240000);

  let versionId;
  let gpuAvailable = false;

  async function waitForDom(/** @type {() => boolean} */ pred, /** @type {string} */ what) {
    const deadline = Date.now() + 20000;
    while (Date.now() < deadline) {
      if (await browser.execute(pred)) return;
      await new Promise((resolve) => setTimeout(resolve, 150));
    }
    throw new Error(`${what}: never appeared within 20s`);
  }

  /** Sets the Crop tool's Angle slider the way a user drag does (native
   * value setter + an `input` event), without a pointer drag. */
  async function setAngle(/** @type {number} */ angle) {
    await browser.execute((a) => {
      const el = /** @type {HTMLInputElement} */ (document.querySelector('input[type="range"][min="-45"][max="45"]'));
      const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, "value").set;
      setter.call(el, String(a));
      el.dispatchEvent(new Event("input", { bubbles: true }));
    }, angle);
    // Svelte applies the new inline transform on the next flush.
    await new Promise((resolve) => setTimeout(resolve, 300));
  }

  /** Everything the assertions need, read in one synchronous page call. */
  async function measure(/** @type {{x: number, y: number}} */ center, /** @type {number} */ angleDeg) {
    return browser.execute(
      (c, a) => {
        const canvas = /** @type {HTMLCanvasElement} */ (document.querySelector(".canvas-wrap canvas"));
        const handle = document.querySelector('[aria-label="Red eye center"]');
        const rotor = document.querySelector(".mask-rotor");
        const dim = document.querySelector(".crop-dim");
        // The rotor is absent before the fix; report that as a transform
        // value so the geometry assertions below fail with their own
        // message instead of a bare null.
        if (!canvas || !handle || !dim) return null;

        // The canvas's layout box is unrotated (offsetWidth/Height); a CSS
        // rotate leaves its centre where it was, so the bounding rect's
        // centre IS the rotation centre.
        const w = canvas.offsetWidth;
        const h = canvas.offsetHeight;
        const box = canvas.getBoundingClientRect();
        const cx = box.left + box.width / 2;
        const cy = box.top + box.height / 2;
        const hr = handle.getBoundingClientRect();

        const rad = (a * Math.PI) / 180;
        const dx = (c.x - 0.5) * w;
        const dy = (c.y - 0.5) * h;
        const at = (/** @type {number} */ cos, /** @type {number} */ sin) => ({
          x: cx + dx * cos - dy * sin,
          y: cy + dx * sin + dy * cos,
        });
        const shadow = getComputedStyle(dim).boxShadow; // e.g. "rgba(0, 0, 0, 0.55) 0px 0px 0px 1000px"
        const spread = Number((shadow.match(/(-?[\d.]+)px(?!.*px)/) ?? [])[1] ?? 0);
        return {
          actual: { x: hr.left + hr.width / 2, y: hr.top + hr.height / 2 },
          expectedRotated: at(Math.cos(rad), Math.sin(rad)),
          expectedUnrotated: at(1, 0),
          rotorTransform: rotor ? getComputedStyle(rotor).transform : "no .mask-rotor element",
          canvasTransform: getComputedStyle(canvas).transform,
          shadowSpread: spread,
          rotatedBoxMax: Math.max(box.width, box.height),
        };
      },
      center,
      angleDeg,
    );
  }

  const distance = (/** @type {{x: number, y: number}} */ p, /** @type {{x: number, y: number}} */ q) =>
    Math.hypot(p.x - q.x, p.y - q.y);

  before(async () => {
    await browser.setTimeout({ script: 90000 });
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
    // By file name, not full path: the catalog dedupes by content, so from a
    // git worktree the fixture is already catalogued under the main
    // checkout's path.
    const match = await browser.execute(async (name) => {
      const images = await window.__TAURI__.core.invoke("list_images");
      return images.find((img) => img.path.endsWith(`/${name}`)) ?? null;
    }, path.basename(FIXTURE_PATH));
    expect(match).not.toBeNull();
    versionId = match.version_id;

    // A known stack written BEFORE Develop opens (see develop-cpu-gpu-parity.e2e.js
    // for why the order matters): one Red Eye mask, no crop.
    await browser.execute(
      (vid, c) =>
        window.__TAURI__.core.invoke("set_edit_stack", {
          versionId: vid,
          stack: {
            schema_version: 1,
            ops: [
              {
                op: "red_eye_mask",
                id: "e2e-red-eye",
                center: c,
                radiusX: 0.05,
                radiusY: 0.05,
                feather: 0.3,
                pupilSize: 50,
                darken: 80,
              },
            ],
          },
        }),
      versionId,
      MASK_CENTER,
    );
    await browser.refresh();
    await openDevelopFor(FIXTURE_NAME);
    await (await $(".canvas-wrap canvas")).waitForExist({ timeout: 20000 });
  });

  it("keeps mask pins on the photo and dims the whole rotated photo while straightening", async function () {
    if (!gpuAvailable) this.skip();

    await waitForDom(() => !!document.querySelector('[aria-label="Crop & Straighten"]'), "tool strip");
    await browser.execute(() => /** @type {HTMLElement} */ (document.querySelector('[aria-label="Crop & Straighten"]')).click());
    await waitForDom(() => !!document.querySelector(".crop-rect"), "crop rect");

    // Angle 0: the overlay sits exactly on the unrotated layout, no rotor transform.
    await setAngle(0);
    let m = await measure(MASK_CENTER, 0);
    expect(m).not.toBeNull();
    expect(distance(m.actual, m.expectedUnrotated)).toBeLessThan(POSITION_TOLERANCE);

    // Straightened: the pin follows the photo's rotation, not the old spot.
    await setAngle(ANGLE);
    m = await measure(MASK_CENTER, ANGLE);
    expect(m).not.toBeNull();
    // The test would be meaningless if rotating barely moved the pin.
    expect(distance(m.expectedRotated, m.expectedUnrotated)).toBeGreaterThan(20);
    expect(distance(m.actual, m.expectedRotated)).toBeLessThan(POSITION_TOLERANCE);
    expect(distance(m.actual, m.expectedUnrotated)).toBeGreaterThan(20);
    // Same rotation as the photo itself.
    expect(m.rotorTransform).not.toBe("none");
    expect(m.rotorTransform).toBe(m.canvasTransform);
    // The dimming spreads beyond the rotated photo's bounding box, so no corner is left undimmed.
    expect(m.shadowSpread).toBeGreaterThanOrEqual(m.rotatedBoxMax);

    // Back to 0: everything returns to the unrotated layout.
    await setAngle(0);
    m = await measure(MASK_CENTER, 0);
    expect(distance(m.actual, m.expectedUnrotated)).toBeLessThan(POSITION_TOLERANCE);
  });
});
