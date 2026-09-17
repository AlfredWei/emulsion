# RFC-0006: Export-plugin hook (M5 plugin/extensibility API v0)

- Status: Draft — for review in PR (flips to Accepted once merged)
- Date: 2026-09-16
- Companion documents: [MILESTONES](../../PRD/MILESTONES.md#m5--performance-gpu-merges-faces), [MILESTONES §M8](../../PRD/MILESTONES.md#m8--polish-extensibility-10-launch), [PROGRESS.md](../../PROGRESS.md), [ADR-0005](../adr/ADR-0005-catalog-storage.md)

## 1. Problem

M5's scope names *"Plugin/extensibility API v0 (even a minimal export-plugin hook is useful here and de-risks M8's extensibility work)"* as its own independent track, alongside GPU rendering, HDR/panorama merge, and faces (all four shipped as Slices 1–6). M8 later commits to *"harden the plugin/extensibility API from M5 into something documented and stable enough for third parties"* — so this slice's real job is to establish the **first, smallest real instance** of an extensibility surface, one M8 can later harden, not to design a finished third-party plugin platform now.

The most direct, precedent-matching form of that first instance — and the one M8's own phrasing implies (an "export-plugin hook") — is Lightroom's own long-standing **Export Actions** feature: after a file is exported, optionally hand it off to an external program (a script or executable the user already has installed) for whatever they want to do next (upload, further conversion, a notification, anything). This is deliberately the narrowest useful extensibility point: it needs no in-process plugin loading, no new IPC surface between the plugin and this app's own data, and no new trust model beyond "the user configured a command they already trust to run on their own machine" — the same boundary every other local script or shell alias already sits at.

Today, `export.rs`'s `export_one`/`export_batch` (§1 of that file) write a JPEG and return; nothing downstream of the write ever runs. There is also no precedent anywhere in this codebase for invoking an external process — every existing feature (RAW decode, merges, face detection, rendering) is in-process Rust or in-webview WebGPU (ADR-0003/0004). This RFC is the first to cross that boundary, which is why it also proposes a new ADR (§5) rather than a dated update to an existing one.

## 2. Non-goals

