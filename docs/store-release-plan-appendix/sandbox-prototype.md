# Emulsion under the macOS App Sandbox: prototype report

**Goal.** The owner wants to sell Emulsion on the Mac App Store, which requires the App Sandbox. The biggest unknown was whether this app, a local-first RAW catalog that reads image files from anywhere on disk, runs at all under the sandbox and what has to change. This report answers it from a sandboxed build that was run, not from docs. The prototype is throwaway. Nothing was pushed or merged.

**Verdict.** It runs. Start-up, data relocation, WebGPU, the AI helper, models, network and export-inside-the-container all work with three entitlements. The two things that break are reading photos outside the container and writing exports outside it, and the catalog stores absolute paths, so a security-scoped bookmark layer is the large change.

## Setup (measured)

- Machine macOS 26.5.2 arm64. Branch `spike/mas-sandbox`, commit `b8cd7c9` (only `app/e2e/specs/sandbox-probe.e2e.js` and `app/e2e/wdio.sandbox.conf.js`; no app code changed). Worktree `<home>/src/claude_cowork_projects/lr_replace/.claude/worktrees/agent-a65c05f5c7d6da5bb`.
- Models. The earlier run's cache was not found. The main checkout's `app/src-tauri/bundled-models/` held 4 of 7 models (nafnet, realesr, sam2 encoder and decoder). I copied those and skipped the fetch script, so dfine, lama and face landmarks are absent from these bundles. Denoise and segment status report `ready`.
- Signing identity. `Apple Development: <signing identity> (<team id>)` was usable with no prompt, so I used it, not ad hoc. Hardened runtime and notarisation were not tried.

## Commands run

```
cd app
node scripts/prepare-ai-helper.mjs
npx tauri build --config src-tauri/tauri.bundle-models.conf.json --config src-tauri/tauri.bundle-helper.conf.json
# drivable variant (embedded WebDriver), same bundle configs
npx tauri build --features wdio-webdriver --bundles app --config src-tauri/tauri.e2e.conf.json \
  --config src-tauri/tauri.bundle-models.conf.json --config src-tauri/tauri.bundle-helper.conf.json
# sign inside out
ID="Apple Development: <signing identity> (<team id>)"
codesign --force -s "$ID" --entitlements helper.entitlements Emulsion.app/Contents/MacOS/emulsion-ai
codesign --force -s "$ID" --entitlements main.entitlements Emulsion.app          # drive.app used main-drivable.entitlements
codesign --verify --deep --strict Emulsion.app
# run the probe
PROBE_OUT=... PROBE_OUTSIDE=... PROBE_CONTAINER=~/Library/Containers/dev.alfredwei.emulsion/Data/Documents \
  APP_BIN=drive.app/Contents/MacOS/emulsion ./node_modules/.bin/wdio run ./e2e/wdio.sandbox.conf.js
```

Entitlements files beside this report: `main.entitlements` (app-sandbox, files.user-selected.read-write, network.client), `helper.entitlements` (app-sandbox, inherit), `main-drivable.entitlements` (main plus network.server, only so the embedded WebDriver on port 4445 can listen; not for the store).

The unsigned tauri output is signed `adhoc,linker-signed` with no Info.plist bound. Re-signing binds Info.plist, so the identifier becomes `dev.alfredwei.emulsion` (measured).

## Sandbox applied (measured)

