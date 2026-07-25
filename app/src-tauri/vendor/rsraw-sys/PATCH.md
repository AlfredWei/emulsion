# Vendored fork of `rsraw-sys` — why this exists

**Origin**: [`rsraw-sys` v0.1.1](https://crates.io/crates/rsraw-sys) from crates.io, upstream repo [github.com/hexilee/rsraw](https://github.com/hexilee/rsraw), MIT-licensed. Copied here (including its vendored LibRaw C++ source, ~2.2MB) on 2026-07-25, not written from scratch.

**Why vendored instead of depending on it directly**: the published crate's `build.rs` contains `panic!("MSVC is not supported")`, which fails Windows Tauri builds outright (they default to the MSVC target). See [ADR-0003](../../../../docs/adr/ADR-0003-raw-decoding.md) for the full history, including the empirical CI confirmation of this failure before this fix existed.

## What's actually changed vs. upstream

`build.rs`: on Windows/MSVC, instead of panicking, link a **prebuilt LibRaw installed via vcpkg** (`vcpkg::find_package("libraw")`) rather than compiling the vendored C++ source with the `cc` crate. macOS/Linux are untouched — they still compile the vendored source exactly as upstream does. `Cargo.toml` gained a `vcpkg` build-dependency for this.

The vendored LibRaw C++ source under `LibRaw/` is still needed on macOS/Linux (that build path is unchanged) and its header is still used for `bindgen` on all platforms — only *how the library gets linked* differs on Windows.

## How this is wired into the app

`app/src-tauri/Cargo.toml` has a `[patch.crates-io]` entry pointing `rsraw-sys` at this directory. `rsraw` (the safe wrapper crate the app actually calls) is **not** forked — it's a normal crates.io dependency; Cargo's patch mechanism resolves its transitive `rsraw-sys` dependency to this local copy automatically.

## Status

See [ADR-0003](../../../../docs/adr/ADR-0003-raw-decoding.md)'s dated update for whether this is confirmed working, and [PROGRESS.md](../../../../PROGRESS.md) for current CI status.
