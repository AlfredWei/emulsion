import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const FIX = (/** @type {string} */ f) => path.resolve(__dirname, "../../../test_image", f);
const SHOT = process.env.FILMSTRIP_SHOT_DIR ?? "";

/** The Library filmstrip's top edge drags its height; cells scale with it, the choice survives a reload and a double-click resets it. */
describe("Filmstrip resize", function () {
  this.timeout(120000);
  const invoke = (/** @type {string} */ cmd, /** @type {any} */ args) => browser.execute((c, a) => window.__TAURI__.core.invoke(c, a), cmd, args);
  // Read the inline custom property, not getBoundingClientRect(): on GitHub's macOS runner the latter stays frozen at the
  // first layout (see wdio.conf.js), while an inline style updates synchronously.
  const metrics = () =>
    browser.execute(() => {
      const wrap = /** @type {HTMLElement} */ (document.querySelector(".filmstrip-wrap"));
      return { cell: parseInt(wrap.style.getPropertyValue("--cell-h"), 10), stored: localStorage.getItem("emulsion_filmstrip_height_v1") };
    });
  const drag = (/** @type {number} */ dy) =>
    browser.execute((d) => {
      const h = /** @type {HTMLElement} */ (document.querySelector(".resize-handle"));
      const r = h.getBoundingClientRect();
      const y = r.top + r.height / 2;
      const ev = (/** @type {string} */ type, /** @type {number} */ cy) => new PointerEvent(type, { bubbles: true, pointerId: 1, clientX: r.left + 40, clientY: cy, button: 0 });
      h.dispatchEvent(ev("pointerdown", y));
      h.dispatchEvent(ev("pointermove", y + d));
      h.dispatchEvent(ev("pointerup", y + d));
    }, dy);

  before(async function () {
    await browser.setTimeout({ script: 120000 });
    await invoke("import_files", { paths: [FIX("Cavenagh-Bridge-Singapore-panorama-frame-a.jpg"), FIX("Field-corn-Liechtenstein-landscape.jpg"), FIX("Smiling-woman-pink-shirt-portrait.jpg")] });
    await browser.refresh();
    await browser.waitUntil(() => browser.execute(() => !!document.querySelector(".filmstrip .cell")), { timeout: 90000 });
  });

  it("starts at the default height", async function () {
    expect((await metrics()).cell).toBe(52);
  });

  it("dragging the top edge up makes the cells taller", async function () {
    await drag(-100);
    const after = await metrics();
    expect(after.cell).toBe(152);
    expect(after.stored).toBe("172");
    if (SHOT) await browser.saveScreenshot(path.join(SHOT, "filmstrip-tall.png"));
  });

  it("the height survives a reload", async function () {
    await browser.refresh();
    await browser.waitUntil(() => browser.execute(() => !!document.querySelector(".filmstrip .cell")), { timeout: 90000 });
    expect((await metrics()).cell).toBe(152);
  });

  it("is limited at both ends, and a double-click on the edge resets it", async function () {
    await drag(-900);
    expect((await metrics()).cell).toBe(240);
    await drag(900);
    expect((await metrics()).cell).toBe(52);
    await drag(-60);
    await browser.execute(() => document.querySelector(".resize-handle")?.dispatchEvent(new MouseEvent("dblclick", { bubbles: true })));
    const m = await metrics();
    expect(m.cell).toBe(52);
    expect(m.stored).toBe("72");
  });
});
