# AI denoise model: NAFNet-SIDD-width32 as ONNX (RFC-0027 §8, §9)

The app downloads one model file for AI denoise, **hosted by this project** (a GitHub release asset) rather than by a third party, so the file the app verifies is the file we converted and published.

- **File**: `nafnet_sidd_w32.onnx`
- **Release**: tag `denoise-model-nafnet-sidd-w32-v1` of `AlfredWei/emulsion`, asset `nafnet_sidd_w32.onnx`
- **Size**: 117,316,644 bytes
- **SHA-256**: `5ff072283fe8c86b7c990aa7331cdb26183e20b08961769c70057b82260b133d` (pinned in `app/src-tauri/src/denoise_models.rs`; the app refuses any file that does not match)
- **What it is**: the `NAFNet-SIDD-width32` real-image denoising checkpoint of megvii-research/NAFNet, loaded into upstream's `NAFNet` class (width 32, encoder blocks [2,2,4,8], middle 12, decoder [2,2,2,2]; 29,159,715 parameters) and exported to ONNX (opset 17, dynamic H and W, input `image`, output `out`, float32 0..1) by `docs/rfc/RFC-0027-appendix/export_nafnet.py`. No weight was changed.
- **Weights' origin**: the checkpoint file used (SHA-256 `89c70e808d1783b6c07911306e106aaf0d4f7f3da8c61078b99ff7f8929a26f4`, 116,861,841 bytes) came from the Hugging Face mirrors `WAS/was-node-suite-weights` and `nyanko7/nafnet-models`, whose LFS hashes agree with each other. It was **not compared with the upstream file** (upstream distributes it through Google Drive and Baidu). Upstream code: `megvii-research/NAFNet` at commit `2b4af71ebe098a92a75910c233a3965a3e93ede4`.
- **Training data**: SIDD (Smartphone Image Denoising Dataset), released under the MIT License per its own page (https://abdokamel.github.io/sidd/). Smartphone sensor noise: results on other cameras are only partly checked (RFC-0027 §8).
- **Licence**: MIT, Copyright (c) 2022 megvii-model (text below). The architecture code used for the export derives from BasicSR (Apache-2.0, Copyright 2018-2020 BasicSR Authors); the ONNX file contains the trained numbers and the network graph, not that source.

## MIT License (megvii-research/NAFNet)

Copyright (c) 2022 megvii-model

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
