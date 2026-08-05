// Thin wrapper around the Develop-related Tauri commands (M1 Slice 3, see
// app/src-tauri/src/lib.rs) — keeps raw command-name strings out of components.

import { invoke } from "@tauri-apps/api/core";

/**
 * @typedef {Object} DevelopPreviewInfo
 * @property {string} path
 * @property {number} width
 * @property {number} height
 */

/**
 * @typedef {Object} EditOp
 * @property {string} op
 * @property {number} value
 */

/**
 * @typedef {Object} LinearGradientMask
 * @property {"linear_gradient_mask"} op
 * @property {string} id
 * @property {{x: number, y: number}} start
 * @property {{x: number, y: number}} end
 * @property {number} feather
 * @property {boolean} invert
 * @property {number} exposure
 * @property {number} contrast
 * @property {number} saturation
 */

/**
 * @typedef {Object} RadialGradientMask
 * @property {"radial_gradient_mask"} op
 * @property {string} id
 * @property {{x: number, y: number}} center
 * @property {number} radiusX
 * @property {number} radiusY
 * @property {number} feather
 * @property {boolean} invert
 * @property {number} exposure
 * @property {number} contrast
 * @property {number} saturation
 */

/**
 * @typedef {Object} Dab
 * @property {number} x
 * @property {number} y
 * @property {number} radius - normalized fraction of image WIDTH only (a
 *   single scalar, not radiusX/radiusY like radial masks) -- rasterization
 *   happens directly in the offscreen canvas's own native-pixel space
 *   (DevelopCanvas.svelte), so `radius * nativeWidth` used for both
 *   dimensions of `ctx.arc()` is inherently a true circle in image-pixel
 *   space, with no separate axis scaling needed.
 * @property {number} hardness - 0 (fully soft) .. 100 (harder edge), baked
 *   into this dab's own radial-gradient falloff at paint time.
 * @property {number} flow - 0..1, baked in at paint time.
 * @property {"add" | "erase"} mode
 */

/**
 * @typedef {Object} BrushMask
 * @property {"brush_mask"} op
 * @property {string} id
 * @property {Dab[]} dabs
 * @property {boolean} invert
 * @property {number} exposure
 * @property {number} contrast
 * @property {number} saturation
 */

/**
 * @typedef {Object} LuminanceRangeMask
 * @property {"luminance_range_mask"} op
 * @property {string} id
 * @property {number} rangeMin - 0-100, matching linear/radial's own feather
 *   scale convention (not a separate 0-1 scale).
 * @property {number} rangeMax - 0-100.
 * @property {number} feather - 0-100, band WIDTH around each of the two
 *   range edges (a different meaning from linear/radial's single-boundary
 *   feather, so it's shown via a dedicated Min/Max/Feather block in
 *   MaskEditorPanel.svelte, not the shared Feather row).
 * @property {boolean} invert
 * @property {number} exposure
 * @property {number} contrast
 * @property {number} saturation
 */

/**
 * @typedef {Object} ColorRangeMask
 * @property {"color_range_mask"} op
 * @property {string} id
 * @property {{r: number, g: number, b: number}} refColor - 0-1 floats,
 *   sampled via a canvas click (see DevelopCanvas.svelte's
 *   sampleSourcePixel) -- matches WGSL's own 0-1 texture-sample
 *   convention, not 0-255.
 * @property {number} range - 0-100, color-distance tolerance around
 *   refColor (a single threshold, not linear/radial's boundary position).
 * @property {number} feather - 0-100, transition-band width beyond
 *   `range` -- a different meaning from linear/radial's single-boundary
 *   feather and from luminance range's two-edge band width, so it's shown
 *   via its own dedicated block in MaskEditorPanel.svelte alongside Range,
 *   not the shared Feather row.
 * @property {boolean} invert
 * @property {number} exposure
 * @property {number} contrast
 * @property {number} saturation
 */

/** @typedef {LinearGradientMask | RadialGradientMask | BrushMask | LuminanceRangeMask | ColorRangeMask} Mask */

/**
 * @typedef {Object} EditStack
 * @property {number} schema_version
 * @property {Array<EditOp | Mask>} ops
 */

/** @returns {Promise<DevelopPreviewInfo>} */
export function getDevelopPreview(/** @type {string} */ path) {
  return invoke("get_develop_preview", { path });
}

