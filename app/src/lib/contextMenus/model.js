// Context menu model (RFC-0028 §3.1): what a menu is made of, and the pure rules the component follows --
// which row Up / Down lands on, where a menu or submenu is placed, how a builder's raw list is tidied.
// Nothing here touches the DOM or a store, so each rule is unit-tested without either.

/**
 * @typedef {Object} MenuItem
 * @property {string} id  stable identifier (tests, e2e, `data-menu-id`)
 * @property {string} label
 * @property {string} [shortcut]  display text only, read from the user's shortcut map by the builder
 * @property {boolean} [disabled]  shown greyed; `reason` says why (tooltip) -- disabled, not hidden, so a menu keeps its shape
 * @property {string} [reason]
 * @property {boolean} [checked]  a radio-style mark (the current rating / flag)
 * @property {boolean} [danger]
 * @property {{ title: string, message: string, confirmLabel: string }} [confirm]  ask before `run`
 * @property {MenuEntry[]} [children]  a submenu
 * @property {() => void} [run]
 */
/** @typedef {{ separator: true }} Separator */
/** @typedef {{ header: string }} Header */
/** @typedef {MenuItem | Separator | Header} MenuEntry */

/** @type {Separator} */
export const SEPARATOR = { separator: true };

/** @param {string} text @returns {Header} */
export function header(text) {
  return { header: text };
}

/** @param {MenuEntry} e @returns {e is MenuItem} */
export function isItem(e) {
  return !("separator" in e) && !("header" in e);
}

/** @param {MenuEntry} e @returns {e is Separator} */
export function isSeparator(e) {
  return "separator" in e;
}

/**
 * Cleans a builder's raw list so builders can use plain conditionals: falsy entries are dropped, a submenu
 * with nothing left in it disappears, and separators are collapsed (none first, none last, never two in a
 * row). Headers stay where they are.
 * @param {(MenuEntry | false | null | undefined)[]} raw
 * @returns {MenuEntry[]}
 */
export function tidy(raw) {
  /** @type {MenuEntry[]} */
  const out = [];
  for (const entry of raw) {
    if (!entry) continue;
    if (isItem(entry) && entry.children) {
      const children = tidy(entry.children);
      if (!children.some(isItem)) continue;
      out.push({ ...entry, children });
      continue;
    }
    if (isSeparator(entry)) {
      const last = out[out.length - 1];
      if (out.length === 0 || (last && isSeparator(last))) continue;
    }
    out.push(entry);
  }
  while (out.length > 0 && isSeparator(out[out.length - 1])) out.pop();
  return out;
}

/** Index of the next enabled item from `from` in direction `dir` (+1 / -1), wrapping; -1 when none is
 * enabled. `from` of -1 starts before the first row going down, and after the last going up.
 * @param {MenuEntry[]} entries @param {number} from @param {1 | -1} dir */
export function nextEnabled(entries, from, dir) {
  const n = entries.length;
  if (n === 0) return -1;
  let i = from < 0 ? (dir === 1 ? -1 : n) : from;
  for (let step = 0; step < n; step++) {
    i = (i + dir + n) % n;
    const e = entries[i];
    if (isItem(e) && !e.disabled) return i;
  }
  return -1;
}

/** @param {MenuEntry[]} entries */
export function firstEnabled(entries) {
  return nextEnabled(entries, -1, 1);
}

/** @param {MenuEntry[]} entries */
export function lastEnabled(entries) {
  return nextEnabled(entries, -1, -1);
}

/** The next enabled item after `from` whose label starts with `char` (case-insensitive), wrapping once;
 * -1 when none does.
 * @param {MenuEntry[]} entries @param {number} from @param {string} char */
export function typeAhead(entries, from, char) {
  const c = char.toLowerCase();
  const n = entries.length;
  for (let step = 1; step <= n; step++) {
    const i = (Math.max(from, -1) + step + n) % n;
    const e = entries[i];
    if (isItem(e) && !e.disabled && e.label.toLowerCase().startsWith(c)) return i;
  }
  return -1;
}

/** What activating an item does.
 * @param {MenuItem} item @returns {"none" | "submenu" | "confirm" | "run"} */
export function activation(item) {
  if (item.disabled) return "none";
  if (item.children) return "submenu";
  if (item.confirm) return "confirm";
  return item.run ? "run" : "none";
}

const MARGIN = 4;

/** Top-left of a menu opened at the pointer, kept fully inside the viewport (it moves up / left instead of
 * overflowing, but never past the top-left margin).
 * @param {number} x @param {number} y @param {{ width: number, height: number }} size @param {{ width: number, height: number }} viewport */
export function placeAtPoint(x, y, size, viewport) {
  return {
    left: Math.max(MARGIN, Math.min(x, viewport.width - size.width - MARGIN)),
    top: Math.max(MARGIN, Math.min(y, viewport.height - size.height - MARGIN)),
  };
}

/** Top-left of a submenu beside its parent row: to the right when it fits, otherwise flipped to the left of
 * the parent menu; aligned with the row and clamped vertically.
 * @param {{ left: number, right: number, top: number }} row  the parent row's rect
 * @param {{ width: number, height: number }} size @param {{ width: number, height: number }} viewport */
export function placeSubmenu(row, size, viewport) {
  const fitsRight = row.right + size.width - 2 <= viewport.width - MARGIN;
  const left = fitsRight ? row.right - 2 : Math.max(MARGIN, row.left - size.width + 2);
  const top = Math.max(MARGIN, Math.min(row.top - 4, viewport.height - size.height - MARGIN));
  return { left, top };
}
