# Progress Log

Running log of where this project stands. Update this whenever a milestone step lands or the plan changes — this is the first thing to read after a session restart or a day away, before re-deriving context from scratch.

## Current phase: M1 — MVP (Import → Library → Basic Develop → Export)

M0 is done on macOS. Windows was deferred by choice on 2026-07-25, then **un-deferred the same day** — see "CI + Windows validation" below. M1 is scoped in MILESTONES.md as "3–6 months," too large for one pass — being built as 5 sequenced slices (see the M1 plan). **Slice 1 (catalog schema v1 + import backend) is done.** Slices 2–5 (Library UI, real Develop pipeline, Export, crash-safety/dogfood) are next, in that order.

See [PRD/MILESTONES.md](PRD/MILESTONES.md#m1--mvp-import--library--basic-develop--export) for M1's scope and exit criteria.

## CI + Windows validation (2026-07-25, PR #2)

Set up `.github/workflows/ci.yml`: a `macos-latest`/`windows-latest` matrix building + testing the Rust core, plus a `frontend-check` job (`npm run check`). This is real Windows validation, not local guessing — GitHub's own Windows runners stand in for the Windows machine this environment doesn't have.

- **Confirmed, empirically, not just from reading source**: `rsraw` fails to build on `windows-latest` with exactly the predicted error — `thread 'main' panicked at rsraw-sys-0.1.1\build.rs:13:9: MSVC is not supported`, after ~6.5 minutes of compiling vendored LibRaw C++. This graduates ADR-0003's finding from "predicted from reading the build script" to "confirmed on real Windows CI." The Windows job is left **failing on purpose** — it should stay red until one of the candidate fixes (GNU target, `libraw-rs`, build-script patch) actually lands, so this can't silently regress into "we forgot Windows was broken."
- **macOS job passes cleanly**, including a step that downloads the same CC0 sample DNG used for local validation and runs it through `EMULSION_TEST_RAW_SAMPLE`-gated real-decode tests — so CI now exercises a real RAW file, not just the error paths, on every PR.
- **Frontend-check job** (`npm run check`, i.e. `svelte-check`) surfaced 15 real pre-existing type errors, all now fixed: added `@webgpu/types` as a real devDependency (wired into `jsconfig.json` — needed going forward for Slice 3's real Develop pipeline, not just the throwaway spike pages), fixed an implicit-`any` handler parameter in the scaffold page, and loosened the throwaway `/m0-spike` and `/m1-smoke` diagnostic pages' object typing via JSDoc rather than fighting strict inference for code that's explicitly documented as disposable.
- The WebGPU-in-webview spike (ADR-0004) is **not yet re-run on Windows** — CI currently only validates `cargo build`/`cargo test` for the Rust core, not launching the full GUI app. That's a real gap if/when the `rsraw` MSVC blocker is resolved and Windows GUI validation becomes the next open question.

PR: [github.com/AlfredWei/emulsion/pull/2](https://github.com/AlfredWei/emulsion/pull/2) — open for review as of this writing, following the new GitHub-flow-with-review practice (see below).

## M1 Slice 1 — catalog schema v1 + import backend: DONE (2026-07-25)

- **Catalog schema v1** (`app/src-tauri/src/catalog.rs`): `images` gained `content_hash` (blake3, for dedupe), `file_size`, `thumbnail_path`, `stack_id` (reserved for basic stacking, unused yet). `image_versions` gained `rating` (0–5, CHECK-constrained), `flag` (`none`/`pick`/`reject`, CHECK-constrained), `color_label` (CHECK-constrained enum) — these sit per-version, not per-image, so virtual copies can be rated independently later. New methods: `find_by_hash`, `add_image_with_metadata`, `set_thumbnail_path`, `set_rating`, `set_flag`, `set_color_label`, `list_images`. 6 tests passing, including constraint-rejection tests.
- **Import backend** (new `app/src-tauri/src/import.rs`): `scan_and_import` walks a directory recursively, filters by a RAW-extension allowlist, hashes each file with `blake3`, skips already-cataloged files by hash, inserts a catalog row + empty edit stack, extracts an embedded JPEG thumbnail via `rsraw`'s cheap `unpack_thumb` path (no full demosaic) and writes it to the OS app-data thumbnails folder. **Reference-only** (stores the original path as-is) — copy-to-managed-folder is real PRD scope but deliberately deferred past Slice 1, first thing to add in a Slice 1.5 if needed sooner. 3 tests passing, including a real end-to-end import + dedupe-on-reimport test (`EMULSION_TEST_RAW_SAMPLE`-gated, same pattern as `raw_decode.rs`).
- **Tauri commands** (`app/src-tauri/src/lib.rs`): `AppState { catalog: Arc<Mutex<Catalog>> }` managed at startup, opened against a real file at `<app data dir>/catalog.sqlite` (first time the app uses a real persistent catalog, not just in-memory test instances). `import_folder` runs on a blocking thread (`spawn_blocking`) so a large import can't stall the UI. `list_images`, `set_rating`, `set_flag`, `set_color_label` round out the command surface Slice 2's Library UI will call.
- **Real end-to-end smoke test through the actual app** (not just `cargo test`): a throwaway `/m1-smoke` route (same pattern as `/m0-spike`) invoked `import_folder` against the scratchpad's two sample DNGs inside the real Tauri window, then `list_images`. Result: `imported: 1, failed: 1` (the lossless DNG imported correctly; the lossy one failed exactly as predicted by the known libjpeg gap — a nice confirmation the earlier finding is real and consistent), thumbnail confirmed on disk as a real 3960×2640 JPEG, catalog confirmed at `~/Library/Application Support/dev.alfredwei.emulsion/catalog.sqlite`. `tauri.conf.json`'s temporary window-`url` override was reverted after the run.

All 12 Rust tests passing (`cargo test --lib`).

## Older: M0 — Foundations & tech spike

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

- **This dev environment is still macOS-only** — cannot run a Windows machine directly. No longer a real blocker for getting Windows signal, though: CI (see above) now uses GitHub's own Windows runners for that, which is how the finding below got confirmed.
- **`rsraw` does not build on Windows/MSVC — now CI-confirmed, not just predicted** (`rsraw-sys`'s build script panics on `cl.exe`-like compilers; exact panic text in the CI section above). The Windows CI job is failing on purpose until this is fixed — see ADR-0003's dated finding for candidate fixes (GNU target, `libraw-rs`, build-script patch).
- **`rsraw`'s vendored LibRaw lacks libjpeg** — cannot decode lossy-compressed DNG or other libjpeg-dependent RAW variants, even on macOS. This is a live gap on the platform we *can* test, not just a Windows concern — worth fixing (patch the build script to link libjpeg, or re-evaluate `libraw-rs`) before M1 claims "broad RAW format support."
- **WebView2's WebGPU support is still unverified** — CI only builds/tests the Rust core, it doesn't launch the full GUI app anywhere yet. The macOS WebGPU spike (ADR-0004) has no Windows equivalent run yet, and can't until the `rsraw` MSVC blocker is resolved enough to produce a Windows build to test in the first place.

## Working practices (see also memory)

- **GitHub flow, as of 2026-07-25**: `main` is the stable branch. Non-trivial work happens on a `feature/*` or `docs/*` branch with logically-grouped commits, gets pushed, and opens a PR (`gh pr create`) — then **stops for review**. Only after explicit approval in chat does the PR get merged (`gh pr merge --merge`, merge-commit style, not squash/rebase), followed by `git checkout main && git pull` before starting the next branch. (Earlier M0/M1 history on `main` predates this — those were merged directly without a PR review step; everything from here on follows the PR flow.)
- No force-push, no skipped hooks, review `git status`/`git diff` before staging broad changes.
- This file should be updated at the end of each work session / whenever a task-list milestone completes, not just at the very end of a phase.
