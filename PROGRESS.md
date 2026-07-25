# Progress Log

Running log of where this project stands. Update this whenever a milestone step lands or the plan changes — this is the first thing to read after a session restart or a day away, before re-deriving context from scratch.

## Current phase: M0 — Foundations & tech spike

See [PRD/MILESTONES.md](PRD/MILESTONES.md#m0--foundations--tech-spike) for M0's scope and exit criteria.

## Done

- **Product docs** (`PRD/`): PRD, 8-milestone roadmap (M0–M7), Lightroom v1→now feature research used to sequence the roadmap. Merged to `main`.
- **Architecture docs** (`docs/`): RFC-0001 (system architecture) + 6 ADRs (app shell = Tauri, frontend = Svelte, RAW decode = LibRaw via FFI, rendering = "decode once in Rust, edit reactively via in-webview WebGPU" — flagged as the highest-risk decision, catalog = SQLite, edit model = versioned JSON edit stack). Merged to `main`.
- **UX design** (`docs/ux/UX-DESIGN.md` + `docs/ux/mockups/library-develop-mockup.html`): design principles, module layouts for Library/Develop, reviewed static mockup (published as an Artifact). Merged to `main`.
- **GitHub repo**: private repo created at [github.com/AlfredWei/emulsion](https://github.com/AlfredWei/emulsion), `main` pushed and tracked.
- **Dev environment**: Rust toolchain updated 1.64.0 → 1.97.1 (Tauri v2 requires 1.77.2+). Confirmed available: cargo, node v23.5.0, npm 11.6.3, Xcode CLI tools, `gh` CLI (authenticated).
- **App scaffold** (`app/`): `create-tauri-app` Svelte(Kit)+TS template, renamed throughout to "Emulsion" (package name, Cargo crate `emulsion_lib`, Tauri `productName`/window title/identifier `dev.alfredwei.emulsion`). `npm install` completed, Rust side builds clean as scaffolded.
- **Rust core dependencies added and confirmed compiling/linking on macOS**: `rusqlite` (bundled SQLite, ADR-0005), `rsraw` (vendored LibRaw via FFI, ADR-0003), `lcms2` (static feature — see correction below, ADR-0004), `thiserror`.
- **Catalog schema v0** (`app/src-tauri/src/catalog.rs`): `images` + `image_versions` tables, versioned JSON edit-stack column per ADR-0006. Round-trip test passing (`cargo test --lib`): insert an image + one edit-stack record, read it back, plus a duplicate-path-rejection test.
- **RAW decode module** (`app/src-tauri/src/raw_decode.rs`): wraps `rsraw`'s real API (`RawImage::open(&[u8])` → `.unpack()` → `.process::<BIT_DEPTH_8>()`). Error-handling paths tested and passing (nonexistent file, non-RAW file both fail cleanly, no panics). **Real-file decode still untested — no sample RAW file in this environment yet.**

### Corrections made during the spike (this is exactly what M0 is for)

- **`rcms` → `lcms2`**: the original ADR-0004 research claimed `rcms` was "memory-safe... verified bit-identical to lcms2." That was wrong — `rcms` v0.1.0's own README says *"Currently sparsely implemented and prone to crashing from a `todo!()`."* Swapped to `lcms2` (real bindings to the mature Little CMS C library, `static` feature), which builds cleanly. ADR-0004 updated with a dated finding section; RFC-0001 updated throughout.
- **`rsraw` + MSVC**: `rsraw-sys`'s build script contains `panic!("MSVC is not supported")`. Windows Tauri builds default to the MSVC target, so **`rsraw` as published will not build out of the box on standard Windows** — needs the GNU target, `libraw-rs` instead, or a build-script patch. ADR-0003 updated with this finding; unresolved, needs Windows-side follow-up (see risks below).
- **`rsraw::Error` is not publicly reachable** (kept in a private module, not re-exported) — minor API-maturity gap, worked around in `raw_decode.rs` by converting to `String` via `Display` at each call site instead of using `thiserror`'s `#[from]`.

## In progress / next up

Working through M0's remaining exit criteria in order:

1. **In-webview WebGPU spike** (the highest-risk item per ADR-0004 / RFC-0001 §8) — currently in progress: a WGSL shader applying a hardcoded exposure/WB-style adjustment to a texture, rendered inside Tauri's webview, checked for color correctness on macOS (WKWebView).
2. Wire decode → IPC → WebGPU display end-to-end for one image (needs a sample RAW file — see below).
3. Write up findings against RFC-0001 §8's three open questions and update that section with results.

## Known constraints / open risks

- **This dev environment is macOS-only.** M0's exit criteria call for validating behavior on both macOS and Windows (WKWebView vs. WebView2) — the WebGPU-in-webview consistency question (ADR-0004) can only be partially answered here. Windows validation needs to happen separately before that ADR is treated as fully confirmed rather than "confirmed on macOS, unverified on Windows."
- **`rsraw` doesn't build on Windows/MSVC as published** (see finding above) — this is now a concrete blocker for the "cross-platform, single codebase" requirement, not just a hypothetical risk. Needs resolution before M1 can rely on it for the Windows build. Candidates: `x86_64-pc-windows-gnu` target, `libraw-rs`, or patching `rsraw-sys`'s build script.
- **No sample RAW file available yet** for real end-to-end decode testing.

## Working practices (see also memory)

- Git flow: `main` is the stable branch; non-trivial units of work happen on `feature/*` branches, get logically-grouped commits, then merge to `main`. No force-push, no skipped hooks.
- This file should be updated at the end of each work session / whenever a task-list milestone completes, not just at the very end of M0.
