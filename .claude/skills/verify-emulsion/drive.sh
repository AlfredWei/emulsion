#!/usr/bin/env bash
# Runs ONE e2e spec against the real debug app with a throwaway HOME, so the app's catalog
# (~/Library/Application Support/dev.alfredwei.emulsion) starts empty and your real one is never touched.
# Usage: .claude/skills/verify-emulsion/drive.sh <spec-name-or-path> [run-id]
#   e.g. drive.sh develop-undo-first-edit
# Evidence (kept after cleanup): .claude/skills/verify-emulsion/artifacts/<run-id>/
#   run.log             the wdio spec reporter output (every step and assertion)
#   catalog-view.txt    read-only sqlite3 view of the throwaway catalog AFTER the run (the second view)
#   meta.txt            commit, binary mtime, spec, exit code
# Exit code is the spec's exit code.
set -uo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
repo="$(cd "$here/../../.." && pwd)"
app="$repo/app"
bin="$app/src-tauri/target/debug/emulsion"
spec="${1:?usage: drive.sh <spec-name-or-path> [run-id]}"
[ -f "$spec" ] || spec="$app/e2e/specs/${spec%.e2e.js}.e2e.js"
[ -f "$spec" ] || { echo "no such spec: $1" >&2; exit 2; }
run_id="${2:-$(date +%Y%m%d-%H%M%S)-$(basename "$spec" .e2e.js)}"
ev="$here/artifacts/$run_id"
mkdir -p "$ev"

"$here/doctor.sh" | tee "$ev/doctor.txt" || { echo "doctor failed; not driving" >&2; exit 3; }

fakehome="$(mktemp -d "${TMPDIR:-/tmp}/emulsion-verify-XXXXXX")"
data="$fakehome/Library/Application Support/dev.alfredwei.emulsion"
{ echo "commit=$(git -C "$repo" rev-parse HEAD)"; echo "binary_mtime=$(stat -f %Sm "$bin")"; echo "spec=$spec"; echo "fakehome=$fakehome"; } > "$ev/meta.txt"

( cd "$app" && HOME="$fakehome" ./node_modules/.bin/wdio run ./e2e/wdio.conf.js --spec "$spec" ) > "$ev/run.log" 2>&1
code=$?
echo "exit_code=$code" >> "$ev/meta.txt"

# Second view: read-only queries on a COPY of the catalog (WAL files included), independent of what the UI showed.
if [ -f "$data/catalog.sqlite" ]; then
  mkdir -p "$ev/catalog-copy"; cp "$data"/catalog.sqlite* "$ev/catalog-copy/"
  {
    echo "images:           $(sqlite3 -readonly "$ev/catalog-copy/catalog.sqlite" 'select count(*) from images;')"
    echo "image_versions:   $(sqlite3 -readonly "$ev/catalog-copy/catalog.sqlite" 'select count(*) from image_versions;')"
    echo "edit_history (version_id|id|label):"
    sqlite3 -readonly "$ev/catalog-copy/catalog.sqlite" 'select version_id, id, label from edit_history order by version_id, id;'
  } > "$ev/catalog-view.txt" 2>&1
  rm -rf "$ev/catalog-copy"
else
  echo "no catalog was created under the throwaway HOME" > "$ev/catalog-view.txt"
fi

# Cleanup: remove only the throwaway HOME. doctor.sh guaranteed no instance was running before us, so any
# instance of this binary still alive now was started by this run (a crashed spec can strand it).
rm -rf "$fakehome"
stray="$(pgrep -f "$bin" || true)"
if [ -n "$stray" ]; then echo "killing stranded app pid(s) from this run: $stray" | tee -a "$ev/meta.txt"; kill $stray; fi

[ -s "$ev/run.log" ] || { echo "evidence missing after cleanup" >&2; exit 4; }
echo "spec exit code: $code    evidence: $ev"
grep -E "passing|failing|✓|✖|Spec Files" "$ev/run.log" | sed 's/^\[[^]]*\] *//' | head -20
exit $code
