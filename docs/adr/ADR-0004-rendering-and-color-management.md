# ADR-0004: Interactive rendering & color-management architecture

- Status: Accepted, confirmed viable on macOS by M0 spike (2026-07-25) — Windows (WebView2) still unverified, see the M0 spike finding below before treating this as fully confirmed
- Date: 2026-07-25
- Relates to: [ADR-0001](ADR-0001-application-shell.md), [ADR-0003](ADR-0003-raw-decoding.md), [PRD §7.4, §7.6, §9](../../PRD/PRD.md), [MILESTONES M0](../../PRD/MILESTONES.md#m0--foundations--tech-spike)

## Context

The PRD requires Develop-panel slider feedback ≤100ms (§9) and GPU-accelerated rendering with a correct CPU fallback (§7.6). [ADR-0001](ADR-0001-application-shell.md) put the UI inside a webview, which creates a genuine tension: the Rust core (where `wgpu`/native GPU access naturally lives) and the canvas the user is looking at are in different processes.

Investigation into how others solve this: streaming a Rust-rendered `wgpu` texture into the webview over Tauri's IPC has been measured at roughly 300ms per frame on modest hardware — three times over budget, and that's before accounting for real edit-stack complexity (multiple masks, curves, etc.). That path is not viable for interactive slider-drag latency.

The viable alternative: modern system webviews (WKWebView on macOS, WebView2 on Windows) increasingly expose the browser's own **WebGPU API** directly to page JavaScript. That means the interactive compute pipeline can run **inside the webview process itself**, using the same kind of WGSL compute/render shaders `wgpu` would use natively — with zero cross-process texture streaming for anything that needs to feel instant.

## Decision

Adopt a **"decode once, edit reactively" pipeline**, split across the process boundary along a line chosen specifically to keep the interactive loop in-process:

1. **Rust core** (one-time, per image, whenever the user opens/switches to it in Develop): decode the RAW file via LibRaw ([ADR-0003](ADR-0003-raw-decoding.md)), demosaic, downsample to a working preview resolution, and hand the frontend a linear-light pixel buffer plus the image's camera/ICC input profile. `lcms2` (real Rust bindings to the mature Little CMS C library — see M0 finding below) performs the input-profile → linear-working-space transform on the Rust side as part of this one-time step.
2. **Frontend (in-webview WebGPU)**: every subsequent edit-stack change (exposure, tone curve, HSL, masks, etc.) is expressed as WGSL compute/render shaders that re-run against the already-resident linear buffer, directly on the GPU, in the same process as the `<canvas>`. No IPC round trip per slider tick. Slider/pointer events are coalesced/debounced into the shader's frame loop rather than dispatched one compute pass per raw input event.
3. **Rust core (export/print path only)**: full-resolution, final-quality rendering for export or print runs natively in Rust (via `wgpu` or CPU, whichever is faster/more consistent for a one-shot non-interactive render) — this path has no <100ms latency requirement, so the cross-process cost that ruled out option 3 below for *interactive* editing is irrelevant here.

## Consequences

- **Requires validating in-webview WebGPU support and behavior consistency** across WKWebView and WebView2 as an M0 spike deliverable (already an M0 exit criterion). If either webview's WebGPU support proves inconsistent, insufficiently performant, or absent on a meaningful fraction of target hardware, the documented fallback is a **native GPU surface composited alongside the webview** (Tauri v2 supports multiple surfaces — a native-rendered region plus a webview region in the same window), at the cost of more native windowing/compositing code.
- The color pipeline now has two GPU-adjacent stages that must agree bit-for-bit-close: Rust-side `lcms2` transforms (input profile → linear) and WebGPU shader math (linear working space → display). Both stages need the same working color space definition (recommend linear Rec.2020 or linear ProPhoto-primaries as the internal working space — final choice deferred to the M0 spike's color-accuracy validation, not fixed here) and consistent handling of transfer functions (no implicit sRGB gamma applied by the browser's canvas compositor — must render into a color-managed/`predisplay: p3`-or-equivalent canvas mode, or manage the transform manually, to avoid the browser silently double-applying color management).
- Masking (M2+) and AI masking (M5+) both need their mask-generation step (CPU/Rust for AI models, GPU for manual brush/gradient) to feed into this same in-webview shader pipeline as an additional input texture — this ADR's architecture needs to accommodate that from the start even though masking isn't in M1's scope, so the shader interface should be designed as "buffer + stack of mask-and-adjustment layers," not hardcoded to a fixed global-only pipeline.

## Alternatives considered and rejected

- **Stream Rust/`wgpu`-rendered frames to the webview over IPC on every edit**: rejected — measured ~300ms/frame, 3x over the ≤100ms budget, and that number only gets worse as edit-stack complexity grows.
- **Native GPU surface overlay for all rendering (not just fallback)**: viable and avoids the WebGPU-webview-consistency risk entirely, but forces a lot more native windowing/compositing code up front for a benefit (avoiding a spike) that may not be necessary — kept as the documented fallback rather than the default, to be adopted only if M0's spike shows in-webview WebGPU is insufficient.
- **Do all rendering server-side/CPU-only, no interactive GPU acceleration**: rejected outright — directly violates PRD §7.6 and §9's explicit performance requirements.

## Update — M0 spike finding: `rcms` was the wrong call (2026-07-25)

The original research behind this ADR cited `rcms` as "a memory-safe pure-Rust reimplementation of Little CMS verified bit-identical to lcms2." That claim does not hold up: `rcms` v0.1.0's own README says plainly *"Currently sparsely implemented and prone to crashing from a `todo!()`."* It is a single-author, early-stage prototype (last substantial work years ago, pulling in a stale/deprecated transitive dependency tree — `rand` 0.6.5, `proc-macro-hack`, `cgmath`, `time` 0.2). It is not viable for a production color pipeline, and the earlier research summary describing it as verified/bit-identical was wrong.

**Corrected decision**: use **`lcms2`** (the well-established Rust binding to the real, mature Little CMS C library, maintained by kornelski) with its `static` feature, so lcms2 is compiled from source rather than depending on a system install. Confirmed on macOS: adds cleanly (`cargo add lcms2 --features static`) and builds/links with no issues in this environment (a system `little-cms2` via Homebrew was present but the `static` feature was used deliberately for build portability, not the system copy). This is the real Little CMS most professional imaging tools already rely on, which is a materially stronger foundation for this ADR's color pipeline than the original `rcms` pick.

This is exactly the kind of thing an M0 spike is for — the lesson going forward is to verify a crate's actual README/source before writing an ADR around it, not just a search-engine summary of it.

## Update — M0 spike finding: in-webview WebGPU confirmed on macOS (2026-07-25)

The core risk this ADR exists to de-risk — does `navigator.gpu` work inside Tauri's actual OS-native webview, not just a generic browser — was tested directly, not assumed. `app/src/routes/m0-spike/+page.svelte` runs inside the real Tauri-launched WKWebView (macOS 26.5.2) and does a full, real WebGPU round trip:

1. Confirms `navigator.gpu` exists.
2. Acquires a `GPUAdapter` and `GPUDevice`.
3. Builds a render pipeline from a WGSL fragment shader that applies a hardcoded "exposure +1EV" adjustment (`0.3 * 2^1 = 0.6`) to a stand-in pixel value — the same shape of operation a real Develop slider would perform.
4. Renders to an offscreen texture, copies it to a readback buffer, and numerically checks the output.

Result, reported via a throwaway Tauri command back to the Rust process's stdout (there's no tool available in this environment to screenshot a native macOS app window, so the page self-reports instead of relying on visual inspection):

