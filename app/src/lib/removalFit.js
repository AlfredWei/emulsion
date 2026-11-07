// Whether a removal in the stack can still be shown on the open photograph (RFC-0032 §12.1, §20). The render skips a patch
// whose decode has another aspect ratio than the photograph (the source file was replaced), and a patch row can be missing
// (a damaged catalog); the strip says so instead of leaving the removal silently out of the picture.

/** Same tolerance as the render's (`removal.rs`, ASPECT_TOLERANCE): relative difference of the aspect ratios. */
export const ASPECT_TOLERANCE = 0.01;

/**
 * @typedef {{ w: number, h: number, hole: boolean } | null} RemovalRow - what the catalog knows of a removal; null when it has no row
 */

/**
 * What is wrong with a removal, or null when nothing is known to be. `photoAspect` is the open photograph's width / height
 * (0 or NaN when not known yet: then only a missing row is reported).
 * @param {RemovalRow | undefined} row @param {number} photoAspect
 * @returns {"missing" | "no longer fits" | null}
 */
export function removalProblem(row, photoAspect) {
  if (row === undefined) return null; // not asked yet
  if (row === null) return "missing";
  if (!(photoAspect > 0) || row.w <= 0 || row.h <= 0) return null;
  const a = row.w / row.h;
  return Math.abs(a - photoAspect) / Math.max(a, photoAspect) > ASPECT_TOLERANCE ? "no longer fits" : null;
}