/** The 1:1 tier alongside `getDevelopPreview`'s Standard/draft tier --
 * uncapped native resolution, built lazily by the backend on first call
 * (see `preview_cache::ensure_develop_full_preview`'s doc comment).
 * DevelopCanvas.svelte only calls this once the user actually zooms an
 * image to 100%, not on every Develop open.
 * @returns {Promise<DevelopPreviewInfo>} */
export function getDevelopFullPreview(/** @type {string} */ path) {
  return invoke("get_develop_full_preview", { path });
}

/** @returns {Promise<EditStack>} */
export function getEditStack(/** @type {number} */ versionId) {
  return invoke("get_edit_stack", { versionId });
}

/** @returns {Promise<void>} */
export function setEditStack(/** @type {number} */ versionId, /** @type {EditStack} */ stack) {
  return invoke("set_edit_stack", { versionId, stack });
}

/** Thumbnail refresh after a Develop edit -- call AFTER setEditStack has
 * already resolved, never chained onto that same call, so a slow/failed
 * regen can never delay the edit save or app quit. Resolves to `null`
 * (not an error) if regeneration failed for any reason -- see the Rust
 * command's doc comment.
 * @returns {Promise<string | null>} the new thumbnail_path, or null */
export function regenerateThumbnail(/** @type {number} */ versionId) {
  return invoke("regenerate_thumbnail", { versionId });
}

/** Find an op's current value by name, or a fallback if not present yet. */
export function opValue(/** @type {EditStack} */ stack, /** @type {string} */ opName, /** @type {number} */ fallback) {
  const op = /** @type {any} */ (stack.ops.find((o) => o.op === opName));
  return op?.value ?? fallback;
}

/** Upsert an op by name (replace if present, append if not) — matches the
 * catalog's edit-stack model (ADR-0006): a slider's current value is a
 * single entry, not an append-only history log. */
export function upsertOp(/** @type {EditStack} */ stack, /** @type {string} */ opName, /** @type {number} */ value) {
  const ops = stack.ops.filter((o) => o.op !== opName);
  ops.push({ op: opName, value });
  return { ...stack, ops };
}

// Tone Curve (M3): a global-only op (see the pipeline-order comment in
// develop_engine.rs/DevelopCanvas.svelte -- exposure -> contrast ->
// saturation -> tone curve, applied before any mask reads the graded rgb),
// but its payload is a structured `points` array, not a single scalar --
// the same reason masks needed their own CRUD helpers instead of reusing
// opValue/upsertOp above.
export const MAX_CURVE_POINTS = 16;
// Two permanent endpoint anchors, identity (no adjustment) -- a straight
// line, not an artificial default S-curve, matching every other adjustment
// in this app defaulting to a true no-op. Frozen so a caller can't
// accidentally mutate the shared default array in place.
export const IDENTITY_TONE_CURVE = Object.freeze([
  Object.freeze({ x: 0, y: 0 }),
  Object.freeze({ x: 1, y: 1 }),
]);
// Matches 8-bit output granularity -- see buildToneCurveLut's own doc
// comment for why both this module and develop_engine.rs build the exact
// same discretized LUT rather than each evaluating the spline exactly.
export const CURVE_LUT_SAMPLES = 256;

/** @returns {readonly {x:number,y:number}[]} */
export function getToneCurvePoints(
  /** @type {EditStack} */ stack,
  /** @type {readonly {x:number,y:number}[]} */ fallback = IDENTITY_TONE_CURVE,
) {
  const op = /** @type {any} */ (stack.ops.find((o) => o.op === "tone_curve"));
  return op?.points ?? fallback;
}

/** @returns {EditStack} */
export function upsertToneCurve(
  /** @type {EditStack} */ stack,
  /** @type {readonly {x:number,y:number}[]} */ points,
) {
  const ops = stack.ops.filter((o) => o.op !== "tone_curve");
  ops.push(/** @type {any} */ ({ op: "tone_curve", points }));
  return { ...stack, ops };
}

/** Fritsch-Carlson monotonic cubic Hermite tangents -- chosen over a naive
 * natural cubic spline specifically because it never overshoots between
 * control points (a natural spline can ring/overshoot and produce visibly
 * reversed-tone banding in a photo). `pts` must already be sorted by x.
 *
 * Step C below (zeroing a tangent whose neighboring secants disagree in
 * sign) is a real, easy-to-skip requirement, not redundant with Step D:
 * Step B's plain averaged tangent can end up with the WRONG SIGN at a
 * local extremum in the data, and Step D's magnitude-only test
 * (alpha^2+beta^2 > 9) does not by itself catch a sign disagreement --
 * only Step C's explicit sign check does. Both steps are required for the
 * result to actually be monotonic. */
