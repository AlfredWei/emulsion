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

  // Each drag gets its own synthetic pointerId rather than reusing 1 for
  // every call. Reusing one on CI's macOS runner: the first two dragHandle
  // calls in a run (across both tests) reliably registered, but every call
  // after that silently had no effect (the read came back unchanged, not
  // erroring) -- consistent with WebKit's real setPointerCapture/
  // releasePointerCapture bookkeeping (handlePanelResizePointerDown/Up,
  // +page.svelte) not fully resetting between synthetic, non-hardware
  // pointer sessions that share an ID, even though each cycle's own
  // capture/release calls succeed individually. A fresh ID per call
  // sidesteps that and also matches how a real OS assigns pointer IDs.
  let nextPointerId = 1;

  /** Dispatches a synthetic drag on the nth `.panel-resize-handle`
   * (0 = History's, on the left; 1 = the adjustments panel's, on the
   * right) by `dx` screen pixels, and returns the resulting widths of
   * `.history-rail` and the develop `.panel`. */
  async function dragHandle(/** @type {0 | 1} */ index, /** @type {number} */ dx) {
    // Everything below (dispatch + wait-for-change poll) runs inside a
    // single browser.execute call rather than as separate WebDriver
    // commands. That matters here specifically: @wdio/tauri-service's
    // ensureActiveWindowFocus check (see wdio.conf.js's own comment on it)
    // runs before *every* WebDriver command and, on CI's macOS runner,
    // routinely eats a full 5s retrying a Tauri core.invoke that isn't
    // available yet -- confirmed by the "Tauri core.invoke not available
    // after 5s timeout" WARNs recurring every ~5-6s throughout this spec's
    // CI runs. A Node-side poll loop (browser.waitUntil, or manually
    // re-calling browser.execute) pays that ~5s tax on *every single poll
    // iteration*, which silently turned a nominal 20s wait into only 3-4
    // real attempts and made it look like the drag "never" registered.
    // Dispatching and polling together in one script pays that tax once
    // and then polls with real millisecond granularity inside the browser,
    // with its own generous internal deadline.
    return browser.execute(
      async (i, delta, pointerId) => {
        const handle = document.querySelectorAll(".panel-resize-handle")[i];
        const rect = handle.getBoundingClientRect();
        const startX = rect.left + rect.width / 2;
        const y = rect.top + rect.height / 2;
        const base = { bubbles: true, cancelable: true, pointerId, clientY: y };
        const read = () => ({
          historyWidth: document.querySelector(".develop-body > .history-rail").getBoundingClientRect().width,
          developWidth: document.querySelector(".develop-body > .panel").getBoundingClientRect().width,
        });
        const before = read();

        handle.dispatchEvent(new PointerEvent("pointerdown", { ...base, clientX: startX }));
        handle.dispatchEvent(new PointerEvent("pointermove", { ...base, clientX: startX + delta }));
        handle.dispatchEvent(new PointerEvent("pointerup", { ...base, clientX: startX + delta }));

        // The pointer events update Svelte's $state synchronously, but the
        // DOM (and so getBoundingClientRect()) doesn't reflect it until a
        // later paint. There's no CSS transition on these widths (a plain
        // synchronous reflow), so the first read that differs from
        // `before` is already the final value -- poll for that instead of
        // guessing a fixed frame/time budget. On CI's macOS runner this
        // occasionally never resolves at all (not just slowly -- 45s
        // wasn't any more successful than 15s, so it's a rare missed
        // event, not a slow flush); the calling `it()` retries on
        // failure to absorb that, so this budget just needs to be well
        // past what local dev ever needs, not try to outlast a stall
        // that isn't going to end.
        const deadline = Date.now() + 15000;
        let after = before;
        while (Date.now() < deadline) {
          await new Promise((resolve) => setTimeout(resolve, 50));
          after = read();
          if (after.historyWidth !== before.historyWidth || after.developWidth !== before.developWidth) break;
        }
        return after;
      },
      index,
      dx,
      nextPointerId++,
    );
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
