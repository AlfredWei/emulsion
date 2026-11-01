import { describe, it, expect } from "vitest";
import { ADAPTIVE_PRESETS, ADAPTIVE_PLANNED, needsFaces, scaleAdjustments, buildMask, handOffAdjustments, canSelectItYourself, selectItYourselfAdds, selectItYourselfLabel, selectItYourselfTarget, isEdited, planAdaptiveApply, describeReport, describeRule, batchStatus, presetMasks, adaptiveLabel } from "./adaptivePresets.js";
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
    expect(plan.added).toEqual([/** @type {any} */ (listMasks(plan.stack)[1]).id]);
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
    expect(describeReport(none.report)).toBe("Sky: not found: no smooth region");
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
    for (const p of ADAPTIVE_PRESETS) for (const r of p.rules) expect(["sky", "face", "shadows", "person"]).toContain(r.region);
  });
  it("a check outcome is written into the mask's source, so the mask panel can ask the user to look", () => {
    const checked = /** @type {any} */ (buildMask(sky.rules[0], found({ outcome: "check", reason: "only 4 % of the frame" }), 100, "deepen-sky", 0));
    expect(checked.source.check).toBe("only 4 % of the frame");
    const plain = /** @type {any} */ (buildMask(sky.rules[0], found(), 100, "deepen-sky", 0));
    expect(plain.source.check).toBeUndefined();
    expect(isEdited(checked)).toBe(false);
  });

  it("any change to the mask counts as the user's: an adjustment the preset did not set, a modifier, a range; key order does not", () => {
    const m = /** @type {any} */ (buildMask(ADAPTIVE_PRESETS[2].rules[0], { outcome: "found", coverage: 0.2, reason: null, mask: { op: "luminance_range_mask", rangeMin: 0, rangeMax: 25, feather: 30, invert: false } }, 100, "cool-shadows", 0));
    expect(isEdited(m)).toBe(false);
    expect(isEdited({ ...m, exposure: 0.5 })).toBe(true);
    expect(isEdited({ ...m, rangeMax: 30 })).toBe(true);
    expect(isEdited({ ...m, modifiers: [{ id: "x" }] })).toBe(true);
    const reordered = Object.fromEntries(Object.entries(m).reverse());
    expect(isEdited(reordered)).toBe(false);
    expect(isEdited(JSON.parse(JSON.stringify(m)))).toBe(false);
  });

  it("replaceEdited replaces a mask the user changed; without it the mask is kept and the report says why", () => {
    const first = planAdaptiveApply(empty, sky, [found()], 100);
    const mine = { ...first.stack, ops: first.stack.ops.map((o) => (/** @type {any} */ (o).source ? { ...o, exposure: -1 } : o)) };
    const kept = planAdaptiveApply(/** @type {any} */ (mine), sky, [found()], 100);
    expect(kept.changed).toBe(false);
    expect(describeRule(kept.report[0]).kind).toBe("skipped");
    const replaced = planAdaptiveApply(/** @type {any} */ (mine), sky, [found()], 100, true);
    expect(replaced.changed).toBe(true);
    expect(presetMasks(replaced.stack, "deepen-sky")).toHaveLength(1);
    expect(/** @type {any} */ (presetMasks(replaced.stack, "deepen-sky")[0]).exposure).toBeCloseTo(-0.3);
  });

  it("presetMasks finds only the masks a preset wrote, and describeRule gives each outcome a kind", () => {
    const plan = planAdaptiveApply(addMask(empty, /** @type {any} */ (createBrushMask())), sky, [found()], 100);
    expect(presetMasks(plan.stack, "deepen-sky")).toHaveLength(1);
    expect(presetMasks(plan.stack, "cool-shadows")).toHaveLength(0);
    expect(describeRule(plan.report[0])).toEqual({ name: "Sky", text: "40 % of the frame", kind: "found" });
    expect(describeRule({ region: "face", outcome: "not_found", coverage: 0, reason: "no face above 24 px", action: "skipped" }).kind).toBe("not_found");
  });

  it("the planned presets are not appliable ones", () => {
    for (const p of ADAPTIVE_PLANNED) expect(ADAPTIVE_PRESETS.some((a) => a.id === p.id)).toBe(false);
  });
  it("batchStatus names what happened to a photo: applied, to check, nothing, or kept", () => {
    const rule = (/** @type {any} */ over) => /** @type {any} */ ({ region: "sky", outcome: "found", coverage: 0.3, reason: null, action: "added", ...over });
    expect(batchStatus([rule({})])).toBe("applied");
    expect(batchStatus([rule({ outcome: "check" })])).toBe("check");
    expect(batchStatus([rule({ outcome: "not_found", action: "skipped" })])).toBe("nothing");
    expect(batchStatus([rule({ action: "skipped", why: "an earlier mask from this preset was edited; confirm to replace it" })])).toBe("kept");
    expect(batchStatus([rule({ action: "skipped", why: "the photo already has 12 masks" })])).toBe("nothing");
  });
  it("the face and person presets need the faces scanned first; sky and shadows do not", () => {
    const need = ADAPTIVE_PRESETS.filter(needsFaces).map((p) => p.id);
    expect(need).toEqual(["brighten-faces", "lift-people"]);
  });

  it("hand-off: the flagged mask's adjustments as they stand now, only the preset's controls and the base three", () => {
    const mask = /** @type {any} */ (buildMask(sky.rules[0], found({ outcome: "check", reason: "look" }), 50, "deepen-sky", 0));
    const stack = addMask(empty, { ...mask, exposure: -0.4, highlights: 30 });
    expect(handOffAdjustments(stack, mask.id)).toEqual({ exposure: -0.4, contrast: 6, saturation: 5 });
  });

  it("hand-off: nothing for a mask no preset wrote, or one that is gone", () => {
    const brush = /** @type {any} */ (createBrushMask("b1"));
    expect(handOffAdjustments(addMask(empty, brush), brush.id)).toBeNull();
    expect(handOffAdjustments(empty, "nope")).toBeNull();
  });

  it("a person mask takes the click as an addition and says so, a sky or a face is replaced", () => {
    expect(selectItYourselfAdds("person")).toBe(true);
    expect(["sky", "face", "shadows"].some(selectItYourselfAdds)).toBe(false);
    expect(selectItYourselfLabel("person")).toBe("Add a person yourself");
    expect(selectItYourselfLabel("sky")).toBe("Select it yourself");
  });

  it("offers Select it yourself on a flagged mask of a sky, face or person rule, and nowhere else", () => {
    expect(["sky", "face", "person"].every(canSelectItYourself)).toBe(true);
    expect(canSelectItYourself("shadows")).toBe(false);
    const flagged = (/** @type {string} */ preset, /** @type {any} */ check = "the model was unsure") => ({ source: { preset, rule: 0, sig: "x", check } });
    expect(selectItYourselfTarget(flagged("lift-people"))).toEqual({ presetId: "lift-people", ruleIndex: 0, region: "person", adds: true });
    expect(selectItYourselfTarget(flagged("deepen-sky"))).toEqual({ presetId: "deepen-sky", ruleIndex: 0, region: "sky", adds: false });
    expect(selectItYourselfTarget(flagged("lift-people", null))).toBeNull(); // a clean mask: nothing to redo
    expect(selectItYourselfTarget(flagged("cool-shadows"))).toBeNull(); // no click-select equivalent for a brightness range
    expect(selectItYourselfTarget(flagged("no-such-preset"))).toBeNull();
    expect(selectItYourselfTarget({ op: "brush_mask" })).toBeNull(); // not a preset mask
    expect(selectItYourselfTarget(null)).toBeNull();
  });
});
