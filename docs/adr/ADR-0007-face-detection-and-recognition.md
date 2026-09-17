# ADR-0007: Face detection & recognition — `tract` (pure-Rust) inference, YuNet + SFace

- Status: Accepted
- Date: 2026-09-17
- Relates to: [ADR-0003](ADR-0003-raw-decoding.md), [RFC-0005](../rfc/RFC-0005-face-detection-people-view.md), [models/FACE_MODELS.md](../../models/FACE_MODELS.md), [PRD MILESTONES §M5](../../PRD/MILESTONES.md#m5--performance-gpu-merges-faces)

## Context

M5 scopes local face detection/recognition for a "People" browsing view, fully local with no cloud model dependency ([RFC-0005](../rfc/RFC-0005-face-detection-people-view.md)). Unlike HDR merge and Panorama merge — hand-rolled, directly-testable algorithms with no external dependency — face detection and embedding require a trained neural network, so this is the first feature in the codebase needing an ML inference runtime and pretrained model weights. No existing ADR governs this decision space: ADR-0003 (RAW decoding) and ADR-0004 (rendering/color) are adjacent but don't cover inference-runtime or model-provenance choices.

Two real Rust-ecosystem options exist for running a pretrained ONNX model:

1. **`ort`** (ONNX Runtime's official Rust bindings) — mature, fast, broad op coverage, GPU execution providers available, but FFI-links Microsoft's native `onnxruntime` C++ library, which must be present/linked per platform.
2. **`tract`** — a pure-Rust ONNX/NNEF inference engine (Sonos). No native library to link, at the cost of narrower op coverage and CPU-only execution.

## Decision

Use **`tract`** (`tract-onnx` 0.23.7) as the inference runtime, running two pretrained ONNX models sourced from `opencv/opencv_zoo` and documented in [models/FACE_MODELS.md](../../models/FACE_MODELS.md): **YuNet** (detection — bounding box + 5-point landmarks, MIT license) for detection, and **SFace** (128-dim embedding, Apache 2.0 license) for embedding. Model weights are fetched once on first use into the app's data directory and cached there — never committed to the repo, never re-fetched at inference time.

## Rationale

- **`tract` avoids repeating a cost this project already paid once.** `rsraw`/`rsraw-sys`'s vendored LibRaw ([ADR-0003](ADR-0003-raw-decoding.md)) needed a version-pinned vcpkg install, a build-script patch, and two real CI debugging passes to get Windows/MSVC linking working. Adding `ort` → `onnxruntime` would reintroduce the exact same *class* of cross-platform native-linking risk for a second dependency. `tract` being pure Rust sidesteps that risk entirely.
- **CPU-only is an accepted tradeoff, not an oversight.** Face detection/embedding over a photo library runs as a background/batch job (per-import-batch detection, explicitly-triggered clustering), not a `<100ms` interactive path like Develop's GPU shader pipeline ([ADR-0004](ADR-0004-rendering-and-color-management.md)) — throughput matters here, not latency, and `tract`'s CPU performance is adequate for small mobile-class detection/embedding models at that pace.
- **Narrower op coverage was named as a real, empirically-checked risk, not assumed away.** Verified 2026-09-09 against the actual chosen model files: `tract-onnx` 0.23.7 loads both YuNet and SFace with no missing-op errors, and — the stronger check, since a graph can parse while still hitting an unimplemented kernel at execution time — actually runs both to completion, producing correctly-shaped output tensors and a real, correctly-differentiated face embedding against a real test photo.
- **YuNet + SFace, not the higher-accuracy InsightFace family.** InsightFace's `buffalo_l`/`antelopev2` weights are licensed "non-commercial research only," incompatible with this repo's own MIT license. YuNet (MIT) and SFace (Apache 2.0) are both usable; full research and license chain-of-custody is in [models/FACE_MODELS.md](../../models/FACE_MODELS.md), including a named, accepted tradeoff: SFace's training data (MS1MV2/MS-Celeb-1M) carries a known consent controversy shared by nearly the entire pre-2020 face-embedding model family.
- **Fetched-once-and-cached, not committed.** SFace alone is ~37MB — nearly half this repo's total `.git` size at the time of the decision for one binary asset, with no Git LFS configured — so despite [RFC-0005](../rfc/RFC-0005-face-detection-people-view.md) §3.3's original lean toward committing (on an assumption of 1–5MB models), the real file size settled this toward download-and-cache into the app's data directory.

## Consequences

- The app requires a one-time network fetch of model weights before face detection can run for the first time — a deliberate, narrow exception to "inference itself never makes a network call," scoped to weight acquisition only, consistent with this project's local-first stance ([[lr_replace_project_context]]).
- `tract`'s op coverage is now a standing constraint on any *future* model swap (e.g., a higher-accuracy or synthetic-data-trained embedding model): a candidate model must be verified against `tract` before adoption, the same way YuNet/SFace were, not assumed compatible.
- Detection/embedding is CPU-bound and sequential per import batch — acceptable for the "browsing/culling aid" framing this feature has ([RFC-0005](../rfc/RFC-0005-face-detection-people-view.md) §2's non-goals explicitly exclude any latency-sensitive or forensic-grade claim), but a real, named limit if this project ever wants GPU-accelerated or real-time inference later — that would require revisiting this decision (`ort`, or a `tract` GPU backend if one matures).
- Model provenance/license must be re-verified and re-documented in `models/FACE_MODELS.md`-style form for any future model addition or swap — this is now the established pattern for how this project handles third-party ML weights, not a one-off exercise.

## Alternatives considered and rejected

- **`ort` (ONNX Runtime bindings)**: rejected — broader op coverage and GPU support don't outweigh reintroducing a native C++ FFI-linking dependency of the exact class already shown costly in [ADR-0003](ADR-0003-raw-decoding.md), for a workload (small mobile-class models, batch throughput) that doesn't need `ort`'s extra headroom.
- **InsightFace `buffalo_l`/`antelopev2` model family**: rejected — pretrained weights are non-commercial-only, incompatible with this repo's MIT license, regardless of runtime choice.
- **A synthetic-data-trained embedding model (e.g. `fdbtrs/SFace-synthetic`)**: rejected for v1 — avoids the real-photo consent-provenance concern entirely, but ships no pretrained weights (training code only), meaning adopting it now would mean training a model from scratch for meaningfully lower accuracy (~92% LFW vs. ~99%+ for MS1M-trained models). Worth revisiting if that project or an equivalent ever ships usable pretrained weights.
- **Committing model weights to the repo**: rejected once real file sizes were known (SFace ~37MB, no LFS configured) — fetch-once-and-cache instead, matching how the same "fetch once, never re-fetch at inference time" property was always intended, just resolved toward download rather than commit.
