import { describe, it, expect } from "vitest";
import {
  ZOOM_MIN,
  ZOOM_MAX,
  ZOOM_STEPS,
  FIT,
  fitScale,
  effectiveScale,
  minZoomScale,
  resolveZoom,
  stepZoom,
  wheelFactor,
  reduceZoom,
  formatZoomPercent,
  visibleRegion,
  scrollForFocus,
  focusAtViewportPoint,
  navigatorPointToFocus,
  contentOffset,
} from "./zoomMath.js";

/** @returns {import("./zoomMath.js").ZoomState} */
const zoom = (/** @type {number} */ scale) => ({ mode: "zoom", scale });

describe("range", () => {
  it("is 50%..200%", () => {
    expect(ZOOM_MIN).toBe(0.5);
    expect(ZOOM_MAX).toBe(2);
    expect(ZOOM_STEPS[0]).toBe(ZOOM_MIN);
    expect(ZOOM_STEPS[ZOOM_STEPS.length - 1]).toBe(ZOOM_MAX);
  });
});

describe("fitScale", () => {
  it("fits the limiting axis", () => {
    expect(fitScale(6000, 4000, 1000, 1000)).toBeCloseTo(1000 / 6000);
    expect(fitScale(4000, 6000, 1000, 1000)).toBeCloseTo(1000 / 6000);
  });
  it("never upscales a small photo", () => {
    expect(fitScale(400, 300, 2000, 2000)).toBe(1);
  });
  it("is 1 (not NaN/Infinity) for degenerate input", () => {
    expect(fitScale(0, 0, 100, 100)).toBe(1);
    expect(fitScale(100, 100, 0, 100)).toBe(1);
    expect(fitScale(NaN, 100, 100, 100)).toBe(1);
  });
});

describe("resolveZoom", () => {
  it("clamps into 50%..200%", () => {
    expect(resolveZoom(10, 0.2)).toEqual(zoom(2));
    expect(resolveZoom(0.1, 0.2)).toEqual(zoom(0.5));
  });
  it("collapses to Fit when the request is at or below Fit", () => {
    expect(resolveZoom(0.75, 0.8)).toBe(FIT);
    expect(resolveZoom(0.8, 0.8)).toBe(FIT);
    expect(resolveZoom(1, 1)).toBe(FIT);
  });
  it("keeps 50% even when Fit is far smaller", () => {
    expect(resolveZoom(0.5, 0.17)).toEqual(zoom(0.5));
  });
  it("treats non-finite input as Fit", () => {
    expect(resolveZoom(NaN, 0.5)).toBe(FIT);
  });
});

describe("minZoomScale / effectiveScale", () => {
  it("floors at 50% or Fit, whichever is larger", () => {
    expect(minZoomScale(0.17)).toBe(0.5);
    expect(minZoomScale(0.8)).toBe(0.8);
  });
  it("reads Fit from context, zoom from state", () => {
    expect(effectiveScale(FIT, 0.3)).toBe(0.3);
    expect(effectiveScale(zoom(1.5), 0.3)).toBe(1.5);
  });
});

describe("stepZoom", () => {
  it("walks up the ladder from Fit", () => {
    expect(stepZoom(0.17, 0.17, 1)).toEqual(zoom(0.5));
    expect(stepZoom(0.8, 0.8, 1)).toEqual(zoom(1));
  });
  it("walks the whole ladder and stops at 200%", () => {
    let s = /** @type {any} */ (zoom(0.5));
    const seen = [];
    for (let i = 0; i < 6; i++) {
      s = stepZoom(s.scale, 0.17, 1);
      seen.push(s.scale);
    }
    expect(seen).toEqual([0.75, 1, 1.5, 2, 2, 2]);
  });
  it("steps down from an off-ladder scale to the step below", () => {
    expect(stepZoom(1.2, 0.17, -1)).toEqual(zoom(1));
    expect(stepZoom(1.2, 0.17, 1)).toEqual(zoom(1.5));
  });
  it("goes to Fit when stepping below the lowest allowed scale", () => {
    expect(stepZoom(0.5, 0.17, -1)).toBe(FIT);
    expect(stepZoom(1, 0.8, -1)).toBe(FIT); // 0.75 < fit 0.8
    expect(stepZoom(1, 1, -1)).toBe(FIT);
  });
  it("lands on a step equal to Fit as Fit", () => {
    expect(stepZoom(1.5, 1, -1)).toBe(FIT);
  });
});

describe("wheelFactor", () => {
  it("zooms in on negative delta and out on positive", () => {
    expect(wheelFactor(-5)).toBeGreaterThan(1);
    expect(wheelFactor(5)).toBeLessThan(1);
    expect(wheelFactor(0)).toBe(1);
  });
  it("caps a big mouse-wheel notch", () => {
    expect(wheelFactor(-100)).toBe(wheelFactor(-30));
    expect(wheelFactor(-30)).toBeLessThan(1.4);
  });
});

