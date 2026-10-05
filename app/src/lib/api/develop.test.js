import { describe, expect, test } from "vitest";
import {
  PRESET_EXCLUDED_OP_NAMES,
  presetEligibleOps,
  applyPresetOps,
  OP_GROUPS,
  copySettingsOps,
  computeAutoWhiteBalance,
  computeEyedropperWhiteBalance,
  computeAutoTone,
  PANEL_OP_NAMES,
  getGrain,
  getGrainStock,
  upsertGrain,
  GRAIN_STOCKS,
  grainStockValues,
  grainPickerState,
  buildGrainUniformData,
  HSL_BAND_NAMES,
  HSL_BAND_CENTERS_DEG,
  nearestHslBand,
  isPanelHidden,
  togglePanelVisibility,
  resetPanel,
  effectiveEditStack,
  MAX_MASKS,
  MAX_MODIFIERS,
  addMask,
  removeMask,
  listMasks,
  createRadialGradientMask,
  createBrushMask,
  createLuminanceRangeMask,
  createSpotMask,
  toModifierShape,
  createModifier,
  listModifiers,
  countModifiers,
  countBrushLayers,
  modifierBlockedReason,
  addModifier,
  updateModifier,
  removeModifier,
  findModifierOwner,
} from "./develop.js";

// Fixtures here model real op shapes (vignette/crop/hsl/masks) that the
// EditOp|Mask typedef union doesn't fully cover -- the same "loosen via
// JSDoc rather than fight strict inference for throwaway-shaped test
// data" practice this project already uses for its own diagnostic pages.
/** @typedef {import('./develop.js').EditStack} EditStack */

describe("presetEligibleOps", () => {
  test("keeps global tonal/color ops", () => {
    const stack = /** @type {EditStack} */ ({
      schema_version: 1,
      ops: [
        { op: "exposure", value: 0.5 },
        { op: "vignette", amount: 20, midpoint: 50, feather: 50 },
      ],
    });
    expect(presetEligibleOps(stack)).toEqual(stack);
  });

  test("strips crop and every mask kind", () => {
    const stack = /** @type {EditStack} */ ({
      schema_version: 1,
      ops: [
        { op: "exposure", value: 0.5 },
        { op: "crop", x: 0, y: 0, width: 1, height: 1, angle: 0 },
        { op: "linear_gradient_mask", id: "a" },
        { op: "radial_gradient_mask", id: "b" },
        { op: "brush_mask", id: "c" },
        { op: "luminance_range_mask", id: "d" },
        { op: "color_range_mask", id: "e" },
      ],
    });
    expect(presetEligibleOps(stack)).toEqual({
      schema_version: 1,
      ops: [{ op: "exposure", value: 0.5 }],
    });
  });

  test("PRESET_EXCLUDED_OP_NAMES covers exactly the 7 mask kinds plus crop, lens_correction, and perspective", () => {
    expect(PRESET_EXCLUDED_OP_NAMES.sort()).toEqual(
      [
        "crop",
        "lens_correction",
        "perspective",
        "linear_gradient_mask",
        "radial_gradient_mask",
        "brush_mask",
        "luminance_range_mask",
        "color_range_mask",
        "spot_mask",
        "red_eye_mask",
      ].sort(),
    );
  });
});

