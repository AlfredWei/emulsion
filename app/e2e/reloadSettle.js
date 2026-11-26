// `browser.refresh()` returns as soon as `location.reload()` has been called. The embedded WebDriver's `execute/sync`
// stores its result in a variable of the page it ran in and polls that variable, so a script that lands in the old
// document just before the new one replaces it never finds its result. The command then waits out the whole script
// timeout (90 s in the specs that set one), retries, and succeeds, which is why a `before` hook with a 120 s budget fails.
// This waits for the new document with short script timeouts, so a lost poll costs 1 s and not 90 s.

const POLL_SCRIPT_MS = 1000;
const SETTLE_BUDGET_MS = 30000;

export function settleAfterReload(/** @type {WebdriverIO.Browser} */ browser) {
  browser.overwriteCommand("refresh", async function (refresh) {
    const { script } = await browser.getTimeouts();
    await browser.execute(() => {
      /** @type {any} */ (window).__beforeReload = true;
    });
    await refresh();
    await browser.setTimeout({ script: POLL_SCRIPT_MS });
    try {
      const deadline = Date.now() + SETTLE_BUDGET_MS;
      for (;;) {
        try {
          const fresh = await browser.execute(() => !(/** @type {any} */ (window).__beforeReload) && document.readyState === "complete");
          if (fresh) return;
        } catch {
          // the poll landed in the document being replaced
        }
        if (Date.now() > deadline) throw new Error(`the page did not finish reloading within ${SETTLE_BUDGET_MS} ms`);
        await browser.pause(50);
      }
    } finally {
      await browser.setTimeout({ script });
    }
  });
}
