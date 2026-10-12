// Auto Sync's pure part (RFC-0028 §3.5): which ops of the open photo changed between two stacks, and are
// allowed to travel to the other selected photos. No state, no IPC: the action module wires it up.

import { PRESET_EXCLUDED_OP_NAMES } from "$lib/api/develop.js";

/** Ops that never auto-sync: the ones Copy Settings and presets already leave out (crop, perspective, lens
 * profile, masks and their local adjustments, spot / red-eye, `ai_denoise`: tied to one photo's pixels or
 * to its own denoised copy), plus `panel_hidden` (a view setting of the open photo, not an edit). They
 * can still be synced on purpose with *Sync Settings…*, whose dialog is group-based. */
export const AUTO_SYNC_EXCLUDED_OP_NAMES = Object.freeze([...PRESET_EXCLUDED_OP_NAMES, "panel_hidden"]);

/** Key-order-independent JSON, so two ops with the same content compare equal. @param {any} value @returns {string} */
function canonical(value) {
  if (Array.isArray(value)) return `[${value.map(canonical).join(",")}]`;
  if (value !== null && typeof value === "object") {
    return `{${Object.keys(value).sort().map((k) => `${JSON.stringify(k)}:${canonical(value[k])}`).join(",")}}`;
  }
  return JSON.stringify(value);
}

/** The syncable ops that differ between two stacks. An op is one unit identified by its name -- a tone curve
 * or the HSL bands count as one op, so a single moved point syncs the whole curve, never a partial list.
 * `ops` are the added or changed ops of `next` (absolute values); `removed` are names present in `previous`
 * and gone from `next` (Reset, a panel's reset). Unchanged ops and excluded ones are in neither.
 * @param {import('$lib/api/develop.js').EditStack} previous
 * @param {import('$lib/api/develop.js').EditStack} next
 * @returns {{ ops: import('$lib/api/develop.js').EditStack['ops'], removed: string[] }} */
export function diffSyncableOps(previous, next) {
  const syncable = /** @param {{ op: string }} o */ (o) => !AUTO_SYNC_EXCLUDED_OP_NAMES.includes(o.op);
  const before = new Map(previous.ops.filter(syncable).map((o) => [o.op, canonical(o)]));
  const after = new Map(next.ops.filter(syncable).map((o) => [o.op, o]));
  const ops = [...after.values()].filter((o) => before.get(o.op) !== canonical(o));
  const removed = [...before.keys()].filter((name) => !after.has(name));
  return { ops, removed };
}
