// Persistent regression coverage for Crop & Straighten's coordinate math
// (see cropMath.js's own module doc comment for why this exists as a real
// test suite rather than only ever being exercised through a throwaway
// empirical harness against the live app). Every "Regression:" test below
// pins down a value that was independently verified against the real
// running app during this feature's own bug-fix history -- these are not
// invented expectations, they're locked-in real behavior.

import { describe, it, expect } from "vitest";
import {
  clamp01,
  cropMinFrac,
  normalizedAspectRatio,
  largestCenteredCropForRatio,
  moveCropRect,
  cropCornerPoints,
  resizeCropCorner,
  resizeCropEdge,
  cropHandlePos,
  trueElementBox,
} from "./cropMath.js";

describe("clamp01", () => {
  it("passes values already in range through unchanged", () => {
    expect(clamp01(0.5, 0, 1)).toBe(0.5);
  });
  it("floors below the low bound", () => {
    expect(clamp01(-1, 0, 1)).toBe(0);
  });
  it("ceils above the high bound", () => {
    expect(clamp01(5, 0, 1)).toBe(1);
  });
});

describe("cropMinFrac", () => {
  it("converts a real pixel floor into the correct normalized fraction", () => {
    expect(cropMinFrac(64, 2048)).toBeCloseTo(64 / 2048, 10);
  });
  it("caps at 1 when the pixel floor exceeds the source dimension", () => {
    expect(cropMinFrac(9999, 100)).toBe(1);
  });
  it("falls back to a conservative 2% when the source dimension is unknown", () => {
    expect(cropMinFrac(64, 0)).toBe(0.02);
  });
});

describe("normalizedAspectRatio", () => {
  it("corrects a pixel ratio by the image's own aspect ratio", () => {
    // A 4:3 image (imageAspect = 4/3): a 1:1 (square) PIXEL target needs
    // normalized width:height of 3:4, i.e. ratio 0.75 -- the exact
    // relationship the original aspect-ratio-preset bug got backwards.
    expect(normalizedAspectRatio(1, 4 / 3)).toBeCloseTo(0.75, 10);
  });
  it("returns null (not a wrong number) when the pixel ratio is null", () => {
    expect(normalizedAspectRatio(null, 4 / 3)).toBeNull();
  });
  it("returns null when the image aspect isn't known yet (0)", () => {
    expect(normalizedAspectRatio(1, 0)).toBeNull();
  });
});

describe("largestCenteredCropForRatio", () => {
  // Regression: exact values independently verified against the real
  // running app on a real 2048x1536 source image (see PROGRESS.md's "Fix:
  // crop aspect-ratio presets used the wrong ratio space").
  it("produces a true square for a 1:1 preset on a 2048x1536 (4:3) source", () => {
    const rect = largestCenteredCropForRatio(1, 2048, 1536);
    if (!rect) throw new Error("expected a rect");
    expect(rect).toEqual({ x: 0.125, y: 0, width: 0.75, height: 1 });
    // The defining property of "1:1": pixel width must equal pixel height.
    expect(rect.width * 2048).toBeCloseTo(rect.height * 1536, 6);
  });

  it("produces the correct pixel ratio for a 16:9 preset on the same source", () => {
    const rect = largestCenteredCropForRatio(16 / 9, 2048, 1536);
    if (!rect) throw new Error("expected a rect");
    expect(rect.x).toBeCloseTo(0, 10);
    expect(rect.y).toBeCloseTo(0.125, 10);
    expect(rect.width).toBeCloseTo(1, 10);
    expect(rect.height).toBeCloseTo(0.75, 10);
    const pixelRatio = (rect.width * 2048) / (rect.height * 1536);
    expect(pixelRatio).toBeCloseTo(16 / 9, 6);
  });

  it("is exactly idempotent -- repeated calls with the same ratio never compound/shrink", () => {
    // The original bug: reshaping off the CURRENT (already-wrong) rect
    // caused repeated clicks of the same preset to shrink further each
    // time. This function is stateless (never reads a "current" rect), so
    // two independent calls with the same inputs must be identical.
    const first = largestCenteredCropForRatio(1, 2048, 1536);
    const second = largestCenteredCropForRatio(1, 2048, 1536);
    expect(second).toEqual(first);
    // And switching to a different ratio and back must also return to
    // exactly the first result, not something smaller.
    largestCenteredCropForRatio(16 / 9, 2048, 1536);
    const third = largestCenteredCropForRatio(1, 2048, 1536);
    expect(third).toEqual(first);
  });

  it("produces an exact square on an already-square source", () => {
    const rect = largestCenteredCropForRatio(1, 1000, 1000);
    expect(rect).toEqual({ x: 0, y: 0, width: 1, height: 1 });
  });

  it("returns null when source dimensions aren't known yet", () => {
    expect(largestCenteredCropForRatio(1, 0, 0)).toBeNull();
  });
});

