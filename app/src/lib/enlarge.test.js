import { describe, it, expect } from "vitest";
import { defaultSettings, parseLongEdge, targetOf, formatEstimate, formatSize, planRows, veryLargeWarning, defaultStem, folderOf } from "./enlarge.js";

/** @param {Partial<import('$lib/api/upscale.js').UpscalePlan>} over */
const plan = (over = {}) => ({ source_width: 6000, source_height: 4000, out_width: 24000, out_height: 16000, megapixels: 384, tiles: 460, seconds: 300, file_bytes: 345_600_000, very_large: false, export_gb: 23, ...over });

describe("targetOf", () => {
  it("maps the scale choices", () => {
    expect(targetOf({ ...defaultSettings(), scale: "2" })).toEqual({ kind: "scale", value: 2 });
    expect(targetOf(defaultSettings())).toEqual({ kind: "scale", value: 4 });
  });

  it("reads the long edge field, tolerating separators, and is null while it is not a number", () => {
    const s = { ...defaultSettings(), scale: /** @type {const} */ ("long_edge") };
    expect(targetOf({ ...s, longEdge: " 8,000 " })).toEqual({ kind: "long_edge", value: 8000 });
    expect(targetOf({ ...s, longEdge: "" })).toBeNull();
    expect(targetOf({ ...s, longEdge: "8k" })).toBeNull();
    expect(parseLongEdge("0")).toBeNull();
    expect(parseLongEdge("-5")).toBeNull();
  });
});

describe("formatting", () => {
  it("rounds an estimate the way an estimate is read", () => {
    expect(formatEstimate(0.2)).toBe("1 s");
    expect(formatEstimate(42.4)).toBe("42 s");
    expect(formatEstimate(95)).toBe("2 min");
    expect(formatEstimate(3600)).toBe("1 h");
    expect(formatEstimate(3900)).toBe("1 h 5 min");
  });

  it("writes sizes in decimal units", () => {
    expect(formatSize(400)).toBe("1 KB");
    expect(formatSize(850_000)).toBe("850 KB");
    expect(formatSize(34_400_000)).toBe("34 MB");
    expect(formatSize(1_240_000_000)).toBe("1.2 GB");
  });

  it("describes a plan in three rows", () => {
    const rows = planRows(plan());
    expect(rows.map((r) => r.label)).toEqual(["Output", "Time", "File"]);
    expect(rows[0].value).toBe("24,000 × 16,000 px (384 MP)");
    expect(rows[1].value).toBe("about 5 min on this computer");
    expect(rows[2].value).toBe("about 346 MB");
  });
});

describe("veryLargeWarning", () => {
  it("is empty for an ordinary result", () => {
    expect(veryLargeWarning(plan({ very_large: false }))).toBe("");
  });

  it("names the size and what exporting it later needs", () => {
    const text = veryLargeWarning(plan({ very_large: true, megapixels: 150, export_gb: 9 }));
    expect(text).toMatch(/^150 MP is very large: it needs several GB of disk and most editors cannot open it/);
    expect(text).toMatch(/about 9\.0 GB/);
  });
});

describe("names", () => {
  it("proposes the backend's default name", () => {
    expect(defaultStem("/a/b/IMG_1.CR3", { kind: "scale", value: 4 })).toBe("IMG_1-Enlarged-4x");
    expect(defaultStem("C:\\pics\\x.jpg", { kind: "long_edge", value: 8000 })).toBe("x-Enlarged-8000px");
    expect(defaultStem("/a/.hidden", null)).toBe(".hidden-Enlarged");
  });

  it("finds the folder", () => {
    expect(folderOf("/a/b/c.jpg")).toBe("/a/b");
    expect(folderOf("C:\\a\\c.jpg")).toBe("C:\\a");
    expect(folderOf("c.jpg")).toBe("c.jpg");
  });
});