function computeTangents(/** @type {{x:number,y:number}[]} */ pts) {
  const n = pts.length;
  const d = [];
  for (let k = 0; k < n - 1; k++) {
    d.push((pts[k + 1].y - pts[k].y) / (pts[k + 1].x - pts[k].x));
  }
  const m = new Array(n);
  m[0] = d[0];
  m[n - 1] = d[n - 2];
  for (let k = 1; k < n - 1; k++) m[k] = (d[k - 1] + d[k]) / 2;
  // Step C: necessary-condition zeroing at interior local extrema.
  for (let k = 1; k < n - 1; k++) {
    if (d[k - 1] === 0 || d[k] === 0 || Math.sign(d[k - 1]) !== Math.sign(d[k])) m[k] = 0;
  }
  // Step D: sufficient-condition (Fritsch-Carlson) rescale, ascending --
  // each interval only ever rescales its own two endpoint tangents using
  // whatever value they currently hold; already-processed intervals are
  // not revisited (the standard single-forward-pass formulation).
  for (let k = 0; k < n - 1; k++) {
    if (d[k] === 0) {
      m[k] = 0;
      m[k + 1] = 0;
      continue;
    }
    const alpha = m[k] / d[k];
    const beta = m[k + 1] / d[k];
    const s = alpha * alpha + beta * beta;
    if (s > 9) {
      const tau = 3 / Math.sqrt(s);
      m[k] = tau * alpha * d[k];
      m[k + 1] = tau * beta * d[k];
    }
  }
  return m;
}

/** Evaluates the Hermite cubic through `pts` (with tangents `m`) at `x`,
 * clamped to the curve's own domain. Segment lookup is a linear scan --
 * fine given MAX_CURVE_POINTS's small bound and that this only runs
 * CURVE_LUT_SAMPLES times per LUT build, not per pixel. */
function hermiteAt(/** @type {{x:number,y:number}[]} */ pts, /** @type {number[]} */ m, /** @type {number} */ x) {
  const n = pts.length;
  const xc = Math.min(Math.max(x, pts[0].x), pts[n - 1].x);
  let k = 0;
  while (k < n - 2 && xc > pts[k + 1].x) k++;
  const h = pts[k + 1].x - pts[k].x;
  const t = h === 0 ? 0 : (xc - pts[k].x) / h;
  const t2 = t * t;
  const t3 = t2 * t;
  const h00 = 2 * t3 - 3 * t2 + 1;
  const h10 = t3 - 2 * t2 + t;
  const h01 = -2 * t3 + 3 * t2;
  const h11 = t3 - t2;
  return h00 * pts[k].y + h10 * h * m[k] + h01 * pts[k + 1].y + h11 * h * m[k + 1];
}

/** The one canonical curve-LUT builder -- consumed by DevelopCanvas.svelte
 * (GPU uniform upload) AND ToneCurveEditor.svelte (drawing the visual
 * curve), so what's drawn always exactly matches what's applied.
 *
 * `develop_engine.rs`'s CPU export path builds this SAME discretized
 * 256-sample LUT (not an independently-exact spline evaluation) so parity
 * between the interactive preview and the final export is by
 * construction -- both sides apply the identical piecewise-linear
 * approximation of the spline, not two differently-rounded exact
 * evaluations that could diverge near a curve's extrema.
 * @returns {Float32Array} length `samples` */
export function buildToneCurveLut(
  /** @type {readonly {x:number,y:number}[]} */ points,
  /** @type {number} */ samples = CURVE_LUT_SAMPLES,
) {
  const pts = [...points].sort((a, b) => a.x - b.x);
  const m = computeTangents(pts);
  const lut = new Float32Array(samples);
  for (let i = 0; i < samples; i++) {
    lut[i] = Math.min(Math.max(hermiteAt(pts, m, i / (samples - 1)), 0), 1);
  }
  return lut;
}

/** Samples a built LUT with linear interpolation between the two nearest
 * entries -- the same formula develop_engine.rs's `sample_lut` and the
 * WGSL shader's `sampleCurveLut` both use, so all three sides agree. */
