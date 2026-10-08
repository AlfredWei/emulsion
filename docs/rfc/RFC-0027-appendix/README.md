# RFC-0027 appendix: the denoise bake-off (slice 3a)

Throwaway harness behind RFC-0027 §4.1. Not product code, not built by CI. The working copy, with the model files and photos, lives in the sibling project `denoise_model/` (outside this repo; its README has every source, commit and SHA-256).

- `export_nafnet.py` — NAFNet-SIDD-width32 (megvii-research/NAFNet, MIT) to ONNX, dynamic H and W.
- `bench.py` — ORT CPU time and peak RSS for one model at one square size, one model per process.
- `quality_real.py` — RFC-0023 §4.7's real-noise protocol (NIND ISO 6400 vs its ISO 200, six scenes) for both models, plus tone shift and high-frequency retention.
- `real_noise_results.json` — per-scene numbers; `real-noise-crops.jpg` — 256 px crops at 150%, rows tree1, chapel, Leonidas, stairs; columns reference (ISO 200), noisy (ISO 6400), SCUNet, NAFNet. NIND photos: Trougnouf, CC BY 4.0 (credits in RFC-0023's appendix `quality-real-noise-sources.json`).

Machine: Apple M1 Pro, 32 GB, ORT 1.30.0 CPU provider, default threads.