describe("reduceZoom", () => {
  it("fit / actual / toggle", () => {
    expect(reduceZoom(zoom(1.5), { type: "fit" }, 0.3)).toBe(FIT);
    expect(reduceZoom(FIT, { type: "actual" }, 0.3)).toEqual(zoom(1));
    expect(reduceZoom(FIT, { type: "toggle" }, 0.3)).toEqual(zoom(1));
    expect(reduceZoom(zoom(1), { type: "toggle" }, 0.3)).toBe(FIT);
    expect(reduceZoom(zoom(2), { type: "toggle" }, 0.3)).toBe(FIT);
  });
  it("toggle on a photo that already fits at 100% stays Fit", () => {
    expect(reduceZoom(FIT, { type: "toggle" }, 1)).toBe(FIT);
  });
  it("set clamps", () => {
    expect(reduceZoom(FIT, { type: "set", scale: 9 }, 0.3)).toEqual(zoom(2));
    expect(reduceZoom(FIT, { type: "set", scale: 0.01 }, 0.3)).toEqual(zoom(0.5));
  });
  it("step from Fit uses Fit as the current scale", () => {
    expect(reduceZoom(FIT, { type: "step", dir: 1 }, 0.3)).toEqual(zoom(0.5));
    expect(reduceZoom(zoom(0.5), { type: "step", dir: -1 }, 0.3)).toBe(FIT);
  });
  it("wheel scales multiplicatively and clamps at 200%", () => {
    const out = /** @type {any} */ (reduceZoom(zoom(1), { type: "wheel", deltaY: -10 }, 0.3));
    expect(out.scale).toBeCloseTo(Math.exp(0.1));
    expect(reduceZoom(zoom(2), { type: "wheel", deltaY: -30 }, 0.3)).toEqual(zoom(2));
  });
  it("wheel from Fit zooms in from the Fit scale, clamped up to 50%", () => {
    expect(reduceZoom(FIT, { type: "wheel", deltaY: -10 }, 0.3)).toEqual(zoom(0.5));
  });
  it("wheel out clamps at 50% rather than snapping to Fit", () => {
    expect(reduceZoom(zoom(0.5), { type: "wheel", deltaY: 30 }, 0.17)).toEqual(zoom(0.5));
  });
  it("wheel out on a small photo returns to Fit at the Fit scale", () => {
    expect(reduceZoom(zoom(1.5), { type: "wheel", deltaY: 30 }, 0.9)).toEqual(zoom(1.5 * wheelFactor(30)));
    expect(reduceZoom(zoom(1.0), { type: "wheel", deltaY: 30 }, 0.9)).toBe(FIT); // 0.74 < fit 0.9
    expect(reduceZoom(zoom(0.95), { type: "wheel", deltaY: 30 }, 0.9)).toBe(FIT);
  });
  it("pinch multiplies the gesture-start scale and clamps", () => {
    expect(reduceZoom(FIT, { type: "pinch", base: 1, gesture: 1.5 }, 0.3)).toEqual(zoom(1.5));
    expect(reduceZoom(FIT, { type: "pinch", base: 1, gesture: 5 }, 0.3)).toEqual(zoom(2));
    expect(reduceZoom(FIT, { type: "pinch", base: 1, gesture: 0.1 }, 0.3)).toEqual(zoom(0.5));
  });
  it("never produces a scale outside the range", () => {
    for (const fit of [0.05, 0.3, 0.5, 0.9, 1]) {
      for (const deltaY of [-1000, -30, -1, 1, 30, 1000]) {
        for (const start of [FIT, zoom(0.5), zoom(1), zoom(2)]) {
          const s = reduceZoom(start, { type: "wheel", deltaY }, fit);
          if (s.mode === "zoom") {
            expect(s.scale).toBeGreaterThanOrEqual(ZOOM_MIN);
            expect(s.scale).toBeLessThanOrEqual(ZOOM_MAX);
            expect(s.scale).toBeGreaterThan(fit);
          }
        }
      }
    }
  });
});

describe("formatZoomPercent", () => {
  it("rounds to whole percent", () => {
    expect(formatZoomPercent(1)).toBe("100%");
    expect(formatZoomPercent(0.5)).toBe("50%");
    expect(formatZoomPercent(1 / 6)).toBe("17%");
    expect(formatZoomPercent(2)).toBe("200%");
  });
});

