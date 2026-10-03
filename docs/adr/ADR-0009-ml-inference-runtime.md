# ADR-0009: ML inference runtime for M6 — ONNX Runtime (`ort`) alongside `tract`

- Status: **Proposed** (macOS accuracy/speed evidence; Windows links and runs on CPU, DirectML unmeasured — see Consequences)
- Date: 2026-10-02
- Relates to: [RFC-0023](../rfc/RFC-0023-on-device-ai-inference.md) (the measurements), [ADR-0007](ADR-0007-face-detection-and-recognition.md), [ADR-0003](ADR-0003-raw-decoding.md), [PRD MILESTONES §M5.7/§M6](../../PRD/MILESTONES.md)

## Context

M6 names an "architecture decision required before scoping in detail": on-device-only inference (no cloud calls, per the local-first constraint) and which runtime runs it. ADR-0007 chose `tract` (pure Rust, CPU only) for the small face models and said any future model "must be verified against `tract` before adoption". M5.7 did that verification for the M6 candidates (MobileSAM, SAM 2 Tiny, a sky model, SCUNet, Real-ESRGAN) on an Apple M1 Pro; results are in RFC-0023 §4.

## Decision

1. **On-device inference only** — confirmed feasible on the measured machine for interactive click-to-mask (≈ 20 ms per click after a 0.35 s per-image encoder pass) and for denoise/super-resolution as explicit, non-interactive actions (minutes per full frame, extrapolated).
2. **Add ONNX Runtime through the `ort` crate** for M6 models, behind one thin Rust module, **CPU by default; an execution provider (CoreML on macOS, DirectML on Windows) only per model and only where a benchmark showed a win.**
3. **Keep `tract` for the face models** (ADR-0007). Both runtimes coexist.
4. Model weights keep ADR-0007's pattern: fetched once and cached, never committed, provenance and license re-verified and written down per model before adoption.

## Rationale (all from RFC-0023 §4, one machine)

- `tract` **failed to load three of the five model families** (SAM 2 encoder and decoder, SCUNet) plus the MobileSAM decoder as exported, and where it loads a model it is **1.5–5× slower** than ORT CPU (MobileSAM encoder 1.79 s vs 0.35 s).
- A GPU/NPU provider is **not a blanket win**: CoreML was 2.7× faster for the sky model, 1.5× for Real-ESRGAN, **slower for both SAM encoders, and 65× slower for SCUNet**. Hence per-model, measured, CPU-default provider selection, which only a runtime with providers can do.
- Cost: a native library per platform (33 MB on macOS arm64) — the same class of cost ADR-0003 paid for LibRaw. Accepted because the alternative cannot run the models.

## Consequences / what is *not* settled

- **Proposed, not Accepted.** Windows is partly closed (RFC-0023 §4.4): `ort` links statically on MSVC both standalone and inside the app crate next to vcpkg LibRaw/`tract`/`rustls` (CI green on Windows and macOS, probe test passes), and CPU inference runs. **Still unknown: DirectML speed** (the CI runner has no DX12 device; the provider must be requested with CPU fallback because session creation hard-fails without an adapter) and `DirectML.dll` shipping. Accept after a real-Windows-GPU measurement, or amend if it fails.
- `ort` has **no stable 2.0** (latest `2.0.0-rc.13`, 2026-07-28): pin an exact version and budget for API churn. Whether `load-dynamic` linking avoids the link cost, and how a bundled runtime library (macOS dylib, `DirectML.dll`) is signed/notarized, are unverified.
- **No accuracy was measured.** Model choices in RFC-0023 §5 are baselines to be judged on real photographs, not decisions. Two have unresolved *license* questions that block shipping: `skyseg` (undocumented training data, third-party re-host) and Real-ESRGAN weights (no stated weight license).
- Only an M1 Pro was measured; M6's minimum-hardware spec is undefined.

## Alternatives considered

- **`tract` only**: rejected on the evidence above (cannot load M6's models as exported; much slower where it can). A static-shape re-export might rescue some models; not attempted, and it would not fix the speed gap.
- **ORT via a vendor-specific SDK per platform (CoreML/Core ML Tools on macOS, DirectML/WinML on Windows) directly**: not evaluated — would double the integration surface; `ort` already exposes both providers.
- **`candle` / `burn` (pure-Rust frameworks)**: not evaluated; no measurement was made, so no claim is made about them.
- **Cloud inference**: out of scope by the product's local-first constraint.
