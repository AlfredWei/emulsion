import { describe, expect, test } from "vitest";
import { packMasks, rasterTargets, MASK_KIND, COMBINE_CODE } from "./maskPack.js";
import { MAX_MASKS, MAX_MODIFIERS, MASK_STRIDE, MOD_STRIDE } from "./gpuHandles.js";
import { MAX_MASKS as API_MAX_MASKS, MAX_MODIFIERS as API_MAX_MODIFIERS } from "$lib/api/develop.js";
import { WGSL } from "./shaders/index.js";

/** @param {string} id @param {any} [extra] */
const radial = (id, extra = {}) => ({
  op: "radial_gradient_mask", id, center: { x: 0.4, y: 0.6 }, radiusX: 0.2, radiusY: 0.3,
  feather: 25, invert: true, exposure: 1.5, contrast: 10, saturation: -20, ...extra,
});
const linearShape = { op: "linear_gradient_mask", start: { x: 0.1, y: 0.2 }, end: { x: 0.9, y: 0.8 }, feather: 40, invert: false };
const brushShape = { op: "brush_mask", dabs: [], invert: true };
const lumShape = { op: "luminance_range_mask", rangeMin: 30, rangeMax: 60, feather: 15, invert: false };
const colShape = { op: "color_range_mask", refColor: { r: 0.1, g: 0.2, b: 0.3 }, range: 25, feather: 10, invert: true };
const noLayer = () => undefined;

