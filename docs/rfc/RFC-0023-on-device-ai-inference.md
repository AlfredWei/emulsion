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

### 4.2 Accuracy check (added 2026-10-03, M6 slice 0) — by eye, six clicks, four photos

Run on four of this repo's own CC-licensed test photos (`test_image/`), overlays in the [appendix](RFC-0023-appendix/README.md): [skyseg](RFC-0023-appendix/accuracy-skyseg.jpg), [MobileSAM](RFC-0023-appendix/accuracy-mobilesam.jpg), [SAM 2 Tiny](RFC-0023-appendix/accuracy-sam2-tiny.jpg). **This is a smoke test, not an evaluation:** no metric, no ground truth, one judge, four images, one positive click per query. It can show a model is clearly unfit; it cannot show one is good.

- **skyseg (320², absolute threshold 0.5):** clean masks with a crisp skyline on the clear-sky landscape (58% of frame) and on the night skyline (25%). **False positive on the portrait:** the bright, blurred wall behind the subject is marked as sky (about 18% of the frame). On a frame with no sky (a building-dominated street view) the raw output peaked at 0.05, so an absolute threshold correctly returns nothing — *but the author's own post-processing min-max-rescales the output*, which turns that near-zero output into a spurious blob; do not copy that step.
- **MobileSAM (this ONNX export):** returns **one mask per click** (its "IoU" prediction reads 0.95–1.02, i.e. not a calibrated score). The field and the sky come out acceptably; a click on the bridge tower gave a small box at its base (0.9% of the frame), a click on a white building gave a 0.2% sliver, and clicks on the portrait's shirt and face gave blobs that mix hair, face and shirt. Unfit as a one-click selector in this export.
- **SAM 2 Tiny (3 candidates per click, best-by-predicted-IoU shown):** corn field, sky (crisp mountain edge), whole pink shirt and the face/skin region all came out clean — **4 of 6 good, 2 part-versus-whole ambiguities** (the tower click picked the stone pier rather than the tower; the building click picked a small sign). The classic SAM ambiguity: the three candidates exist precisely so the user can pick, so the UI must expose them (or let a second click refine) rather than auto-choosing the top one.
- **Cost of the better model:** SAM 2 Tiny's encoder is 1.0 s vs MobileSAM's 0.35 s (CPU, §4), and `tract` cannot load it (§4.1).

### 4.2a Broader sky test (added 2026-10-03): 8 more photos — this reverses a claim made in the previous section

Eight openly licensed photos (night sky, overcast, cloud study, sunbeams, city skyline, long-exposure sea, snow mountain, bare branches; sources and licences in [ATTRIBUTION.md](RFC-0023-appendix/ATTRIBUTION.md)), `skyseg` (absolute threshold 0.5) against SAM 2 Tiny given **one click in the sky**, best-by-predicted-IoU candidate. Sheets: [part 1](RFC-0023-appendix/accuracy-sky-eight-photos-part1.jpg), [part 2](RFC-0023-appendix/accuracy-sky-eight-photos-part2.jpg) (red = skyseg, blue = SAM 2, green dot = click; photo 8 is CC BY-SA and not committed). Judged by eye; the two models' mutual agreement (IoU) is given as a consistency signal, **not** as accuracy.

| # | Scene | skyseg | SAM 2 Tiny + sky click | agreement IoU |
|---|---|---|---|---|
| 1 | night sky over a hill | good | good | 0.97 |
| 2 | overcast, misty mountains | good | good, follows the ridge slightly better | 0.98 |
| 3 | cloud study (cumulus + haze) | whole frame = sky (arguably right: it is all cloud) | **blue sky only; the cloud itself and the haze are left out** | 0.35 |
| 4 | sunbeams over mountains | clean skyline, includes the sunbeam region | **fragmented: holes along the cloud bank** | 0.84 |
| 5 | city skyline | good; paints over the thinnest spire tips | good; outlines the spires separately | 0.98 |
| 6 | long-exposure sea and sky | **correct, clean horizon** | **spills into the sea as pixelated blotches** | 0.76 |
| 7 | snow mountain | good | good | 0.98 |
| 8 | bare branches against blue sky | sky mask covers the thin branches (320-px output is too coarse) | misses patches of sky (blocky, 256-px mask) and keeps the branches | 0.70 |

