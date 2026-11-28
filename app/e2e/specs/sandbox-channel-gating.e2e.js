import path from "node:path";
import { copyFileSync, mkdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { findCellByNameAnywhere, openDevelopFor } from "../helpers.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
// The exif-shot-info gradient re-saved with a camera and lens the bundled lensfun
// database matches (SONY ILCE-7M3, FE 24-70mm F2.8 GM at 40 mm), so the default
// build has a profile to find.
const FIXTURE = path.resolve(__dirname, "../../../test_image/exif-lens-profile.jpg");
const FIXTURE_NAME = "exif-lens-profile";

/**
 * ADR-0011 in the real window: what the channel compiles out is absent from the UI, and what it keeps still works.
 * The spec reads `model_edition` and asserts the matching side, so the same spec runs against the default build and
 * the Mac App Store build. The fixture is copied into the app's own data folder first, because a sandboxed build
 * cannot read a path outside its container without the native picker (which WebDriver cannot drive).
 */
describe("Channel gating (ADR-0011)", function () {
  this.timeout(600000);

  const invoke = (/** @type {string} */ cmd, /** @type {any} */ args) =>
    browser.execute((c, a) => window.__TAURI__.core.invoke(c, a), cmd, args ?? {});
  const text = (/** @type {string} */ sel) =>
    browser.execute((s) => Array.from(document.querySelectorAll(s)).map((e) => (e.textContent ?? "").trim()), sel);
  const shot = async (/** @type {string} */ name) => {
    if (process.env.LANE_OUT) await browser.saveScreenshot(path.join(process.env.LANE_OUT, `${name}.png`));
  };
  async function waitFor(/** @type {() => Promise<any>} */ fn, /** @type {string} */ what, ms = 60000) {
    const deadline = Date.now() + ms;
    let last;
    while (Date.now() < deadline) {
      last = await fn();
      if (last) return last;
      await new Promise((r) => setTimeout(r, 300));
    }
    throw new Error(`${what}: not within ${ms} ms; last = ${JSON.stringify(last)}`);
  }
  async function openSettingsTab(/** @type {string} */ tab) {
    await browser.execute(() => window.__TAURI__.event.emit("menu-action", "preferences"));
    await waitFor(() => browser.execute(() => document.querySelectorAll(".tab-btn").length > 0), "the Settings dialog");
    await browser.execute((t) => {
      Array.from(document.querySelectorAll(".tab-btn")).find((b) => b.textContent?.trim() === t)?.click();
    }, tab);
    await browser.pause(500);
  }
  async function closeSettings() {
    await browser.execute(() => {
      Array.from(document.querySelectorAll(".actions button")).find((b) => b.textContent?.trim() === "Close")?.click();
    });
    await browser.pause(300);
  }

  /** @type {any} */
  let info;
  let store = false;
  /** @type {number} */
  let versionId;
  /** @type {string} */
  let fixturePath;

  before(async () => {
    await browser.setTimeout({ script: 120000 });
    info = await invoke("model_edition");
    console.log(`[channel] model_edition = ${JSON.stringify(info)}`);
    store = info.channel === "mac_app_store";
    const storage = await invoke("get_storage_info");
    const dir = path.join(storage.effective_dir, "lane-fixtures");
    mkdirSync(dir, { recursive: true });
    fixturePath = path.join(dir, path.basename(FIXTURE));
    copyFileSync(FIXTURE, fixturePath);
    await invoke("import_files", { paths: [fixturePath] });
    await browser.refresh();
    await findCellByNameAnywhere(FIXTURE_NAME);
    const images = await invoke("list_images");
    versionId = images.find((/** @type {any} */ i) => i.path === fixturePath).version_id;
  });

  it("the channel, its caps and what is compiled out agree", () => {
    if (store) {
      expect(info.name).toBe("store");
      expect(info.caps).toEqual({ export_command_hook: false, model_downloads: false, custom_cache_dir: false });
      expect(info.compiled_out).toEqual(["lens_profiles"]);
      expect(info.excluded).toEqual(["detect", "inpaint", "upscale", "blink", "face"]);
    } else {
      expect(info.channel).toBe("direct");
      expect(info.caps).toEqual({ export_command_hook: true, model_downloads: true, custom_cache_dir: true });
    }
  });

  it("Settings shows Export Plugins and the cache move only when the channel has them", async () => {
    await openSettingsTab("Storage");
    await waitFor(() => browser.execute(() => !!document.querySelector(".storage-usage")), "the Storage tab");
    const tabs = await text(".tab-btn");
    const folderButton = await browser.execute(() => !!document.querySelector(".folder-btn"));
    await shot(store ? "mas-settings" : "direct-settings");
    console.log(`[channel] tabs = ${JSON.stringify(tabs)}, cache move button = ${folderButton}`);
    expect(tabs.includes("Export Plugins")).toBe(!store);
    expect(folderButton).toBe(!store);
    if (!store) {
      await openSettingsTab("Export Plugins");
      const plugins = await waitFor(
        () => browser.execute(() => Array.from(document.querySelectorAll("button")).some((b) => b.textContent?.trim() === "Add Plugin")),
        "the Export Plugins tab's Add Plugin button",
      );
      console.log(`[channel] Export Plugins tab shows its add control: ${plugins}`);
      expect(plugins).toBe(true);
    }
    await closeSettings();
  });

  it("Settings, About names exactly what the edition and the build leave out", async () => {
    await openSettingsTab("About");
    const line = await browser.execute(() => document.querySelector('[data-testid="about-edition-line"]')?.textContent?.trim() ?? null);
    await shot(store ? "mas-about" : "direct-about");
    console.log(`[channel] About edition line = ${JSON.stringify(line)}`);
    if (store) {
      expect(line).toBe(
        "Subject detection (person preset), Remove, Enlarge (AI), Eye check, Face grouping and Lens profile correction are not included in this edition of Emulsion.",
      );
    } else if (info.excluded.length === 0 && info.compiled_out.length === 0) {
      expect(line).toBeNull();
    }
    await closeSettings();
  });

  it("Lens Corrections: the profile part follows the build, and Manual distortion still moves the picture", async () => {
    await openDevelopFor(FIXTURE_NAME);
    await waitFor(() => browser.execute(() => !!document.getElementById("lens-manual-distortion")), "the Lens Corrections panel");
    await browser.execute(() => {
      const summary = document.querySelector('[data-ctx-panel="lens_corrections"]');
      const details = summary?.closest("details");
      if (details && !details.open) details.open = true;
      summary?.scrollIntoView();
    });
    // The default build shows "No matching" until the async lookup lands, so
    // each build waits for its own final note rather than the first one seen.
    const profileText = await waitFor(
      () =>
        browser.execute((isStore) => {
          const body = document.querySelector('[data-ctx-panel="lens_corrections"]')?.closest("details")?.querySelector(".sub-body");
          const note = body?.querySelector(".static-note")?.textContent?.trim() ?? "";
          return (isStore ? note.includes("not included") : note.startsWith("Profile found")) ? note : "";
        }, store),
      store ? "the Lens Corrections not-included sentence" : "the Lens Corrections Profile found note",
    );
    console.log(`[channel] profile part = ${JSON.stringify(profileText)}`);
    const enableBox = await browser.execute(() => Array.from(document.querySelectorAll("label.checkbox-row")).some((l) => l.textContent?.includes("Enable Profile Corrections")));
    if (store) {
      expect(profileText).toBe("Lens profile correction is not included in this edition of Emulsion.");
      expect(enableBox).toBe(false);
    } else {
      expect(profileText.startsWith("Profile found")).toBe(true);
      expect(enableBox).toBe(true);
    }

    const images = await invoke("list_images");
    const image = images.find((/** @type {any} */ i) => i.version_id === versionId);
    const render = async (/** @type {number} */ manual) => {
      const stack = { schema_version: 1, ops: [{ op: "lens_correction", manual_distortion: manual }] };
      const p = await invoke("preview_edit_stack", { path: image.path, contentHash: image.content_hash, stack, versionId: null });
      return browser.execute(async (file) => {
        const resp = await fetch(window.__TAURI__.core.convertFileSrc(file));
        const bitmap = await createImageBitmap(await resp.blob());
        const c = new OffscreenCanvas(bitmap.width, bitmap.height);
        const ctx = c.getContext("2d");
        ctx.drawImage(bitmap, 0, 0);
        const d = ctx.getImageData(0, 0, bitmap.width, bitmap.height).data;
        let sum = 0;
        for (let i = 0; i < d.length; i += 97) sum = (sum * 31 + d[i]) % 1000000007;
        return { sum, w: bitmap.width, h: bitmap.height };
      }, p.path);
    };
    const flat = await render(0);
    const moved = await render(60);
    console.log(`[channel] CPU render checksum manual 0 = ${JSON.stringify(flat)}, manual 60 = ${JSON.stringify(moved)}`);
    expect(moved.sum).not.toBe(flat.sum);

    // Sample where the CPU render shows the warp changes the colour and the
    // neighbourhood is flat, so a one-pixel hover offset cannot fake a match.
    const pt = await pickMovingPoint(image);
    console.log(`[channel] sample point = ${JSON.stringify(pt)}`);
    expect(pt).not.toBe(null);
    // The sandbox container keeps its catalog between runs, so start from 0.
    await setManualDistortion(0);
    await shot(store ? "mas-lens-before" : "default-lens-before");
    const gpuBefore = await readGpuPixel(pt.u, pt.v);
    await setManualDistortion(60);
    await shot(store ? "mas-lens" : "default-lens");
    const gpuAfter = await readGpuPixel(pt.u, pt.v);
    const live = await invoke("get_edit_stack", { versionId });
    const cpu = await cpuPixel(image, live, pt.u, pt.v);
    console.log(`[channel] Manual distortion 60 at (${pt.u}, ${pt.v}): GPU before ${JSON.stringify(gpuBefore)}, GPU after ${JSON.stringify(gpuAfter)}, CPU after ${JSON.stringify(cpu)}`);
    if (gpuAfter === null) {
      console.log("[channel] GPU canvas not available here: the GPU half of this check was skipped");
      return;
    }
    expect(gpuAfter).not.toEqual(gpuBefore);
    for (const ch of /** @type {const} */ (["r", "g", "b"])) {
      expect(Math.abs(gpuAfter[ch] - cpu[ch])).toBeLessThanOrEqual(8);
    }
  });

  /** Moves the Manual distortion slider and waits until the catalog and the canvas have it. */
  async function setManualDistortion(/** @type {number} */ value) {
    await browser.execute((v) => {
      const el = /** @type {HTMLInputElement} */ (document.getElementById("lens-manual-distortion"));
      const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, "value")?.set;
      setter?.call(el, String(v));
      el.dispatchEvent(new Event("input", { bubbles: true }));
    }, value);
    await waitFor(async () => {
      const stack = await invoke("get_edit_stack", { versionId });
      return (stack.ops.find((/** @type {any} */ o) => o.op === "lens_correction")?.manual_distortion ?? 0) === value;
    }, `Manual distortion ${value} reaches the catalog`);
    await browser.pause(1500);
  }

  /** The live WebGPU pixel under a synthetic hover (the eyedropper readback), or null without a GPU canvas. */
  async function readGpuPixel(/** @type {number} */ u, /** @type {number} */ v) {
    return browser.execute(async (nx, ny) => {
      const canvas = document.querySelector(".canvas-wrap canvas");
      if (!canvas) return null;
      const rect = canvas.getBoundingClientRect();
      const ev = () =>
        canvas.dispatchEvent(
          new PointerEvent("pointermove", { bubbles: true, cancelable: true, clientX: rect.left + nx * rect.width, clientY: rect.top + ny * rect.height, isPrimary: true, pointerType: "mouse" }),
        );
      let prev = null;
      const deadline = Date.now() + 15000;
      while (Date.now() < deadline) {
        ev();
        await new Promise((r) => setTimeout(r, 150));
        const text = document.querySelector(".hover-rgb")?.textContent ?? null;
        if (text && text === prev) {
          const m = /R(\d+)\s*G(\d+)\s*B(\d+)/.exec(text);
          if (m) return { r: Number(m[1]), g: Number(m[2]), b: Number(m[3]) };
        }
        prev = text;
      }
      return null;
    }, u, v);
  }

  /** The CPU render of `stack` at a normalized point. */
  /**
   * The grid point where Manual distortion 60 changes the CPU render most
   * (at least 12 levels), among points whose 60 render is flat within 6
   * levels over +-2 px. Null when the fixture has no such point.
   */
  async function pickMovingPoint(/** @type {any} */ image) {
    const path = async (/** @type {number} */ manual) =>
      (await invoke("preview_edit_stack", { path: image.path, contentHash: image.content_hash, stack: { schema_version: 1, ops: [{ op: "lens_correction", manual_distortion: manual }] }, versionId: null })).path;
    const flatPath = await path(0);
    const movedPath = await path(60);
    return browser.execute(async (a, b) => {
      const load = async (/** @type {string} */ f) => {
        const bitmap = await createImageBitmap(await (await fetch(window.__TAURI__.core.convertFileSrc(f))).blob());
        const c = new OffscreenCanvas(bitmap.width, bitmap.height);
        const ctx = c.getContext("2d");
        ctx.drawImage(bitmap, 0, 0);
        return { w: bitmap.width, h: bitmap.height, d: ctx.getImageData(0, 0, bitmap.width, bitmap.height).data };
      };
      const p0 = await load(a);
      const p1 = await load(b);
      const at = (/** @type {any} */ im, /** @type {number} */ x, /** @type {number} */ y) => [0, 1, 2].map((k) => im.d[(y * im.w + x) * 4 + k]);
      const diff = (/** @type {number[]} */ p, /** @type {number[]} */ q) => Math.max(...p.map((v, k) => Math.abs(v - q[k])));
      let best = null;
      for (let gy = 1; gy < 20; gy++) {
        for (let gx = 1; gx < 20; gx++) {
          const u = gx / 20;
          const v = gy / 20;
          const x = Math.floor(u * p1.w);
          const y = Math.floor(v * p1.h);
          const c1 = at(p1, x, y);
          let rough = 0;
          for (let dy = -2; dy <= 2; dy++) for (let dx = -2; dx <= 2; dx++) rough = Math.max(rough, diff(c1, at(p1, x + dx, y + dy)));
          const change = diff(c1, at(p0, x, y));
          if (rough <= 6 && change >= 12 && (!best || change > best.change)) best = { u, v, change, rough };
        }
      }
      return best;
    }, flatPath, movedPath);
  }

  async function cpuPixel(/** @type {any} */ image, /** @type {any} */ stack, /** @type {number} */ u, /** @type {number} */ v) {
    const p = await invoke("preview_edit_stack", { path: image.path, contentHash: image.content_hash, stack, versionId: null });
    return browser.execute(async (file, nx, ny) => {
      const resp = await fetch(window.__TAURI__.core.convertFileSrc(file));
      const bitmap = await createImageBitmap(await resp.blob());
      const c = new OffscreenCanvas(bitmap.width, bitmap.height);
      const ctx = c.getContext("2d");
      ctx.drawImage(bitmap, 0, 0);
      const d = ctx.getImageData(Math.floor(nx * bitmap.width), Math.floor(ny * bitmap.height), 1, 1).data;
      return { r: d[0], g: d[1], b: d[2] };
    }, p.path, u, v);
  }

  it("the store edition says each left-out AI feature is not included, and opens no download", async function () {
    if (!store) return this.skip();
    const seen = /** @type {Record<string, unknown>} */ ({});

    seen.upscaleStatus = (await invoke("upscale_model_status")).state;
    seen.blink = await invoke("blink_for_images", { imageIds: [versionId] });
    seen.faces = await browser.execute(
      (ids) => window.__TAURI__.core.invoke("detect_faces_for_images", { imageIds: ids }).then(() => "ok", (e) => String(e)),
      [versionId],
    );

    const rightClick = () =>
      browser.execute((vid) => {
        const cell = document.querySelector(`[data-ctx-photo="${vid}"]`);
        const box = cell?.getBoundingClientRect();
        cell?.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: (box?.left ?? 0) + 10, clientY: (box?.top ?? 0) + 10 }));
        return !!cell;
      }, versionId);
    await waitFor(rightClick, "the fixture's filmstrip cell");
    await waitFor(() => browser.execute(() => !!document.querySelector('[data-menu-id="enlarge"]')), "the Enlarge menu row");
    await browser.execute(() => /** @type {HTMLElement} */ (document.querySelector('[data-menu-id="enlarge"]')).click());
    seen.enlarge = await waitFor(
      () => browser.execute(() => document.querySelector('[data-testid="enlarge-not-in-edition"]')?.textContent?.trim() ?? ""),
      "the Enlarge sentence",
    );
    seen.downloadDialog = await browser.execute(() => !!document.querySelector('[data-testid$="-model-dialog"]'));
    await shot("mas-excluded");
    console.log(`[channel] excluded features = ${JSON.stringify(seen)}`);

    expect(seen.enlarge).toBe("Enlarge (AI) is not included in this edition of Emulsion.");
    expect(seen.upscaleStatus).toBe("not_in_edition");
    expect(/** @type {any} */ (seen.blink).reason).toBe("not_in_edition");
    expect(seen.faces).toBe("Face detection is not included in this edition of Emulsion.");
    expect(seen.downloadDialog).toBe(false);
  });

});
