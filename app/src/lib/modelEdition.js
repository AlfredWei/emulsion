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

/** What a person calls each feature a build can compile out (ADR-0011), listed after the model families. */
const COMPILED_FEATURE = /** @type {const} */ ([["lens_profiles", "Lens profile correction"]]);

/** @typedef {{ name?: string, excluded?: unknown, channel?: string, caps?: Partial<ChannelCaps>, compiled_out?: unknown }} EditionInfo */
/** @typedef {{ export_command_hook: boolean, model_downloads: boolean, custom_cache_dir: boolean }} ChannelCaps */

/** The one line Settings > About shows when the running edition leaves features out, or null when it leaves none.
 * Reads the `model_edition` command's result. A failed or odd result, and a family this table does not know, give no
 * line rather than a wrong one.
 * @param {EditionInfo | null | undefined} info
 * @returns {string | null} */
export function editionAboutLine(info) {
  const excluded = Array.isArray(info?.excluded) ? info.excluded : [];
  const compiledOut = Array.isArray(info?.compiled_out) ? info.compiled_out : [];
  const features = [
    ...FAMILY_FEATURE.filter(([family]) => excluded.includes(family)),
    ...COMPILED_FEATURE.filter(([feature]) => compiledOut.includes(feature)),
  ].map(([, feature]) => feature);
  if (features.length === 0) return null;
  if (features.length === 1) return notInEdition(features[0]);
  const last = features[features.length - 1];
  return `${features.slice(0, -1).join(", ")} and ${last} are not included in this edition of Emulsion.`;
}

/** Whether this build contains lens profile correction. A failed result counts as yes, which is what every build
 * before the channel flag did. @param {EditionInfo | null | undefined} info */
export function hasLensProfiles(info) {
  return !(Array.isArray(info?.compiled_out) && info.compiled_out.includes("lens_profiles"));
}

/** What this build's channel contains. A capability the result does not state counts as present, which is what every
 * build before the channel flag did; the Mac App Store build always states all three.
 * @param {EditionInfo | null | undefined} info
 * @returns {ChannelCaps} */
export function channelCaps(info) {
  const caps = info?.caps ?? {};
  return {
    export_command_hook: caps.export_command_hook !== false,
    model_downloads: caps.model_downloads !== false,
    custom_cache_dir: caps.custom_cache_dir !== false,
  };
}