export function sampleCurveLut(/** @type {Float32Array} */ lut, /** @type {number} */ v) {
  const idx = Math.min(Math.max(v, 0), 1) * (lut.length - 1);
  const i0 = Math.floor(idx);
  const i1 = Math.min(i0 + 1, lut.length - 1);
  const frac = idx - i0;
  return lut[i0] * (1 - frac) + lut[i1] * frac;
}

// HSL / Color Mixer (M3): a global-only op (applied after exposure ->
// contrast -> saturation -> tone curve, before any mask -- see
// develop_engine.rs/DevelopCanvas.svelte's shared pipeline-order comment),
// with a band-KEYED payload (not a single scalar, not an array) -- a
// per-band UI control needs to patch exactly one band's 3 values without
// threading the other 21 through, which a flat array index wouldn't give
// for free. Band order is fixed and shared with the GPU-upload/CPU-parse
// order in DevelopCanvas.svelte/develop_engine.rs -- no band-name string
// is ever uploaded to the GPU, only positional order, so this list is the
// single source of truth both sides must stay in lockstep with.
export const HSL_BAND_NAMES = ["red", "orange", "yellow", "green", "aqua", "blue", "purple", "magenta"];
// Evenly spaced 45 degrees apart starting at red -- this is a from-scratch
// reimplementation, not file-compatible with Lightroom, and there's no
// verifiable source for Adobe's own exact internal band-center angles to
// match instead. Index-aligned with HSL_BAND_NAMES.
export const HSL_BAND_CENTERS_DEG = [0, 45, 90, 135, 180, 225, 270, 315];

export const IDENTITY_HSL_BANDS = Object.freeze(
  Object.fromEntries(
    HSL_BAND_NAMES.map((name) => [name, Object.freeze({ hue: 0, saturation: 0, luminance: 0 })]),
  ),
);

/** @returns {Readonly<Record<string, {hue: number, saturation: number, luminance: number}>>} */
export function getHslBands(
  /** @type {EditStack} */ stack,
  /** @type {Readonly<Record<string, {hue: number, saturation: number, luminance: number}>>} */ fallback = IDENTITY_HSL_BANDS,
) {
  const op = /** @type {any} */ (stack.ops.find((o) => o.op === "hsl"));
  return op?.bands ?? fallback;
}

/** Patches ONE band's values (any subset of hue/saturation/luminance),
 * leaving the other 7 bands untouched -- the access pattern a per-band UI
 * control actually needs, matching how masks needed their own CRUD
 * instead of opValue/upsertOp's single-scalar shape.
 * @returns {EditStack} */
export function upsertHslBand(
  /** @type {EditStack} */ stack,
  /** @type {string} */ bandName,
  /** @type {Partial<{hue: number, saturation: number, luminance: number}>} */ patch,
) {
  const current = getHslBands(stack);
  const bands = { ...current, [bandName]: { ...current[bandName], ...patch } };
  const ops = stack.ops.filter((o) => o.op !== "hsl");
  ops.push(/** @type {any} */ ({ op: "hsl", bands }));
  return { ...stack, ops };
}

/** Packs the 8-band object into the exact Float32Array layout
 * DevelopCanvas.svelte's `hslBandsBuffer` expects (8 x vec4, band order =
 * HSL_BAND_NAMES, w-component unused padding). */
export function buildHslUniformData(
  /** @type {Readonly<Record<string, {hue: number, saturation: number, luminance: number}>>} */ bands,
) {
  const data = new Float32Array(32);
  HSL_BAND_NAMES.forEach((name, i) => {
    const b = bands[name] ?? IDENTITY_HSL_BANDS[name];
    data.set([b.hue, b.saturation, b.luminance, 0], i * 4);
  });
  return data;
}

// Split Toning (M3): a global-only op (applied after exposure ->
// contrast -> saturation -> tone curve -> HSL, before any mask -- see
// develop_engine.rs/DevelopCanvas.svelte's shared pipeline-order
// comment), with a NESTED per-zone payload -- a per-zone UI control
// patches one zone's hue/saturation without touching the other zone or
// balance, same access pattern upsertHslBand already gives per-band. Also
// scales cleanly to a later 3-zone Color Grading follow-up (a `midtones`
// key becomes additive, not a flat-naming-scheme collision).
export const IDENTITY_SPLIT_TONING = Object.freeze({
  shadows: Object.freeze({ hue: 0, saturation: 0 }),
  highlights: Object.freeze({ hue: 0, saturation: 0 }),
  balance: 0,
});

