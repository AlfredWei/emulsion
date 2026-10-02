# RFC-0023: On-device AI inference for M6 — runtime, models, and what was actually measured (M5.7)

- Status: Draft for review
- Date: 2026-10-02
- Relates to: [PRD MILESTONES §M5.7 / §M6](../../PRD/MILESTONES.md), [ADR-0007](../adr/ADR-0007-face-detection-and-recognition.md) (`tract`), [ADR-0003](../adr/ADR-0003-raw-decoding.md) (native-library cost), proposed [ADR-0009](../adr/ADR-0009-ml-inference-runtime.md)

## 0. Process note

M5.7 is a research-only milestone (M0's relationship to M1): resolve M6's "architecture decision required before scoping in detail" with real feasibility numbers, and write a recommendation per M6 feature area so M6 starts as a build milestone. The user approved downloading a small set of model weights for benchmarking (2026-10-02); everything was fetched into a scratch directory outside the repo, hashes are in the [appendix](RFC-0023-appendix/SHA256SUMS), and no weights are committed.

**What this RFC can and cannot claim.** It measures *speed and loadability* of five off-the-shelf ONNX models on **one machine** (Apple M1 Pro, 32 GB). It does **not** measure *accuracy* (no model was run on a real photograph and judged), and **nothing was measured on Windows**. Where a number is extrapolated or a claim comes from a web page rather than a run, the text says so.

## 1. Questions

1. Is fully on-device inference feasible for M6's features at a reasonable speed on target hardware?
2. Which runtime: keep `tract` (pure Rust, CPU only, ADR-0007) or add ONNX Runtime (`ort`) with GPU/NPU execution providers?
3. Which model per feature: subject/object selection, sky, denoise, super-resolution (and, for awareness, generative removal/fill)?

## 2. Method

[`bench_ort.py`, `bench_one.py`, `tractbench/`](RFC-0023-appendix/README.md): ONNX Runtime 1.30.0 (CPU vs `CoreMLExecutionProvider`, `MLComputeUnits=ALL`) and `tract-onnx` 0.23.7 (CPU), random-valued inputs at stated shapes, median of 3–5 runs after one warm-up run. Op lists and I/O shapes were read from the ONNX graphs. Licenses were read from the upstream repositories (not only the Hugging Face re-host tags). Timing of random input is a fair speed test and says nothing about quality.

## 3. Models tried

| Role | Model (re-host) | Size | Upstream license (read, not assumed) | Notes |
|---|---|---|---|---|
| Click/subject select | MobileSAM (`vietanhdev/segment-anything-onnx-models`) | enc ≈ 36 MB zip | Apache-2.0 (code); weights shipped under the repo | encoder 5 M params; the decoder file is the SAM decoder (named `sam_vit_h…`) |
| Click/subject select | SAM 2 Hiera-Tiny (`vietanhdev/segment-anything-2-onnx-models`) | 134 + 21 MB | Apache-2.0 for code *and* checkpoints per the upstream README (demo fonts SIL OFL; one GPU component under its own license — irrelevant to inference) | upstream marks the ONNX export experimental, image input only |
| Sky | `skyseg.onnx` (`JianyuanWang/skyseg`) | 176 MB | MIT as tagged — **but a third-party re-host of a third-party model whose training data the card does not state** | U²-Net-class, 320×320, 7 sigmoid outputs (fused + side outputs) |
| Denoise | SCUNet `color_real_psnr` (`Heliosoph/scunet-onnx`) | 3.8 + 73 MB | Apache-2.0 (upstream states code and checkpoints; trained on synthetic degradation) | blind real-world denoise, dynamic H×W |
| Super-resolution | Real-ESRGAN `realesr-general-x4v3` (`Heliosoph/realesrgan-onnx`) | 4.9 MB | **BSD-3-Clause for the code; the upstream README does not state a separate license for the weights** | tiny model (PReLU + DepthToSpace), 4× |

## 4. Measured results (Apple M1 Pro, 32 GB; seconds unless stated)

| Case | ORT CPU | ORT CoreML (ALL) | `tract` CPU |
|---|---|---|---|
| MobileSAM encoder, 1024² | **0.35** | 0.82 (load 8.2 s) | 1.79 |
| SAM 2 Tiny encoder, 1024² | 0.96 | 1.42 (load 7.8 s) | **fails to load** |
| SAM decoder (MobileSAM), 1 click | 0.022 | — | **fails to load** |
| SAM 2 Tiny decoder, 1 click | 0.020 | — | **fails to load** |
| skyseg, 320² | 0.39 | **0.145** (load 8.7 s) | 0.65 |
| SCUNet real_psnr, 512² | **3.85** | **248** (first run 36 s) | **fails to load** |
| SCUNet real_psnr, 1024² | 16.2 | not run | — |
| Real-ESRGAN general-x4v3, 256²→1024² | 0.55 | **0.37** | 0.83 |
| Real-ESRGAN general-x4v3, 512²→2048² | 2.21 | not run | — |

Not measured: any Windows run (DirectML, CUDA), any machine other than this one, CoreML on the 1024² SCUNet and 512² Real-ESRGAN cases, peak memory, energy, and accuracy of any model. A first full ORT run died silently after the SCUNet-512 case (no JSON written); the remaining cases were re-run one at a time, so figures from the two runs come from separate processes.

### 4.1 What the numbers show

1. **A blanket "use the GPU/Neural Engine provider" is wrong for this model set.** CoreML helped two models (skyseg 2.7×, Real-ESRGAN 1.5×), hurt two (MobileSAM encoder 2.3× slower, SAM 2 1.5× slower), and was pathological for SCUNet (65× slower than CPU — the graph evidently partitions badly across ANE/GPU/CPU; cause not investigated). The CoreML runs also pay a **7–9 s first-load compile** per model per session (no compiled-model cache was configured; whether one removes it is untested). The execution provider has to be chosen per model from a measurement, with CPU as the safe default.
2. **`tract` cannot be the only runtime.** Where it loads a model it is 1.5–5× slower than ORT CPU (MobileSAM encoder 5×), and it failed to load three of the five model families: SAM 2 encoder (a `Conv` rank-inference failure in `conv_s0`), SAM 2 decoder (a symbolic `num_labels` dimension it could not unify with 1), SCUNet (the same class: symbolic `batch` against a fixed shape in a `Pad`), and the MobileSAM decoder (`Resize` whose size depends on the runtime `orig_im_size` input). These are failures *as exported and as loaded by this harness* — a re-export with static shapes, or constant-folding the size input, might fix some. Not attempted. ADR-0007's own consequence ("`tract`'s op coverage is a standing constraint on any future model swap") has now been confirmed against the M6 candidates.
3. **Interactive click-to-mask is feasible on this machine.** Encoder once per image (0.35 s CPU, background-able) then ~20 ms per click for MobileSAM and for SAM 2 Tiny's decoder: inside the ~100 ms interactive budget. SAM 2 Tiny's encoder is 2.7× slower than MobileSAM's with no *measured* quality benefit (quality was not measured).
4. **Denoise is a batch/explicit action, not an interactive slider.** SCUNet at 3.85 s per 512² tile on CPU extrapolates (area scaling, no tile overlap, **not measured end to end**) to about 92 tiles × 3.85 s ≈ **6 minutes for a 24 MP frame**. The 1024² result (16.2 s for 4× the area) is consistent with linear scaling.
5. **Super-resolution is similarly an explicit action.** Real-ESRGAN general-x4v3 at 2.2 s per 512² input tile extrapolates to ≈ 3 min for a 24 MP frame at 4× (96 MP output, which must be tiled and streamed; peak memory unmeasured).
6. **Runtime cost on disk:** the ORT dynamic library in the Python wheel is 33 MB (macOS arm64); the `ort` crate has no stable 2.0 yet (latest `2.0.0-rc.13`, 2026-07-28) — a real pin-and-track risk. `tract-onnx` is at 0.23.8 (2026-09-21). Neither claim was verified by building `ort` into this app.

## 5. Recommendations per M6 feature area

These are recommendations to start M6 as a build milestone, not final model choices; each needs the quality evaluation this spike did not do.

1. **Subject / object selection (click, box):** **MobileSAM** as the baseline (smallest, fastest measured, Apache-2.0), running on **ORT CPU**; evaluate SAM 2 Tiny on real photographs before preferring it (2.7× slower encoder, ONNX export experimental, no CoreML benefit). Pre-compute the encoder embedding in the background after Develop opens an image; cache by content hash like the existing Develop preview cache. Interactive cost is the ~20 ms decoder.
2. **Sky:** **do not ship `skyseg` as is.** It is the fastest model here (0.145 s on CoreML) and loads fine, but its training data is undocumented and it is a re-host of a re-host: the MIT tag is not a provenance chain, and ADR-0007's own rule is "re-verify and document provenance for any third-party ML weights". M6 slice 0 should either (a) trace `skyseg` to a documented source and dataset, (b) find a model with a documented, permissively licensed dataset, or (c) deliver *Select Sky* as MobileSAM prompted by a sky-colour/position heuristic plus the existing luminance/colour-range masks, with no sky-specific model at all. (c) is unevaluated.
3. **AI denoise:** **SCUNet (Apache-2.0) on ORT CPU, scoped as an explicit action / export-time step with a crop preview**, not a live slider. The 6-minute full-frame extrapolation is the main product risk; a lighter model should be evaluated before committing (candidates *named but not tested*: NAFNet-class, DRUNet-class). Do not use CoreML for it (measured pathological).
4. **Super-resolution:** **Real-ESRGAN `realesr-general-x4v3`** (4.9 MB) on **ORT CoreML on macOS** (0.37 vs 0.55 s on the one size measured) with CPU fallback, as an explicit action with tiling. **Confirm the weights' license** before shipping (code is BSD-3; the weights' terms are not stated in the upstream README).
5. **Composable masking (add/subtract/intersect):** no model; this is mask-engine work in M6's own scope and needs no research here.
6. **Generative removal/fill (M7):** **not surveyed in this pass.** M7 keeps ownership of that decision (MILESTONES), but the M5.7 scope also asked for an awareness survey; that remains open.

