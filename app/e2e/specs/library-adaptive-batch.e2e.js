import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const FIX = (/** @type {string} */ f) => path.resolve(__dirname, "../../../test_image", f);
const PHOTOS = [FIX("Field-corn-Liechtenstein-landscape.jpg"), FIX("Cavenagh-Bridge-Singapore-panorama-frame-a.jpg"), FIX("Szechenyi-Chain-Bridge-Budapest-night.jpg")];

/**
 * RFC-0035 B3 in the real window: the Library's "Apply Adaptive…" over three selected photographs (a landscape, a
 * bridge, a night scene). It needs no GPU: it reads the dialog's report and the stored stacks, and checks that the two
 * agree photo by photo, that nothing is hidden and that each written photo has its own History row.
 */
describe("Library: apply an adaptive preset to the selection (RFC-0035 B3)", function () {
  this.timeout(360000);

  const state = /** @type {Record<string, any>} */ ({});
  const invoke = (/** @type {string} */ cmd, /** @type {any} */ args) => browser.execute((c, a) => window.__TAURI__.core.invoke(c, a), cmd, args);

  async function waitFor(/** @type {() => Promise<any>} */ fn, /** @type {string} */ what, ms = 120000) {
    const deadline = Date.now() + ms;
    let last;
    while (Date.now() < deadline) {
      last = await fn();
      if (last) return last;
      await new Promise((r) => setTimeout(r, 200));
    }
    throw new Error(`${what}: not within ${ms} ms; last = ${JSON.stringify(last)}`);
  }

  const clickCell = (/** @type {number} */ versionId, /** @type {object} */ mods = {}) =>
    browser.execute(
      (vid, m) => document.querySelector(`[data-ctx-photo="${vid}"]`)?.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true, view: window, ...m })),
      versionId,
      mods,
    );

  before(async function () {
    await browser.setTimeout({ script: 120000 });
    await invoke("import_files", { paths: PHOTOS });
    const images = await invoke("list_images", {});
    state.imgs = PHOTOS.map((p) => images.find((/** @type {any} */ i) => i.path === p));
    for (const i of state.imgs) expect(i).toBeTruthy();
    await browser.refresh();
    await waitFor(() => browser.execute((vid) => !!document.querySelector(`[data-ctx-photo="${vid}"]`), state.imgs[0].version_id), "the grid cells");
  });

  it("the dialog asks for Amount, then reports every selected photo and matches what was written", async function () {
    const ids = state.imgs.map((/** @type {any} */ i) => i.version_id);
    await clickCell(ids[0]);
    await clickCell(ids[1], { metaKey: true, ctrlKey: true });
    await clickCell(ids[2], { metaKey: true, ctrlKey: true });
    await browser.execute(() => {
      const sel = /** @type {HTMLSelectElement} */ (document.querySelector(".adaptive-select"));
      sel.value = "cool-shadows";
      sel.dispatchEvent(new Event("change", { bubbles: true }));
    });
    await waitFor(() => browser.execute(() => !!document.querySelector(".adaptive-batch-run")), "the Amount question");
    const ask = await browser.execute(() => document.querySelector('[role="dialog"]')?.textContent ?? "");
    expect(ask).toContain("3 photos");
    await browser.execute(() => /** @type {HTMLElement} */ (document.querySelector(".adaptive-batch-run")).click());
    await waitFor(() => browser.execute(() => !!document.querySelector(".adaptive-batch-summary")), "the report");

    const report = await browser.execute(() => {
      const d = document.querySelector('[role="dialog"]');
      const groups = Array.from(d?.querySelectorAll("section.group") ?? []).map((g) => ({ cls: g.className, text: g.textContent ?? "" }));
      return { text: d?.textContent ?? "", groups };
    });
    // Every photo is accounted for: named in a group, or counted as applied cleanly.
    const inGroups = state.imgs.filter((/** @type {any} */ i) => report.groups.some((g) => g.text.includes(path.basename(i.path)))).length;
    const cleanly = Number(/(\d+) applied cleanly/.exec(report.text)?.[1] ?? 0);
    expect(inGroups + cleanly).toBe(3);

    // The report and the catalog agree photo by photo: a mask exists exactly where the report says one was added.
    let withMask = 0;
    for (const i of state.imgs) {
      const stack = await invoke("get_edit_stack", { versionId: i.version_id });
      const masks = stack.ops.filter((/** @type {any} */ o) => o.op === "luminance_range_mask");
      const history = await invoke("get_history", { versionId: i.version_id });
      const labelled = history.filter((/** @type {any} */ h) => h.label === "Adaptive: Cool the shadows 100 %").length;
      expect(masks.length).toBe(labelled);
      expect(masks.length).toBeLessThanOrEqual(1);
      withMask += masks.length;
    }
    const summary = await browser.execute(() => document.querySelector(".adaptive-batch-summary")?.textContent ?? "");
    expect(summary).toContain(`${withMask} of 3`);
    console.log(`BATCH_E2E ${JSON.stringify(summary.replace(/\s+/g, " ").trim())} groups=${report.groups.map((g) => g.cls.replace(/.*adaptive-batch-/, "")).join(",")}`);
  });

  it("Done closes the dialog and applying again keeps one mask per photo (it replaces, it does not stack)", async function () {
    await browser.execute(() => /** @type {HTMLElement} */ (document.querySelector('[role="dialog"] .actions .primary')).click());
    await waitFor(() => browser.execute(() => !document.querySelector('[role="dialog"][aria-label^="Apply"]')), "the dialog closed");
    const ids = state.imgs.map((/** @type {any} */ i) => i.version_id);
    const before = await Promise.all(ids.map(async (/** @type {number} */ v) => (await invoke("get_edit_stack", { versionId: v })).ops.filter((/** @type {any} */ o) => /_mask$/.test(o.op)).length));
    await browser.execute(() => {
      const sel = /** @type {HTMLSelectElement} */ (document.querySelector(".adaptive-select"));
      sel.value = "cool-shadows";
      sel.dispatchEvent(new Event("change", { bubbles: true }));
    });
    await waitFor(() => browser.execute(() => !!document.querySelector(".adaptive-batch-run")), "the Amount question again");
    await browser.execute(() => /** @type {HTMLElement} */ (document.querySelector(".adaptive-batch-run")).click());
    await waitFor(() => browser.execute(() => !!document.querySelector(".adaptive-batch-summary")), "the second report");
    const after = await Promise.all(ids.map(async (/** @type {number} */ v) => (await invoke("get_edit_stack", { versionId: v })).ops.filter((/** @type {any} */ o) => /_mask$/.test(o.op)).length));
    expect(after).toEqual(before);
  });
});
