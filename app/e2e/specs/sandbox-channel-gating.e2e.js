import path from "node:path";
import { copyFileSync, mkdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { findCellByNameAnywhere, openDevelopFor } from "../helpers.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const FIXTURE = path.resolve(__dirname, "../../../test_image/exif-shot-info.jpg");
const FIXTURE_NAME = "exif-shot-info";

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
      const plugins = await browser.execute(() => document.body.textContent?.includes("Add plugin") ?? false);
      console.log(`[channel] Export Plugins tab shows its add control: ${plugins}`);
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
    const profileText = await waitFor(
      () =>
        browser.execute(() => {
          const body = document.querySelector('[data-ctx-panel="lens_corrections"]')?.closest("details")?.querySelector(".sub-body");
          const note = body?.querySelector(".static-note")?.textContent?.trim() ?? "";
          return note.startsWith("Profile found") || note.startsWith("No matching") || note.includes("not included") ? note : "";
        }),
      "the Lens Corrections profile note",
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

    const before = await shotBuffer();
    await browser.execute(() => {
      const el = /** @type {HTMLInputElement} */ (document.getElementById("lens-manual-distortion"));
      const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, "value")?.set;
      setter?.call(el, "60");
      el.dispatchEvent(new Event("input", { bubbles: true }));
    });
    await waitFor(async () => {
      const stack = await invoke("get_edit_stack", { versionId });
      return stack.ops.find((/** @type {any} */ o) => o.op === "lens_correction")?.manual_distortion === 60;
    }, "the Manual distortion edit reaches the catalog");
    await browser.pause(1500);
    const after = await shotBuffer();
    await shot(store ? "mas-lens" : "default-lens");
    console.log(`[channel] window screenshot changed after Manual distortion: ${!before.equals(after)}`);
    expect(before.equals(after)).toBe(false);
  });

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

  async function shotBuffer() {
    return Buffer.from(await browser.takeScreenshot(), "base64");
  }
});
