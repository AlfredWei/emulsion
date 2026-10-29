import { describe, it, expect } from "vitest";
import { ADAPTIVE_PRESETS, scaleAdjustments, buildMask, isEdited, planAdaptiveApply, describeReport, adaptiveLabel } from "./adaptivePresets.js";
import { MAX_MASKS, listMasks, createBrushMask, addMask } from "$lib/api/develop.js";

const sky = /** @type {any} */ (ADAPTIVE_PRESETS.find((p) => p.id === "deepen-sky"));
const empty = /** @type {import("$lib/api/develop.js").EditStack} */ ({ schema_version: 1, ops: [{ op: "exposure", value: 0.5 }] });
const found = (/** @type {any} */ extra = {}) => /** @type {any} */ ({ outcome: "found", coverage: 0.4, reason: null, mask: { op: "segment_mask", logits: "AAAA", feather: 40, grow: 0, invert: false }, ...extra });
const notFound = /** @type {any} */ ({ outcome: "not_found", coverage: 0, reason: "no smooth region", mask: null });

describe("adaptive presets", () => {
  it("scales every adjustment by Amount once, to the numbers the mask panel shows", () => {
    expect(scaleAdjustments({ exposure: -0.3, contrast: 12, saturation: 10 }, 100)).toEqual({ exposure: -0.3, contrast: 12, saturation: 10 });
    expect(scaleAdjustments({ exposure: -0.3, contrast: 12 }, 60)).toEqual({ exposure: -0.18, contrast: 7.2 });
    expect(scaleAdjustments({ exposure: -0.3 }, 0).exposure).toBeCloseTo(0);
  });

  it("builds a stored mask: the resolver's raster, the scaled adjustments, an id and the preset it came from", () => {
    const m = /** @type {any} */ (buildMask(sky.rules[0], found(), 50, "deepen-sky", 0));
    expect(m).toMatchObject({ op: "segment_mask", logits: "AAAA", exposure: -0.15, contrast: 6, saturation: 5, source: { preset: "deepen-sky", rule: 0 } });
    expect(typeof m.id).toBe("string");
    expect(isEdited(m)).toBe(false);
    expect(isEdited({ ...m, contrast: 7 })).toBe(true);
  });

  it("a preset that sets only some adjustments still builds a mask the panel can open (exposure, contrast, saturation present)", () => {
    const shadows = ADAPTIVE_PRESETS.find((p) => p.id === "cool-shadows");
    const m = /** @type {any} */ (buildMask(/** @type {any} */ (shadows).rules[0], found(), 100, "cool-shadows", 0));
    expect(m.exposure).toBe(0);
    expect(m.contrast).toBe(0);
    expect(m.saturation).toBe(-5);
    expect(m.temperature).toBe(-8);
  });

  it("adds a found mask after the existing ones and leaves the rest of the stack alone", () => {
    const brush = createBrushMask();
    const start = addMask(empty, /** @type {any} */ (brush));
    const plan = planAdaptiveApply(start, sky, [found()], 100);
    expect(plan.changed).toBe(true);
    expect(plan.stack.ops[0]).toEqual(empty.ops[0]);
    expect(plan.stack.ops.map((o) => o.op)).toEqual(["exposure", "brush_mask", "segment_mask"]);
    expect(plan.report[0]).toMatchObject({ outcome: "found", action: "added" });
  });

  it("applies a check mask and says to check it; a not-found region adds nothing", () => {
    const plan = planAdaptiveApply(empty, sky, [found({ outcome: "check", reason: "only 4 % of the frame" })], 100);
    expect(listMasks(plan.stack)).toHaveLength(1);
    expect(describeReport(plan.report)).toContain("check it");
    const none = planAdaptiveApply(empty, sky, [notFound], 100);
    expect(none.changed).toBe(false);
    expect(none.stack).toBe(empty);
    expect(describeReport(none.report)).toBe("Sky: not found (no smooth region)");
  });

  it("changes nothing at Amount 0", () => {
    const plan = planAdaptiveApply(empty, sky, [found()], 0);
    expect(plan.changed).toBe(false);
    expect(plan.stack).toBe(empty);
  });

  it("re-applying replaces the preset's own earlier mask, not a second copy", () => {
    const once = planAdaptiveApply(empty, sky, [found()], 100).stack;
    const twice = planAdaptiveApply(once, sky, [found()], 40);
    expect(listMasks(twice.stack)).toHaveLength(1);
    expect(/** @type {any} */ (listMasks(twice.stack)[0]).exposure).toBe(-0.12);
    expect(twice.report[0].action).toBe("replaced");
  });

  it("keeps a mask the user edited and says so instead of replacing it", () => {
    const once = planAdaptiveApply(empty, sky, [found()], 100).stack;
    const edited = { ...once, ops: once.ops.map((o) => (o.op === "segment_mask" ? { ...o, exposure: -1 } : o)) };
    const plan = planAdaptiveApply(edited, sky, [found()], 100);
    expect(plan.changed).toBe(false);
    expect(listMasks(plan.stack)).toHaveLength(1);
    expect(/** @type {any} */ (listMasks(plan.stack)[0]).exposure).toBe(-1);
    expect(describeReport(plan.report)).toContain("was edited");
  });

  it("never goes past the mask limit", () => {
    let full = empty;
    for (let i = 0; i < MAX_MASKS; i++) full = addMask(full, /** @type {any} */ (createBrushMask()));
    const plan = planAdaptiveApply(full, sky, [found()], 100);
    expect(plan.changed).toBe(false);
    expect(listMasks(plan.stack)).toHaveLength(MAX_MASKS);
    expect(plan.report[0].why).toContain(`${MAX_MASKS} masks`);
  });

  it("labels the History row with the preset and the Amount", () => {
    expect(adaptiveLabel(sky, 60)).toBe("Adaptive: Deepen sky 60 %");
  });

  it("every starter rule is bound to a region the resolvers know", () => {
    for (const p of ADAPTIVE_PRESETS) for (const r of p.rules) expect(["sky", "face", "shadows"]).toContain(r.region);
  });
});