## 6. Runtime decision (proposed — see ADR-0009)

Add **ONNX Runtime via the `ort` crate** behind one thin `ml` module, **CPU execution by default, an execution provider opted in per model only where a benchmark showed a win** (CoreML for Real-ESRGAN and skyseg-class models on macOS; DirectML on Windows is **unmeasured** and stays desk research until run). Keep **`tract` for the existing face models** (ADR-0007) unchanged — they work, and moving them is out of scope. The two runtimes coexist; the cost is a second inference dependency and a native library per platform (the ADR-0003 class of cost), accepted because `tract` cannot load the models M6 needs and is several times slower on the ones it can.

## 7. Open questions M6 slice 0 must close (not answered here)

- **Windows:** DirectML/CUDA numbers, and whether `ort` links cleanly on the project's MSVC/vcpkg setup (the ADR-0003 pain point). Nothing here was run on Windows.
- **Accuracy:** run each shortlisted model on real photographs (landscape/sky, portrait, busy scenes) and judge masks, denoise, and upscale by eye and with a simple metric; this spike's timing says nothing about quality.
- **Minimum hardware:** only an M1 Pro (a high-end laptop chip) was measured. M6's "define the target machine spec" is not done.
- **`ort` link mode and stability:** whether the `load-dynamic` feature (load the runtime library at run time rather than link it) removes the linking cost, how the pre-release `2.0.0-rc` series should be pinned, and what the signed-binary/notarization story is for a bundled `libonnxruntime`. Not verified.
- **CoreML compile cache** and why SCUNet partitions so badly (try `MLComputeUnits=CPUAndGPU`, static shapes, or the ORT compiled-model cache).
- **Static-shape re-exports** for `tract` where a model is wanted in pure Rust (SAM 2, SCUNet).
- **Generative fill/remove** awareness survey (§5.6).

## 8. Non-goals

Implementation of any M6 feature; a quality benchmark; picking M7's approach; cloud inference (out of scope for the product).