/** @returns {{shadows: {hue: number, saturation: number}, highlights: {hue: number, saturation: number}, balance: number}} */
export function getSplitToning(
  /** @type {EditStack} */ stack,
  /** @type {typeof IDENTITY_SPLIT_TONING} */ fallback = IDENTITY_SPLIT_TONING,
) {
  const op = /** @type {any} */ (stack.ops.find((o) => o.op === "split_toning"));
  if (!op) return fallback;
  return {
    shadows: { ...IDENTITY_SPLIT_TONING.shadows, ...op.shadows },
    highlights: { ...IDENTITY_SPLIT_TONING.highlights, ...op.highlights },
    balance: op.balance ?? 0,
  };
}

/** Patches ONE zone's hue/saturation (any subset), leaving the other
 * zone and balance untouched.
 * @returns {EditStack} */
export function upsertSplitToningZone(
  /** @type {EditStack} */ stack,
  /** @type {"shadows" | "highlights"} */ zone,
  /** @type {Partial<{hue: number, saturation: number}>} */ patch,
) {
  const current = getSplitToning(stack);
  const next = { ...current, [zone]: { ...current[zone], ...patch } };
  const ops = stack.ops.filter((o) => o.op !== "split_toning");
  ops.push(/** @type {any} */ ({ op: "split_toning", ...next }));
  return { ...stack, ops };
}

/** @returns {EditStack} */
export function upsertSplitToningBalance(/** @type {EditStack} */ stack, /** @type {number} */ balance) {
  const current = getSplitToning(stack);
  const ops = stack.ops.filter((o) => o.op !== "split_toning");
  ops.push(/** @type {any} */ ({ op: "split_toning", ...current, balance }));
  return { ...stack, ops };
}

/** Packs into the exact Float32Array layout DevelopCanvas.svelte's
 * `splitToningBuffer`/the WGSL `SplitToning` struct expects (field order
 * matters -- must match the struct's own field order exactly). */
export function buildSplitToningUniformData(
  /** @type {ReturnType<typeof getSplitToning>} */ st,
) {
  return new Float32Array([
    st.shadows.hue, st.shadows.saturation,
    st.highlights.hue, st.highlights.saturation,
    st.balance, 0, 0, 0,
  ]);
}

// M3 Slice 5/6: masks are id-keyed, multi-instance ops -- unlike the global
// sliders above (opValue/upsertOp's find-by-name-and-replace model), there
// can be several masks (of possibly different kinds) on one image, so each
// needs a stable identity of its own, not a shared name slot. Matches the
// WGSL shader's fixed-size array (DevelopCanvas.svelte), which the UI must
// cap at -- a combined cap across every kind, not per-kind, since it's a
// hardware/uniform-array-size limit.
export const MAX_MASKS = 8;

// An explicit allowlist, not an implicit naming convention (e.g.
// `endsWith("_mask")`) -- simpler and more robust, since it can't silently
// misclassify some future non-mask op that happens to share the suffix.
const MASK_OP_NAMES = [
  "linear_gradient_mask",
  "radial_gradient_mask",
  "brush_mask",
  "luminance_range_mask",
  "color_range_mask",
];

// Mask kinds with no on-canvas geometry to show (brush's painted region,
// luminance range's pixel-value-based selection) get a toggleable colored
// overlay -- linear/radial keep their existing dashed-outline-only
// feedback instead, a deliberate scope decision from the overlay slice
// (PROGRESS.md), preserved here as the single place this list lives so the
// hotkey gate (+page.svelte) and the checkbox gate (MaskEditorPanel.svelte)
// can't drift apart.
export const OVERLAY_CAPABLE_MASK_OPS = ["brush_mask", "luminance_range_mask", "color_range_mask"];

/** @returns {LinearGradientMask} */
export function createLinearGradientMask(
  /** @type {{x: number, y: number}} */ start,
  /** @type {{x: number, y: number}} */ end,
) {
  return {
    op: "linear_gradient_mask",
    id: crypto.randomUUID(),
    start,
    end,
    feather: 0,
    invert: false,
    exposure: 0,
    contrast: 0,
    saturation: 0,
  };
}

