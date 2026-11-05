import { describe, it, expect } from "vitest";
import { RemovalStore, LARGE_MARK_FRACTION } from "./removal.svelte.js";
import { MaskStore } from "./masks.svelte.js";
import { DevelopStore } from "./develop.svelte.js";
import { createLibraryStore } from "./library.svelte.js";

function setup() {
  const develop = new DevelopStore(createLibraryStore());
  const masks = new MaskStore(develop);
  return { masks, removal: new RemovalStore(masks) };
}

describe("RemovalStore", () => {
  it("is armed exactly while the remove tool is the active tool", () => {
    const { masks, removal } = setup();
    expect(removal.active).toBe(false);
    masks.activeTool = "remove";
    expect(removal.active).toBe(true);
    masks.activeTool = "brush";
    expect(removal.active).toBe(false);
  });

  it("measures the marks' extent as a fraction of the photograph's longer side, brush radius included", () => {
    const { removal } = setup();
    expect(removal.extent(1.5)).toBe(0);
    // A lasso from x 0.2..0.4 on a 3:2 photograph: 0.2 of the width, which is the longer side.
    removal.shapes = [{ kind: "polygon", points: [[0.2, 0.4], [0.4, 0.4], [0.3, 0.5]] }];
    expect(removal.extent(1.5)).toBeCloseTo(0.2, 6);
    // The same outline on a portrait 2:3 photograph: x spans 0.2 of the short side, y 0.1 of the long side.
    expect(removal.extent(2 / 3)).toBeCloseTo(Math.max(0.2 * (2 / 3), 0.1), 6);
    // A single dab of radius 0.05 (of the longer side) is 0.1 across, whatever the aspect.
    removal.shapes = [{ kind: "stroke", points: [[0.5, 0.5]], radius: 0.05 }];
    expect(removal.extent(1.5)).toBeCloseTo(0.1, 6);
    expect(removal.extent(2 / 3)).toBeCloseTo(0.1, 6);
    // Two marks far apart count as one extent.
    removal.shapes = [{ kind: "stroke", points: [[0.1, 0.5]], radius: 0.01 }, { kind: "stroke", points: [[0.9, 0.5]], radius: 0.01 }];
    expect(removal.extent(1)).toBeGreaterThan(LARGE_MARK_FRACTION);
  });

  it("clearing the marks also drops the large-area question", () => {
    const { removal } = setup();
    removal.shapes = [{ kind: "stroke", points: [[0.5, 0.5]], radius: 0.01 }];
    removal.guarding = true;
    removal.clearMarks();
    expect([removal.shapes.length, removal.guarding]).toEqual([0, false]);
  });
});
