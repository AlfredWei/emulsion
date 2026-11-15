import { describe, expect, it } from "vitest";
import { editionAboutLine, modelGate, notInEdition } from "./modelEdition.js";

describe("modelGate", () => {
  it("ready runs, missing offers the download, not_in_edition offers nothing", () => {
    expect(modelGate({ state: "ready" })).toBe("ready");
    expect(modelGate({ state: "missing" })).toBe("download");
    expect(modelGate({ state: "not_in_edition" })).toBe("excluded");
  });

  it("an unknown or absent state never offers a download", () => {
    expect(modelGate({ state: "something_new" })).toBe("excluded");
    expect(modelGate(null)).toBe("excluded");
    expect(modelGate(undefined)).toBe("excluded");
    expect(modelGate({})).toBe("excluded");
  });
});

describe("notInEdition", () => {
  it("is one sentence naming the feature and giving no reason", () => {
    expect(notInEdition("Remove")).toBe("Remove is not included in this edition of Emulsion.");
  });
});

describe("editionAboutLine", () => {
  it("shows nothing for the default edition", () => {
    expect(editionAboutLine({ name: "default", excluded: [] })).toBeNull();
  });

  it("names exactly Remove for the test-no-lama fixture", () => {
    expect(editionAboutLine({ name: "test-no-lama", excluded: ["inpaint"] })).toBe(
      "Remove is not included in this edition of Emulsion.",
    );
  });

  it("lists several features in a fixed order whatever order the backend sends", () => {
    expect(editionAboutLine({ name: "x", excluded: ["face", "denoise", "upscale"] })).toBe(
      "AI Denoise, Enlarge (AI) and Face grouping are not included in this edition of Emulsion.",
    );
  });

  it("names every feature when every family is left out", () => {
    const all = ["denoise", "segment", "detect", "inpaint", "upscale", "blink", "face"];
    const line = editionAboutLine({ excluded: all });
    for (const f of ["AI Denoise", "Select Subject", "Subject detection (person preset)", "Remove", "Enlarge (AI)", "Eye check", "Face grouping"]) {
      expect(line).toContain(f);
    }
  });

  it("gives no line, and does not throw, for a failed or unknown result", () => {
    expect(editionAboutLine(null)).toBeNull();
    expect(editionAboutLine(undefined)).toBeNull();
    expect(editionAboutLine({})).toBeNull();
    expect(editionAboutLine({ excluded: "inpaint" })).toBeNull();
    expect(editionAboutLine({ excluded: ["something_new"] })).toBeNull();
    expect(editionAboutLine({ excluded: [null, 3] })).toBeNull();
  });

  it("ignores an unknown family but still names the known ones", () => {
    expect(editionAboutLine({ excluded: ["something_new", "inpaint"] })).toBe(
      "Remove is not included in this edition of Emulsion.",
    );
  });
});
