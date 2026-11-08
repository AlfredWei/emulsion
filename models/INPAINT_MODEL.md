# Object-removal model: LaMa (fp32) as ONNX (RFC-0032, ADR-0010)

The app ships one inpainting file for the Remove tool, **bundled in the installer** beside the other models; the build fetches it and the app verifies it. There is no download dialog.

- **File**: `lama_fp32.onnx`
- **Source**: Hugging Face `Carve/LaMa-ONNX`, revision `c3c0c9e468934d62e79c329e35d82dd09ff8c444` (pinned in `models/bundled-models.json`)
- **Size**: 208,044,816 bytes
- **SHA-256**: `1faef5301d78db7dda502fe59966957ec4b79dd64e16f03ed96913c7a4eb68d6` (pinned in `models/bundled-models.json` and `app/src-tauri/src/inpaint_models.rs`; the app refuses any file that does not match). The same hash is what the repository publishes for that file at that revision.
- **What it is**: LaMa (Resolution-robust Large Mask Inpainting with Fourier Convolutions), the `big-lama` weights, exported to ONNX. Fixed 512 x 512 window: inputs `image` 1x3x512x512 (RGB, 0..1) and `mask` 1x1x512x512 (1 = fill); output 1x3x512x512 on the 0..255 scale, the pixels outside the mask coming back as given. Run on the ONNX Runtime CPU provider in the `emulsion-ai` helper (`bin/emulsion-ai/inpaint.rs`); `ai/inpaint_pipeline.rs` plans the window and builds the patch.
- **Licence**: Apache-2.0 for the code and the weights, per the model card of this repository and of the original `big-lama` card. **The upstream `LICENSE` text was not retrieved when this was written (RFC-0032 §9.4), so it is not reproduced here**; adding it with the other bundled models' notices is open.
- **Training data**: Places2. **Its terms were not read** (the page was unreachable during the bake-off). The product owner accepted shipping a model trained on it on 2026-11-02 (ADR-0010 §2), knowing that.
- **Measured** (RFC-0032 §9): closest to the original in 46 of 60 hole cases against MI-GAN and a PatchMatch baseline; 2.3 s per removal on an M1 Pro CPU at its native window; bit-identical between runs on one machine and provider, texture-level differences between CPU and CoreML (§10).

## In a development build (`make dev`)

The 208 MB file is not committed. Copy it to `models/lama_fp32.onnx` (it is git-ignored; the SHA-256 above must match, or `node app/scripts/fetch-bundled-models.mjs --from <dir>` fetches or copies it). A debug build seeds the app's model cache from the checkout's `models/` at startup and when the Remove tool checks availability, **after** any `target/debug/models` a bundled build left behind (before 2026-11-08 that folder hid this one). Restart the app after adding the file.
