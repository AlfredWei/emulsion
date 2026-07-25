# RFC-0001: System architecture & technology stack

- Status: Draft — for review before M0 implementation begins
- Date: 2026-07-25
- Companion documents: [PRD](../../PRD/PRD.md), [MILESTONES](../../PRD/MILESTONES.md), [lightroom-reference](../../PRD/lightroom-reference.md), [UX-DESIGN](../ux/UX-DESIGN.md)

## 1. Purpose

This RFC describes the technical architecture for the whole application, tying together the individual decisions recorded in `docs/adr/`. It exists so the shape of the system is reviewable as one coherent design before any M0 implementation code is written, per [MILESTONES.md](../../PRD/MILESTONES.md#m0--foundations--tech-spike).

## 2. Goals restated from the PRD

- Non-destructive end-to-end; originals on disk are never modified ([PRD §2](../../PRD/PRD.md)).
- Local-first, permanently — no server component anywhere in this architecture ([PRD §3](../../PRD/PRD.md)).
- Develop-panel adjustment feedback ≤100ms on 5-year-old mid-range hardware; a 50k-image catalog stays responsive ([PRD §9](../../PRD/PRD.md)).
- Color-managed throughout: working space → display profile → output profile, with trustworthy soft proofing ([PRD §9](../../PRD/PRD.md)).
- Cross-platform desktop (macOS + Windows), single codebase ([PRD §5](../../PRD/PRD.md)).

## 3. High-level component diagram

```
┌───────────────────────────────────────────────────────────────────┐
│                         Tauri application                          │
│                                                                     │
│  ┌───────────────────────────┐        ┌────────────────────────┐  │
│  │        Rust core            │  IPC   │   OS-native webview     │  │
│  │  (native process)          │◄──────►│   (frontend, Svelte)    │  │
│  │                             │        │                         │  │
│  │  • Catalog engine (SQLite)  │        │  • Library UI           │  │
│  │  • Import / RAW decode      │        │  • Develop UI           │  │
│  │    (LibRaw via FFI)         │        │  • In-webview WebGPU    │  │
│  │  • Color transforms (rcms)  │  once   │    render engine       │  │
│  │  • Export / print renderer  │  per   │    (interactive edits) │  │
│  │  • File I/O, backups        │  image │                         │  │
│  └───────────────────────────┘        └────────────────────────┘  │
└───────────────────────────────────────────────────────────────────┘
                 │
                 ▼
        Local disk: originals (untouched),
        catalog.sqlite, previews, XMP exports
```

Decision references: shell = [ADR-0001](../adr/ADR-0001-application-shell.md); frontend = [ADR-0002](../adr/ADR-0002-frontend-ui-stack.md); RAW decode = [ADR-0003](../adr/ADR-0003-raw-decoding.md); render split = [ADR-0004](../adr/ADR-0004-rendering-and-color-management.md); catalog = [ADR-0005](../adr/ADR-0005-catalog-storage.md); edit model = [ADR-0006](../adr/ADR-0006-edit-representation.md).

## 4. Core data flow: the Develop loop ("decode once, edit reactively")

This is the architecture's central idea and its biggest de-risking target for M0 ([ADR-0004](../adr/ADR-0004-rendering-and-color-management.md)):

1. User opens an image in Develop. **Once**, the Rust core: decodes the RAW file (LibRaw), demosaics, downsamples to a working preview resolution, applies the input color profile → linear working space transform (`rcms`), and sends the resulting linear pixel buffer to the frontend over IPC.
2. The frontend loads that buffer into a WebGPU texture, resident in the webview process.
3. Every further user action (slider drag, mask paint, curve edit) appends/updates an operation in the in-memory edit stack ([ADR-0006](../adr/ADR-0006-edit-representation.md)) and re-runs the WGSL shader pipeline against the already-resident texture — **no IPC round trip, no re-decode**. Pointer/slider events are coalesced into the shader's frame loop rather than dispatched one GPU pass per raw input event.
4. On save (automatic, non-destructive), the current edit stack is written back to the catalog DB via IPC — this is small JSON, not pixels, so it's cheap regardless of IPC cost.
5. On export or print, the Rust core re-renders the full edit stack at full resolution natively (not through the webview), since that path has no interactive-latency requirement.

This split exists specifically because streaming Rust-rendered frames into the webview over IPC was measured too slow (~300ms/frame) for interactive use — see [ADR-0004](../adr/ADR-0004-rendering-and-color-management.md) for the full rationale and the native-surface-overlay fallback if in-webview WebGPU proves insufficient.

## 5. Core data flow: import

1. User points Import at a folder/volume.
2. Rust core walks the source, decodes embedded previews (fast — no full demosaic needed for a thumbnail) for immediate grid display, and queues full processing in the background.
3. Catalog rows are created (image reference, path, capture metadata) in SQLite; original files are copied or referenced in place per user choice, never modified.
4. Duplicate detection runs against existing catalog entries before committing new rows.

This must not block the UI — background/async processing is a hard requirement carried from [PRD §7.2 and §7.6](../../PRD/PRD.md).

## 6. Core data flow: export

Batch export runs entirely in the Rust core: for each selected image-version, replay its edit stack (§4 step 5) at full resolution, apply output color space transform (`rcms`) and output sharpening, write the target format, all in a background task queue so Library/Develop remain usable during a large batch export ([PRD §7.5](../../PRD/PRD.md)).

## 7. Cross-cutting concerns

- **Performance budget**: ≤100ms Develop feedback is achieved by keeping the entire interactive loop in-process in the webview (§4); the only cross-process trips are the once-per-image decode and cheap JSON edit-stack saves.
- **Memory budget**: Tauri's baseline (~45MB idle, no bundled browser engine) leaves headroom for image buffers; the render pipeline should bound concurrent full-res textures and evict off-screen ones (detailed in [UX-DESIGN.md](../ux/UX-DESIGN.md) as a user-visible quality/performance mode).
- **Color management**: `rcms` (Rust) handles all ICC input/output profile transforms; the WebGPU shader pipeline works entirely in a fixed linear working space so it never needs its own ICC engine — the two stages just need to agree on the working space definition, validated in M0 ([ADR-0004](../adr/ADR-0004-rendering-and-color-management.md)).
- **Crash safety**: the edit stack is small, cheap-to-write JSON ([ADR-0006](../adr/ADR-0006-edit-representation.md)) in a SQLite DB ([ADR-0005](../adr/ADR-0005-catalog-storage.md)) — write-ahead logging and periodic autosave of the in-progress stack (not just on module switch) should be an M1 requirement so a crash mid-edit loses at most seconds of work, per [PRD §9](../../PRD/PRD.md).
- **No cloud dependency anywhere**: every component above runs entirely on the local machine; there is no network call in this architecture, consistent with [PRD §3 and §5](../../PRD/PRD.md).

## 8. Open questions carried into M0

These are exactly [MILESTONES.md M0](../../PRD/MILESTONES.md#m0--foundations--tech-spike)'s exit criteria, restated as architecture questions this RFC cannot answer on paper alone:

1. Does in-webview WebGPU (WKWebView on macOS, WebView2 on Windows) perform consistently enough, and is its color/precision behavior consistent enough, to be the primary interactive render path — or is the native-GPU-surface-overlay fallback needed on one or both OSes? ([ADR-0004](../adr/ADR-0004-rendering-and-color-management.md))
2. What working color space (linear Rec.2020 vs. linear ProPhoto-primaries vs. other) gives the best balance of gamut coverage and precision for the shader pipeline? ([ADR-0004](../adr/ADR-0004-rendering-and-color-management.md))
3. Does `rsraw`'s LibRaw build actually work cleanly in a Tauri-bundled app on both target OSes, including packaging/signing? ([ADR-0003](../adr/ADR-0003-raw-decoding.md))

M0's prototype (per MILESTONES.md) — open a RAW file, decode, apply a hardcoded adjustment, display a color-correct preview on both OSes — is designed to answer all three.
