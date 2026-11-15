// Model editions in the UI (RFC-0038 slice 4, ADR-0012). A build's edition may leave a model out; the backend then reports
// the state "not_in_edition" and offers nothing. Every place that used to treat "not ready" as "offer the download" asks
// modelGate instead, so a new state can never open a download dialog by accident.

/** Whether a model status lets the feature run, needs a download (or import) first, or is not part of this build.
 * An unknown state counts as excluded: offering a download for something we do not understand is the wrong default.
 * @param {{ state?: string } | null | undefined} status
 * @returns {"ready" | "download" | "excluded"} */
export function modelGate(status) {
  const state = status?.state;
  if (state === "ready") return "ready";
  if (state === "missing") return "download";
  return "excluded";
}

/** The one sentence shown where the feature would have been. No reason is given. @param {string} feature */
export function notInEdition(feature) {
  return `${feature} is not included in this edition of Emulsion.`;
}

/** What a person calls the feature each model family drives, in the order Settings > About lists them. The names are
 * the ones the "not included" sentences already use where the feature itself says so. */
const FAMILY_FEATURE = /** @type {const} */ ([
  ["denoise", "AI Denoise"],
  ["segment", "Select Subject"],
  ["detect", "Subject detection (person preset)"],
  ["inpaint", "Remove"],
  ["upscale", "Enlarge (AI)"],
  ["blink", "Eye check"],
  ["face", "Face grouping"],
]);

/** The one line Settings > About shows when the running edition leaves features out, or null when it leaves none.
 * Reads the `model_edition` command's result. A failed or odd result, and a family this table does not know, give no
 * line rather than a wrong one.
 * @param {{ name?: string, excluded?: unknown } | null | undefined} info
 * @returns {string | null} */
export function editionAboutLine(info) {
  const excluded = Array.isArray(info?.excluded) ? info.excluded : [];
  const features = FAMILY_FEATURE.filter(([family]) => excluded.includes(family)).map(([, feature]) => feature);
  if (features.length === 0) return null;
  if (features.length === 1) return notInEdition(features[0]);
  const last = features[features.length - 1];
  return `${features.slice(0, -1).join(", ")} and ${last} are not included in this edition of Emulsion.`;
}
