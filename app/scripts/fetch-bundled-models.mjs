// Puts the model files listed in models/bundled-models.json into app/src-tauri/bundled-models/ so a release build
// can ship them (npm run build:release). Every file is verified by size and SHA-256 before it is kept; one that
// is already there and valid is left alone. A partial download resumes with a Range request.
//
//   node scripts/fetch-bundled-models.mjs                   download what is missing
//   node scripts/fetch-bundled-models.mjs --from <dir>      copy what is missing from <dir> first (no network)
//   node scripts/fetch-bundled-models.mjs --edition <name>  bundle only that edition's models (default: EMULSION_EDITION, else "default")
//   node scripts/fetch-bundled-models.mjs --list            print the files the edition bundles and stop (no network, nothing written)
//
// An edition (models/bundled-models.json, "editions") is the list of model families a build may use. The app binary is built for
// one name (EMULSION_EDITION at compile time), so build both with the same variable: `EMULSION_EDITION=<name> npm run build:release`.
// Files of other editions are removed from the output folder (a stale 208 MB model must not ride along), and
// bundled-models.edition.json is written last, only after every file has verified; the app reads it and can only narrow itself by it.
//
// An entry with a "path" instead of a "url" is a small model committed in models/ itself: it is copied from there
// (and verified) with no network use.

import { createHash } from "node:crypto";
import { createReadStream, createWriteStream, existsSync, mkdirSync, readdirSync, readFileSync, renameSync, rmSync, statSync, copyFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { pipeline } from "node:stream/promises";
import { Readable } from "node:stream";
import { editionFile, resolveEdition, strayFiles } from "./bundledEditions.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const manifestText = readFileSync(resolve(here, "../../models/bundled-models.json"), "utf8");
const manifest = JSON.parse(manifestText);
const EDITION_FILE = "bundled-models.edition.json";
const outDir = resolve(here, "../src-tauri/bundled-models");
const fromIdx = process.argv.indexOf("--from");
const fromDir = fromIdx >= 0 ? resolve(process.argv[fromIdx + 1]) : null;
const editionIdx = process.argv.indexOf("--edition");
const edition = resolveEdition(manifest, { flag: editionIdx >= 0 ? process.argv[editionIdx + 1] : null, env: process.env.EMULSION_EDITION });

async function sha256(path) {
  const h = createHash("sha256");
  await pipeline(createReadStream(path), h);
  return h.digest("hex");
}

async function valid(path, file) {
  return existsSync(path) && statSync(path).size === file.size && (await sha256(path)) === file.sha256;
}

async function download(file, part) {
  const have = existsSync(part) ? statSync(part).size : 0;
  const res = await fetch(file.url, { headers: have > 0 ? { Range: `bytes=${have}-` } : {} });
  if (!res.ok && res.status !== 206) throw new Error(`${file.url}: HTTP ${res.status}`);
  const append = res.status === 206;
  if (!append && have > 0) rmSync(part); // the server ignored the range: start over
  await pipeline(Readable.fromWeb(res.body), createWriteStream(part, { flags: append ? "a" : "w" }));
}

if (process.argv.includes("--list")) {
  console.log(`edition ${edition.name}: ${edition.include.join(", ")}`);
  for (const f of edition.files) console.log(`include  ${f.name}`);
  for (const f of edition.omitted) console.log(`omit     ${f.name}`);
  process.exit(0);
}

mkdirSync(outDir, { recursive: true });
// Another edition's files, and the old edition file, go first: if this run fails half way, no stale file is left to ship.
for (const name of strayFiles(readdirSync(outDir), edition, EDITION_FILE)) {
  rmSync(join(outDir, name), { force: true });
  console.log(`removed  ${name} (not in edition ${edition.name})`);
}
for (const file of edition.files) {
  const dest = join(outDir, file.name);
  if (await valid(dest, file)) {
    console.log(`ok       ${file.name}`);
    continue;
  }
  if (file.path) {
    const committed = resolve(here, "../../models", file.path);
    if (!(await valid(committed, file))) throw new Error(`${file.path}: the committed model file is missing or fails size/SHA-256 verification`);
    copyFileSync(committed, dest);
    console.log(`copied   ${file.name} (committed in models/)`);
    continue;
  }
  const local = fromDir ? join(fromDir, file.name) : null;
  if (local && (await valid(local, file))) {
    copyFileSync(local, dest);
    console.log(`copied   ${file.name}`);
    continue;
  }
  const part = `${dest}.part`;
  console.log(`download ${file.name} (${(file.size / 1e6).toFixed(0)} MB) from ${new URL(file.url).host}`);
  await download(file, part);
  if (!(await valid(part, file))) {
    rmSync(part, { force: true });
    throw new Error(`${file.name}: downloaded file failed size/SHA-256 verification`);
  }
  renameSync(part, dest);
  console.log(`verified ${file.name}`);
}
writeFileSync(join(outDir, EDITION_FILE), JSON.stringify(editionFile(edition, createHash("sha256").update(manifestText).digest("hex")), null, 2) + "\n");
console.log(`bundled models for edition ${edition.name} ready in ${outDir}`);
