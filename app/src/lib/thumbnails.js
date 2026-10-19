// Which thumbnails are only provisional (RFC-0030 slices D and E). A thumbnail goes through tiers, and the file's name
// says which: the camera's own EXIF picture `{id}-t.jpg` (tier 0), the 512 px grid thumbnail `{id}-s.jpg`, and the
// 1024 px `{id}.jpg` that every earlier catalog already has. The backend (`import.rs`) uses the same rule.

/** True for tier 0 and the small tier: a better thumbnail is still to come.
 * @param {string | null | undefined} path */
export function isProvisionalThumbnail(path) {
  return typeof path === "string" && /-[ts]\.jpg$/.test(path);
}

/** True when a photo has no thumbnail yet or only a provisional one: the final one is still to come.
 * @param {string | null | undefined} path */
export function needsRealThumbnail(path) {
  return path == null || isProvisionalThumbnail(path);
}
