# Develop edit and History

A user moves a slider in Develop; the edit is saved to the catalog after a short pause and recorded as a History entry. The first edit also records an "Original" entry so it can be undone. Cmd+Z undoes, Cmd+Shift+Z redoes, and clicking a History row jumps there.

## Sub-features

- `edit-save` a slider change is stored as the photo's edit stack.
- `history-rows` the edit adds a History row; the very first edit adds an `Original` row before it.
- `undo-redo` Cmd+Z restores the previous stack, Cmd+Shift+Z brings the edit back.
- `history-click-then-edit` jump to an earlier row, then edit: later rows are discarded. **Open finding** for revisited content, see Gotchas.

## How to get to it (user POV)

- Library, double-click a photo, "Develop →".
- Exposure is the `#exposure` range input; History rows render as `.entry-label` text in the History panel.
- Keyboard: Cmd+Z, Cmd+Shift+Z (Ctrl on non-macOS).

## Driving it with the e2e harness

Preconditions: baseline from `README.md`; spec `develop-undo-first-edit.e2e.js`. Needs no GPU.

- **Seed.** The spec imports `test_image/Smiling-woman-pink-shirt-portrait.jpg`, sets a known stack (`contrast 7`, `exposure 0.1234`) through `set_edit_stack`, reloads and opens Develop.
- **Edit.** Run `drive.sh develop-undo-first-edit`. It sets `#exposure` to 0.9 with the native value setter and an `input` event. Observable result: `get_edit_stack` reports exposure 0.9 and `get_history` is `["Original", "Exposure"]`; `.entry-label` contains `Original`.
- **Undo.** Dispatches `keydown` `z` with `metaKey` and `ctrlKey`. Observable result: stack equals the seeded baseline exactly.
- **Redo.** Dispatches the same with `shiftKey`. Observable result: exposure 0.9 again.
- **Side-effect second view.** `catalog-view.txt` lists `edit_history` rows `Original` then `Exposure` for the version. Proven passing in this setup on 2026-11-05 (1 passing).

## Gotchas

- Open finding (see skill `verify-edit-history`): in the Rust catalog, if an edit returns the photo to content an earlier History row already holds, then jumping to the first row and editing again leaves the abandoned rows in the list. Sequence: edit to A, edit back to the start, click `Original`, edit to A again; the list should be `Original, A` and the catalog gives `Original, A, (start), A`. Not yet reproduced through the UI; a journey for it will report failure until the code is fixed. Never weaken the journey to make it pass.
- Edits are saved after a ~250 ms debounce. Wait for `get_edit_stack` to change, never a fixed sleep.
- Keyboard focus is unreliable; shortcuts are dispatched as `KeyboardEvent` on `window`.
- Seed a stack no earlier run left a history row for, or the first edit will not record `Original`.