describe("applyPresetOps", () => {
  test("upserts preset ops by name, leaving unrelated target ops alone", () => {
    const target = /** @type {EditStack} */ ({
      schema_version: 1,
      ops: [
        { op: "exposure", value: 0.1 },
        { op: "crop", x: 0, y: 0, width: 1, height: 1, angle: 0 },
      ],
    });
    const preset = /** @type {EditStack} */ ({
      schema_version: 1,
      ops: [{ op: "exposure", value: 0.5 }, { op: "contrast", value: 10 }],
    });

    const merged = applyPresetOps(target, preset);

    expect(merged.ops).toEqual([
      { op: "crop", x: 0, y: 0, width: 1, height: 1, angle: 0 },
      { op: "exposure", value: 0.5 },
      { op: "contrast", value: 10 },
    ]);
  });

  test("never touches masks or crop on the target, since presets never carry those op names", () => {
    const target = /** @type {EditStack} */ ({
      schema_version: 1,
      ops: [
        { op: "linear_gradient_mask", id: "a", exposure: 0.3 },
        { op: "crop", x: 0.1, y: 0.1, width: 0.5, height: 0.5, angle: 5 },
      ],
    });
    const preset = /** @type {EditStack} */ ({ schema_version: 1, ops: [{ op: "exposure", value: 0.5 }] });

    const merged = applyPresetOps(target, preset);

    expect(merged.ops).toContainEqual({ op: "linear_gradient_mask", id: "a", exposure: 0.3 });
    expect(merged.ops).toContainEqual({ op: "crop", x: 0.1, y: 0.1, width: 0.5, height: 0.5, angle: 5 });
  });

  test("whole-object replace: an hsl preset op replaces ALL bands, not just the ones it set", () => {
    const target = /** @type {any} */ ({
      schema_version: 1,
      ops: [
        {
          op: "hsl",
          bands: { red: { hue: 10, saturation: 0, luminance: 0 }, orange: { hue: 0, saturation: 0, luminance: 0 } },
        },
      ],
    });
    const preset = /** @type {any} */ ({
      schema_version: 1,
      ops: [
        {
          op: "hsl",
          bands: { red: { hue: 0, saturation: 0, luminance: 0 }, orange: { hue: 40, saturation: 0, luminance: 0 } },
        },
      ],
    });

    const merged = applyPresetOps(target, preset);

    // The target's own red=10 adjustment is gone, not preserved -- this
    // is the documented known limitation, pinned here so a future change
    // to a smarter per-field merge shows up as an intentional test change.
    expect(merged.ops).toEqual([
      {
        op: "hsl",
        bands: { red: { hue: 0, saturation: 0, luminance: 0 }, orange: { hue: 40, saturation: 0, luminance: 0 } },
      },
    ]);
  });
});

describe("copySettingsOps", () => {
  test("keeps only ops belonging to a selected group", () => {
    const stack = /** @type {EditStack} */ ({
      schema_version: 1,
      ops: [
        { op: "exposure", value: 0.5 },
        { op: "temperature", value: 200 },
        { op: "hsl", bands: {} },
      ],
    });

    expect(copySettingsOps(stack, ["basic_tone"]).ops).toEqual([{ op: "exposure", value: 0.5 }]);
  });

  test("unions ops across multiple selected groups", () => {
    const stack = /** @type {EditStack} */ ({
      schema_version: 1,
      ops: [
        { op: "exposure", value: 0.5 },
        { op: "temperature", value: 200 },
        { op: "hsl", bands: {} },
      ],
    });

    const copied = copySettingsOps(stack, ["basic_tone", "white_balance"]);

    expect(copied.ops).toEqual([{ op: "exposure", value: 0.5 }, { op: "temperature", value: 200 }]);
  });

  test("an unselected group's ops are dropped even if every group is preset-eligible", () => {
    const stack = /** @type {any} */ ({
      schema_version: 1,
      ops: [{ op: "vignette", amount: 20, midpoint: 50, feather: 50 }],
    });

    expect(copySettingsOps(stack, ["basic_tone"]).ops).toEqual([]);
  });

  test("still strips crop/masks/lens/perspective even if the caller passes every group id", () => {
    const stack = /** @type {EditStack} */ ({
      schema_version: 1,
      ops: [
        { op: "exposure", value: 0.5 },
        { op: "crop", x: 0, y: 0, width: 1, height: 1, angle: 0 },
        { op: "brush_mask", id: "a" },
      ],
    });

    const copied = copySettingsOps(
      stack,
      OP_GROUPS.map((g) => g.id),
    );

    expect(copied.ops).toEqual([{ op: "exposure", value: 0.5 }]);
  });

  test("no selected groups yields an empty ops list", () => {
    const stack = /** @type {EditStack} */ ({ schema_version: 1, ops: [{ op: "exposure", value: 0.5 }] });
    expect(copySettingsOps(stack, []).ops).toEqual([]);
  });
});

