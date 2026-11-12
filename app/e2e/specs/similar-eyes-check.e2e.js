import path from "node:path";
import os from "node:os";
import { fileURLToPath } from "node:url";
import { execFileSync } from "node:child_process";
import { appendFileSync, copyFileSync, existsSync, mkdirSync, mkdtempSync, rmSync } from "node:fs";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const PORTRAIT = path.resolve(__dirname, "../../../test_image/Smiling-woman-pink-shirt-portrait.jpg");
const YUNET = process.env.YUNET_MODEL ?? "";
const IDENTIFIER = "dev.alfredwei.emulsion";

/**
 * RFC-0031 §8.15 in the real window: opening a group of similar photographs asks the eye check, and a frame whose face has one eye
 * closed (the portrait's left eye reads 0.165, its right 0.233) gets the "One eye closed" badge; a second copy gets the same. LOCAL ONLY
 * (macOS): needs the face detector file (YUNET_MODEL=<face_detection_yunet_2023mar.onnx>, which a face scan fetches into the app's
 * cache; the spec copies it into the throwaway HOME) and the mesh model, which the dev build seeds from `models/`. Two copies of one
 * photograph (the second with one byte appended, so the catalog does not take it for a duplicate) make a group: same picture, same
 * camera (none); the spec gives them capture times a second apart.
 */
describe("Similar view: the eye check badge", function () {
  this.timeout(300000);

  const invoke = (/** @type {string} */ cmd, /** @type {any} */ args) => browser.execute((c, a) => window.__TAURI__.core.invoke(c, a), cmd, args ?? {});
  const text = (/** @type {string} */ sel) => browser.execute((s) => Array.from(document.querySelectorAll(s)).map((e) => (e.textContent ?? "").trim()).join("|"), sel);
  const click = (/** @type {string} */ sel, /** @type {string | null} */ label = null) =>
    browser.execute(
      (s, l) => {
        const el = Array.from(document.querySelectorAll(s)).find((e) => !l || (e.textContent ?? "").trim().startsWith(l));
        if (!el) throw new Error(`nothing to click: ${s} ${l ?? ""}`);
        /** @type {HTMLElement} */ (el).click();
      },
      sel,
      label,
    );
  const SHOTS = process.env.SHOTS_DIR ?? "";
  let workDir = "";

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

  before(async function () {
    if (!YUNET || !existsSync(YUNET) || process.platform !== "darwin") return this.skip();
    const faceModels = path.join(os.homedir(), "Library", "Application Support", IDENTIFIER, "face_models");
    mkdirSync(faceModels, { recursive: true });
    copyFileSync(YUNET, path.join(faceModels, path.basename(YUNET)));
    workDir = mkdtempSync(path.join(os.tmpdir(), "emulsion-eyes-"));
    const copies = ["eyes-a.jpg", "eyes-b.jpg"].map((n) => path.join(workDir, n));
    copies.forEach((c) => copyFileSync(PORTRAIT, c));
    appendFileSync(copies[1], Buffer.from([0])); // one byte after the end of the picture: another file by content, the same picture
    await browser.setTimeout({ script: 300000 });
    await invoke("import_files", { paths: copies });
    await browser.refresh();
  });

  after(() => {
    if (workDir) rmSync(workDir, { recursive: true, force: true });
  });

  it("opening the group shows the eye check's badge on each frame, and the stored answers survive a second opening", async () => {
    await waitFor(async () => (await invoke("cull_analysis_status")).analysed >= 2, "both photographs measured");
    // The pictures carry no capture time, and a group needs one: give the two frames the same second and the next, in the catalog file.
    const db = path.join(os.homedir(), "Library", "Application Support", IDENTIFIER, "catalog.sqlite");
    const ids = (await invoke("list_images")).map((/** @type {any} */ i) => i.image_id).sort((/** @type {number} */ a, /** @type {number} */ b) => a - b);
    execFileSync("sqlite3", [db, `PRAGMA busy_timeout=10000; UPDATE images SET captured_at = '2024-05-01T10:00:00' WHERE id = ${ids[0]}; UPDATE images SET captured_at = '2024-05-01T10:00:01' WHERE id = ${ids[1]};`]);
    await browser.refresh();
    await waitFor(async () => (await text(".tree-item")).includes("Similar"), "the Similar entry");
    await click(".tree-item", "Similar");
    await waitFor(async () => (await text(".similar .cell")) !== "" || (await browser.execute(() => !!document.querySelector(".similar .cell"))), "a group", 30000);
    await click(".similar .cell");
    await waitFor(async () => (await browser.execute(() => document.querySelectorAll('[data-testid="eyes-badge"]').length)) === 2, "a badge on both frames", 120000);
    expect(await text('[data-testid="eyes-badge"]')).toBe("One eye closed|One eye closed");
    if (SHOTS) await browser.saveScreenshot(path.join(SHOTS, "similar-eyes-badge.png"));
    // the answers are stored: asking again is a read, not a second measurement (the rows carry the current rules version)
    const again = await invoke("blink_for_images", { imageIds: ids });
    expect(again.available).toBe(true);
    expect(again.photos.length).toBe(2);
    expect(again.photos.every((/** @type {any} */ p) => p.faces === 1 && p.judged === 1 && p.one_closed === 1 && p.closed === 0)).toBe(true);
  });
});