What it shows: in 4 of 8 photos the two agree closely (0.97–0.98), and in the other four **`skyseg` is the better *sky* mask in 3 (photos 3, 4, 6) and neither is acceptable at fine detail in 1 (photo 8)**. SAM 2 prompted with a sky point selects "the thing under the click", not the semantic class: it leaves clouds out, fragments around them, and bleeds on low-contrast scenes. **The "lead candidate" in the previous version of §5.2 — SAM 2 Tiny with a sky point — is therefore not supported and is withdrawn.** Both models produce low-resolution masks (320 / 256 px upscaled) and both need edge refinement for thin structures (the app already has a guided-filter implementation in the Develop shaders that could be reused; not tried).

**So the best-performing sky model is the one with the unresolved provenance (§4.3).** A quick search of Hugging Face for permissively licensed sky/semantic-segmentation models with a documented dataset found nothing usable (hits were PASCAL-VOC DeepLab exports, which have no sky class, and an aerial-imagery model). Candidates not yet checked: models trained on COCO-Stuff or ADE20K (the dataset and weight licences must be read, ADE20K in particular is often research-only), or training a small sky model on a documented permissive dataset.

### 4.3 Correction to §3 and the `skyseg` provenance finding

§3 called `skyseg` "a third-party re-host of a third-party model whose training data the card does not state". Reading the author's repository makes it worse: the original author (xiongzhu666, MIT) says they published only **"a small sky-seg model of 2Mb (trained by u2netp)"** and that **"we couldn't public the high-precision model because it used in our product"**. The 176 MB `skyseg.onnx` on Hugging Face (the file measured here) is therefore **not** the author's published model by their own account, and its origin is unexplained. Its MIT tag cannot be relied on. The author also notes it "has some defect: in the scene of building, some detail of building will be considered as sky" — consistent with the portrait false positive above.

### 4.4 Windows and CI-runner results (added 2026-10-03, M6 slice 0)

A throwaway standalone crate, [`spikes/ort-spike`](../../spikes/ort-spike/README.md) (`ort` pinned `=2.0.0-rc.13`, ONNX Runtime downloaded at build time), run by a manual-trigger workflow (`.github/workflows/ort-spike.yml`, spike branch only) on GitHub's `windows-latest` and `macos-latest` runners; weights fetched at run time, same files as §3. [Run 37038143960](https://github.com/AlfredWei/emulsion/actions/runs/37038143960). The same binary was first checked locally on the M1 Pro: Rust `ort` reproduces the Python ONNX Runtime numbers of §4 (skyseg 378 ms CPU / 116 ms CoreML, SAM 2 encoder 960 ms, decoder 22.5 ms).

| Case (median of 3, ms) | M1 Pro (local) | `windows-latest` CPU | `macos-latest` CPU | `macos-latest` CoreML |
|---|---|---|---|---|
| skyseg 320² | 378 | 889 | 1982 | 386 |
| SAM 2 Tiny encoder 1024² | 960 | 2825 | 5452 | **165 879** |
| SAM 2 Tiny decoder, 1 click | 22.5 | 61 | 84 | 207 |

What this establishes — and what it does not:

