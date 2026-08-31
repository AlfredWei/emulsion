/** Shared helpers across e2e specs. See golden-path.e2e.js's file-level
 * comment for the platform quirks these work around (WebKit/embedded
 * WebDriver click/double-click unreliability, native OS dialogs). */

/** `.cell` (GridCell.svelte) has no per-item id -- a fixture's own
 * `.file-name` text is the only stable way to find its specific tile. */
export function findCellByName(/** @type {string} */ name) {
  return $(`//div[contains(concat(' ', @class, ' '), ' cell ')][.//span[contains(@class, 'file-name') and contains(text(), '${name}')]]`);
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
  const cell = await findCellByName(fixtureName);
  await browser.execute((el) => {
    el.dispatchEvent(new MouseEvent("dblclick", { bubbles: true, cancelable: true, view: window }));
  }, cell);

  const developButton = await $(".hud-develop-btn");
  await developButton.waitForExist({ timeout: 10000 });
  await clickEl(developButton);
}
