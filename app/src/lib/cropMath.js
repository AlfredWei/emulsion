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

/** The largest CENTERED rect of a given PIXEL aspect ratio that stays
 * entirely inside the image once it's rotated by `angleDeg` about its own
 * center -- i.e. the inner-fit region a straighten/rotate needs so the
 * fixed on-screen crop viewport never exposes the rotated image's
 * blanked-out corners (see `rotate_image` in develop_engine.rs, which
 * fills out-of-bounds samples with black; both sides rotate about the
 * same image-center pivot, verified in that file's own test suite).
 *
 * Derived from the standard "axis-aligned rect inscribed in a rotated
 * rect" support-function argument, done in PIXEL space (not normalized --
 * the two axes scale differently whenever the source isn't square, the
 * same trap `normalizedAspectRatio` exists to avoid elsewhere in this
 * file): for a centered rect with pixel half-extents (a, b) to stay inside
 * a `sourceWidth` x `sourceHeight` rect rotated by `angleDeg`, every one
 * of its 4 corners must land back inside the unrotated source once
 * rotated by `-angleDeg`, which reduces (by the symmetry of |sin|/|cos|
 * across quadrants) to two linear constraints:
 *   a*cos + b*sin <= sourceWidth/2
 *   a*sin + b*cos <= sourceHeight/2
 * Substituting `b = a / pixelRatio` and solving each for `a` gives the two
 * candidate bounds; the tighter one is the answer. At angle 0 this
 * degenerates to exactly `largestCenteredCropForRatio`'s own result (sin=0,
 * cos=1 leaves only the `a <= sourceWidth/2` / `a <= sourceHeight/2 *
 * pixelRatio` pair) -- kept as a separate function anyway since callers
 * that already know they're at angle 0 (e.g. this file's own unit tests)
 * shouldn't have to thread a redundant `0` through every call site.
 *
 * Deliberately CENTERED-only, same named scope cut as this file's other
 * angle-unaware helpers: an off-center crop's own maximal inscribed bound
 * needs the same per-corner argument repeated at an arbitrary offset --
 * real added complexity this function's only caller (recentering the crop
 * whenever the straighten angle itself changes) doesn't need to solve.
 * @returns {CropRect | null} */
export function inscribedCropForAngle(
  /** @type {number} */ pixelRatio,
  /** @type {number} */ sourceWidth,
  /** @type {number} */ sourceHeight,
  /** @type {number} */ angleDeg,
) {
  if (!pixelRatio || sourceWidth <= 0 || sourceHeight <= 0) return null;
  const rad = (Math.abs(angleDeg) * Math.PI) / 180;
  const sin = Math.abs(Math.sin(rad));
  const cos = Math.abs(Math.cos(rad));
  const aFromWidth = sourceWidth / 2 / (cos + sin / pixelRatio);
  const aFromHeight = sourceHeight / 2 / (sin + cos / pixelRatio);
  const aPx = Math.min(aFromWidth, aFromHeight);
  const bPx = aPx / pixelRatio;
  const width = clamp01((2 * aPx) / sourceWidth, 0, 1);
  const height = clamp01((2 * bPx) / sourceHeight, 0, 1);
  return { x: (1 - width) / 2, y: (1 - height) / 2, width, height };
}

/** Whether every corner of `rect` (normalized [0,1] coordinates, in the
 * SAME rotate-in-place frame `inscribedCropForAngle` and
 * `develop_engine.rs`'s `rotate_image`/`crop_rect_px` use) samples real
 * source pixels once the image is rotated by `angleDeg` about its own
 * center, rather than the blanked-out (black-filled) corners
 * `rotate_image` leaves behind. Off-center rects are real inputs here
 * (unlike `inscribedCropForAngle`, which only ever proposes a CENTERED
 * replacement) -- this is the general per-corner check that drag/resize
 * needs to REJECT a move/resize that would push an existing, possibly
 * off-center rect into that blanked region, not just to compute a fresh
 * centered one. Uses the exact inverse mapping `rotate_image` samples
 * with (`source = center + R_cw(-theta) * (output - center)`, hand-
 * verified against CSS `rotate(deg)` in that function's own test) so a
 * rect this reports valid is GUARANTEED to exclude every black pixel
 * `rotate_image` would produce, not just approximately so. At angle 0
 * this is trivially true for any in-bounds rect (sin=0, cos=1 reduces the
 * mapping to the identity).
 * @returns {boolean} */
