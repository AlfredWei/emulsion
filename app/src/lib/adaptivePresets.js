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

/** Amount (0-100 %) is applied once, here: the numbers written are the real ones the mask panel shows. Exact for
 * every pointwise control and Texture, which are linear in strength (RFC-0034 §3.2).
 * @param {Record<string, number>} adjustments @param {number} amount */
export function scaleAdjustments(adjustments, amount) {
  /** @type {Record<string, number>} */
  const out = {};
  for (const [k, v] of Object.entries(adjustments)) out[k] = Math.round(v * (amount / 100) * 100) / 100;
  return out;
}

/** What identifies a mask as the product of a rule, and what it was when written (to tell an edited one later). */
const signature = (/** @type {any} */ m, /** @type {string[]} */ keys) => JSON.stringify(keys.map((k) => m[k] ?? 0));

/** @param {AdaptiveRule} rule @param {import('$lib/api/adaptive.js').AdaptiveRegionResult} result @param {number} amount
 * @param {string} presetId @param {number} ruleIndex */
export function buildMask(rule, result, amount, presetId, ruleIndex) {
  const adjustments = scaleAdjustments(rule.adjustments, amount);
  const mask = { ...(result.mask ?? {}), id: crypto.randomUUID(), ...adjustments, ...(rule.feather !== undefined ? { feather: rule.feather } : {}) };
  return { ...mask, source: { preset: presetId, rule: ruleIndex, sig: signature(mask, Object.keys(adjustments)), keys: Object.keys(adjustments) } };
}

/** A mask the user changed since the preset wrote it (an adjustment moved). */
export function isEdited(/** @type {any} */ mask) {
  const s = mask.source;
  return !!s && signature(mask, s.keys) !== s.sig;
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
 * Never exceeds `MAX_MASKS` (a rule that would is skipped and says so). Amount 0 changes nothing.
 * @param {import('$lib/api/develop.js').EditStack} stack @param {AdaptivePreset} preset
 * @param {import('$lib/api/adaptive.js').AdaptiveRegionResult[]} results one per rule, in order @param {number} amount
 * @returns {{ stack: import('$lib/api/develop.js').EditStack, report: RuleReport[], changed: boolean }}
 */
export function planAdaptiveApply(stack, preset, results, amount) {
  const report = /** @type {RuleReport[]} */ (preset.rules.map((rule, i) => ({ region: rule.region, outcome: results[i].outcome, coverage: results[i].coverage, reason: results[i].reason, action: "skipped" })));
  if (amount <= 0) {
    report.forEach((r) => (r.why = "Amount is 0"));
    return { stack, report, changed: false };
  }
  let next = stack;
  let changed = false;
  preset.rules.forEach((rule, i) => {
    const result = results[i];
    if (result.outcome === "not_found" || !result.mask) return;
    const earlier = /** @type {any[]} */ (listMasks(next)).filter((m) => m.source?.preset === preset.id && m.source?.rule === i);
    if (earlier.some(isEdited)) {
      report[i].why = "an earlier mask from this preset was edited; delete it to re-apply";
      return;
    }
    const without = earlier.length ? { ...next, ops: next.ops.filter((o) => !earlier.includes(/** @type {any} */ (o))) } : next;
    if (listMasks(without).length >= MAX_MASKS) {
      report[i].why = `the photo already has ${MAX_MASKS} masks`;
      return;
    }
    next = addMask(without, /** @type {any} */ (buildMask(rule, result, amount, preset.id, i)));
    report[i].action = earlier.length ? "replaced" : "added";
    changed = true;
  });
  return { stack: next, report, changed };
}

const REGION_LABEL = { sky: "Sky", face: "Faces", shadows: "Shadows" };

/** One line per rule for the notification (the editor UI of slice C will show the same data properly).
 * @param {RuleReport[]} report */
export function describeReport(report) {
  return report
    .map((r) => {
      const name = REGION_LABEL[/** @type {keyof typeof REGION_LABEL} */ (r.region)] ?? r.region;
      const pct = `${Math.round(r.coverage * 100)} % of the frame`;
      if (r.outcome === "not_found") return `${name}: not found${r.reason ? ` (${r.reason})` : ""}`;
      if (r.action === "skipped") return `${name}: found but not applied (${r.why ?? "unknown"})`;
      return r.outcome === "check" ? `${name}: ${pct}, check it${r.reason ? ` (${r.reason})` : ""}` : `${name}: ${pct}`;
    })
    .join("; ");
}

/** "Adaptive: Deepen sky 60 %", the History row of one apply (one undo step). */
export const adaptiveLabel = (/** @type {AdaptivePreset} */ preset, /** @type {number} */ amount) => `Adaptive: ${preset.name} ${amount} %`;
