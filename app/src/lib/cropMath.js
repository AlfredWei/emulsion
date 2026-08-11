// Crop & Straighten (M3): pure, DOM-free coordinate/geometry math shared
// between DevelopCanvas.svelte (interactive drag handles, live rotation
// preview) and +page.svelte (aspect-ratio preset buttons). Pulled out of
// both components specifically so this math -- the exact area three
// separate real bugs were found in during this feature's own rollout
// (aspect-ratio-space confusion, pan/zoom bleedthrough, a 0x0 preview
// collapse, rotated-canvas overflow, drag math ignoring live rotation) --
// has real, persistent, DOM-free unit test coverage instead of only ever
// being exercised through a throwaway empirical harness against the live
// app. See cropMath.test.js.

/**
 * @typedef {Object} CropRect
 * @property {number} x
 * @property {number} y
 * @property {number} width
 * @property {number} height
 */

export function clamp01(/** @type {number} */ v, /** @type {number} */ lo, /** @type {number} */ hi) {
  return Math.min(Math.max(v, lo), hi);
}

/** Real pixel floor converted to the correct per-axis NORMALIZED fraction
 * for a given source dimension -- e.g. `cropMinFrac(64, sourceWidth)`.
 * Falls back to a conservative 2% when the source dimension isn't known
 * yet (mirrors the fallback this always had before a real image was
 * loaded). */
export function cropMinFrac(/** @type {number} */ minPx, /** @type {number} */ sourceDim) {
  return sourceDim > 0 ? Math.min(minPx / sourceDim, 1) : 0.02;
}

/** Converts a PIXEL aspect ratio (e.g. `1` for 1:1, `16/9` for 16:9) into
 * the equivalent ratio in NORMALIZED crop-rect space (fractions of the
 * source image's own width/height, which are generally NOT equal). This
 * conversion is the single most bug-prone piece of this whole feature --
 * using the raw pixel ratio directly against normalized width/height
 * previously shipped as a real bug (see PROGRESS.md's "crop aspect-ratio
 * presets used the wrong ratio space"). `imageAspect` is `sourceWidth /
 * sourceHeight`. Returns `null` if `imageAspect` isn't known/valid yet, so
 * every caller has one obvious way to say "can't correct this right now"
 * instead of silently producing a wrong ratio.
 * @returns {number | null} */
export function normalizedAspectRatio(/** @type {number | null} */ pixelRatio, /** @type {number} */ imageAspect) {
  if (!pixelRatio || !imageAspect) return null;
  return pixelRatio / imageAspect;
}

/** The largest rect of a given PIXEL aspect ratio that fits centered
 * within the full [0,1]x[0,1] normalized image. Deliberately stateless --
 * doesn't take the CURRENT crop rect as an input at all, so repeated
 * calls with the same ratio are exactly idempotent (never compounds/
 * shrinks across repeated clicks, the failure mode of the original bug
 * this replaced). Returns `null` under the same "can't correct this yet"
 * condition `normalizedAspectRatio` does.
 * @returns {CropRect | null} */
export function largestCenteredCropForRatio(
  /** @type {number} */ pixelRatio,
  /** @type {number} */ sourceWidth,
  /** @type {number} */ sourceHeight,
) {
  const imageAspect = sourceWidth > 0 && sourceHeight > 0 ? sourceWidth / sourceHeight : 0;
  const normalizedRatio = normalizedAspectRatio(pixelRatio, imageAspect);
  if (!normalizedRatio) return null;
  let width = 1;
  let height = width / normalizedRatio;
  if (height > 1) {
    height = 1;
    width = height * normalizedRatio;
  }
  return { x: (1 - width) / 2, y: (1 - height) / 2, width, height };
}

/** Moves the whole rect by (dx,dy), clamped so it never leaves [0,1].
 * @returns {CropRect} */
export function moveCropRect(/** @type {CropRect} */ start, /** @type {number} */ dx, /** @type {number} */ dy) {
  return {
    ...start,
    x: clamp01(start.x + dx, 0, 1 - start.width),
    y: clamp01(start.y + dy, 0, 1 - start.height),
  };
}

/** Which corner is FIXED (diagonally opposite) and which is DRAGGED, for a
 * given corner handle -- the fixed corner never moves during the drag,
 * matching how every real crop tool's corner-resize behaves. */
export function cropCornerPoints(/** @type {string} */ which, /** @type {CropRect} */ r) {
  const left = r.x, top = r.y, right = r.x + r.width, bottom = r.y + r.height;
  if (which === "nw") return { fixed: [right, bottom], dragged: [left, top] };
  if (which === "ne") return { fixed: [left, bottom], dragged: [right, top] };
  if (which === "sw") return { fixed: [right, top], dragged: [left, bottom] };
  return { fixed: [left, top], dragged: [right, bottom] }; // "se"
}

/** Corner-handle resize, with the fixed opposite corner as anchor. When
 * `aspectLock` (a PIXEL ratio) is set, the new size is derived from
 * whichever axis implies the LARGER extent, corrected via
 * `normalizedAspectRatio` before use (see that function's own doc comment
 * for why the raw pixel ratio can never be used directly against
 * normalized width/height). `minFracX`/`minFracY` are the per-axis
 * normalized floors from `cropMinFrac`.
 * @returns {CropRect} */
