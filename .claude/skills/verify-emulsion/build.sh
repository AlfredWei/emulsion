#!/usr/bin/env bash
# Builds the debug app WITH the embedded WebDriver server (the build half of `npm run test:e2e`).
# Usage: .claude/skills/verify-emulsion/build.sh   (incremental; first build is slow)
set -euo pipefail
repo="$(cd "$(dirname "$0")/../../.." && pwd)"
cd "$repo/app"
exec ./node_modules/.bin/tauri build --debug --no-bundle --features wdio-webdriver --config src-tauri/tauri.e2e.conf.json
