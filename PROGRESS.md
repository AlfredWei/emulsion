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

## M0 status: functionally done on macOS; Windows deferred (user decision, 2026-07-25)

1. **In-webview WebGPU spike — DONE, confirmed on macOS.** `app/src/routes/m0-spike/+page.svelte` ran inside the real Tauri-launched WKWebView (macOS 26.5.2): `navigator.gpu` present, adapter/device acquired, a real WGSL "exposure +1EV" shader rendered to an offscreen texture and read back — output matched the expected math exactly (153 = round(0.6×255)). Confirms the "decode once, edit reactively via in-webview WebGPU" architecture (ADR-0004) is viable on macOS. Kept the spike page in the repo as a re-runnable check; `tauri.conf.json`'s temporary window-`url` override used to load it was reverted after the run.
2. **Real RAW decode — DONE, confirmed on macOS, with a real gap found.** Downloaded a CC0-licensed Canon EOS 5D Mark III DNG sample from [raw.pixls.us](https://raw.pixls.us/) (not committed to the repo — large, third-party provenance; kept in the local scratchpad only). Lossless-compressed DNG decodes correctly end-to-end (3960×2640, buffer size exactly right). **Lossy-compressed DNG fails** — `rsraw`'s vendored LibRaw build doesn't link `libjpeg`, so it can't decode the common lossy-compressed-DNG / JPEG-in-RAW case. Real-file test added as `EMULSION_TEST_RAW_SAMPLE`-gated (`app/src-tauri/src/raw_decode.rs`), not a committed fixture. Full detail in ADR-0003's dated finding.
3. **Windows validation — explicitly deferred**, by user decision (not a gap to chase right now). This environment is macOS-only. Two concrete, real Windows unknowns are now on record for whenever Windows work resumes (not hypothetical — both were discovered by actually building things, see risks below): `rsraw` doesn't build under MSVC, and WebView2's WebGPU support is unverified. No CI was set up for this — deferred entirely per user's explicit choice, revisit before any Windows release is planned.

RFC-0001 §8's three open questions have been updated with these results — two confirmed-on-macOS/open-on-Windows, one (working color space) still fully open.

M0's own exit criteria (MILESTONES.md) are effectively met on macOS: RAW decode works, a hardcoded adjustment renders color-correctly via the real render pipeline, catalog schema v0 round-trips. The "on both macOS and Windows" half of that criterion is the one open item, and it's deferred by choice, not by accident.

## Known constraints / open risks

- **This dev environment is macOS-only** — cannot run or verify anything on Windows directly. Windows work is deferred (see above), not blocked-and-forgotten — worth remembering to come back to before a Windows release is ever planned.
- **`rsraw` does not build on Windows/MSVC as published** (`rsraw-sys`'s build script panics on `cl.exe`-like compilers).
- **`rsraw`'s vendored LibRaw lacks libjpeg** — cannot decode lossy-compressed DNG or other libjpeg-dependent RAW variants, even on macOS. This is a live gap on the platform we *can* test, not just a Windows concern — worth fixing (patch the build script to link libjpeg, or re-evaluate `libraw-rs`) before M1 claims "broad RAW format support."
- **WebView2's WebGPU support is unverified** — the same spike that passed on WKWebView needs to run on Windows before ADR-0004 is fully confirmed cross-platform.

## Working practices (see also memory)

- Git flow: `main` is the stable branch; non-trivial units of work happen on `feature/*` branches, get logically-grouped commits, then merge to `main`. No force-push, no skipped hooks.
- This file should be updated at the end of each work session / whenever a task-list milestone completes, not just at the very end of M0.