/** M3 Slice 6: default feather=50 (not 0 like linear's default) and
 * invert=false meaning the effect applies OUTSIDE the ellipse -- both
 * deliberately match real Lightroom's own Radial Filter convention (its
 * classic vignette use case; checking Invert flips it to the
 * spotlight/subject-enhancement inside application).
 * @returns {RadialGradientMask} */
export function createRadialGradientMask(
  /** @type {{x: number, y: number}} */ center,
  /** @type {number} */ radiusX,
  /** @type {number} */ radiusY,
) {
  return {
    op: "radial_gradient_mask",
    id: crypto.randomUUID(),
    center,
    radiusX,
    radiusY,
    feather: 50,
    invert: false,
    exposure: 0,
    contrast: 0,
    saturation: 0,
  };
}

/** M3 Slice 7: no mask-level `feather` (unlike linear/radial) -- softness
 * is baked per-dab at paint time from the current tool setting (real
 * Lightroom's own brush-options model: Size/Feather/Flow apply to
 * whatever gets painted NEXT, not editable globally after the fact).
 * Accepts an optional `id` -- DevelopCanvas.svelte generates the id itself
 * (needed synchronously, before this creation call round-trips back down
 * as a prop) and passes it through, rather than discarding a locally-
 * generated one here.
 * @returns {BrushMask} */
export function createBrushMask(/** @type {string=} */ id) {
  return {
    op: "brush_mask",
    id: id ?? crypto.randomUUID(),
    dabs: [],
    invert: false,
    exposure: 0,
    contrast: 0,
    saturation: 0,
  };
}

/** Defaults (30/70/20) select midtones out of the box -- gives the user
 * something visible to tune immediately, rather than an empty or
 * full-frame selection. No canvas interaction needed to create this kind
 * (see DevelopCanvas.svelte/MaskToolStrip.svelte -- it's created directly
 * on tool-button click, real Lightroom's own behavior for this mask kind).
 * @returns {LuminanceRangeMask} */
export function createLuminanceRangeMask() {
  return {
    op: "luminance_range_mask",
    id: crypto.randomUUID(),
    rangeMin: 30,
    rangeMax: 70,
    feather: 20,
    invert: false,
    exposure: 0,
    contrast: 0,
    saturation: 0,
  };
}

/** M3 Slice 8: created from a single click-to-sample on the canvas (see
 * DevelopCanvas.svelte's colorRangeClickStart handling) -- unlike
 * luminance range's no-canvas-interaction creation, this genuinely needs
 * the sampled pixel color, which only the canvas can produce, so the
 * caller passes it in rather than this factory picking a default.
 * Defaults (25/20) select a moderate, immediately-tunable range around
 * the sampled color, same "give the user something visible to tune
 * immediately" reasoning as luminance range's own 30/70/20 defaults.
 * @returns {ColorRangeMask} */
export function createColorRangeMask(/** @type {{r: number, g: number, b: number}} */ refColor) {
  return {
    op: "color_range_mask",
    id: crypto.randomUUID(),
    refColor,
    range: 25,
    feather: 20,
    invert: false,
    exposure: 0,
    contrast: 0,
    saturation: 0,
  };
}

/** @returns {Mask[]} */
export function listMasks(/** @type {EditStack} */ stack) {
  return /** @type {any} */ (stack.ops.filter((o) => MASK_OP_NAMES.includes(o.op)));
}

/** @returns {EditStack} */
export function addMask(/** @type {EditStack} */ stack, /** @type {Mask} */ mask) {
  return { ...stack, ops: [...stack.ops, mask] };
}

/** @returns {EditStack} */
export function updateMask(
  /** @type {EditStack} */ stack,
  /** @type {string} */ id,
  /** @type {Partial<Mask>} */ patch,
) {
  const ops = stack.ops.map((o) =>
    MASK_OP_NAMES.includes(o.op) && /** @type {any} */ (o).id === id
      ? /** @type {any} */ ({ ...o, ...patch })
      : o,
  );
  return { ...stack, ops };
}

/** @returns {EditStack} */
export function removeMask(/** @type {EditStack} */ stack, /** @type {string} */ id) {
  const ops = stack.ops.filter((o) => !(MASK_OP_NAMES.includes(o.op) && /** @type {any} */ (o).id === id));
  return { ...stack, ops };
}
