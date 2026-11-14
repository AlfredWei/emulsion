# RFC-0038 appendix: training data and terms of the bundled and downloaded models

Prepared 2026-11-14 from the repository (read only) and web pages opened the same day, by two independent research runs that were then
merged (provenance at the end). This is engineering research, not legal advice. Where a page says something about a licence, it is
reported as what the page says. **Whether a dataset's terms reach a model trained on it is an open legal question throughout; it goes
to counsel and is not answered here.** Two verdict-driving pages (Places2's archived download page, WIDER FACE's own page) could not be
re-opened when the runs were merged; they rest on the researchers' reads, and the WIDER FACE label was also seen on a Hugging Face copy.

Why this exists: `THIRD_PARTY_NOTICES.md` records each model's code and weight licence, not what it was trained on. For a free MIT
tool that was an accepted gap (ADR-0010 accepted Places2 with the terms unread; `models/FACE_MODELS.md` accepted SFace's training-data
provenance on 2026-09-08 "for local photo organization, not a commercial face-recognition product"). For a paid product it is not.

**Verdict rule** (one rule for all eight, so counsel can overturn the rule instead of eight judgments):
- **No known blocker**: permissive weights licence, and the training data is permissive or owned by the party that licensed the weights.
- **Needs a closer look**: the main training data is permissive or has no stated licence, but some part of the lineage may carry
  non-commercial terms (a pretrained backbone, a "research only" sentence without a licence, an undocumented recipe).
- **Likely a blocker**: the main training dataset's own page states non-commercial terms and its owner is not the party that licensed
  the weights.

