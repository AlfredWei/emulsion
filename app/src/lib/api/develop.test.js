import { describe, expect, test } from "vitest";
import { PRESET_EXCLUDED_OP_NAMES, presetEligibleOps, applyPresetOps } from "./develop.js";

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

  test("PRESET_EXCLUDED_OP_NAMES covers exactly the 5 mask kinds plus crop and lens_correction", () => {
    expect(PRESET_EXCLUDED_OP_NAMES.sort()).toEqual(
      [
        "crop",
        "lens_correction",
        "linear_gradient_mask",
        "radial_gradient_mask",
        "brush_mask",
        "luminance_range_mask",
        "color_range_mask",
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
