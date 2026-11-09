// How a photo's capture settings are written where space is tight (the Develop histogram's caption). The Library info
// panel lists the same facts as separate rows. Pure, so the wording is tested without a DOM.

/** "1/250s", "2s". @param {number} seconds */
export function formatShutter(seconds) {
  if (seconds >= 1) return `${seconds}s`;
  return `1/${Math.round(1 / seconds)}s`;
}

/**
 * The capture settings the catalog knows, in Lightroom's order -- ISO, focal length, aperture, shutter -- each as a
 * short string; one the file did not record is left out. Empty when none are known.
 * @param {{ iso?: number | null, focal_length?: number | null, aperture?: number | null, shutter_speed?: number | null } | null | undefined} image
 * @returns {string[]}
 */
export function shotInfoParts(image) {
  if (!image) return [];
  /** @type {string[]} */
  const parts = [];
  if (image.iso) parts.push(`ISO ${image.iso}`);
  if (image.focal_length) parts.push(`${Math.round(image.focal_length)} mm`);
  if (image.aperture) parts.push(`f/${image.aperture.toFixed(1)}`);
  if (image.shutter_speed) parts.push(formatShutter(image.shutter_speed));
  return parts;
}
