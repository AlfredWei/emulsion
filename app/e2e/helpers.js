/** Shared helpers across e2e specs. See golden-path.e2e.js's file-level
 * comment for the platform quirks these work around (WebKit/embedded
 * WebDriver click/double-click unreliability, native OS dialogs). */

/** `.cell` (GridCell.svelte) has no per-item id -- a fixture's own
 * `.file-name` text is the only stable way to find its specific tile. */
export function findCellByName(/** @type {string} */ name) {
  return $(`//div[contains(concat(' ', @class, ' '), ' cell ')][.//span[contains(@class, 'file-name') and contains(text(), '${name}')]]`);
}

/** LibraryGrid.svelte is DOM-virtualized (only rows near the current
 * scroll position exist at all -- see its own file comment), and the
 * grid is sorted newest-imported-first. A fixture that's re-imported as a
 * no-op duplicate on every run (its `added_at` never advances -- see
 * golden-path.e2e.js) sinks further from the top as the shared, never-
 * reset-between-runs dev catalog accumulates other imports over time,
 * eventually landing outside the initially-rendered window. This scrolls
 * the grid down in increments, re-checking after each, so a lookup
 * succeeds regardless of where in a possibly-large catalog the target
 * row currently sits. A no-op (zero scrolling) when the cell is already
 * rendered, so it's safe to use in place of `findCellByName` generally.
 *
 * Polling is done entirely through `browser.execute()` (a raw DOM text
 * match, not a WebdriverIO `$`/`findElement` query) until the cell is
 * confirmed present, and only THEN resolved to a real element handle via
 * `findCellByName`. This matters a lot here: `@wdio/tauri-service`
 * intercepts every `$`/`$$`/`findElement`/`elementClick` command with a
 * window-focus check that itself calls into the Tauri bridge, and in
 * this environment that check is reliably failing with "Tauri core
 * .invoke not available after 5s timeout" -- a flat 5s tax on EVERY such
 * command (not a one-time settle cost). A loop built on `$`-based
 * `.isExisting()` calls each pays that tax per iteration, which starves
 * a nominal 20s budget down to 3-4 real iterations; `execute()` isn't in
 * the service's intercepted command list, so looping through it is
 * effectively untaxed. */
export async function findCellByNameAnywhere(
  /** @type {string} */ name,
  /** @type {{timeout?: number}} */ { timeout = 20000 } = {},
) {
  const deadline = Date.now() + timeout;
  let found = await browser.execute(
    (n) => Array.from(document.querySelectorAll(".file-name")).some((el) => el.textContent?.includes(n)),
    name,
  );
  while (!found && Date.now() < deadline) {
    await browser.execute(() => {
      const el = document.querySelector(".grid-scroll");
      if (!el) return;
      el.scrollTop = Math.min(el.scrollTop + el.clientHeight, el.scrollHeight);
      // Setting `.scrollTop` from a script doesn't reliably fire a native
      // `scroll` event in this WebKit + embedded-WebDriver combination
      // (the same class of quirk as golden-path.e2e.js's dispatched-click
      // workaround) -- LibraryGrid.svelte's `onscroll` handler (which
      // recomputes the virtualized window) never runs without this.
      el.dispatchEvent(new Event("scroll"));
    });
    await browser.pause(150);
    found = await browser.execute(
      (n) => Array.from(document.querySelectorAll(".file-name")).some((el) => el.textContent?.includes(n)),
      name,
    );
  }
  return findCellByName(name);
}

/** A WebdriverIO `.click()` (real pointer-down/up actions) intermittently
 * doesn't register in this embedded-WebDriver + WKWebView combination.
 * Dispatching the DOM method directly is what actually reaches Svelte's
 * `onclick` handlers reliably here. */
export async function clickEl(elOrPromise) {
  const el = await elOrPromise;
  await browser.execute((e) => e.click(), el);
}

/** Opens Develop for the given fixture image, already imported and
 * visible in the Library grid: double-clicks its cell (via a dispatched
 * `dblclick` event -- see golden-path.e2e.js's comment on why) into
 * Library's Loupe view, then clicks Loupe's own "Develop →" button. */
export async function openDevelopFor(/** @type {string} */ fixtureName) {
  const cell = await findCellByNameAnywhere(fixtureName);
  await browser.execute((el) => {
    el.dispatchEvent(new MouseEvent("dblclick", { bubbles: true, cancelable: true, view: window }));
  }, cell);

  const developButton = await $(".hud-develop-btn");
  await developButton.waitForExist({ timeout: 10000 });
  await clickEl(developButton);
}