```
M0_SPIKE_RESULT: {"hasNavigatorGpu":true,"adapter":"{}","deviceAcquired":true,
"renderSubmitted":true,"readback":{"r":153,"g":153,"b":153,"a":255},
"expected":153,"colorCorrect":true,"error":null}
```

`153 = round(0.6 * 255)` — the shader's math round-tripped through the GPU and back exactly as expected. **This confirms the "decode once, edit reactively via in-webview WebGPU" architecture is viable on macOS**, not just plausible on paper. The native-GPU-surface-overlay fallback is no longer needed on macOS and can be deprioritized unless a later, more complex shader (real masks, multiple layers) reveals a problem this simple spike didn't exercise.

**Still open**: this only tests WKWebView. WebView2 (Windows) has not been tested — this environment is macOS-only (see PROGRESS.md). Do not treat this ADR as fully confirmed cross-platform until the same spike (or equivalent) runs on Windows.

## Update — develop-engine extraction: CPU/GPU unification deliberately deferred to M5 (2026-07-30)

M3's "extract the develop engine" scope item (PRD/MILESTONES.md) surfaced a real decision this ADR's step 3 had left open: the export renderer was specified as "via `wgpu` or CPU, whichever is faster/more consistent," and the M1 Slice 5 implementation chose CPU (`export.rs`'s `apply_edit_stack`, since moved to `develop_engine.rs`). That CPU implementation and `DevelopCanvas.svelte`'s WGSL fragment shader apply the identical exposure → contrast → saturation formula, hand-synced rather than sharing one executable implementation.

Two ways to read "consolidate into a single module boundary" were considered: (A) introduce native `wgpu` and headless/offscreen-render the export path reusing the *exact same* WGSL shader source as the interactive preview — true single-execution unification; (B) consolidate what's actually consolidatable today — move the CPU-side canonical logic out of `export.rs` (a module about file export, not op interpretation) into its own `develop_engine.rs`, with a broadened parity-test table and explicit doc-comment cross-references to the WGSL shader as the two sides that must be hand-kept in sync.

**Decision: (B), with (A) explicitly deferred to M5**, not silently dropped. Reasoning, confirmed by a design review before implementation:

- MILESTONES.md's own wording already frames this item as setting up M5's *later* "GPU pipeline... with correct, seamless CPU fallback" work "rather than competing with it" — M5 already owns true GPU-path unification as dedicated, budgeted scope. Doing (A) now would relitigate this ADR's own step-3 decision as a side effect of a basic-infra cleanup item.
- (A) would introduce this codebase's first async-by-design native dependency (`wgpu`'s device/adapter/buffer-readback API requires async even for pure headless/offscreen rendering, no window needed) purely to save hand-syncing three simple formulas — a poor risk/reward trade for a project that has already been burned once on unverified cross-platform GPU/build assumptions (ADR-0003's Windows vcpkg saga; this ADR's own still-unverified-on-Windows WebGPU caveat, directly above). Headless GPU context creation is a real, currently-untested failure class, particularly on machines/CI without a real GPU.
- (B) genuinely satisfies "single module boundary" for what's actually shared: one canonical CPU implementation (previously duplicated in spirit, since two independent call sites — `export.rs` and `import.rs`'s thumbnail regeneration — both reused a function living inside an unrelated module), one place it's tested, and a named, documented parity obligation with the WGSL shader rather than an implicit, undocumented one.

