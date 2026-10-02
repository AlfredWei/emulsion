# RFC-0023 appendix: the on-device inference benchmark

Everything here is the throwaway harness behind RFC-0023's measured numbers — not product code, not built by CI.

- `bench_ort.py` / `bench_one.py` — ONNX Runtime 1.30.0 (Python wheel), CPU vs `CoreMLExecutionProvider` (`MLComputeUnits=ALL`), random-input timings (`bench_one.py` runs a single case, which is how the cases after SCUNet 512 were completed; the first full run died there).
- `tractbench/` — loads a model with `tract-onnx` 0.23.7, fixes the input shapes, optimizes, runs it three times (CPU only).
- `SHA256SUMS` — the exact weight files measured (not committed; fetch from the Hugging Face repos listed in RFC-0023 §3).

Machine: Apple M1 Pro, 16-core GPU, 32 GB, macOS (Darwin 25.5). Inputs are random tensors: timing only, **no accuracy was measured**.

## Accuracy check (M6 slice 0, 2026-10-03)

`acc_sky.py`, `acc_sam.py`, `acc_sam2.py` run the shortlisted models on four of this repo's own CC-licensed test photos (`test_image/`: corn-field landscape, Singapore bridge frame, Budapest night, portrait) and write the overlay sheets `accuracy-skyseg.jpg`, `accuracy-mobilesam.jpg`, `accuracy-sam2-tiny.jpg` (red = predicted mask, green dot = the click). Judged **by eye**, no metric, six clicks. Preprocessing for skyseg follows the original author's `onnx_interence.py`; the SAM scripts follow the models' ONNX input shapes (the exact preprocessing the exporter intended was not independently confirmed).