describe("computeAutoWhiteBalance", () => {
  test("neutral gray yields 0 temp and 0 tint", () => {
    const { temperature, tint } = computeAutoWhiteBalance({ r: 0.5, g: 0.5, b: 0.5 });
    expect(temperature).toBe(0);
    expect(tint).toBe(0);
  });

  test("warm image (high red, low blue) yields negative temperature (cooling)", () => {
    const { temperature } = computeAutoWhiteBalance({ r: 0.7, g: 0.5, b: 0.3 });
    expect(temperature).toBeLessThan(0);
  });

  test("cool image (low red, high blue) yields positive temperature (warming)", () => {
    const { temperature } = computeAutoWhiteBalance({ r: 0.3, g: 0.5, b: 0.7 });
    expect(temperature).toBeGreaterThan(0);
  });

  test("green cast yields positive tint (magenta shift)", () => {
    const { tint } = computeAutoWhiteBalance({ r: 0.4, g: 0.7, b: 0.4 });
    expect(tint).toBeGreaterThan(0);
  });
});

describe("computeEyedropperWhiteBalance", () => {
  test("neutral sample yields 0 temp and 0 tint", () => {
    const { temperature, tint } = computeEyedropperWhiteBalance({ r: 0.5, g: 0.5, b: 0.5 });
    expect(temperature).toBe(0);
    expect(tint).toBe(0);
  });

  test("warm sampled patch cools down", () => {
    const { temperature } = computeEyedropperWhiteBalance({ r: 0.8, g: 0.5, b: 0.2 });
    expect(temperature).toBeLessThan(0);
  });
});

describe("computeAutoTone", () => {
  test("balanced histogram produces sensible parameters", () => {
    const r = new Uint32Array(256).fill(10);
    const g = new Uint32Array(256).fill(10);
    const b = new Uint32Array(256).fill(10);

    const result = computeAutoTone({ r, g, b });
    expect(typeof result.exposure).toBe("number");
    expect(typeof result.contrast).toBe("number");
    expect(typeof result.highlights).toBe("number");
    expect(typeof result.shadows).toBe("number");
    expect(typeof result.whites).toBe("number");
    expect(typeof result.blacks).toBe("number");
  });

  test("dark under-exposed histogram produces positive exposure boost", () => {
    const r = new Uint32Array(256);
    const g = new Uint32Array(256);
    const b = new Uint32Array(256);
    // All pixels concentrated in shadows [0..50]
    for (let i = 0; i <= 50; i++) {
      r[i] = 100;
      g[i] = 100;
      b[i] = 100;
    }
    const result = computeAutoTone({ r, g, b });
    expect(result.exposure).toBeGreaterThan(0);
  });
});


