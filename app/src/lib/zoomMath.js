// Pure, DOM-free zoom model for Develop's canvas (and anything else that
// wants the same rules). Every zoom input -- the +/- buttons, the Fit/100%
// click toggle, ctrl+wheel, trackpad pinch, a typed preset -- is turned into
// ONE action and run through `reduceZoom`, so the clamping rules live in a
// single place and have unit tests (zoomMath.test.js) instead of being
// re-derived inside each event handler.
//
// Vocabulary
//   scale   CSS pixels per SOURCE-NATIVE pixel (1 = "100%"). Always relative
//           to the photo's native resolution, never to whichever preview tier
//           happens to be uploaded, so "100%" means the same thing before and
//           after the 1:1 tier finishes loading.
//   fit     the scale at which the whole image just fits the viewport. It is
//           a MODE, not a number inside the zoom range: a 24 MP photo in a
//           1000 px viewport fits at ~17 %, well below ZOOM_MIN.
//   region  the visible part of the image as a normalized (0..1) rect -- what
//           the navigator draws.

/** Smallest explicit zoom ("small to 50%"). Fit may be smaller still. */
export const ZOOM_MIN = 0.5;
/** Largest zoom ("zoom in max to 200%"). */
export const ZOOM_MAX = 2;
/** Ladder the +/- buttons walk. Fit sits below it (or on it, see stepZoom). */
export const ZOOM_STEPS = Object.freeze([0.5, 0.75, 1, 1.5, 2]);

/** Per-pixel-of-wheel-delta exponential sensitivity, and the largest |delta|
 * one event may contribute -- a mouse-wheel notch reports ~100 px, a
 * trackpad pinch a few px, and an uncapped notch would jump 2.7x. */
export const WHEEL_ZOOM_SENSITIVITY = 0.01;
export const WHEEL_ZOOM_MAX_DELTA = 30;

/** Two scales closer than this are the same scale (float noise from fit). */
const EPS = 1e-3;

/** @typedef {{ mode: "fit" } | { mode: "zoom", scale: number }} ZoomState */
/** @typedef {{ x: number, y: number, w: number, h: number }} Region */

/** @type {ZoomState} */
export const FIT = Object.freeze({ mode: "fit" });

/** @param {number} scale @returns {ZoomState} */
function zoomAt(scale) {
  return { mode: "zoom", scale };
}

/** @param {number} v @param {number} lo @param {number} hi */
function clamp(v, lo, hi) {
  return Math.min(hi, Math.max(lo, v));
}

/** Scale that fits `natW x natH` inside `availW x availH`. Never above 1: a
 * small photo is shown at its own size rather than blown up (matches the
 * canvas's `max-width/max-height: 100%` behaviour). Returns 1 for degenerate
 * input so callers never see NaN/Infinity. */
export function fitScale(
  /** @type {number} */ natW,
  /** @type {number} */ natH,
  /** @type {number} */ availW,
  /** @type {number} */ availH,
) {
  if (!(natW > 0) || !(natH > 0) || !(availW > 0) || !(availH > 0)) return 1;
  return Math.min(availW / natW, availH / natH, 1);
}

/** The scale a state is actually drawn at. */
export function effectiveScale(/** @type {ZoomState} */ state, /** @type {number} */ fit) {
  return state.mode === "fit" ? fit : state.scale;
}

/** Lowest explicit scale allowed for this photo: 50 %, or Fit when Fit is
 * larger (a small photo can't be zoomed out past the point it fits). */
export function minZoomScale(/** @type {number} */ fit) {
  return Math.max(ZOOM_MIN, fit);
}

/** Clamp a requested scale into the zoom range and return the matching
 * state. A request at or below Fit (when Fit is inside the range) collapses
 * to Fit so the two never disagree about what "fully zoomed out" is. */
export function resolveZoom(/** @type {number} */ scale, /** @type {number} */ fit) {
  if (!Number.isFinite(scale)) return FIT;
  const s = clamp(scale, ZOOM_MIN, ZOOM_MAX);
  return s <= fit + EPS ? FIT : zoomAt(s);
}

/** Next ladder step above/below `current`. Stepping down past the lowest
 * allowed scale lands on Fit -- the discrete "zoom out all the way" gesture. */
export function stepZoom(
  /** @type {number} */ current,
  /** @type {number} */ fit,
  /** @type {1 | -1} */ dir,
) {
  if (dir > 0) {
    const next = ZOOM_STEPS.find((s) => s > current + EPS);
    return next === undefined ? zoomAt(ZOOM_MAX) : resolveZoom(next, fit);
  }
  const below = [...ZOOM_STEPS].reverse().find((s) => s < current - EPS);
  if (below === undefined || below < fit - EPS) return FIT;
  return resolveZoom(below, fit);
}

/** Scale factor one wheel event applies. Positive deltaY (scroll down /
 * pinch in) shrinks. */
export function wheelFactor(/** @type {number} */ deltaY) {
  return Math.exp(-clamp(deltaY, -WHEEL_ZOOM_MAX_DELTA, WHEEL_ZOOM_MAX_DELTA) * WHEEL_ZOOM_SENSITIVITY);
}

/**
 * @typedef {(
 *   | { type: "fit" }
 *   | { type: "actual" }
 *   | { type: "toggle" }
 *   | { type: "set", scale: number }
 *   | { type: "step", dir: 1 | -1 }
 *   | { type: "wheel", deltaY: number }
 *   | { type: "pinch", base: number, gesture: number }
 * )} ZoomAction
 */