- **No plugin marketplace, discovery, or bundled plugins.** v0 is entirely user-configured: a name, a command, and an argument template, entered by hand in Settings. Nothing is downloaded, installed, or run without the user having typed the exact command themselves.
- **No sandboxing, signing, or capability restrictions.** The invoked command runs with the same OS privileges as the app itself, unconstrained — acceptable specifically because the user is the one who configured it (see §1's trust-model framing), not because this is safe for arbitrary untrusted plugins. Real sandboxing/verification is explicitly M8's job ("harden... stable enough for third parties" implies third-party plugins need a stronger boundary than v0's self-configured one).
- **No hook points beyond post-export.** No pre-import hook, no Develop-time hook, no custom render/mask plugin. The PRD itself scopes v0 down to "even a minimal export-plugin hook."
- **No waiting on, or surfacing, the invoked process's exit code/stdout/stderr in the UI.** Invocation is fire-and-forget (§3.3) by construction — this also means no timeout/hang-handling logic is needed, since the export flow never blocks on the plugin process finishing.
- **No cross-platform script-runtime bundling.** `command` is resolved exactly as `std::process::Command::new` resolves it on that OS (an absolute path, or a name found on `PATH`) — no bundled interpreter, no `.bat`/`.sh` abstraction layer.
- **No documented, versioned "plugin API contract" for third parties.** That documentation/stability work is M8's named job. v0 has no manifest format, no plugin discovery convention, no compatibility guarantee across app versions — just a working mechanism to harden later.
- **One plugin per export run, not a per-file or per-plugin-type registry.** A batch export invokes the *same* user-selected plugin once per successfully exported file (§3.3) — not a plugin pipeline, not multiple plugins chained.

## 3. Design

### 3.1 Data model: `export_plugins` table

New table, following `presets`' exact precedent (`catalog.rs:606-611` — a named row plus one opaque templated payload, not the single-value `settings` KV pattern that `BackupSettings`/`cache_dir` use, since this needs an arbitrary-length *list* of named entries):

```sql
CREATE TABLE IF NOT EXISTS export_plugins (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    command TEXT NOT NULL,
    args_template_json TEXT NOT NULL,  -- JSON array of argument strings, see §3.2
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
```

`Catalog` methods, mirroring `presets`' own CRUD shape (`list_presets`/`add_preset`/`delete_preset`): `list_export_plugins() -> Result<Vec<ExportPlugin>>`, `add_export_plugin(name: &str, command: &str, args_template: &[String]) -> Result<i64>`, `update_export_plugin(id: i64, name: &str, command: &str, args_template: &[String]) -> Result<()>`, `delete_export_plugin(id: i64) -> Result<()>`.

### 3.2 Argument template: fixed placeholder set, substituted per exported file

`args_template` is a plain `Vec<String>` (e.g. `["--upload", "{path}"]`), stored as JSON. Exactly three placeholders are recognized, substituted via plain string replacement (no shell parsing, no globbing) against **one exported file at a time**:

- `{path}` — the exported file's full absolute path.
- `{dir}` — its parent directory.
- `{filename}` — its base filename (with extension).

A template argument containing no placeholder is passed through unchanged (e.g. a fixed flag like `--upload`). Deliberately a small, fixed set (not a general templating language) — exactly what a v0 hook needs to hand a plugin "the file that was just exported," nothing more; more placeholders can be added later without a schema change, since the template is just a string.

### 3.3 Invocation: fire-and-forget, after each successful `export_one`

`ExportOptions` (`export.rs:26-31`) gains `plugin_id: Option<i64>`. `export_batch` (`export.rs:146-162`) resolves the plugin (if any) once up front, and after each item's `export_one` call succeeds, substitutes the placeholders (§3.2) against that item's own output path and spawns the command:

```rust
std::process::Command::new(&plugin.command)
    .args(resolved_args)
    .spawn()  // not .output()/.status() -- deliberately not awaited
```

`Command::new(...).args(...)` — never a shell string handed to `sh -c`/`cmd /C` — so a filename or configured argument containing shell metacharacters can't be reinterpreted; this is a correctness property (arguments arrive at the child process exactly as configured), not a sandboxing claim (§2). `.spawn()` returns as soon as the OS has started the process; the export flow never waits on it, matching §2's fire-and-forget non-goal exactly — a slow or hanging plugin cannot stall or fail an export.

A `spawn()` error (bad command, not found, not executable) is real and worth surfacing, just not as an export failure: `ExportResult` gains a new `plugin_error: Option<String>` field, set only when `spawn()` itself errors, kept separate from the existing `error` field (which means "the export itself failed") so a plugin misconfiguration never makes an otherwise-successful export look like a failure to the existing frontend success/failure branching in `ExportDialog.svelte`.

### 3.4 Tauri commands + frontend

Four new commands (registered in the existing single `generate_handler!` list, `lib.rs:1778`), matching every existing list/CRUD command's shape (e.g. `list_presets`, `get_backup_settings`): `list_export_plugins`, `add_export_plugin`, `update_export_plugin`, `delete_export_plugin`. `export_images` (`lib.rs:1401-1423`) is unchanged in shape — `ExportOptions` already flows through untouched to `export_batch`, so `plugin_id` needs no new command parameter.

New `app/src/lib/api/exportPlugins.js`, one thin wrapper per command, following the project's established one-module-per-concern convention (`export.js`'s own header comment). Two small UI additions:

- **`SettingsDialog.svelte`**: a new "Export Plugins" section — a list of configured plugins (name, command, args template) with add/edit/delete, same interaction shape as the existing Backup settings section.
- **`ExportDialog.svelte`**: a "Run after export" dropdown, populated from `listExportPlugins()`, defaulting to "None" — sets `options.plugin_id` alongside the dialog's existing destination/long-edge/quality fields.

## 4. Testability

- **Rust unit tests** (`export.rs` and/or a small new `export_plugin.rs`):
  - Placeholder substitution is a pure string function — test each of `{path}`/`{dir}`/`{filename}`, a template with no placeholder, and a template using the same placeholder twice, all against a known fixed path, asserting the exact resulting `Vec<String>`.
  - `Catalog` CRUD round-trip for `export_plugins` (add → list → update → list → delete → list), matching `presets`' own existing test shape.
  - `spawn()` invocation itself, gated like every other real-environment test in this codebase (`EMULSION_TEST_RAW_SAMPLE`'s own pattern): spawn a trivial, universally-present command (e.g. `touch` on Unix — Windows equivalent noted as a follow-up, or skipped with a clear reason on that target) against a temp file path, and assert the *marker* file it creates exists afterward — proves the real wiring end-to-end without needing an actual third-party plugin.
  - A deliberately-bad `command` (nonexistent binary) confirms `spawn()` fails cleanly into `plugin_error`, and that the export's own `output_path`/`error` are unaffected by that failure.
- **No new e2e (WebdriverIO) spec for v0.** The new Settings CRUD UI and Export dialog dropdown are both thin, already-e2e-adjacent surfaces (Settings dialog and Export dialog both have manual-verification precedent from their own original slices) — verified interactively against a real built `.app` instead, same reasoning M5's merge RFCs used for their own thin frontend wrappers.

## 5. ADR: new ADR-0008 required, not a dated update

Unlike every other M5 slice so far, no existing ADR governs "invoking an external process" — this is the first time this codebase crosses that boundary (§1). A new **ADR-0008: Plugin/extensibility API v0** should be added once this ships, recording: the decision to start with a single post-export hook rather than a broader plugin system; the fire-and-forget, no-shell, no-sandbox execution model and why that's an acceptable v0 trust boundary (self-configured local commands, not third-party plugins); and an explicit "what M8 hardening needs to add" list (manifest/versioning, sandboxing or at least a permissions prompt, signing/verification, additional hook points) so that milestone starts from a documented decision instead of rediscovering these tradeoffs from scratch.

(Numbered ADR-0008, not ADR-0007 as originally drafted here: [RFC-0005](RFC-0005-face-detection-people-view.md) §6 independently recommended a new ADR for the face-detection/`tract` decision, and that slice shipped first, claiming [ADR-0007](../adr/ADR-0007-face-detection-and-recognition.md).)
