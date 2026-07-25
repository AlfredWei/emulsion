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

1. **In-webview WebGPU spike — DONE, confirmed on macOS.** `app/src/routes/m0-spike/+page.svelte` ran inside the real Tauri-launched WKWebView (macOS 26.5.2): `navigator.gpu` present, adapter/device acquired, a real WGSL "exposure +1EV" shader rendered to an offscreen texture and read back — output matched the expected math exactly (153 = round(0.6×255)). Confirms the "decode once, edit reactively via in-webview WebGPU" architecture (ADR-0004) is viable on macOS. Full result and detail in ADR-0004's dated finding section. Kept the spike page in the repo as a re-runnable check (harmless, not part of the real app UI); `tauri.conf.json`'s temporary window-`url` override used to load it was reverted after the run.
2. **Blocked on a sample RAW file**: real end-to-end decode → IPC → WebGPU display for an actual photo hasn't been done — `raw_decode.rs` is only validated against its error paths (nonexistent file, non-RAW file) so far. Need either a sample RAW file from the user or explicit permission to fetch a public-domain one.
3. **Blocked on Windows access**: this environment is macOS-only, and M0 now has two concrete (not hypothetical) Windows unknowns — see risks below. Both need a real Windows machine/CI runner to resolve, which isn't available here.

RFC-0001 §8's three open questions have been updated with these results.

## Known constraints / open risks

- **This dev environment is macOS-only** — cannot run or verify anything on Windows directly.
- **`rsraw` does not build on Windows/MSVC as published** (`rsraw-sys`'s build script panics on `cl.exe`-like compilers) — a concrete blocker for the Windows build, not a hypothetical one. Candidates to resolve: `x86_64-pc-windows-gnu` target, `libraw-rs` instead, or patching `rsraw-sys`'s build script. Needs a Windows environment to actually work on.
- **WebView2's WebGPU support is unverified** — the same spike that passed on WKWebView needs to run on Windows before ADR-0004 is fully confirmed cross-platform.
- **No sample RAW file available yet** for real end-to-end decode testing.

## Working practices (see also memory)

- Git flow: `main` is the stable branch; non-trivial units of work happen on `feature/*` branches, get logically-grouped commits, then merge to `main`. No force-push, no skipped hooks.
- This file should be updated at the end of each work session / whenever a task-list milestone completes, not just at the very end of M0.