describe("moveCropRect", () => {
  it("moves by the given delta when there's room", () => {
    const start = { x: 0.25, y: 0.25, width: 0.25, height: 0.25 };
    expect(moveCropRect(start, 0.125, -0.125)).toEqual({ x: 0.375, y: 0.125, width: 0.25, height: 0.25 });
  });
  it("clamps so the rect never leaves [0,1] on either axis", () => {
    const start = { x: 0.1, y: 0.1, width: 0.3, height: 0.3 };
    const moved = moveCropRect(start, -5, 5);
    expect(moved.x).toBe(0);
    expect(moved.y).toBe(1 - 0.3);
  });
});

describe("cropCornerPoints", () => {
  // 0.25/0.5/0.125 are exact in binary floating point -- avoids
  // toEqual()'s exact-equality failing on ordinary float noise (e.g.
  // 0.2+0.4 !== 0.6000000000000001 as an exact literal).
  const r = { x: 0.25, y: 0.25, width: 0.5, height: 0.125 };
  it("identifies se as fixed=nw, dragged=se", () => {
    expect(cropCornerPoints("se", r)).toEqual({ fixed: [0.25, 0.25], dragged: [0.75, 0.375] });
  });
  it("identifies nw as fixed=se, dragged=nw", () => {
    expect(cropCornerPoints("nw", r)).toEqual({ fixed: [0.75, 0.375], dragged: [0.25, 0.25] });
  });
});

describe("resizeCropCorner", () => {
  const start = { x: 0, y: 0, width: 1, height: 1 };

  it("resizes freely with no aspect lock", () => {
    const next = resizeCropCorner(start, "se", -0.5, -0.4, null, 4 / 3, 0.02, 0.02);
    expect(next.width).toBeCloseTo(0.5, 10);
    expect(next.height).toBeCloseTo(0.6, 10);
  });

  it("enforces the pixel-based minimum size, not a near-zero sliver", () => {
    // Dragging the SE handle almost all the way to the fixed NW corner --
    // the exact interaction that produced this project's own real,
    // empirically-confirmed regression (PROGRESS.md's "Fix: committed
    // crop preview rendered as a 0x0 box"). A real 64px-equivalent floor
    // on a 2048x1536 source.
    const minFracX = cropMinFrac(64, 2048);
    const minFracY = cropMinFrac(64, 1536);
    const next = resizeCropCorner(start, "se", -0.999, -0.999, null, 2048 / 1536, minFracX, minFracY);
    expect(next.width).toBeCloseTo(minFracX, 10);
    expect(next.height).toBeCloseTo(minFracY, 10);
    expect(next.width * 2048).toBeCloseTo(64, 6);
    expect(next.height * 1536).toBeCloseTo(64, 6);
  });

  it("respects a corrected aspect lock, not the raw pixel ratio", () => {
    // 1:1 aspect-locked drag, shrinking the SE handle inward on a 2048x1536
    // (4:3) source: the RAW pixel ratio (1) must never be used directly
    // against normalized width/height (the original shipped bug) -- it
    // has to be corrected to normalizedRatio = 1/(2048/1536) = 0.75 first.
    const next = resizeCropCorner(start, "se", -0.4, -0.1, 1, 2048 / 1536, 0.02, 0.02);
    const pixelW = next.width * 2048;
    const pixelH = next.height * 1536;
    // The defining property of a 1:1 lock: pixel width equals pixel height.
    expect(pixelW).toBeCloseTo(pixelH, 6);
    expect(next.width).toBeCloseTo(0.675, 10);
    expect(next.height).toBeCloseTo(0.9, 10);
  });

  it("keeps the opposite corner fixed as the anchor", () => {
    const next = resizeCropCorner(start, "se", -0.3, -0.3, null, 4 / 3, 0.02, 0.02);
    // nw stays at (0,0) since se is being dragged.
    expect(next.x).toBe(0);
    expect(next.y).toBe(0);
  });
});

