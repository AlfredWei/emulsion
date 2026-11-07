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
  /** "brush" paints strokes; "lasso" draws an outline; "select" takes the object under a click (the click-select model). */
  mode = $state(/** @type {"brush" | "lasso" | "select"} */ ("brush"));
  /** Brush radius as a fraction of the photograph's longer side. */
  brushRadius = $state(0.012);
  /** The installed removal engine's id; "" until asked. A removal made with another one can be tried again. */
  engine = $state("");
  /** Which of the photograph's finished removals have a stored hole (so can be healed instead), by patch id. */
  healable = $state(/** @type {number[]} */ ([]));
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
  /** The object the last click-select click(s) took, still being refined: its clicks, the model's candidates for them and
   * the one shown. It is a mark like any other once the user starts another object or leaves select mode.
   * @type {null | { prompts: import('$lib/api/develop.js').SegmentPrompt[], candidates: import('$lib/api/segment.js').SegmentCandidate[], index: number, mark: import('$lib/api/removal.js').MaskShape }} */
  selection = $state(null);
  /** Where click-select is: "idle" (not asked / model missing), "preparing" (the encoder runs on the photograph), "ready", "deciding" (a click is decoded). */
  selectPhase = $state(/** @type {"idle" | "preparing" | "ready" | "deciding"} */ ("idle"));
  /** Why click-select is not usable or what the last click did, shown as is; "" for none. */
  selectNote = $state("");
  /** Content hash the click-select helper holds an embedding for. */
  preparedHash = /** @type {string | null} */ (null);
  /** Bumped when the marks are cleared so an answer for abandoned marks is ignored. */
  generation = 0;
  /** The marks are large: the next press of Remove asks for confirmation instead of running. */
  guarding = $state(false);

  /** Every mark that would be removed: the drawn and selected ones, then the object still being refined. */
  allShapes() {
    return this.selection ? [...this.shapes, this.selection.mark] : this.shapes;
  }

  /** How many marks would be removed. */
  get count() {
    return this.shapes.length + (this.selection ? 1 : 0);
  }

  /** The marks' joint extent: the longer side of their bounding box as a fraction of the photograph's longer side.
   * `aspect` is the photograph's width / height (a normalised x and y span different lengths unless it is 1); a
   * stroke counts its radius on every side.
   * @param {number} [aspect] */
  extent(aspect = 1) {
    // The photograph's sides in units of its longer side.
    const wl = aspect >= 1 ? 1 : aspect;
    const hl = aspect >= 1 ? 1 / aspect : 1;
    let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
    for (const s of this.allShapes()) {
      const r = s.kind === "stroke" ? s.radius : 0;
      const pts = s.kind === "segment" ? [[s.bbox[0], s.bbox[1]], [s.bbox[2], s.bbox[3]]] : s.points;
      for (const [x, y] of pts) {
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
    this.generation++;
    this.shapes = [];
    this.selection = null;
    this.guarding = false;
  }
}

export const removal = new RemovalStore(masks);
