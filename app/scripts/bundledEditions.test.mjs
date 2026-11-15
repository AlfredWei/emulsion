import { describe, expect, it } from "vitest";
import { editionFile, resolveEdition, strayFiles } from "./bundledEditions.mjs";

const manifest = {
  files: [
    { name: "a.onnx", family: "denoise" },
    { name: "b.encoder.onnx", family: "segment" },
    { name: "b.decoder.onnx", family: "segment" },
    { name: "lama.onnx", family: "inpaint" },
  ],
  editions: {
    default: { include: ["denoise", "segment", "inpaint", "face"] },
    "no-lama": { include: ["denoise", "segment", "face"] },
  },
};

describe("resolveEdition", () => {
  it("the default edition bundles every file", () => {
    const r = resolveEdition(manifest);
    expect(r.name).toBe("default");
    expect(r.files.map((f) => f.name)).toEqual(["a.onnx", "b.encoder.onnx", "b.decoder.onnx", "lama.onnx"]);
    expect(r.omitted).toEqual([]);
  });

  it("an edition leaves out the files of the families it does not list", () => {
    const r = resolveEdition(manifest, { flag: "no-lama" });
    expect(r.files.map((f) => f.name)).toEqual(["a.onnx", "b.encoder.onnx", "b.decoder.onnx"]);
    expect(r.omitted.map((f) => f.name)).toEqual(["lama.onnx"]);
  });

  it("takes the name from the environment when there is no flag, and the two must agree", () => {
    expect(resolveEdition(manifest, { env: "no-lama" }).name).toBe("no-lama");
    expect(resolveEdition(manifest, { flag: "no-lama", env: "no-lama" }).name).toBe("no-lama");
    expect(() => resolveEdition(manifest, { flag: "no-lama", env: "default" })).toThrow(/disagrees/);
  });

  it("an unknown edition or family is an error, never a silent default", () => {
    expect(() => resolveEdition(manifest, { flag: "nope" })).toThrow(/unknown edition "nope".*default, no-lama/);
    expect(() => resolveEdition({ ...manifest, editions: { default: { include: ["typo"] } } })).toThrow(/unknown family "typo"/);
    expect(() => resolveEdition({ ...manifest, files: [{ name: "x.onnx" }] })).toThrow(/no "family"/);
  });
});

describe("editionFile and strayFiles", () => {
  it("the edition file names what the bundle holds", () => {
    const r = resolveEdition(manifest, { flag: "no-lama" });
    expect(editionFile(r, "abc")).toEqual({ edition: "no-lama", include: ["denoise", "segment", "face"], files: ["a.onnx", "b.encoder.onnx", "b.decoder.onnx"], manifest_sha256: "abc" });
  });

  it("a model of another edition is pruned, so a no-LaMa build after a default build does not ship LaMa", () => {
    const r = resolveEdition(manifest, { flag: "no-lama" });
    const present = ["a.onnx", "lama.onnx", "lama.onnx.part", "b.encoder.onnx.part", "bundled-models.edition.json", "README.txt"];
    expect(strayFiles(present, r, "bundled-models.edition.json")).toEqual(["lama.onnx", "lama.onnx.part", "bundled-models.edition.json"]);
  });
});
