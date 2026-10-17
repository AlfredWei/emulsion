// What the Library shows about AI enlargements (RFC-0029 §3.2): the grid chip's text and the lines of the
// "Connected photos" block. Pure, so the wording is tested without a DOM.

/** "×4", "×2", "×2.5": the factor on the chip. @param {number} scale */
export function scaleLabel(scale) {
  const rounded = Math.round(scale * 10) / 10;
  return `×${Number.isInteger(rounded) ? rounded : rounded.toFixed(1)}`;
}

/**
 * @typedef {Object} ConnectedLine
 * @property {"origin" | "enlargement"} kind
 * @property {string} text  what the line says
 * @property {string | null} note  a second, quieter line (edited since / original removed), or null
 * @property {number | null} imageId  the photo the line's button goes to, null when there is none (original removed)
 * @property {string} action  the button's label ("" when the line has no button)
 */

/**
 * The block's lines for one photo: where it came from if it is an enlargement, then its enlargements.
 * @param {import('$lib/api/catalog.js').UpscaleLinks} links
 * @param {(imageId: number) => string | null} nameOf  the file name of a catalog photo, null when it is not in the list
 * @returns {ConnectedLine[]}
 */
export function connectedLines(links, nameOf) {
  /** @type {ConnectedLine[]} */
  const lines = [];
  const { origin, enlargements } = links;
  if (origin) {
    const name = origin.source_image_id === null ? null : nameOf(origin.source_image_id);
    if (origin.source_image_id === null) {
      lines.push({ kind: "origin", text: `Enlarged ${scaleLabel(origin.scale)} (AI)`, note: "The original was removed from the catalog.", imageId: null, action: "" });
    } else {
      lines.push({
        kind: "origin",
        text: `Enlarged ${scaleLabel(origin.scale)} (AI) from ${name ?? "the original"}`,
        note: origin.edited_since ? "The original has been edited since. This is the version as it was then." : null,
        imageId: origin.source_image_id,
        action: "Go to original",
      });
    }
  }
  if (enlargements.length > 0) {
    const n = enlargements.length;
    lines.push({
      kind: "enlargement",
      text: n === 1 ? `1 enlargement: ${scaleLabel(enlargements[0].scale)}` : `${n} enlargements: ${enlargements.map((e) => scaleLabel(e.scale)).join(", ")}`,
      note: enlargements.some((e) => e.edited_since) ? "Edited since: the enlargement shows the earlier version." : null,
      imageId: enlargements[0].result_image_id,
      action: n === 1 ? "Show enlargement" : "Show enlargements",
    });
  }
  return lines;
}

/**
 * The info panel's Enlarge button for an original: "Enlarge again…" once an enlargement shows an earlier edit of it
 * (the new one is another connected photo; the first is kept), "Enlarge (AI)…" otherwise. Null for an enlargement,
 * which is not enlarged again (it would compound invented detail).
 * @param {import('$lib/api/catalog.js').UpscaleLinks | null} links  null while unread or for an ordinary photo
 * @param {boolean} isEnlargement
 * @returns {string | null}
 */
export function enlargeButtonLabel(links, isEnlargement) {
  if (isEnlargement) return null;
  return links?.enlargements.some((e) => e.edited_since) ? "Enlarge again…" : "Enlarge (AI)…";
}
