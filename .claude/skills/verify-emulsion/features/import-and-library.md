# Import and Library

A user brings photographs into the catalog and finds them again in the Library grid; double-clicking a photo opens Loupe, whose "Develop →" button opens Develop. Importing the same file twice does not create a second copy.

## Sub-features

- `import-files` imports files; the catalog gets an image and a version for each, and a duplicate is a no-op.
- `import-progress` a multi-file import emits one ordered progress event per candidate file, ending at (total, total).
- `library-grid` the imported photo appears as a cell in the virtualized, newest-first grid.
- `open-loupe-develop` double-click a cell for Loupe, then the Develop button.

## How to get to it (user POV)

- Toolbar "Import Files…" opens a native file picker (not drivable).
- Drag/drop and "Import Folder…" are not mapped.
- Library is the module shown at launch; the grid is `.grid-scroll`, each cell is `.cell` with a `.file-name` label.

## Driving it with the e2e harness

Preconditions: baseline from `README.md`; spec files `golden-path.e2e.js` (journey 1 and 2) and `import-progress.e2e.js`.

- **Import (picker bypassed).** Run `drive.sh golden-path`. The spec calls `import_files` with `test_image/Field-corn-Liechtenstein-landscape.jpg`, reloads the page, then looks for the cell. Observable result: first test `✓ imports a fixture photo and shows it in the Library grid`.
- **Find the cell.** `findCellByNameAnywhere("Field-corn-Liechtenstein-landscape")` scrolls `.grid-scroll` (dispatching `scroll`) until `.file-name` text matches.
- **Open Develop.** `openDevelopFor(name)` dispatches `dblclick` on the cell, waits for `.hud-develop-btn`, clicks it with `clickEl`. Result: Develop's `#exposure` slider exists (second test).
- **Progress events.** Run `drive.sh import-progress`. The spec imports every JPEG in `test_image/` and checks the `import-progress` event sequence. Observable result: `✓ emits one 'import-progress' event per candidate file, ending at (total, total)`.
- **Side-effect second view.** `catalog-view.txt` from `drive.sh` lists `images` and `image_versions` counts; a duplicate import leaves them unchanged.

## Gotchas

- The picker is bypassed. Do not report "Import Files… works" from these specs.
- After `browser.refresh()` the first WebdriverIO command can take ~20 s to settle; use the 60 s timeouts the specs use.
- The grid is virtualized; a cell outside the rendered window does not exist in the DOM until scrolled to.
- `golden-path` also contains the Export journey; run the whole file, it is cheap.