## 1. NAFNet-SIDD-width32 (AI Denoise, in the installer)
- **Weights licence**: MIT, Copyright (c) 2022 megvii-model; the same file carries Apache-2.0 for BasicSR (https://raw.githubusercontent.com/megvii-research/NAFNet/main/LICENSE).
- **Training data**: SIDD only; the upstream config reads SIDD crops and sets `pretrain_network_g: ~`, so no pretrained initialisation (https://raw.githubusercontent.com/megvii-research/NAFNet/main/options/train/SIDD/NAFNet-width32.yml).
- **Dataset terms**: "The dataset and the associated code repositories are under the MIT License" (https://abdokamel.github.io/sidd/, https://www.eecs.yorku.ca/~kamel/sidd/). Attribution; no commercial restriction stated.
- **Conversion**: our own ONNX export. The checkpoint came from Hugging Face mirrors; `nyanko7/nafnet-models` has no card and states no terms.
- **Verdict**: no known blocker. Confidence: high. Not retrieved: a byte-for-byte comparison of the mirror's file with upstream (already open in `DENOISE_MODEL.md`).

## 2. SAM 2 Hiera Tiny (Select Subject, in the installer)
- **Weights licence**: "The SAM 2 model checkpoints, SAM 2 demo code (front-end and back-end), and SAM 2 training code are licensed under Apache 2.0" (https://github.com/facebookresearch/sam2).
- **Training data**: "Our released model is trained on SA-V manual + Internal and SA-1B", plus "internally available licensed video data" (https://arxiv.org/html/2408.00714).
- **Dataset terms**: SA-V is CC BY 4.0 (https://ai.meta.com/datasets/segment-anything-video/). SA-1B: "Limited; see full license language", intended use "Research purposes only" (https://ai.meta.com/datasets/segment-anything/). Meta's internal video: terms not public.
- **Why not a blocker**: Meta owns SA-1B and SA-V and licensed the weights itself; SA-1B's research-only terms limit what others may do with the dataset, which we neither receive nor use. One run rated this "needs a closer look" because the SA-1B licence text was not read; that text is still unread.
- **Conversion**: `vietanhdev/segment-anything-2-onnx-models` says "License unchanged from upstream (Apache 2.0)" and adds no terms (https://huggingface.co/vietanhdev/segment-anything-2-onnx-models).
- **Verdict**: no known blocker. Confidence: medium. Not retrieved: the SA-1B licence text; the internal data; what the Hiera encoder was initialised from.

## 3. D-FINE medium COCO (person detection, in the installer)
- **Weights licence**: Apache-2.0 (https://huggingface.co/ustc-community/dfine-medium-coco, https://github.com/Peterande/D-FINE). Upstream warns that Objects365 checkpoints "should not be assumed to be commercially cleared"; ours is the COCO-only checkpoint.
- **Training data (detector)**: COCO train2017. COCO terms: annotations "licensed under a Creative Commons Attribution 4.0 License"; "The COCO Consortium does not own the copyright of the images. Use of the images must abide by the Flickr Terms of Use" (https://github.com/cocodataset/cocodataset.github.io/blob/master/dataset/termsofuse.htm). Images are not in the model file.
- **Training data (backbone, new finding)**: the M config uses HGNetv2 `B2` with `pretrained: True`, and the code downloads `PPHGNetV2_B2_stage1.pth` (configs `dfine_hgnetv2_m_coco.yml`, `include/dfine_hgnetv2.yml`, `src/nn/backbone/hgnetv2.py` in https://github.com/Peterande/D-FINE). The timm card for the same-named checkpoint says "Trained by model authors on mined ImageNet-22k and ImageNet-1k using SSLD distillation" (https://huggingface.co/timm/hgnetv2_b2.ssld_stage1_in22k_in1k). ImageNet's terms: "Researcher shall use the Database only for non-commercial research and educational purposes" (via https://huggingface.co/datasets/imagenet-1k/blob/main/README.md; image-net.org refused the connection).
- **Conversion**: our own export; no added terms.
- **Verdict**: needs a closer look. COCO is fine on its face; the ImageNet lineage enters through the backbone the shipped weights were fine-tuned from. Almost every detector shares that lineage, so this is a policy question for counsel (decision D7). Confidence: medium. Not retrieved: whether the file is byte-identical to the PaddleClas/timm checkpoint (matched by name only).

## 4. LaMa big-lama (Remove tool, in the installer)
- **Weights licence**: Apache-2.0, Samsung Research 2021 (upstream `LICENSE` read 2026-11-14, #331).
- **Training data**: "The best model (Places2, Places Challenge)"; the training setup also downloads an ADE20K ResNet50-dilated network "for _perceptual loss_" (https://github.com/advimman/lama). That network is used only during training and is not in the shipped weights.
- **Dataset terms** (from the researcher's read of archived copies; not re-opened at merge): Places2, archived download page (2021-02-20): "You will use the data only for non-commercial research and educational purposes" and "You will NOT distribute the above images" (web.archive.org/web/20210220220343/http://places2.csail.mit.edu/download.html). Today's page offers the legacy dataset "for research purposes" via a form. ADE20K (archived 2024): "Researcher shall use the Database only for non-commercial research and educational purposes". The other run could not reach places2.csail.mit.edu at all.
- **Conversion**: `Carve/LaMa-ONNX` is Apache-2.0, adds no terms and says nothing about training data (https://huggingface.co/Carve/LaMa-ONNX).
- **Verdict**: likely a blocker for a paid product, until counsel says dataset terms do not reach the weights. Confidence: medium. ADR-0010 accepted Places2 with the terms unread, for an MIT app; this is the first reading and it is non-commercial. Not retrieved: Places2's current terms text; whether "Places Challenge" has its own terms.

## 5. Real-ESRGAN realesr-general-x4v3 (Upscale, in the installer)
- **Weights licence**: BSD-3-Clause (the repository licence; the weights state no separate terms).
- **Training data**: not documented for this checkpoint. The model zoo calls it only "A tiny small model", and the v0.2.5.0 release notes do not mention it. The project's general recipe is "DF2K (DIV2K and Flickr2K) + OST" (https://github.com/xinntao/Real-ESRGAN/blob/master/docs/Training.md).
- **Dataset terms, if that recipe was used**: DIV2K "made available for academic research purpose only"; "the copyright belongs to the original owners" (https://data.vision.ee.ethz.ch/cvl/DIV2K/). Flickr2K: "collected by ourselves using Flickr API", no licence stated (https://github.com/limbee/NTIRE2017). OST: no licence stated (https://github.com/xinntao/SFTGAN).
- **Conversion**: our own export; no added terms.
- **Verdict**: needs a closer look, leaning towards a blocker. Confidence: low, because the checkpoint's real training data is unknown. Least provenance of the eight. Not retrieved: the checkpoint's actual training set; whether a VGG perceptual loss was used.

## 6. MediaPipe Face Mesh V2 (eye check, in the installer)
- **Weights licence**: "LICENSED UNDER Apache License, Version 2.0", authors at Google (the model card PDF on storage.googleapis.com/mediapipe-assets, read in full by one run).
- **Training data**: per the card, "All dataset images were captured on a diverse set of smartphone cameras … via an AR (Augmented Reality) application"; evaluation samples came from the same source.
- **Dataset terms**: none published; the data appears to be Google's own, and Google licensed the weights.
- **Conversion**: `fernandotonon/QtMeshEditor-facemesh-onnx` is Apache-2.0 for "code AND weights" and adds no terms (https://huggingface.co/fernandotonon/QtMeshEditor-facemesh-onnx). The file is mirrored from an aggregate repository (QtMeshEditor-models) whose own terms are not stated.
- **Verdict**: no known blocker. Confidence: medium. Not retrieved: the conversion script's licence; the model bundle's own licence file.

## 7. YuNet 2023mar (face detection, downloaded on first use)
- **Weights licence**: "All files in this directory are licensed under MIT License" (https://github.com/opencv/opencv_zoo/tree/main/models/face_detection_yunet).
- **Training data**: "YuNet face detection training on WIDER Face" (https://github.com/ShiqiYu/libfacedetection.train, itself BSD-3). This confirms what `FACE_MODELS.md` had left unconfirmed.
- **Dataset terms**: WIDER FACE's page gives its licence as "Creative Common License (cc by-nc-nd)" (http://shuoyang1213.me/WIDERFACE/; the same label was seen on a Hugging Face dataset card).
- **Conversion**: none; opencv_zoo is the publisher.
- **Verdict**: likely a blocker. It gates both face grouping and the blink check. Confidence: medium. Not retrieved: the image terms of WIDER's parent dataset.

## 8. SFace 2021dec (face grouping, downloaded on first use)
- **Weights licence**: Apache-2.0 for the opencv_zoo directory (https://github.com/opencv/opencv_zoo/tree/main/models/face_recognition_sface); the original repository has no licence (https://github.com/zhongyy/SFace).
- **Training data**: not stated for the released checkpoint. opencv_zoo says only "MobileFaceNet instances trained on the SFace loss function"; the paper's experiments use "CASIA-WebFace, VGGFace2, and MS-Celeb-1M" (https://arxiv.org/abs/2205.12010). `FACE_MODELS.md` names MS1MV2; neither run found a source for that.
- **Dataset terms**: MS-Celeb-1M's licence allowed use "for the limited purpose of conducting non-commercial research" and "prohibits distribution of the dataset or derivatives"; the same source says the "Implication on pre-trained models is unclear" (https://ar5iv.labs.arxiv.org/html/2108.02922). Microsoft withdrew the dataset in 2019, saying it "was intended for academic purposes".
- **Conversion**: published inside opencv_zoo under Apache-2.0; no added terms, but the chain is weak.
- **Verdict**: likely a blocker; separately, face embeddings raise biometric-data questions for a paid product (decision D11). Confidence: medium. Not retrieved: CASIA-WebFace and VGGFace2 terms; which dataset this checkpoint actually used.

## Summary table

| Model | Weights licence | Training data | Dataset terms (as read) | Conversion adds terms? | Verdict | Confidence |
|---|---|---|---|---|---|---|
| NAFNet-SIDD-w32 | MIT (+Apache-2.0 BasicSR) | SIDD, no pretraining | MIT | No | No known blocker | High |
| SAM 2 Hiera Tiny | Apache-2.0 (Meta) | SA-V + SA-1B + Meta internal video | SA-V CC BY 4.0; SA-1B research-only, Meta-owned; internal not public | No | No known blocker | Medium |
| D-FINE M COCO | Apache-2.0 | COCO; HGNetV2-B2 stage1 init (ImageNet-22k/1k) | COCO annotations CC BY 4.0, images under Flickr terms; ImageNet non-commercial | No | Needs a closer look | Medium |
| LaMa big-lama | Apache-2.0 | Places2/Challenge; ADE20K network in the loss | Places2 non-commercial, no redistribution; ADE20K non-commercial | No | Likely a blocker | Medium |
| Real-ESRGAN general-x4v3 | BSD-3 | Undocumented; recipe DF2K + OST | DIV2K "academic research purpose only"; Flickr2K/OST none stated | No | Needs a closer look (leaning blocker) | Low |
| MediaPipe Face Mesh V2 | Apache-2.0 (Google) | Google's own smartphone/AR captures | Not published | No (aggregate repo unstated) | No known blocker | Medium |
| YuNet | MIT | WIDER FACE | CC BY-NC-ND | n/a | Likely a blocker | Medium |
| SFace | Apache-2.0 (opencv_zoo); upstream has none | CASIA-WebFace / VGGFace2 / MS-Celeb-1M (not stated which) | MS-Celeb-1M non-commercial, withdrawn | No extra terms; weak chain | Likely a blocker (+ biometric) | Medium |

## Provenance

Two independent runs (a Sonnet run and an Opus run) each opened sources and wrote a report; the Opus run opened more primary pages and
found the D-FINE backbone lineage, the archived Places2 and ADE20K terms and WIDER FACE's own licence line, so it is the base. Kept from
the Sonnet run: the SAM 2 caution about the unread SA-1B text, the aggregate-repository caveat for the Face Mesh conversion, and the
reminder that the earlier owner acceptances (ADR-0010, FACE_MODELS.md) were made for a free tool. Where the runs disagreed (SAM 2:
"closer look" against "no known blocker"; YuNet: "closer look" against "likely a blocker") the verdict follows the run that read the
primary page, and the other reading is noted. Not verified by the merge: every URL (the two flagged pages could not be re-opened).
