# Subject-selection model: SAM 2 Hiera Tiny as ONNX

Bundled in the installer (see `models/bundled-models.json`, which pins both files and their SHA-256; the app verifies them, `app/src-tauri/src/segment_models.rs`). This note exists so the model has a place for its provenance like the others; the source facts below are from the manifest and the notices.

- **Files**: `sam2_hiera_tiny.encoder.onnx` (134,261,315 bytes) and `sam2_hiera_tiny.decoder.onnx` (20,640,886 bytes)
- **Source**: Hugging Face `vietanhdev/segment-anything-2-onnx-models`, revision `071f58077599431edd0e5d2ac52ecca4c78f1cab`, an ONNX conversion of Meta's SAM 2 (facebookresearch/sam2)
- **Licence**: Apache-2.0. Meta's repository says "The SAM 2 model checkpoints, SAM 2 demo code (front-end and back-end), and SAM 2 training code are licensed under Apache 2.0" (https://github.com/facebookresearch/sam2); the conversion's card says "License unchanged from upstream (Apache 2.0)" and adds no terms.

## Training data and its terms (read 2026-11-14)

- **Trained on**: "Our released model is trained on SA-V manual + Internal and SA-1B", plus "internally available licensed video data" (https://arxiv.org/html/2408.00714).
- **Dataset terms**: SA-V is CC BY 4.0 (https://ai.meta.com/datasets/segment-anything-video/). SA-1B: "Limited; see full license language", intended use "Research purposes only" (https://ai.meta.com/datasets/segment-anything/). Meta's internal video: terms not public.
- **Reading**: Meta owns SA-1B and SA-V and licensed the weights itself, so the research-only terms limit what others may do with the dataset, which this app neither receives nor uses. One research run rated this "needs a closer look" for the unread SA-1B text; the other "no known blocker".
- **Not retrieved**: the SA-1B licence text; the internal video data; what the Hiera encoder was initialised from.
Whether a dataset's terms reach the trained weights is a legal question this note does not answer (RFC-0038, question 7). Engineering research, not legal advice.
