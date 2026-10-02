# RFC-0023 appendix: the on-device inference benchmark

Everything here is the throwaway harness behind RFC-0023's measured numbers — not product code, not built by CI.

- `bench_ort.py` / `bench_one.py` — ONNX Runtime 1.30.0 (Python wheel), CPU vs `CoreMLExecutionProvider` (`MLComputeUnits=ALL`), random-input timings (`bench_one.py` runs a single case, which is how the cases after SCUNet 512 were completed; the first full run died there).
- `tractbench/` — loads a model with `tract-onnx` 0.23.7, fixes the input shapes, optimizes, runs it three times (CPU only).
- `SHA256SUMS` — the exact weight files measured (not committed; fetch from the Hugging Face repos listed in RFC-0023 §3).

Machine: Apple M1 Pro, 16-core GPU, 32 GB, macOS (Darwin 25.5). Inputs are random tensors: timing only, **no accuracy was measured**.