1. **`ort` builds and runs on a Windows MSVC runner**, with ONNX Runtime linked **statically** into the executable (21.5 MB release exe; no `onnxruntime.dll`), started from a different directory than the build output. The only native file placed next to the exe is `DirectML.dll` (from the `directml` feature), which would have to ship with the app.
2. **Windows CPU numbers are usable for background work**: ~0.9 s for the sky model, ~2.8 s for the SAM 2 encoder (once per image, background), ~60 ms per click on a shared 4-vCPU runner. These are not representative of user hardware (a shared CI VM); they say the CPU path is not unreasonable, not what a given customer sees.
3. **DirectML could not be measured: the runner has no DirectX 12 device.** Creating the provider failed on every model with `No devices detected that match the filter criteria` (hard error, because the spike asked for `error_on_failure`). So **DirectML speed, and whether it helps or hurts these models as CoreML does, is still unknown**, and a real Windows GPU machine is required. It also shows a practical point: the provider must be requested with fallback to CPU, or an app on a machine without a DX12 adapter fails to open a session.
4. **CoreML repeats its pathology on a different Mac**: on the macOS runner the SAM 2 encoder took **166 s on CoreML vs 5.5 s on CPU** (and 8 s of provider load), while the sky model was 5× faster on CoreML (386 vs 1982 ms). This independently confirms §4.1 point 1: provider choice must be per model and measured, CPU by default.
5. **`ort` also links inside the real app crate, on both platforms** (throwaway draft PR #199, closed unmerged): with `ort =2.0.0-rc.13` (`download-binaries`, `tls-native`, `copy-dylibs`) added next to the vcpkg LibRaw, `tract`, `rustls` and `lcms2`, the normal CI was green on `windows-latest` and `macos-latest` for Rust build + test (456 tests incl. a probe that calls `ort::init()` and `ort::info()`, which passed on both) and for the WebdriverIO E2E suite (the full app launched). The ADR-0003 pain point (MSVC + vcpkg) did not recur. This is a link-and-initialise check only: no model was run inside the app, and no installer was built.
6. **Not established here:** code signing/notarization of the bundled runtime and of `DirectML.dll`, the `load-dynamic` alternative, installer size impact, and any GPU number on Windows.

### 4.5 Sky model licence search (desk research, added 2026-10-03, M6 slice 0)

Goal: find a sky model whose weights *and training data* have documented, shippable terms (replacing `skyseg`, §4.3). **No model was downloaded or run in this pass**; this is a read of project pages, not legal advice.

| Candidate | What the page says | Verdict |
|---|---|---|
| SegFormer-class models fine-tuned on **ADE20K** (e.g. Keras `segformer_b0_ade20k_512`) | Weights labelled MIT; the card states no data restriction. The upstream NVIDIA SegFormer code/weights use a non-commercial source-code licence, per a search summary (not verified at source). | **Not cleared.** ADE20K's terms say "only for non-commercial research and educational purposes" and that MIT CSAIL "does not own the copyright of the images". The terms do not mention derived models, so whether weights trained on it may ship in a commercial app is an open legal question, not a yes. An MIT label on the weights does not settle it. |
| `fast-skyseg` (WEIIEW97) | States MIT, though no LICENSE file was seen. Training framework only. | **No weights provided**; says nothing about which dataset was used. Usable only as a training recipe. |
| Models trained on **COCO-Stuff** (has a `sky` class) | Annotations CC BY 4.0; the underlying COCO images are under Flickr terms (mixed per-image licences). COCO's own terms page could not be retrieved in this pass. | **Unresolved**, but less restrictive than ADE20K on its face. No specific pretrained model was identified or checked. |
| `skyseg` (current best, §4.2a) | Provenance unexplained (§4.3). | Cannot ship. |

What this means:

1. **No ready-made sky model with clean provenance was found.** The common route (an ADE20K-trained segmenter) inherits a non-commercial dataset term.
2. **Two realistic paths remain**, neither tried: (a) train a small sky model ourselves on a dataset whose image and annotation terms we have read and can commit to the repo's ATTRIBUTION list (COCO-Stuff annotations on CC BY images only, or openly licensed photos such as the Wikimedia Commons ones already used in §4.2a with our own masks); (b) skip a dedicated sky model and ship *Select Sky* as the existing luminance/colour-range mask refined by a click-to-select model (SAM 2 Tiny), accepting §4.2a's weaker results (clouds left out, fragments, bleeding on low-contrast scenes).
3. Not checked: SkyFinder and other sky-specific datasets' terms, OneFormer/Mask2Former weights (COCO/ADE-trained; same dataset question), and whether a lawyer would treat model weights as a derivative of the training images at all. That last question decides how much the dataset terms matter, and it is for the product owner, not this spike.

### 4.6 A sky model trained on documented data (added 2026-10-03, M6 slice 0)

Path 1 of §4.5, chosen by the product owner: train our own. The training project is **independent of this repo** (`mask_training/`, sibling directory, own git repo; scripts, `attribution.csv` listing every image's licence and Flickr source, README with provenance). Only results are recorded here.

**What was trained:** a 1.56 M-parameter U-Net **from scratch** (no pretrained weights, so no ImageNet-style dataset terms), 20 epochs on a COCO-Stuff subset: annotations CC BY 4.0, images only those whose own Flickr licence is CC BY 2.0, "no known copyright restrictions" or US-gov (19,935 of 118,287 train images; NC, ND and SA excluded). Used 6,822 sky images + 5,000 non-sky = 11,822 train, 477 held-out. One seed, one run.

**A mistake worth recording:** the first run counted COCO-Stuff's separate `clouds` class as not-sky and drew some "no sky" images from cloud scenes. It scored held-out IoU 0.670 and visibly failed on clouds and overcast skies (agreement with skyseg 0.21–0.45 on three photos). After counting `clouds` as sky (fog ignored) the same recipe gave the numbers below. Anyone reusing COCO-Stuff for sky must merge both classes.

**Results (run 2):**

| Measure | Value |
|---|---|
| Held-out sky IoU (277 COCO val images with sky, coarse labels) | **0.777** |
| Pixels falsely called sky on 200 val images with no sky | **1.8 %** |
| Agreement with skyseg on the §4.2a photos (IoU, photos 1–8) | 0.99, 0.99, 0.98, 0.77, 0.98, 1.00, 0.89, **0.19** |
| ONNX size / ORT CPU, M1 Pro | 6.2 MB / 33 ms at 288², 104 ms at 512² (matches torch to 4e-6) |

Sheets: `accuracy-sky-trained-part1.jpg`, `-part2.jpg` (red = skyseg, blue = this model; photo 8 is CC BY-SA and is not in a committed sheet).

**Human review (added the same day; supersedes the IoU-based reading above):** 45 further Wikimedia Commons photos (CC0 / public domain / CC BY; list in `RFC-0023-appendix/sky-review/gt_photos_meta.json`) in 12 scene categories were shown to the product owner as blind A/B overlays (our model vs skyseg, order randomised per photo; key sealed until all answers were in). The owner answered per photo with A, B, both, or none. Raw answers: `sky-review/review_answers.json`; key: `review_key.json`; scoring: `score_review.py`.

**Correction (same day) on how those answers were read.** The labelling plan the owner chose was "pick the best candidate"; a later instruction of mine redefined A/B as "only this one is acceptable", and the first write-up scored them that way ("skyseg 36/45, ours 6/45; ours 0/4 on clear sky"). That reading is **not supported**: in at least eight photos the two masks differ by 0.1–0.3 % of the image area (photos 4, 6, 10, 11, 16, 30, 31, 44), which nobody can see, yet one letter was given. The answers are therefore best read as a **forced preference** (A/B = this one is better), with `both` = indistinguishable and `none` = neither is good. **Withdrawn: the 13 % vs 80 % "acceptable" rates, their confidence intervals, and "0 of 4 on clear blue sky".**

**What the answers do support:**

- Preference: skyseg preferred in **32**, ours in **2**, tie (`both`) in **4**, neither good (`none`) in **7**.
- An objective check against the masks the owner did not reject (the 36 photos where skyseg was preferred or tied; its mask is the reference; `eval_refine.py`, `where_errors.py` in `mask_training/`): our model differs from it by **under 1 % of the image area in 18 photos, 1–5 % in 4, and over 5 % in 14**. Mean IoU against that reference is 0.755. So it is close to skyseg on about half of these photos and **wrong by large regions on about 40 %**.
- The large errors are not at edges. Where they occur (photos 1, 7, 8, 13, 18, 19, 22, 23, 25, 27, 28, 29, 36, 43) the model **misses whole regions of dark, grey or monochrome sky (photos 29, 36 are black-and-white and almost entirely missed), blue sky seen through foliage and branches, dark storm cloud, and the top of a saturated red sunset**. Training skews to bright blue/white COCO skies, with only 10 % B&W augmentation.
- **Edge refinement does not help.** A guided filter on the model output at full resolution (radius and eps tuned on odd-numbered photos, tested on even-numbered) moved IoU from 0.755 to 0.755 (test half 0.770 to 0.766). Only 55 % of the disagreeing pixels lie within 12 px of the reference boundary.

- **Stronger augmentation (run 3, same recipe and data, one seed each) helped the dark and saturated skies but not the rest.** Gamma 0.4–2.5, exposure, per-channel gain, channel swaps, contrast/saturation, 30 % B&W and an illumination gradient (`augment_strong` in `train.py`). Against the same 36 reference masks (`eval_model.py`): mean IoU **0.755 → 0.780**; photos within 1 % of the reference **18 → 14**; 1–5 % **4 → 9**; over 5 % **14 → 13**. Large improvements: both sunsets (photos 7, 8: 20 %/23 % wrong → 2 %/2 %), night (28: 12 % → 0.6 %, 27: 35 % → 20 %), one B&W storm (36: 46 % → 25 %). **Regressions** on clear sky (1: 10 → 14 %), a sea horizon (10: 0.1 → 3.6 %), trees (18: 34 → 41 %, 20: 3 → 16 %), a storm (35: 3 → 7 %), a lake (43: 5 → 11 %). **No change on sky seen through branches** (22, 23, 25: 27 %, 20 %, 12 %), and COCO held-out IoU fell 0.777 → 0.734. Run-to-run noise was not measured (epoch-to-epoch validation IoU swings by about 0.03), so the small differences are not trustworthy; the sunset and night gains are large enough to be real. Augmentation alone is therefore not the fix.
- **Higher resolution (run 4: 512-px input, 448-px crops, original augmentation) did not help; it was worse.** Mean IoU against the reference **0.755 → 0.709** (0.689 if fed 288 px), photos over 5 % wrong **14 → 17**, COCO held-out IoU 0.736 (same range as the other runs). It also regressed both sunsets (20 %/23 % → 54 %/33 % wrong) and a night shot. Confounds: 14 epochs and batch 16 rather than 20 and 32, one seed, noise unmeasured.
- **The bare-branch and some night photos are complete misses, not edge or resolution errors.** On photos 22, 23 and 29 the run-2 and run-4 models predict **0.00 % sky** (maximum probability 0.06–0.30), so the error equals the whole reference sky area (26.9 %, 19.5 %, 41.3 %) for every model. They do not recognise grey overcast sky behind winter branches, or black-and-white night sky, as sky at all. **This replaces the earlier guess that thin structure/288 px is the limit.** What is left to try: more diverse training images of those appearances, more global context or capacity in the model, longer training.
- **More landscape-style training data (desk search only; nothing downloaded):** COCO-Stuff's shippable images are already all used. The *Sky Detection Dataset* (Zenodo 15488022, 1,708 images) is CC BY 4.0 for the masks but its images were "collected from publicly available sources" and are "intended for academic and non-commercial research", so it is excluded. **SkyFinder** (Zenodo 5884485, CC BY 4.0, 53 static AMOS webcams, one hand-made mask per camera, ~90k frames in 10.5 GB, many weather/night/fog conditions) would add dark and foggy skies, but the page and Zenodo record state nothing about the rights of the underlying webcam images, the AMOS archive's own terms could not be fetched (certificate error), and 53 scenes would add no foliage or branches. Open Images has mask annotations for 350 classes but a sky class was not confirmed. Not pursued without a decision on the webcam-rights question.

- **A larger model with global context (run 5) did not fix it, so the stopping rule was applied.** U-Net widened to 4.9 M parameters with 4 self-attention layers at the 1/16 bottleneck (`model_big.py`), strong augmentation, 24 epochs (~3.6 h on the M1 Pro GPU), 288-px input. Criteria fixed *before* the run: reference-set mean IoU >= 0.80, at most 8 photos over 5 % wrong, and at least half of the reference sky predicted on two of the three complete-miss photos (22, 23, 29), within a guard of <= ~120 ms CPU at 288 px and <= 20 MB. **Result: mean IoU 0.753, 12 photos over 5 % wrong, sky predicted on 0 of the 3 complete-miss photos (still 0.00 %), COCO held-out IoU 0.714.** It fixed some sunsets (13: 12 % → 3 % wrong) and broke others (33 fog: 0.2 % → 8 %; 18 trees: 34 % → 58 %). Speed/size were within the guard (74 ms at 288² on ORT CPU, 19.8 MB) but are moot. Caveat for any future attempt: the ONNX export of the attention block bakes in the token grid, so the model only runs at the input size it was exported with (a 512-px input failed on a 288-exported file); fixed-size input would have been required.
- **Conclusion of this attempt (five from-scratch runs, ~12 h of GPU time, 36 reference photos):** nothing trained here reaches skyseg quality; the best run (3, strong augmentation) scored IoU 0.780 and left 13 photos over 5 % wrong. Augmentation, resolution and model size/context each moved individual cases both ways and none fixed the recognition failures (bare branches, B&W night sky). The remaining untried lever is **more diverse training images with sound rights** (SkyFinder is the only candidate found, with the webcam-rights question above); training recipes were not the bottleneck within the budget. Per the agreement made before run 5, the project falls back to the non-ML recommendation in §5.2.

**Caveats:** one rater; the reference mask is skyseg's, chosen by the owner over ours (a mask judged better, not proven correct); 45 photos, four per category, chosen by search ranking; photo 5 is a painting. Skyseg is itself not shippable (§4.3).

**What was established regardless of the above:** the training pipeline is reproducible and its provenance is documentable (dataset, licences per image, recipe, hashes in the independent `mask_training/` project); the run-1 label mistake (§4.6 above) cost 0.1 IoU; a model of this size and recipe runs at 33 ms (288²) on an M1 Pro CPU. **What is not established:** whether the failure is the model, the coarse COCO-Stuff labels, the 288-px resolution, or the lack of landscape-style training data. Edge refinement was tried and does not help (above). The failure pattern (dark, mono, through-foliage and saturated skies) points at training data and augmentation first (strong exposure/gamma/hue/B&W augmentation, landscape-style images), then model capacity and input resolution. Augmentation (run 3) and resolution (run 4) were then tried: neither fixed it, and the worst cases are complete misses (no sky predicted). Capacity, global context and more diverse data remain untried.

## 5. Recommendations per M6 feature area

These are recommendations to start M6 as a build milestone, not final model choices; each needs the quality evaluation this spike did not do.

1. **Subject / object selection (click, box):** **SAM 2 Tiny** is now the baseline, **not MobileSAM**: in the §4.2 smoke test MobileSAM's single-mask export was unfit as a one-click selector, while SAM 2 Tiny's three candidates produced 4 clean masks out of 6 and 2 part/whole ambiguities. Costs: encoder 1.0 s on CPU (vs 0.35 s), ORT only (`tract` cannot load it), and the upstream ONNX export is flagged experimental. Run the encoder in the background after Develop opens an image and cache the embedding by content hash like the existing Develop preview cache (≈ 1 s once per image, then ~20 ms per click). **The UI must offer the three candidates / a refine click**; auto-picking the top-IoU mask picked the wrong level in 2 of 6 tries. A larger SAM 2 variant and a multi-mask MobileSAM export were not tried.
2. **Sky (final for slice 0):** **no trained sky model ships in M6's first cut.** `skyseg` is the best performer measured (§4.2a, preferred in 32 of 45 reviewed photos, §4.6) but cannot ship (provenance §4.3, false positive on a bright blurred background §4.2). A from-scratch model on documented data (§4.6, five runs) did not get close: it fails by whole regions on dark, grey, mono and through-foliage skies. **SAM 2 with a sky click is not a sky detector either** (§4.2a: leaves out clouds, fragments, bleeds on low-contrast scenes). **Recommendation: *Select Sky* in M6 is the existing luminance/colour-range mask (deterministic, user-adjustable, no model), and SAM 2 Tiny is offered only as the general click-to-select object tool (§5.1), with its sky weaknesses documented in the UI copy.** Reopen only if (a) a sky model with documented data and weights terms is found, or (b) more diverse training images with sound rights become available (e.g. SkyFinder after its webcam-image rights are resolved) and a new run beats the reference-set criteria in §4.6. Edge refinement was tested and does not help (§4.6).
3. **AI denoise:** **SCUNet (Apache-2.0) on ORT CPU, scoped as an explicit action / export-time step with a crop preview**, not a live slider. The 6-minute full-frame extrapolation is the main product risk; a lighter model should be evaluated before committing (candidates *named but not tested*: NAFNet-class, DRUNet-class). Do not use CoreML for it (measured pathological).
4. **Super-resolution:** **Real-ESRGAN `realesr-general-x4v3`** (4.9 MB) on **ORT CoreML on macOS** (0.37 vs 0.55 s on the one size measured) with CPU fallback, as an explicit action with tiling. **Confirm the weights' license** before shipping (code is BSD-3; the weights' terms are not stated in the upstream README).
5. **Composable masking (add/subtract/intersect):** no model; this is mask-engine work in M6's own scope and needs no research here.
6. **Generative removal/fill (M7):** **not surveyed in this pass.** M7 keeps ownership of that decision (MILESTONES), but the M5.7 scope also asked for an awareness survey; that remains open.

## 6. Runtime decision (proposed — see ADR-0009)

Add **ONNX Runtime via the `ort` crate** behind one thin `ml` module, **CPU execution by default, an execution provider opted in per model only where a benchmark showed a win** (CoreML for Real-ESRGAN and skyseg-class models on macOS; DirectML on Windows is **unmeasured** and stays desk research until run). Keep **`tract` for the existing face models** (ADR-0007) unchanged — they work, and moving them is out of scope. The two runtimes coexist; the cost is a second inference dependency and a native library per platform (the ADR-0003 class of cost), accepted because `tract` cannot load the models M6 needs and is several times slower on the ones it can.

## 7. Open questions M6 slice 0 must close (not answered here)

- **Windows:** DirectML/CUDA numbers on a real GPU machine (the CI runner has no DX12 device). Linking on the project's MSVC/vcpkg setup is closed (§4.4 point 5).
- **Accuracy:** run each shortlisted model on real photographs (landscape/sky, portrait, busy scenes) and judge masks, denoise, and upscale by eye and with a simple metric; this spike's timing says nothing about quality.
- **Minimum hardware:** only an M1 Pro (a high-end laptop chip) was measured. M6's "define the target machine spec" is not done.
- **`ort` link mode and stability:** whether the `load-dynamic` feature (load the runtime library at run time rather than link it) removes the linking cost, how the pre-release `2.0.0-rc` series should be pinned, and what the signed-binary/notarization story is for a bundled `libonnxruntime`. Not verified.
- **CoreML compile cache** and why SCUNet partitions so badly (try `MLComputeUnits=CPUAndGPU`, static shapes, or the ORT compiled-model cache).
- **Static-shape re-exports** for `tract` where a model is wanted in pure Rust (SAM 2, SCUNet).
- **Generative fill/remove** awareness survey (§5.6).

## 8. Non-goals

Implementation of any M6 feature; a quality benchmark; picking M7's approach; cloud inference (out of scope for the product).
