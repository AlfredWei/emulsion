// Adaptive presets (RFC-0035 §3.1/§3.3): a preset whose adjustments are bound to a kind of region that is
// found per photograph. Pure functions: what a preset is, how a resolved region becomes a mask scaled by Amount,
// and how the masks join (or replace the earlier ones of) a stack. The resolving itself is Rust
// (`resolve_adaptive_region`) and the writing is `actions/adaptiveActions.js`.

import { MAX_MASKS, addMask, listMasks } from "$lib/api/develop.js";

/**
 * One rule of an adaptive preset: the region it is bound to and the mask adjustments (RFC-0034's controls and
 * Texture) applied inside it. `feather` overrides the resolver's own softness when set.
 * @typedef {Object} AdaptiveRule
 * @property {"sky" | "face" | "shadows"} region
 * @property {Record<string, number>} adjustments
 * @property {number=} feather
 *
 * @typedef {Object} AdaptivePreset
 * @property {string} id
 * @property {string} name
 * @property {AdaptiveRule[]} rules
 */

/** The starter presets (RFC-0033 §3.3), restrained start values to be judged on photographs, not derived values.
 * "Lift subject" and "Soften background" need the segmenter (a click, GPU only) and are not here (RFC-0035 §3.5).
 * @type {AdaptivePreset[]} */
export const ADAPTIVE_PRESETS = [
  { id: "deepen-sky", name: "Deepen sky", rules: [{ region: "sky", adjustments: { exposure: -0.3, contrast: 12, saturation: 10 } }] },
  { id: "brighten-faces", name: "Brighten faces", rules: [{ region: "face", adjustments: { exposure: 0.3, shadows: 12, temperature: 3 } }] },
  { id: "cool-shadows", name: "Cool the shadows", rules: [{ region: "shadows", adjustments: { temperature: -8, saturation: -5 } }] },
];

/** Shown greyed out in the section so the user knows they are coming: they need the segmenter (one click, GPU
 * only, RFC-0035 §3.5). Not appliable.
 * @type {{ id: string, name: string, region: string, needs: string }[]} */
export const ADAPTIVE_PLANNED = [
  { id: "separate-subject", name: "Separate the subject", region: "subject", needs: "Needs a click and GPU acceleration" },
  { id: "warm-person", name: "Warm the person", region: "person", needs: "Needs a click and GPU acceleration" },
];

/** Amount (0-100 %) is applied once, here: the numbers written are the real ones the mask panel shows. Exact for
 * every pointwise control and Texture, which are linear in strength (RFC-0034 §3.2).
 * @param {Record<string, number>} adjustments @param {number} amount */
export function scaleAdjustments(adjustments, amount) {
  /** @type {Record<string, number>} */
  const out = {};
  for (const [k, v] of Object.entries(adjustments)) out[k] = Math.round(v * (amount / 100) * 100) / 100;
  return out;
}

/** Key order does not matter: the catalog stores objects with sorted keys.
 * @param {any} v @returns {string} */
const canonical = (v) =>
  Array.isArray(v) ? `[${v.map(canonical).join(",")}]` : v && typeof v === "object" ? `{${Object.keys(v).sort().map((k) => `${JSON.stringify(k)}:${canonical(v[k])}`).join(",")}}` : JSON.stringify(v);

/** What the mask was when the preset wrote it: a hash of the WHOLE mask but its id and `source`, so an adjustment the
 * preset did not set, a modifier, a range or a brush stroke added later all count as the user's change.
 * @param {any} mask */
const signature = (mask) => {
  const { id: _id, source: _source, ...rest } = mask;
  const text = canonical(rest);
  let h = 2166136261;
  for (let i = 0; i < text.length; i++) h = Math.imul(h ^ text.charCodeAt(i), 16777619);
  return `${text.length}:${(h >>> 0).toString(16)}`;
};

/** @param {AdaptiveRule} rule @param {import('$lib/api/adaptive.js').AdaptiveRegionResult} result @param {number} amount
 * @param {string} presetId @param {number} ruleIndex */
export function buildMask(rule, result, amount, presetId, ruleIndex) {
  const adjustments = scaleAdjustments(rule.adjustments, amount);
  // The base three every mask carries: a preset that sets only some adjustments (shadows: temperature, saturation)
  // must not leave the panel reading `undefined.toFixed` the moment the mask is selected (a stack loaded from the
  // catalog gets these from Rust's defaults, this one never made the trip).
  const mask = { exposure: 0, contrast: 0, saturation: 0, ...(result.mask ?? {}), id: crypto.randomUUID(), ...adjustments, ...(rule.feather !== undefined ? { feather: rule.feather } : {}) };
  // `check` (the resolver's reason) marks a selection the user is asked to look at; the mask panel says so.
  const check = result.outcome === "check" ? { check: result.reason ?? "Look at this selection" } : {};
  return { ...mask, source: { preset: presetId, rule: ruleIndex, sig: signature(mask), ...check } };
}

/** The masks a preset wrote on this stack (they carry `source.preset`), in stack order.
 * @param {import('$lib/api/develop.js').EditStack} stack @param {string} presetId
 * @returns {any[]} */
export function presetMasks(stack, presetId) {
  return /** @type {any[]} */ (listMasks(stack)).filter((m) => m.source?.preset === presetId);
}

/** A mask the user changed since the preset wrote it (anything in it, not only the preset's own adjustments). */
export function isEdited(/** @type {any} */ mask) {
  const s = mask.source;
  return !!s && signature(mask) !== s.sig;
}

