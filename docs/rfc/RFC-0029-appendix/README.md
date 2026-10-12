# RFC-0029 appendix: slice 4a, super-resolution bake-off (2026-10-12)

Machine: Apple M1 Pro, ONNX Runtime CPU (`onnxruntime` 1.30, PyTorch 2.14, run from the sibling `mask_training/venv`; nothing was installed). All numbers are from this machine only; no Windows, no GPU / CoreML.

## Files

- `convert.py` exports a model to ONNX and **compares the ONNX output with PyTorch's** (max abs difference printed). The Real-ESRGAN networks are re-written from the upstream definitions (`SRVGGNetCompact`, `RRDBNet`), SwinIR uses the upstream `network_swinir.py` (Apache-2.0; SHA-256 `9e143898679ebeebc5d2fc94ad1b89c38aa4a4d43da4e0fcba0f93e476994913`, fetched from raw.githubusercontent.com/JingyunLiang/SwinIR/main/models/ and not committed; its three `timm` helpers are shimmed). `load_state_dict` is strict.
- `bench1.py` time and peak RSS of one model per process; `quality.py` PSNR / SSIM / high-frequency ratio on the eight photos, two degradations, Strength blends; `quality-results.json` its output; the `crop_*.jpg` sheets (columns: reference, Lanczos, `general_x4v3`, `x4plus`, `swinir_psnr`).
- The eight reference photos are RFC-0023 §4.7's (CC0; see ../RFC-0023-appendix/ATTRIBUTION.md; photos 19, 24, 25 are shown here, all CC0). The photos themselves are not committed.

## Weights used (downloaded 2026-10-12 with the user's approval, outside the repo, sizes checked against the release metadata)

| File | Source | Bytes | SHA-256 |
|---|---|---|---|
| `realesr-general-x4v3.pth` | github.com/xinntao/Real-ESRGAN, v0.2.5.0 | 4,885,111 | `8dc7edb9ac80ccdc30c3a5dca6616509367f05fbc184ad95b731f05bece96292` |
| `RealESRGAN_x4plus.pth` | github.com/xinntao/Real-ESRGAN, v0.1.0 | 67,040,989 | `4fa0d38905f75ac06eb49a7951b426670021be3018265fd191d2125df9d682f1` |
| `003_realSR_BSRGAN_DFO_s64w8_SwinIR-M_x4_PSNR.pth` | github.com/JingyunLiang/SwinIR, v0.0 | 67,129,849 | `1fd8fed99684bd271db55563e4906d36459cf535446820053f7a1081d4781dc5` |

The SwinIR GAN checkpoint was **not** tested (its download was stopped to give the other two the bandwidth, about 50 KB/s in total).
