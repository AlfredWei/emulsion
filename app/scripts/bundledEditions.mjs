// Which model files a release bundle carries for an edition (RFC-0038 slice 4, ADR-0012). Pure, so Vitest covers it;
// fetch-bundled-models.mjs does the file work. The Rust side (src-tauri/src/editions.rs) reads the same manifest.

/**
 * @param {{ files: { name: string, family: string }[], editions: Record<string, { include: string[] }> }} manifest
 * @param {{ flag?: string | null, env?: string | null }} [choice] `--edition` and `EMULSION_EDITION`
 */
export function resolveEdition(manifest, { flag = null, env = null } = {}) {
  const f = flag || null;
  const e = env || null;
  if (f && e && f !== e) {
    throw new Error(`--edition ${f} disagrees with EMULSION_EDITION=${e}: the app binary is compiled for the edition in EMULSION_EDITION, so the bundle must be the same one`);
  }
  const name = f ?? e ?? "default";
  const def = manifest.editions?.[name];
  if (!def) throw new Error(`unknown edition "${name}"; the manifest defines: ${Object.keys(manifest.editions ?? {}).join(", ") || "none"}`);
  for (const file of manifest.files) {
    if (!file.family) throw new Error(`${file.name} has no "family" in the manifest`);
  }
  const include = new Set(def.include);
  const families = new Set(manifest.files.map((x) => x.family));
  for (const fam of def.include) {
    // `face` has no file (YuNet and SFace are downloaded); any other name must be a family some file carries.
    if (fam !== "face" && !families.has(fam)) throw new Error(`edition "${name}" lists an unknown family "${fam}"`);
  }
  return {
    name,
    include: [...def.include],
    files: manifest.files.filter((x) => include.has(x.family)),
    omitted: manifest.files.filter((x) => !include.has(x.family)),
  };
}

/** The JSON the bundle carries beside the models (read by editions.rs). */
export function editionFile(resolved, manifestSha256) {
  return { edition: resolved.name, include: resolved.include, files: resolved.files.map((x) => x.name), manifest_sha256: manifestSha256 };
}

/** Files in the output folder that the edition does not want: another edition's models, their `.part` files, and any old edition file. */
export function strayFiles(present, resolved, editionFileName) {
  const wanted = new Set(resolved.files.map((x) => x.name));
  return present.filter((n) => {
    if (n === editionFileName) return true; // rewritten last, only after every file verified
    if (n.endsWith(".onnx.part")) return !wanted.has(n.slice(0, -".part".length));
    if (n.endsWith(".onnx")) return !wanted.has(n);
    return false;
  });
}
