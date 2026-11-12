# Similar view: the eye check

Opening a group in the Similar view asks the eye check about its frames; a frame whose face has both eyes closed gets an **Eyes closed** badge and one with a single eye closed gets **One eye closed**. Faces the check cannot judge (small, or the mesh unsure) get nothing.

## Sub-features

- Badge per frame in the group's Survey view (`data-testid="eyes-badge"`), text "Eyes closed" or "One eye closed" with a count above 1.
- Header hint "Checking eyes…" while the check runs, and a reason when a model is missing (the face detector, which a face scan fetches).
- Stored answers: asking again serves the stored rows (`cull_blink` table), it does not measure again.

## How to get to it (user POV)

Library rail > Similar > open a group.

## Driving it with the e2e harness

`YUNET_MODEL=<face_detection_yunet_2023mar.onnx> drive.sh similar-eyes-check` (macOS, local only; 1 passing in about 7 s, recorded 2026-11-12). The spec copies the detector file into the throwaway HOME's `face_models/`, imports two copies of the portrait (the second with one byte appended so it is not a duplicate), gives both a capture time a second apart with `sqlite3` (the picture has none and a group needs one), reloads, opens Similar then the group, and waits for two badges reading "One eye closed". The second view is `blink_for_images`, whose rows must say 1 face, 1 judged, 1 one-closed per photo. The mesh model is seeded from `models/` by a development build.

## Gotchas

- Not driven: the badge on a face with both eyes closed (the portrait has one eye reading closed) and a group photograph; the unit tests and the Python measurements cover the verdicts, this spec covers the wiring.
- A group needs a capture time; photographs with none are not grouped (the spec sets one in the catalog file).
- Without the detector file in the cache the check reports "face_models" and no badge appears.
