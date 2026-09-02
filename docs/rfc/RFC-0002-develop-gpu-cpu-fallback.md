# RFC-0002: Seamless CPU fallback for Develop's interactive canvas

- Status: Draft — for review in PR (flips to Accepted once merged)
- Date: 2026-09-02
- Companion documents: [ADR-0004](../adr/ADR-0004-rendering-and-color-management.md), [MILESTONES](../../PRD/MILESTONES.md#m5--performance-gpu-merges-faces), [PROGRESS.md](../../PROGRESS.md)

## 1. Problem

M5's scope line is *"Full GPU-accelerated Develop rendering (building on the M0 spike) with correct, seamless CPU fallback."* Today there is no fallback: `DevelopCanvas.svelte`'s `initGpu()` throws whenever `navigator.gpu` is absent or adapter/device acquisition fails, and the `$effect` catch handler just sets `status = "error"` — Develop becomes entirely unusable.

This is a real, not hypothetical, gap: ADR-0004 explicitly flags that in-webview WebGPU support has **only ever been spike-tested on WKWebView (macOS)**, never on WebView2 (Windows) — "do not treat this ADR as fully confirmed cross-platform until the same spike runs on Windows." Until that validation happens, this app may be silently unusable today on some real Windows machines, or on any machine/VM/GPU-passthrough configuration where a `GPUAdapter` can't be acquired.

## 2. Non-goals

- **Native `wgpu`/headless CPU-GPU unification** (ADR-0004's "Update — develop-engine extraction" section, deferred option A: making the CPU export path and the interactive WGSL path share one executable implementation). This RFC is about *availability* when GPU is absent, not about *unifying* the two engines when both exist — that stays deferred to a later M5 decision made "with full context," not pre-empted here.
- **Real Windows/WebView2 hardware validation.** This RFC makes the app *survive* a WebGPU-unavailable environment; it does not itself validate whether WebView2 actually lacks WebGPU on any given Windows machine. No Windows hardware is available in this dev environment.
- **Feature parity with GPU mode.** See §5.

## 3. Design

### 3.1 Detection: only a real device-acquisition failure triggers fallback

`initGpu()` (`DevelopCanvas.svelte`) has three failure classes that were previously collapsed into one `status = "error"`:

1. `navigator.gpu` absent, `requestAdapter()` returns null, or `requestDevice()` rejects — genuine environment incapability. **New: routes to `status = "cpu-fallback"`.**
2. `uncapturederror` / a WGSL `getCompilationInfo()` compile failure — these only fire *after* a device was successfully acquired, meaning the environment supports WebGPU fine and this is a real shader/app bug. **Unchanged: stays `status = "error"`**, never masked as an environment gap (masking it here would hide real regressions in CI/dev).
3. `loadImage(path)` failing (decode error, missing file) — unrelated to GPU. **Unchanged: stays `status = "error"`.**

A small pure helper, `classifyGpuFailure(error)` (new `gpuFallback.js`), maps class 1's three throw sites to a `{reason, message}` pair for the on-canvas banner — kept separate from the async device/adapter orchestration so it's independently unit-testable.

### 3.2 Rendering: reuse the existing CPU preview pipeline, add no new Rust command

`preview_edit_stack` (`src-tauri/src/lib.rs`, backed by `preview_cache::ensure_graded_preview_for_hash`) already runs the full CPU pipeline (`develop_engine.rs`: lens correction → perspective → the complete edit stack including all 8 mask types → crop) at draft-tier resolution, cached by content-hash+stack-hash. This is the exact command M4.5 Slice 5 built for History/Snapshot/Preset hover-preview — this slice reuses it as the *primary* canvas content in fallback mode instead of only a rail-hover thumbnail.

Because `DevelopCanvas.svelte` only receives decomposed per-field props (not the full `EditStack` object), and `+page.svelte` already owns both `editStack` and an established "compute a static preview CPU-side, hand the canvas a URL" pattern (`softProofPreviewUrl`, driven by a debounced `$effect` watching `editStack`), the fallback preview follows that same shape rather than inventing a new one: `+page.svelte` gets a new `cpuFallbackPreviewUrl` state and a debounced `$effect` (250ms, matching `flushEditStack`'s own "settled after a drag" window — re-rendering CPU-side on every slider tick is exactly the cost GPU was chosen to avoid), calling `previewEditStack(path, contentHash, editStack)` whenever `gpuFallbackActive` is true. `DevelopCanvas` reports `gpuFallbackActive` upward via a new `onGpuFallback(active: boolean)` callback prop, fired the moment `initGpu` throws (and symmetrically `false` whenever GPU acquisition succeeds, in case a later image's acquisition recovers).

`DevelopCanvas` separately fetches just the source image's true (unedited, uncropped) pixel dimensions via `getDevelopPreview` — needed for `onSourceDimensions`, which crop-aspect-ratio math elsewhere depends on — without needing the pixel bytes themselves.

### 3.3 Template: a distinct `status` value does most of the gating for free

Introducing `"cpu-fallback"` as its own `status` value (rather than reusing `"ready"` with a side flag) means every existing `{#if status === "ready"}`-gated block — the zoom badge, the mask-overlay div (and therefore every mask handle/pin), the committed-crop CSS preview, the histogram/eyedropper/before-after machinery — automatically does *not* render in fallback mode, with no additional gating code needed in `DevelopCanvas.svelte` itself. The real `<canvas>` (never configured with a WebGPU context in this mode) is hidden via a CSS class; a new `{#if status === "cpu-fallback"}` block renders `cpuFallbackPreviewUrl` as an `<img>` (falling back to a "Rendering preview…" placeholder before the first debounced render resolves, avoiding a blank-canvas flash) plus a persistent on-canvas banner.

### 3.4 Mask/Crop tools: disabled, not broken

`MaskToolStrip.svelte` gets a new `gpuUnavailable` prop, OR'd into every tool button's existing `disabled` condition (the same per-button pattern already used for the "at mask cap" gate) — covering Crop and all 7 mask tools. Masks/crop already baked into a stack from a prior GPU session still render correctly in fallback (the CPU engine applies them faithfully); only *creating/adjusting* new geometry is blocked, since that interactive geometry (brush rasterization, crop-handle drag math) is built against the WebGPU canvas's backing store and CSS-transform tricks that don't apply to a static `<img>`.

## 4. Testability

- **Vitest**: `classifyGpuFailure` — one case per `initGpu` throw site plus a non-Error input, asserting the correct `reason` and a non-empty `message`.
- **e2e** (`app/e2e/specs/develop-gpu-fallback.e2e.js`): forces `navigator.gpu` unavailable via `Object.defineProperty(navigator, "gpu", { value: undefined, configurable: true })` (confirmed empirically to work against the real WKWebView build — no non-configurable-property obstacle here, unlike `window.__TAURI_INTERNALS__`), opens Develop, and asserts: the fallback banner appears, the real `<canvas>` is hidden, a rendered `<img>` with a real `src` appears, every mask/crop tool button is disabled, and an Exposure edit produces a new (different) rendered image after the debounce window. Run against a real rebuilt e2e-featured `.app` — 3/3 passing in isolation, and `golden-path.e2e.js` re-run afterward (also 3/3) to confirm the normal GPU path is completely unaffected.

## 5. v1 scope: explicit, graceful degradation

Disabled in fallback mode, each for a stated reason (not silently broken): mask creation/editing, Crop & Straighten, 100%-zoom tier, live histogram, eyedropper/hover-pixel readout, before/after toggle — all either need WebGPU-canvas-specific interactive geometry or GPU-texture readback with no CPU equivalent built here. A photographer can still see their image, see the current edit stack's graded result, and keep editing via the Develop panel's sliders (debounced, not live-while-dragging) — Develop stays usable, just not at full GPU-mode fidelity.
