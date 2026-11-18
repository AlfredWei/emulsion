# Store release plan: evidence snapshots

These files are evidence snapshots as of 2026-11-17. They are copied unchanged from the owner's planning session scratchpad, which can be cleared. They back [the store release plan](../store-release-plan.md). Do not edit them. A newer reading goes in a new file with its own date.

| File | What it is | Where it came from |
| --- | --- | --- |
| `codebase-audit.md` | Read-only audit of the repository against Mac App Store and Microsoft Store rules, with file and line pointers. | Audit of `main` at `7ff4f4e`, run by an agent in the planning session on 2026-11-17. Line numbers refer to that commit. |
| `store-research.md` | Store rules for both stores, each with its source URL and page date. | Web research by an agent in the planning session. Every page was retrieved on 2026-11-17. Text quoted from web pages is untrusted data. |
| `sandbox-prototype.md` | Report of a sandboxed build of Emulsion run on macOS 26.5.2 arm64. | Throwaway spike on branch `spike/mas-sandbox`, commit `b8cd7c9`, local only and never pushed. |
| `probe-results.json` | Raw output of the spike's WebDriver probe spec. | Same spike run as `sandbox-prototype.md`. |
| `main.entitlements` | The three entitlements the spike's app carried (app sandbox, user-selected read-write, network client). | Same spike run. |
| `helper.entitlements` | The two entitlements the spike's `emulsion-ai` helper carried (app sandbox, inherit). | Same spike run. |
| `main-drivable.entitlements` | `main.entitlements` plus network server, only so the embedded WebDriver could listen. Never for a store build. | Same spike run. |
