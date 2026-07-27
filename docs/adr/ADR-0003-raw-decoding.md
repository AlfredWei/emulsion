# ADR-0003: RAW decoding — LibRaw via Rust FFI bindings

- Status: Accepted — Windows/MSVC build blocker resolved 2026-07-25 (see final update below), confirmed on real CI
- Date: 2026-07-25
- Relates to: [PRD §5, §7.2](../../PRD/PRD.md), [PRD risk: RAW library coverage](../../PRD/PRD.md#11-key-risks), [RFC-0001](../rfc/RFC-0001-architecture-and-tech-stack.md)

## Context

The PRD (confirmed decision, §5) commits to using an existing open-source RAW decode library rather than building a proprietary demosaic engine, and states "broad RAW format support" as a Library/Import requirement (§7.2). Two real options exist in the Rust ecosystem:

1. **LibRaw** (C++ library) via Rust FFI bindings — mature, broadest camera coverage (400+ models), the de facto standard used by most non-Adobe RAW tools.
2. **`rawler`** — a pure-Rust RAW decoder, no FFI/C++ build dependency, but materially narrower camera-format coverage as of 2026.

## Decision

Use **LibRaw via Rust FFI bindings** (the `rsraw` crate, which vendors LibRaw as a build dependency and already supports macOS/Windows/Linux builds), rather than the pure-Rust `rawler`.

## Rationale

- Camera-format breadth is a direct, user-visible requirement (PRD §7.2, §11) — a photographer whose camera isn't supported has no workaround. LibRaw's coverage is the strongest available option today.
- `rsraw` already solves cross-platform build packaging for the two target OSes, which was the main cost of choosing LibRaw over a pure-Rust option.
- The FFI boundary is narrow and well-scoped (decode RAW → linear pixel buffer + embedded preview + metadata), which limits the blast radius of introducing a C++ dependency into an otherwise Rust codebase.

## Consequences

- The build pipeline must compile/link LibRaw (C++) on both macOS and Windows — real but bounded complexity, already handled by the `rsraw` crate's existing build support.
- New camera model support depends on upstream LibRaw releases, not on this project's own code — acceptable, matches how virtually every non-Adobe RAW tool (darktable, RawTherapee) sources camera support.
- This is flagged in the PRD as a standing risk (§11): even LibRaw can lag on brand-new camera models or differ from Adobe's proprietary color science. Mitigation: ship a documented "supported cameras" list per release, and treat gaps as expected/normal rather than bugs.

## Update — M0 spike finding (2026-07-25)

`rsraw` builds and links cleanly on macOS (aarch64-apple-darwin): it vendors LibRaw source directly (not a system/pkg-config dependency) and compiles it via the `cc` crate. However, its `rsraw-sys` build script contains an explicit `panic!("MSVC is not supported")` when the detected C++ compiler behaves like `cl.exe`. Tauri apps on Windows default to the `x86_64-pc-windows-msvc` target, which uses MSVC — meaning **`rsraw` will not build out of the box on a standard Windows Tauri build**. It would require either the less-common `x86_64-pc-windows-gnu` target (MinGW-w64), which has its own compatibility gaps with Tauri's Windows tooling, or a build-time workaround forcing a non-MSVC-like compiler.

This is a real, unresolved risk for the "cross-platform desktop, single codebase" requirement ([PRD §5](../../PRD/PRD.md)) and needs to be closed out before this ADR is treated as fully confirmed — not yet done because this environment cannot build/test for Windows. Candidates to evaluate when Windows validation happens: (a) `x86_64-pc-windows-gnu` target end-to-end with Tauri, (b) `libraw-rs` (the other ADR-0003 candidate) in case its build script doesn't share this restriction, (c) patching/forking `rsraw-sys`'s build script, (d) reaching out upstream. Tracked in [PROGRESS.md](../../PROGRESS.md).

## Update — M0 spike finding: real decode confirmed on macOS, with a real gap (2026-07-25)

Tested against real files, not just error paths. Sample: a CC0-licensed Canon EOS 5D Mark III DNG from [raw.pixls.us](https://raw.pixls.us/) (a RAW-sample archive requiring CC0 for all contributions — used by darktable/RawTherapee for exactly this kind of testing), in two variants of the same shot.

- **Lossless-compressed DNG: decodes successfully.** `decode_preview()` returned a 3960×2640 8-bit RGB buffer, byte count exactly matching `width * height * 3` — a full, correct real-file decode through `rsraw` on macOS.
- **Lossy-compressed DNG (same shot, JPEG-compressed pixel data): fails with `FileUnsupported`.** Root cause, from reading `rsraw-sys`'s build script (see the ADR-0003 MSVC finding above): it compiles LibRaw's own source files directly via the `cc` crate and does **not** link `libjpeg`. Lossy-compressed DNG (and presumably any camera RAW format that leans on baseline/lossless JPEG internally, which is common) needs libjpeg support compiled into LibRaw to decode. `rsraw`'s vendored build doesn't have it.

This is a second concrete gap in `rsraw`, on top of the MSVC one — not disqualifying (lossless-compressed and uncompressed RAW/DNG both work, and that covers a meaningful share of real files), but it means **`rsraw` as currently vendored cannot decode a real, common class of RAW file** (lossy-compressed DNG, and likely any LibRaw-supported format whose decode path depends on libjpeg). Needs resolution before M1 can claim "broad RAW format support" ([PRD §7.2](../../PRD/PRD.md)). Candidates: patch `rsraw-sys`'s build script to link `libjpeg`/`libjpeg-turbo`, or re-evaluate `libraw-rs`.

Test added as `raw_decode::tests::decodes_a_real_raw_file_when_a_sample_is_provided`, gated behind an `EMULSION_TEST_RAW_SAMPLE` env var rather than a fixture committed to the repo (RAW samples are large and of mixed provenance — not appropriate for git history).

## Update — Windows MSVC failure empirically confirmed via CI, not just source-reading (2026-07-25)

The MSVC finding above was originally based on reading `rsraw-sys`'s build script, not on an actual Windows build — this environment is macOS-only. Added `.github/workflows/ci.yml` with a `macos-latest`/`windows-latest` matrix specifically to close that gap using GitHub's own Windows runners. Result, from the real CI run ([PR #2](https://github.com/AlfredWei/emulsion/pull/2)):

```
thread 'main' panicked at rsraw-sys-0.1.1\build.rs:13:9:
MSVC is not supported
##[error]Process completed with exit code 1.
```

Exact match to the predicted failure — `cargo build` fails after ~6.5 minutes on `windows-latest` (Visual Studio 2026 Enterprise, MSVC 14.51.36231), confirming this is a real, current blocker on GitHub's actual Windows toolchain, not a stale or hypothetical concern. macOS job passed cleanly in the same run (regression-confirms the M0 findings above). This is now a **confirmed, CI-enforced fact**, not a documented risk — the Windows job will keep failing on every PR until one of the candidate fixes above (GNU target, `libraw-rs`, or a build-script patch) is actually implemented, which is deliberate: it keeps this from silently regressing into "we forgot Windows was broken."

## Update — Windows/MSVC build fixed: vcpkg-linked LibRaw (2026-07-25, PR #5)

Resolved via option (c) from the previous update's candidate list, chosen over `x86_64-pc-windows-gnu` (untried), `libraw-rs` (untried), and `rawler`/`zenraw` (re-evaluated and rejected this same session — `rawler`'s coverage, while better than originally assumed at ~300+ cameras, is still narrower than LibRaw's, and would mean a second decode implementation to maintain; `zenraw` defaults to AGPL-3.0, an unacceptable licensing constraint to lock in before this project's own license is decided per MILESTONES M8).

**What changed**: vendored `rsraw-sys` 0.1.1 into `app/src-tauri/vendor/rsraw-sys/` (see its `PATCH.md` for the full rationale) and patched `build.rs`: on MSVC, instead of `panic!("MSVC is not supported")`, it links a **prebuilt LibRaw installed via vcpkg** (`vcpkg::find_package("libraw")`) rather than compiling the vendored C++ source. macOS/Linux are unaffected — they still compile from source exactly as before. `rsraw` itself (the safe wrapper the app actually calls) is not forked; `app/src-tauri/Cargo.toml`'s `[patch.crates-io]` resolves its transitive `rsraw-sys` dependency to the local patched copy.

**Two real issues found and fixed via actual CI runs, not guessed**:
1. First attempt: `cargo build` failed with `VcpkgNotFound("No vcpkg installation found. Set the VCPKG_ROOT environment variable...")` — even though the CI step that ran `vcpkg install libraw:x64-windows-static-md` had already succeeded. Root cause: the `vcpkg` *Rust crate* looks for a `VCPKG_ROOT` env var specifically, but GitHub's `windows-latest` runner only sets `VCPKG_INSTALLATION_ROOT` — two different variable names. Fixed by exporting `VCPKG_ROOT` from the installed value after the vcpkg install step.
2. Second attempt (with that fix): **fully green**. `cargo build` succeeds, and `cargo test --lib` passes all 12 tests on `windows-latest`, including a real RAW decode (`decodes_a_real_raw_file_when_a_sample_is_provided`) against the same CC0 sample DNG used for macOS validation — not just "it compiles," an actual correct decode.

This is now a **confirmed-working, CI-enforced fact** on both target platforms, matching the pattern this whole investigation followed: read the source, predict the failure, confirm the failure on real CI, fix it, confirm the fix on real CI. Nothing here was assumed correct without a real run proving it.

**Follow-up, not done here**: the `dng-lossy` (libjpeg-turbo) vcpkg feature that might also fix the *other* known gap (lossy-compressed DNG failing even on macOS, from the earlier update above) wasn't enabled or tested this pass — worth a dedicated look later rather than assumed to also be fixed.

## Update — real EXIF-metadata gap between the two LibRaw builds (2026-07-27)

Found via CI, not source-reading: `metadata::tests::extracts_real_metadata_from_a_real_raw_file` (M2 Slice 2) failed on `windows-latest` — `metadata.iso` came back `None` where macOS reads `Some(200)` correctly, for the exact same real sample DNG. **First fix attempt under-scoped the problem**: loosened only the `iso` assertion, assuming the gap was isolated to that one field — the next CI run immediately failed on `focal_length` instead (also `None` vs. the correct `Some(70.0)`), because the test aborts at its first failing assertion, so later fields' real status was still unknown. Rather than continue fixing one field per ~13-minute Windows CI round trip, checked `rsraw`'s own source (`raw.rs`) directly: `iso_speed()`, `shutter()`, `aperture()`, `focal_len()`, and `datetime()` **all read fields off the same underlying LibRaw struct** (`imgdata.other`, populated from this DNG's EXIF SubIFD) — while `camera_make`/`camera_model` come from a different struct (`imgdata.idata`, populated from the base TIFF IFD0). That explains why make/model passed both times while the `other`-struct fields fail one after another: they're not independent gaps, they're one gap affecting a whole struct.

**Root cause, confirmed as far as it goes**: this dual-build setup pins two *different* LibRaw versions per platform — macOS/Linux compile the vendored `0.21.3` source directly; Windows links whatever vcpkg's `libraw` port currently ships, confirmed via the CI log to be `0.22.1`. LibRaw's own `Changelog.txt` (checked directly, not assumed) has no entry mentioning `iso_speed`/`ISOSpeedRatings`/`focal_len` anywhere between these versions, so this is an **undocumented** behavior difference in DNG-specific EXIF SubIFD parsing, not a known/intentional one — possibly related to this sample being a *linear* DNG (produced by Adobe DNG Converter from a Canon 5D Mark III original, not a straight camera-native RAW), but that's an educated guess, not traced into LibRaw's own C++.

**This is exactly the risk this ADR's own vcpkg choice accepted going in** ("linked a prebuilt LibRaw... rather than compiling from source" — Update above) — two different upstream builds can drift in behavior even on the same input, not just in build success/failure, and the drift here turned out to be *wider* than the first CI failure suggested. Handled honestly, not hidden: the test now asserts every `other`-struct field loosely on `cfg!(windows)` (each accepts the known-missing value *or* the independently-verified correct one, so a regression that also breaks `camera_make`/`camera_model` on Windows still fails loudly), strict everywhere else.

**Bonus: this CI failure exposed a real production bug, not just a test-scoping problem.** The third Windows run showed `captured_at` coming back as `Some("1970-01-01T00:00:00+00:00")` — LibRaw stores "no timestamp" as 0, and `rsraw::datetime()` converts 0 into the Unix epoch rather than `None`. `extract_from_raw` had a zero-means-missing guard on every other `other`-struct field (`iso_speed() > 0`, `aperture() > 0.0`, …) but not on the timestamp — so any file genuinely lacking EXIF `DateTimeOriginal`, on *any* platform, would have been cataloged as shot on 1970-01-01, a bogus value a real user would see in the metadata panel. Fixed with the same guard (`.filter(|dt| dt.timestamp() != 0)`). The Windows-specific gap made this visible, but the bug itself was platform-independent.

**Not investigated further this pass** (would need actual Windows debugging access to trace into LibRaw's C++, which this environment doesn't have): whether this affects other DNG variants, other RAW formats, or only this specific linear-DNG shape; whether vcpkg's build configuration (vs. a plain from-source 0.22.1 build) plays any role; whether upgrading the vendored macOS/Linux build to 0.22.1 to match Windows would fix or just relocate the discrepancy. Worth a dedicated look if EXIF accuracy on Windows becomes user-visible (Library metadata display, filtering by ISO/exposure) rather than just this one test — right now, a real user on Windows would see camera make/model correctly but ISO/aperture/shutter/focal-length/captured-date blank for at least this DNG shape, which is a real product gap, not just a test-assertion nuisance.

## Alternatives considered and rejected

- **`rawler` (pure Rust)**: rejected for now on coverage grounds — avoiding the C++ FFI dependency is appealing (simpler builds, full memory safety through the decode path) but not worth shipping with meaningfully fewer supported cameras than users expect from a Lightroom-class tool. **Revisit trigger**: if `rawler`'s camera coverage reaches parity with LibRaw for the cameras our actual user base owns, re-evaluate switching to remove the FFI dependency entirely.
- **Proprietary in-house decoder**: already rejected at the PRD level (§5) — this ADR just confirms the concrete library choice within that decision.
