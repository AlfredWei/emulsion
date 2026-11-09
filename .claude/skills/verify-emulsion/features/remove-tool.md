# Remove tool

In Develop, the Remove tool lets a user mark an object to erase from the photo. Without the removal model installed, or without a GPU canvas, the tool says so and does not let the user draw marks it could not act on.

## Sub-features

- `remove-tool-button` `[data-testid="remove-tool"]` is present in Develop.
- `remove-no-gpu` on a CPU-fallback canvas the button is disabled and its title contains `unavailable without GPU`.
- `remove-no-model` on a GPU canvas, arming the tool shows `[data-testid="remove-unavailable"]` containing `not installed`, and the marking layer `[data-testid="remove-layer"]` appears.
- `remove-no-marks` dragging on the layer draws nothing and `[data-testid="remove-run"]` never appears.
- `remove-toggle-off` toggling the tool off removes the layer and writes no `remove` op to the edit stack.

## How to get to it (user POV)

- Library, double-click a photo, "Develop →", then the Remove tool button in Develop's tool strip.

## Driving it with the e2e harness

Preconditions: baseline from `README.md`; spec `develop-remove-tool.e2e.js`; the removal model is NOT installed (the default for a fresh machine and for CI).

- **Run.** `drive.sh develop-remove-tool`. It imports `Smiling-woman-pink-shirt-portrait.jpg`, opens Develop, waits for the canvas to settle (CPU-fallback badge `.cpu-fallback-badge`, or a GPU canvas with the tool enabled).
- **CPU fallback branch.** Observable result: button disabled, title contains `unavailable without GPU`, no layer. The spec returns early; `remove-no-model` and `remove-no-marks` are then **not exercised**.
- **GPU branch.** Clicks the tool, expects the `not installed` message and the layer, dispatches pointer events on the layer, expects no marks and no run button; second test toggles off.
- **Second view.** `catalog-view.txt` shows no `edit_history` row for a `remove` op. Optionally `get_edit_stack` through the spec shows no `remove` op.
- **With the model.** `INPAINT_MODEL=<path>/lama_fp32.onnx drive.sh develop-remove-tool-real-model` (macOS, GPU; the spec copies the model into the throwaway HOME). Recorded 2026-11-07: 13 passing in 4 min 8 s with `SEGMENT_MODELS_DIR` (the folder holding `sam2_hiera_tiny.encoder.onnx` and `.decoder.onnx`; 11 without, the two Select steps skipped) (the first attempts can hit the webview stall in `before all`; rerun). Journey: paint a stroke, Remove, lasso a second removal, large-area question and Clear, Undo last, add another and delete the *first* by its `remove-delete` chip (`data-patch`), leave the tool, then re-arm, paint a stroke and press `remove-heal` (expect a `spot_mask` with `mode: heal`, no new removal, the tool left, last History row `Heal instead`), then re-arm and press the `remove-heal-removal` chip (`data-patch`) of the remaining removal: it leaves the stack, a second heal `spot_mask` appears with dabs on the lasso's place, last row `Heal instead`. Observable results are `get_edit_stack`'s `remove` items, the canvas's `data-removals-source`, and `get_history`'s last label.
- **"No longer fits".** `INPAINT_MODEL=<path>/lama_fp32.onnx drive.sh develop-removal-no-longer-fits` (macOS, GPU; 2 passing in about 2 min 16 s, recorded 2026-11-09). A real removal, then `sqlite3` rewrites the patch's `src_w` / `src_h` in the throwaway catalog and the photo is reopened: `remove-problem` (`data-patch`) reads "no longer fits", `remove-undo` clears it. It does not replace the file (Develop would not notice; RFC-0032 §23).

## Gotchas

- Which branch ran depends on the machine's WebGPU. Report which one you saw.
- The marking layer lives in the GPU canvas's mask overlay; without GPU there is nothing to mark.
- Click-to-select inside the tool (branch `feature/remove-click-select`) is not covered by this spec.
