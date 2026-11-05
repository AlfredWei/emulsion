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
- **With the model.** `develop-remove-tool-real-model.e2e.js` does the real removal; it needs the downloaded model and is local-only. Not mapped here until a run on a machine with the model has been recorded.

## Gotchas

- Which branch ran depends on the machine's WebGPU. Report which one you saw.
- The marking layer lives in the GPU canvas's mask overlay; without GPU there is nothing to mark.
- Click-to-select inside the tool (branch `feature/remove-click-select`) is not covered by this spec.
