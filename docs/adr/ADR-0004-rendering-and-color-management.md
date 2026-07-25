# ADR-0004: Interactive rendering & color-management architecture

- Status: Accepted, highest-risk decision in the roadmap — explicit M0 spike required before treating this as final
- Date: 2026-07-25
- Relates to: [ADR-0001](ADR-0001-application-shell.md), [ADR-0003](ADR-0003-raw-decoding.md), [PRD §7.4, §7.6, §9](../../PRD/PRD.md), [MILESTONES M0](../../PRD/MILESTONES.md#m0--foundations--tech-spike)

## Context

The PRD requires Develop-panel slider feedback ≤100ms (§9) and GPU-accelerated rendering with a correct CPU fallback (§7.6). [ADR-0001](ADR-0001-application-shell.md) put the UI inside a webview, which creates a genuine tension: the Rust core (where `wgpu`/native GPU access naturally lives) and the canvas the user is looking at are in different processes.

Investigation into how others solve this: streaming a Rust-rendered `wgpu` texture into the webview over Tauri's IPC has been measured at roughly 300ms per frame on modest hardware — three times over budget, and that's before accounting for real edit-stack complexity (multiple masks, curves, etc.). That path is not viable for interactive slider-drag latency.

The viable alternative: modern system webviews (WKWebView on macOS, WebView2 on Windows) increasingly expose the browser's own **WebGPU API** directly to page JavaScript. That means the interactive compute pipeline can run **inside the webview process itself**, using the same kind of WGSL compute/render shaders `wgpu` would use natively — with zero cross-process texture streaming for anything that needs to feel instant.

## Decision

Adopt a **"decode once, edit reactively" pipeline**, split across the process boundary along a line chosen specifically to keep the interactive loop in-process:

1. **Rust core** (one-time, per image, whenever the user opens/switches to it in Develop): decode the RAW file via LibRaw ([ADR-0003](ADR-0003-raw-decoding.md)), demosaic, downsample to a working preview resolution, and hand the frontend a linear-light pixel buffer plus the image's camera/ICC input profile. `rcms` performs the input-profile → linear-working-space transform on the Rust side as part of this one-time step.
2. **Frontend (in-webview WebGPU)**: every subsequent edit-stack change (exposure, tone curve, HSL, masks, etc.) is expressed as WGSL compute/render shaders that re-run against the already-resident linear buffer, directly on the GPU, in the same process as the `<canvas>`. No IPC round trip per slider tick. Slider/pointer events are coalesced/debounced into the shader's frame loop rather than dispatched one compute pass per raw input event.
3. **Rust core (export/print path only)**: full-resolution, final-quality rendering for export or print runs natively in Rust (via `wgpu` or CPU, whichever is faster/more consistent for a one-shot non-interactive render) — this path has no <100ms latency requirement, so the cross-process cost that ruled out option 3 below for *interactive* editing is irrelevant here.

## Consequences

- **Requires validating in-webview WebGPU support and behavior consistency** across WKWebView and WebView2 as an M0 spike deliverable (already an M0 exit criterion). If either webview's WebGPU support proves inconsistent, insufficiently performant, or absent on a meaningful fraction of target hardware, the documented fallback is a **native GPU surface composited alongside the webview** (Tauri v2 supports multiple surfaces — a native-rendered region plus a webview region in the same window), at the cost of more native windowing/compositing code.
- The color pipeline now has two GPU-adjacent stages that must agree bit-for-bit-close: Rust-side `rcms` transforms (input profile → linear) and WebGPU shader math (linear working space → display). Both stages need the same working color space definition (recommend linear Rec.2020 or linear ProPhoto-primaries as the internal working space — final choice deferred to the M0 spike's color-accuracy validation, not fixed here) and consistent handling of transfer functions (no implicit sRGB gamma applied by the browser's canvas compositor — must render into a color-managed/`predisplay: p3`-or-equivalent canvas mode, or manage the transform manually, to avoid the browser silently double-applying color management).
- Masking (M2+) and AI masking (M5+) both need their mask-generation step (CPU/Rust for AI models, GPU for manual brush/gradient) to feed into this same in-webview shader pipeline as an additional input texture — this ADR's architecture needs to accommodate that from the start even though masking isn't in M1's scope, so the shader interface should be designed as "buffer + stack of mask-and-adjustment layers," not hardcoded to a fixed global-only pipeline.

## Alternatives considered and rejected

- **Stream Rust/`wgpu`-rendered frames to the webview over IPC on every edit**: rejected — measured ~300ms/frame, 3x over the ≤100ms budget, and that number only gets worse as edit-stack complexity grows.
- **Native GPU surface overlay for all rendering (not just fallback)**: viable and avoids the WebGPU-webview-consistency risk entirely, but forces a lot more native windowing/compositing code up front for a benefit (avoiding a spike) that may not be necessary — kept as the documented fallback rather than the default, to be adopted only if M0's spike shows in-webview WebGPU is insufficient.
- **Do all rendering server-side/CPU-only, no interactive GPU acceleration**: rejected outright — directly violates PRD §7.6 and §9's explicit performance requirements.
