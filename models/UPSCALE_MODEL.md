# AI super-resolution model: Real-ESRGAN `realesr-general-x4v3` as ONNX (RFC-0029 §3.7, §8, §9)

**Status: committed in this folder and bundled in the installer** (project owner's decision, 2026-10-17: at 4.8 MB it travels with the source and the installer, like the denoise and Select Subject models). `models/bundled-models.json` lists it with a `path` (copied from here by `app/scripts/fetch-bundled-models.mjs`, no network); on first use the app copies it into its model folder, verified against the pin below. A development build (`tauri dev`) seeds from this folder too. **The download fallback (release asset below) is still not published**: a 404 is reported as "not published for download yet" and *I have the file* (import) works. The decision accepts the *implicit* licence reading of RFC-0029 §8: the weights carry no separate stated terms and the training data's terms are unstated.

- **File**: `realesr_general_x4v3.onnx`
- **Optional download fallback (not published)**: tag `upscale-model-realesr-general-x4v3-v1` of `AlfredWei/emulsion`, asset `realesr_general_x4v3.onnx`
- **Size**: 4,866,388 bytes
- **SHA-256**: `a337283437fd0fc12e6882a4f4cc0467cf2f275782e8290f487d755f806137fb` (pinned in `app/src-tauri/src/upscale_models.rs`; the app refuses any file that does not match)
- **What it is**: the 4x `realesr-general-x4v3` checkpoint (`SRVGGNetCompact`, 32 conv layers, 1,213,296 parameters) of xinntao/Real-ESRGAN, loaded with `strict=True` and exported to ONNX (opset 17, dynamic H and W, input `image`, output `out`, float32 0..1, 4x) by `docs/rfc/RFC-0029-appendix/convert.py general_x4v3`. No weight was changed. The export was checked against PyTorch: max absolute difference 6e-6 on a 256 px tile.
- **Weights' origin**: `realesr-general-x4v3.pth` (4,885,111 bytes, SHA-256 `8dc7edb9ac80ccdc30c3a5dca6616509367f05fbc184ad95b731f05bece96292`) from the `v0.2.5.0` release of github.com/xinntao/Real-ESRGAN, downloaded 2026-10-17 and loaded with `torch.load(weights_only=True)`.
- **Licence**: the repository is BSD-3-Clause, Copyright (c) 2021, Xintao Wang (full text below); **the weights carry no separate stated terms and the training data's terms are unstated** (RFC-0029 §8). That reading is implicit, not explicit.

## BSD 3-Clause License (xinntao/Real-ESRGAN)

Reproduced because the model is now redistributed in the installer (clause 2). Fetched from the repository's `LICENSE` on 2026-10-17.

BSD 3-Clause License

Copyright (c) 2021, Xintao Wang
All rights reserved.

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are met:

1. Redistributions of source code must retain the above copyright notice, this
   list of conditions and the following disclaimer.

2. Redistributions in binary form must reproduce the above copyright notice,
   this list of conditions and the following disclaimer in the documentation
   and/or other materials provided with the distribution.

3. Neither the name of the copyright holder nor the names of its
   contributors may be used to endorse or promote products derived from
   this software without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE
FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY,
OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
