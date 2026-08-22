import { describe, it, expect } from "vitest";
import { folderKeyForPath, buildFolderEntries } from "./libraryFolders.js";

describe("folderKeyForPath", () => {
  it("joins the last two directory segments with a forward slash", () => {
    expect(folderKeyForPath("/Users/alfred/Photos/2026/wedding/img_0001.CR3")).toBe("2026/wedding");
  });

  it("handles Windows-style backslash paths", () => {
    expect(folderKeyForPath("C:\\Users\\alfred\\Photos\\2026\\wedding\\img_0001.CR3")).toBe("2026/wedding");
  });

  it("returns null when there are fewer than 2 parent directories", () => {
    expect(folderKeyForPath("/photos/img.jpg")).toBeNull();
    expect(folderKeyForPath("img.jpg")).toBeNull();
  });

  it("ignores a trailing slash / repeated separators", () => {
    expect(folderKeyForPath("/a//b/c/img.jpg")).toBe("b/c");
  });
});

describe("buildFolderEntries", () => {
  it("groups images by folder key and counts them, sorted alphabetically", () => {
    const images = /** @type {any[]} */ ([
      { path: "/root/2026/wedding/a.jpg" },
      { path: "/root/2026/wedding/b.jpg" },
      { path: "/root/2025/trip/c.jpg" },
      { path: "/single-level/d.jpg" }, // excluded, only 1 parent dir
    ]);
    expect(buildFolderEntries(images)).toEqual([
      { key: "2025/trip", count: 1 },
      { key: "2026/wedding", count: 2 },
    ]);
  });

  it("returns an empty list for no images", () => {
    expect(buildFolderEntries([])).toEqual([]);
  });
});