describe("maskPack", () => {
  test("budgets agree between the GPU module and the model", () => {
    expect(MAX_MASKS).toBe(API_MAX_MASKS);
    expect(MAX_MODIFIERS).toBe(API_MAX_MODIFIERS);
  });

  test("the WGSL declares the same array sizes and a 4th vec4 on Mask", () => {
    expect(WGSL).toContain(`const MAX_MASKS = ${MAX_MASKS};`);
    expect(WGSL).toContain(`const MAX_MODS = ${MAX_MODIFIERS};`);
    expect(WGSL).toMatch(/mods: vec4<f32>,/);
    expect(MASK_STRIDE).toBe(16);
    expect(MOD_STRIDE).toBe(12);
  });

  test("a mask without modifiers packs the pre-RFC layout and an empty modifier range", () => {
    const { maskData, modCount } = packMasks([radial("a")], noLayer);
    expect(Array.from(maskData.slice(0, 16))).toEqual([0.4, 0.6, 0.2, 0.3, 25, 1, 1, 0, 1.5, 10, -20, 0, 0, 0, 0, 0].map(Math.fround));
    expect(modCount).toBe(0);
  });

  test("modifiers are flattened in mask order with first/count per mask", () => {
    const masks = [
      radial("a", { modifiers: [{ id: "a1", combine: "subtract", shape: linearShape }, { id: "a2", combine: "intersect", shape: lumShape }] }),
      radial("b"),
      radial("c", { modifiers: [{ id: "c1", combine: "add", shape: colShape }] }),
    ];
    const { maskData, modData, modCount } = packMasks(masks, noLayer);
    expect(modCount).toBe(3);
    expect([maskData[12], maskData[13]]).toEqual([0, 2]);
    expect([maskData[MASK_STRIDE + 12], maskData[MASK_STRIDE + 13]]).toEqual([2, 0]);
    expect([maskData[2 * MASK_STRIDE + 12], maskData[2 * MASK_STRIDE + 13]]).toEqual([2, 1]);
    // modifier 0: linear, subtract
    expect(Array.from(modData.slice(0, 9))).toEqual([0.1, 0.2, 0.9, 0.8, 40, 0, MASK_KIND.linear_gradient_mask, 0, COMBINE_CODE.subtract].map(Math.fround));
    // modifier 1: luminance, intersect
    expect(Array.from(modData.slice(MOD_STRIDE, MOD_STRIDE + 9))).toEqual([30, 60, 0, 0, 15, 0, MASK_KIND.luminance_range_mask, 0, COMBINE_CODE.intersect]);
    // modifier 2: colour, add, inverted
    expect(Array.from(modData.slice(2 * MOD_STRIDE, 2 * MOD_STRIDE + 9))).toEqual([0.1, 0.2, 0.3, 25, 10, 1, MASK_KIND.color_range_mask, 0, COMBINE_CODE.add].map(Math.fround));
  });

  test("a modifier carries no adjustment slots", () => {
    const { modData } = packMasks([radial("a", { modifiers: [{ id: "m", combine: "add", shape: linearShape }] })], noLayer);
    expect(Array.from(modData.slice(9, MOD_STRIDE))).toEqual([0, 0, 0]);
  });

  test("brush layers come from the id of the mask or the modifier", () => {
    /** @type {Record<string, number>} */
    const layers = { b: 3, mb: 5 };
    const masks = [{ op: "brush_mask", id: "b", dabs: [], invert: false, exposure: 0, contrast: 0, saturation: 0, modifiers: [{ id: "mb", combine: "subtract", shape: brushShape }] }];
    const { maskData, modData } = packMasks(masks, (id) => layers[id]);
    expect(maskData[7]).toBe(3);
    expect(modData[7]).toBe(5);
    expect(modData[5]).toBe(1); // inverted
  });

  test("spot and red eye keep their layouts and never get modifiers", () => {
    const spot = { op: "spot_mask", id: "s", dabs: [{ x: 0.5, y: 0.5, radius: 0.1 }], feather: 20, mode: "heal", sourceOffset: { dx: 0.1, dy: -0.1 }, modifiers: [{ id: "x", combine: "add", shape: linearShape }] };
    const eye = { op: "red_eye_mask", id: "e", center: { x: 0.3, y: 0.3 }, radiusX: 0.1, radiusY: 0.1, feather: 10, pupilSize: 40, darken: 60 };
    const { maskData, modCount } = packMasks([spot, eye], () => 2);
    expect(modCount).toBe(0);
    expect([maskData[6], maskData[7], maskData[8]]).toEqual([MASK_KIND.spot_mask, 2, 1]);
    expect([maskData[12], maskData[13]]).toEqual([0, 0]);
    const o = MASK_STRIDE;
    expect([maskData[o + 5], maskData[o + 6], maskData[o + 11]]).toEqual([40, MASK_KIND.red_eye_mask, 60]);
  });

  test("modifiers beyond the budget are dropped, never wrapped into another mask's range", () => {
    const mods = Array.from({ length: MAX_MODIFIERS + 3 }, (_, i) => ({ id: `m${i}`, combine: "add", shape: linearShape }));
    const { maskData, modCount } = packMasks([radial("a", { modifiers: mods }), radial("b", { modifiers: [{ id: "z", combine: "add", shape: lumShape }] })], noLayer);
    expect(modCount).toBe(MAX_MODIFIERS);
    expect(maskData[13]).toBe(MAX_MODIFIERS);
    expect([maskData[MASK_STRIDE + 12], maskData[MASK_STRIDE + 13]]).toEqual([MAX_MODIFIERS, 0]);
  });

  test("an ineligible modifier shape is skipped and does not shift the count", () => {
    const { maskData, modCount } = packMasks([radial("a", { modifiers: [{ id: "x", combine: "add", shape: { op: "spot_mask" } }, { id: "y", combine: "add", shape: linearShape }] })], noLayer);
    expect(modCount).toBe(1);
    expect(maskData[13]).toBe(1);
  });

  test("rasterTargets lists brush/spot masks and brush modifiers, keyed by modifier id", () => {
    const masks = [
      radial("r", { modifiers: [{ id: "rb", combine: "subtract", shape: { ...brushShape, dabs: [{ x: 0, y: 0, radius: 1 }] } }, { id: "rl", combine: "add", shape: linearShape }] }),
      { op: "spot_mask", id: "s", dabs: [] },
      { op: "brush_mask", id: "b", dabs: [] },
    ];
    expect(rasterTargets(masks).map((t) => [t.id, t.op])).toEqual([["rb", "brush_mask"], ["s", "spot_mask"], ["b", "brush_mask"]]);
  });

  test("a segment mask packs grow, feather, invert, kind 7 and its layer, with adjustments", () => {
    const seg = { op: "segment_mask", id: "g", logits: "AAAA", feather: 35, grow: -20, invert: true, exposure: 0.5, contrast: 3, saturation: 4 };
    const { maskData, modCount } = packMasks([seg], (id) => (id === "g" ? 5 : undefined));
    expect(MASK_KIND.segment_mask).toBe(7);
    expect(Array.from(maskData.slice(0, 13))).toEqual([-20, 0, 0, 0, 35, 1, 7, 5, 0.5, 3, 4, 0, 0]);
    expect(modCount).toBe(0);
  });

  test("a segment modifier packs like a stand-alone segment and takes its layer by modifier id", () => {
    const shape = { op: "segment_mask", logits: "BBBB", feather: 10, grow: 30, invert: false };
    const { modData } = packMasks([radial("a", { modifiers: [{ id: "sm", combine: "subtract", shape }] })], (id) => (id === "sm" ? 3 : 0));
    expect(Array.from(modData.slice(0, 9))).toEqual([30, 0, 0, 0, 10, 0, 7, 3, COMBINE_CODE.subtract]);
  });

  test("rasterTargets includes segment masks and segment modifiers with their logits", () => {
    const masks = [
      { op: "segment_mask", id: "g", logits: "AAAA", feather: 0, grow: 0, invert: false },
      radial("r", { modifiers: [{ id: "rs", combine: "intersect", shape: { op: "segment_mask", logits: "CCCC", feather: 0, grow: 0, invert: false } }] }),
    ];
    expect(rasterTargets(masks).map((t) => [t.id, t.op, /** @type {any} */ (t).logits])).toEqual([["g", "segment_mask", "AAAA"], ["rs", "segment_mask", "CCCC"]]);
  });

  test("the WGSL segment branch uses the constants the Rust twin uses (masks.rs SEGMENT_*)", () => {
    expect(WGSL).toContain("kind > 6.5");
    expect(WGSL).toContain("v * 8.0 - 4.0 + se.x * 3.0 / 100.0"); // byte -> logit (range 4), grow 3 logits per 100
    expect(WGSL).toContain("0.25 + 3.45 * clamp(pr.x / 100.0, 0.0, 1.0)"); // ramp half-width, feather 0..100
    expect(WGSL).toContain("let weighted = kind < 4.5 || kind > 6.5;");
  });
});
