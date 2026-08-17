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

/**
 * @typedef {Object} SpotMask
 * @property {"spot_mask"} op
 * @property {string} id
 * @property {"clone" | "heal"} mode
 * @property {{x: number, y: number}} dest - the destination circle's
 *   center (what the user sees/drags).
 * @property {number} radius - normalized fraction of image WIDTH, same
 *   single-scalar "true circle in pixel space" convention as a brush
 *   dab's own `radius` (see `Dab`'s own doc comment) -- NOT radiusX/
 *   radiusY like radial masks, since a spot is always a plain circle.
 * @property {number} feather - 0-100, same edge-softness scale as linear/
 *   radial's own `feather`.
 * @property {{x: number, y: number}} source - the content SOURCE
 *   circle's center, independently draggable from `dest` (same
 *   two-independent-points precedent as `LinearGradientMask`'s `start`/
 *   `end`). Auto-placed on creation, not user-chosen up front the way
 *   Photoshop's Option/Alt-click convention works -- see
 *   `createSpotMask`'s own doc comment.
 *
 * Deliberately has NO `invert`/`exposure`/`contrast`/`saturation` fields,
 * unlike every other mask kind: a spot mask doesn't gate a parametric
 * adjustment, it copies pixel CONTENT from `source` into `dest` -- there
 * is no "settings" channel to invert or dial in.
 */

/** @typedef {LinearGradientMask | RadialGradientMask | BrushMask | LuminanceRangeMask | ColorRangeMask | SpotMask} Mask */

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

/**
 * @typedef {Object} HistoryEntry
 * @property {number} id
 * @property {string} label
 * @property {string} created_at
 */

/**
 * @typedef {Object} SnapshotEntry
 * @property {number} id
 * @property {string} name
 * @property {string} created_at
 */

/** @returns {Promise<EditStack>} */
export function getEditStack(/** @type {number} */ versionId) {
  return invoke("get_edit_stack", { versionId });
}

/** `label` is the human-readable name for this edit ("Exposure", "Crop",
 * "Add Mask", ...) shown in the History panel -- omit it (or pass
 * `undefined`) for a flush with nothing new pending (switching images,
 * exporting, closing the window), which persists the stack without
 * creating a history row. See `Catalog::record_edit_stack`'s doc comment.
 * @returns {Promise<HistoryEntry[]>} the version's fresh history list */
export function setEditStack(
  /** @type {number} */ versionId,
  /** @type {EditStack} */ stack,
  /** @type {string=} */ label,
) {
  return invoke("set_edit_stack", { versionId, stack, label: label ?? null });
}

/** @returns {Promise<HistoryEntry[]>} */
export function getHistory(/** @type {number} */ versionId) {
  return invoke("get_history", { versionId });
}

/** Moves the live edit stack to a past history entry -- undo, redo, and
 * jump-to-entry in the History panel are all this same call; the caller
 * just picks which `historyId` to restore. Does not itself create a new
 * history row.
 * @returns {Promise<EditStack>} */
export function restoreHistoryEntry(/** @type {number} */ versionId, /** @type {number} */ historyId) {
  return invoke("restore_history_entry", { versionId, historyId });
}

/** @returns {Promise<SnapshotEntry>} */
export function addSnapshot(/** @type {number} */ versionId, /** @type {string} */ name) {
  return invoke("add_snapshot", { versionId, name });
}

/** @returns {Promise<SnapshotEntry[]>} */
export function getSnapshots(/** @type {number} */ versionId) {
  return invoke("get_snapshots", { versionId });
}

/** Restoring a snapshot IS a new, undoable edit (unlike
 * `restoreHistoryEntry`) -- it appends its own "Restore Snapshot: {name}"
 * history row, included in the returned list.
 * @returns {Promise<[EditStack, HistoryEntry[]]>} */
export function restoreSnapshot(/** @type {number} */ versionId, /** @type {number} */ snapshotId) {
  return invoke("restore_snapshot", { versionId, snapshotId });
}

/** @returns {Promise<void>} */
export function deleteSnapshot(/** @type {number} */ versionId, /** @type {number} */ snapshotId) {
  return invoke("delete_snapshot", { versionId, snapshotId });
}

/**
 * @typedef {Object} PresetEntry
 * @property {number} id
 * @property {string} name
 * @property {EditStack} edit_stack
 * @property {string} created_at
 */

