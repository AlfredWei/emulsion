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
