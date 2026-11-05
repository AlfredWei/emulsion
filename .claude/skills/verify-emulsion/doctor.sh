#!/usr/bin/env bash
# Read-only: is this checkout worth driving? Usage: .claude/skills/verify-emulsion/doctor.sh
# Exit 0 = ready to drive; non-zero = at least one FAIL line (fix it first).
set -u
repo="$(cd "$(dirname "$0")/../../.." && pwd)"
app="$repo/app"
bin="$app/src-tauri/target/debug/emulsion"
fail=0
ok()  { echo "OK    $*"; }
bad() { echo "FAIL  $*"; fail=1; }

echo "repo   $repo"
echo "commit $(git -C "$repo" rev-parse --short HEAD) on $(git -C "$repo" rev-parse --abbrev-ref HEAD)"

if [ -x "$bin" ]; then
  ok "e2e binary exists: $bin"
  # The WebDriver server is only compiled in with --features wdio-webdriver; a plain `make dev` build cannot be driven.
  if grep -aq "wdio-webdriver" "$bin"; then ok "binary has the embedded WebDriver plugin"; else bad "binary lacks wdio-webdriver: run .claude/skills/verify-emulsion/build.sh"; fi
  newer="$(find "$app/src" "$app/static" "$app/src-tauri/src" "$app/src-tauri/Cargo.toml" "$app/src-tauri/tauri.conf.json" -type f -newer "$bin" 2>/dev/null | head -3)"
  if [ -z "$newer" ]; then ok "binary is newer than every source file"; else bad "binary is older than sources (run build.sh):"; echo "$newer" | sed 's/^/        /'; fi
else
  bad "e2e binary missing: run .claude/skills/verify-emulsion/build.sh"
fi

[ -x "$app/node_modules/.bin/wdio" ] && ok "wdio installed" || bad "wdio missing: cd app && npm install"
command -v sqlite3 >/dev/null && ok "sqlite3 available (read-only second view)" || bad "sqlite3 missing"
[ -f "$repo/test_image/Smiling-woman-pink-shirt-portrait.jpg" ] && ok "fixtures present in test_image/" || bad "test_image/ fixtures missing"

# Never double-drive: another instance of this exact binary means someone else's session.
if pgrep -f "$bin" >/dev/null; then bad "an instance of $bin is already running (not started by this run); do not drive or kill it"; else ok "no running instance of the e2e binary"; fi

# GPU: informational only. CPU-fallback runners are legal; GPU-only features report their own skip.
echo "note  GPU availability is decided inside the app; features/README.md says which journeys need it"
exit $fail
