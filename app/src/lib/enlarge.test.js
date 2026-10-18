import { describe, it, expect } from "vitest";
import { defaultSettings, parseLongEdge, targetOf, formatEstimate, formatSize, planRows, veryLargeWarning, defaultStem, folderOf, regionFractions, centreFromClick, targetKey, splitEnlargeable, batchRows, batchVeryLargeWarning, batchSourceLine, jobLabel, batchOutcome } from "./enlarge.js";

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

describe("the preview navigator", () => {
  it("draws the region as fractions of the photo", () => {
    expect(regionFractions({ x: 1500, y: 1000, w: 750, h: 500 }, 6000, 4000)).toEqual({ left: 0.25, top: 0.25, width: 0.125, height: 0.125 });
    expect(regionFractions({ x: 0, y: 0, w: 10, h: 10 }, 0, 0)).toEqual({ left: 0, top: 0, width: 1, height: 1 });
  });

  it("turns a click into a centre inside 0..1", () => {
    expect(centreFromClick(32, 20, 64, 40)).toEqual([0.5, 0.5]);
    expect(centreFromClick(-5, 100, 64, 40)).toEqual([0, 1]);
    expect(centreFromClick(1, 1, 0, 0)).toEqual([0.5, 0.5]);
  });

  it("names a target so a preview of another scale can be told apart", () => {
    expect(targetKey({ kind: "scale", value: 4 })).toBe("scale:4");
    expect(targetKey({ kind: "long_edge", value: 8000 })).toBe("long_edge:8000");
    expect(targetKey(null)).toBe("");
  });
});

describe("batch", () => {
  const img = (/** @type {number} */ id, /** @type {number | null} */ scale = null) => /** @type {any} */ ({ image_id: id, enlarged_scale: scale });
  const dur = (/** @type {number} */ ms) => `${Math.round(ms / 1000)} s`;

  it("leaves enlargements out of a selection and counts them", () => {
    const { photos, skipped } = splitEnlargeable([img(1), img(2, 4), img(3), img(4, 2)]);
    expect(photos.map((p) => p.image_id)).toEqual([1, 3]);
    expect(skipped).toBe(2);
  });

  it("totals the plans and says how many photos cannot take the size", () => {
    const rows = batchRows([plan({ megapixels: 40, seconds: 100, file_bytes: 30_000_000 }), plan({ megapixels: 20, seconds: 50, file_bytes: 15_000_000 })], 3);
    expect(rows).toEqual([
      { label: "Photos", value: "2 photos (1 photo cannot be enlarged to this size)" },
      { label: "Output", value: "60.0 MP in total" },
      { label: "Time", value: "about 3 min on this computer" },
      { label: "Files", value: "about 45 MB in total" },
    ]);
    expect(batchRows([plan()], 1)[0].value).toBe("1 photo");
  });

  it("warns only when some result is very large", () => {
    expect(batchVeryLargeWarning([plan(), plan()])).toBe("");
    expect(batchVeryLargeWarning([plan({ very_large: true }), plan()])).toMatch(/^1 photo would come out very large/);
  });

  it("words the source line and the job label", () => {
    expect(batchSourceLine(5, 0)).toBe("5 photos");
    expect(batchSourceLine(3, 1)).toBe("3 photos (1 enlargement is left out)");
    expect(batchSourceLine(3, 2)).toBe("3 photos (2 enlargements are left out)");
    expect(jobLabel("a.jpg", 1, 5)).toBe("a.jpg (2 of 5)");
    expect(jobLabel("a.jpg", 0, 1)).toBe("a.jpg");
  });

  it("reports how a batch ended", () => {
    expect(batchOutcome({ done: 4, count: 4, failed: [], stopped: false, ms: 12000 }, dur)).toBe("Enlarged 4 of 4 photos in 12 s.");
    expect(batchOutcome({ done: 3, count: 4, failed: [{ name: "b.jpg", reason: "no memory" }], stopped: false, ms: 3000 }, dur)).toBe("Enlarged 3 of 4 photos in 3 s. 1 failed: b.jpg (no memory).");
    expect(batchOutcome({ done: 2, count: 5, failed: [], stopped: true, ms: 1000 }, dur)).toBe("Stopped after 2 of 5 photos. The finished ones are in the catalog.");
    expect(batchOutcome({ done: 0, count: 5, failed: [], stopped: true, ms: 1000 }, dur)).toBe("Stopped after 0 of 5 photos.");
  });
});