// RFC-0013: per-panel visibility toggle + reset.
describe("panel visibility and reset", () => {
  test("isPanelHidden is false with no markers, true once one is added", () => {
    const stack = /** @type {EditStack} */ ({ schema_version: 1, ops: [{ op: "dehaze", value: 50 }] });
    expect(isPanelHidden(stack, "dehaze")).toBe(false);
    const hidden = togglePanelVisibility(stack, "dehaze");
    expect(isPanelHidden(hidden, "dehaze")).toBe(true);
    // the real op is untouched -- toggling visibility never touches values
    expect(hidden.ops).toContainEqual({ op: "dehaze", value: 50 });
  });

  test("toggling visibility twice round-trips to the original stack", () => {
    const stack = /** @type {any} */ ({ schema_version: 1, ops: [{ op: "vignette", amount: 30 }] });
    const roundTripped = togglePanelVisibility(togglePanelVisibility(stack, "vignette"), "vignette");
    expect(roundTripped).toEqual(stack);
  });

  test("resetPanel removes only that panel's own op names, per the panel->op-name table", () => {
    for (const [panel, names] of Object.entries(PANEL_OP_NAMES)) {
      const stack = /** @type {EditStack} */ ({
        schema_version: 1,
        ops: [...names.map((op) => ({ op, value: 1 })), { op: "exposure", value: 5 }],
      });
      const reset = resetPanel(stack, panel);
      const remaining = reset.ops.map((o) => o.op);
      for (const name of names) expect(remaining).not.toContain(name);
      // an op outside this panel's own table survives, unless the panel
      // itself IS "basic" (which owns "exposure")
      if (panel !== "basic") expect(remaining).toContain("exposure");
    }
  });

  test("resetPanel never touches that panel's own panel_hidden marker", () => {
    const stack = /** @type {EditStack} */ ({
      schema_version: 1,
      ops: [
        { op: "dehaze", value: 50 },
        { op: "panel_hidden", panel: "dehaze" },
      ],
    });
    const reset = resetPanel(stack, "dehaze");
    expect(isPanelHidden(reset, "dehaze")).toBe(true);
    expect(reset.ops.some((o) => o.op === "dehaze")).toBe(false);
  });

  test("effectiveEditStack strips a hidden panel's ops and its own marker, leaving other panels untouched", () => {
    const stack = /** @type {EditStack} */ ({
      schema_version: 1,
      ops: [
        { op: "exposure", value: 20 },
        { op: "dehaze", value: 50 },
        { op: "panel_hidden", panel: "dehaze" },
      ],
    });
    const effective = effectiveEditStack(stack);
    expect(effective.ops.map((o) => o.op)).toEqual(["exposure"]);
    // the real stack (what a slider reads/writes) is untouched
    expect(stack.ops).toHaveLength(3);
  });

  test("effectiveEditStack returns the same stack unchanged when nothing is hidden", () => {
    const stack = /** @type {EditStack} */ ({ schema_version: 1, ops: [{ op: "exposure", value: 20 }] });
    expect(effectiveEditStack(stack)).toBe(stack);
  });

  test("effectiveEditStack strips every op a multi-op panel owns (Noise Reduction: luma_nr + color_nr)", () => {
    const stack = /** @type {any} */ ({
      schema_version: 1,
      ops: [
        { op: "luma_nr", amount: 40 },
        { op: "color_nr", amount: 40 },
        { op: "panel_hidden", panel: "noise_reduction" },
      ],
    });
    expect(effectiveEditStack(stack).ops).toEqual([]);
  });
});

// RFC-0019: HSL band centers at real hues.
describe("HSL band centers", () => {
  test("sit at the hue positions of the colors they name, index-aligned with HSL_BAND_NAMES", () => {
    const at = (/** @type {string} */ name) => HSL_BAND_CENTERS_DEG[HSL_BAND_NAMES.indexOf(name)];
    expect([at("red"), at("orange"), at("yellow"), at("green")]).toEqual([0, 30, 60, 120]);
    expect([at("aqua"), at("blue"), at("purple"), at("magenta")]).toEqual([180, 240, 270, 300]);
  });

  test("nearestHslBand names real yellow/orange/green/blue correctly (old 45deg spacing called 60deg 'orange')", () => {
    expect(nearestHslBand(60)).toBe("yellow");
    expect(nearestHslBand(30)).toBe("orange");
    expect(nearestHslBand(120)).toBe("green");
    expect(nearestHslBand(240)).toBe("blue");
    // a chartreuse pixel (hue 90) is now the yellow/green tie's green side of
    // the boundary, no longer "yellow" by a wide margin
    expect(nearestHslBand(100)).toBe("green");
  });

  test("nearestHslBand picks the same band the render path weights highest, across unequal gaps and the 360->0 seam", () => {
    // weight of band i at a hue, mirroring hsl_band_weight (hsl_split.rs)
    const weight = (/** @type {number} */ hue, /** @type {number} */ i) => {
      const n = HSL_BAND_CENTERS_DEG.length;
      const c = HSL_BAND_CENTERS_DEG[i];
      const d = ((((hue - c + 180) % 360) + 360) % 360) - 180;
      const gap = d >= 0 ? (((HSL_BAND_CENTERS_DEG[(i + 1) % n] - c) % 360) + 360) % 360 : (((c - HSL_BAND_CENTERS_DEG[(i + n - 1) % n]) % 360) + 360) % 360;
      const dist = Math.abs(d);
      return dist >= gap ? 0 : 0.5 * (Math.cos((dist / gap) * Math.PI) + 1);
    };
    for (let hue = 0.3; hue < 360; hue += 1.7) {
      const best = HSL_BAND_NAMES[HSL_BAND_NAMES.map((_, i) => weight(hue, i)).reduce((bi, w, i, ws) => (w > ws[bi] ? i : bi), 0)];
      expect(nearestHslBand(hue), `hue ${hue}`).toBe(best);
    }
  });
});

