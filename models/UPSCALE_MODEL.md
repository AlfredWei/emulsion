# AI super-resolution model: Real-ESRGAN `realesr-general-x4v3` as ONNX (RFC-0029 §3.7, §8, §9)

**Status: not published, not bundled.** The pinned file below is what the app will accept; the release asset it would be downloaded from does not exist yet. Publishing it, and shipping the model at all, waits for the project owner's decision on the weights' implicit licence (RFC-0029 §8, §9). The *Import...* path works with the file below in the meantime.

- **File**: `realesr_general_x4v3.onnx`
- **Planned release**: tag `upscale-model-realesr-general-x4v3-v1` of `AlfredWei/emulsion`, asset `realesr_general_x4v3.onnx`
- **Size**: 4,866,388 bytes
- **SHA-256**: `a337283437fd0fc12e6882a4f4cc0467cf2f275782e8290f487d755f806137fb` (pinned in `app/src-tauri/src/upscale_models.rs`; the app refuses any file that does not match)
- **What it is**: the 4x `realesr-general-x4v3` checkpoint (`SRVGGNetCompact`, 32 conv layers, 1,213,296 parameters) of xinntao/Real-ESRGAN, loaded with `strict=True` and exported to ONNX (opset 17, dynamic H and W, input `image`, output `out`, float32 0..1, 4x) by `docs/rfc/RFC-0029-appendix/convert.py general_x4v3`. No weight was changed. The export was checked against PyTorch: max absolute difference 6e-6 on a 256 px tile.
- **Weights' origin**: `realesr-general-x4v3.pth` (4,885,111 bytes, SHA-256 `8dc7edb9ac80ccdc30c3a5dca6616509367f05fbc184ad95b731f05bece96292`) from the `v0.2.5.0` release of github.com/xinntao/Real-ESRGAN, downloaded 2026-10-17 and loaded with `torch.load(weights_only=True)`.
- **Licence**: the repository is BSD-3-Clause; **the weights carry no separate stated terms and the training data's terms are unstated** (RFC-0029 §8). That reading is implicit, not explicit.
