import { describe, it, expect } from "vitest";
import { flushSync } from "svelte";
import { DevelopStore } from "./develop.svelte.js";
import { DevelopView } from "./developView.svelte.js";
import { createLibraryStore } from "./library.svelte.js";
import { upsertOp, upsertCrop } from "$lib/api/develop.js";

function setup() {
  const develop = new DevelopStore(createLibraryStore());
  return { develop, view: new DevelopView(develop) };
}

describe("DevelopView", () => {
  it("every scalar adjustment reads its op, and falls back to 0 when the op is absent", () => {
    const { develop, view } = setup();
    const scalars = ["exposure", "contrast", "saturation", "temperature", "tint", "highlights", "shadows", "whites", "blacks", "dehaze", "texture", "clarity"];
    for (const name of scalars) expect(/** @type {any} */ (view)[name]).toBe(0);
    scalars.forEach((name, i) => {
      develop.editStack = upsertOp(develop.editStack, name, i + 1);
    });
    scalars.forEach((name, i) => expect(/** @type {any} */ (view)[name]).toBe(i + 1));
  });

  it("structured adjustments fall back to their identity value on an empty stack", () => {
    const { view } = setup();
    for (const name of ["vignette", "lensCorrection", "perspective", "grain", "sharpen", "lumaNR", "colorNR", "crop", "toneCurvePoints", "hslBands", "splitToning"]) {
      expect(/** @type {any} */ (view)[name], name).toBeTruthy();
    }
    expect(view.crop).toMatchObject({ x: 0, y: 0, width: 1, height: 1 });
  });

  it("follows a wholesale replacement of the edit stack (restore, paste, reset)", () => {
    const { develop, view } = setup();
    develop.editStack = upsertOp(develop.editStack, "exposure", 2);
    expect(view.exposure).toBe(2);
    develop.editStack = { schema_version: 1, ops: [] };
    expect(view.exposure).toBe(0);
  });

  it("structured adjustments read their op too (crop)", () => {
    const { develop, view } = setup();
    develop.editStack = upsertCrop(develop.editStack, { x: 0.1, y: 0.2, width: 0.5, height: 0.6, angle: 3 });
    expect(view.crop).toMatchObject({ x: 0.1, y: 0.2, width: 0.5, height: 0.6, angle: 3 });
  });

  it("keeps per-field granularity: an effect on `exposure` does not re-run when only `contrast` changes", () => {
    const { develop, view } = setup();
    /** @type {number[]} */
    const exposureSeen = [];
    /** @type {number[]} */
    const contrastSeen = [];
    const stop = $effect.root(() => {
      $effect(() => {
        exposureSeen.push(view.exposure);
      });
      $effect(() => {
        contrastSeen.push(view.contrast);
      });
    });
    flushSync();
    develop.editStack = upsertOp(develop.editStack, "contrast", 30);
    flushSync();
    develop.editStack = upsertOp(develop.editStack, "exposure", 1);
    flushSync();
    stop();
    expect(exposureSeen).toEqual([0, 1]); // not re-run for the contrast change
    expect(contrastSeen).toEqual([0, 30]); // nor for the exposure change
  });
});