describe("Grain tone (RFC-0022 slice 2)", () => {
  test("an op without `tone` reads as 0, so stored edits and built-in presets are unchanged", () => {
    const stack = /** @type {any} */ ({ schema_version: 1, ops: [{ op: "grain", amount: 25, size: 30, roughness: 40 }] });
    expect(getGrain(stack)).toEqual({ amount: 25, size: 30, roughness: 40, tone: 0, chroma: 0 });
  });

  test("a tone patch keeps the other fields, and a later patch keeps tone", () => {
    const base = /** @type {any} */ ({ schema_version: 1, ops: [{ op: "grain", amount: 25, size: 30, roughness: 40 }] });
    const toned = upsertGrain(base, { tone: 70 });
    expect(getGrain(toned)).toEqual({ amount: 25, size: 30, roughness: 40, tone: 70, chroma: 0 });
    expect(getGrain(upsertGrain(toned, { amount: 55 }))).toEqual({ amount: 55, size: 30, roughness: 40, tone: 70, chroma: 0 });
  });

  test("tone is the fourth uniform float, where the WGSL Grain struct reads it", () => {
    const data = buildGrainUniformData({ amount: 10, size: 20, roughness: 30, tone: 40, chroma: 50 });
    expect(Array.from(data).slice(0, 4)).toEqual([10, 20, 30, 40]);
  });
});

describe("Grain colour (RFC-0022 slice 3)", () => {
  test("an op without `chroma` reads as 0 (mono grain), so stored edits and built-in presets are unchanged", () => {
    const stack = /** @type {any} */ ({ schema_version: 1, ops: [{ op: "grain", amount: 25, size: 30, roughness: 40, tone: 20 }] });
    expect(getGrain(stack).chroma).toBe(0);
  });

  test("a chroma patch keeps the other fields, and later patches keep chroma", () => {
    const base = /** @type {any} */ ({ schema_version: 1, ops: [{ op: "grain", amount: 25, size: 30, roughness: 40, tone: 20 }] });
    const coloured = upsertGrain(base, { chroma: 80 });
    expect(getGrain(coloured)).toEqual({ amount: 25, size: 30, roughness: 40, tone: 20, chroma: 80 });
    expect(getGrain(upsertGrain(coloured, { tone: 50 })).chroma).toBe(80);
  });

  test("the uniform is 8 floats (the WGSL Grain struct is 32 bytes): chroma in slot 4, three zero pads", () => {
    const data = buildGrainUniformData({ amount: 10, size: 20, roughness: 30, tone: 40, chroma: 50 });
    expect(Array.from(data)).toEqual([10, 20, 30, 40, 50, 0, 0, 0]);
  });
});

