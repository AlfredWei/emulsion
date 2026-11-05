---
name: verify-emulsion
description: Drive the real Emulsion desktop app (Tauri + Svelte, macOS WKWebView) the way a user does and capture proof that a change works. Use after any change to Library, Develop, History/undo, Remove tool, Export or import, when you need evidence from the running app and not just unit tests.
---

# Verify Emulsion by driving the real app

Surface: a Tauri desktop window (Rust core + Svelte UI in the OS webview). There is no URL to open and no CLI to call. The only scripted way to drive the real window is the repo's own **WebdriverIO suite** (`app/e2e/`), which talks to a WebDriver server embedded in a special debug build. This skill wraps that suite so a run is isolated, repeatable and leaves evidence.

Feature recipes: `features/README.md` (read it first, then the matching feature file).
Other proof, not this skill: `verify-edit-history` (exhaustive check of undo/redo history logic in Rust).

## Launch

The app is launched **by the test runner**, not by you. Two steps:

1. **Build the drivable binary** (only when `doctor.sh` says it is missing or older than the sources):

   ```bash
   .claude/skills/verify-emulsion/build.sh
   ```

   This is the build half of `npm run test:e2e`: `tauri build --debug --no-bundle --features wdio-webdriver --config src-tauri/tauri.e2e.conf.json`, producing `app/src-tauri/target/debug/emulsion`. A binary from `make dev` or `npm run tauri dev` has no WebDriver server and cannot be driven. Incremental builds take minutes; a cold build is much longer.

2. **Run a spec** (starts the window, drives it, closes it):

   ```bash
   .claude/skills/verify-emulsion/drive.sh develop-undo-first-edit
   ```

   Ready means the spec reporter prints `✓` lines for the app, since the runner waits for the embedded driver and the Tauri bridge. The first command after a page reload can take ~20 s to settle (see `app/e2e/helpers.js`); that is normal, not a hang.

**Empty data dir, every run.** `drive.sh` sets `HOME` to a throwaway directory, so the app's catalog (`~/Library/Application Support/dev.alfredwei.emulsion/catalog.sqlite`) starts empty, migrations and default presets run on first open, and your real catalog is never touched. (`npm run test:e2e` run directly does NOT do this: it reuses and grows the real dev catalog.) Specs import their own fixtures from `test_image/`.

**Teardown** is done by `drive.sh`: it deletes the throwaway HOME and kills only an app process that outlived the spec.

## Doctor

```bash
.claude/skills/verify-emulsion/doctor.sh
```

Read-only. Run it first, and again whenever anything looks off. It checks: the binary exists and contains the WebDriver plugin, the binary is newer than every file under `app/src`, `app/static`, `app/src-tauri/src`, `Cargo.toml` and `tauri.conf.json`, `wdio` and `sqlite3` are installed, fixtures exist, and **no instance of the binary is already running**. `drive.sh` refuses to start if it fails.

## Drive

`drive.sh <spec>` runs one spec from `app/e2e/specs/` (name without `.e2e.js`, or a path). The feature files list which spec proves which journey. Conventions the specs rely on, and that any new probe must follow:

- **Call backend commands with** `browser.execute((c, a) => window.__TAURI__.core.invoke(c, a), "get_edit_stack", { versionId })`. `window.__TAURI__` exists because the e2e build sets `withGlobalTauri`.
- **Native file pickers cannot be driven** (Import Files…, Export destination, Choose Folder…). Specs call the same backend command the button would (`import_files`, `export_images`, `set_cache_dir`) and drive everything else through the real UI. Say so in a proof; never claim the picker was exercised.
- **Clicks and keys:** WebdriverIO `.click()` and keyboard input are unreliable in this WKWebView embedding. Use `clickEl()` / `openDevelopFor()` / `findCellByNameAnywhere()` from `app/e2e/helpers.js`, set a slider with the native `value` setter plus a dispatched `input` event, and send shortcuts as a dispatched `KeyboardEvent` (`metaKey: true, ctrlKey: true`).
- **Stable handles:** `data-testid` attributes (`remove-tool`, `remove-layer`, `remove-run`, ...), `#exposure`, `.export-btn`, `.hud-develop-btn`, `.entry-label` (History rows), `.module-switch button`, `[data-ctx-photo="<versionId>"]` (filmstrip/grid cell), `.file-name`. Prefer these over coordinates.
- **Waiting:** poll a DOM or backend condition with a deadline (`waitFor` in the specs). Do not use fixed sleeps. Prefer `browser.execute` polling over `$()` queries in loops: every `$()` pays a ~5 s Tauri-bridge check in this setup.
- **GPU:** Develop's canvas uses WebGPU and falls back to CPU rendering. Features that need the GPU canvas skip or assert the disabled state on a runner without it; the spec output says which happened. A "skipped (no GPU)" is not a pass for the GPU path.
- **Models:** specs named `*-real-model*` need downloaded ONNX models and run locally only. The default specs assert the "model not installed" behaviour.

New probe: copy the closest existing spec, keep it under `app/e2e/specs/` so it shares `helpers.js`, and run it with `drive.sh`.

## Evidence

Each `drive.sh` run keeps `.claude/skills/verify-emulsion/artifacts/<run-id>/` (cleanup never deletes it):

| File | What it proves |
|---|---|
| `run.log` | Every step and assertion of the spec, pass or fail |
| `catalog-view.txt` | Read-only `sqlite3` view of the catalog after the run: image/version counts and `edit_history` rows. This is the independent second view of side effects |
| `meta.txt` | Commit, binary build time, spec, exit code |
| `doctor.txt` | The doctor output at start |

Proof standards:

- Exercise the real user path: UI elements and the same Tauri commands the buttons call. Do not write the catalog directly, and do not call test-only endpoints.
- A proof states the action and the resulting state (spec log), and the side effect read from outside the UI (`catalog-view.txt`, or a file on disk for Export).
- State what was bypassed (native pickers) and what was skipped (GPU, models).
- Mocks: none. The suite runs the real binary against a real SQLite file. External geocoding is the only network feature and no mapped journey uses it.
- A dry run is not a proof. If a spec says "skipped" or "pending", report that, not "verified".

## Cleanup

`drive.sh` removes the throwaway HOME and any app process left by the run. If you aborted a run by hand, run `doctor.sh`: if it reports a running instance that you started, stop that PID yourself. Do not `pkill` by name; an instance you did not start belongs to someone else.

## Not isolated

Runs are sequential (`maxInstances: 1` in `app/e2e/wdio.conf.js`; do not start two `drive.sh` at once). `HOME` isolation was confirmed for the catalog (the real catalog's file mtimes were unchanged across a run). It was not checked whether the OS webview keeps other state (WebKit storage) outside `HOME`; no mapped journey depends on it.
