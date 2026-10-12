import { describe, it, expect } from "vitest";
import { diffSyncableOps, AUTO_SYNC_EXCLUDED_OP_NAMES } from "./autoSync.js";

/** @param {any[]} ops */
const stack = (...ops) => ({ schema_version: 1, ops });

describe("diffSyncableOps", () => {
  it("returns the op whose value changed, with its absolute new value", () => {
    const d = diffSyncableOps(stack({ op: "exposure", value: 0.2 }, { op: "contrast", value: 5 }), stack({ op: "exposure", value: 0.8 }, { op: "contrast", value: 5 }));
    expect(d).toEqual({ ops: [{ op: "exposure", value: 0.8 }], removed: [] });
  });

  it("returns an op that was added", () => {
    const d = diffSyncableOps(stack(), stack({ op: "exposure", value: 0.8 }));
    expect(d.ops).toEqual([{ op: "exposure", value: 0.8 }]);
    expect(d.removed).toEqual([]);
  });

  it("names an op that was removed (a reset)", () => {
    const d = diffSyncableOps(stack({ op: "exposure", value: 0.8 }, { op: "contrast", value: 5 }), stack({ op: "contrast", value: 5 }));
    expect(d).toEqual({ ops: [], removed: ["exposure"] });
  });

  it("reports nothing for identical stacks, and for the same ops in another order or key order", () => {
    expect(diffSyncableOps(stack({ op: "exposure", value: 1 }), stack({ op: "exposure", value: 1 }))).toEqual({ ops: [], removed: [] });
    const a = stack({ op: "exposure", value: 1 }, { op: "hsl", bands: { red: { hue: 1, sat: 2 } } });
    const b = stack({ op: "hsl", bands: { red: { sat: 2, hue: 1 } } }, { op: "exposure", value: 1 });
    expect(diffSyncableOps(a, b)).toEqual({ ops: [], removed: [] });
  });

  it("treats a list op as one unit: one moved curve point returns the whole curve", () => {
    const before = stack({ op: "tone_curve", points: [{ x: 0, y: 0 }, { x: 0.5, y: 0.5 }, { x: 1, y: 1 }] });
    const after = stack({ op: "tone_curve", points: [{ x: 0, y: 0 }, { x: 0.5, y: 0.6 }, { x: 1, y: 1 }] });
    expect(diffSyncableOps(before, after).ops).toEqual(after.ops);
  });

  it("never returns the per-photo ops, changed or removed", () => {
    const before = stack({ op: "crop", x: 0, y: 0, width: 1, height: 1 }, { op: "brush_mask", id: "m1" }, { op: "ai_denoise", amount: 70 }, { op: "panel_hidden", panel: "basic" });
    const after = stack({ op: "crop", x: 0.1, y: 0, width: 0.9, height: 1 }, { op: "linear_gradient_mask", id: "m2" }, { op: "lens_correction", enabled: true }, { op: "perspective", vertical: 3 }, { op: "ai_denoise", amount: 10 });
    expect(diffSyncableOps(before, after)).toEqual({ ops: [], removed: [] });
  });

  it("returns only the syncable part of an edit that also touches an excluded op", () => {
    const d = diffSyncableOps(stack(), stack({ op: "exposure", value: 1 }, { op: "crop", x: 0.1 }));
    expect(d.ops).toEqual([{ op: "exposure", value: 1 }]);
  });

  it("excludes every mask kind, crop, lens profile, perspective, ai_denoise and panel_hidden", () => {
    for (const name of ["linear_gradient_mask", "radial_gradient_mask", "brush_mask", "luminance_range_mask", "color_range_mask", "segment_mask", "spot_mask", "red_eye_mask", "crop", "lens_correction", "perspective", "ai_denoise", "panel_hidden"]) {
      expect(AUTO_SYNC_EXCLUDED_OP_NAMES).toContain(name);
    }
  });
});