/** Keeps only the preset-eligible ops from an edit stack (see
 * `PRESET_EXCLUDED_OP_NAMES`'s own doc comment for why crop/masks are
 * excluded) -- the ONLY place this filtering happens for the "Save
 * Current as Preset" path. Also re-applied defensively when importing a
 * preset file, since a hand-edited or foreign file could contain a
 * crop/mask op that would otherwise sail through undetected (Rust's
 * `import_preset_file` command does no filtering of its own -- it just
 * reads+parses the file, on the same "Rust never interprets ops"
 * boundary every edit-stack command keeps).
 * @returns {EditStack} */
export function presetEligibleOps(/** @type {EditStack} */ stack) {
  return { schema_version: stack.schema_version, ops: stack.ops.filter((o) => !PRESET_EXCLUDED_OP_NAMES.includes(o.op)) };
}

/** Applies a preset's ops onto a target edit stack -- a per-op upsert by
 * `op` name (same filter-then-push idiom `upsertOp`/`upsertToneCurve`/etc.
 * already use individually, generalized here since a preset's ops are
 * name-only and don't need per-op semantic knowledge to merge). Crop and
 * every mask on `target` are left completely untouched, since presets
 * never carry those op names in the first place (see `presetEligibleOps`).
 *
 * KNOWN LIMITATION, surfaced in the Presets UI: each preset op REPLACES
 * the matching op on `target` wholesale, not a partial/per-field merge.
 * For `hsl`/`split_toning`/`tone_curve` specifically, this means applying
 * a preset that touches even one HSL band replaces the target's ENTIRE
 * `hsl` op (all 8 bands) -- any of the target's own HSL/Split Toning/Tone
 * Curve adjustments on fields the preset itself left at identity are
 * silently zeroed, not preserved. A real, accepted v1 scope cut (per-field
 * merging would need to know each op's own field structure, breaking the
 * "merge is name-only, no semantic knowledge needed" simplicity this
 * function relies on) -- not a bug, but real enough to warn about in the UI
 * rather than let a user discover it as unexplained data loss.
 * @returns {EditStack} */
export function applyPresetOps(/** @type {EditStack} */ target, /** @type {EditStack} */ presetStack) {
  let ops = target.ops;
  for (const presetOp of presetStack.ops) {
    ops = [...ops.filter((o) => o.op !== presetOp.op), presetOp];
  }
  return { schema_version: target.schema_version, ops };
}

/** @returns {Promise<PresetEntry>} */
export function createPreset(/** @type {string} */ name, /** @type {EditStack} */ stack) {
  return invoke("create_preset", { name, stack });
}

/** @returns {Promise<PresetEntry[]>} */
export function listPresets() {
  return invoke("list_presets");
}

/** @returns {Promise<void>} */
export function deletePreset(/** @type {number} */ presetId) {
  return invoke("delete_preset", { presetId });
}

/** Raw file read + parse ONLY -- does not filter ops or write to the
 * catalog. Callers MUST run the result's `ops` through `presetEligibleOps`
 * before ever calling `createPreset` with it (see that function's own doc
 * comment for why).
 * @returns {Promise<{name: string, schema_version: number, ops: Array<EditOp | Mask>}>} */
export function importPresetFile(/** @type {string} */ path) {
  return invoke("import_preset_file", { path });
}

