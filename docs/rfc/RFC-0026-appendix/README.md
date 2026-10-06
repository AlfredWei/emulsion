# RFC-0026 appendix

- `ref_sam2.py` -- runs the six RFC-0023 §4.2 clicks through **Python onnxruntime** and dumps each candidate's 256x256 logits and predicted IoU as raw f32 files. Needs the two SAM 2 Tiny ONNX files (hashes in `../RFC-0023-appendix/SHA256SUMS`) in `models/` next to the script and `onnxruntime`, `numpy`, `Pillow`.
- The Rust side compares against that dump with an ignored test:

```
SEGMENT_MODELS_DIR=<folder with the two .onnx files> SEGMENT_REF_DIR=<dump folder> \
  cargo test --release --bin emulsion-ai -- --ignored --nocapture segment_matches
```

Nothing here is committed besides the script; weights and dumps stay outside the repo.

Memory split (needs the weights): `SEGMENT_MODELS_DIR=<dir> cargo test --release --test ai_helper -- --ignored --nocapture real_model` prints the app process's and the helper process's RSS and shows the helper idling out.