export function cropRectFitsRotatedBounds(
  /** @type {CropRect} */ rect,
  /** @type {number} */ sourceWidth,
  /** @type {number} */ sourceHeight,
  /** @type {number} */ angleDeg,
) {
  if (sourceWidth <= 0 || sourceHeight <= 0) return false;
  if (!angleDeg) return true;
  const rad = (angleDeg * Math.PI) / 180;
  const sin = Math.sin(rad);
  const cos = Math.cos(rad);
  const cx = sourceWidth / 2;
  const cy = sourceHeight / 2;
  const corners = [
    [rect.x * sourceWidth, rect.y * sourceHeight],
    [(rect.x + rect.width) * sourceWidth, rect.y * sourceHeight],
    [rect.x * sourceWidth, (rect.y + rect.height) * sourceHeight],
    [(rect.x + rect.width) * sourceWidth, (rect.y + rect.height) * sourceHeight],
  ];
  const eps = 1e-3;
  return corners.every(([ox, oy]) => {
    const dx = ox - cx;
    const dy = oy - cy;
    const sx = cx + dx * cos + dy * sin;
    const sy = cy - dx * sin + dy * cos;
    return sx >= -eps && sx <= sourceWidth + eps && sy >= -eps && sy <= sourceHeight + eps;
  });
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

/** The committed-crop CSS preview's `.crop-clip` wrapper, sized to the
 * crop rect's own NATIVE PIXEL dimensions -- what a 100%-zoom view needs
 * so the canvas (percentage-positioned inside it, see
 * DevelopCanvas.svelte's own markup doc comment) renders at true 1:1
 * scale: canvas CSS width = `(100/crop.width)%` of this box's width =
 * `(1/crop.width) * crop.width * sourceWidth` = `sourceWidth`, exactly
 * matching the canvas's own backing-store width. Trivial arithmetic, but
 * pulled out as its own named function (not inlined at the one call site)
 * so the "why 100% zoom works here at all" reasoning above has one place
 * to live and one place to be tested, matching every other piece of this
 * feature's coordinate math.
 * @returns {{w: number, h: number}} */
export function nativeCropClipSize(
  /** @type {CropRect} */ crop,
  /** @type {number} */ sourceWidth,
  /** @type {number} */ sourceHeight,
) {
  return { w: crop.width * sourceWidth, h: crop.height * sourceHeight };
}

/** Where to scroll a zoomed viewport so a given point stays centered,
 * correcting for the committed-crop preview's own coordinate offset.
 *
 * `nativeX`/`nativeY` are a focus point in the FULL image's native-pixel
 * space (e.g. from `screenToNativePixel` against the canvas element,
 * which always reports coordinates relative to the whole source image --
 * see that function's own doc comment: it doesn't know or care whether a
 * crop is committed). When a crop IS committed, though, the scrollable
 * box is `.crop-clip` -- NOT the canvas itself -- and `.crop-clip`'s own
 * origin sits `crop.x * sourceWidth` / `crop.y * sourceHeight` native
 * pixels away from the canvas's own (0,0), because the canvas is
 * positioned inside it via a `left: -crop.x*100/crop.width%` offset (see
 * DevelopCanvas.svelte's markup): the crop's own top-left, not the full
 * image's, is what lands at `.crop-clip`'s (0,0). A focus point given in
 * full-image coordinates therefore has to be re-based onto `.crop-clip`'s
 * own origin before it means anything as a scroll target -- skipping this
 * correction (using `nativeX`/`nativeY` directly, as the plain uncropped
 * case correctly does) silently scrolls to the wrong place by exactly
 * that offset the moment a crop is committed.
 * @returns {{scrollLeft: number, scrollTop: number}} */
export function scrollTargetForNativeFocus(
  /** @type {number} */ nativeX,
  /** @type {number} */ nativeY,
  /** @type {boolean} */ isCropped,
  /** @type {CropRect} */ crop,
  /** @type {number} */ sourceWidth,
  /** @type {number} */ sourceHeight,
  /** @type {number} */ viewportWidth,
  /** @type {number} */ viewportHeight,
) {
  const targetX = isCropped ? nativeX - crop.x * sourceWidth : nativeX;
  const targetY = isCropped ? nativeY - crop.y * sourceHeight : nativeY;
  return { scrollLeft: targetX - viewportWidth / 2, scrollTop: targetY - viewportHeight / 2 };
}