/** The one mechanism every zoom input goes through.
 *
 *  - `fit`     show the whole image
 *  - `actual`  100 %
 *  - `toggle`  Fit <-> 100 % (the click-the-photo gesture)
 *  - `set`     an explicit scale (clamped to the range)
 *  - `step`    next/previous ladder step
 *  - `wheel`   continuous, multiplicative, from the current scale
 *  - `pinch`   `base` (scale when the gesture began) x `gesture` (WebKit's
 *              cumulative gesture scale)
 *
 * Continuous inputs (`wheel`, `pinch`) clamp at the lowest allowed scale
 * instead of snapping to Fit, so one more flick doesn't unexpectedly jump a
 * 50 % view to a 17 % one; the discrete `step -1` is what goes on to Fit.
 * @returns {ZoomState} */
export function reduceZoom(
  /** @type {ZoomState} */ state,
  /** @type {ZoomAction} */ action,
  /** @type {number} */ fit,
) {
  const current = effectiveScale(state, fit);
  switch (action.type) {
    case "fit":
      return FIT;
    case "actual":
      return resolveZoom(1, fit);
    case "toggle":
      return state.mode === "fit" ? resolveZoom(1, fit) : FIT;
    case "set":
      return resolveZoom(action.scale, fit);
    case "step":
      return stepZoom(current, fit, action.dir);
    case "wheel":
      return continuous(current * wheelFactor(action.deltaY), fit);
    case "pinch":
      return continuous(action.base * action.gesture, fit);
    default:
      return state;
  }
}

/** @param {number} scale @param {number} fit @returns {ZoomState} */
function continuous(scale, fit) {
  if (!Number.isFinite(scale)) return FIT;
  const s = clamp(scale, minZoomScale(fit), ZOOM_MAX);
  return s <= fit + EPS ? FIT : zoomAt(s);
}

/** "100%" / "50%" / "17%" -- what the readout shows. */
export function formatZoomPercent(/** @type {number} */ scale) {
  return `${Math.round(scale * 100)}%`;
}

// ---- Geometry: scroll position <-> visible region ------------------------
//
// The canvas scrolls natively (`.canvas-wrap { overflow: auto }`), so "where
// am I looking" is just scrollLeft/scrollTop. `offset` is where the content
// box starts INSIDE the scrollable area (the wrap's padding, or more when the
// content is smaller than the viewport and margin:auto centers it), measured
// from the DOM rather than assumed, because engines differ on whether end
// padding is part of the scrollable overflow.

/** Where the content box starts inside the scrollable area, per axis. The
 * canvas is a flex item with `margin: auto` inside a padded wrap: when it is
 * smaller than the wrap's content area the free space is split evenly
 * (centred), otherwise the margins collapse to 0 and it starts at the
 * padding. Computed rather than measured so it is reactive and testable. */
export function contentOffset(
  /** @type {{ w: number, h: number }} */ view,
  /** @type {{ w: number, h: number }} */ content,
  /** @type {number} */ padding,
) {
  const axis = (/** @type {number} */ v, /** @type {number} */ c) => (c <= v - 2 * padding ? (v - c) / 2 : padding);
  return { x: axis(view.w, content.w), y: axis(view.h, content.h) };
}

/** The visible part of the content as a normalized rect (the navigator's
 * reference rectangle). Clipped to the content, so a fully visible image is
 * {0, 0, 1, 1}. */
export function visibleRegion(
  /** @type {{ scrollLeft: number, scrollTop: number }} */ scroll,
  /** @type {{ w: number, h: number }} */ view,
  /** @type {{ x: number, y: number }} */ offset,
  /** @type {{ w: number, h: number }} */ content,
) {
  if (!(content.w > 0) || !(content.h > 0)) return { x: 0, y: 0, w: 1, h: 1 };
  const left = (scroll.scrollLeft - offset.x) / content.w;
  const top = (scroll.scrollTop - offset.y) / content.h;
  const right = left + view.w / content.w;
  const bottom = top + view.h / content.h;
  const x = clamp(left, 0, 1);
  const y = clamp(top, 0, 1);
  return { x, y, w: clamp(right, 0, 1) - x, h: clamp(bottom, 0, 1) - y };
}

/** Scroll position that puts the normalized image point `focus` at the
 * viewport point `at` (viewport-relative px). Pass the viewport centre to
 * centre a point; pass the cursor to zoom about the cursor. Unclamped -- the
 * browser clamps scrollLeft/Top itself. */
export function scrollForFocus(
  /** @type {{ x: number, y: number }} */ focus,
  /** @type {{ x: number, y: number }} */ at,
  /** @type {{ x: number, y: number }} */ offset,
  /** @type {{ w: number, h: number }} */ content,
) {
  return {
    scrollLeft: offset.x + focus.x * content.w - at.x,
    scrollTop: offset.y + focus.y * content.h - at.y,
  };
}

/** Inverse of the above for hit-testing: which normalized image point sits
 * under a viewport point. May fall outside 0..1 over the padding. */
export function focusAtViewportPoint(
  /** @type {{ x: number, y: number }} */ at,
  /** @type {{ scrollLeft: number, scrollTop: number }} */ scroll,
  /** @type {{ x: number, y: number }} */ offset,
  /** @type {{ w: number, h: number }} */ content,
) {
  return {
    x: (scroll.scrollLeft + at.x - offset.x) / content.w,
    y: (scroll.scrollTop + at.y - offset.y) / content.h,
  };
}

/** Map a pointer position inside the navigator thumbnail to the normalized
 * image point it represents, clamped to the image. */
export function navigatorPointToFocus(
  /** @type {number} */ px,
  /** @type {number} */ py,
  /** @type {{ w: number, h: number }} */ thumb,
) {
  if (!(thumb.w > 0) || !(thumb.h > 0)) return { x: 0.5, y: 0.5 };
  return { x: clamp(px / thumb.w, 0, 1), y: clamp(py / thumb.h, 0, 1) };
}
