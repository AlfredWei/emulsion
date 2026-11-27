import { describe, expect, it } from "vitest";
import { channelCaps, editionAboutLine, hasLensProfiles, modelGate, notInEdition } from "./modelEdition.js";

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

describe("the store edition (ADR-0011)", () => {
  const store = {
    name: "store",
    excluded: ["detect", "inpaint", "upscale", "blink", "face"],
    channel: "mac_app_store",
    caps: { export_command_hook: false, model_downloads: false, custom_cache_dir: false },
    compiled_out: ["lens_profiles"],
  };

  it("the About line names every left-out feature, lens profiles last", () => {
    expect(editionAboutLine(store)).toBe(
      "Subject detection (person preset), Remove, Enlarge (AI), Eye check, Face grouping and Lens profile correction are not included in this edition of Emulsion.",
    );
  });

  it("lens profiles alone give a one-feature line", () => {
    expect(editionAboutLine({ excluded: [], compiled_out: ["lens_profiles"] })).toBe(
      "Lens profile correction is not included in this edition of Emulsion.",
    );
  });

  it("a build without lens profiles says so, and a failed result keeps them", () => {
    expect(hasLensProfiles(store)).toBe(false);
    expect(hasLensProfiles({ compiled_out: [] })).toBe(true);
    expect(hasLensProfiles(null)).toBe(true);
  });

  it("the store's channel has no export hook, downloads or cache move; an unstated cap counts as present", () => {
    expect(channelCaps(store)).toEqual({ export_command_hook: false, model_downloads: false, custom_cache_dir: false });
    expect(channelCaps(null)).toEqual({ export_command_hook: true, model_downloads: true, custom_cache_dir: true });
    expect(channelCaps({ caps: { model_downloads: false } })).toEqual({
      export_command_hook: true,
      model_downloads: false,
      custom_cache_dir: true,
    });
  });
});
