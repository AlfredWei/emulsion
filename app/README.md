# app/

The Tauri + SvelteKit application. See the [root README](../README.md) for the project overview, quick start, and documentation map — this file only covers things specific to working inside this folder.

## Layout

- `src/` — Svelte(Kit) frontend (Library/Develop UI, in-webview WebGPU rendering per [ADR-0004](../docs/adr/ADR-0004-rendering-and-color-management.md)).
- `src-tauri/` — Rust core: `catalog.rs` (SQLite catalog, [ADR-0005](../docs/adr/ADR-0005-catalog-storage.md)), `raw_decode.rs` (LibRaw via `rsraw`, [ADR-0003](../docs/adr/ADR-0003-raw-decoding.md)), `lib.rs` (Tauri commands).

## Recommended IDE setup

[VS Code](https://code.visualstudio.com/) + [Svelte](https://marketplace.visualstudio.com/items?itemName=svelte.svelte-vscode) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer).

## Commands

Prefer the root [Makefile](../Makefile) (`make dev`, `make test`, etc.) — it wraps the commands below with the right working directory.

```bash
npm run tauri dev          # start the app
npm run tauri build        # production bundle
npm run check               # svelte/TS type-check
cd src-tauri && cargo test --lib   # Rust core tests
```

Real-file RAW decode tests are opt-in — they're skipped unless `EMULSION_TEST_RAW_SAMPLE` points at a local RAW/DNG file (not committed to the repo; see [ADR-0003](../docs/adr/ADR-0003-raw-decoding.md)'s M0 finding for why).