describe("Grain film stocks (RFC-0022 slice 4)", () => {
  const ids = /** @type {Array<keyof typeof GRAIN_STOCKS>} */ (Object.keys(GRAIN_STOCKS));
  const none = { amount: 0, size: 25, roughness: 50, tone: 0, chroma: 0 };

  test("every stock's five values are inside the sliders' 0..100 range, with a provenance tag and caption", () => {
    for (const id of ids) {
      const v = grainStockValues(id);
      for (const x of Object.values(v)) {
        expect(Number.isInteger(x)).toBe(true);
        expect(x).toBeGreaterThanOrEqual(0);
        expect(x).toBeLessThanOrEqual(100);
      }
      expect(GRAIN_STOCKS[id].tag).toMatch(/^[DRA](·[DRA])*$/);
      expect(GRAIN_STOCKS[id].caption.length).toBeGreaterThan(0);
    }
  });

  test("the table matches RFC-0022 §2.4: Size slider = (µm − 6)/30·100, B&W stocks have Colour 0, Gold 200 grainier than Portra 400", () => {
    const um = { portra400: 11, gold200: 13, superia400: 12, cinestill800t: 16, trix400: 18, hp5: 18, delta100: 9 };
    for (const id of ids) expect(GRAIN_STOCKS[id].size).toBe(Math.round(((um[id] - 6) / 30) * 100));
    for (const id of ids) {
      if (GRAIN_STOCKS[id].group === "bw") expect(GRAIN_STOCKS[id].chroma).toBe(0);
      else expect(GRAIN_STOCKS[id].chroma).toBeGreaterThan(0);
    }
    expect(GRAIN_STOCKS.portra400.amount).toBe(40);
    expect(GRAIN_STOCKS.gold200.amount).toBe(59);
  });

  test("no two stocks share all five values (the picker's derived selection is unambiguous)", () => {
    const keys = ids.map((id) => JSON.stringify(grainStockValues(id)));
    expect(new Set(keys).size).toBe(ids.length);
  });

  test("the picker shows a stock exactly while all five values equal its preset, else None (Amount 0) or Custom", () => {
    for (const id of ids) expect(grainPickerState(grainStockValues(id), id)).toEqual({ selected: id, resetTo: null });
    expect(grainPickerState(none, null)).toEqual({ selected: "none", resetTo: null });
    expect(grainPickerState({ ...none, amount: 30 }, null)).toEqual({ selected: "custom", resetTo: null });
    const drifted = { ...grainStockValues("portra400"), roughness: 21 };
    expect(grainPickerState(drifted, "portra400")).toEqual({ selected: "custom", resetTo: "portra400" });
    // Amount dragged to 0 after loading a stock: shown as None, but Reset is still offered.
    expect(grainPickerState({ ...grainStockValues("trix400"), amount: 0 }, "trix400")).toEqual({ selected: "none", resetTo: "trix400" });
    // An unknown remembered id (a stock removed in a later release) offers nothing.
    expect(grainPickerState(drifted, "kodachrome")).toEqual({ selected: "custom", resetTo: null });
  });

  test("an op without `stock` reads null, and the stock survives slider patches until cleared", () => {
    const base = /** @type {any} */ ({ schema_version: 1, ops: [{ op: "grain", amount: 25, size: 30, roughness: 40 }] });
    expect(getGrainStock(base)).toBeNull();
    const loaded = upsertGrain(base, { ...grainStockValues("hp5"), stock: "hp5" });
    expect(getGrainStock(loaded)).toBe("hp5");
    expect(getGrain(loaded)).toEqual(grainStockValues("hp5"));
    const edited = upsertGrain(loaded, { amount: 10 });
    expect(getGrainStock(edited)).toBe("hp5");
    expect(getGrain(edited).amount).toBe(10);
    expect(getGrainStock(upsertGrain(edited, { stock: null }))).toBeNull();
  });

  test("`stock` never reaches the GPU uniform (still 8 floats from the five values)", () => {
    const g = getGrain(upsertGrain(/** @type {any} */ ({ schema_version: 1, ops: [] }), { ...grainStockValues("portra400"), stock: "portra400" }));
    expect(Array.from(buildGrainUniformData(g))).toEqual([40, 17, 20, 50, 80, 0, 0, 0]);
  });
});

