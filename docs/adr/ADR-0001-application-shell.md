# ADR-0001: Application shell — Tauri (Rust core + OS-native webview)

- Status: Accepted
- Date: 2026-07-25
- Relates to: [PRD §5](../../PRD/PRD.md#5-assumptions--constraints), [MILESTONES M0](../../PRD/MILESTONES.md#m0--foundations--tech-spike), [RFC-0001](../rfc/RFC-0001-architecture-and-tech-stack.md)

## Context

The PRD commits to a cross-platform desktop app (macOS + Windows, single codebase) with explicit performance and memory requirements (§9: Develop slider feedback ≤100ms on mid-range 5-year-old hardware; a 50k-image catalog must stay responsive). The UI also needs to be "modern and easy to use" — meaning fast iteration on visual design and interaction polish matters, not just raw framework benchmarks.

Candidates considered:
1. **Electron** — Chromium + Node.js bundled per app.
2. **Tauri** — Rust core process + the operating system's own webview (WKWebView on macOS, WebView2 on Windows) for UI rendering.
3. **Qt/C++** (or Qt/QML) — mature native cross-platform toolkit, C++ core.
4. **Native-per-OS** — Swift/SwiftUI on macOS, C++/C# on Windows, two codebases.
5. **Rust-native GUI** (Slint, Iced, egui) — no webview at all, GPU-rendered native widget trees.

## Decision

Use **Tauri**: a Rust core process handling the catalog engine, RAW decode, file I/O, and export rendering, with the UI rendered in the OS's native webview and built with standard web technology (see [ADR-0002](ADR-0002-frontend-ui-stack.md)).

## Rationale

- **Memory/performance**: current benchmarking shows Tauri idle memory around 45MB vs. Electron's ~180MB, roughly 4x faster cold start, and an order-of-magnitude smaller installer — because Tauri ships no bundled browser engine. This is decisive against a PRD that explicitly calls out memory and performance as first-class requirements.
- **Single codebase, both target OSes**: satisfies the cross-platform-desktop decision without maintaining two native UI codebases (ruling out option 4).
- **Modern UI without reinventing a component ecosystem**: a web-tech frontend has by far the most mature tooling for building a genuinely modern, polished UI quickly — Qt/QML and Rust-native GUI frameworks (Slint/Iced/egui) are functional but have much smaller design/component ecosystems, which works against the "modern and easy to use" requirement on a small team's timeline.
- **Rust core aligns with the RAW/GPU work anyway**: ADR-0003 (RAW decoding) and the catalog engine are naturally Rust-shaped work regardless of shell choice; Tauri lets that Rust code be the actual application core rather than a side process Electron/Node would have to shell out to.

## Consequences

- Introduces a real architectural risk: getting GPU-accelerated, low-latency image rendering to work well *inside a webview* is not solved by picking Tauri — it requires the specific approach documented in [ADR-0004](ADR-0004-rendering-and-color-management.md) (in-webview WebGPU, not IPC-streamed native textures). This is the single biggest open risk in the whole architecture and is explicitly scoped as an M0 spike deliverable.
- The team needs working Rust proficiency (already implied by the RAW-decode and catalog-engine work either way).
- Packaging/notarization/code-signing on both OSes is handled by Tauri's bundler — revisited in detail at M8, not decided further here.

## Alternatives considered and rejected

- **Electron**: rejected primarily on memory/performance grounds, which directly contradicts a stated PRD requirement; existing large Electron apps are cited running 1GB+ RAM at peak, unacceptable for an image-heavy tool expected to hold many large buffers already.
- **Qt/C++**: a reasonable, battle-tested alternative (this is close to what darktable and RawTherapee use) but was set aside because it makes the "modern, easy to use" UI goal materially harder to hit — QML's design tooling and ecosystem for polished modern UI is much thinner than the web ecosystem, and C++ has a slower iteration loop and a larger memory-safety burden than Rust for a solo/small team.
- **Native-per-OS**: rejected outright — doubles the UI implementation and maintenance surface, directly against the "single codebase" constraint.
- **Rust-native GUI (Slint/Iced/egui)**: attractive on paper (no webview at all, avoids ADR-0004's risk entirely) but immature component/design ecosystems as of 2026 make hitting a "modern and easy to use" bar slower for a small team; kept as the fallback option if ADR-0004's in-webview WebGPU spike fails outright in M0.