/** @returns {Promise<void>} */
export function exportPresetFile(
  /** @type {string} */ name,
  /** @type {EditStack} */ stack,
  /** @type {string} */ path,
) {
  return invoke("export_preset_file", { name, stack, path });
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

/** Reverts every adjustment, tool op, and mask back to default in one
 * shot -- clears the ops array entirely, keeping schema_version. Real
 * Lightroom's own Develop "Reset": a full revert to as-shot, not a
 * per-section reset. Every section's own getter already falls back to its
 * own identity default when its op is absent, so nothing else needs to
 * change for this to take full effect.
 * @returns {EditStack} */
export function resetEditStack(/** @type {EditStack} */ stack) {
  return { ...stack, ops: [] };
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

// Vignette (M3): a flat, non-nested payload -- unlike Split Toning's
// per-zone shape above, there's no natural per-element repetition in
// three named fields, so this mirrors upsertSplitToningBalance's flat
// patch-and-replace shape rather than upsertSplitToningZone's per-zone
// one. Global-only (applied after Dehaze, before any mask -- see
// develop_engine.rs/DevelopCanvas.svelte's shared pipeline-order
// comment).
export const IDENTITY_VIGNETTE = Object.freeze({ amount: 0, midpoint: 50, feather: 50 });

/** @returns {{amount: number, midpoint: number, feather: number}} */
export function getVignette(
  /** @type {EditStack} */ stack,
  /** @type {typeof IDENTITY_VIGNETTE} */ fallback = IDENTITY_VIGNETTE,
) {
  const op = /** @type {any} */ (stack.ops.find((o) => o.op === "vignette"));
  if (!op) return fallback;
  return {
    amount: op.amount ?? 0,
    midpoint: op.midpoint ?? 50,
    feather: op.feather ?? 50,
  };
}

/** Patches any subset of {amount, midpoint, feather}, leaving the rest
 * untouched.
 * @returns {EditStack} */
export function upsertVignette(
  /** @type {EditStack} */ stack,
  /** @type {Partial<{amount: number, midpoint: number, feather: number}>} */ patch,
) {
  const current = getVignette(stack);
  const next = { ...current, ...patch };
  const ops = stack.ops.filter((o) => o.op !== "vignette");
  ops.push(/** @type {any} */ ({ op: "vignette", ...next }));
  return { ...stack, ops };
}

/** Packs into the exact Float32Array layout DevelopCanvas.svelte's
 * `vignetteBuffer`/the WGSL `Vignette` struct expects (field order
 * matters -- must match the struct's own field order exactly). */
export function buildVignetteUniformData(
  /** @type {ReturnType<typeof getVignette>} */ v,
) {
  return new Float32Array([v.amount, v.midpoint, v.feather, 0]);
}

// Grain (M3): same flat, non-nested payload shape as Vignette above.
// Global-only, applied after Vignette, before any mask (matching real
// Lightroom's own Effects-panel order). Defaults match real Lightroom's
// own Grain defaults exactly (Amount 0 -- off, Size 25, Roughness 50).
export const IDENTITY_GRAIN = Object.freeze({ amount: 0, size: 25, roughness: 50 });

/** @returns {{amount: number, size: number, roughness: number}} */
export function getGrain(
  /** @type {EditStack} */ stack,
  /** @type {typeof IDENTITY_GRAIN} */ fallback = IDENTITY_GRAIN,
) {
  const op = /** @type {any} */ (stack.ops.find((o) => o.op === "grain"));
  if (!op) return fallback;
  return {
    amount: op.amount ?? 0,
    size: op.size ?? 25,
    roughness: op.roughness ?? 50,
  };
}

/** Patches any subset of {amount, size, roughness}, leaving the rest
 * untouched.
 * @returns {EditStack} */
export function upsertGrain(
  /** @type {EditStack} */ stack,
  /** @type {Partial<{amount: number, size: number, roughness: number}>} */ patch,
) {
  const current = getGrain(stack);
  const next = { ...current, ...patch };
  const ops = stack.ops.filter((o) => o.op !== "grain");
  ops.push(/** @type {any} */ ({ op: "grain", ...next }));
  return { ...stack, ops };
}

/** Packs into the exact Float32Array layout DevelopCanvas.svelte's
 * `grainBuffer`/the WGSL `Grain` struct expects (field order matters --
 * must match the struct's own field order exactly). */
export function buildGrainUniformData(
  /** @type {ReturnType<typeof getGrain>} */ g,
) {
  return new Float32Array([g.amount, g.size, g.roughness, 0]);
}

// Lens Corrections (M3): the one op in this file with a nested `profile`
// payload -- see develop_engine.rs's own header comment on this op for
// the full Profile/Manual split rationale (real Lightroom's own tab
// split; manual vignette is deliberately NOT a separate control, since
// it's the same radial-gain formula the existing `vignette` op above
// already exposes). `profile` is baked in a SEPARATE step
// (`setLensProfile`, called once per Develop-open from a
// `lookup_lens_profile` Tauri result) rather than through slider
// upserts -- it's resolved equipment data, not a user-dragged value.
// Amounts default to 100 (not 0), matching develop_engine.rs's own
// `LensCorrection::default()` and its documented reasoning: once a
// profile is enabled, real Lightroom's sliders start at full correction.
/** @typedef {Object} LensCorrectionState
 * @property {boolean} profile_enabled
 * @property {number} distortion_amount
 * @property {number} vignette_amount
 * @property {number} ca_amount
 * @property {number} manual_distortion
 * @property {number} manual_ca
 * @property {LensProfileMatch | null} profile
 */

export const IDENTITY_LENS_CORRECTION = /** @type {LensCorrectionState} */ (
  Object.freeze({
    profile_enabled: false,
    distortion_amount: 100,
    vignette_amount: 100,
    ca_amount: 100,
    manual_distortion: 0,
    manual_ca: 0,
    profile: null,
  })
);

/** @returns {LensCorrectionState} */
export function getLensCorrection(
  /** @type {EditStack} */ stack,
  /** @type {LensCorrectionState} */ fallback = IDENTITY_LENS_CORRECTION,
) {
  const op = /** @type {any} */ (stack.ops.find((o) => o.op === "lens_correction"));
  if (!op) return fallback;
  return {
    profile_enabled: op.profile_enabled ?? false,
    distortion_amount: op.distortion_amount ?? 100,
    vignette_amount: op.vignette_amount ?? 100,
    ca_amount: op.ca_amount ?? 100,
    manual_distortion: op.manual_distortion ?? 0,
    manual_ca: op.manual_ca ?? 0,
    profile: op.profile ?? null,
  };
}

/** Patches any subset of the flat fields (profile_enabled/amounts/manual
 * sliders), leaving `profile` and everything else untouched -- use
 * `setLensProfile` to replace the baked profile block itself.
 * @returns {EditStack} */
export function upsertLensCorrection(
  /** @type {EditStack} */ stack,
  /** @type {Partial<LensCorrectionState>} */ patch,
) {
  const current = getLensCorrection(stack);
  const next = { ...current, ...patch };
  const ops = stack.ops.filter((o) => o.op !== "lens_correction");
  ops.push(/** @type {any} */ ({ op: "lens_correction", ...next }));
  return { ...stack, ops };
}

/** Bakes (or clears, via `null`) the matched camera+lens profile into the
 * op -- the ONLY way `profile` itself changes; every other lens-
 * correction field goes through `upsertLensCorrection` instead.
 * @returns {EditStack} */
export function setLensProfile(
  /** @type {EditStack} */ stack,
  /** @type {LensProfileMatch | null} */ profile,
) {
  return upsertLensCorrection(stack, { profile });
}

/** Packs into the exact Float32Array layout DevelopCanvas.svelte's
 * `lensCorrectionBuffer`/the WGSL `LensCorrectionParams` struct expects
 * (field order matters -- must match the struct's own field order
 * exactly). A missing/null `profile` or a missing distortion/tca/
 * vignetting sub-block naturally uploads as "model 0" / all-zero
 * coefficients, which the shader treats as a no-op -- there's no
 * separate "has a profile" flag to keep in sync. */
export function buildLensCorrectionUniformData(
  /** @type {ReturnType<typeof getLensCorrection>} */ lc,
) {
  const p = lc.profile;
  const d = /** @type {any} */ (p?.distortion ?? null);
  const t = /** @type {any} */ (p?.tca ?? null);
  const v = p?.vignetting ?? null;

  const distortionModel = !d ? 0 : d.model === "poly3" ? 1 : d.model === "poly5" ? 2 : d.model === "ptlens" ? 3 : 0;
  const [dc0, dc1, dc2] = !d
    ? [0, 0, 0]
    : d.model === "poly3"
      ? [d.k1, 0, 0]
      : d.model === "poly5"
        ? [d.k1, d.k2, 0]
        : [d.a, d.b, d.c]; // ptlens

  const tcaModel = !t ? 0 : t.model === "linear" ? 1 : t.model === "poly3" ? 2 : 0;
  const [tr0, tr1, tr2, tb0, tb1, tb2] = !t
    ? [0, 0, 0, 0, 0, 0]
    : t.model === "linear"
      ? [t.kr, 0, 0, t.kb, 0, 0]
      : [t.red[0], t.red[1], t.red[2], t.blue[0], t.blue[1], t.blue[2]]; // poly3

  return new Float32Array([
    lc.profile_enabled ? 1 : 0,
    lc.distortion_amount,
    lc.vignette_amount,
    lc.ca_amount,
    lc.manual_distortion,
    lc.manual_ca,
    p?.crop_factor ?? 1,
    p?.real_focal ?? 50,
    p?.lens_center_x ?? 0,
    p?.lens_center_y ?? 0,
    distortionModel,
    dc0,
    dc1,
    dc2,
    tcaModel,
    tr0,
    tr1,
    tr2,
    tb0,
    tb1,
    tb2,
    v?.k1 ?? 0,
    v?.k2 ?? 0,
    v?.k3 ?? 0,
  ]);
}

/** @typedef {Object} LensProfileMatch
 * @property {string} camera
 * @property {string} lens
 * @property {number} crop_factor
 * @property {number} real_focal
 * @property {number} lens_center_x
 * @property {number} lens_center_y
 * @property {({model: "poly3", k1: number} | {model: "poly5", k1: number, k2: number} | {model: "ptlens", a: number, b: number, c: number}) | null} distortion
 * @property {({model: "linear", kr: number, kb: number} | {model: "poly3", red: [number, number, number], blue: [number, number, number]}) | null} tca
 * @property {{k1: number, k2: number, k3: number} | null} vignetting
 */

/** `null` when no camera+lens match is found in the bundled lensfun
 * database (missing/unrecognized EXIF, or genuinely no profile for that
 * equipment) -- callers show "no matching profile" rather than treating
 * this as an error.
 * @returns {Promise<LensProfileMatch | null>} */
export function lookupLensProfile(
  /** @type {{cameraMake?: string | null, cameraModel?: string | null, lensModel?: string | null, focalLength?: number | null, aperture?: number | null}} */ params,
) {
  return invoke("lookup_lens_profile", {
    cameraMake: params.cameraMake ?? null,
    cameraModel: params.cameraModel ?? null,
    lensModel: params.lensModel ?? null,
    focalLength: params.focalLength ?? null,
    aperture: params.aperture ?? null,
  });
}

// Sharpening (M3): a flat, non-nested payload -- same shape as Vignette/
// Grain above. Global-only, applied after Noise Reduction, before
// Vignette/Grain (see develop_engine.rs/DevelopCanvas.svelte's shared
// pipeline-order comment). Defaults chosen so turning Amount up alone
// produces a sensible result without also needing to tune Radius/Detail
// first (Amount 0 -- off, matching every other amount-style op's own
// identity-at-zero contract).
export const IDENTITY_SHARPEN = Object.freeze({ amount: 0, radius: 25, detail: 50, masking: 0 });

/** @returns {{amount: number, radius: number, detail: number, masking: number}} */
export function getSharpen(
  /** @type {EditStack} */ stack,
  /** @type {typeof IDENTITY_SHARPEN} */ fallback = IDENTITY_SHARPEN,
) {
  const op = /** @type {any} */ (stack.ops.find((o) => o.op === "sharpen"));
  if (!op) return fallback;
  return {
    amount: op.amount ?? 0,
    radius: op.radius ?? 25,
    detail: op.detail ?? 50,
    masking: op.masking ?? 0,
  };
}

/** Patches any subset of {amount, radius, detail, masking}, leaving the
 * rest untouched.
 * @returns {EditStack} */
export function upsertSharpen(
  /** @type {EditStack} */ stack,
  /** @type {Partial<{amount: number, radius: number, detail: number, masking: number}>} */ patch,
) {
  const current = getSharpen(stack);
  const next = { ...current, ...patch };
  const ops = stack.ops.filter((o) => o.op !== "sharpen");
  ops.push(/** @type {any} */ ({ op: "sharpen", ...next }));
  return { ...stack, ops };
}

/** Packs into the exact Float32Array layout DevelopCanvas.svelte's
 * `sharpenBuffer`/the WGSL `SharpenParams` struct expects (field order
 * matters). */
export function buildSharpenUniformData(
  /** @type {ReturnType<typeof getSharpen>} */ s,
) {
  return new Float32Array([s.amount, s.radius, s.detail, s.masking]);
}

// Luminance Noise Reduction (M3): same flat, non-nested payload shape.
export const IDENTITY_LUMA_NR = Object.freeze({ amount: 0, detail: 50, contrast: 0 });

/** @returns {{amount: number, detail: number, contrast: number}} */
export function getLumaNr(
  /** @type {EditStack} */ stack,
  /** @type {typeof IDENTITY_LUMA_NR} */ fallback = IDENTITY_LUMA_NR,
) {
  const op = /** @type {any} */ (stack.ops.find((o) => o.op === "luma_nr"));
  if (!op) return fallback;
  return {
    amount: op.amount ?? 0,
    detail: op.detail ?? 50,
    contrast: op.contrast ?? 0,
  };
}

/** @returns {EditStack} */
export function upsertLumaNr(
  /** @type {EditStack} */ stack,
  /** @type {Partial<{amount: number, detail: number, contrast: number}>} */ patch,
) {
  const current = getLumaNr(stack);
  const next = { ...current, ...patch };
  const ops = stack.ops.filter((o) => o.op !== "luma_nr");
  ops.push(/** @type {any} */ ({ op: "luma_nr", ...next }));
  return { ...stack, ops };
}

/** Packs into the exact Float32Array layout DevelopCanvas.svelte's
 * `lumaNRBuffer`/the WGSL `LumaNrParams` struct expects. */
export function buildLumaNrUniformData(
  /** @type {ReturnType<typeof getLumaNr>} */ n,
) {
  return new Float32Array([n.amount, n.detail, n.contrast, 0]);
}

// Color Noise Reduction (M3): same flat, non-nested payload shape.
export const IDENTITY_COLOR_NR = Object.freeze({ amount: 0, detail: 50 });

/** @returns {{amount: number, detail: number}} */
export function getColorNr(
  /** @type {EditStack} */ stack,
  /** @type {typeof IDENTITY_COLOR_NR} */ fallback = IDENTITY_COLOR_NR,
) {
  const op = /** @type {any} */ (stack.ops.find((o) => o.op === "color_nr"));
  if (!op) return fallback;
  return {
    amount: op.amount ?? 0,
    detail: op.detail ?? 50,
  };
}

/** @returns {EditStack} */
export function upsertColorNr(
  /** @type {EditStack} */ stack,
  /** @type {Partial<{amount: number, detail: number}>} */ patch,
) {
  const current = getColorNr(stack);
  const next = { ...current, ...patch };
  const ops = stack.ops.filter((o) => o.op !== "color_nr");
  ops.push(/** @type {any} */ ({ op: "color_nr", ...next }));
  return { ...stack, ops };
}

/** Packs into the exact Float32Array layout DevelopCanvas.svelte's
 * `colorNRBuffer`/the WGSL `ColorNrParams` struct expects. */
export function buildColorNrUniformData(
  /** @type {ReturnType<typeof getColorNr>} */ n,
) {
  return new Float32Array([n.amount, n.detail, 0, 0]);
}

// Crop & Straighten (M3): a flat, non-nested payload -- same shape as
// every other structured op above. UNLIKE every other op, this has no
// WGSL uniform/buffer twin at all -- it's deliberately kept out of the
// GPU pipeline entirely (see DevelopCanvas.svelte's own doc comment on
// the committed-crop CSS preview for why: canvasEl.width/height never
// changing avoids an entire class of coordinate-system bugs a design
// review caught in an earlier GPU-pass-based draft). The REAL pixel-level
// rotate+crop only happens once, in Rust, shared by export and thumbnail
// regen (see develop_engine.rs's own `crop` module).
//
// `x`/`y`/`width`/`height` are normalized 0-1, defined in the SAME
// coordinate space as the canvas's own (unrotated) layout box -- CSS
// `transform: rotate()` doesn't affect an element's own layout geometry,
// only its visual rendering, so this coordinate space is deliberately
// invariant to whatever rotation is applied for preview/export purposes.
// `angle` is degrees, -45..45 (matching real Lightroom's own Straighten
// range), POSITIVE = CLOCKWISE (matching CSS's own `rotate(Ndeg)`
// convention exactly, so the live preview and the real Rust-rotated
// output agree on direction -- deliberately picked and hand-tested, not
// assumed, since a mismatched sign here would be an immediately obvious,
// embarrassing "image un-rotates on commit" bug).
export const IDENTITY_CROP = Object.freeze({ x: 0, y: 0, width: 1, height: 1, angle: 0 });

/** @returns {{x: number, y: number, width: number, height: number, angle: number}} */
export function getCrop(
  /** @type {EditStack} */ stack,
  /** @type {typeof IDENTITY_CROP} */ fallback = IDENTITY_CROP,
) {
  const op = /** @type {any} */ (stack.ops.find((o) => o.op === "crop"));
  if (!op) return fallback;
  return {
    x: op.x ?? 0,
    y: op.y ?? 0,
    width: op.width ?? 1,
    height: op.height ?? 1,
    angle: op.angle ?? 0,
  };
}

/** Patches any subset of {x, y, width, height, angle}, leaving the rest
 * untouched.
 * @returns {EditStack} */
export function upsertCrop(
  /** @type {EditStack} */ stack,
  /** @type {Partial<{x: number, y: number, width: number, height: number, angle: number}>} */ patch,
) {
  const current = getCrop(stack);
  const next = { ...current, ...patch };
  const ops = stack.ops.filter((o) => o.op !== "crop");
  ops.push(/** @type {any} */ ({ op: "crop", ...next }));
  return { ...stack, ops };
}

/** True when the crop op is at (or effectively at) its identity value --
 * used to skip the CSS committed-crop preview wrapper and the Rust
 * rotate+crop step entirely, matching every other op's "skip when
 * unused" discipline. A small epsilon guards against float drift from
 * repeated aspect-ratio-preset reshaping landing at e.g. 0.0000001
 * instead of exactly 0. */
export function isCropIdentity(/** @type {ReturnType<typeof getCrop>} */ crop) {
  const eps = 1e-6;
  return (
    Math.abs(crop.x) < eps &&
    Math.abs(crop.y) < eps &&
    Math.abs(crop.width - 1) < eps &&
    Math.abs(crop.height - 1) < eps &&
    Math.abs(crop.angle) < eps
  );
}

// Eyedropper pickers (M3): the canvas's existing click-to-sample gesture
// (DevelopCanvas.svelte's sampleSourcePixel, reused from the color-range
// mask feature) reports a raw {r,g,b} 0-1 float sample. Every one of the
// three sampling-based eyedroppers (Split Toning zone tint, HSL band
// identification, Tone Curve point x-value) needs SOME projection of that
// color into hue/saturation/lightness -- this is the one shared conversion,
// not three ad hoc ones.

function remEuclid(/** @type {number} */ x, /** @type {number} */ m) {
  const r = x % m;
  return r < 0 ? r + m : r;
}

/** RGB (0-1 floats) -> HSL, mirroring develop_engine.rs's `rgb_to_hsl`
 * exactly (same `(max+min)/2` lightness -- NOT this module's separate
 * perceptual-luma formula used elsewhere for global Saturation -- and the
 * same `rem_euclid` hue wraparound the WGSL shader's own `rgbToHsl` uses).
 * @returns {{h: number, s: number, l: number}} h in 0-360 deg, s/l in 0-1 */
export function rgbToHsl(/** @type {number} */ r, /** @type {number} */ g, /** @type {number} */ b) {
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const delta = max - min;
  const l = (max + min) / 2;
  if (delta === 0) return { h: 0, s: 0, l };
  const s = delta / (1 - Math.abs(2 * l - 1));
  let h;
  if (max === r) h = 60 * remEuclid((g - b) / delta, 6);
  else if (max === g) h = 60 * ((b - r) / delta + 2);
  else h = 60 * ((r - g) / delta + 4);
  return { h: remEuclid(h, 360), s, l };
}

/** Which HSL_BAND_NAMES entry a hue is angularly closest to -- for the HSL
 * eyedropper's band-jump navigation only, NOT the render path's
 * `hsl_band_weight`/`hueBandWeight` smooth-blend formula (a different
 * question: "how much should up to 2 adjacent bands blend" vs. this
 * function's "which single band wins"). Since HSL_BAND_CENTERS_DEG are
 * evenly spaced 45deg apart, nearest-by-circular-distance and
 * highest-blend-weight always pick the same band, so this is exact for a
 * single-winner answer, not an approximation of the render-path formula.
 * @returns {string} one of HSL_BAND_NAMES */
export function nearestHslBand(/** @type {number} */ hueDeg) {
  let bestIndex = 0;
  let bestDist = Infinity;
  for (let i = 0; i < HSL_BAND_CENTERS_DEG.length; i++) {
    const d = Math.abs(remEuclid(hueDeg - HSL_BAND_CENTERS_DEG[i] + 180, 360) - 180);
    if (d < bestDist) {
      bestDist = d;
      bestIndex = i;
    }
  }
  return HSL_BAND_NAMES[bestIndex];
}

// Minimum spacing (normalized 0-1 units) enforced between any two tone
// curve points' x -- guards against a degenerate/zero-width segment (a
// divide-by-zero in the secant-slope computation this module/
// develop_engine.rs/DevelopCanvas.svelte's WGSL shader all share). Lifted
// out of ToneCurveEditor.svelte's own former private MIN_X_GAP so both that
// component's click-on-graph gesture and the Tone Curve eyedropper's
// click-on-photo gesture enforce the identical rule from one place.
export const MIN_TONE_CURVE_X_GAP = 0.001;

/** Sort-insert (x,y) into a tone curve's point list. Returns `points`
 * UNCHANGED (the same array reference) if rejected -- already at
 * MAX_CURVE_POINTS, or within MIN_TONE_CURVE_X_GAP of an existing point's x
 * -- so callers can cheaply check `result === points` to know whether the
 * insert actually happened.
 * @returns {readonly {x: number, y: number}[]} */
export function insertToneCurvePoint(
  /** @type {readonly {x: number, y: number}[]} */ points,
  /** @type {number} */ x,
  /** @type {number} */ y,
) {
  const sorted = [...points].sort((a, b) => a.x - b.x);
  if (sorted.length >= MAX_CURVE_POINTS) return points;
  if (sorted.some((p) => Math.abs(p.x - x) < MIN_TONE_CURVE_X_GAP)) return points;
  return [...sorted, { x, y }].sort((a, b) => a.x - b.x);
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
  "spot_mask",
];

// Presets (M3): op names a preset must NEVER capture -- `crop` and every
// mask kind. `linear_gradient_mask`/`radial_gradient_mask`/`brush_mask`/
// `color_range_mask` carry per-image geometry or a sampled pixel value
// tied to the exact photo they were created on; `crop` is normalized
// geometry tied to that photo's own framing. `luminance_range_mask` is
// the one exception worth naming explicitly: it's a pure tone-threshold
// (rangeMin/rangeMax/feather) with no geometry or sampled-pixel data at
// all, actually as portable as the global ops below -- excluded anyway
// for v1 scope simplicity (one exclusion list, not "masks except this
// one"), not because it's tied to the source image like the other four.
export const PRESET_EXCLUDED_OP_NAMES = [...MASK_OP_NAMES, "crop", "lens_correction"];

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

/** M4 Slice 1 (Healing/Clone brush): created from a click-drag on the
 * canvas (see DevelopCanvas.svelte's spot mask pointer handling), which
 * supplies `dest` and `radius` directly -- this factory's own job is just
 * picking a `source`. Real Photoshop asks the user to Option/Alt-click a
 * source FIRST; this app instead auto-places one a fixed offset away
 * (`SPOT_SOURCE_OFFSET_FACTOR`x the diameter, horizontally, flipped to
 * the other side if that would land outside the frame) and leaves it
 * immediately draggable via its own handle -- a named, simpler scope cut
 * vs. Lightroom's own content-aware auto-placement heuristic. Defaults to
 * `mode: "heal"` (the more generally useful default over a plain "clone"
 * copy, matching Lightroom's own Spot Removal tool default).
 * @returns {SpotMask} */
export function createSpotMask(
  /** @type {{x: number, y: number}} */ dest,
  /** @type {number} */ radius,
) {
  const SPOT_SOURCE_OFFSET_FACTOR = 2.5;
  const offsetX = radius * 2 * SPOT_SOURCE_OFFSET_FACTOR;
  const sourceX = dest.x + offsetX + radius <= 1 ? dest.x + offsetX : dest.x - offsetX;
  return {
    op: "spot_mask",
    id: crypto.randomUUID(),
    mode: "heal",
    dest,
    radius,
    feather: 20,
    source: {
      x: Math.min(1, Math.max(0, sourceX)),
      y: dest.y,
    },
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
