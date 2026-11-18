#!/usr/bin/env bash
# Builds a drivable macOS bundle under the App Sandbox, with its own bundle ID so it gets its own container.
#
#   scripts/lanes/build-sandboxed.sh --channel <direct|mas> --bundle-id <id> [--out <dir>]
#
# The build is the spike's (tauri build --features wdio-webdriver --bundles app) plus the e2e, models and helper configs,
# and a merged config that sets `identifier`. The helper is signed with app sandbox plus inherit, then the app with
# lane-drivable.entitlements. Signing is ad hoc unless LANE_SIGN_ID names an identity. The last line printed is the app path.
set -euo pipefail

channel=""
bundle_id=""
out="${LANE_APPS_DIR:-${TMPDIR:-/tmp}/lane-apps}"
while [ $# -gt 0 ]; do
  case "$1" in
    --channel) channel="${2:?}"; shift 2 ;;
    --bundle-id) bundle_id="${2:?}"; shift 2 ;;
    --out) out="${2:?}"; shift 2 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done
case "$channel" in direct|mas) ;; *) echo "--channel must be direct or mas" >&2; exit 2 ;; esac
case "$bundle_id" in
  "") echo "--bundle-id is required" >&2; exit 2 ;;
  *[!A-Za-z0-9.-]*) echo "--bundle-id may hold only letters, digits, dots and hyphens" >&2; exit 2 ;;
esac
[ "$(uname -s)" = Darwin ] || { echo "the sandbox exists only on macOS" >&2; exit 2; }

repo="$(cd "$(dirname "$0")/../.." && pwd)"
app="$repo/app"
tauri="$app/src-tauri"
sign_id="${LANE_SIGN_ID:--}"

[ -d "$tauri/bundled-models" ] || { echo "app/src-tauri/bundled-models/ is missing: run node app/scripts/fetch-bundled-models.mjs" >&2; exit 2; }
triple="$(rustc -vV | sed -n 's/^host: //p')"
[ -f "$tauri/binaries/emulsion-ai-$triple" ] || (cd "$app" && node scripts/prepare-ai-helper.mjs)

mkdir -p "$out"
work="$(mktemp -d "${TMPDIR:-/tmp}/lane-build-XXXXXX")"
trap 'rm -rf "$work"' EXIT
printf '{"identifier":"%s"}\n' "$bundle_id" > "$work/identifier.json"

# The channel flag lands with S1, so until then the channel is recorded and changes nothing in the build.
echo "channel=$channel bundle_id=$bundle_id sign=$([ "$sign_id" = - ] && echo adhoc || echo identity)" >&2

(cd "$app" && ./node_modules/.bin/tauri build --features wdio-webdriver --bundles app \
  --config src-tauri/tauri.e2e.conf.json \
  --config src-tauri/tauri.bundle-models.conf.json \
  --config src-tauri/tauri.bundle-helper.conf.json \
  --config "$work/identifier.json" >&2)

built="$tauri/target/release/bundle/macos/Emulsion.app"
[ -d "$built" ] || { echo "no bundle at $built" >&2; exit 1; }
dest="$out/$bundle_id.app"
rm -rf "$dest"
cp -R "$built" "$dest"

codesign --force -s "$sign_id" --entitlements "$tauri/entitlements/lane-helper.entitlements" "$dest/Contents/MacOS/emulsion-ai" >&2
codesign --force -s "$sign_id" --entitlements "$tauri/entitlements/lane-drivable.entitlements" "$dest" >&2
codesign --verify --deep --strict "$dest" >&2

{
  echo "== app"; codesign -d --entitlements :- "$dest" 2>&1
  echo "== helper"; codesign -d --entitlements :- "$dest/Contents/MacOS/emulsion-ai" 2>&1
} > "$out/$bundle_id.entitlements.txt"
echo "entitlements: $out/$bundle_id.entitlements.txt" >&2
echo "$dest"
