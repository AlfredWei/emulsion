// Which thumbnails are only placeholders (RFC-0030 slice D). A JPEG's own EXIF picture is shown at once as tier 0
// and named `{id}-t.jpg`; the real thumbnail replaces it a moment later. The backend (`import.rs`) uses the same rule.

/** @param {string | null | undefined} path */
export function isPlaceholderThumbnail(path) {
  return typeof path === "string" && path.endsWith("-t.jpg");
}

/** True when a photo has no thumbnail yet or only the tier-0 picture: the real one is still to come.
 * @param {string | null | undefined} path */
export function needsRealThumbnail(path) {
  return path == null || isPlaceholderThumbnail(path);
}
