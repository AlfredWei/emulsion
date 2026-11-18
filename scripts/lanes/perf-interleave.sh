#!/usr/bin/env bash
# Measures one perf-probe metric at two commits in alternating order, so drift in the machine favours neither side.
#
#   scripts/lanes/perf-interleave.sh <trunk-sha> <head-sha> <metric> [--runs N]
#   scripts/lanes/perf-interleave.sh --self-test
#
# <metric> is a key of the perf.json that app/e2e/specs/perf-probe.e2e.js writes (import_ms, first_grid_ms, export_ms).
# The probe spec and the wdio config always come from this checkout, so a trunk that lacks the harness is still measured.
# Only the app binary differs. Each side's debug binary is built once in a worktree under $PERF_WORK, all of them into one
# $CARGO_TARGET_DIR so the second build reuses the dependencies. PERF_BIN_TRUNK and PERF_BIN_HEAD name prebuilt binaries instead.
# Prints both medians, the trunk spread and the head to trunk ratio. Exit 1 when any run fails or reports no number.
set -euo pipefail

median() { printf '%s\n' "$@" | sort -g | awk '{a[NR]=$1} END {if (NR%2) print a[(NR+1)/2]; else print (a[NR/2]+a[NR/2+1])/2}'; }
spread_pct() { printf '%s\n' "$@" | sort -g | awk '{a[NR]=$1} END {m=(NR%2)?a[(NR+1)/2]:(a[NR/2]+a[NR/2+1])/2; printf "%.2f", (a[NR]-a[1])/m*100}'; }

self_test() {
  local got
  got="$(median 5 1 3)"; [ "$got" = 3 ] || { echo "median of odd count: got $got" >&2; exit 1; }
  got="$(median 4 1 3 2)"; [ "$got" = 2.5 ] || { echo "median of even count: got $got" >&2; exit 1; }
  got="$(median 100)"; [ "$got" = 100 ] || { echo "median of one value: got $got" >&2; exit 1; }
  got="$(spread_pct 90 100 110)"; [ "$got" = 20.00 ] || { echo "spread: got $got" >&2; exit 1; }
  got="$(spread_pct 7 7 7 7 7)"; [ "$got" = 0.00 ] || { echo "spread of equal values: got $got" >&2; exit 1; }
  got="$(spread_pct 1200 1000 1100 1300 900)"; [ "$got" = 36.36 ] || { echo "spread of unsorted values: got $got" >&2; exit 1; }
  echo "perf-interleave self-test: ok"
}

[ "${1:-}" = "--self-test" ] && { self_test; exit 0; }

trunk="${1:?usage: perf-interleave.sh <trunk-sha> <head-sha> <metric> [--runs N]}"
head_sha="${2:?}"
metric="${3:?}"
runs=5
[ "${4:-}" = "--runs" ] && runs="${5:?}"
case "$metric" in import_ms|first_grid_ms|export_ms) ;; *) echo "unknown metric: $metric" >&2; exit 2 ;; esac

repo="$(cd "$(dirname "$0")/../.." && pwd)"
work="${PERF_WORK:-$(mktemp -d "${TMPDIR:-/tmp}/perf-interleave-XXXXXX")}"
mkdir -p "$work"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$work/target}"
case "$(uname -s)" in MINGW*|MSYS*|CYGWIN*) exe=.exe ;; *) exe= ;; esac

build_side() {
  local sha="$1" override="$2" tree bin
  if [ -n "$override" ]; then echo "$override"; return; fi
  bin="$work/bin/$sha/emulsion$exe"
  if [ ! -x "$bin" ]; then
    tree="$work/tree-$sha"
    [ -d "$tree" ] || git -C "$repo" worktree add --detach "$tree" "$sha" >&2
    [ -e "$tree/app/node_modules" ] || ln -s "$repo/app/node_modules" "$tree/app/node_modules"
    (cd "$tree/app" && ./node_modules/.bin/tauri build --debug --no-bundle --features wdio-webdriver --config src-tauri/tauri.e2e.conf.json >&2)
    mkdir -p "$work/bin/$sha"
    cp "$CARGO_TARGET_DIR/debug/emulsion$exe" "$bin"
  fi
  echo "$bin"
}

trunk_bin="$(build_side "$trunk" "${PERF_BIN_TRUNK:-}")"
head_bin="$(build_side "$head_sha" "${PERF_BIN_HEAD:-}")"

probe() {
  local bin="$1" out home
  out="$(mktemp -d "$work/probe-XXXXXX")"
  home="$(mktemp -d "$work/home-XXXXXX")"
  (cd "$repo/app" && HOME="$home" APP_BIN="$bin" LANE_OUT="$out" ./node_modules/.bin/wdio run ./e2e/wdio.sandbox.conf.js --spec ./e2e/specs/perf-probe.e2e.js) >"$out/run.log" 2>&1 || { echo "probe failed, see $out/run.log" >&2; return 1; }
  node -e 'const m=JSON.parse(require("fs").readFileSync(process.argv[1]+"/perf.json","utf8"))[process.argv[2]]; if(typeof m!=="number") process.exit(3); console.log(m)' "$out" "$metric"
}

trunk_vals=()
head_vals=()
for i in $(seq 1 "$runs"); do
  if [ $((i % 2)) -eq 1 ]; then order="trunk head"; else order="head trunk"; fi
  for side in $order; do
    if [ "$side" = trunk ]; then v="$(probe "$trunk_bin")"; trunk_vals+=("$v"); else v="$(probe "$head_bin")"; head_vals+=("$v"); fi
    echo "pair $i $side $metric=$v" >&2
  done
done

tm="$(median "${trunk_vals[@]}")"
hm="$(median "${head_vals[@]}")"
echo "metric $metric, $runs pairs, alternating order"
echo "trunk $trunk values: ${trunk_vals[*]}"
echo "head  $head_sha values: ${head_vals[*]}"
echo "trunk median $tm spread $(spread_pct "${trunk_vals[@]}") percent"
echo "head median $hm spread $(spread_pct "${head_vals[@]}") percent"
awk -v h="$hm" -v t="$tm" 'BEGIN {printf "ratio head to trunk %.3f\n", h/t}'