export function resizeCropCorner(
  /** @type {CropRect} */ start,
  /** @type {string} */ which,
  /** @type {number} */ dx,
  /** @type {number} */ dy,
  /** @type {number | null} */ aspectLock,
  /** @type {number} */ imageAspect,
  /** @type {number} */ minFracX,
  /** @type {number} */ minFracY,
) {
  const { fixed, dragged } = cropCornerPoints(which, start);
  let newX = clamp01(dragged[0] + dx, 0, 1);
  let newY = clamp01(dragged[1] + dy, 0, 1);
  let width = Math.abs(newX - fixed[0]);
  let height = Math.abs(newY - fixed[1]);
  const normalizedRatio = normalizedAspectRatio(aspectLock, imageAspect);
  if (normalizedRatio) {
    if (width / normalizedRatio >= height) {
      height = width / normalizedRatio;
    } else {
      width = height * normalizedRatio;
    }
    const signX = newX >= fixed[0] ? 1 : -1;
    const signY = newY >= fixed[1] ? 1 : -1;
    newX = fixed[0] + signX * width;
    newY = fixed[1] + signY * height;
  }
  width = Math.max(width, minFracX);
  height = Math.max(height, minFracY);
  let x = Math.min(fixed[0], newX);
  let y = Math.min(fixed[1], newY);
  x = clamp01(x, 0, 1 - width);
  y = clamp01(y, 0, 1 - height);
  return { x, y, width, height };
}

/** Edge-handle resize -- always free-form (aspect lock is corner-only, a
 * deliberate, named scope cut: edge-preserves-ratio math combined with
 * bounds/min-size clamping simultaneously is real, fiddly complexity real
 * Lightroom itself doesn't apply symmetrically either).
 * @returns {CropRect} */
export function resizeCropEdge(
  /** @type {CropRect} */ start,
  /** @type {string} */ which,
  /** @type {number} */ dx,
  /** @type {number} */ dy,
  /** @type {number} */ minFracX,
  /** @type {number} */ minFracY,
) {
  let { x, y, width, height } = start;
  if (which === "e") {
    width = clamp01(width + dx, minFracX, 1 - x);
  } else if (which === "w") {
    const newX = clamp01(x + dx, 0, x + width - minFracX);
    width = width + (x - newX);
    x = newX;
  } else if (which === "s") {
    height = clamp01(height + dy, minFracY, 1 - y);
  } else if (which === "n") {
    const newY = clamp01(y + dy, 0, y + height - minFracY);
    height = height + (y - newY);
    y = newY;
  }
  return { x, y, width, height };
}

/** Screen position (as a normalized [0,1] pair) for a given handle,
 * matching the SAME coordinate space the crop rect itself is drawn in.
 * @returns {[number, number]} */
export function cropHandlePos(/** @type {string} */ which, /** @type {CropRect} */ c) {
  const midX = c.x + c.width / 2;
  const midY = c.y + c.height / 2;
  /** @type {Record<string, [number, number]>} */
  const positions = {
    nw: [c.x, c.y],
    n: [midX, c.y],
    ne: [c.x + c.width, c.y],
    e: [c.x + c.width, midY],
    se: [c.x + c.width, c.y + c.height],
    s: [midX, c.y + c.height],
    sw: [c.x, c.y + c.height],
    w: [c.x, midY],
  };
  return positions[which];
}

/** Derives an element's TRUE (unrotated) on-screen box from its CURRENT
 * `getBoundingClientRect()` plus its own true CSS-pixel size (e.g.
 * `offsetWidth`/`offsetHeight` -- layout measurements a CSS `transform`
 * never affects).
 *
 * Why this is needed at all: the crop tool's overlay (dim bands, the crop
 * rect, every handle) lives in a FIXED, UNROTATED coordinate space --
 * positioned via `offsetLeft`/`offsetTop`/`offsetWidth`/`offsetHeight`,
 * which a live `transform: rotate()` on the canvas never touches (see
 * this project's own straighten-preview design: the boundary stays fixed
 * on screen while the photo content visibly rotates underneath it).
 * `getBoundingClientRect()` on a ROTATED canvas, though, reflects the
 * rotated shape's axis-aligned bounding box -- strictly LARGER than the
 * true unrotated box, and offset from it -- so using its `width`/
 * `height`/`left`/`top` directly to interpret a click (as this file
 * briefly did, and had to correct) silently mis-scales/mis-offsets the
 * result the moment a nonzero angle is set: NOT because the click itself
 * needs "un-rotating" (the overlay/handles never rotate, so a click on
 * them is already in the right space), but because the rotated rect's
 * OWN reported width/height/left/top are simply the wrong numbers for a
 * space that never rotated in the first place.
 *
 * The fix exploits one fact that stays true regardless of rotation: a CSS
 * rotation around an element's own center (the default `transform-origin:
 * 50% 50%`) never moves that center point, so `rect`'s own center --
 * even though `rect` itself is the larger, rotated AABB -- is reliable
 * for deriving the box's TRUE top-left. At angle 0 this is an exact
 * identity (`rect` already IS the true box), verified in
 * cropMath.test.js -- a strict generalization, safe to call
 * unconditionally rather than needing an "only when rotated" branch. */
export function trueElementBox(
  /** @type {{left: number, top: number, width: number, height: number}} */ rect,
  /** @type {number} */ trueWidth,
  /** @type {number} */ trueHeight,
) {
  const centerX = rect.left + rect.width / 2;
  const centerY = rect.top + rect.height / 2;
  return { left: centerX - trueWidth / 2, top: centerY - trueHeight / 2, width: trueWidth, height: trueHeight };
}