describe("contentOffset", () => {
  it("centres content smaller than the padded viewport", () => {
    expect(contentOffset({ w: 1000, h: 800 }, { w: 400, h: 300 }, 22)).toEqual({ x: 300, y: 250 });
  });
  it("starts at the padding once the content overflows", () => {
    expect(contentOffset({ w: 1000, h: 800 }, { w: 4000, h: 3000 }, 22)).toEqual({ x: 22, y: 22 });
  });
  it("handles one overflowing axis and one centred axis", () => {
    expect(contentOffset({ w: 1000, h: 800 }, { w: 4000, h: 300 }, 22)).toEqual({ x: 22, y: 250 });
  });
  it("switches exactly at the padded edge", () => {
    expect(contentOffset({ w: 1000, h: 800 }, { w: 956, h: 756 }, 22)).toEqual({ x: 22, y: 22 });
  });
});

describe("visibleRegion", () => {
  const offset = { x: 22, y: 22 };
  it("is the whole image when it fits", () => {
    const r = visibleRegion({ scrollLeft: 0, scrollTop: 0 }, { w: 1000, h: 800 }, { x: 100, y: 50 }, { w: 800, h: 600 });
    expect(r).toEqual({ x: 0, y: 0, w: 1, h: 1 });
  });
  it("is the top-left corner at scroll 0", () => {
    // 4000x3000 content, 1000x800 viewport
    const r = visibleRegion({ scrollLeft: 0, scrollTop: 0 }, { w: 1000, h: 800 }, offset, { w: 4000, h: 3000 });
    expect(r.x).toBe(0);
    expect(r.y).toBe(0);
    expect(r.w).toBeCloseTo((1000 - 22) / 4000);
    expect(r.h).toBeCloseTo((800 - 22) / 3000);
  });
  it("tracks scroll", () => {
    const r = visibleRegion({ scrollLeft: 2022, scrollTop: 1022 }, { w: 1000, h: 800 }, offset, { w: 4000, h: 3000 });
    expect(r.x).toBeCloseTo(0.5);
    expect(r.y).toBeCloseTo(1000 / 3000);
    expect(r.w).toBeCloseTo(0.25);
    expect(r.h).toBeCloseTo(800 / 3000);
  });
  it("clips at the far edge", () => {
    const r = visibleRegion({ scrollLeft: 3500, scrollTop: 0 }, { w: 1000, h: 800 }, offset, { w: 4000, h: 3000 });
    expect(r.x + r.w).toBeCloseTo(1);
  });
  it("tolerates empty content", () => {
    expect(visibleRegion({ scrollLeft: 0, scrollTop: 0 }, { w: 10, h: 10 }, offset, { w: 0, h: 0 })).toEqual({ x: 0, y: 0, w: 1, h: 1 });
  });
});

describe("scrollForFocus / focusAtViewportPoint", () => {
  const offset = { x: 22, y: 22 };
  const content = { w: 4000, h: 3000 };
  it("centres a focus point", () => {
    const s = scrollForFocus({ x: 0.5, y: 0.5 }, { x: 500, y: 400 }, offset, content);
    expect(s).toEqual({ scrollLeft: 22 + 2000 - 500, scrollTop: 22 + 1500 - 400 });
  });
  it("round-trips with focusAtViewportPoint", () => {
    const focus = { x: 0.31, y: 0.77 };
    const at = { x: 321, y: 123 };
    const s = scrollForFocus(focus, at, offset, content);
    const back = focusAtViewportPoint(at, s, offset, content);
    expect(back.x).toBeCloseTo(focus.x);
    expect(back.y).toBeCloseTo(focus.y);
  });
  it("keeps the point under the cursor when the scale changes", () => {
    // zoom about the cursor: focus under cursor at old scale, re-anchored at new scale
    const at = { x: 400, y: 300 };
    const oldScroll = { scrollLeft: 700, scrollTop: 300 };
    const oldContent = { w: 2000, h: 1500 };
    const focus = focusAtViewportPoint(at, oldScroll, offset, oldContent);
    const newContent = { w: 4000, h: 3000 };
    const s = scrollForFocus(focus, at, offset, newContent);
    const after = focusAtViewportPoint(at, s, offset, newContent);
    expect(after.x).toBeCloseTo(focus.x);
    expect(after.y).toBeCloseTo(focus.y);
  });
});

describe("navigatorPointToFocus", () => {
  it("maps and clamps", () => {
    expect(navigatorPointToFocus(50, 25, { w: 100, h: 50 })).toEqual({ x: 0.5, y: 0.5 });
    expect(navigatorPointToFocus(-10, 999, { w: 100, h: 50 })).toEqual({ x: 0, y: 1 });
    expect(navigatorPointToFocus(5, 5, { w: 0, h: 0 })).toEqual({ x: 0.5, y: 0.5 });
  });
});