/**
 * @typedef {Object} RuleReport
 * @property {string} region
 * @property {import('$lib/api/adaptive.js').AdaptiveOutcome} outcome
 * @property {number} coverage
 * @property {string | null} reason
 * @property {"added" | "replaced" | "skipped"} action
 * @property {string=} why  - why a mask was not added although the region was found
 */

/**
 * Applies a resolved preset to a stack: for each rule `found` and `check` add a mask (in rule order, after the
 * existing ones), `not_found` adds nothing. A mask this preset wrote earlier (`source.preset`) is replaced unless
 * the user edited it, in which case it is kept and the rule is skipped (RFC-0035 §3.3.4; the confirm dialog is slice C).
 * `added` lists the new masks' ids in rule order (the caller selects the first, so the user sees what was found).
 * Never exceeds `MAX_MASKS` (a rule that would is skipped and says so). Amount 0 changes nothing.
 * `replaceEdited` is the user's answer to "Replace the mask you changed?": true replaces it too.
 * @param {import('$lib/api/develop.js').EditStack} stack @param {AdaptivePreset} preset
 * @param {import('$lib/api/adaptive.js').AdaptiveRegionResult[]} results one per rule, in order @param {number} amount
 * @param {boolean} [replaceEdited]
 * @returns {{ stack: import('$lib/api/develop.js').EditStack, report: RuleReport[], changed: boolean, added: string[] }}
 */
export function planAdaptiveApply(stack, preset, results, amount, replaceEdited = false) {
  const report = /** @type {RuleReport[]} */ (preset.rules.map((rule, i) => ({ region: rule.region, outcome: results[i].outcome, coverage: results[i].coverage, reason: results[i].reason, action: "skipped" })));
  if (amount <= 0) {
    report.forEach((r) => (r.why = "Amount is 0"));
    return { stack, report, changed: false, added: [] };
  }
  let next = stack;
  let changed = false;
  /** @type {string[]} */
  const added = [];
  preset.rules.forEach((rule, i) => {
    const result = results[i];
    if (result.outcome === "not_found" || !result.mask) return;
    const earlier = /** @type {any[]} */ (listMasks(next)).filter((m) => m.source?.preset === preset.id && m.source?.rule === i);
    if (!replaceEdited && earlier.some(isEdited)) {
      report[i].why = "an earlier mask from this preset was edited; confirm to replace it";
      return;
    }
    const without = earlier.length ? { ...next, ops: next.ops.filter((o) => !earlier.includes(/** @type {any} */ (o))) } : next;
    if (listMasks(without).length >= MAX_MASKS) {
      report[i].why = `the photo already has ${MAX_MASKS} masks`;
      return;
    }
    const mask = /** @type {any} */ (buildMask(rule, result, amount, preset.id, i));
    next = addMask(without, mask);
    added.push(mask.id);
    report[i].action = earlier.length ? "replaced" : "added";
    changed = true;
  });
  return { stack: next, report, changed, added };
}

const REGION_LABEL = { sky: "Sky", face: "Faces", shadows: "Shadows" };

/** One rule's line of the report: what was looked for, how it went, in words. `kind` picks the icon and colour.
 * @param {RuleReport} r
 * @returns {{ name: string, text: string, kind: "found" | "check" | "not_found" | "skipped" }} */
export function describeRule(r) {
  const name = REGION_LABEL[/** @type {keyof typeof REGION_LABEL} */ (r.region)] ?? r.region;
  const pct = `${Math.round(r.coverage * 100)} % of the frame`;
  if (r.outcome === "not_found") return { name, text: `not found${r.reason ? `: ${r.reason}` : ""}`, kind: "not_found" };
  if (r.action === "skipped") return { name, text: `found but not applied (${r.why ?? "unknown"})`, kind: "skipped" };
  if (r.outcome === "check") return { name, text: `${pct}${r.reason ? `, ${r.reason}` : ""}. Applied, check it`, kind: "check" };
  return { name, text: pct, kind: "found" };
}

/** The whole report on one line, for the notification.
 * @param {RuleReport[]} report */
export function describeReport(report) {
  return report
    .map((r) => {
      const d = describeRule(r);
      return `${d.name}: ${d.text}`;
    })
    .join("; ");
}

/**
 * One photograph's line in a batch report (RFC-0035 B3). Nothing is hidden: every photo of the selection has a row.
 * `applied`: masks added and nothing to look at. `check`: masks added and at least one is flagged. `nothing`: the region
 * was not found, nothing changed. `kept`: an earlier mask of this preset was changed by the user and was left alone.
 * `failed`: the photo could not be analysed or written (the reason is in `error`).
 * @typedef {Object} BatchRow
 * @property {number} versionId
 * @property {string} name
 * @property {"applied" | "check" | "nothing" | "kept" | "failed"} status
 * @property {RuleReport[]} report
 * @property {string=} error
 */

/** @param {RuleReport[]} report @returns {BatchRow["status"]} */
export function batchStatus(report) {
  const added = report.filter((r) => r.action !== "skipped");
  if (added.length === 0) return report.some((r) => r.outcome !== "not_found" && r.action === "skipped" && /edited/.test(r.why ?? "")) ? "kept" : "nothing";
  return added.some((r) => r.outcome === "check") ? "check" : "applied";
}

/** "Adaptive: Deepen sky 60 %", the History row of one apply (one undo step). */
export const adaptiveLabel = (/** @type {AdaptivePreset} */ preset, /** @type {number} */ amount) => `Adaptive: ${preset.name} ${amount} %`;
