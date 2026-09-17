# ADR-0008: Plugin/extensibility API v0 — fire-and-forget, no-shell, self-configured export-plugin hook

- Status: Accepted
- Date: 2026-09-17
- Relates to: [ADR-0005](ADR-0005-catalog-storage.md), [RFC-0006](../rfc/RFC-0006-export-plugin-hook.md), [PRD MILESTONES §M5](../../PRD/MILESTONES.md#m5--performance-gpu-merges-faces), [PRD MILESTONES §M8](../../PRD/MILESTONES.md#m8--polish-extensibility-10-launch)

## Context

M5 scopes a "plugin/extensibility API v0," explicitly framed as small enough to de-risk M8's later work to "harden the plugin/extensibility API from M5 into something documented and stable enough for third parties." [RFC-0006](../rfc/RFC-0006-export-plugin-hook.md) chose the narrowest useful instance of that surface: a **post-export hook**, matching Lightroom's own long-standing Export Actions feature — after a file is exported, optionally hand it off to a user-configured external command.

No existing ADR governs invoking an external process: every prior feature in this codebase (RAW decode, HDR/panorama merge, face detection) is in-process Rust or in-webview WebGPU ([ADR-0003](ADR-0003-raw-decoding.md), [ADR-0004](ADR-0004-rendering-and-color-management.md), [ADR-0007](ADR-0007-face-detection-and-recognition.md)). This is the first feature to cross that boundary, which is why it warrants its own ADR rather than a dated update to an existing one.

## Decision

Add a single **post-export hook**: an `export_plugins` catalog table (name, command, JSON-encoded argument template) following the existing `presets` table's precedent, plus an optional `plugin_id` on `ExportOptions`. After each file in an export batch is successfully written, `export_batch` substitutes three fixed placeholders (`{path}`, `{dir}`, `{filename}`) into the configured argument template and invokes the command via `std::process::Command::new(...).args(...).spawn()` — never a shell string — without waiting for it to finish.

## Rationale

- **Narrowest useful extensibility point, not a plugin platform.** No in-process plugin loading, no new IPC surface, no new trust model beyond "the user configured a command they already trust to run on their own machine" — the same boundary any local script or shell alias already sits at. This deliberately establishes the *first instance* M8 can later harden, rather than trying to design a finished third-party plugin system now.
- **`Command::new(...).args(...)`, never `sh -c`/`cmd /C`.** This is a correctness property, not a sandboxing claim: a filename or configured argument containing shell metacharacters is passed to the child process exactly as configured, never reinterpreted by a shell. It does not make the invoked command safe to run — see the accepted trust boundary below — it only prevents an unrelated class of injection bug.
- **Fire-and-forget (`.spawn()`, not `.output()`/`.status()`).** The export flow never blocks on or depends on the plugin process's completion, so a slow or hanging plugin cannot stall or fail an export. This also removes the need for any timeout/hang-handling logic — there is nothing to time out. A `spawn()` failure (bad command, not found, not executable) is surfaced via a new `plugin_error` field kept separate from `ExportResult`'s existing `error` field, so a plugin misconfiguration never makes an otherwise-successful export look like a failure.
- **No sandboxing, signing, or capability restrictions in v0 — an accepted, explicit trust boundary, not an oversight.** The invoked command runs with the same OS privileges as the app itself. This is acceptable specifically because v0 plugins are entered by hand by the user in Settings — nothing is downloaded, installed, or run without the user having typed the exact command themselves. It is explicitly **not** an acceptable boundary for third-party-distributed plugins, which is exactly why M8's own scope is to harden this into something sandboxed/verified before any third-party distribution story exists.
- **One hook point (post-export), one plugin per export run.** Matches the PRD's own scoping language ("even a minimal export-plugin hook is useful here") and avoids building a plugin pipeline or multi-hook registry before there's a second real use case to generalize from.

## Consequences

- Any future hook point (pre-import, Develop-time, a custom render/mask plugin) is a new, separate decision — this ADR's trust model and invocation mechanism don't automatically extend to a hook with different timing or data-access needs, and each should be evaluated on its own before reuse is assumed safe.
- **M8's hardening work has a concrete, named starting checklist** rather than needing to rediscover these tradeoffs from scratch: a manifest/versioning format, sandboxing or at minimum a permissions prompt before a configured command first runs, signing/verification if plugins are ever distributed rather than hand-typed, and consideration of additional hook points beyond post-export.
- Because invocation is fire-and-forget, the app has no way to report a plugin's own success/failure, output, or side effects back to the user — this is a deliberate v0 scope boundary, not a missing feature, and should be stated plainly in any user-facing documentation of this feature.
- `export_plugins` follows `presets`' existing CRUD/table shape, keeping the catalog schema's conventions consistent ([ADR-0005](ADR-0005-catalog-storage.md)) rather than introducing a new storage pattern for what is structurally the same "list of named, opaque-payload entries" shape.

## Alternatives considered and rejected

- **A broader in-process plugin system (dynamic loading, WASM, or a scripting-language sandbox) for v0**: rejected — over-scoped for what M5 asks for ("even a minimal export-plugin hook is useful here"), and would front-load exactly the manifest/sandboxing/versioning design work that M8 is explicitly scoped to own later, without a second real use case yet to generalize from.
- **Waiting on the plugin process and surfacing its exit code/output in the UI**: rejected for v0 — turns a fire-and-forget hook into a blocking step with its own timeout/hang-handling and UI surface, adding real complexity for a v0 whose only job is to prove the mechanism works.
- **Shelling out via `sh -c`/`cmd /C` to allow shell syntax (pipes, redirection) in the configured command**: rejected — reintroduces shell metacharacter reinterpretation of the exported filename/args for a convenience v0 doesn't need; a user who wants shell composition can point `command` at their own wrapper script instead.