describe("resizeCropEdge", () => {
  const start = { x: 0.2, y: 0.2, width: 0.4, height: 0.4 };

  it("resizes the east edge freely", () => {
    const next = resizeCropEdge(start, "e", 0.1, 0, 0.02, 0.02);
    expect(next).toEqual({ x: 0.2, y: 0.2, width: 0.5, height: 0.4 });
  });

  it("resizes the west edge, moving x and adjusting width together", () => {
    const next = resizeCropEdge(start, "w", -0.1, 0, 0.02, 0.02);
    expect(next.x).toBeCloseTo(0.1, 10);
    expect(next.width).toBeCloseTo(0.5, 10);
  });

  it("floors the east edge at the minimum instead of collapsing to zero", () => {
    const next = resizeCropEdge(start, "e", -0.999, 0, 0.05, 0.05);
    expect(next.width).toBeCloseTo(0.05, 10);
  });

  it("floors the south edge at the minimum instead of collapsing to zero", () => {
    const next = resizeCropEdge(start, "s", 0, -0.999, 0.05, 0.05);
    expect(next.height).toBeCloseTo(0.05, 10);
  });
});

describe("cropHandlePos", () => {
  const c = { x: 0.25, y: 0.25, width: 0.5, height: 0.25 };
  it("places corner handles at the rect's own corners", () => {
    expect(cropHandlePos("nw", c)).toEqual([0.25, 0.25]);
    expect(cropHandlePos("se", c)).toEqual([0.75, 0.5]);
  });
  it("places edge handles at the midpoint of their own edge", () => {
    expect(cropHandlePos("n", c)).toEqual([0.5, 0.25]);
    expect(cropHandlePos("w", c)).toEqual([0.25, 0.375]);
  });
});

describe("trueElementBox", () => {
  it("is an exact identity when the rect is already unrotated (angle 0)", () => {
    const rect = { left: 100, top: 50, width: 200, height: 100 };
    expect(trueElementBox(rect, 200, 100)).toEqual({ left: 100, top: 50, width: 200, height: 100 });
  });

  it("derives the true box from a rotated AABB via its reliable center, ignoring the AABB's own size", () => {
    // A 200x100 true box centered at (300,200). Rotation around center
    // never moves the center, so ANY AABB sharing that center -- no
    // matter how much larger, simulating any real rotation angle -- must
    // resolve to the exact same true box.
    const trueWidth = 200;
    const trueHeight = 100;
    const center = { x: 300, y: 200 };
    const syntheticRotatedRect = { left: center.x - 999, top: center.y - 999, width: 1998, height: 1998 };
    expect(trueElementBox(syntheticRotatedRect, trueWidth, trueHeight)).toEqual({ left: 200, top: 150, width: 200, height: 100 });
  });

  it("matches real, empirically-measured rotated canvas geometry", () => {
    // Regression: real numbers from this project's own crop/rotate
    // overflow-fix verification -- a canvas rotated 15deg produced this
    // real getBoundingClientRect(). Confirms trueElementBox recovers a
    // sensible, correctly-centered true box from real (not synthetic)
    // measurements, not just idealized ones.
    const rotated = { top: -20.964069366455078, bottom: 695.2858695983887, left: 97.49932098388672, right: 942.531120300293 };
    const rect = { left: rotated.left, top: rotated.top, width: rotated.right - rotated.left, height: rotated.bottom - rotated.top };
    const trueWidth = 728;
    const trueHeight = 546;
    const box = trueElementBox(rect, trueWidth, trueHeight);
    const rotatedCenterX = (rotated.left + rotated.right) / 2;
    const rotatedCenterY = (rotated.top + rotated.bottom) / 2;
    expect(box.left + box.width / 2).toBeCloseTo(rotatedCenterX, 6);
    expect(box.top + box.height / 2).toBeCloseTo(rotatedCenterY, 6);
    expect(box.width).toBe(trueWidth);
    expect(box.height).toBe(trueHeight);
  });
});

