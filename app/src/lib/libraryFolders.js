// Folder-by-path grouping for the Library rail's "Folders" section (M4
// Library slice) -- entirely client-side, same "extracted pure function +
// unit test" shape as collectionRules.js. No catalog schema change: every
// ImageSummary already carries its full source path, so a folder identity
// is derived from that rather than stored.

/** Groups images by their parent directory's last two path segments (e.g.
 * "/users/x/2026/wedding/img.jpg" -> "2026/wedding") -- deliberately just
 * the last two, not the full ancestor chain, to keep the rail's folder
 * names short and stable even when photos live deep inside a dated import
 * structure. Handles both POSIX and Windows separators. Returns `null` for
 * a path with fewer than 2 real parent directories -- there's no
 * meaningful "last 2 folders" for those, so they're excluded from the
 * Folders list entirely rather than showing a misleading partial key.
 * @param {string} path
 * @returns {string | null} */
export function folderKeyForPath(path) {
  const parts = path.split(/[/\\]/).filter(Boolean);
  const dirParts = parts.slice(0, -1); // drop the filename
  if (dirParts.length < 2) return null;
  return dirParts.slice(-2).join("/");
}

/** @typedef {{ key: string, count: number }} FolderEntry */

/** Builds the sorted [{key, count}] list the rail renders, one entry per
 * distinct folder key across `images`.
 * @param {import('./api/catalog.js').ImageSummary[]} images
 * @returns {FolderEntry[]} */
export function buildFolderEntries(images) {
  const counts = new Map();
  for (const img of images) {
    const key = folderKeyForPath(img.path);
    if (key === null) continue;
    counts.set(key, (counts.get(key) ?? 0) + 1);
  }
  return Array.from(counts.entries())
    .map(([key, count]) => ({ key, count }))
    .sort((a, b) => a.key.localeCompare(b.key));
}
