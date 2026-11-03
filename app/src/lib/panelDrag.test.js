import { describe, it, expect, vi, afterEach } from "vitest";
import { clampPanelPosition, loadPanelPosition, savePanelPosition, MIN_VISIBLE_PX } from "./panelDrag.js";

const panel = { w: 210, h: 500 };
const container = { w: 1000, h: 700 };

describe("clampPanelPosition", () => {
  it("leaves a position inside the container alone and rounds it", () => {
    expect(clampPanelPosition({ x: 100.4, y: 50.6 }, panel, container)).toEqual({ x: 100, y: 51 });
  });

  it("keeps the whole width inside, and the top of the panel inside at the bottom", () => {
    expect(clampPanelPosition({ x: -50, y: -9 }, panel, container)).toEqual({ x: 0, y: 0 });
    expect(clampPanelPosition({ x: 5000, y: 5000 }, panel, container)).toEqual({ x: 1000 - 210, y: 700 - MIN_VISIBLE_PX });
  });

  it("copes with a container smaller than the panel (a tiny window) without going negative", () => {
    expect(clampPanelPosition({ x: 40, y: 40 }, panel, { w: 150, h: 20 })).toEqual({ x: 0, y: 0 });
  });

  it("a short panel is not pushed above the bottom edge by the header allowance", () => {
    expect(clampPanelPosition({ x: 0, y: 9999 }, { w: 210, h: 20 }, container).y).toBe(700 - 20);
  });
});

describe("remembered position", () => {
  afterEach(() => vi.unstubAllGlobals());
  const store = () => {
    const m = new Map();
    vi.stubGlobal("localStorage", { getItem: (/** @type {string} */ k) => m.get(k) ?? null, setItem: (/** @type {string} */ k, /** @type {string} */ v) => m.set(k, v), removeItem: (/** @type {string} */ k) => m.delete(k) });
    return m;
  };

  it("round-trips, and forgets on null", () => {
    store();
    expect(loadPanelPosition("k")).toBeNull();
    savePanelPosition("k", { x: 12, y: 34 });
    expect(loadPanelPosition("k")).toEqual({ x: 12, y: 34 });
    savePanelPosition("k", null);
    expect(loadPanelPosition("k")).toBeNull();
  });

  it("ignores what is not a position, and works without storage at all", () => {
    const m = store();
    m.set("k", "not json");
    expect(loadPanelPosition("k")).toBeNull();
    m.set("k", JSON.stringify({ x: "1", y: 2 }));
    expect(loadPanelPosition("k")).toBeNull();
    vi.unstubAllGlobals(); // Node has no localStorage here: both calls must be harmless
    expect(loadPanelPosition("k")).toBeNull();
    expect(() => savePanelPosition("k", { x: 1, y: 2 })).not.toThrow();
  });
});
