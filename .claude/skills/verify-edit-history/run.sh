#!/usr/bin/env bash
# Runs the bounded exhaustive check of the Develop edit history (see SKILL.md).
# Usage: .claude/skills/verify-edit-history/run.sh
# Exit 0 = both properties hold; non-zero = at least one counterexample (printed).
set -uo pipefail
repo="$(cd "$(dirname "$0")/../../.." && pwd)"
cd "$repo/app/src-tauri"
# --ignored: the checks are #[ignore]d so `make test` stays fast. --test-threads=1: readable output.
cargo test --lib edit_history_check -- --ignored --test-threads=1 --nocapture 2>&1 \
  | grep -E "alphabet|sequence:|after step|^test result|error(\[|:)" 
exit "${PIPESTATUS[0]}"
