// Pure math for Develop's per-slider step-nudge (up/down) buttons (M4.5
// Slice 7). Isolated from DevelopPanel.svelte so the float-precision
// rounding below has its own regression coverage rather than being
// eyeballed through the live app.

/** Number of digits after the decimal point in `step` (e.g. 0.05 -> 2, 1 -> 0). */
function stepDecimalPlaces(/** @type {number} */ step) {
  const s = String(step);
  const i = s.indexOf(".");
  return i === -1 ? 0 : s.length - i - 1;
}

/**
 * Nudges `value` by one `step` in the given `direction` (+1 or -1), clamped
 * to [min, max]. Rounds to `step`'s own decimal precision -- without this,
 * repeated nudges on a fractional step (Exposure's 0.05, Perspective
 * Rotate's 0.1) drift into values like 0.15000000000000002 from plain
 * floating-point addition.
 */
export function nudgeValue(
  /** @type {number} */ value,
  /** @type {1 | -1} */ direction,
  /** @type {number} */ step,
  /** @type {number} */ min,
  /** @type {number} */ max,
) {
  const raw = value + direction * step;
  const clamped = Math.min(max, Math.max(min, raw));
  const factor = 10 ** stepDecimalPlaces(step);
  return Math.round(clamped * factor) / factor;
}
