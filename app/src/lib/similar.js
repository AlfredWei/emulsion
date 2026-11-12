// Pure helpers for the Similar view (RFC-0031 slice C): turning the backend's groups (ids only) into what the grid and
// the group view show, and working out who "Reject the others" would reject. No state, no IPC.

/**
 * @typedef {Object} SimilarFrame
 * @property {import('$lib/api/catalog.js').ImageSummary} image
 * @property {number} relative - share of the group's best sharpness (1 = the best)
 * @property {boolean} soft
 * @property {boolean} best - the suggested frame
 */

/**
 * @typedef {Object} SimilarGroupView
 * @property {number} key - the first member's image id, stable while the group's members stay the same
 * @property {SimilarFrame[]} frames - chronological
 * @property {SimilarFrame | null} best
 */

/**
 * Joins the groups with the library's photos. A member that is no longer in the library (removed since the groups
 * were fetched) is dropped, and a group left with fewer than two frames disappears.
 * @param {import('$lib/api/cull.js').SimilarGroup[]} groups
 * @param {import('$lib/api/catalog.js').ImageSummary[]} images
 * @returns {SimilarGroupView[]}
 */
export function buildGroupViews(groups, images) {
  const byVersion = new Map(images.map((img) => [img.version_id, img]));
  /** @type {SimilarGroupView[]} */
  const views = [];
  for (const group of groups) {
    /** @type {SimilarFrame[]} */
    const frames = [];
    for (const m of group.members) {
      const image = byVersion.get(m.version_id);
      if (!image) continue;
      frames.push({ image, relative: m.relative_sharpness, soft: m.soft, best: m.image_id === group.best_image_id });
    }
    if (frames.length < 2) continue;
    views.push({ key: frames[0].image.image_id, frames, best: frames.find((f) => f.best) ?? null });
  }
  return views;
}

/**
 * The versions "Reject the others" would flag: every frame but the kept one that is not rejected already.
 * @param {SimilarGroupView} group
 * @param {number} keepVersionId
 * @returns {number[]}
 */
export function rejectOthersTargets(group, keepVersionId) {
  return group.frames.filter((f) => f.image.version_id !== keepVersionId && f.image.flag !== "reject").map((f) => f.image.version_id);
}

/**
 * Plain words for the slider's setting. Mirrors `GroupParams::from_similarity` in cull_groups.rs (2 s at 0, 18 s at 1),
 * which is what the backend actually uses; this is only the label.
 * @param {number} similarity 0 (stricter) to 1 (looser)
 */
export function similarityLabel(similarity) {
  const s = Math.min(1, Math.max(0, similarity));
  return `taken within ${Math.round(2 + 16 * s)} s of each other`;
}

/** Wording of the confirmation for "Reject the others".
 * @param {number} count */
export function rejectOthersMessage(count) {
  return `The selected photo stays. The ${count === 1 ? "other photo gets" : `${count} others get`} the Reject flag; no file is changed or deleted. To undo, show Rejected photos (Reject filter) and change the flag.`;
}

/**
 * The badge a frame gets from the eye check, or null when there is nothing to say. Only what the check could tell counts: a photograph
 * whose faces were all too small or too uncertain gets no badge, never a guess (RFC-0031 §8.14).
 * @param {{ faces: number, judged: number, closed: number, one_closed: number } | null | undefined} blink
 * @returns {{ text: string, title: string } | null}
 */
export function eyesBadge(blink) {
  if (!blink || blink.judged === 0) return null;
  const of = `of ${blink.judged} face${blink.judged === 1 ? "" : "s"} checked (${blink.faces} found)`;
  if (blink.closed > 0) return { text: blink.closed > 1 ? `Eyes closed ×${blink.closed}` : "Eyes closed", title: `Both eyes closed in ${blink.closed} ${of}` };
  if (blink.one_closed > 0) return { text: blink.one_closed > 1 ? `One eye closed ×${blink.one_closed}` : "One eye closed", title: `One eye closed in ${blink.one_closed} ${of}: a wink, a squint or a half blink` };
  return null;
}
