#!/usr/bin/env bash
# Writes the contents of the public source repository for one release: the LibRaw source exactly as the app builds it,
# the two MIT wrapper crates around it, and the MPL-2.0 crates the app links. CDDL-1.0 section 3.1 and MPL-2.0
# section 3.2(a) ask that this source be available to everyone who receives the app. app/src/lib/openSource.js holds
# the repository's address, and docs/release/open-source-repository.md is the owner's checklist.
#
#   scripts/release/export_open_source.sh <tag> <out-dir>
#
# <tag> is the app's release tag. <out-dir> is an empty folder or a clone of the public repository; the script replaces
# its libraw/, rsraw-sys/, rsraw/, mpl/ and README.md and leaves everything else (such as .git) alone.
# Exit 1 when this checkout's vendored crates or Cargo.lock differ from <tag>, because then the source written would not
# be the source the tag builds. The LibRaw and wrapper files come from <tag> itself through git archive.
set -euo pipefail

[ $# -eq 2 ] || { sed -n '2,13p' "$0" >&2; exit 2; }
tag="$1"
out="$2"
repo="$(git rev-parse --show-toplevel)"
vendor="app/src-tauri/vendor"
cd "$repo"

git rev-parse --verify --quiet "$tag^{commit}" >/dev/null || { echo "no such tag or commit: $tag" >&2; exit 1; }
if ! git diff --quiet "$tag" -- "$vendor/rsraw-sys" "$vendor/rsraw" app/src-tauri/Cargo.lock; then
  echo "this checkout's $vendor or Cargo.lock differs from $tag; check out $tag first:" >&2
  git diff --stat "$tag" -- "$vendor/rsraw-sys" "$vendor/rsraw" app/src-tauri/Cargo.lock >&2
  exit 1
fi

mkdir -p "$out"
out="$(cd "$out" && pwd)"
rm -rf "$out/libraw" "$out/rsraw-sys" "$out/rsraw" "$out/mpl" "$out/README.md"
mkdir -p "$out/libraw" "$out/rsraw-sys" "$out/rsraw" "$out/mpl"

git archive "$tag" "$vendor/rsraw-sys/LibRaw" | tar -x -C "$out/libraw" --strip-components=5
git archive "$tag" "$vendor/rsraw-sys/build.rs" "$vendor/rsraw-sys/Cargo.toml" "$vendor/rsraw-sys/src" \
  "$vendor/rsraw-sys/PATCH.md" "$vendor/rsraw-sys/README.md" | tar -x -C "$out/rsraw-sys" --strip-components=4
git archive "$tag" "$vendor/rsraw" | tar -x -C "$out/rsraw" --strip-components=4

for f in COPYRIGHT LICENSE.CDDL LICENSE.LGPL; do
  [ -s "$out/libraw/$f" ] || { echo "libraw/$f is missing from $tag" >&2; exit 1; }
done

# The MPL-2.0 crates at the versions the lockfile pins, copied from Cargo's registry as crates.io serves them. A proc-macro
# runs at build time and does not ship, so it is left out.
mpl_list="$(cd app/src-tauri && cargo metadata --format-version 1 --locked --color never | python3 -I -c '
import json, os, sys
for p in json.load(sys.stdin)["packages"]:
    proc_macro = any("proc-macro" in t["kind"] for t in p["targets"])
    if "MPL" in (p.get("license") or "") and not proc_macro:
        print(p["name"], p["version"], os.path.dirname(p["manifest_path"]), sep="\t")
')"
[ -n "$mpl_list" ] || { echo "cargo metadata found no MPL-2.0 crate" >&2; exit 1; }
mpl_lines=""
while IFS=$'\t' read -r name version dir; do
  mkdir -p "$out/mpl/$name-$version"
  (cd "$dir" && tar -c --exclude .cargo-ok --exclude .cargo_vcs_info.json .) | tar -x -C "$out/mpl/$name-$version"
  mpl_lines+="- \`mpl/$name-$version/\`, also at https://crates.io/crates/$name/$version"$'\n'
done <<<"$mpl_list"

libraw_version="$(sed -n 's/^#define LIBRAW_[A-Z]*_VERSION \([0-9]*\)$/\1/p' "$out/libraw/libraw/libraw_version.h" | paste -sd. -)"
commit="$(git rev-parse "$tag^{commit}")"

cat >"$out/README.md" <<EOF
# Emulsion open-source components

This repository holds the source code of the open-source components in Emulsion release \`$tag\` (built from commit
\`$commit\`) whose licences ask that their source be available to everyone who receives the app. Emulsion itself is
proprietary and is not here.

## LibRaw $libraw_version

Emulsion uses LibRaw $libraw_version under the CDDL-1.0, as LibRaw LLC permits. \`libraw/\` is the LibRaw source exactly
as Emulsion compiles it, unchanged, with LibRaw's \`COPYRIGHT\`, \`LICENSE.CDDL\` and \`LICENSE.LGPL\`. The BSD notices of
its DCB, FBDD and X3F parts are in the headers of \`libraw/src/demosaic/dcb_demosaic.cpp\` and \`libraw/src/x3f/\`.

Terms of Emulsion's licence that differ from the CDDL are offered by Emulsion's author alone, not by LibRaw LLC, and
nothing in them limits your rights under the CDDL in this source.

## The wrapper crates

- \`rsraw-sys/\` builds LibRaw and holds the Rust bindings to its header, including the generated \`src/bindings.rs\`.
  MIT, from https://github.com/hexilee/rsraw, changed as its \`PATCH.md\` says.
- \`rsraw/\` is the Rust wrapper around those bindings. MIT, from the same upstream, changed as its \`PATCH.md\` says.

## MPL-2.0 crates

These crates are used unmodified, at the versions this release links:

$mpl_lines
## Releases

Each Emulsion release has a tag of the same name here.
EOF

echo "wrote $out for $tag (LibRaw $libraw_version, $(wc -l <<<"$mpl_list" | tr -d ' ') MPL-2.0 crates)"
