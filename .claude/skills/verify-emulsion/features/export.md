# Export

A user exports the photo open in Develop to a JPEG on disk. The Export button is enabled once a Develop photo is open and opens an export dialog.

## Sub-features

- `export-button` the `.export-btn` enables when a photo is open in Develop and opens `.dialog`.
- `export-file` exporting writes `<photo name>.jpg` in the destination folder.

## How to get to it (user POV)

- Develop, then the Export button in the toolbar. The destination folder is chosen with a native picker (not drivable).

## Driving it with the e2e harness

Preconditions: baseline from `README.md`; spec `golden-path.e2e.js` (third test, it depends on the first two).

- **Open the dialog.** `drive.sh golden-path`. Waits for `.export-btn` to be enabled, clicks it with `clickEl`, waits for `.dialog`. Observable result: `.dialog` exists.
- **Export (picker bypassed).** Calls `export_images` with `{ destination_dir: <tmp dir>, long_edge: null, quality: 90 }`. Observable result: no `error` in the result and the file `<tmp dir>/Field-corn-Liechtenstein-landscape.jpg` exists.
- **Second view.** The file on disk is the independent proof. The spec writes it to `emulsion-e2e-*` under the system temp dir (`os.tmpdir()`, so `$TMPDIR`), which `drive.sh` does not delete. Right after the run: `ls -l "${TMPDIR:-/tmp}"/emulsion-e2e-*/Field-corn-Liechtenstein-landscape.jpg` and `file` it (expect JPEG image data). Remove those temp dirs yourself when done.

## Gotchas

- The destination picker and the dialog's own Export confirm are not driven; the file is written by the same backend command, called directly. Do not report the dialog's confirm button as verified.
- Export does a full decode and re-encode; the spec sets a 90 s script timeout.
- The test depends on the earlier tests in `golden-path` (the photo must be imported and open in Develop). Run the whole file.
