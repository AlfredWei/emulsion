# Emulsion verification map

This directory is the maintained source for verifying the user-facing behavior of Emulsion in the real window. Read this index before driving the app, then use the matching feature file as the recipe.

## Baseline preconditions

- `.claude/skills/verify-emulsion/doctor.sh` exits 0 (binary built with the WebDriver plugin and newer than the sources; no other instance running).
- Every run goes through `.claude/skills/verify-emulsion/drive.sh <spec>`, which uses a throwaway `HOME`, so the catalog starts empty. Specs import their own photos from `test_image/`.
- Never drive an instance that this run did not start.

## Driving conventions

- The harness is WebdriverIO (`app/e2e/`) against the debug binary built with `--features wdio-webdriver`. The recipe for a journey is the spec named in the feature file plus the stable handles listed there.
- Native OS pickers (Import Files…, Export destination, Choose Folder…) cannot be driven. The specs call the same backend command through `window.__TAURI__.core.invoke`; a proof must say so.
- Clicks and keys go through dispatched DOM events and the helpers in `app/e2e/helpers.js`; plain WebDriver clicks are unreliable here.
- Prefer `data-testid` and the listed ids/classes over coordinates.
- Specs that need WebGPU report their own skip or disabled-state assertion on a runner without it.

## Proof and skip reporting

- Capture the user action and the resulting state: `run.log` for the steps, `catalog-view.txt` (read-only `sqlite3`) or a file on disk as the second view of side effects.
- Record the feature ID and spec name with every artifact (artifacts are under `.claude/skills/verify-emulsion/artifacts/<run-id>/`).
- Report an unreachable path with the attempted command and the unmet precondition (no GPU, no model, native picker).
- Do not report a path as verified through a different path (a backend `invoke` is not the picker).

## Feature entry contract

Each feature file starts with an H1 title and one paragraph describing the user-visible behavior. It then uses exactly four H2 sections in this order: `Sub-features`, `How to get to it (user POV)`, `Driving it with the e2e harness`, `Gotchas`.

## Features

- [Import and Library](./import-and-library.md): bring photos in, find them in the grid, open them in Loupe and Develop.
- [Develop edit and History](./develop-edit-history.md): change a slider, see it stored, undo and redo through History. Carries an open finding.
- [Export](./export.md): export the open Develop photo to a JPEG file.
- [Remove tool](./remove-tool.md): the object-removal tool's controls, in its no-model and no-GPU states.
- [Similar view: the eye check](./similar-eyes-check.md): the Eyes closed / One eye closed badges on the frames of an opened group.

Existing specs not yet mapped (each has a header comment saying what it proves): `develop-auto-sync`, `develop-multiselect`, `develop-mask-shapes`, `develop-select-subject`, `develop-adaptive-presets`, `library-adaptive-batch`, `hdr-merge`, `storage-settings`, `panel-resize`, `context-menu`, `develop-module-roundtrip`, `develop-cpu-gpu-parity`, `develop-performance`.
