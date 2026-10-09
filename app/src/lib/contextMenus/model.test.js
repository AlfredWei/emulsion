import { describe, it, expect } from "vitest";
import { SEPARATOR, header, tidy, nextEnabled, firstEnabled, lastEnabled, typeAhead, activation, placeAtPoint, placeSubmenu } from "./model.js";

const item = (/** @type {string} */ label, /** @type {object} */ extra = {}) => ({ id: label, label, run: () => {}, ...extra });

describe("tidy", () => {
  it("drops falsy entries and collapses separators (none first, none last, never two in a row)", () => {
    const out = tidy([SEPARATOR, item("a"), SEPARATOR, SEPARATOR, false, item("b"), SEPARATOR, null, undefined]);
    expect(out.map((e) => ("label" in e ? e.label : "-"))).toEqual(["a", "-", "b"]);
  });

  it("removes a submenu with nothing left in it, and tidies the ones that stay", () => {
    const out = tidy([item("keep", { children: [SEPARATOR, item("x"), SEPARATOR] }), item("gone", { children: [false, SEPARATOR] }), item("plain")]);
    expect(out.map((e) => ("label" in e ? e.label : "-"))).toEqual(["keep", "plain"]);
    const keep = /** @type {any} */ (out[0]);
    expect(keep.children).toHaveLength(1);
  });

  it("keeps headers", () => {
    const out = tidy([header("3 photos"), item("a")]);
    expect(out[0]).toEqual({ header: "3 photos" });
  });
});

describe("keyboard row choice", () => {
  const rows = [header("h"), item("Open"), SEPARATOR, item("Rating", { children: [item("x")] }), item("Off", { disabled: true }), item("Zoom")];

  it("Down and Up skip separators, headers and disabled rows, and wrap", () => {
    expect(nextEnabled(rows, -1, 1)).toBe(1);
    expect(nextEnabled(rows, 1, 1)).toBe(3);
    expect(nextEnabled(rows, 3, 1)).toBe(5); // 4 is disabled
    expect(nextEnabled(rows, 5, 1)).toBe(1); // wraps
    expect(nextEnabled(rows, 1, -1)).toBe(5); // wraps up
    expect(nextEnabled(rows, -1, -1)).toBe(5);
  });

  it("Home / End, and -1 when nothing is enabled", () => {
    expect(firstEnabled(rows)).toBe(1);
    expect(lastEnabled(rows)).toBe(5);
    expect(nextEnabled([item("a", { disabled: true })], -1, 1)).toBe(-1);
    expect(nextEnabled([], -1, 1)).toBe(-1);
  });

  it("type-ahead goes to the next enabled row starting with the letter, wrapping", () => {
    expect(typeAhead(rows, -1, "z")).toBe(5);
    expect(typeAhead(rows, 5, "o")).toBe(1);
    expect(typeAhead(rows, 1, "o")).toBe(1); // wraps back to itself when it is the only match (Off is disabled)
    expect(typeAhead(rows, 1, "q")).toBe(-1);
  });
});

describe("activation", () => {
  it("is nothing for a disabled item, a submenu for children, confirm before run, else run", () => {
    expect(activation(item("a", { disabled: true }))).toBe("none");
    expect(activation(item("a", { children: [item("b")] }))).toBe("submenu");
    expect(activation(item("a", { confirm: { title: "t", message: "m", confirmLabel: "ok" } }))).toBe("confirm");
    expect(activation(item("a"))).toBe("run");
    expect(activation({ id: "x", label: "x" })).toBe("none");
  });
});

describe("placement", () => {
  const vp = { width: 800, height: 600 };
  const size = { width: 200, height: 300 };

  it("a menu at the pointer moves up and left instead of leaving the window", () => {
    expect(placeAtPoint(100, 100, size, vp)).toEqual({ left: 100, top: 100 });
    expect(placeAtPoint(790, 590, size, vp)).toEqual({ left: 596, top: 296 });
    expect(placeAtPoint(-20, -20, size, vp)).toEqual({ left: 4, top: 4 });
  });

  it("a submenu opens to the right of its row, flips left when that does not fit, and stays inside vertically", () => {
    expect(placeSubmenu({ left: 100, right: 300, top: 50 }, size, vp)).toEqual({ left: 298, top: 46 });
    const flipped = placeSubmenu({ left: 560, right: 760, top: 50 }, size, vp);
    expect(flipped.left).toBe(362);
    expect(placeSubmenu({ left: 100, right: 300, top: 500 }, size, vp).top).toBe(296);
  });
});
