# Face-mesh model for blink evidence: MediaPipe Face Mesh V2 as ONNX (RFC-0031 §8.14, §8.15)

**Status: committed in this folder and bundled in the installer.** `models/bundled-models.json` lists it with a `path`; on first use the app copies it into `blink_models/` under its cache root, verified against the pin below (`app/src-tauri/src/blink_models.rs`, `bundled_models.rs`). A development build seeds from this folder too. There is no download path.

- **File**: `face_landmarks.onnx`
- **Size**: 4,920,979 bytes
- **SHA-256**: `d16e5a55e6a480284d468ee32469692464049518c311662ab3956681de31e3e9`
- **What it is**: Google MediaPipe's Face Landmarks Detector (Face Mesh V2) converted to ONNX by a third party. Input `[N, 256, 256, 3]` RGB 0..1, a rotated and cropped face; outputs 478 landmarks x (x, y, z) in crop pixels (1,434 values) and a face-presence logit. The app reads the eyelid points (six per eye) for an eye aspect ratio and the presence score; it does not use the Blendshapes model, which `tract` cannot load.
- **Where from**: Hugging Face repository `fernandotonon/QtMeshEditor-facemesh-onnx` (a mirror of one graph in `fernandotonon/QtMeshEditor-models`), downloaded 2026-11-09 with the go-ahead of the project owner. Size and hash matched the repository's listed LFS object.
- **Licence reading (not complete)**: the mirror's model card says **Apache-2.0** ("a direct ONNX conversion of a Google MediaPipe model, Apache-2.0 code and weights"); Google's own Face Mesh V2 and Blendshape V2 model cards say Apache-2.0 too (read through a third-party mirror of the card PDFs, not from Google's storage). **Open items:** (1) the Apache-2.0 licence text must reach recipients of the installer (§4 of the licence); the app has no third-party notices file yet, so this slice does not discharge it; (2) the conversion script's own licence was not read; (3) the bundle's licence file upstream was not read.
- **How it was checked**: the Rust reading (`blink_mesh.rs`) matches the Python tool's output on two real faces to presence 0.02 and eye ratio 0.01 (test `matches_the_python_tool_on_two_real_faces`, which now runs in CI because the file is committed).