- `codesign -d --entitlements :-` shows the three keys on the app and the two on `emulsion-ai`.
- `~/Library/Containers/dev.alfredwei.emulsion` appeared on first launch. I could not delete it afterwards (`Operation not permitted`, SIP protected), so it persists as the sandbox container.
- Denial capture. `log stream` and `log show` with `eventMessage CONTAINS "deny" AND eventMessage CONTAINS "emulsion"` (use `/usr/bin/log`; zsh's builtin `log` shadows it) caught one line. File-access denials for paths outside the container were not logged at all, although the operations failed with EPERM. So absence of a log line is not evidence of access.

```
Sandbox: emulsion(26483) deny(1) mach-lookup com.apple.Safari.SafeBrowsing.Service
Sandbox: emulsion(26884) deny(1) process-exec* .../Containers/dev.alfredwei.emulsion/Data/Documents/mytool
```

## Checks

| Check | Result | Evidence |
|---|---|---|
| App starts | pass | Window up, catalog opened, two imported photos still shown after a quit and relaunch (screenshot of the relaunched sandboxed app). |
| Data dir logic | pass | `app_data_dir()` (`lib.rs:2314`) resolves to `~/Library/Containers/dev.alfredwei.emulsion/Data/Library/Application Support/dev.alfredwei.emulsion/` holding `catalog.sqlite(+wal,shm)`, `thumbnails/`, `previews/`, model dirs. No code change needed. A fresh install starts empty, so an existing non-store catalog is not seen (guess: users must migrate by hand). |
| Bundled models and edition | pass | `segment_model_status` and `denoise_model_status` returned `ready` from `Contents/Resources/models`. |
| GPU (WebGPU in WKWebView, not wgpu) | pass | `navigator.gpu.requestAdapter()` and `requestDevice()` succeeded (`apple/apple`). The GPU path is the webview's, so no Metal entitlement was needed. |
| `emulsion-ai` helper spawns and runs | pass | `ai_helper_info` went `running:false` to `running:true`; `segment_prepare` returned `encode_ms 1247` using the sam2 models, so it also loaded a model. Helper signed with `inherit`. Spawn is `supervisor.rs:68-75`, path from `current_exe()` at `supervisor.rs:177`. |
| Import from a path outside the container, no dialog | fail | `import_files` on a JPEG and a CR2 in `/private/tmp/...` and in a folder under `$HOME` gave `failed:1` each. The same files copied into the container imported fine. Cause is inferred as EPERM from the sandbox (no log line). |
| Import folder outside the container | fail, silent | `import_folder` returned `imported:0, failed:0`. `read_dir` failure is swallowed at `import.rs:53` (`let Ok(entries) = ... else`), so the user would see an empty import and no error. |
| Import through the file-open dialog (Powerbox grant) | not tested | Needs a human click. The Library "File" button is a canvas in the web view and background clicks are refused; File > Import Files... was disabled in the menu when driven. Not tried with full-screen control. Whether a grant survives a relaunch is therefore also untested (inferred: it does not without a bookmark, per Apple's model). |
| Export to a folder outside the container | fail | `export_images` gave `error: "Operation not permitted (os error 1)"` from `create_dir_all`/write (`export.rs:272`, `write_unique` at `export.rs:102`). Nothing was written. |
| Export inside the container | pass | JPEG written, `error:null`. |
| Export to a user-chosen folder via save panel | not tested | Same reason as the import dialog. Inferred to work for the session because `files.user-selected.read-write` is set. |
| Export-plugin hook, system binary | pass | `/usr/bin/touch` ran and created the marker inside the container (`export_plugin.rs:49`). |
| Export-plugin hook, user's own program | fail | A non-system binary (`touch` copied into the container) gave `plugin_error: Operation not permitted`, denial `process-exec*`. Export itself still succeeded. Any program the user installs (a script, a tool in `/usr/local`) will fail the same way (inferred). |
| Place search (Nominatim) | pass | `geocode_search "Singapore"` returned 2 results (`geocode.rs:182`). |
| Map tiles | pass | `Image` load of `tile.openstreetmap.org/0/0/0.png` in the webview loaded (256 px). Both needed only `network.client`. |
| Model load | pass for 4 models | See helper row. dfine, lama and face landmarks not bundled here, so not tested. |
| Thumbnails and previews location | pass | `thumbnail_path` rows point into the container's `thumbnails/`. Settings > Storage custom location (`storage.rs:83` `move_cache_dir`) was not exercised; it points anywhere on disk and would hit the same wall (inferred). |
| Info.plist or launch problems | pass | None seen. WKWebView, drag, IPC worked with no extra entitlement. The SafeBrowsing mach-lookup denial looked harmless (inferred; page loaded). |

## What the store build must change, ranked by size

1. **Security-scoped bookmarks for every photo and folder (large).** The catalog stores absolute paths (`list_images` shows `path`) and every later read goes straight to disk: import (`import.rs:303`, `:397`), thumbnails, previews, Develop decode, export source. After a relaunch no outside path is readable without a stored bookmark. Needs a bookmark table in the catalog, a startable access scope around each path, handling of moved or stale bookmarks, and a "Reconnect folder" flow. Folder-level bookmarks on the imported root are the smallest workable unit. Sidecar and neighbour-file reads (RAW + XMP, HDR brackets, panorama frames) need the folder grant, not just the file.
2. **Silent failure on denied reads (medium).** `import.rs:53` swallows `read_dir` errors and reports success with zero files. Surface the error, at least on the store build.
3. **Exports and cache moves outside the container (medium).** Export destination must come from the save panel in the same session, or from a stored bookmark for "same folder as last time" (`export.rs:272`). Settings > Storage `move_cache_dir` (`storage.rs:83`) and catalog backup to a folder (`catalog/backup.rs:140`) need the same treatment, or drop the custom cache location on the store build.
4. **Export-plugin hook (small to medium).** `export_plugin.rs:49` cannot start a user's own program. Options: drop the feature on the store build (smallest), limit it to system binaries, or redesign around a user-supplied helper inside the bundle or an XPC/Shortcuts hand-off. Today it fails with a plugin error but does not break the export.
5. **Entitlements and packaging (small).** Keep the three entitlements. Use Apple Distribution and installer certificates, a provisioning profile, `com.apple.application-identifier` and team id, and `com.apple.security.cs.*` only if hardened runtime needs it (guess; not tried). Do not ship `network.server`. Sign `emulsion-ai` with `inherit`; the real MAS build needs the helper to carry the same team id.
6. **Migration and first-run notes (small).** The sandbox gives an empty catalog; an existing non-store catalog is invisible. Say so in the store listing or add an import-from-backup step.
7. **Dialog-granted flow and relaunch persistence (verify next).** Not tested here. This is the first thing to prove once a bookmark layer exists: grant via picker, quit, relaunch, open the photo.

## Not verified

Powerbox grants and their persistence, the save panel, Settings > Storage move, backup, print, HDR and panorama merge, models not bundled here (dfine, lama, face), hardened runtime, notarisation, App Store validation (`altool`/Transporter), and the real Mac App Distribution signing. The helper's own network and file needs under `inherit` were only exercised through `segment_prepare`.

## Artifacts

Entitlements: `<scratchpad>/store-plan/{main,helper,main-drivable}.entitlements`. Raw probe output: `.../store-plan/probe-results.json`. Signed apps: `<tmp>/mas/{dev,drive}.app`. Build logs: `.../scratchpad/build.log`, `build2.log`.
