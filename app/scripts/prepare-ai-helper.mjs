// Builds the `emulsion-ai` helper (release) and puts it where Tauri's `bundle.externalBin` looks for it, so a release
// build ships it beside the app executable (src-tauri/tauri.bundle-helper.conf.json; the app finds it there, see
// `AiHelper::default_exe`). Without it an installer would start with Enlarge (AI), AI Denoise and Select Subject all
// failing at first use. Run by `npm run build:release`.
//
//   node scripts/prepare-ai-helper.mjs

import { execFileSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, statSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const tauriDir = resolve(here, "../src-tauri");
const exe = process.platform === "win32" ? ".exe" : "";

const hostLine = execFileSync("rustc", ["-vV"], { encoding: "utf8" })
  .split("\n")
  .find((l) => l.startsWith("host:"));
if (!hostLine) throw new Error("could not read the target triple from `rustc -vV`");
const triple = hostLine.slice("host:".length).trim();

// The same profile and target as `tauri build` (the host), so the helper is not compiled twice.
execFileSync("cargo", ["build", "--release", "--bin", "emulsion-ai"], { cwd: tauriDir, stdio: "inherit" });

const built = join(tauriDir, "target", "release", `emulsion-ai${exe}`);
if (!existsSync(built)) throw new Error(`cargo built no ${built}`);
const outDir = join(tauriDir, "binaries");
mkdirSync(outDir, { recursive: true });
const dest = join(outDir, `emulsion-ai-${triple}${exe}`);
copyFileSync(built, dest);
console.log(`helper: ${dest} (${(statSync(dest).size / 1e6).toFixed(1)} MB)`);