**Consequence carried forward**: when M5 does its GPU pipeline rewrite, revisit whether `develop_engine.rs`'s CPU implementation should be replaced by (A) at that point, or kept as the documented "seamless CPU fallback" M5's own scope already calls for — that's M5's decision to make with full context, not pre-empted here.

## Update — M5 Slice 1: seamless CPU fallback shipped (2026-09-02)

The "seamless CPU fallback" this ADR's own scope has named twice above (this file's step 3, and the develop-engine-extraction update just above) is now built — see [RFC-0002](../rfc/RFC-0002-develop-gpu-cpu-fallback.md) for the full design and [PROGRESS.md](../../PROGRESS.md)'s M5 Slice 1 entry for verification detail. Summary of the decision actually made, since it differs in one respect from what step 3 above originally sketched ("a native GPU surface composited alongside the webview"):

- **The fallback is `develop_engine.rs`'s existing CPU pipeline, rendered as a static, debounced preview image — not a native-GPU-surface overlay.** The overlay option named in this ADR's original "Consequences" section is real native windowing/compositing work, undertaken only if in-webview WebGPU proves *categorically* insufficient. What M5 Slice 1 addresses is narrower and more common in practice: a single machine/environment where `navigator.gpu` is absent or device acquisition fails, while the app overall still ships on the in-webview WebGPU architecture for everyone else. `develop_engine.rs` already existed, already implements every op the interactive shader does (see the develop-engine-extraction update above), and needed no new Rust command — `preview_edit_stack` (M4.5 Slice 5) already renders an arbitrary edit stack via this exact engine at draft resolution. A debounced (250ms) CPU re-render on edit, rather than a truly live/interactive one, is the accepted cost — matches the milestone's own "seamless" framing (Develop stays usable) without claiming full GPU-mode interactivity in a mode chosen specifically because sustained CPU-side rendering is comparatively expensive.
- **Detection is scoped to real device-acquisition failure only** (`navigator.gpu` missing, no adapter, or `requestDevice()` rejecting) — a shader-compile failure or a later `uncapturederror` (both meaning WebGPU itself works fine) still surface as `status = "error"`, exactly as before this slice, so a real app/shader regression is never silently masked as an environment gap.
- **Windows/WebView2 validation is still open** — this slice makes the app *survive* a WebGPU-unavailable environment (verified via a real, empirically-confirmed `navigator.gpu` monkeypatch against the actual WKWebView build, not a mock), but does not itself confirm or deny whether WebView2 lacks WebGPU on any real Windows machine. This ADR's existing "Still open" caveat two sections above is unchanged by this update.
- **Native `wgpu`/CPU-GPU unification remains deferred**, per the develop-engine-extraction update's own consequence-carried-forward note — untouched by this slice.