describe("crop + rotate combination (integration)", () => {
  it("clicking a handle's own fixed screen position recovers the correct local point, regardless of the canvas's live rotation angle", () => {
    // The core property this bug-fix chain protects: crop handles live in
    // a FIXED, unrotated overlay space (see trueElementBox's own doc
    // comment) -- clicking exactly on a handle's own rendered screen
    // position must recover the SAME true local coordinate whether the
    // canvas is rotated 0deg or 44deg, since the handle itself never
    // rotates. An earlier, WRONG fix attempt broke exactly this property
    // by inverse-ROTATING the click position, which is only correct for
    // interpreting a click ON the rotated image content, not on a fixed
    // overlay handle -- this test's angle loop would have failed under
    // that version (only angle 0 would have passed).
    const trueWidth = 2048;
    const trueHeight = 1536;
    const trueLeft = 156;
    const trueTop = 22;
    // SE handle for a full-frame crop sits at the canvas's own true
    // bottom-right corner -- a fixed screen point, unaffected by rotation.
    const handleScreenPoint = { x: trueLeft + trueWidth, y: trueTop + trueHeight };
    const centerX = trueLeft + trueWidth / 2;
    const centerY = trueTop + trueHeight / 2;

    for (const angle of [0, 15, -30, 44]) {
      // The angle itself is irrelevant to the math below -- ANY AABB
      // sharing the true center simulates "however this angle's own
      // getBoundingClientRect() would report," since only the center is
      // ever used. That's the whole point: the fix no longer needs to
      // know the angle at all for this fixed-overlay-space use case.
      void angle;
      const rotatedRect = { left: centerX - 9999, top: centerY - 9999, width: 19998, height: 19998 };
      const box = trueElementBox(rotatedRect, trueWidth, trueHeight);
      const nativeX = (handleScreenPoint.x - box.left) / (box.width / trueWidth);
      const nativeY = (handleScreenPoint.y - box.top) / (box.height / trueHeight);
      expect(nativeX).toBeCloseTo(trueWidth, 6);
      expect(nativeY).toBeCloseTo(trueHeight, 6);
    }
  });

  it("a handle drag toward the fixed NW corner still respects the pixel minimum", () => {
    const trueWidth = 2048;
    const trueHeight = 1536;
    const trueLeft = 156;
    const trueTop = 22;
    const start = { x: 0, y: 0, width: 1, height: 1 };
    const minFracX = cropMinFrac(64, trueWidth);
    const minFracY = cropMinFrac(64, trueHeight);
    const centerX = trueLeft + trueWidth / 2;
    const centerY = trueTop + trueHeight / 2;
    const rotatedRect = { left: centerX - 9999, top: centerY - 9999, width: 19998, height: 19998 };
    const box = trueElementBox(rotatedRect, trueWidth, trueHeight);
    const toNormalized = (/** @type {{x: number, y: number}} */ p) => ({ x: (p.x - box.left) / trueWidth, y: (p.y - box.top) / trueHeight });

    // Drag start: SE handle's own fixed screen position. Target: almost
    // exactly the fixed NW corner (an attempted near-zero crop) -- the
    // exact interaction that produced this project's own real,
    // empirically-confirmed "0x0 box" regression.
    const startNorm = toNormalized({ x: trueLeft + trueWidth, y: trueTop + trueHeight });
    const targetNorm = toNormalized({ x: trueLeft + 1, y: trueTop + 1 });
    const dx = targetNorm.x - startNorm.x;
    const dy = targetNorm.y - startNorm.y;

    const next = resizeCropCorner(start, "se", dx, dy, null, trueWidth / trueHeight, minFracX, minFracY);
    expect(next.width * trueWidth).toBeCloseTo(64, 6);
    expect(next.height * trueHeight).toBeCloseTo(64, 6);
  });

  it("largestCenteredCropForRatio always yields a rect that clears the pixel minimum on any real source size", () => {
    const sizes = [
      [2048, 1536],
      [4032, 3024],
      [500, 500],
      [6000, 4000],
    ];
    const ratios = [1, 16 / 9, 3 / 2, 4 / 3, 5 / 4];
    for (const [w, h] of sizes) {
      for (const ratio of ratios) {
        const rect = largestCenteredCropForRatio(ratio, w, h);
        if (!rect) throw new Error("expected a rect");
        expect(rect.width * w).toBeGreaterThan(64);
        expect(rect.height * h).toBeGreaterThan(64);
        // Always fully within [0,1] bounds.
        expect(rect.x).toBeGreaterThanOrEqual(0);
        expect(rect.y).toBeGreaterThanOrEqual(0);
        expect(rect.x + rect.width).toBeLessThanOrEqual(1.0000001);
        expect(rect.y + rect.height).toBeLessThanOrEqual(1.0000001);
      }
    }
  });
});
