# Face detection / embedding models (RFC-0005 §3.3, §5)

Resolved 2026-09-08 after a real license/provenance research pass (RFC-0005
§5 explicitly required this before any implementation, not a rubber-stamp of
the model *family* recommendation alone). Both files come from the same
upstream project, `opencv/opencv_zoo`.

## Detection: YuNet

- **File**: `face_detection_yunet_2023mar.onnx`
- **Source**: https://github.com/opencv/opencv_zoo/blob/main/models/face_detection_yunet/face_detection_yunet_2023mar.onnx
- **Pinned commit**: `f12e12798e8314f7c074a6656816c048dcc95b7a` (last commit to touch this file), verified against `opencv/opencv_zoo` main at `47534e27c9851bb1128ccc0102f1145e27f23f98`
- **Blob SHA**: `2d8804a5986e229f1fde3a1994feacc66c91b58b`
- **Size**: 232,589 bytes (~227 KiB)
- **License**: MIT — confirmed directly from `models/face_detection_yunet/LICENSE` in the upstream repo. No ambiguity found.
- **Shape**: dynamic input (any resolution), outputs bounding box + 5-point landmarks (eyes/nose/mouth corners) per detected face.
- **Training data**: not documented in the model's own README (it points at a separate training repo, `ShiqiYu/libfacedetection.train`, without restating the dataset there); believed to be WIDER FACE (a standard, uncontroversial academic benchmark) based on general familiarity with this model family, but this was **not independently confirmed from a primary source** in this research pass — flagged honestly rather than stated as verified.

## Embedding: SFace

- **File**: `face_recognition_sface_2021dec.onnx`
- **Source**: https://github.com/opencv/opencv_zoo/blob/main/models/face_recognition_sface/face_recognition_sface_2021dec.onnx
- **Blob SHA**: `5817e559d509b2c1d5069f3c49a388bc45d4395f`
- **Size**: 38,696,353 bytes (~36.9 MiB) — notably larger than RFC-0005 §3.3's original "typically 1-5MB" assumption, which is why these are fetched-once-and-cached rather than committed to the repo (see "Storage" below).
- **License**: Apache 2.0, per `models/face_recognition_sface/LICENSE` in the upstream repo. **Provenance caveat, checked and worth stating plainly**: the *original* upstream repo for this model (`zhongyy/SFace`) carries no license file of its own, and an [unanswered 2021 GitHub issue](https://github.com/opencv/opencv/issues/21192) shows someone else asked exactly this question and got no reply. The Apache-2.0 grant in `opencv_zoo` was added in the same commit that added the model, contributed by Yaoyao Zhong — who appears to be the model's own original author, contributing their own work under `opencv_zoo`'s license terms. That's a credible chain, not an independent third-party confirmation.
- **Training data**: MS1MV2, a refined cut of **MS-Celeb-1M** — a dataset Microsoft took down in 2019 after journalists (the Exposing.ai / MegaPixels project) found it had scraped facial images without meaningful consent. This is a real, known ethical concern, distinct from the license question above, and not unique to this model: nearly every high-accuracy face-embedding model published before ~2020 (ArcFace, CosFace, the various MobileFaceNet checkpoints, InsightFace's `buffalo_l`/`antelopev2`) shares this same training-data lineage.
- **Ruled out instead**: InsightFace's `buffalo_l`/`antelopev2`/MobileFaceNet family — code is MIT, but the pretrained *weights* are explicitly "non-commercial research purposes only" per InsightFace's own licensing terms, incompatible with this repo's own MIT license (which permits commercial use/resale of "the Software"). A fully synthetic-data-trained alternative exists ([fdbtrs/SFace-synthetic](https://github.com/fdbtrs/SFace-Privacy-friendly-and-Accurate-Face-Recognition-using-Synthetic-Data), avoids the real-photo consent issue entirely) but ships no pretrained weights at all — only training code — so adopting it would mean training a model from scratch, a substantially bigger undertaking, for meaningfully lower accuracy (~92% LFW vs. ~99%+ for MS1M-trained models).
- **Explicit decision, made 2026-09-08**: ship SFace anyway, accepting the training-data provenance concern as a known, named tradeoff rather than a silently-ignored one — the alternative (train a lower-accuracy model from scratch) was judged not worth the accuracy cost for this feature's purpose (local photo organization, not a commercial face-recognition product).

## Storage: fetched once, cached — not committed to the repo

RFC-0005 §3.3 leaned toward committing model files, reasoning they'd be
"typically 1-5MB each." SFace alone is ~37MB — nearly half this repo's
current total `.git` size (~72MB) for one binary asset, and this repo has no
Git LFS configured. Given that, and that the license terms (unlike the
ruled-out InsightFace family) don't actually require committing, both files
are fetched once on first use into the app's data directory and cached
there — the same "fetch once, never re-fetch at inference time" shape
RFC-0005 §3.3 always intended, just resolved toward download-and-cache
rather than commit now that the real file size is known.

## Verification

`tract`'s ONNX op coverage against these two specific files is an empirical
check still to be done at integration time (RFC-0005 §3.2's own named risk:
"must be verified empirically against whatever specific ONNX model is
chosen... not assumed") — not yet performed as of this document.
