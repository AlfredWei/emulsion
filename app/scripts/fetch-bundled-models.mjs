// Puts the model files listed in models/bundled-models.json into app/src-tauri/bundled-models/ so a release build
// can ship them (npm run build:release). Every file is verified by size and SHA-256 before it is kept; one that
// is already there and valid is left alone. A partial download resumes with a Range request.
//
//   node scripts/fetch-bundled-models.mjs                   download what is missing
//   node scripts/fetch-bundled-models.mjs --from <dir>      copy what is missing from <dir> first (no network)

import { createHash } from "node:crypto";
import { createReadStream, createWriteStream, existsSync, mkdirSync, readFileSync, renameSync, rmSync, statSync, copyFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { pipeline } from "node:stream/promises";
import { Readable } from "node:stream";

const here = dirname(fileURLToPath(import.meta.url));
const manifest = JSON.parse(readFileSync(resolve(here, "../../models/bundled-models.json"), "utf8"));
const outDir = resolve(here, "../src-tauri/bundled-models");
const fromIdx = process.argv.indexOf("--from");
const fromDir = fromIdx >= 0 ? resolve(process.argv[fromIdx + 1]) : null;

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

mkdirSync(outDir, { recursive: true });
for (const file of manifest.files) {
  const dest = join(outDir, file.name);
  if (await valid(dest, file)) {
    console.log(`ok       ${file.name}`);
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
console.log(`bundled models ready in ${outDir}`);
