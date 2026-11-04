// Object removal tool state (RFC-0032 slice E): the marks being drawn, the brush, and the job. Pure view state, never
// written to the edit stack; the finished patches are (by the actions in removalActions.js). The tool is "armed" by
// `masks.activeTool === "remove"`, which also gives the canvas its full-photo view and keeps one tool active at a time.

import { masks } from "./masks.svelte.js";

/** How big a mark may be (its longer side as a fraction of the photograph's longer side) before the tool warns. */
export const LARGE_MARK_FRACTION = 0.25;

export class RemovalStore {
  /** @param {import('./masks.svelte.js').MaskStore} masks */
  constructor(masks) {
    this.masks = masks;
  }

  /** The tool is armed. */
  active = $derived(this.masks.activeTool === "remove");
  /** "brush" paints strokes; "lasso" draws an outline. */
  mode = $state(/** @type {"brush" | "lasso"} */ ("brush"));
  /** Brush radius as a fraction of the photograph's longer side. */
  brushRadius = $state(0.012);
  /** The marks drawn and not yet removed, in the order drawn. */
  shapes = $state(/** @type {import('$lib/api/removal.js').MaskShape[]} */ ([]));
  /** "idle" | "running". */
  phase = $state(/** @type {"idle" | "running"} */ ("idle"));
  /** The last failure, shown as is; "" for none. */
  error = $state("");
  /** What the last removal did, e.g. "Removed 2 things"; "" for none. */
  notice = $state("");
  /** Whether the model is installed; null until asked. */
  available = $state(/** @type {boolean | null} */ (null));
  /** The marks are large: the next press of Remove asks for confirmation instead of running. */
  guarding = $state(false);

  /** The marks' joint extent: the longer side of their bounding box as a fraction of the photograph's longer side.
   * `aspect` is the photograph's width / height (a normalised x and y span different lengths unless it is 1); a
   * stroke counts its radius on every side.
   * @param {number} [aspect] */
  extent(aspect = 1) {
    // The photograph's sides in units of its longer side.
    const wl = aspect >= 1 ? 1 : aspect;
    const hl = aspect >= 1 ? 1 / aspect : 1;
    let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
    for (const s of this.shapes) {
      const r = s.kind === "stroke" ? s.radius : 0;
      for (const [x, y] of s.points) {
        x0 = Math.min(x0, x - r / wl);
        x1 = Math.max(x1, x + r / wl);
        y0 = Math.min(y0, y - r / hl);
        y1 = Math.max(y1, y + r / hl);
      }
    }
    if (!Number.isFinite(x0)) return 0;
    return Math.max((x1 - x0) * wl, (y1 - y0) * hl);
  }

  clearMarks() {
    this.shapes = [];
    this.guarding = false;
  }
}

export const removal = new RemovalStore(masks);
