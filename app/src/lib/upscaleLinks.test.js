import { describe, it, expect } from "vitest";
import { scaleLabel, connectedLines } from "./upscaleLinks.js";

const origin = (/** @type {object} */ o = {}) => ({ source_image_id: 7, source_version_id: 70, scale: 4, strength: 45, model_id: "m", created_at: "", edited_since: false, ...o });
const enl = (/** @type {number} */ id, /** @type {number} */ scale = 4, /** @type {boolean | null} */ edited = false) => ({ result_image_id: id, scale, strength: 45, model_id: "m", created_at: "", edited_since: edited });
const names = (/** @type {number} */ id) => (id === 7 ? "IMG_0042.jpg" : null);

describe("scaleLabel", () => {
  it("writes whole factors plainly and others with one decimal", () => {
    expect(scaleLabel(4)).toBe("×4");
    expect(scaleLabel(2)).toBe("×2");
    expect(scaleLabel(2.5)).toBe("×2.5");
    expect(scaleLabel(3.9999)).toBe("×4");
    expect(scaleLabel(1.96)).toBe("×2");
  });
});

describe("connectedLines", () => {
  it("is empty for an ordinary photo", () => {
    expect(connectedLines({ origin: null, enlargements: [] }, names)).toEqual([]);
  });

  it("names the original and offers to go there", () => {
    const [line] = connectedLines({ origin: /** @type {any} */ (origin()), enlargements: [] }, names);
    expect(line).toMatchObject({ kind: "origin", text: "Enlarged ×4 (AI) from IMG_0042.jpg", note: null, imageId: 7, action: "Go to original" });
  });

  it("says when the original was edited since, and keeps the link", () => {
    const [line] = connectedLines({ origin: /** @type {any} */ (origin({ edited_since: true })), enlargements: [] }, names);
    expect(line.note).toMatch(/edited since/i);
    expect(line.imageId).toBe(7);
  });

  it("says the original was removed and offers nothing to click", () => {
    const [line] = connectedLines({ origin: /** @type {any} */ (origin({ source_image_id: null, source_version_id: null, edited_since: null })), enlargements: [] }, names);
    expect(line).toMatchObject({ text: "Enlarged ×4 (AI)", note: "The original was removed from the catalog.", imageId: null, action: "" });
  });

  it("falls back to 'the original' when the file is not in the loaded list", () => {
    const [line] = connectedLines({ origin: /** @type {any} */ (origin({ source_image_id: 99 })), enlargements: [] }, names);
    expect(line.text).toBe("Enlarged ×4 (AI) from the original");
  });

  it("lists one enlargement and several, and flags edited-since on the original's side", () => {
    const one = connectedLines({ origin: null, enlargements: [enl(5)] }, names);
    expect(one).toEqual([{ kind: "enlargement", text: "1 enlargement: ×4", note: null, imageId: 5, action: "Show enlargement" }]);
    const many = connectedLines({ origin: null, enlargements: [enl(6, 2.5, true), enl(5, 4)] }, names);
    expect(many[0]).toMatchObject({ text: "2 enlargements: ×2.5, ×4", imageId: 6, action: "Show enlargements" });
    expect(many[0].note).toMatch(/edited since/i);
  });
});
