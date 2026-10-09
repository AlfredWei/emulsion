// Keyboard and hover navigation of an open context menu as a pure reducer (RFC-0028 §3.1), so the rules --
// Up / Down skip disabled rows and wrap, Right enters a submenu, Left leaves it, Home / End, type-ahead --
// are tested without a DOM. The component keeps one `NavState` and renders `levelsOf(root, state)`.

import { firstEnabled, lastEnabled, nextEnabled, typeAhead, isItem } from "./model.js";

/**
 * @typedef {Object} NavState
 * @property {number[]} path  the highlighted row at each open level (root first); [] = nothing highlighted yet
 * @property {boolean} sub  whether the deepest highlighted row's submenu is open (shallower ones always are)
 */

/** @type {NavState} */
export const INITIAL = { path: [], sub: false };

/** The entries of every visible level, root first.
 * @param {import('./model.js').MenuEntry[]} root @param {NavState} state */
export function levelsOf(root, state) {
  /** @type {import('./model.js').MenuEntry[][]} */
  const levels = [root];
  for (let d = 0; d < state.path.length; d++) {
    const entry = levels[d][state.path[d]];
    if (!entry || !isItem(entry) || !entry.children) break;
    if (d === state.path.length - 1 && !state.sub) break;
    levels.push(entry.children);
  }
  return levels;
}

/** The highlighted item, if the path points at one.
 * @param {import('./model.js').MenuEntry[]} root @param {NavState} state */
export function highlighted(root, state) {
  if (state.path.length === 0) return null;
  const levels = levelsOf(root, { path: state.path.slice(0, -1), sub: true });
  const entries = levels[state.path.length - 1];
  const entry = entries?.[state.path[state.path.length - 1]];
  return entry && isItem(entry) ? entry : null;
}

/** The pointer entered row `index` of level `depth`: highlight it and open its submenu if it has one.
 * @param {import('./model.js').MenuEntry[]} root @param {NavState} state @param {number} depth @param {number} index @returns {NavState} */
export function hover(root, state, depth, index) {
  const path = [...state.path.slice(0, depth), index];
  const levels = levelsOf(root, { path: path.slice(0, -1), sub: true });
  const entry = levels[depth]?.[index];
  return { path, sub: Boolean(entry && isItem(entry) && entry.children && !entry.disabled) };
}

/** @param {import('./model.js').MenuEntry[]} root @param {NavState} state @param {string} key
 * @returns {{ state: NavState, activate: boolean, close: boolean }} */
export function navigate(root, state, key) {
  const depth = Math.max(0, state.path.length - 1);
  const levels = levelsOf(root, { path: state.path.slice(0, -1), sub: true });
  const entries = levels[depth] ?? root;
  const current = state.path.length > 0 ? state.path[depth] : -1;
  const stay = { state, activate: false, close: false };
  /** @param {number} i */
  const move = (i) => (i < 0 ? stay : { state: { path: [...state.path.slice(0, depth), i], sub: false }, activate: false, close: false });

  switch (key) {
    case "ArrowDown":
      return move(nextEnabled(entries, current, 1));
    case "ArrowUp":
      return move(nextEnabled(entries, current, -1));
    case "Home":
      return move(firstEnabled(entries));
    case "End":
      return move(lastEnabled(entries));
    case "ArrowRight": {
      const item = highlighted(root, state);
      if (!item?.children || item.disabled) return stay;
      const first = firstEnabled(item.children);
      return first < 0 ? stay : { state: { path: [...state.path, first], sub: false }, activate: false, close: false };
    }
    case "ArrowLeft":
      return state.path.length > 1 ? { state: { path: state.path.slice(0, -1), sub: false }, activate: false, close: false } : stay;
    case "Enter":
    case " ":
      return { state, activate: highlighted(root, state) !== null, close: false };
    case "Escape":
      return { state, activate: false, close: true };
    default: {
      if (key.length !== 1 || !/\S/.test(key)) return stay;
      return move(typeAhead(entries, current, key));
    }
  }
}
