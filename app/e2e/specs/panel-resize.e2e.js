import path from "node:path";
import { fileURLToPath } from "node:url";
import { findCellByName, openDevelopFor } from "../helpers.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
// A different fixture from golden-path.e2e.js's, purely to avoid the two
// specs' catalog entries/edit history interfering with each other on a
// repeated local run (see golden-path's own comment on catalog
// accumulation).
const FIXTURE_PATH = path.resolve(__dirname, "../../../test_image/Szechenyi-Chain-Bridge-Budapest-night.jpg");
const FIXTURE_NAME = "Szechenyi-Chain-Bridge-Budapest-night";
const STORAGE_KEY = "emulsion_develop_panel_widths_v1";

/**
 * M4.5: Develop's History (left) and adjustments (right) rails are
 * drag-resizable via a thin `.panel-resize-handle` divider on each side
 * (+page.svelte), with the chosen widths persisted to localStorage
 * (panelLayout.js) so they survive a restart.
 *
 * The actual drag is dispatched as synthetic `pointerdown`/`pointermove`/
 * `pointerup` events computed from each handle's own `getBoundingClientRect()`,
 * rather than driven by WebdriverIO's pixel/screen-coordinate pointer
 * actions -- golden-path.e2e.js's double-click already found that a real
 * WebDriver pointer action doesn't reliably land inside this embedded
 * WebKit WebDriver's event timing/targeting, and a 1px-wide divider is an
 * even smaller, less forgiving target than a whole grid cell. Dispatching
 * the events directly at the exact element's own measured center is
 * precise and avoids that whole class of flakiness.
 */
describe("Develop panel resize", () => {
  before(async () => {
    await browser.setTimeout({ script: 90000 });
    await browser.execute(
      async (fixturePath) => window.__TAURI__.core.invoke("import_files", { paths: [fixturePath] }),
      FIXTURE_PATH,
    );
    // Unlike the SQLite catalog (real data, deliberately left to
    // accumulate across local runs -- see golden-path.e2e.js), the panel
    // widths are a pure UI preference persisted in this same webview's
    // localStorage, which also survives across separate `npm run
    // test:e2e` invocations. Clearing it before the reload below (rather
    // than leaving whatever a prior run dragged it to) is what makes
    // `beforeWidths` below reliably the real defaults.
    await browser.execute((key) => localStorage.removeItem(key), STORAGE_KEY);
    await browser.refresh();

    const cell = await findCellByName(FIXTURE_NAME);
    await cell.waitForExist({ timeout: 20000 });
    await openDevelopFor(FIXTURE_NAME);

    const historyRail = await $(".history-rail");
    await historyRail.waitForExist({ timeout: 20000 });
  });

  function readPanelWidths() {
    return browser.execute(() => ({
      historyWidth: document.querySelector(".develop-body > .history-rail").getBoundingClientRect().width,
      developWidth: document.querySelector(".develop-body > .panel").getBoundingClientRect().width,
    }));
  }

  /** Dispatches a synthetic drag on the nth `.panel-resize-handle`
   * (0 = History's, on the left; 1 = the adjustments panel's, on the
   * right) by `dx` screen pixels, and returns the resulting widths of
   * `.history-rail` and the develop `.panel`. */
  async function dragHandle(/** @type {0 | 1} */ index, /** @type {number} */ dx) {
    await browser.execute(
      (i, delta) => {
        const handle = document.querySelectorAll(".panel-resize-handle")[i];
        const rect = handle.getBoundingClientRect();
        const startX = rect.left + rect.width / 2;
        const y = rect.top + rect.height / 2;
        const base = { bubbles: true, cancelable: true, pointerId: 1, clientY: y };
        handle.dispatchEvent(new PointerEvent("pointerdown", { ...base, clientX: startX }));
        handle.dispatchEvent(new PointerEvent("pointermove", { ...base, clientX: startX + delta }));
        handle.dispatchEvent(new PointerEvent("pointerup", { ...base, clientX: startX + delta }));
      },
      index,
      dx,
    );
    // The pointer events update Svelte's $state synchronously, but the DOM
    // (and so getBoundingClientRect()) doesn't reflect it until a later
    // paint -- reading it in the same synchronous tick as the dispatch
    // above raced the old, pre-drag width. A single requestAnimationFrame
    // wait was enough locally but not on CI's macOS runner (a slower or
    // differently-scheduled WebKit build), where the very next read still
    // observed the pre-drag value. Poll from the Node side instead of
    // guessing a fixed frame count: keep reading until two consecutive
    // reads agree, which is correct regardless of how many paints the
    // update actually takes.
    let widths = await readPanelWidths();
    await browser.waitUntil(
      async () => {
        const next = await readPanelWidths();
        const stable = next.historyWidth === widths.historyWidth && next.developWidth === widths.developWidth;
        widths = next;
        return stable;
      },
      { timeout: 5000, interval: 50 },
    );
    return widths;
  }

  it("drag-resizes both rails and persists the resulting widths", async () => {
    const beforeWidths = await browser.execute(() => ({
      historyWidth: document.querySelector(".develop-body > .history-rail").getBoundingClientRect().width,
      developWidth: document.querySelector(".develop-body > .panel").getBoundingClientRect().width,
    }));

    // History sits on the left -- dragging its handle right grows it.
    const afterHistoryDrag = await dragHandle(0, 80);
    expect(afterHistoryDrag.historyWidth).toBe(beforeWidths.historyWidth + 80);

    // The adjustments panel sits on the right -- dragging its handle left
    // (negative dx) grows it, matching the sign flip in
    // handlePanelResizePointerMove (+page.svelte).
    const afterDevelopDrag = await dragHandle(1, -60);
    expect(afterDevelopDrag.developWidth).toBe(beforeWidths.developWidth + 60);
    // The earlier History resize must still hold -- confirms the two
    // handles' drag state doesn't cross-contaminate.
    expect(afterDevelopDrag.historyWidth).toBe(afterHistoryDrag.historyWidth);

    const stored = await browser.execute((key) => JSON.parse(localStorage.getItem(key)), STORAGE_KEY);
    expect(stored.history).toBe(afterDevelopDrag.historyWidth);
    expect(stored.develop).toBe(afterDevelopDrag.developWidth);
  });

  it("clamps drags beyond the configured min/max bounds", async () => {
    const grownPastMax = await dragHandle(0, 100000);
    expect(grownPastMax.historyWidth).toBe(400); // HISTORY_PANEL_MAX_WIDTH, panelLayout.js

    const shrunkPastMin = await dragHandle(0, -100000);
    expect(shrunkPastMin.historyWidth).toBe(160); // HISTORY_PANEL_MIN_WIDTH, panelLayout.js
  });
});
