import { describe, it, expect } from "vitest";
import { SEPARATOR, header } from "./model.js";
import { INITIAL, levelsOf, highlighted, hover, navigate } from "./navigation.js";

const item = (/** @type {string} */ label, /** @type {object} */ extra = {}) => ({ id: label, label, run: () => {}, ...extra });
const root = [
  header("2 photos"),
  item("Open"),
  item("Rating", { children: [item("5"), item("Off", { disabled: true }), item("4")] }),
  SEPARATOR,
  item("Export"),
];

describe("levelsOf / hover", () => {
  it("shows only the root until a row with a submenu is hovered", () => {
    expect(levelsOf(root, INITIAL)).toHaveLength(1);
    const s = hover(root, INITIAL, 0, 1);
    expect(s).toEqual({ path: [1], sub: false });
    expect(levelsOf(root, s)).toHaveLength(1);
    const r = hover(root, s, 0, 2);
    expect(r).toEqual({ path: [2], sub: true });
    expect(levelsOf(root, r)).toHaveLength(2);
  });

  it("hovering another root row closes the submenu; hovering inside it keeps it", () => {
    const open = hover(root, INITIAL, 0, 2);
    const inside = hover(root, open, 1, 2);
    expect(inside).toEqual({ path: [2, 2], sub: false });
    expect(levelsOf(root, inside)).toHaveLength(2);
    const away = hover(root, inside, 0, 4);
    expect(away).toEqual({ path: [4], sub: false });
    expect(levelsOf(root, away)).toHaveLength(1);
  });

  it("a disabled submenu row does not open", () => {
    const r = [item("Parent", { children: [item("c")], disabled: true })];
    expect(hover(r, INITIAL, 0, 0).sub).toBe(false);
  });
});

describe("navigate", () => {
  it("Down from nothing lands on the first enabled row (not the header); Up from nothing on the last", () => {
    expect(navigate(root, INITIAL, "ArrowDown").state.path).toEqual([1]);
    expect(navigate(root, INITIAL, "ArrowUp").state.path).toEqual([4]);
    expect(navigate(root, INITIAL, "Home").state.path).toEqual([1]);
    expect(navigate(root, INITIAL, "End").state.path).toEqual([4]);
  });

  it("Right enters a submenu on its first enabled row; Left leaves it; Left at the root does nothing", () => {
    const on = { path: [2], sub: false };
    const inside = navigate(root, on, "ArrowRight").state;
    expect(inside.path).toEqual([2, 0]);
    expect(levelsOf(root, inside)).toHaveLength(2);
    expect(navigate(root, inside, "ArrowDown").state.path).toEqual([2, 2]); // skips the disabled row
    const back = navigate(root, inside, "ArrowLeft").state;
    expect(back).toEqual({ path: [2], sub: false });
    expect(navigate(root, back, "ArrowLeft").state).toBe(back);
    expect(navigate(root, { path: [1], sub: false }, "ArrowRight").state.path).toEqual([1]); // no submenu
  });

  it("Enter activates the highlighted item only; Escape closes; type-ahead moves", () => {
    expect(navigate(root, INITIAL, "Enter").activate).toBe(false);
    expect(navigate(root, { path: [4], sub: false }, "Enter").activate).toBe(true);
    expect(navigate(root, INITIAL, "Escape").close).toBe(true);
    expect(navigate(root, INITIAL, "e").state.path).toEqual([4]);
    expect(navigate(root, INITIAL, "q").state).toBe(INITIAL);
  });

  it("highlighted() resolves the row at any depth", () => {
    expect(highlighted(root, { path: [2, 2], sub: false })?.label).toBe("4");
    expect(highlighted(root, INITIAL)).toBeNull();
    expect(highlighted(root, { path: [3], sub: false })).toBeNull(); // a separator
  });
});
