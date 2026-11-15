import { describe, expect, it } from "vitest";
import { modelGate, notInEdition } from "./modelEdition.js";

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