describe("Composable masking model (RFC-0025)", () => {
  /** @returns {any} */
  const stack = (/** @type {any[]} */ ...ops) => ({ schema_version: 1, ops });
  const radial = (/** @type {string} */ id) => ({ ...createRadialGradientMask({ x: 0.5, y: 0.5 }, 0.2, 0.2), id });

  test("toModifierShape drops identity and adjustments, keeps geometry", () => {
    const shape = /** @type {any} */ (toModifierShape(radial("a")));
    expect(shape.op).toBe("radial_gradient_mask");
    expect(shape.center).toEqual({ x: 0.5, y: 0.5 });
    for (const k of ["id", "exposure", "contrast", "saturation"]) expect(k in shape).toBe(false);
  });

  test("addModifier appends in order and leaves the input stack untouched", () => {
    const s0 = stack(radial("a"));
    const m1 = createModifier("subtract", createBrushMask(), "m1");
    const m2 = createModifier("intersect", createLuminanceRangeMask(), "m2");
    const s2 = addModifier(addModifier(s0, "a", m1), "a", m2);
    expect(listModifiers(/** @type {any} */ (listMasks(s2)[0])).map((m) => m.id)).toEqual(["m1", "m2"]);
    expect("modifiers" in s0.ops[0]).toBe(false);
  });

  test("spot and red-eye can neither take nor be a modifier", () => {
    const spot = createSpotMask({ x: 0.5, y: 0.5, radius: 0.05 }, "s");
    const s = stack(spot, radial("a"));
    expect(modifierBlockedReason(s, "s", "radial_gradient_mask")).not.toBeNull();
    expect(modifierBlockedReason(s, "a", "spot_mask")).not.toBeNull();
    expect(modifierBlockedReason(s, "a", "red_eye_mask")).not.toBeNull();
    expect(modifierBlockedReason(s, "a", "linear_gradient_mask")).toBeNull();
    expect(modifierBlockedReason(s, "nope", "linear_gradient_mask")).not.toBeNull();
  });

  test("total modifier cap is enforced across masks", () => {
    let s = stack(radial("a"), radial("b"));
    for (let i = 0; i < MAX_MODIFIERS; i++) {
      s = addModifier(s, i % 2 ? "a" : "b", createModifier("add", radial("x"), `m${i}`));
    }
    expect(countModifiers(s)).toBe(MAX_MODIFIERS);
    expect(modifierBlockedReason(s, "a", "radial_gradient_mask")).toMatch(/Maximum/);
    expect(addModifier(s, "a", createModifier("add", radial("x"))))
      .toBe(s);
  });

  test("brush modifiers share the brush-layer budget with brush and spot masks", () => {
    let s = stack(radial("a"));
    for (let i = 0; i < MAX_MASKS - 1; i++) s = addMask(s, createBrushMask(`b${i}`));
    expect(countBrushLayers(s)).toBe(MAX_MASKS - 1);
    s = addModifier(s, "a", createModifier("subtract", createBrushMask(), "mb"));
    expect(countBrushLayers(s)).toBe(MAX_MASKS);
    expect(modifierBlockedReason(s, "a", "brush_mask")).toMatch(/Brush layers/);
    // a non-brush shape is still allowed
    expect(modifierBlockedReason(s, "a", "linear_gradient_mask")).toBeNull();
  });

  test("updateModifier merges into the shape; removeModifier drops the key when empty", () => {
    let s = addModifier(stack(radial("a")), "a", createModifier("add", radial("x"), "m1"));
    s = updateModifier(s, "a", "m1", { combine: "subtract", shape: { feather: 77 } });
    const m = listModifiers(/** @type {any} */ (listMasks(s)[0]))[0];
    expect(m.combine).toBe("subtract");
    expect(/** @type {any} */ (m.shape).feather).toBe(77);
    expect(/** @type {any} */ (m.shape).radiusX).toBe(0.2);
    s = removeModifier(s, "a", "m1");
    expect("modifiers" in s.ops[0]).toBe(false);
  });

  test("removeMask takes its modifiers with it; unrelated masks are untouched", () => {
    const s = addModifier(stack(radial("a"), radial("b")), "a", createModifier("add", radial("x"), "m1"));
    const r = removeMask(s, "a");
    expect(listMasks(r).map((m) => m.id)).toEqual(["b"]);
    expect(countModifiers(r)).toBe(0);
  });

  test("findModifierOwner finds the owning mask, or null", () => {
    const s = addModifier(stack(radial("a"), radial("b")), "b", createModifier("add", radial("x"), "m1"));
    expect(findModifierOwner(s, "m1")).toBe("b");
    expect(findModifierOwner(s, "nope")).toBeNull();
    expect(findModifierOwner(s, "a")).toBeNull(); // a mask id is not a modifier id
  });
});
