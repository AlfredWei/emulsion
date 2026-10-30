import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const FIX = (/** @type {string} */ f) => path.resolve(__dirname, "../../../test_image", f);
const SHOT = process.env.FILMSTRIP_SHOT_DIR ?? "";

/** The Library filmstrip's top edge drags its height; cells scale with it, the choice survives a reload and a double-click resets it. */
describe("Filmstrip resize", function () {
  this.timeout(120000);
  const invoke = (/** @type {string} */ cmd, /** @type {any} */ args) => browser.execute((c, a) => window.__TAURI__.core.invoke(c, a), cmd, args);
  const metrics = () =>
    browser.execute(() => {
      const strip = /** @type {HTMLElement} */ (document.querySelector(".filmstrip"));
      const cell = /** @type {HTMLElement} */ (document.querySelector(".filmstrip .cell"));
      return { strip: Math.round(strip.getBoundingClientRect().height), cell: Math.round(cell.getBoundingClientRect().height), cellW: Math.round(cell.getBoundingClientRect().width), stored: localStorage.getItem("emulsion_filmstrip_height_v1") };
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
    const m = await metrics();
    expect(m.cell).toBeGreaterThanOrEqual(50);
    expect(m.cell).toBeLessThanOrEqual(53);
  });

  it("dragging the top edge up makes the strip taller and the cells bigger, keeping their 3:2 shape", async function () {
    const before = await metrics();
    await drag(-100);
    const after = await metrics();
    expect(after.strip - before.strip).toBeGreaterThanOrEqual(95);
    expect(after.cell - before.cell).toBeGreaterThanOrEqual(95);
    expect(Math.abs(after.cellW - after.cell * 1.5)).toBeLessThanOrEqual(2);
    expect(after.stored).toBe("172");
    if (SHOT) await browser.saveScreenshot(path.join(SHOT, "filmstrip-tall.png"));
  });

  it("the height survives a reload", async function () {
    await browser.refresh();
    await browser.waitUntil(() => browser.execute(() => !!document.querySelector(".filmstrip .cell")), { timeout: 90000 });
    expect((await metrics()).cell).toBeGreaterThanOrEqual(150);
  });

  it("is limited at both ends, and a double-click on the edge resets it", async function () {
    await drag(-900);
    expect((await metrics()).stored).toBe("260");
    await drag(900);
    expect((await metrics()).stored).toBe("72");
    await drag(-60);
    await browser.execute(() => document.querySelector(".resize-handle")?.dispatchEvent(new MouseEvent("dblclick", { bubbles: true })));
    const m = await metrics();
    expect(m.stored).toBe("72");
    expect(m.cell).toBeLessThanOrEqual(53);
  });
});
